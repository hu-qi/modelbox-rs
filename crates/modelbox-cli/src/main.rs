use std::env;
use std::fs;
use std::path::Path;

use modelbox_core::{FlowError, GraphSpec, Runtime, Value};

#[cfg(feature = "onnx")]
mod bench;
#[cfg(feature = "onnx")]
mod bench_compare;
#[cfg(feature = "onnx")]
mod mnist_eval;
#[cfg(feature = "onnx")]
mod mnist_png;

const DEMO: &str = include_str!("../../../examples/hello.json");
#[cfg(feature = "onnx")]
const ONNX_GRAPH: &str = include_str!("../../../examples/relu.json");

fn usage() -> &'static str {
    "Usage:
  modelbox demo [TEXT ...]
  modelbox validate <graph.json>
  modelbox run <graph.json> <TEXT>
  modelbox run-batch <graph.json> <TEXT ...>
  modelbox infer <model.onnx> <tensor.json>   (requires --features onnx)
  modelbox infer-mnist-png <model.onnx> <28x28-grayscale.png>   (requires --features onnx)
  modelbox eval-mnist-csv <model.onnx> <labeled-test.csv> [min_correct]   (requires --features onnx)
  modelbox bench <model.onnx> <tensor.json> [iterations] [warmup]   (requires --features onnx)
  modelbox bench-batch <model.onnx> <tensor.json> [items] [warmup]   (requires --features onnx)
  modelbox bench-overhead <tensor.json> [items] [warmup]   (requires --features onnx)
  modelbox bench-compare <model.onnx> <tensor.json> [items] [rounds] [warmup]   (requires --features onnx)

Text examples use core.input; ONNX inference is an opt-in feature.
Graph JSON is the native v0.1 format; legacy DOT/TOML import is not yet supported."
}

fn main() {
    if let Err(error) = execute() {
        eprintln!("modelbox: {error}");
        std::process::exit(1);
    }
}

fn parse_graph(contents: &str) -> Result<GraphSpec, FlowError> {
    Ok(serde_json::from_str(contents)?)
}

fn read_graph(path: &str) -> Result<GraphSpec, FlowError> {
    parse_graph(&fs::read_to_string(Path::new(path))?)
}

fn execute() -> Result<(), FlowError> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();
    if command == "--help" || command == "-h" || command == "help" {
        println!("{}", usage());
        return Ok(());
    }
    let runtime = Runtime::default();
    match command.as_str() {
        "demo" => {
            let graph = parse_graph(DEMO)?;
            let inputs: Vec<_> = args.map(Value::Text).collect();
            let inputs = if inputs.is_empty() {
                vec![Value::Text("hello modelbox".to_owned())]
            } else {
                inputs
            };
            let result = runtime.run_many(&graph, inputs)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        #[cfg(feature = "onnx")]
        "bench" => bench::run(args.collect())?,
        #[cfg(feature = "onnx")]
        "bench-batch" => bench::run_batch(args.collect())?,
        #[cfg(feature = "onnx")]
        "bench-overhead" => bench::run_overhead(args.collect())?,
        #[cfg(feature = "onnx")]
        "bench-compare" => bench_compare::run(args.collect())?,
        #[cfg(feature = "onnx")]
        "infer-mnist-png" => mnist_png::run(args.collect())?,
        #[cfg(feature = "onnx")]
        "eval-mnist-csv" => mnist_eval::run(args.collect())?,
        #[cfg(feature = "onnx")]
        "infer" => {
            use std::sync::Arc;

            use modelbox_core::{OnnxCpuFlowUnit, TensorF32};

            let model_path = args
                .next()
                .ok_or_else(|| FlowError::Execution(usage().to_owned()))?;
            let tensor_path = args
                .next()
                .ok_or_else(|| FlowError::Execution(usage().to_owned()))?;
            if args.next().is_some() {
                return Err(FlowError::Execution(usage().to_owned()));
            }
            let tensor: TensorF32 = serde_json::from_str(&fs::read_to_string(tensor_path)?)?;
            let graph = parse_graph(ONNX_GRAPH)?;
            let mut inference_runtime = Runtime::default();
            inference_runtime
                .registry_mut()
                .register(Arc::new(OnnxCpuFlowUnit::load(model_path)?))?;
            let result = inference_runtime.run(&graph, Value::TensorF32(tensor))?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        "validate" => {
            let path = args
                .next()
                .ok_or_else(|| FlowError::Execution(usage().to_owned()))?;
            if args.next().is_some() {
                return Err(FlowError::Execution(usage().to_owned()));
            }
            let graph = read_graph(&path)?;
            let plan = runtime.compile(&graph)?;
            println!(
                "valid graph: {} ({} nodes, {} edges, terminal outputs: {:?})",
                graph.name,
                plan.node_count(),
                plan.edge_count(),
                plan.terminal_names()
            );
        }
        "run" | "run-batch" => {
            let path = args
                .next()
                .ok_or_else(|| FlowError::Execution(usage().to_owned()))?;
            let inputs: Vec<_> = args.map(Value::Text).collect();
            if command == "run" && inputs.len() != 1 {
                return Err(FlowError::Execution(usage().to_owned()));
            }
            let graph = read_graph(&path)?;
            let result = runtime.run_many(&graph, inputs)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        _ => return Err(FlowError::Execution(usage().to_owned())),
    }
    Ok(())
}
