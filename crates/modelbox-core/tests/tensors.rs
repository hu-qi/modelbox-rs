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
    assert_eq!(
        serde_json::from_str::<Value>(&serde_json::to_string(&val).unwrap()).unwrap(),
        val
    );
}

#[test]
fn tensor_pipeline() {
    let graph: GraphSpec = serde_json::from_str(GRAPH).unwrap();
    let runtime = Runtime::default();
    let tensor = TensorF32::new(vec![3], vec![-1.0, 2.0, 3.0]).unwrap();
    let result = runtime
        .run(&graph, Value::TensorF32(tensor.clone()))
        .unwrap();
    assert_eq!(result.outputs["result.out"], vec![Value::TensorF32(tensor)]);
    assert_eq!(runtime.compile(&graph).unwrap().node_count(), 2);
    assert_eq!(
        FlowRegistry::default()
            .get("tensor.output")
            .unwrap()
            .inputs()[0]
            .data_type,
        DataType::TensorF32
    );
}

#[test]
fn port_mismatch_rejected() {
    let mut graph: GraphSpec = serde_json::from_str(GRAPH).unwrap();
    graph.nodes[0].unit = "core.input".to_owned();
    assert!(Runtime::default()
        .compile(&graph)
        .err()
        .unwrap()
        .to_string()
        .contains("type mismatch"));
}

#[test]
fn tensor_fanout_with_backpressure_keeps_each_sequence() {
    let graph: GraphSpec = serde_json::from_str(
        r#"{
          "name": "tensor_fanout_backpressure",
          "queue_capacity": 1,
          "nodes": [
            {"id": "source", "unit": "tensor.input"},
            {"id": "left", "unit": "tensor.output"},
            {"id": "right", "unit": "tensor.output"}
          ],
          "edges": [
            {"from": "source.out", "to": "left.in"},
            {"from": "source.out", "to": "right.in"}
          ]
        }"#,
    )
    .unwrap();

    let values: Vec<Value> = (0..256)
        .map(|index| {
            Value::TensorF32(TensorF32::new(vec![3], vec![index as f32, -1.0, 3.0]).unwrap())
        })
        .collect();
    let result = Runtime::default().run_many(&graph, values.clone()).unwrap();
    assert_eq!(result.input_count, 256);
    assert_eq!(result.outputs["left.out"], values);
    assert_eq!(result.outputs["right.out"], values);
}

#[test]
fn tensor_output_moves_buffer_without_copying() {
    let graph: GraphSpec = serde_json::from_str(GRAPH).unwrap();
    let tensor = TensorF32::new(vec![4], vec![1.0, 2.0, -3.0, 4.0]).unwrap();
    let original_buffer = tensor.data().as_ptr();
    let result = Runtime::default()
        .run(&graph, Value::TensorF32(tensor))
        .unwrap();
    let captured = &result.outputs["result.out"];
    match captured.as_slice() {
        [Value::TensorF32(output)] => {
            assert_eq!(output.data(), &[1.0, 2.0, -3.0, 4.0]);
            assert_eq!(original_buffer, output.data().as_ptr());
        }
        _ => panic!("tensor.output must produce exactly one f32 tensor"),
    }
}

#[test]
fn tensor_output_owned_path_checks_types() {
    let unit = FlowRegistry::default().get("tensor.output").unwrap();
    let error = unit
        .process_owned(vec![Value::Text("wrong type".to_owned())], &serde_json::Value::Null)
        .unwrap_err();
    assert!(error.to_string().contains("tensor_f32"));
}
