use modelbox_core::{DataType, FlowRegistry, GraphSpec, Runtime, TensorF32, Value};

const GRAPH: &str = r#"{
  "name": "tensor_identity",
  "nodes": [
    {"id": "source", "unit": "tensor.input"},
    {"id": "result", "unit": "tensor.output"}
  ],
  "edges": [{"from": "source.out", "to": "result.in"}]
}"#;

#[test]
fn shape_and_payload() {
    let tensor = TensorF32::new(vec![2, 2], vec![1.0, -2.0, 3.0, 4.0]).unwrap();
    assert_eq!(tensor.shape(), &[2, 2]);
    assert_eq!(tensor.data()[1], -2.0);
    assert!(TensorF32::new(vec![2, 3], vec![1.0; 5]).is_err());
    assert!(TensorF32::new(vec![0], vec![]).is_err());
    assert!(TensorF32::new(vec![usize::MAX, 2], vec![]).is_err());
    assert!(TensorF32::new(vec![1], vec![f32::NAN]).is_err());
}

#[test]
fn serde_cannot_bypass_validation() {
    let invalid = r#"{"type":"tensor_f32","value":{"shape":[2],"data":[1.0]}}"#;
    assert!(serde_json::from_str::<Value>(invalid).is_err());
    let val = Value::TensorF32(TensorF32::new(vec![2], vec![1.0, 2.0]).unwrap());
    assert_eq!(serde_json::from_str::<Value>(&serde_json::to_string(&val).unwrap()).unwrap(), val);
}

#[test]
fn tensor_pipeline() {
    let graph: GraphSpec = serde_json::from_str(GRAPH).unwrap();
    let runtime = Runtime::default();
    let tensor = TensorF32::new(vec![3], vec![-1.0, 2.0, 3.0]).unwrap();
    let result = runtime.run(&graph, Value::TensorF32(tensor.clone())).unwrap();
    assert_eq!(result.outputs["result.out"], vec![Value::TensorF32(tensor)]);
    assert_eq!(runtime.compile(&graph).unwrap().node_count(), 2);
    assert_eq!(FlowRegistry::default().get("tensor.output").unwrap().inputs()[0].data_type, DataType::TensorF32);
}

#[test]
fn port_mismatch_rejected() {
    let mut graph: GraphSpec = serde_json::from_str(GRAPH).unwrap();
    graph.nodes[0].unit = "core.input".to_owned();
    assert!(Runtime::default().compile(&graph).err().unwrap().to_string().contains("type mismatch"));
}
