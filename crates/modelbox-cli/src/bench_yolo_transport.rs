//! Model-free three-node Graph transport control on the actual YOLO input.
//!
//! Unlike TensorIdentity's borrowed implementation, this explicitly forwards
//! the owned f32 buffer. Graph and direct paths both receive pre-cloned inputs:
//! neither measured interval includes cloning the 640x640x3 f32 input.

use std::sync::Arc;
use std::time::Instant;

use modelbox_core::{
    DataType, FlowError, FlowUnit, GraphSpec, PortDef, Result, Runtime, TensorF32, Value,
};
use serde_json::{json, Value as JsonValue};

use crate::bench::IDENTITY_GRAPH;
use crate::yolo_infer;

const MAX_ROUNDS: usize = 101;
const MAX_WARMUP: usize = 100;
const INPUTS: [PortDef; 1] = [PortDef::new("in", DataType::TensorF32)];
const OUTPUTS: [PortDef; 1] = [PortDef::new("out", DataType::TensorF32)];

struct OwnedIdentity;

impl FlowUnit for OwnedIdentity {
    fn name(&self) -> &'static str {
        "bench.tensor_identity"
    }

    fn inputs(&self) -> &'static [PortDef] {
        &INPUTS
    }

    fn outputs(&self) -> &'static [PortDef] {
        &OUTPUTS
    }

    fn process(&self, inputs: &[Value], _config: &JsonValue) -> Result<Vec<Value>> {
        match inputs {
            [Value::TensorF32(_)] => Ok(inputs.to_vec()),
            _ => Err(invalid("expects one f32 tensor")),
        }
    }

    fn process_owned(&self, inputs: Vec<Value>, _config: &JsonValue) -> Result<Vec<Value>> {
        if !matches!(inputs.as_slice(), [Value::TensorF32(_)]) {
            return Err(invalid("expects one owned f32 tensor"));
        }
        Ok(inputs)
    }
}

fn invalid(message: impl std::fmt::Display) -> FlowError {
    FlowError::Execution(format!("YOLO tensor transport control: {message}"))
}

fn count(raw: Option<&String>, default: usize, low: usize, high: usize) -> Result<usize> {
    let value = match raw {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| invalid(format!("count must be {low}..={high}")))?,
        None => default,
    };
    if !(low..=high).contains(&value) {
        return Err(invalid(format!("count must be {low}..={high}")));
    }
    Ok(value)
}

fn stats(ns: &[u128]) -> JsonValue {
    let mut ordered = ns.to_vec();
    ordered.sort_unstable();
    let milliseconds = |value: u128| value as f64 / 1_000_000.0;
    let rank = |percent: usize| ordered[(ordered.len() * percent).div_ceil(100) - 1];
    json!({
        "min": milliseconds(ordered[0]),
        "mean": ns.iter().map(|&v| v as f64).sum::<f64>() / ns.len() as f64 / 1_000_000.0,
        "p50": milliseconds(rank(50)),
        "p95": milliseconds(rank(95)),
        "max": milliseconds(*ordered.last().expect("nonempty timings"))
    })
}

fn one_tensor(mut output: Vec<Value>) -> Result<TensorF32> {
    if output.len() != 1 {
        return Err(invalid("expected one output tensor"));
    }
    match output.remove(0) {
        Value::TensorF32(tensor) => Ok(tensor),
        _ => Err(invalid("unexpected output type")),
    }
}

pub(crate) fn run(args: Vec<String>) -> Result<()> {
    if args.is_empty() || args.len() > 3 {
        return Err(invalid(crate::usage()));
    }
    let rounds = count(args.get(1), 21, 3, MAX_ROUNDS)?;
    let warmup = count(args.get(2), 5, 0, MAX_WARMUP)?;

    let rgb = image::open(&args[0]).map_err(invalid)?.to_rgb8();
    let (input, _) = yolo_infer::letterbox(&rgb)?;
    let bytes = input.data().len() * std::mem::size_of::<f32>();
    let mut runtime = Runtime::default();
    let identity = Arc::new(OwnedIdentity);
    runtime.registry_mut().register(identity.clone())?;
    let spec: GraphSpec = serde_json::from_str(IDENTITY_GRAPH)?;
    let plan = runtime.compile(&spec)?;

    let mut graph_ns = Vec::with_capacity(rounds);
    let mut direct_ns = Vec::with_capacity(rounds);
    let mut pair_delta_ms = Vec::with_capacity(rounds);
    let mut checksum = 0.0_f64;

    // Values are cloned BEFORE each measured call so that we measure only
    // forwarding and worker/channel/capture overhead, not large Vec copies.
    for turn in 0..(warmup + rounds) {
        let graph_input = Value::TensorF32(input.clone());
        let direct_input = Value::TensorF32(input.clone());
        // These allocations are created before either timed interval. Verify
        // that the single-consumer Graph forwards them without a hidden copy.
        let graph_buffer = match &graph_input {
            Value::TensorF32(tensor) => tensor.data().as_ptr(),
            _ => unreachable!("prepared Graph input is a tensor"),
        };
        let direct_buffer = match &direct_input {
            Value::TensorF32(tensor) => tensor.data().as_ptr(),
            _ => unreachable!("prepared direct input is a tensor"),
        };

        let run_graph = || -> Result<(u128, TensorF32)> {
            let start = Instant::now();
            let result = runtime.run_plan(&plan, vec![graph_input])?;
            let elapsed = start.elapsed().as_nanos();
            let mut outputs = result.outputs;
            let output = outputs
                .remove("result.out")
                .ok_or_else(|| invalid("missing result.out"))?;
            Ok((elapsed, one_tensor(output)?))
        };
        let run_direct = || -> Result<(u128, TensorF32)> {
            let start = Instant::now();
            let output = identity.process_owned(vec![direct_input], &JsonValue::Null)?;
            let elapsed = start.elapsed().as_nanos();
            Ok((elapsed, one_tensor(output)?))
        };

        let (g_ns, g_tensor, d_ns, d_tensor) = if turn % 2 == 0 {
            let (g_ns, g_tensor) = run_graph()?;
            let (d_ns, d_tensor) = run_direct()?;
            (g_ns, g_tensor, d_ns, d_tensor)
        } else {
            let (d_ns, d_tensor) = run_direct()?;
            let (g_ns, g_tensor) = run_graph()?;
            (g_ns, g_tensor, d_ns, d_tensor)
        };
        if g_tensor != input || d_tensor != input {
            return Err(invalid("identity transfer altered the input tensor"));
        }
        if !std::ptr::eq(g_tensor.data().as_ptr(), graph_buffer) {
            return Err(invalid(
                "Graph unexpectedly copied the single-consumer input tensor",
            ));
        }
        if !std::ptr::eq(d_tensor.data().as_ptr(), direct_buffer) {
            return Err(invalid(
                "direct owned identity unexpectedly copied the input tensor",
            ));
        }
        checksum = g_tensor.data().iter().map(|&v| f64::from(v)).sum();
        if turn >= warmup {
            graph_ns.push(g_ns);
            direct_ns.push(d_ns);
            pair_delta_ms.push((g_ns as i128 - d_ns as i128) as f64 / 1_000_000.0);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "benchmark": "yolov8n_input_identity_graph_transport_control",
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "input_image": args[0],
            "input_tensor_shape": [1,3,640,640],
            "input_tensor_bytes": bytes,
            "warmup_pairs": warmup,
            "measured_pairs": rounds,
            "graph_identity_ms": stats(&graph_ns),
            "direct_owned_identity_ms": stats(&direct_ns),
            "paired_graph_minus_direct_ms": pair_delta_ms,
            "output_matches_input_exactly": true,
            "graph_tensor_buffer_reused": true,
            "direct_tensor_buffer_reused": true,
            "last_input_output_checksum": checksum,
            "preprocessing_and_input_clones_outside_timed_intervals": true,
            "onnx_model_loaded": false,
            "same_graph_topology_as_onnx": true,
            "warning": "No ONNX compute: this is a separate empty-operator worker/channel/capture control, NOT a subtraction-ready estimate of the model-specific Graph overhead. It measures one input frame with fresh workers per call."
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{count, stats, OwnedIdentity};
    use crate::bench::IDENTITY_GRAPH;
    use modelbox_core::{FlowUnit, GraphSpec, Runtime, TensorF32, Value};
    use serde_json::Value as JsonValue;
    use std::sync::Arc;

    #[test]
    fn accepts_bounded_iterations() {
        assert_eq!(count(None, 21, 3, 101).unwrap(), 21);
        assert_eq!(count(Some(&"0".to_owned()), 5, 0, 100).unwrap(), 0);
        assert!(count(Some(&"2".to_owned()), 21, 3, 101).is_err());
        assert!(count(Some(&"102".to_owned()), 21, 3, 101).is_err());
    }

    #[test]
    fn validates_percentiles() {
        let summary = stats(&[1_000_000, 2_000_000, 3_000_000, 4_000_000, 5_000_000]);
        assert_eq!(summary["p50"], 3.0);
        assert_eq!(summary["p95"], 5.0);
    }

    #[test]
    fn owned_graph_round_trip_reuses_the_original_allocation() {
        let mut runtime = Runtime::default();
        runtime
            .registry_mut()
            .register(Arc::new(OwnedIdentity))
            .unwrap();
        let graph: GraphSpec = serde_json::from_str(IDENTITY_GRAPH).unwrap();
        let plan = runtime.compile(&graph).unwrap();

        let source = TensorF32::new(vec![1, 2], vec![1.0, 2.0]).unwrap();
        let original_ptr = source.data().as_ptr();
        let result = runtime
            .run_plan(&plan, vec![Value::TensorF32(source)])
            .unwrap();
        let values = result.outputs.get("result.out").unwrap();
        match values.as_slice() {
            [Value::TensorF32(tensor)] => {
                assert_eq!(tensor.data(), &[1.0, 2.0]);
                assert!(std::ptr::eq(tensor.data().as_ptr(), original_ptr));
            }
            _ => panic!("expected one tensor"),
        }
    }

    #[test]
    fn owned_identity_is_exact_and_type_checked() {
        let input = Value::TensorF32(TensorF32::new(vec![1, 2], vec![1.0, 2.0]).unwrap());
        assert_eq!(
            OwnedIdentity
                .process_owned(vec![input.clone()], &JsonValue::Null)
                .unwrap(),
            vec![input]
        );
        assert!(OwnedIdentity
            .process_owned(vec![Value::Integer(1)], &JsonValue::Null)
            .is_err());
    }
}
