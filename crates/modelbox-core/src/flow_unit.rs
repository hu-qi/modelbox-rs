use std::collections::HashMap;
use std::sync::Arc;

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
            "core.input" => &NONE,
            "text.concat" => &CONCAT_TEXT,
            _ => &INPUT_TEXT,
        }
    }

    fn outputs(&self) -> &'static [PortDef] {
        &OUTPUT_TEXT
    }

    fn is_source(&self) -> bool {
        self.name == "core.input"
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
            "core.input" => {
                return Err(FlowError::Execution(
                    "core.input must be driven by Runtime".to_owned(),
                ));
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
