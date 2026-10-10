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

// Real pre-trained MNIST CNN (MIT licensed upstream), not the synthetic Relu
// fixture. The input is a 28x28 grayscale tensor, not an accuracy dataset.
#[test]
fn trained_mnist_cnn_graph_matches_direct_flowunit() {
    use modelbox_core::FlowUnit;

    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mnist.onnx");
    let input: TensorF32 =
        serde_json::from_str(include_str!("../../../examples/mnist-1-input.json")).unwrap();
    assert_eq!(input.shape(), &[1, 1, 28, 28]);
    let unit = Arc::new(OnnxCpuFlowUnit::load(model).unwrap());

    let direct = unit
        .process(&[Value::TensorF32(input.clone())], &serde_json::Value::Null)
        .unwrap();
    let mut runtime = Runtime::default();
    runtime.registry_mut().register(unit.clone()).unwrap();
    let graph_output = runtime.run(&graph(), Value::TensorF32(input)).unwrap();
    let graph_result = &graph_output.outputs["result.out"];
    assert_eq!(graph_result.len(), 1);
    let (expected, actual) = match (direct.as_slice(), graph_result.as_slice()) {
        ([Value::TensorF32(expected)], [Value::TensorF32(actual)]) => (expected, actual),
        _ => panic!("MNIST must emit a single float32 output tensor"),
    };
    assert_eq!(expected.shape(), &[1, 10]);
    assert_eq!(actual.shape(), &[1, 10]);
    for (&reference, &observed) in expected.data().iter().zip(actual.data()) {
        assert!(reference.is_finite() && observed.is_finite());
        assert!(
            (reference - observed).abs() <= 0.0001 * reference.abs().max(1.0),
            "ONNX direct {reference} differs from graph {observed}"
        );
    }
    assert!(expected.data().iter().any(|score| *score != 0.0));
}

#[test]
fn trained_mnist_cnn_rejects_invalid_image_shape() {
    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mnist.onnx");
    let mut runtime = Runtime::default();
    runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(model).unwrap()))
        .unwrap();
    // Correct element count but wrong spatial rank/dimensions.
    let wrong = TensorF32::new(vec![784], vec![0.0; 784]).unwrap();
    assert!(runtime.run(&graph(), Value::TensorF32(wrong)).is_err());
}

#[test]
fn trained_mnist_cnn_matches_observed_mac_logit_baseline() {
    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mnist.onnx");
    let input: TensorF32 =
        serde_json::from_str(include_str!("../../../examples/mnist-1-input.json")).unwrap();
    let mac_observed: Vec<f32> =
        serde_json::from_str(include_str!("../../../examples/mnist-1-observed-logits.json"))
            .unwrap();

    let mut runtime = Runtime::default();
    runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(model).unwrap()))
        .unwrap();
    let result = runtime.run(&graph(), Value::TensorF32(input)).unwrap();
    let actual = match result.outputs["result.out"].as_slice() {
        [Value::TensorF32(tensor)] => tensor.data(),
        _ => panic!("MNIST graph must capture one tensor"),
    };
    assert_eq!(mac_observed.len(), 10);
    assert_eq!(actual.len(), 10);
    for (&expected, &observed) in mac_observed.iter().zip(actual.iter()) {
        let tolerance = 0.02_f32.max(expected.abs() * 0.002);
        assert!(
            (expected - observed).abs() <= tolerance,
            "observed golden score {expected} vs inference {observed}"
        );
    }
    let predicted = actual
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, _)| index);
    assert_eq!(predicted, Some(1));
}
