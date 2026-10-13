use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{FlowError, Result};

/// A deliberately limited subset of legacy ModelBox CPU Buffer metadata.
/// Only explicitly typed scalar attributes are supported; there is no
/// arbitrary pointer/device/stream context serialization.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BufferMetaValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

/// Immutable CPU-owned bytes and small, typed metadata. Cloning a CpuBuffer
/// shares storage; deep_copy() detaches both data and metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "CpuBufferRepr", into = "CpuBufferRepr")]
pub struct CpuBuffer {
    data: Arc<Vec<u8>>,
    metadata: Arc<BTreeMap<String, BufferMetaValue>>,
}

#[derive(Serialize, Deserialize)]
struct CpuBufferRepr {
    data: Vec<u8>,
    #[serde(default)]
    metadata: BTreeMap<String, BufferMetaValue>,
}

impl CpuBuffer {
    pub const MAX_BYTES: usize = 16 * 1024 * 1024;
    pub const MAX_METADATA_FIELDS: usize = 64;

    pub fn new(data: Vec<u8>, metadata: BTreeMap<String, BufferMetaValue>) -> Result<Self> {
        if data.len() > Self::MAX_BYTES {
            return Err(FlowError::Execution(format!(
                "CPU buffer exceeds {} bytes",
                Self::MAX_BYTES
            )));
        }
        if metadata.len() > Self::MAX_METADATA_FIELDS
            || metadata.keys().any(|key| {
                key.is_empty()
                    || key.len() > 128
                    || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
        {
            return Err(FlowError::Execution(
                "invalid CPU buffer metadata field count or key".to_owned(),
            ));
        }
        for value in metadata.values() {
            if let BufferMetaValue::Text(text) = value {
                if text.len() > 4096 {
                    return Err(FlowError::Execution(
                        "CPU buffer metadata string exceeds 4096 bytes".to_owned(),
                    ));
                }
            }
        }
        Ok(Self {
            data: Arc::new(data),
            metadata: Arc::new(metadata),
        })
    }

    pub fn data(&self) -> &[u8] {
        self.data.as_slice()
    }

    pub fn metadata(&self) -> &BTreeMap<String, BufferMetaValue> {
        &self.metadata
    }

    /// Exact shared data ownership, unlike an ordinary byte-for-byte comparison.
    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }

    pub fn deep_copy(&self) -> Self {
        Self {
            data: Arc::new((*self.data).clone()),
            metadata: Arc::new((*self.metadata).clone()),
        }
    }
}

impl TryFrom<CpuBufferRepr> for CpuBuffer {
    type Error = String;

    fn try_from(value: CpuBufferRepr) -> std::result::Result<Self, Self::Error> {
        Self::new(value.data, value.metadata).map_err(|error| error.to_string())
    }
}

impl From<CpuBuffer> for CpuBufferRepr {
    fn from(value: CpuBuffer) -> Self {
        Self {
            data: Arc::try_unwrap(value.data).unwrap_or_else(|shared| (*shared).clone()),
            metadata: Arc::try_unwrap(value.metadata)
                .unwrap_or_else(|shared| (*shared).clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharing_and_detaching_preserve_all_metadata() {
        let mut metadata = BTreeMap::new();
        metadata.insert("width".to_owned(), BufferMetaValue::Integer(2));
        metadata.insert("height".to_owned(), BufferMetaValue::Integer(3));
        metadata.insert("format".to_owned(), BufferMetaValue::Text("rgb".to_owned()));
        metadata.insert("ready".to_owned(), BufferMetaValue::Boolean(true));
        let buffer = CpuBuffer::new(vec![0, 1, 2, 255], metadata).unwrap();
        let shared = buffer.clone();
        let detached = buffer.deep_copy();
        assert!(buffer.shares_storage_with(&shared));
        assert!(!buffer.shares_storage_with(&detached));
        assert_eq!(buffer, shared);
        assert_eq!(buffer, detached);
        assert_eq!(buffer.data(), &[0, 1, 2, 255]);
    }

    #[test]
    fn serde_roundtrip_preserves_binary_zero_and_typed_metadata() {
        let buffer = CpuBuffer::new(
            vec![0, 128, 255],
            BTreeMap::from([
                ("width".to_owned(), BufferMetaValue::Integer(1)),
                ("format".to_owned(), BufferMetaValue::Text("rgb".to_owned())),
            ]),
        )
        .unwrap();
        let payload = serde_json::to_string(&buffer).unwrap();
        let restored: CpuBuffer = serde_json::from_str(&payload).unwrap();
        assert_eq!(buffer, restored);
        assert_eq!(restored.data(), &[0, 128, 255]);
    }

    #[test]
    fn invalid_payload_or_metadata_fails_closed() {
        let bad_key = r#"{"data":[1,2,3],"metadata":{"not valid":1}}"#;
        assert!(serde_json::from_str::<CpuBuffer>(bad_key).is_err());
        let complex_meta = r#"{"data":[],"metadata":{"shape":[1,2,3]}}"#;
        assert!(serde_json::from_str::<CpuBuffer>(complex_meta).is_err());
        let illegal_byte = r#"{"data":[256],"metadata":{}}"#;
        assert!(serde_json::from_str::<CpuBuffer>(illegal_byte).is_err());
        let huge_meta = BTreeMap::from([(
            "note".to_owned(),
            BufferMetaValue::Text("x".repeat(4097)),
        )]);
        assert!(CpuBuffer::new(Vec::new(), huge_meta).is_err());
    }
}
