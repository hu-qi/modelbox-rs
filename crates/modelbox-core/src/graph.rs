use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{FlowError, FlowRegistry, FlowUnit, Result};

fn default_capacity() -> usize {
    16
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphSpec {
    pub name: String,
    #[serde(default = "default_capacity")]
    pub queue_capacity: usize,
    pub nodes: Vec<NodeSpec>,
    pub edges: Vec<EdgeSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeSpec {
    pub id: String,
    pub unit: String,
    #[serde(default)]
    pub config: JsonValue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdgeSpec {
    pub from: String,
    pub to: String,
}

pub(crate) struct PlannedNode {
    pub id: String,
    pub unit: Arc<dyn FlowUnit>,
    pub config: JsonValue,
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedEdge {
    pub from_node: usize,
    pub from_port: usize,
    pub to_node: usize,
    pub to_port: usize,
}

/// A validated DAG with type-checked and uniquely wired input ports.
pub struct ExecutionPlan {
    pub(crate) nodes: Vec<PlannedNode>,
    pub(crate) edges: Vec<PlannedEdge>,
    pub(crate) source: usize,
    pub(crate) queue_capacity: usize,
    pub(crate) terminal_ports: Vec<(usize, usize)>,
}

fn endpoint(endpoint: &str) -> Result<(&str, &str)> {
    endpoint
        .rsplit_once('.')
        .filter(|(node, port)| !node.is_empty() && !port.is_empty())
        .ok_or_else(|| FlowError::InvalidGraph(format!("endpoint must be node.port: {endpoint}")))
}

impl ExecutionPlan {
    pub fn compile(spec: &GraphSpec, registry: &FlowRegistry) -> Result<Self> {
        if spec.name.trim().is_empty() {
            return Err(FlowError::InvalidGraph("graph name is empty".to_owned()));
        }
        if !(1..=65_536).contains(&spec.queue_capacity) {
            return Err(FlowError::InvalidGraph(
                "queue_capacity must be 1..=65536".to_owned(),
            ));
        }
        if spec.nodes.is_empty() || spec.nodes.len() > 1024 || spec.edges.len() > 16_384 {
            return Err(FlowError::InvalidGraph(
                "graph must have 1..=1024 nodes and <=16384 edges".to_owned(),
            ));
        }
        let mut ids = HashMap::new();
        let mut nodes = Vec::with_capacity(spec.nodes.len());
        let mut sources = Vec::new();

        for (index, node) in spec.nodes.iter().enumerate() {
            if node.id.is_empty()
                || !node
                    .id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                || ids.insert(node.id.clone(), index).is_some()
            {
                return Err(FlowError::InvalidGraph(format!(
                    "duplicate or invalid node id: {}",
                    node.id
                )));
            }
            let unit = registry.get(&node.unit)?;
            unit.validate_config(&node.config)?;
            if unit.is_source() {
                if !unit.inputs().is_empty() || unit.outputs().len() != 1 {
                    return Err(FlowError::InvalidGraph(format!(
                        "source {} must have zero inputs and one output",
                        node.id
                    )));
                }
                sources.push(index);
            } else if unit.inputs().is_empty() {
                return Err(FlowError::InvalidGraph(format!(
                    "non-source {} has no inputs",
                    node.id
                )));
            }
            let mut seen = HashSet::new();
            for port in unit.inputs().iter().chain(unit.outputs()) {
                if port.name.is_empty() || !seen.insert(port.name) {
                    return Err(FlowError::InvalidGraph(format!(
                        "duplicate or empty port on node {}",
                        node.id
                    )));
                }
            }
            nodes.push(PlannedNode {
                id: node.id.clone(),
                unit,
                config: node.config.clone(),
            });
        }
        if sources.len() != 1 {
            return Err(FlowError::InvalidGraph(format!(
                "v0.1 requires exactly one source, found {}",
                sources.len()
            )));
        }

        let mut edges = Vec::with_capacity(spec.edges.len());
        let mut wired_inputs = HashSet::new();
        let mut wired_outputs = HashSet::new();
        let mut indegree = vec![0_usize; nodes.len()];
        let mut adjacency = vec![Vec::new(); nodes.len()];

        for edge in &spec.edges {
            let (from_name, from_port_name) = endpoint(&edge.from)?;
            let (to_name, to_port_name) = endpoint(&edge.to)?;
            let from_node = *ids.get(from_name).ok_or_else(|| {
                FlowError::InvalidGraph(format!("unknown source node: {from_name}"))
            })?;
            let to_node = *ids.get(to_name).ok_or_else(|| {
                FlowError::InvalidGraph(format!("unknown destination node: {to_name}"))
            })?;
            let from_port = nodes[from_node]
                .unit
                .outputs()
                .iter()
                .position(|port| port.name == from_port_name)
                .ok_or_else(|| {
                    FlowError::InvalidGraph(format!("unknown output port: {}", edge.from))
                })?;
            let to_port = nodes[to_node]
                .unit
                .inputs()
                .iter()
                .position(|port| port.name == to_port_name)
                .ok_or_else(|| {
                    FlowError::InvalidGraph(format!("unknown input port: {}", edge.to))
                })?;
            if !wired_inputs.insert((to_node, to_port)) {
                return Err(FlowError::InvalidGraph(format!(
                    "input port has multiple upstream writers: {}",
                    edge.to
                )));
            }
            let actual = nodes[from_node].unit.outputs()[from_port].data_type;
            let expected = nodes[to_node].unit.inputs()[to_port].data_type;
            if actual != expected {
                return Err(FlowError::InvalidGraph(format!(
                    "type mismatch {} ({actual:?}) -> {} ({expected:?})",
                    edge.from, edge.to
                )));
            }
            wired_outputs.insert((from_node, from_port));
            adjacency[from_node].push(to_node);
            indegree[to_node] += 1;
            edges.push(PlannedEdge {
                from_node,
                from_port,
                to_node,
                to_port,
            });
        }

        for (node_index, node) in nodes.iter().enumerate() {
            for input_index in 0..node.unit.inputs().len() {
                if !wired_inputs.contains(&(node_index, input_index)) {
                    return Err(FlowError::InvalidGraph(format!(
                        "unconnected required input: {}.{}",
                        node.id,
                        node.unit.inputs()[input_index].name
                    )));
                }
            }
        }

        let mut queue: VecDeque<usize> = indegree
            .iter()
            .enumerate()
            .filter_map(|(index, degree)| (*degree == 0).then_some(index))
            .collect();
        let mut visited = 0;
        while let Some(current) = queue.pop_front() {
            visited += 1;
            for &next in &adjacency[current] {
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    queue.push_back(next);
                }
            }
        }
        if visited != nodes.len() {
            return Err(FlowError::InvalidGraph("graph contains a cycle".to_owned()));
        }
        let mut terminal_ports = Vec::new();
        for (node_index, node) in nodes.iter().enumerate() {
            for port in 0..node.unit.outputs().len() {
                if !wired_outputs.contains(&(node_index, port)) {
                    terminal_ports.push((node_index, port));
                }
            }
        }
        if spec.edges.is_empty() || terminal_ports.is_empty() {
            return Err(FlowError::InvalidGraph(
                "graph requires edges and at least one terminal output".to_owned(),
            ));
        }

        Ok(Self {
            nodes,
            edges,
            source: sources[0],
            queue_capacity: spec.queue_capacity,
            terminal_ports,
        })
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn terminal_names(&self) -> Vec<String> {
        self.terminal_ports
            .iter()
            .map(|&(node, port)| {
                format!(
                    "{}.{}",
                    self.nodes[node].id,
                    self.nodes[node].unit.outputs()[port].name
                )
            })
            .collect()
    }
}
