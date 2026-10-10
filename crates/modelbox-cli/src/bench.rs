//! End-to-end CPU graph-call benchmark for a single f32 ONNX inference FlowUnit.
//! The model session and graph plan are built once, before measured iterations.

use std::fs;
use std::sync::Arc;
use std::time::Instant;

use modelbox_core::{FlowError, GraphSpec, OnnxCpuFlowUnit, Runtime, TensorF32, Value};
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
            "last_output_sum": last_checksum,
            "includes_worker_thread_startup": true,
            "excludes_onnx_session_load": true
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{nearest_rank, parse_count};

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
