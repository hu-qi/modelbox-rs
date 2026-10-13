//! ModelBox Rust v0.1: typed dataflow graph compilation and bounded execution.
//!
//! This CPU-first milestone deliberately does not claim binary compatibility with
//! legacy ModelBox drivers or support for GPU/NPU execution.

mod cpu_buffer;
mod error;
mod flow_unit;
mod graph;
mod runtime;
mod value;

#[cfg(feature = "onnx")]
mod onnx_cpu;
#[cfg(feature = "onnx")]
pub use onnx_cpu::OnnxCpuFlowUnit;

pub use cpu_buffer::{BufferMetaValue, CpuBuffer};
pub use error::FlowError;
pub use flow_unit::{FlowRegistry, FlowUnit, FlowUnitDescriptor, PortDef, PortDescriptor};
pub use graph::{EdgeSpec, ExecutionPlan, GraphSpec, NodeSpec};
pub use runtime::{
    EdgeChannelProfile, EdgeDeliveryEvent, EdgeTracedRun, NodeProfile, NodeTransportProfile,
    ProfiledRun, RunResult, Runtime, TraceEvent, TracedRun, TransportProfiledRun,
};
pub use value::{DataType, TensorF32, Value};

pub type Result<T> = std::result::Result<T, FlowError>;
