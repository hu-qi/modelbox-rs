use serde::{Deserialize, Serialize};

/// Edge port types are checked before workers are launched.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    Text,
    Integer,
    Boolean,
    Bytes,
    TensorF32,
}

/// Owned values avoid exposing borrowed buffers across worker lifetimes.
/// Device-backed buffers and zero-copy transfer are intentionally out of scope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Text(String),
    Integer(i64),
    Boolean(bool),
    Bytes(Vec<u8>),
    TensorF32(TensorF32),
}

impl Value {
    pub fn data_type(&self) -> DataType {
        match self {
            Self::Text(_) => DataType::Text,
            Self::Integer(_) => DataType::Integer,
            Self::Boolean(_) => DataType::Boolean,
            Self::Bytes(_) => DataType::Bytes,
            Self::TensorF32(_) => DataType::TensorF32,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "TensorRepr", into = "TensorRepr")]
pub struct TensorF32 {
    shape: Vec<usize>,
    data: Vec<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TensorRepr {
    shape: Vec<usize>,
    data: Vec<f32>,
}

impl TryFrom<TensorRepr> for TensorF32 {
    type Error = String;

    fn try_from(raw: TensorRepr) -> std::result::Result<Self, Self::Error> {
        Self::new(raw.shape, raw.data).map_err(|error| error.to_string())
    }
}

impl From<TensorF32> for TensorRepr {
    fn from(value: TensorF32) -> Self {
        Self {
            shape: value.shape,
            data: value.data,
        }
    }
}

impl TensorF32 {
    /// Owned contiguous row-major f32 tensor on CPU.
    /// Shape [] denotes one scalar element.
    pub fn new(shape: Vec<usize>, data: Vec<f32>) -> crate::Result<Self> {
        if shape.len() > 8 || shape.contains(&0) {
            return Err(crate::FlowError::Execution(
                "invalid tensor rank or dimension".to_owned(),
            ));
        }
        let count = shape
            .iter()
            .try_fold(1usize, |a, &b| a.checked_mul(b))
            .ok_or_else(|| crate::FlowError::Execution("tensor size overflow".to_owned()))?;
        if count > 8_388_608 || count != data.len() {
            return Err(crate::FlowError::Execution(format!(
                "tensor requires {count} values; got {}",
                data.len()
            )));
        }
        if data.iter().any(|v| !v.is_finite()) {
            return Err(crate::FlowError::Execution(
                "tensor has non-finite values".to_owned(),
            ));
        }
        Ok(Self { shape, data })
    }

    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }
}
