use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::{DataType, FlowError, Result, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortDef {
    pub name: &'static str,
    pub data_type: DataType,
}

impl PortDef {
    pub const fn new(name: &'static str, data_type: DataType) -> Self {
        Self { name, data_type }
    }
}

/// Custom units are registered by name and executed on dedicated workers.
/// The trait is a Rust API, NOT a stable dynamically loadable plugin ABI.
pub trait FlowUnit: Send + Sync {
    fn name(&self) -> &'static str;
    fn inputs(&self) -> &'static [PortDef];
    fn outputs(&self) -> &'static [PortDef];
    fn process(&self, inputs: &[Value], config: &JsonValue) -> Result<Vec<Value>>;

    /// Consume input values where the implementation can reuse their buffers.
    ///
    /// Existing custom FlowUnits need not change: by default this delegates
    /// to the borrowed `process` method. Units that forward tensor/byte/text
    /// values may override it to avoid copying large buffers.
    fn process_owned(&self, inputs: Vec<Value>, config: &JsonValue) -> Result<Vec<Value>> {
        self.process(&inputs, config)
    }

    fn validate_config(&self, _config: &JsonValue) -> Result<()> {
        Ok(())
    }
    fn is_source(&self) -> bool {
        false
    }
}

pub struct FlowRegistry {
    units: HashMap<String, Arc<dyn FlowUnit>>,
}

impl Default for FlowRegistry {
    fn default() -> Self {
        let mut registry = Self {
            units: HashMap::new(),
        };
        for name in [
            "core.input",
            "core.output",
            "tensor.input",
            "tensor.output",
            "text.uppercase",
            "text.prefix",
            "text.concat",
        ] {
            registry
                .register(Arc::new(Builtin { name }))
                .expect("built-in names must be unique");
        }
        registry
    }
}

impl FlowRegistry {
    pub fn empty() -> Self {
        Self {
            units: HashMap::new(),
        }
    }

    pub fn register(&mut self, unit: Arc<dyn FlowUnit>) -> Result<()> {
        let name = unit.name();
        if name.is_empty() || self.units.contains_key(name) {
            return Err(FlowError::InvalidGraph(format!(
                "empty or duplicate flow unit name: {name}"
            )));
        }
        self.units.insert(name.to_owned(), unit);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<Arc<dyn FlowUnit>> {
        self.units
            .get(name)
            .cloned()
            .ok_or_else(|| FlowError::UnknownUnit(name.to_owned()))
    }
}

const NONE: [PortDef; 0] = [];
const INPUT_TEXT: [PortDef; 1] = [PortDef::new("in", DataType::Text)];
const OUTPUT_TEXT: [PortDef; 1] = [PortDef::new("out", DataType::Text)];
const INPUT_TENSOR: [PortDef; 1] = [PortDef::new("in", DataType::TensorF32)];
const OUTPUT_TENSOR: [PortDef; 1] = [PortDef::new("out", DataType::TensorF32)];
const CONCAT_TEXT: [PortDef; 2] = [
    PortDef::new("left", DataType::Text),
    PortDef::new("right", DataType::Text),
];

struct Builtin {
    name: &'static str,
}

impl FlowUnit for Builtin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn inputs(&self) -> &'static [PortDef] {
        match self.name {
            "core.input" | "tensor.input" => &NONE,
            "tensor.output" => &INPUT_TENSOR,
            "text.concat" => &CONCAT_TEXT,
            _ => &INPUT_TEXT,
        }
    }

    fn outputs(&self) -> &'static [PortDef] {
        match self.name {
            "tensor.input" | "tensor.output" => &OUTPUT_TENSOR,
            _ => &OUTPUT_TEXT,
        }
    }

    fn is_source(&self) -> bool {
        self.name == "core.input" || self.name == "tensor.input"
    }

    fn validate_config(&self, config: &JsonValue) -> Result<()> {
        if self.name == "text.prefix" && config.get("prefix").and_then(JsonValue::as_str).is_none()
        {
            return Err(FlowError::InvalidGraph(
                "text.prefix requires a string config.prefix".to_owned(),
            ));
        }
        Ok(())
    }

    fn process_owned(&self, inputs: Vec<Value>, config: &JsonValue) -> Result<Vec<Value>> {
        match self.name {
            "tensor.output" => match inputs.as_slice() {
                [Value::TensorF32(_)] => Ok(inputs),
                _ => Err(FlowError::Execution(
                    "tensor.output expects tensor_f32".to_owned(),
                )),
            },
            "core.output" => match inputs.as_slice() {
                [Value::Text(_)] => Ok(inputs),
                _ => Err(FlowError::Execution(
                    "core.output requires a text input at position 0".to_owned(),
                )),
            },
            _ => self.process(&inputs, config),
        }
    }

    fn process(&self, inputs: &[Value], config: &JsonValue) -> Result<Vec<Value>> {
        let read_text = |index: usize| -> Result<&str> {
            match inputs.get(index) {
                Some(Value::Text(text)) => Ok(text),
                _ => Err(FlowError::Execution(format!(
                    "{} requires a text input at position {index}",
                    self.name
                ))),
            }
        };
        let value = match self.name {
            "core.input" | "tensor.input" => {
                return Err(FlowError::Execution(
                    "core.input must be driven by Runtime".to_owned(),
                ));
            }
            "tensor.output" => {
                return match inputs.first() {
                    Some(Value::TensorF32(tensor)) => Ok(vec![Value::TensorF32(tensor.clone())]),
                    _ => Err(FlowError::Execution(
                        "tensor.output expects tensor_f32".to_owned(),
                    )),
                };
            }
            "core.output" => read_text(0)?.to_owned(),
            "text.uppercase" => read_text(0)?.to_uppercase(),
            "text.prefix" => {
                let prefix = config
                    .get("prefix")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| FlowError::Execution("missing config.prefix".to_owned()))?;
                format!("{prefix}{}", read_text(0)?)
            }
            "text.concat" => format!("{}{}", read_text(0)?, read_text(1)?),
            _ => {
                return Err(FlowError::UnknownUnit(self.name.to_owned()));
            }
        };
        Ok(vec![Value::Text(value)])
    }
}

 
/// Read-only discovery data for Studio; runtime compilation remains authoritative.
/// A missing config schema does NOT mean that arbitrary configs are accepted.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct FlowUnitDescriptor {
    pub name: String,
    pub is_source: bool,
    pub inputs: Vec<PortDescriptor>,
    pub outputs: Vec<PortDescriptor>,
    pub required_string_config_keys: Vec<String>,
    pub config_schema_complete: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PortDescriptor {
    pub name: String,
    pub data_type: DataType,
}

impl FlowRegistry {
    /// Stable ordering, based on the actual registered implementations.
    /// Custom units have no introspectable config schema in today's FlowUnit trait.
    pub fn descriptors(&self) -> Vec<FlowUnitDescriptor> {
        let mut descriptors: Vec<_> = self.units.values().map(|unit| {
            let ports = |defs: &[PortDef]| defs.iter().map(|p| PortDescriptor {
                name: p.name.to_owned(),
                data_type: p.data_type,
            }).collect();
            FlowUnitDescriptor {
                name: unit.name().to_owned(),
                is_source: unit.is_source(),
                inputs: ports(unit.inputs()),
                outputs: ports(unit.outputs()),
                required_string_config_keys: if unit.name() == "text.prefix" {
                    vec!["prefix".to_owned()]
                } else {
                    Vec::new()
                },
                // No FlowUnit can currently promise an exhaustive config contract.
                config_schema_complete: false,
            }
        }).collect();
        descriptors.sort_by(|a, b| a.name.cmp(&b.name));
        descriptors
    }
}

#[cfg(test)]
mod studio_catalog_tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_sorted_and_matches_registered_ports() {
        let registry = FlowRegistry::default();
        let list = registry.descriptors();
        assert_eq!(list.len(), 7);
        assert!(list.windows(2).all(|pair| pair[0].name < pair[1].name));
        let prefix = list.iter().find(|d| d.name == "text.prefix").unwrap();
        assert_eq!(prefix.inputs[0].data_type, DataType::Text);
        assert_eq!(prefix.required_string_config_keys, ["prefix"]);
        assert!(!prefix.config_schema_complete);
        let tensor = list.iter().find(|d| d.name == "tensor.input").unwrap();
        assert!(tensor.is_source);
        assert_eq!(tensor.outputs[0].data_type, DataType::TensorF32);
        let encoded = serde_json::to_value(&list).unwrap();
        assert_eq!(encoded.iter().find(|x| x["name"] == "tensor.input").unwrap()["outputs"][0]["data_type"], "tensor_f32");
    }

    #[test]
    fn empty_catalog_is_empty() {
        assert!(FlowRegistry::empty().descriptors().is_empty());
    }
}
