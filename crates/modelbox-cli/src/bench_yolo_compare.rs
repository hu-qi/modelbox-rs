//! Alternating direct FlowUnit vs Graph calls for the actual YOLOv8n model.
//!
//! Both paths share ONE ONNX CPU session and the same preprocessed TensorF32.
//! This isolates a descriptive Graph-vs-direct gap but not pure scheduler cost.

use std::sync::Arc;
use std::time::Instant;

use modelbox_core::{
    ExecutionPlan, FlowError, FlowUnit, GraphSpec, OnnxCpuFlowUnit, Runtime, TensorF32, Value,
};
use serde_json::{json, Value as JsonValue};

use crate::{yolo_infer, yolo_post, ONNX_GRAPH};

const MAX_ROUNDS: usize = 101;
const MAX_WARMUP: usize = 100;

fn invalid(msg: impl std::fmt::Display) -> FlowError {
    FlowError::Execution(format!("YOLO direct-vs-Graph comparison: {msg}"))
}

fn count(
    raw: Option<&String>,
    default: usize,
    min: usize,
    max: usize,
) -> modelbox_core::Result<usize> {
    let value = match raw {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| invalid(format!("count must be {min}..={max}")))?,
        None => default,
    };
    if !(min..=max).contains(&value) {
        return Err(invalid(format!("count must be {min}..={max}")));
    }
    Ok(value)
}

fn one_tensor(values: Vec<Value>) -> modelbox_core::Result<TensorF32> {
    let mut output = values.into_iter();
    match (output.next(), output.next()) {
        (Some(Value::TensorF32(tensor)), None) if tensor.shape() == [1, 84, 8400] => Ok(tensor),
        _ => Err(invalid("expected exactly one f32[1,84,8400] output")),
    }
}

fn direct(unit: &OnnxCpuFlowUnit, input: &Value) -> modelbox_core::Result<(u128, TensorF32)> {
    let start = Instant::now();
    let result = unit.process(std::slice::from_ref(input), &JsonValue::Null);
    let ns = start.elapsed().as_nanos();
    Ok((ns, one_tensor(result?)?))
}

fn graph(
    runtime: &Runtime,
    plan: &ExecutionPlan,
    input: Value,
) -> modelbox_core::Result<(u128, TensorF32)> {
    let start = Instant::now();
    let result = runtime.run_plan(plan, vec![input]);
    let ns = start.elapsed().as_nanos();
    let mut output = result?.outputs;
    Ok((
        ns,
        one_tensor(
            output
                .remove("result.out")
                .ok_or_else(|| invalid("missing result.out"))?,
        )?,
    ))
}

fn statistics(values: &[u128]) -> JsonValue {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let ms = |ns: u128| ns as f64 / 1_000_000.0;
    let rank = |percent: usize| sorted[(sorted.len() * percent).div_ceil(100) - 1];
    json!({
        "min": ms(sorted[0]),
        "mean": values.iter().map(|&v| v as f64).sum::<f64>() / values.len() as f64 / 1_000_000.0,
        "p50": ms(rank(50)),
        "p95": ms(rank(95)),
        "max": ms(*sorted.last().expect("nonempty observations")),
    })
}

fn numerical_difference(a: &TensorF32, b: &TensorF32) -> modelbox_core::Result<f32> {
    if a.shape() != b.shape() {
        return Err(invalid("Graph/direct output shapes differ"));
    }
    let max_error = a
        .data()
        .iter()
        .zip(b.data())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0_f32, f32::max);
    if max_error > 1e-4 {
        return Err(invalid(format!(
            "Graph vs direct outputs differ by {max_error}, allowed 0.0001"
        )));
    }
    Ok(max_error)
}

pub(crate) fn run(args: Vec<String>) -> modelbox_core::Result<()> {
    if !(2..=4).contains(&args.len()) {
        return Err(invalid(crate::usage()));
    }
    let rounds = count(args.get(2), 15, 3, MAX_ROUNDS)?;
    let warmup = count(args.get(3), 5, 0, MAX_WARMUP)?;
    let model_path = &args[0];
    let image_path = &args[1];

    // Deliberately exclude JPEG decoding and tensor creation from paired timings.
    let rgb = image::open(image_path).map_err(invalid)?.to_rgb8();
    let (tensor, geometry) = yolo_infer::letterbox(&rgb)?;
    let input = Value::TensorF32(tensor);

    let unit = Arc::new(OnnxCpuFlowUnit::load(model_path)?);
    let mut runtime = Runtime::default();
    runtime.registry_mut().register(unit.clone())?;
    let spec: GraphSpec = serde_json::from_str(ONNX_GRAPH)?;
    let plan = runtime.compile(&spec)?;

    // Alternating order guards against systematic first-path hot-cache bias.
    for n in 0..warmup {
        if n % 2 == 0 {
            graph(&runtime, &plan, input.clone())?;
            direct(unit.as_ref(), &input)?;
        } else {
            direct(unit.as_ref(), &input)?;
            graph(&runtime, &plan, input.clone())?;
        }
    }

    let mut graph_ns = Vec::with_capacity(rounds);
    let mut direct_ns = Vec::with_capacity(rounds);
    let mut paired_delta_ns = Vec::with_capacity(rounds);
    let mut max_output_difference = 0.0_f32;
    let mut last_count = 0;
    let mut fingerprint = 0.0_f64;

    for n in 0..rounds {
        let (graph_time, graph_tensor, direct_time, direct_tensor) = if n % 2 == 0 {
            let (g_ns, g_tensor) = graph(&runtime, &plan, input.clone())?;
            let (d_ns, d_tensor) = direct(unit.as_ref(), &input)?;
            (g_ns, g_tensor, d_ns, d_tensor)
        } else {
            let (d_ns, d_tensor) = direct(unit.as_ref(), &input)?;
            let (g_ns, g_tensor) = graph(&runtime, &plan, input.clone())?;
            (g_ns, g_tensor, d_ns, d_tensor)
        };
        max_output_difference =
            max_output_difference.max(numerical_difference(&graph_tensor, &direct_tensor)?);
        graph_ns.push(graph_time);
        direct_ns.push(direct_time);
        paired_delta_ns.push(graph_time as i128 - direct_time as i128);

        // Numerical guard is outside BOTH timing windows; do not hide
        // accidental zero-work or output disappearance.
        let detections = yolo_post::decode(&graph_tensor, 640, 640, 0.25, 0.45, 300)?;
        last_count = 0;
        fingerprint = 0.0;
        for d in detections {
            let coords = yolo_infer::project_xyxy(d.xyxy, geometry);
            if coords[2] > coords[0] && coords[3] > coords[1] {
                last_count += 1;
                fingerprint += f64::from(d.confidence)
                    + d.class_id as f64
                    + coords.iter().map(|&coord| f64::from(coord)).sum::<f64>();
            }
        }
    }
    let mean = |series: &[u128]| {
        series.iter().map(|&x| x as f64).sum::<f64>() / rounds as f64 / 1_000_000.0
    };
    let graph_mean = mean(&graph_ns);
    let direct_mean = mean(&direct_ns);
    let deltas: Vec<_> = paired_delta_ns
        .iter()
        .map(|&delta| delta as f64 / 1_000_000.0)
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "benchmark": "yolov8n_paired_graph_vs_direct_same_onnx_session",
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "model_path": model_path,
            "image_path": image_path,
            "input_shape": [1,3,640,640],
            "output_shape": [1,84,8400],
            "warmup_pairs": warmup,
            "measured_pairs": rounds,
            "alternating_starting_path": true,
            "graph_ms": statistics(&graph_ns),
            "direct_flowunit_ms": statistics(&direct_ns),
            "descriptive_gap": {
                "graph_minus_direct_mean_ms": graph_mean - direct_mean,
                "graph_to_direct_mean_ratio": graph_mean / direct_mean,
                "paired_graph_minus_direct_ms": deltas,
                "warning": "NOT pure Graph scheduler cost. Direct FlowUnit and Graph differ in worker creation, input ownership, channels/capture and output representation. Both call the same ORT backend and session."
            },
            "max_output_element_abs_diff": max_output_difference,
            "last_detection_count": last_count,
            "last_detection_numeric_fingerprint": fingerprint,
            "session_and_graph_compile_excluded": true,
            "image_decode_preprocess_nms_outside_timed_windows": true,
            "warning": "Single image repeated; no streaming/camera FPS. Pair order alternates and uses one shared ONNX CPU session; thermal/power state still matters."
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{count, numerical_difference, statistics};
    use modelbox_core::TensorF32;

    #[test]
    fn rejects_invalid_rounds_and_warmups() {
        assert_eq!(count(None, 15, 3, 101).unwrap(), 15);
        assert_eq!(count(Some(&"0".into()), 5, 0, 100).unwrap(), 0);
        assert!(count(Some(&"2".into()), 15, 3, 101).is_err());
        assert!(count(Some(&"102".into()), 15, 3, 101).is_err());
    }

    #[test]
    fn computes_nearest_rank_percentiles() {
        let r = statistics(&[1_000_000, 5_000_000, 3_000_000]);
        assert_eq!(r["mean"], 3.0);
        assert_eq!(r["p50"], 3.0);
        assert_eq!(r["p95"], 5.0);
    }

    #[test]
    fn checks_numerical_equality_and_drift() {
        let a = TensorF32::new(vec![1, 1], vec![0.5]).unwrap();
        let b = TensorF32::new(vec![1, 1], vec![0.50001]).unwrap();
        let c = TensorF32::new(vec![1, 1], vec![0.55]).unwrap();
        assert!(numerical_difference(&a, &b).is_ok());
        assert!(numerical_difference(&a, &c).is_err());
    }
}
