//! Evaluate pre-trained MNIST CPU inference against externally labeled test rows.
//! The CSV is label,u8_pixel_0,...,u8_pixel_783 with no header.

use std::fs;
use std::sync::Arc;

use modelbox_core::{FlowError, GraphSpec, OnnxCpuFlowUnit, Runtime, TensorF32, Value};
use serde_json::json;

const MAX_SAMPLES: usize = 10_000;
const CHUNK_SIZE: usize = 64;

#[derive(Clone)]
struct Sample {
    label: usize,
    image: Value,
}

fn invalid(message: impl std::fmt::Display) -> FlowError {
    FlowError::Execution(format!("MNIST evaluation: {message}"))
}

fn parse_csv(contents: &str) -> modelbox_core::Result<Vec<Sample>> {
    let mut samples = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        if samples.len() == MAX_SAMPLES {
            return Err(invalid("CSV exceeds 10,000 labeled examples"));
        }
        let columns: Vec<_> = line.trim_end_matches('\r').split(',').collect();
        if columns.len() != 785 {
            return Err(invalid(format!(
                "row {} has {} columns, expected 785 (label + 784 pixels)",
                index + 1,
                columns.len()
            )));
        }
        let label = columns[0]
            .parse::<usize>()
            .map_err(|_| invalid(format!("row {} has invalid label", index + 1)))?;
        if label >= 10 {
            return Err(invalid(format!("row {} label is not in 0..=9", index + 1)));
        }
        let data = columns[1..]
            .iter()
            .enumerate()
            .map(|(pixel, value)| {
                value
                    .parse::<u8>()
                    .map(|intensity| f32::from(intensity) / 255.0)
                    .map_err(|_| {
                        invalid(format!(
                            "row {} pixel {} is not an integer from 0 to 255",
                            index + 1,
                            pixel
                        ))
                    })
            })
            .collect::<modelbox_core::Result<Vec<_>>>()?;
        samples.push(Sample {
            label,
            image: Value::TensorF32(TensorF32::new(vec![1, 1, 28, 28], data)?),
        });
    }
    if samples.is_empty() {
        return Err(invalid("CSV must contain at least one labeled row"));
    }
    Ok(samples)
}

fn classify(value: &Value) -> modelbox_core::Result<usize> {
    let output = match value {
        Value::TensorF32(output) if output.shape() == [1, 10] => output,
        _ => return Err(invalid("ONNX model must emit a [1,10] float32 tensor")),
    };
    output
        .data()
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, _)| index)
        .ok_or_else(|| invalid("model output is empty"))
}

pub(crate) fn run(args: Vec<String>) -> modelbox_core::Result<()> {
    if !(2..=3).contains(&args.len()) {
        return Err(invalid(crate::usage()));
    }
    let samples = parse_csv(&fs::read_to_string(&args[1])?)?;
    let minimum = match args.get(2) {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| invalid("min_correct must be a nonnegative integer"))?,
        None => 0,
    };
    if minimum > samples.len() {
        return Err(invalid("min_correct cannot exceed sample count"));
    }
    let mut runtime = Runtime::default();
    runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(&args[0])?))?;
    let graph: GraphSpec = serde_json::from_str(crate::ONNX_GRAPH)?;
    let plan = runtime.compile(&graph)?;

    let mut confusion = [[0_usize; 10]; 10];
    let mut predictions = Vec::with_capacity(samples.len());
    let mut wrong = Vec::new();
    let mut correct = 0usize;
    for (chunk_id, batch) in samples.chunks(CHUNK_SIZE).enumerate() {
        let frames = batch.iter().map(|sample| sample.image.clone()).collect();
        let output = runtime.run_plan(&plan, frames)?;
        let results = output
            .outputs
            .get("result.out")
            .ok_or_else(|| invalid("missing result.out"))?;
        if results.len() != batch.len() {
            return Err(invalid("inference output count does not match input count"));
        }
        for (offset, (sample, result)) in batch.iter().zip(results).enumerate() {
            let predicted = classify(result)?;
            confusion[sample.label][predicted] += 1;
            let matched = predicted == sample.label;
            if matched {
                correct += 1;
            } else {
                wrong.push(json!({
                    "row_index": chunk_id * CHUNK_SIZE + offset,
                    "expected": sample.label,
                    "predicted": predicted
                }));
            }
            predictions.push(predicted);
        }
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "dataset": &args[1],
            "model": &args[0],
            "sample_count": samples.len(),
            "correct": correct,
            "accuracy": correct as f64 / samples.len() as f64,
            "min_correct": minimum,
            "predicted_labels": predictions,
            "misclassified": wrong,
            "confusion_matrix_rows_expected_columns_predicted": confusion,
            "preprocessing": "u8 grayscale / 255.0 into float32[1,1,28,28]",
            "warning": "10-sample fixture is a functional accuracy smoke test, not a statistically representative MNIST evaluation"
        }))?
    );
    if correct < minimum {
        return Err(invalid(format!(
            "{correct} predictions correct, fewer than required {minimum}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_csv;

    #[test]
    fn external_fixture_has_valid_labels_and_shapes() {
        let records = parse_csv(include_str!(
            "../../../benchmarks/fixtures/mnist-test-10.csv"
        ))
        .unwrap();
        assert_eq!(records.len(), 10);
        assert_eq!(
            records
                .iter()
                .map(|sample| sample.label)
                .collect::<Vec<_>>(),
            [7, 2, 1, 0, 4, 1, 4, 9, 5, 9]
        );
        for record in records {
            match record.image {
                modelbox_core::Value::TensorF32(tensor) => {
                    assert_eq!(tensor.shape(), [1, 1, 28, 28]);
                    assert!(tensor
                        .data()
                        .iter()
                        .all(|value| (0.0..=1.0).contains(value)));
                }
                _ => panic!("expected a f32 image tensor"),
            }
        }
    }

    #[test]
    fn rejects_malformed_csv() {
        assert!(parse_csv("").is_err());
        assert!(parse_csv("7,0,1").is_err());

        let valid = format!("7,{}", vec!["0"; 784].join(","));
        let invalid_label = valid.replacen("7,", "10,", 1);
        assert!(parse_csv(&invalid_label).is_err());

        let invalid_pixel = format!("7,300,{}", vec!["0"; 783].join(","));
        assert!(parse_csv(&invalid_pixel).is_err());
    }
}
