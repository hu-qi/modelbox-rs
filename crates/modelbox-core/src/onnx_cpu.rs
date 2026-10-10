//! Optional CPU-only ONNX Runtime adapter. Supports a single f32 input/output
//! model and serializes shared Session::run() access with a mutex.

use std::path::Path;
use std::sync::Mutex;

use ort::{session::Session, value::TensorRef};
use serde_json::Value as JsonValue;

use crate::{DataType, FlowError, FlowUnit, PortDef, Result, TensorF32, Value};

const IN: [PortDef; 1] = [PortDef::new("in", DataType::TensorF32)];
const OUT: [PortDef; 1] = [PortDef::new("out", DataType::TensorF32)];

pub struct OnnxCpuFlowUnit {
    session: Mutex<Session>,
}

fn onnx_error(error: impl std::fmt::Display) -> FlowError {
    FlowError::Execution(format!("ONNX CPU: {error}"))
}

impl OnnxCpuFlowUnit {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let session = Session::builder()
            .map_err(onnx_error)?
            .commit_from_file(path.as_ref())
            .map_err(onnx_error)?;
        if session.inputs().len() != 1 || session.outputs().len() != 1 {
            return Err(onnx_error("v0.2 requires exactly one input and one output"));
        }
        Ok(Self {
            session: Mutex::new(session),
        })
    }
}

impl FlowUnit for OnnxCpuFlowUnit {
    fn name(&self) -> &'static str {
        "onnx.cpu"
    }

    fn inputs(&self) -> &'static [PortDef] {
        &IN
    }

    fn outputs(&self) -> &'static [PortDef] {
        &OUT
    }

    fn process(&self, values: &[Value], _config: &JsonValue) -> Result<Vec<Value>> {
        let input = match values.first() {
            Some(Value::TensorF32(tensor)) => tensor,
            _ => return Err(onnx_error("expected tensor_f32 input")),
        };
        let array =
            TensorRef::from_array_view((input.shape(), input.data())).map_err(onnx_error)?;
        let mut session = self.session.lock().map_err(onnx_error)?;
        let outputs = session.run(ort::inputs![array]).map_err(onnx_error)?;
        let (shape, data) = outputs[0].try_extract_tensor::<f32>().map_err(onnx_error)?;
        let dims: Vec<usize> = shape
            .iter()
            .map(|&dim| usize::try_from(dim).map_err(onnx_error))
            .collect::<Result<Vec<_>>>()?;
        Ok(vec![Value::TensorF32(TensorF32::new(dims, data.to_vec())?)])
    }
}
