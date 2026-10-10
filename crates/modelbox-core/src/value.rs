use serde::{Deserialize, Serialize};

/// Edge port types are checked before workers are launched.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    Text,
    Integer,
    Boolean,
    Bytes,
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
}

impl Value {
    pub fn data_type(&self) -> DataType {
        match self {
            Self::Text(_) => DataType::Text,
            Self::Integer(_) => DataType::Integer,
            Self::Boolean(_) => DataType::Boolean,
            Self::Bytes(_) => DataType::Bytes,
        }
    }
}
