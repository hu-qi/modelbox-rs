//! Release-oriented sequential file-to-detections YOLOv8n CPU stage benchmark.
//!
//! Measures a single-image request repeatedly, not camera FPS or a streaming
//! pipeline. Model load and graph compilation occur BEFORE any warmup or timing.

use std::sync::Arc;
use std::time::Instant;

use modelbox_core::{FlowError, GraphSpec, OnnxCpuFlowUnit, Runtime, Value};
use serde_json::{json, Value as JsonValue};

use crate::{yolo_infer, yolo_post, ONNX_GRAPH};

const MAX_ITERATIONS: usize = 200;
const MAX_WARMUP: usize = 100;

fn invalid(message: impl std::fmt::Display) -> FlowError {
    FlowError::Execution(format!("YOLO stage benchmark: {message}"))
}

fn count(
    input: Option<&String>,
    default: usize,
    min: usize,
    max: usize,
) -> modelbox_core::Result<usize> {
    let value = match input {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| invalid(format!("count must be {min}..={max}")))?,
        None => default,
    };
    if !(min..=max).contains(&value) {
        return Err(invalid(format!("count must be {min}..={max}")));
    }
    Ok(value)
}

fn percentile(sorted: &[u128], numerator: usize) -> u128 {
    let index = (sorted.len() * numerator).div_ceil(100) - 1;
    sorted[index]
}

fn statistics(values: &[u128]) -> JsonValue {
    assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let to_ms = |nanos: u128| nanos as f64 / 1_000_000.0;
    json!({
        "min": to_ms(sorted[0]),
        "p50": to_ms(percentile(&sorted, 50)),
        "p95": to_ms(percentile(&sorted, 95)),
        "max": to_ms(sorted[sorted.len() - 1]),
        "mean": values.iter().map(|&x| x as f64).sum::<f64>() / values.len() as f64 / 1_000_000.0,
    })
}

pub(crate) fn run(args: Vec<String>) -> modelbox_core::Result<()> {
    if !(2..=4).contains(&args.len()) {
        return Err(invalid(crate::usage()));
    }
    let iterations = count(args.get(2), 20, 1, MAX_ITERATIONS)?;
    let warmup = count(args.get(3), 5, 0, MAX_WARMUP)?;
    let model_path = &args[0];
    let image_path = &args[1];

    let graph: GraphSpec = serde_json::from_str(ONNX_GRAPH)?;
    let mut runtime = Runtime::default();
    runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(model_path)?))?;
    let plan = runtime.compile(&graph)?;

    let mut decode_ns = Vec::with_capacity(iterations);
    let mut preprocess_ns = Vec::with_capacity(iterations);
    let mut graph_ns = Vec::with_capacity(iterations);
    let mut postprocess_ns = Vec::with_capacity(iterations);
    let mut total_ns = Vec::with_capacity(iterations);
    let mut last_count = 0_usize;
    let mut last_fingerprint = 0.0_f64;
    let mut maximum_stage_reconciliation_delta_ns = 0_u128;

    for index in 0..(warmup + iterations) {
        let start = Instant::now();
        let rgb = image::open(image_path).map_err(invalid)?.to_rgb8();
        let after_decode = Instant::now();

        let (input, geometry) = yolo_infer::letterbox(&rgb)?;
        let after_preprocess = Instant::now();

        let result = runtime.run_plan(&plan, vec![Value::TensorF32(input)])?;
        let after_graph = Instant::now();
        let tensor = match result.outputs.get("result.out").map(Vec::as_slice) {
            Some([Value::TensorF32(output)]) => output,
            _ => return Err(invalid("expected exactly one f32 Graph output")),
        };
        if tensor.shape() != [1, 84, 8400] {
            return Err(invalid(format!(
                "expected YOLOv8n output [1,84,8400], got {:?}",
                tensor.shape()
            )));
        }

        let candidates = yolo_post::decode(tensor, 640, 640, 0.25, 0.45, 300)?;
        last_count = 0;
        last_fingerprint = 0.0;
        for candidate in candidates {
            let coords = yolo_infer::project_xyxy(candidate.xyxy, geometry);
            if coords[2] > coords[0] && coords[3] > coords[1] {
                last_count += 1;
                last_fingerprint += f64::from(candidate.confidence);
                last_fingerprint += coords.iter().map(|&v| f64::from(v)).sum::<f64>();
                last_fingerprint += candidate.class_id as f64;
            }
        }
        let after_postprocess = Instant::now();

        if index >= warmup {
            let stage = [
                (after_decode - start).as_nanos(),
                (after_preprocess - after_decode).as_nanos(),
                (after_graph - after_preprocess).as_nanos(),
                (after_postprocess - after_graph).as_nanos(),
            ];
            let total = (after_postprocess - start).as_nanos();
            let sum = stage.iter().sum::<u128>();
            maximum_stage_reconciliation_delta_ns =
                maximum_stage_reconciliation_delta_ns.max(total.abs_diff(sum));
            decode_ns.push(stage[0]);
            preprocess_ns.push(stage[1]);
            graph_ns.push(stage[2]);
            postprocess_ns.push(stage[3]);
            total_ns.push(total);
        }
    }
    let mean_total_ms =
        total_ns.iter().map(|&ns| ns as f64).sum::<f64>() / iterations as f64 / 1_000_000.0;
    let mean_stage_sum_ms = [&decode_ns, &preprocess_ns, &graph_ns, &postprocess_ns]
        .iter()
    .map(|stage| stage.iter().map(|&ns| ns as f64).sum::<f64>())
    .sum::<f64>()
        / iterations as f64
        / 1_000_000.0;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "benchmark": "yolov8n_sequential_file_to_detection_cpu_stages",
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "model_path": model_path,
            "image_path": image_path,
            "model_shape": [1, 3, 640, 640],
            "output_shape": [1, 84, 8400],
            "warmup_iterations": warmup,
            "measured_iterations": iterations,
            "thresholds": {"confidence": 0.25, "iou": 0.45, "max_detections": 300},
            "latency_ms": {
                "decode_file_and_rgb": statistics(&decode_ns),
                "letterbox_and_chw": statistics(&preprocess_ns),
                "graph_including_onnx_cpu": statistics(&graph_ns),
                "nms_and_project_boxes": statistics(&postprocess_ns),
                "end_to_end": statistics(&total_ns)
            },
            "mean_stage_sum_ms": mean_stage_sum_ms,
            "max_stage_total_reconciliation_delta_ns": maximum_stage_reconciliation_delta_ns,
            "estimated_sequential_images_per_second_from_mean": if mean_total_ms > 0.0 {
                1000.0 / mean_total_ms
            } else {
                0.0
            },
            "last_detection_count": last_count,
            "last_detection_numeric_fingerprint": last_fingerprint,
            "onnx_session_and_graph_compile_excluded": true,
            "file_read_and_decode_included": true,
            "worker_spawn_and_graph_transport_included": true,
            "warning": "Sequential one-image requests with OS file cache. Stage p50/p95 are separately computed and are not additive. This is not a live-stream throughput, GPU/NPU, or independent detection accuracy benchmark."
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{count, percentile, statistics};

    #[test]
    fn validates_warmup_and_iteration_bounds() {
        assert_eq!(count(None, 20, 1, 200).unwrap(), 20);
        assert_eq!(count(Some(&"0".into()), 0, 0, 100).unwrap(), 0);
        assert!(count(Some(&"0".into()), 20, 1, 200).is_err());
        assert!(count(Some(&"201".into()), 20, 1, 200).is_err());
        assert!(count(Some(&"101".into()), 5, 0, 100).is_err());
        assert!(count(Some(&"x".into()), 5, 0, 100).is_err());
    }

    #[test]
    fn calculates_nearest_rank_and_stage_mean() {
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 50), 3);
        assert_eq!(percentile(&[1, 2, 3, 4, 5], 95), 5);
        let result = statistics(&[1_000_000, 2_000_000, 3_000_000]);
        assert_eq!(result["mean"], 2.0);
        assert_eq!(result["p50"], 2.0);
        assert_eq!(result["p95"], 3.0);
    }

    #[test]
    fn captures_single_iteration_percentiles() {
        let result = statistics(&[1_234_567]);
        assert_eq!(result["min"], result["p95"]);
        assert_eq!(result["mean"], 1.234567);
    }
}
