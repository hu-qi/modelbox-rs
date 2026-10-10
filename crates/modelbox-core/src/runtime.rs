use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, SendTimeoutError, Sender};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{ExecutionPlan, FlowError, FlowRegistry, FlowUnit, GraphSpec, Result, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunResult {
    /// Unconnected outputs are exposed as node_id.port_name.
    /// Every vector preserves source sequence order.
    pub outputs: BTreeMap<String, Vec<Value>>,
    pub input_count: usize,
}

#[derive(Default)]
pub struct Runtime {
    registry: FlowRegistry,
}

#[derive(Clone)]
struct Frame {
    sequence: usize,
    value: Value,
}

type Captures = Arc<Mutex<BTreeMap<String, Vec<Frame>>>>;
type FirstError = Arc<Mutex<Option<FlowError>>>;

fn remember_error(errors: &FirstError, cancelled: &AtomicBool, error: FlowError) {
    let mut guard = errors.lock().expect("error mutex poisoned");
    if guard.is_none() {
        *guard = Some(error);
    }
    cancelled.store(true, Ordering::Release);
}

fn receive(receiver: &Receiver<Frame>, cancelled: &AtomicBool) -> Option<Frame> {
    loop {
        if cancelled.load(Ordering::Acquire) {
            return None;
        }
        match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(frame) => return Some(frame),
            Err(RecvTimeoutError::Disconnected) => return None,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Bounded transport keeps ownership of a timed-out frame for retries.
/// Cancellation and unexpected downstream disconnect retain their old semantics.
fn send_frame(
    node_id: &str,
    port_name: &str,
    sender: &Sender<Frame>,
    mut pending: Frame,
    cancelled: &AtomicBool,
) -> Result<()> {
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        match sender.send_timeout(pending, Duration::from_millis(20)) {
            Ok(()) => return Ok(()),
            Err(SendTimeoutError::Timeout(frame)) => pending = frame,
            Err(SendTimeoutError::Disconnected(_)) => {
                if cancelled.load(Ordering::Acquire) {
                    return Ok(());
                }
                return Err(FlowError::Execution(format!(
                    "downstream of {node_id}.{port_name} disconnected unexpectedly"
                )));
            }
        }
    }
}

fn dispatch(
    node_id: &str,
    unit: &dyn FlowUnit,
    sequence: usize,
    values: Vec<Value>,
    senders: &[Vec<Sender<Frame>>],
    captures: &Captures,
    cancelled: &AtomicBool,
) -> Result<()> {
    if values.len() != unit.outputs().len() {
        return Err(FlowError::Execution(format!(
            "node {node_id} returned {} outputs, expected {}",
            values.len(),
            unit.outputs().len()
        )));
    }
    for (port_index, value) in values.into_iter().enumerate() {
        let port = unit.outputs()[port_index];
        if value.data_type() != port.data_type {
            return Err(FlowError::Execution(format!(
                "node {node_id}.{} emitted {:?}, expected {:?}",
                port.name,
                value.data_type(),
                port.data_type
            )));
        }

        // Single-reader edges and terminal capture can take ownership without
        // allocating a new Tensor/Vec. Fan-out clones only for extra readers.
        if let Some((last, fanout)) = senders[port_index].split_last() {
            for sender in fanout {
                send_frame(
                    node_id,
                    port.name,
                    sender,
                    Frame {
                        sequence,
                        value: value.clone(),
                    },
                    cancelled,
                )?;
            }
            send_frame(
                node_id,
                port.name,
                last,
                Frame { sequence, value },
                cancelled,
            )?;
        } else {
            let key = format!("{node_id}.{}", port.name);
            captures
                .lock()
                .expect("capture mutex poisoned")
                .entry(key)
                .or_default()
                .push(Frame { sequence, value });
        }
    }
    Ok(())
}

struct Worker {
    node_id: String,
    unit: Arc<dyn FlowUnit>,
    config: JsonValue,
    inputs: Vec<Receiver<Frame>>,
    outputs: Vec<Vec<Sender<Frame>>>,
    source_values: Option<Vec<Value>>,
    captures: Captures,
    cancelled: Arc<AtomicBool>,
}

impl Worker {
    fn execute(self) -> Result<()> {
        if let Some(values) = self.source_values {
            for (sequence, value) in values.into_iter().enumerate() {
                if self.cancelled.load(Ordering::Acquire) {
                    break;
                }
                dispatch(
                    &self.node_id,
                    self.unit.as_ref(),
                    sequence,
                    vec![value],
                    &self.outputs,
                    &self.captures,
                    &self.cancelled,
                )?;
            }
            return Ok(());
        }

        loop {
            if self.cancelled.load(Ordering::Acquire) {
                break;
            }
            let first = match receive(&self.inputs[0], &self.cancelled) {
                Some(frame) => frame,
                None => break,
            };
            let mut values = vec![first.value];
            for receiver in self.inputs.iter().skip(1) {
                match receive(receiver, &self.cancelled) {
                    Some(frame) if frame.sequence == first.sequence => values.push(frame.value),
                    Some(frame) => {
                        return Err(FlowError::Execution(format!(
                            "node {} got out-of-sync frames: {} vs {}",
                            self.node_id, first.sequence, frame.sequence
                        )));
                    }
                    None if self.cancelled.load(Ordering::Acquire) => return Ok(()),
                    None => {
                        return Err(FlowError::Execution(format!(
                            "node {} lost a required input stream",
                            self.node_id
                        )));
                    }
                }
            }
            let output = self.unit.process(&values, &self.config)?;
            dispatch(
                &self.node_id,
                self.unit.as_ref(),
                first.sequence,
                output,
                &self.outputs,
                &self.captures,
                &self.cancelled,
            )?;
        }
        Ok(())
    }
}

impl Runtime {
    pub fn new(registry: FlowRegistry) -> Self {
        Self { registry }
    }

    pub fn registry_mut(&mut self) -> &mut FlowRegistry {
        &mut self.registry
    }

    pub fn compile(&self, spec: &GraphSpec) -> Result<ExecutionPlan> {
        ExecutionPlan::compile(spec, &self.registry)
    }

    pub fn run(&self, spec: &GraphSpec, value: Value) -> Result<RunResult> {
        self.run_many(spec, vec![value])
    }

    /// A bounded worker/channel pipeline for finite source batches.
    ///
    /// v0.1 supports one source, one item per source sequence, and synchronized
    /// DAG fan-out/fan-in. It does not yet implement dynamic stream joins.
    pub fn run_many(&self, spec: &GraphSpec, values: Vec<Value>) -> Result<RunResult> {
        let plan = self.compile(spec)?;
        self.run_plan(&plan, values)
    }

    pub fn run_plan(&self, plan: &ExecutionPlan, values: Vec<Value>) -> Result<RunResult> {
        let expected = plan.nodes[plan.source].unit.outputs()[0].data_type;
        for (index, value) in values.iter().enumerate() {
            if value.data_type() != expected {
                return Err(FlowError::Execution(format!(
                    "source frame {index} is {:?}, expected {expected:?}",
                    value.data_type()
                )));
            }
        }
        let count = values.len();
        let cancelled = Arc::new(AtomicBool::new(false));
        let errors: FirstError = Arc::new(Mutex::new(None));
        let captures: Captures = Arc::new(Mutex::new(BTreeMap::new()));

        {
            let mut entries = captures.lock().expect("capture mutex poisoned");
            for name in plan.terminal_names() {
                entries.insert(name, Vec::new());
            }
        }

        let mut inbound: Vec<Vec<Option<Receiver<Frame>>>> = plan
            .nodes
            .iter()
            .map(|node| vec![None; node.unit.inputs().len()])
            .collect();
        let mut outbound: Vec<Vec<Vec<Sender<Frame>>>> = plan
            .nodes
            .iter()
            .map(|node| vec![Vec::new(); node.unit.outputs().len()])
            .collect();

        for edge in &plan.edges {
            let (sender, receiver) = bounded(plan.queue_capacity);
            inbound[edge.to_node][edge.to_port] = Some(receiver);
            outbound[edge.from_node][edge.from_port].push(sender);
        }

        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(plan.nodes.len());
            let mut source_values = Some(values);
            for (index, node) in plan.nodes.iter().enumerate() {
                let inputs: Vec<_> = inbound[index]
                    .iter_mut()
                    .map(|receiver| receiver.take().expect("validated input is connected"))
                    .collect();
                let worker = Worker {
                    node_id: node.id.clone(),
                    unit: Arc::clone(&node.unit),
                    config: node.config.clone(),
                    inputs,
                    outputs: std::mem::take(&mut outbound[index]),
                    source_values: if index == plan.source {
                        source_values.take()
                    } else {
                        None
                    },
                    captures: Arc::clone(&captures),
                    cancelled: Arc::clone(&cancelled),
                };
                let worker_errors = Arc::clone(&errors);
                let worker_cancelled = Arc::clone(&cancelled);
                handles.push(scope.spawn(move || {
                    if let Err(error) = worker.execute() {
                        remember_error(&worker_errors, &worker_cancelled, error);
                    }
                }));
            }
            for handle in handles {
                if handle.join().is_err() {
                    remember_error(
                        &errors,
                        &cancelled,
                        FlowError::Execution("worker thread panicked".to_owned()),
                    );
                }
            }
        });

        if let Some(error) = errors.lock().expect("error mutex poisoned").take() {
            return Err(error);
        }
        let mut outputs = BTreeMap::new();
        let mut entries = captures.lock().expect("capture mutex poisoned");
        for (name, frames) in entries.iter_mut() {
            frames.sort_by_key(|frame| frame.sequence);
            outputs.insert(
                name.clone(),
                frames.drain(..).map(|frame| frame.value).collect(),
            );
        }
        Ok(RunResult {
            outputs,
            input_count: count,
        })
    }
}
