//! Interleaved paired ONNX and Tensor Identity finite-batch measurements.
//! Ratios here are descriptive; they do not attribute time to the scheduler
//! or isolate pure ONNX kernel cost.

use std::fs;
use std::sync::Arc;
use std::time::Instant;

use modelbox_core::{
    ExecutionPlan, FlowError, GraphSpec, OnnxCpuFlowUnit, Runtime, TensorF32, Value,
};
use serde_json::{json, Value as JsonValue};

use crate::bench::{batch_input_bytes, TensorIdentity, IDENTITY_GRAPH};
use crate::{usage, ONNX_GRAPH};

const MAX_ITEMS: usize = 10_000;
const MAX_ROUNDS: usize = 31;
const MAX_WARMUP: usize = 1_000;

fn bounded_number(
    argument: Option<&String>,
    default: usize,
    minimum: usize,
    maximum: usize,
    label: &str,
) -> Result<usize, FlowError> {
    let value = match argument {
        Some(raw) => raw.parse::<usize>().map_err(|_| {
            FlowError::Execution(format!("{label} must be in {minimum}..={maximum}"))
        })?,
        None => default,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(FlowError::Execution(format!(
            "{label} must be in {minimum}..={maximum}"
        )));
    }
    Ok(value)
}

fn measure(
    runtime: &Runtime,
    plan: &ExecutionPlan,
    prepared: &[Value],
) -> Result<(u128, f64), FlowError> {
    // Input buffer construction and cloning are deliberately outside timing.
    let input = prepared.to_vec();
    let start = Instant::now();
    let result = runtime.run_plan(plan, input)?;
    let elapsed_ns = start.elapsed().as_nanos();

    let values = result
        .outputs
        .get("result.out")
        .ok_or_else(|| FlowError::Execution("missing result.out capture".to_owned()))?;
    if result.input_count != prepared.len() || values.len() != prepared.len() {
        return Err(FlowError::Execution(format!(
            "batch cardinality mismatch: expected {}, got {}",
            prepared.len(),
            values.len()
        )));
    }
    let mut checksum = 0.0_f64;
    for value in values {
        match value {
            Value::TensorF32(tensor) => {
                checksum += tensor
                    .data()
                    .iter()
                    .map(|&v| f64::from(v))
                    .sum::<f64>();
            }
            _ => {
                return Err(FlowError::Execution(
                    "benchmark requires tensor_f32 output".to_owned(),
                ));
            }
        }
    }
    Ok((elapsed_ns, checksum))
}

fn summary(observations: &[u128], items: usize) -> JsonValue {
    let mut ordered = observations.to_vec();
    ordered.sort_unstable();
    let percentile = |percent: usize| {
        let index = (ordered.len() * percent).div_ceil(100) - 1;
        ordered[index] as f64 / 1_000_000.0
    };
    let mean_ms = ordered.iter().map(|&ns| ns as f64).sum::<f64>()
        / ordered.len() as f64
        / 1_000_000.0;
    let median_ms = percentile(50);
    json!({
        "elapsed_ms": {
            "min": ordered[0] as f64 / 1_000_000.0,
            "p50": median_ms,
            "p95": percentile(95),
            "mean": mean_ms
        },
        "throughput_items_per_sec_from_median_batch_duration": if median_ms > 0.0 {
            items as f64 * 1000.0 / median_ms
        } else {
            0.0
        },
        "per_round_elapsed_ms": observations
            .iter()
            .map(|&ns| ns as f64 / 1_000_000.0)
            .collect::<Vec<_>>()
    })
}

pub(crate) fn run(args: Vec<String>) -> Result<(), FlowError> {
    if args.len() < 2 || args.len() > 5 {
        return Err(FlowError::Execution(usage().to_owned()));
    }
    let items = bounded_number(args.get(2), 1_000, 1, MAX_ITEMS, "items")?;
    let rounds = bounded_number(args.get(3), 9, 3, MAX_ROUNDS, "rounds")?;
    let warmup = bounded_number(args.get(4), 10, 0, MAX_WARMUP, "warmup")?;

    let input: TensorF32 = serde_json::from_str(&fs::read_to_string(&args[1])?)?;
    let input_batch_bytes = batch_input_bytes(&input, items)?;
    let identity_graph: GraphSpec = serde_json::from_str(IDENTITY_GRAPH)?;
    let onnx_graph: GraphSpec = serde_json::from_str(ONNX_GRAPH)?;

    let mut identity_runtime = Runtime::default();
    identity_runtime
        .registry_mut()
        .register(Arc::new(TensorIdentity))?;
    let identity_plan = identity_runtime.compile(&identity_graph)?;

    let mut onnx_runtime = Runtime::default();
    onnx_runtime
        .registry_mut()
        .register(Arc::new(OnnxCpuFlowUnit::load(&args[0])?))?;
    let onnx_plan = onnx_runtime.compile(&onnx_graph)?;

    for _ in 0..warmup {
        let sample = vec![Value::TensorF32(input.clone())];
        identity_runtime.run_plan(&identity_plan, sample.clone())?;
        onnx_runtime.run_plan(&onnx_plan, sample)?;
    }
    let batch: Vec<Value> = (0..items)
        .map(|_| Value::TensorF32(input.clone()))
        .collect();
    let mut identity_ns = Vec::with_capacity(rounds);
    let mut onnx_ns = Vec::with_capacity(rounds);
    let mut identity_checksum = 0.0_f64;
    let mut onnx_checksum = 0.0_f64;

    // Alternating order mitigates first-in-pair bias and progressive thermal drift.
    for index in 0..rounds {
        if index % 2 == 0 {
            let (ns, checksum) = measure(&identity_runtime, &identity_plan, &batch)?;
            identity_ns.push(ns);
            identity_checksum = checksum;
            let (ns, checksum) = measure(&onnx_runtime, &onnx_plan, &batch)?;
            onnx_ns.push(ns);
            onnx_checksum = checksum;
        } else {
            let (ns, checksum) = measure(&onnx_runtime, &onnx_plan, &batch)?;
            onnx_ns.push(ns);
            onnx_checksum = checksum;
            let (ns, checksum) = measure(&identity_runtime, &identity_plan, &batch)?;
            identity_ns.push(ns);
            identity_checksum = checksum;
        }
    }

    let identity_stats = summary(&identity_ns, items);
    let onnx_stats = summary(&onnx_ns, items);
    let identity_median = identity_stats["elapsed_ms"]["p50"]
        .as_f64()
        .ok_or_else(|| FlowError::Execution("missing identity median".to_owned()))?;
    let onnx_median = onnx_stats["elapsed_ms"]["p50"]
        .as_f64()
        .ok_or_else(|| FlowError::Execution("missing ONNX median".to_owned()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "benchmark": "paired_onnx_vs_identity_finite_batch",
            "model_path": args[0],
            "batch_items": items,
            "paired_rounds": rounds,
            "warmup_single_frame_calls_per_graph": warmup,
            "input_batch_bytes": input_batch_bytes,
            "input_shape": input.shape(),
            "execution_order": "identity_then_onnx_even_rounds; onnx_then_identity_odd_rounds",
            "identity": identity_stats,
            "onnx_cpu": onnx_stats,
            "last_checksums": {
                "identity": identity_checksum,
                "onnx_cpu": onnx_checksum
            },
            "descriptive_comparison": {
                "identity_median_as_percent_of_onnx_median": if onnx_median > 0.0 {
                    100.0 * identity_median / onnx_median
                } else {
                    0.0
                },
                "onnx_minus_identity_median_ms": onnx_median - identity_median,
                "warning": "Identity median is NOT pure scheduler overhead; difference is NOT pure ONNX kernel time"
            },
            "measurement_scope": "one preconstructed finite batch per timed run_plan; session loading and preparation excluded; thread startup, transport, inference and capture included"
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{bounded_number, summary};

    #[test]
    fn number_bounds() {
        assert_eq!(bounded_number(None, 9, 3, 31, "rounds").unwrap(), 9);
        assert_eq!(bounded_number(Some(&"3".to_owned()), 9, 3, 31, "rounds").unwrap(), 3);
        assert!(bounded_number(Some(&"2".to_owned()), 9, 3, 31, "rounds").is_err());
        assert!(bounded_number(Some(&"32".to_owned()), 9, 3, 31, "rounds").is_err());
        assert_eq!(bounded_number(Some(&"0".to_owned()), 10, 0, 1000, "warmup").unwrap(), 0);
    }

    #[test]
    fn sample_summary_percentiles() {
        let s = summary(&[2_000_000, 1_000_000, 5_000_000, 3_000_000, 4_000_000], 1000);
        assert_eq!(s["elapsed_ms"]["p50"].as_f64().unwrap(), 3.0);
        assert_eq!(s["elapsed_ms"]["p95"].as_f64().unwrap(), 5.0);
        assert_eq!(s["elapsed_ms"]["mean"].as_f64().unwrap(), 3.0);
    }
}
