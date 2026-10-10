# Trained MNIST CNN model attribution

- Binary: `../../examples/mnist.onnx`, 26,454 bytes
- Upstream project: [Microsoft ONNX Runtime Inference Examples](https://github.com/microsoft/onnxruntime-inference-examples)
- Source path: [c_cxx/MNIST/mnist.onnx](https://github.com/microsoft/onnxruntime-inference-examples/blob/main/c_cxx/MNIST/mnist.onnx)
- Upstream Git blob SHA: `fc1a3f733c6e6243dd23dacb125b7a372de55a50`
- License: MIT (full text in [LICENSE](LICENSE)).
- Input: `float32[1,1,28,28]` grayscale values in [0,1].
- Output: `float32[1,10]` raw logits; post-processing softmax is not part of the model.
- The `mnist-1-input.json` fixture is a programmatically drawn synthetic digit-like pattern. No independent labeled accuracy test is claimed.

This model comes from a third-party reference implementation. It is not authored by the modelbox-rs team. The tests validate image tensor shape and executable CPU ONNX operators, compare graph and direct execution, and reject invalid shapes; accuracy benchmarking will require labeled datasets and a separate independent reference.
