use std::sync::Arc;

use modelbox_core::{
    DataType, EdgeSpec, FlowError, FlowRegistry, FlowUnit, GraphSpec, NodeSpec, PortDef, Runtime,
    Value,
};

const HELLO: &str = include_str!("../../../examples/hello.json");
const FANOUT: &str = include_str!("../../../examples/fanout.json");

fn graph(json: &str) -> GraphSpec {
    serde_json::from_str(json).expect("example JSON must be valid")
}

fn values(result: &modelbox_core::RunResult, key: &str) -> Vec<Value> {
    result
        .outputs
        .get(key)
        .expect("terminal must exist")
        .clone()
}

#[test]
fn hello_runs_end_to_end() {
    let runtime = Runtime::default();
    let result = runtime
        .run(&graph(HELLO), Value::Text("edge ai".into()))
        .unwrap();
    assert_eq!(result.input_count, 1);
    assert_eq!(
        values(&result, "result.out"),
        vec![Value::Text("ModelBox Rust says: EDGE AI".into())]
    );
}

#[test]
fn fanout_fanin_preserves_order_under_backpressure() {
    let graph = graph(FANOUT);
    let runtime = Runtime::default();
    let inputs: Vec<_> = (0..250)
        .map(|number| Value::Text(number.to_string()))
        .collect();
    let result = runtime.run_many(&graph, inputs).unwrap();
    let expected: Vec<_> = (0..250)
        .map(|number| Value::Text(format!("LEFT: {number}RIGHT: {number}")))
        .collect();
    assert_eq!(values(&result, "result.out"), expected);
}

#[test]
fn fanout_with_zero_messages_is_valid() {
    let result = Runtime::default().run_many(&graph(FANOUT), vec![]).unwrap();
    assert_eq!(result.input_count, 0);
    assert!(values(&result, "result.out").is_empty());
}

#[test]
fn wrong_source_type_is_rejected_before_workers_spawn() {
    let error = Runtime::default()
        .run(&graph(HELLO), Value::Integer(42))
        .unwrap_err();
    assert!(error.to_string().contains("source frame 0"));
}

#[test]
fn missing_prefix_is_rejected_at_compile_time() {
    let mut graph = graph(HELLO);
    graph.nodes[2].config = serde_json::json!({});
    let error = Runtime::default().compile(&graph).err().unwrap();
    assert!(error.to_string().contains("config.prefix"));
}

#[test]
fn missing_port_fails_validation() {
    let mut graph = graph(HELLO);
    graph.edges[0].to = "upper.missing".into();
    let error = Runtime::default().compile(&graph).err().unwrap();
    assert!(error.to_string().contains("unknown input port"));
}

#[test]
fn double_wired_input_is_rejected() {
    let mut graph = graph(HELLO);
    graph.edges.push(EdgeSpec {
        from: "source.out".into(),
        to: "upper.in".into(),
    });
    let error = Runtime::default().compile(&graph).err().unwrap();
    assert!(error.to_string().contains("multiple upstream writers"));
}

#[test]
fn missing_required_connection_is_rejected() {
    let mut graph = graph(HELLO);
    graph.edges.remove(0);
    let error = Runtime::default().compile(&graph).err().unwrap();
    assert!(error.to_string().contains("unconnected required input"));
}

#[test]
fn duplicate_node_ids_are_rejected() {
    let mut graph = graph(HELLO);
    graph.nodes[1].id = "source".to_owned();
    let error = Runtime::default().compile(&graph).err().unwrap();
    assert!(error.to_string().contains("duplicate or invalid node id"));
}

#[test]
fn cyclic_components_are_rejected() {
    let mut graph = graph(HELLO);
    graph.nodes.push(NodeSpec {
        id: "cycle_a".into(),
        unit: "text.uppercase".into(),
        config: serde_json::Value::Null,
    });
    graph.nodes.push(NodeSpec {
        id: "cycle_b".into(),
        unit: "text.uppercase".into(),
        config: serde_json::Value::Null,
    });
    graph.edges.push(EdgeSpec {
        from: "cycle_a.out".into(),
        to: "cycle_b.in".into(),
    });
    graph.edges.push(EdgeSpec {
        from: "cycle_b.out".into(),
        to: "cycle_a.in".into(),
    });
    let error = Runtime::default().compile(&graph).err().unwrap();
    assert!(error.to_string().contains("cycle"));
}

#[test]
fn invalid_queue_capacity_is_rejected() {
    let mut graph = graph(HELLO);
    graph.queue_capacity = 0;
    let error = Runtime::default().compile(&graph).err().unwrap();
    assert!(error.to_string().contains("queue_capacity"));
}

const INPUTS: [PortDef; 1] = [PortDef::new("in", DataType::Text)];
const TEXT_OUTPUTS: [PortDef; 1] = [PortDef::new("out", DataType::Text)];
const INTEGER_OUTPUTS: [PortDef; 1] = [PortDef::new("out", DataType::Integer)];

struct IntegerUnit;
impl FlowUnit for IntegerUnit {
    fn name(&self) -> &'static str {
        "test.integer"
    }
    fn inputs(&self) -> &'static [PortDef] {
        &INPUTS
    }
    fn outputs(&self) -> &'static [PortDef] {
        &INTEGER_OUTPUTS
    }
    fn process(
        &self,
        _inputs: &[Value],
        _config: &serde_json::Value,
    ) -> modelbox_core::Result<Vec<Value>> {
        Ok(vec![Value::Integer(1)])
    }
}

struct FailingUnit;
impl FlowUnit for FailingUnit {
    fn name(&self) -> &'static str {
        "test.fail"
    }
    fn inputs(&self) -> &'static [PortDef] {
        &INPUTS
    }
    fn outputs(&self) -> &'static [PortDef] {
        &TEXT_OUTPUTS
    }
    fn process(
        &self,
        _inputs: &[Value],
        _config: &serde_json::Value,
    ) -> modelbox_core::Result<Vec<Value>> {
        Err(FlowError::Execution("deliberate failure".to_owned()))
    }
}

struct WrongOutputUnit;
impl FlowUnit for WrongOutputUnit {
    fn name(&self) -> &'static str {
        "test.wrong_output"
    }
    fn inputs(&self) -> &'static [PortDef] {
        &INPUTS
    }
    fn outputs(&self) -> &'static [PortDef] {
        &TEXT_OUTPUTS
    }
    fn process(
        &self,
        _inputs: &[Value],
        _config: &serde_json::Value,
    ) -> modelbox_core::Result<Vec<Value>> {
        Ok(vec![Value::Integer(3)])
    }
}

fn replace_upper(graph: &mut GraphSpec, unit: &str) {
    graph.nodes[1].unit = unit.to_owned();
}

#[test]
fn custom_unit_edges_are_type_checked() {
    let mut registry = FlowRegistry::default();
    registry.register(Arc::new(IntegerUnit)).unwrap();
    let mut graph = graph(HELLO);
    replace_upper(&mut graph, "test.integer");
    let runtime = Runtime::new(registry);
    let error = runtime.compile(&graph).err().unwrap();
    assert!(error.to_string().contains("type mismatch"));
}

#[test]
fn cancellation_propagates_when_unit_fails() {
    let mut registry = FlowRegistry::default();
    registry.register(Arc::new(FailingUnit)).unwrap();
    let mut graph = graph(HELLO);
    graph.queue_capacity = 1;
    replace_upper(&mut graph, "test.fail");
    let inputs = (0..500).map(|_| Value::Text("x".into())).collect();
    let error = Runtime::new(registry).run_many(&graph, inputs).unwrap_err();
    assert!(error.to_string().contains("deliberate failure"));
}

#[test]
fn runtime_rejects_incorrect_output_type() {
    let mut registry = FlowRegistry::default();
    registry.register(Arc::new(WrongOutputUnit)).unwrap();
    let mut graph = graph(HELLO);
    replace_upper(&mut graph, "test.wrong_output");
    let error = Runtime::new(registry)
        .run(&graph, Value::Text("x".into()))
        .unwrap_err();
    assert!(error.to_string().contains("emitted"));
}

#[test]
fn duplicate_flowunit_registration_is_rejected() {
    let mut registry = FlowRegistry::default();
    assert!(registry.register(Arc::new(IntegerUnit)).is_ok());
    assert!(registry.register(Arc::new(IntegerUnit)).is_err());
}

#[test]
fn output_is_stable_across_multiple_runs() {
    let runtime = Runtime::default();
    let graph = graph(FANOUT);
    let plan = runtime.compile(&graph).unwrap();
    assert_eq!(plan.node_count(), 5);
    assert_eq!(plan.edge_count(), 5);
    for _ in 0..10 {
        let result = runtime
            .run_plan(&plan, vec![Value::Text("reusable".into())])
            .unwrap();
        assert_eq!(
            values(&result, "result.out"),
            vec![Value::Text("LEFT: reusableRIGHT: reusable".into())]
        );
    }
}

#[test]
fn profiled_run_preserves_outputs_and_counts_non_source_units() {
    let spec = graph(HELLO);
    let runtime = Runtime::default();
    let baseline = runtime.run(&spec, Value::Text("hello".into())).unwrap();
    let profiled = runtime
        .run_many_profiled(&spec, vec![Value::Text("hello".into())])
        .unwrap();
    assert_eq!(profiled.result.outputs, baseline.outputs);
    assert_eq!(profiled.result.input_count, 1);
    assert!(!profiled.node_process.contains_key("source"));
    for node in ["upper", "prefix", "result"] {
        assert_eq!(profiled.node_process[node].calls, 1);
    }
}
