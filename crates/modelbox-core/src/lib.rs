//! ModelBox Rust v0.1: typed dataflow graph compilation and bounded execution.
//!
//! This CPU-first milestone deliberately does not claim binary compatibility with
//! legacy ModelBox drivers or support for GPU/NPU execution.

mod error;
mod flow_unit;
mod graph;
mod runtime;
mod value;

#[cfg(feature = "onnx")]
mod onnx_cpu;
#[cfg(feature = "onnx")]
pub use onnx_cpu::OnnxCpuFlowUnit;

pub use error::FlowError;
pub use flow_unit::{FlowRegistry, FlowUnit, PortDef};
pub use graph::{EdgeSpec, ExecutionPlan, GraphSpec, NodeSpec};
pub use runtime::{RunResult, Runtime};
pub use value::{DataType, TensorF32, Value};

pub type Result<T> = std::result::Result<T, FlowError>;
