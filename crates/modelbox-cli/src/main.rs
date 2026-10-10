use std::env;
use std::fs;
use std::path::Path;

use modelbox_core::{FlowError, GraphSpec, Runtime, Value};

const DEMO: &str = include_str!("../../../examples/hello.json");

fn usage() -> &'static str {
    "Usage:
  modelbox demo [TEXT ...]
  modelbox validate <graph.json>
  modelbox run <graph.json> <TEXT>
  modelbox run-batch <graph.json> <TEXT ...>

The input source is core.input and the v0.1 data type is text.
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
