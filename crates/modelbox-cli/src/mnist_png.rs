//! Explicit MNIST image adapter: 28x28 eight-bit grayscale PNG, values in [0,1].
use std::fs::File;
use std::io::{BufRead, BufReader, Seek};
use std::path::Path;
use std::sync::Arc;

use modelbox_core::{FlowError, GraphSpec, OnnxCpuFlowUnit, Runtime, TensorF32, Value};
use serde_json::json;

fn png_error(error: impl std::fmt::Display) -> FlowError {
    FlowError::Execution(format!("MNIST PNG: {error}"))
}

fn decode_mnist_png<R: BufRead + Seek>(input: R) -> modelbox_core::Result<TensorF32> {
    let mut reader = png::Decoder::new(input).read_info().map_err(png_error)?;
    if reader.info().width != 28 || reader.info().height != 28 {
        return Err(png_error("image must be exactly 28x28 pixels"));
    }
    if reader.info().animation_control.is_some() {
        return Err(png_error("animated PNG is not supported"));
    }
    if reader.output_color_type() != (png::ColorType::Grayscale, png::BitDepth::Eight) {
        return Err(png_error(
            "requires an 8-bit grayscale PNG (not RGB or palette)",
        ));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| png_error("decoded image size overflow"))?;
    if size != 784 {
        return Err(png_error("unexpected decoded grayscale image size"));
    }
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(png_error)?;
    if frame.width != 28 || frame.height != 28 || frame.buffer_size() != 784 {
        return Err(png_error("unexpected PNG frame dimensions"));
    }
    let values = buffer
        .into_iter()
        .map(|pixel| f32::from(pixel) / 255.0)
        .collect();
    TensorF32::new(vec![1, 1, 28, 28], values)
}

pub(crate) fn load_mnist_png(path: impl AsRef<Path>) -> modelbox_core::Result<TensorF32> {
    decode_mnist_png(BufReader::new(File::open(path)?))
}

pub(crate) fn run(args: Vec<String>) -> modelbox_core::Result<()> {
    if args.len() != 2 {
        return Err(FlowError::Execution(crate::usage().to_owned()));
    }
    let model = &args[0];
    let image = &args[1];
    let tensor = load_mnist_png(image)?;
    let mut runtime = Runtime::default();
    runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(model)?))?;
    let graph: GraphSpec = serde_json::from_str(crate::ONNX_GRAPH)?;
    let result = runtime.run(&graph, Value::TensorF32(tensor))?;
    let captured = result
        .outputs
        .get("result.out")
        .ok_or_else(|| FlowError::Execution("MNIST graph did not capture result.out".to_owned()))?;
    let scores = match captured.as_slice() {
        [Value::TensorF32(tensor)] if tensor.shape() == [1, 10] => tensor.data(),
        _ => return Err(png_error("expected one [1,10] float32 logit tensor")),
    };
    let (digit, _) = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .ok_or_else(|| png_error("empty logits"))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "predicted_digit": digit,
            "preprocessing": "28x28 8-bit grayscale PNG / 255.0; no resize or inversion",
            "input_shape": [1, 1, 28, 28],
            "model_path": model,
            "image_path": image,
            "result": result
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use modelbox_core::TensorF32;

    use super::decode_mnist_png;

    #[test]
    fn checked_in_png_matches_checked_in_tensor() {
        let decoded =
            decode_mnist_png(Cursor::new(include_bytes!("../../../examples/mnist-1.png"))).unwrap();
        let expected: TensorF32 =
            serde_json::from_str(include_str!("../../../examples/mnist-1-input.json")).unwrap();
        assert_eq!(decoded, expected);
    }

    fn encoded_png(width: u32, height: u32, color: png::ColorType, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width, height);
            encoder.set_color(color);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(pixels).unwrap();
        }
        bytes
    }

    #[test]
    fn rejects_wrong_size_and_color() {
        let small = encoded_png(2, 2, png::ColorType::Grayscale, &[0, 0, 0, 0]);
        assert!(decode_mnist_png(Cursor::new(small))
            .unwrap_err()
            .to_string()
            .contains("exactly 28x28"));

        let rgb = encoded_png(28, 28, png::ColorType::Rgb, &vec![0; 28 * 28 * 3]);
        assert!(decode_mnist_png(Cursor::new(rgb))
            .unwrap_err()
            .to_string()
            .contains("grayscale"));
    }
}
