//! Standard MNIST test-set IDX image/label reader (no CSV conversion or Python).
//! The file headers use big-endian 32-bit integers (IDX image magic 2051,
//! label magic 2049). Input values are uint8 divided by 255.0.

use std::fs;

use modelbox_core::{FlowError, TensorF32, Value};

use crate::mnist_eval::{self, Sample};

const MAX_SAMPLES: usize = 10_000;
const IMAGE_PIXELS: usize = 28 * 28;

fn invalid(message: impl std::fmt::Display) -> FlowError {
    FlowError::Execution(format!("MNIST IDX: {message}"))
}

fn field(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated IDX header"),
    )
}

fn parse_idx(images: &[u8], labels: &[u8]) -> modelbox_core::Result<Vec<Sample>> {
    if images.len() < 16 || labels.len() < 8 {
        return Err(invalid(
            "images require a 16-byte header and labels an 8-byte header",
        ));
    }
    if field(images, 0) != 2051 {
        return Err(invalid("invalid image magic (expected 2051)"));
    }
    if field(labels, 0) != 2049 {
        return Err(invalid("invalid label magic (expected 2049)"));
    }
    let count = field(images, 4) as usize;
    if count == 0 || count > MAX_SAMPLES {
        return Err(invalid(
            "number of test images must be between 1 and 10,000",
        ));
    }
    if count != field(labels, 4) as usize {
        return Err(invalid("image/label counts disagree"));
    }
    if field(images, 8) != 28 || field(images, 12) != 28 {
        return Err(invalid("image dimensions must be 28x28"));
    }
    let expected_images_len = 16 + count * IMAGE_PIXELS;
    let expected_labels_len = 8 + count;
    if images.len() != expected_images_len || labels.len() != expected_labels_len {
        return Err(invalid(format!(
            "IDX file lengths disagree with headers: images {} vs {}, labels {} vs {}",
            images.len(),
            expected_images_len,
            labels.len(),
            expected_labels_len
        )));
    }

    images[16..]
        .chunks_exact(IMAGE_PIXELS)
        .zip(&labels[8..])
        .enumerate()
        .map(|(index, (pixels, &digit))| {
            if digit > 9 {
                return Err(invalid(format!("label {index} is outside 0..=9")));
            }
            let f32_pixels = pixels.iter().map(|&v| f32::from(v) / 255.0).collect();
            Ok(Sample {
                label: digit as usize,
                image: Value::TensorF32(TensorF32::new(vec![1, 1, 28, 28], f32_pixels)?),
            })
        })
        .collect()
}

pub(crate) fn run(args: Vec<String>) -> modelbox_core::Result<()> {
    if !(3..=4).contains(&args.len()) {
        return Err(invalid(crate::usage()));
    }
    let images = fs::read(&args[1])?;
    let labels = fs::read(&args[2])?;
    let samples = parse_idx(&images, &labels)?;
    let minimum = mnist_eval::parse_minimum(args.get(3), samples.len())?;
    mnist_eval::evaluate(
        &args[0],
        &format!("{} + {}", args[1], args[2]),
        samples,
        minimum,
    )
}

#[cfg(test)]
mod tests {
    use super::parse_idx;
    use modelbox_core::Value;

    fn example_idx() -> (Vec<u8>, Vec<u8>) {
        let mut images = vec![0, 0, 8, 3];
        images.extend_from_slice(&2_u32.to_be_bytes());
        images.extend_from_slice(&28_u32.to_be_bytes());
        images.extend_from_slice(&28_u32.to_be_bytes());
        images.resize(16 + 2 * 784, 0);
        images[16] = 255;
        images[16 + 784 + 783] = 127;

        let mut labels = vec![0, 0, 8, 1];
        labels.extend_from_slice(&2_u32.to_be_bytes());
        labels.extend_from_slice(&[7, 2]);
        (images, labels)
    }

    #[test]
    fn reads_big_endian_headers_and_pixels() {
        let (images, labels) = example_idx();
        let samples = parse_idx(&images, &labels).unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!((samples[0].label, samples[1].label), (7, 2));
        match &samples[0].image {
            Value::TensorF32(tensor) => {
                assert_eq!(tensor.shape(), &[1, 1, 28, 28]);
                assert_eq!(tensor.data()[0], 1.0);
            }
            _ => panic!("expected f32 tensor"),
        }
        match &samples[1].image {
            Value::TensorF32(tensor) => {
                assert_eq!(tensor.data()[783], 127.0 / 255.0);
            }
            _ => panic!("expected f32 tensor"),
        }
    }

    #[test]
    fn rejects_wrong_magic_count_dimensions_and_file_lengths() {
        let (images, labels) = example_idx();
        let mut wrong = images.clone();
        wrong[3] = 0;
        assert!(parse_idx(&wrong, &labels).is_err());

        let mut wrong_labels = labels.clone();
        wrong_labels[3] = 3;
        assert!(parse_idx(&images, &wrong_labels).is_err());

        let mut wrong = images.clone();
        wrong[7] = 3;
        assert!(parse_idx(&wrong, &labels).is_err());

        let mut wrong = images.clone();
        wrong[11] = 27;
        assert!(parse_idx(&wrong, &labels).is_err());

        let mut wrong = images.clone();
        wrong.pop();
        assert!(parse_idx(&wrong, &labels).is_err());

        let mut wrong = images.clone();
        wrong.push(0);
        assert!(parse_idx(&wrong, &labels).is_err());

        let mut wrong = labels.clone();
        wrong.pop();
        assert!(parse_idx(&images, &wrong).is_err());

        let mut wrong = labels.clone();
        wrong[8] = 10;
        assert!(parse_idx(&images, &wrong).is_err());

        assert!(parse_idx(&[], &labels).is_err());
        assert!(parse_idx(&images, &[]).is_err());
    }
}
