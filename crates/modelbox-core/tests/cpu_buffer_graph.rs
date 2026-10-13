use std::collections::BTreeMap;

use modelbox_core::{
    BufferMetaValue, CpuBuffer, DataType, FlowRegistry, GraphSpec, Runtime, Value,
};

const GRAPH: &str = include_str!("../../../examples/cpu_buffer_fanout.json");
const INPUT: &str = include_str!("../../../examples/cpu_buffer_value.json");

fn graph() -> GraphSpec {
    serde_json::from_str(GRAPH).unwrap()
}

fn input() -> Value {
    serde_json::from_str(INPUT).unwrap()
}

#[test]
fn typed_cpu_buffer_graph_preserves_binary_and_metadata_through_both_branches() {
    let value = input();
    let buffer = match &value {
        Value::CpuBuffer(buffer) => buffer,
        _ => panic!("expected CPU buffer"),
    };
    assert_eq!(buffer.data(), &[0, 1, 127, 128, 255]);
    assert_eq!(buffer.metadata()["width"], BufferMetaValue::Integer(5));
    let outputs = Runtime::default().run(&graph(), value.clone()).unwrap();
    assert_eq!(outputs.input_count, 1);
    assert_eq!(outputs.outputs["left.out"], vec![value.clone()]);
    assert_eq!(outputs.outputs["right.out"], vec![value]);
    let left = match &outputs.outputs["left.out"][0] {
        Value::CpuBuffer(buffer) => buffer,
        _ => panic!("expected CPU buffer"),
    };
    let right = match &outputs.outputs["right.out"][0] {
        Value::CpuBuffer(buffer) => buffer,
        _ => panic!("expected CPU buffer"),
    };
    assert_eq!(left.data(), right.data());
    assert_eq!(left.metadata(), right.metadata());
    assert!(!left.shares_storage_with(right));
}

#[test]
fn identity_shares_storage_but_deep_copy_detaches_it() {
    let registry = FlowRegistry::default();
    let original = match input() {
        Value::CpuBuffer(buffer) => buffer,
        _ => panic!("expected CPU buffer"),
    };
    let passthrough = registry.get("buffer.identity").unwrap();
    let copied = registry.get("buffer.deep_copy").unwrap();
    let output = passthrough
        .process_owned(
            vec![Value::CpuBuffer(original.clone())],
            &serde_json::Value::Null,
        )
        .unwrap();
    let identity = match &output[0] {
        Value::CpuBuffer(buffer) => buffer,
        _ => panic!("expected CPU buffer"),
    };
    assert!(original.shares_storage_with(identity));
    let output = copied
        .process_owned(
            vec![Value::CpuBuffer(original.clone())],
            &serde_json::Value::Null,
        )
        .unwrap();
    let detached = match &output[0] {
        Value::CpuBuffer(buffer) => buffer,
        _ => panic!("expected CPU buffer"),
    };
    assert_eq!(&original, detached);
    assert!(!original.shares_storage_with(detached));
}

#[test]
fn buffer_many_frames_bounded_fanout_preserves_sequence_and_contents() {
    let mut inputs = Vec::new();
    for index in 0..256 {
        let metadata = BTreeMap::from([
            ("frame".to_owned(), BufferMetaValue::Integer(index)),
            ("label".to_owned(), BufferMetaValue::Text("cpu".to_owned())),
        ]);
        let buffer = CpuBuffer::new(vec![0, index as u8, 255], metadata).unwrap();
        inputs.push(Value::CpuBuffer(buffer));
    }
    let output = Runtime::default()
        .run_many(&graph(), inputs.clone())
        .unwrap();
    assert_eq!(output.input_count, 256);
    assert_eq!(output.outputs["left.out"], inputs);
    assert_eq!(output.outputs["right.out"], inputs);
}

#[test]
fn incompatible_ports_and_invalid_config_fail_at_compile_time() {
    let mut spec = graph();
    spec.nodes[1].unit = "text.uppercase".to_owned();
    let error = Runtime::default().compile(&spec).err().unwrap();
    assert!(error.to_string().contains("type mismatch"));
    let mut spec = graph();
    spec.nodes[1].config = serde_json::json!({"unexpected": 1});
    let error = Runtime::default().compile(&spec).err().unwrap();
    assert!(error.to_string().contains("do not accept configuration"));
}

#[test]
fn source_rejects_old_untyped_bytes_before_workers_spawn() {
    let error = Runtime::default()
        .run(&graph(), Value::Bytes(vec![0, 1, 2]))
        .unwrap_err();
    assert!(error.to_string().contains("source frame 0"));
    assert_eq!(input().data_type(), DataType::CpuBuffer);
}
