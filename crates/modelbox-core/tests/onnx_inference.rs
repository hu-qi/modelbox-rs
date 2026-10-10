#![cfg(feature = "onnx")]

use std::path::PathBuf;
use std::sync::Arc;

use modelbox_core::{GraphSpec, OnnxCpuFlowUnit, Runtime, TensorF32, Value};

fn model_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/relu.onnx")
}

fn graph() -> GraphSpec {
    serde_json::from_str(include_str!("../../../examples/relu.json")).unwrap()
}

#[test]
fn onnx_relu_cpu_end_to_end() {
    let mut runtime = Runtime::default();
    runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(model_path()).unwrap()))
        .unwrap();
    let values = vec![
        Value::TensorF32(TensorF32::new(vec![3], vec![-1.0, 2.0, 3.0]).unwrap()),
        Value::TensorF32(TensorF32::new(vec![3], vec![-3.0, 4.0, -2.0]).unwrap()),
    ];
    let output = runtime.run_many(&graph(), values).unwrap();
    assert_eq!(output.input_count, 2);
    assert_eq!(
        output.outputs["result.out"],
        vec![
            Value::TensorF32(TensorF32::new(vec![3], vec![0.0, 2.0, 3.0]).unwrap()),
            Value::TensorF32(TensorF32::new(vec![3], vec![0.0, 4.0, 0.0]).unwrap())
        ]
    );
}

#[test]
fn onnx_rejects_incorrect_shape() {
    let mut runtime = Runtime::default();
    runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(model_path()).unwrap()))
        .unwrap();
    let input = Value::TensorF32(TensorF32::new(vec![2], vec![1.0, 2.0]).unwrap());
    assert!(runtime.run(&graph(), input).is_err());
}
