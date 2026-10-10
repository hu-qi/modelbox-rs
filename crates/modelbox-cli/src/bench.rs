//! End-to-end CPU graph-call benchmark for a single f32 ONNX inference FlowUnit.
//! The model session and graph plan are built once, before measured iterations.

use std::fs;
use std::sync::Arc;
use std::time::Instant;

use modelbox_core::{
    DataType, FlowError, FlowUnit, GraphSpec, OnnxCpuFlowUnit, PortDef, Runtime, TensorF32, Value,
};
use serde_json::json;

use crate::{usage, ONNX_GRAPH};

const MAX_ITERATIONS: usize = 10_000;
const MAX_WARMUP: usize = 1_000;

fn parse_count(
    argument: Option<&String>,
    default: usize,
    max: usize,
    name: &str,
) -> Result<usize, FlowError> {
    let parsed = match argument {
        Some(raw) => raw.parse::<usize>().map_err(|_| {
            FlowError::Execution(format!("{name} must be an integer between 1 and {max}"))
        })?,
        None => default,
    };
    if parsed == 0 || parsed > max {
        return Err(FlowError::Execution(format!(
            "{name} must be between 1 and {max}"
        )));
    }
    Ok(parsed)
}

fn nearest_rank(sorted: &[u128], numerator: usize, denominator: usize) -> u128 {
    let index = (sorted.len() * numerator).div_ceil(denominator) - 1;
    sorted[index]
}

/// Executes N independent graph calls; each call creates workers and joins them.
/// This is not a microbenchmark of the ONNX session's kernel alone.
pub(crate) fn run(args: Vec<String>) -> Result<(), FlowError> {
    if args.len() < 2 || args.len() > 4 {
        return Err(FlowError::Execution(usage().to_owned()));
    }
    let iterations = parse_count(args.get(2), 100, MAX_ITERATIONS, "iterations")?;
    let warmup = match args.get(3) {
        Some(raw) => raw.parse::<usize>().map_err(|_| {
            FlowError::Execution("warmup must be an integer between 0 and 1000".to_owned())
        })?,
        None => 10,
    };
    if warmup > MAX_WARMUP {
        return Err(FlowError::Execution(
            "warmup exceeds maximum 1000".to_owned(),
        ));
    }

    let input: TensorF32 = serde_json::from_str(&fs::read_to_string(&args[1])?)?;
    let graph: GraphSpec = serde_json::from_str(ONNX_GRAPH)?;
    let mut runtime = Runtime::default();
    runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(&args[0])?))?;
    let plan = runtime.compile(&graph)?;

    let mut durations = Vec::with_capacity(iterations);
    let mut last_checksum = 0.0_f64;
    let mut output_shape = Vec::new();
    for index in 0..warmup + iterations {
        let start = Instant::now();
        let result = runtime.run_plan(&plan, vec![Value::TensorF32(input.clone())])?;
        let elapsed = start.elapsed();
        let values = result
            .outputs
            .get("result.out")
            .ok_or_else(|| FlowError::Execution("missing result.out terminal".to_owned()))?;
        let tensor = match values.as_slice() {
            [Value::TensorF32(tensor)] => tensor,
            _ => {
                return Err(FlowError::Execution(
                    "benchmark expects exactly one tensor output".to_owned(),
                ))
            }
        };
        output_shape = tensor.shape().to_vec();
        last_checksum = tensor.data().iter().map(|&value| f64::from(value)).sum();
        if index >= warmup {
            durations.push(elapsed.as_nanos());
        }
    }

    durations.sort_unstable();
    let min_ms = durations[0] as f64 / 1_000_000.0;
    let p50_ms = nearest_rank(&durations, 50, 100) as f64 / 1_000_000.0;
    let p95_ms = nearest_rank(&durations, 95, 100) as f64 / 1_000_000.0;
    let mean_ms =
        durations.iter().map(|&n| n as f64).sum::<f64>() / durations.len() as f64 / 1_000_000.0;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "benchmark": "onnx_cpu_graph_call",
            "execution_provider": "CPU",
            "model_path": args[0],
            "input_shape": input.shape(),
            "output_shape": output_shape,
            "warmup_iterations": warmup,
            "measured_iterations": iterations,
            "latency_ms": {
                "min": min_ms,
                "p50": p50_ms,
                "p95": p95_ms,
                "mean": mean_ms
            },
            "throughput_items_per_sec": if mean_ms > 0.0 { 1000.0 / mean_ms } else { 0.0 },
            "throughput_is_estimated_from_mean_latency": true,
            "last_output_sum": last_checksum,
            "includes_worker_thread_startup": true,
            "excludes_onnx_session_load": true
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{batch_input_bytes, nearest_rank, parse_count};
    use modelbox_core::TensorF32;

    #[test]
    fn batch_input_budget_is_bounded() {
        let input = TensorF32::new(vec![2], vec![1.0, 2.0]).unwrap();
        assert_eq!(batch_input_bytes(&input, 10).unwrap(), 80);
        assert!(batch_input_bytes(&input, 10_000_000).is_err());
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        assert_eq!(nearest_rank(&[1, 2, 3, 4, 5], 50, 100), 3);
        assert_eq!(nearest_rank(&[1, 2, 3, 4, 5], 95, 100), 5);
        assert_eq!(nearest_rank(&[1], 95, 100), 1);
    }

    #[test]
    fn iteration_bounds_are_enforced() {
        assert!(parse_count(Some(&"0".to_owned()), 100, 10_000, "iterations").is_err());
        assert!(parse_count(Some(&"10001".to_owned()), 100, 10_000, "iterations").is_err());
        assert_eq!(
            parse_count(Some(&"12".to_owned()), 100, 10_000, "iterations").unwrap(),
            12
        );
        assert_eq!(parse_count(None, 100, 10_000, "iterations").unwrap(), 100);
    }
}

const MAX_BATCH_BYTES: usize = 64 * 1024 * 1024;

const TENSOR_IN: [PortDef; 1] = [PortDef::new("in", DataType::TensorF32)];
const TENSOR_OUT: [PortDef; 1] = [PortDef::new("out", DataType::TensorF32)];

const IDENTITY_GRAPH: &str = r#"{
  "name": "tensor_identity_benchmark",
  "queue_capacity": 2,
  "nodes": [
    {"id": "source", "unit": "tensor.input"},
    {"id": "infer", "unit": "bench.tensor_identity"},
    {"id": "result", "unit": "tensor.output"}
  ],
  "edges": [
    {"from": "source.out", "to": "infer.in"},
    {"from": "infer.out", "to": "result.in"}
  ]
}"#;

/// A test-only FlowUnit preserving the 3-node inference graph topology.
/// Measures graph/worker/channel/tensor cloning without ONNX Runtime calls.
struct TensorIdentity;

impl FlowUnit for TensorIdentity {
    fn name(&self) -> &'static str {
        "bench.tensor_identity"
    }

    fn inputs(&self) -> &'static [PortDef] {
        &TENSOR_IN
    }

    fn outputs(&self) -> &'static [PortDef] {
        &TENSOR_OUT
    }

    fn process(
        &self,
        inputs: &[Value],
        _config: &serde_json::Value,
    ) -> Result<Vec<Value>, FlowError> {
        match inputs {
            [Value::TensorF32(tensor)] => Ok(vec![Value::TensorF32(tensor.clone())]),
            _ => Err(FlowError::Execution(
                "tensor identity expects one f32 tensor".to_owned(),
            )),
        }
    }
}

fn batch_input_bytes(input: &TensorF32, count: usize) -> Result<usize, FlowError> {
    let bytes = input
        .data()
        .len()
        .checked_mul(std::mem::size_of::<f32>())
        .and_then(|size| size.checked_mul(count))
        .ok_or_else(|| FlowError::Execution("batch byte size overflow".to_owned()))?;
    if bytes > MAX_BATCH_BYTES {
        return Err(FlowError::Execution(format!(
            "batch input exceeds {} MiB safety limit",
            MAX_BATCH_BYTES / 1024 / 1024
        )));
    }
    Ok(bytes)
}

/// Measures one actual finite-batch pipeline invocation, not the reciprocal
/// of independently measured single-request latency.
///
/// The entire input batch and graph execution plan are built before timing;
/// worker startup, inference, backpressure, capture and shutdown are timed.
pub(crate) fn run_batch(args: Vec<String>) -> Result<(), FlowError> {
    run_batch_impl(args, false)
}

/// Matches the 3-node ONNX graph with an identity tensor operator,
/// isolating much of the graph scheduling/transport/capture overhead.
pub(crate) fn run_overhead(args: Vec<String>) -> Result<(), FlowError> {
    if args.is_empty() || args.len() > 3 {
        return Err(FlowError::Execution(usage().to_owned()));
    }
    let mut normalized = Vec::with_capacity(args.len() + 1);
    normalized.push("<no-model>".to_owned());
    normalized.extend(args);
    run_batch_impl(normalized, true)
}

fn run_batch_impl(args: Vec<String>, without_onnx: bool) -> Result<(), FlowError> {
    if args.len() < 2 || args.len() > 4 {
        return Err(FlowError::Execution(usage().to_owned()));
    }
    let items = parse_count(args.get(2), 1_000, MAX_ITERATIONS, "items")?;
    let warmup = match args.get(3) {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| FlowError::Execution("warmup must be 0..=1000".to_owned()))?,
        None => 10,
    };
    if warmup > MAX_WARMUP {
        return Err(FlowError::Execution("warmup must be 0..=1000".to_owned()));
    }

    let input: TensorF32 = serde_json::from_str(&fs::read_to_string(&args[1])?)?;
    let input_batch_bytes = batch_input_bytes(&input, items)?;
    let graph: GraphSpec = serde_json::from_str(if without_onnx {
        IDENTITY_GRAPH
    } else {
        ONNX_GRAPH
    })?;
    let mut runtime = Runtime::default();
    if without_onnx {
        runtime.registry_mut().register(Arc::new(TensorIdentity))?;
    } else {
        runtime
            .registry_mut()
            .register(Arc::new(OnnxCpuFlowUnit::load(&args[0])?))?;
    }
    let plan = runtime.compile(&graph)?;

    for _ in 0..warmup {
        runtime.run_plan(&plan, vec![Value::TensorF32(input.clone())])?;
    }

    // Input preparation is intentionally outside the measured interval.
    let values: Vec<_> = (0..items)
        .map(|_| Value::TensorF32(input.clone()))
        .collect();

    let start = Instant::now();
    let result = runtime.run_plan(&plan, values)?;
    let elapsed = start.elapsed();

    let captured = result
        .outputs
        .get("result.out")
        .ok_or_else(|| FlowError::Execution("missing result.out terminal".to_owned()))?;
    if result.input_count != items || captured.len() != items {
        return Err(FlowError::Execution(format!(
            "batch output cardinality mismatch: inputs={}, outputs={}",
            result.input_count,
            captured.len()
        )));
    }

    let mut aggregate_checksum = 0.0_f64;
    let mut output_shape = Vec::new();
    for value in captured {
        match value {
            Value::TensorF32(tensor) => {
                output_shape = tensor.shape().to_vec();
                aggregate_checksum += tensor
                    .data()
                    .iter()
                    .map(|&value| f64::from(value))
                    .sum::<f64>();
            }
            _ => {
                return Err(FlowError::Execution(
                    "batch benchmark expects only f32 tensor outputs".to_owned(),
                ));
            }
        }
    }

    let elapsed_secs = elapsed.as_secs_f64();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "benchmark": if without_onnx {
                "tensor_identity_finite_batch_throughput"
            } else {
                "onnx_cpu_finite_batch_throughput"
            },
            "execution_provider": if without_onnx { "none" } else { "CPU" },
            "contains_onnx_inference": !without_onnx,
            "model_path": if without_onnx { serde_json::Value::Null } else { json!(args[0]) },
            "input_shape": input.shape(),
            "output_shape": output_shape,
            "batch_items": items,
            "warmup_single_frame_calls": warmup,
            "input_batch_bytes": input_batch_bytes,
            "elapsed_ms": elapsed_secs * 1000.0,
            "throughput_items_per_sec": if elapsed_secs > 0.0 {
                items as f64 / elapsed_secs
            } else {
                0.0
            },
            "aggregate_output_sum": aggregate_checksum,
            "timing_scope": "single_run_plan_over_preconstructed_finite_batch",
            "includes_worker_thread_startup_and_teardown": true,
            "includes_output_capture": true,
            "excludes_input_batch_construction": true,
            "excludes_onnx_session_load": true
        }))?
    );
    Ok(())
}
