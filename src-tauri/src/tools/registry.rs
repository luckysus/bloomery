use super::{ToolDefinition, ToolId, ToolSource};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateId { id: ToolId },
    UnknownId { id: ToolId },
    InvalidDefinition { field: String, message: String },
    InvalidSchema { field: String, message: String },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId { id } => write!(formatter, "tool id is already registered: {id}"),
            Self::UnknownId { id } => write!(formatter, "tool id is not registered: {id}"),
            Self::InvalidDefinition { field, message } => {
                write!(formatter, "invalid tool {field}: {message}")
            }
            Self::InvalidSchema { field, message } => {
                write!(formatter, "invalid tool {field}: {message}")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    definitions: BTreeMap<ToolId, ToolDefinition>,
    enabled: BTreeSet<ToolId>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, definition: ToolDefinition) -> Result<(), RegistryError> {
        validate_definition(&definition)?;
        if self.definitions.contains_key(&definition.id) {
            return Err(RegistryError::DuplicateId { id: definition.id });
        }
        let id = definition.id.clone();
        self.definitions.insert(id.clone(), definition);
        self.enabled.insert(id);
        Ok(())
    }

    pub fn set_enabled(&mut self, id: &ToolId, enabled: bool) -> Result<(), RegistryError> {
        if !self.definitions.contains_key(id) {
            return Err(RegistryError::UnknownId { id: id.clone() });
        }
        if enabled {
            self.enabled.insert(id.clone());
        } else {
            self.enabled.remove(id);
        }
        Ok(())
    }

    pub fn is_enabled(&self, id: &ToolId) -> bool {
        self.enabled.contains(id)
    }

    pub fn snapshot(&self) -> ToolSnapshot {
        self.snapshot_for_domain(None)
    }

    pub fn snapshot_for_domain(&self, domain: Option<&str>) -> ToolSnapshot {
        let tools = self
            .definitions
            .iter()
            .filter(|(id, definition)| {
                self.enabled.contains(*id) && applies_to_domain(definition, domain)
            })
            .map(|(_, definition)| definition.clone())
            .collect();
        ToolSnapshot { tools }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSnapshot {
    pub tools: Vec<ToolDefinition>,
}

impl ToolSnapshot {
    pub fn iter(&self) -> impl Iterator<Item = &ToolDefinition> {
        self.tools.iter()
    }
}

fn applies_to_domain(definition: &ToolDefinition, domain: Option<&str>) -> bool {
    match domain {
        Some(domain) => definition.domains.is_empty() || definition.domains.contains(domain),
        None => definition.domains.is_empty(),
    }
}

fn validate_definition(definition: &ToolDefinition) -> Result<(), RegistryError> {
    if definition.name.trim().is_empty() {
        return Err(RegistryError::InvalidDefinition {
            field: "name".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    if definition.description.trim().is_empty() {
        return Err(RegistryError::InvalidDefinition {
            field: "description".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    if definition.timeout.is_zero() {
        return Err(RegistryError::InvalidDefinition {
            field: "timeout".to_string(),
            message: "must be greater than zero".to_string(),
        });
    }
    validate_schema(&definition.input_schema).map_err(|message| RegistryError::InvalidSchema {
        field: "input_schema".to_string(),
        message,
    })?;
    validate_schema(&definition.output_schema).map_err(|message| RegistryError::InvalidSchema {
        field: "output_schema".to_string(),
        message,
    })?;
    for domain in &definition.domains {
        if domain.trim().is_empty() {
            return Err(RegistryError::InvalidDefinition {
                field: "domains".to_string(),
                message: "domain identifiers must not be empty".to_string(),
            });
        }
    }
    if let ToolSource::Domain { package_id, .. } = &definition.source {
        if package_id.trim().is_empty() {
            return Err(RegistryError::InvalidDefinition {
                field: "source".to_string(),
                message: "domain package id must not be empty".to_string(),
            });
        }
    }
    Ok(())
}

fn validate_schema(schema: &Value) -> Result<(), String> {
    let Some(object) = schema.as_object() else {
        return Err("schema must be a JSON object".to_string());
    };
    if let Some(type_value) = object.get("type") {
        validate_type(type_value)?;
    }
    if let Some(properties) = object.get("properties") {
        let Some(properties) = properties.as_object() else {
            return Err("properties must be an object".to_string());
        };
        for property in properties.values() {
            validate_schema(property)?;
        }
    }
    if let Some(required) = object.get("required") {
        let Some(required) = required.as_array() else {
            return Err("required must be an array".to_string());
        };
        let mut seen = BTreeSet::new();
        for field in required {
            let Some(field) = field.as_str() else {
                return Err("required entries must be strings".to_string());
            };
            if !seen.insert(field) {
                return Err(format!("required contains duplicate field {field}"));
            }
        }
    }
    if let Some(additional) = object.get("additionalProperties") {
        if !additional.is_boolean() {
            validate_schema(additional)?;
        }
    }
    if let Some(items) = object.get("items") {
        if !items.is_boolean() {
            validate_schema(items)?;
        }
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(alternatives) = object.get(keyword) {
            let Some(alternatives) = alternatives.as_array() else {
                return Err(format!("{keyword} must be an array"));
            };
            if alternatives.is_empty() {
                return Err(format!("{keyword} must not be empty"));
            }
            for alternative in alternatives {
                validate_schema(alternative)?;
            }
        }
    }
    if let Some(enum_values) = object.get("enum") {
        if !enum_values.is_array() {
            return Err("enum must be an array".to_string());
        }
    }
    Ok(())
}

fn validate_type(type_value: &Value) -> Result<(), String> {
    const TYPES: &[&str] = &[
        "null", "boolean", "object", "array", "number", "integer", "string",
    ];
    let types = match type_value {
        Value::String(value) => vec![value.as_str()],
        Value::Array(values) if !values.is_empty() => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| "type array entries must be strings".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?,
        Value::Array(_) => return Err("type array must not be empty".to_string()),
        _ => return Err("type must be a string or array of strings".to_string()),
    };
    if types.iter().any(|value| !TYPES.contains(value)) {
        return Err("type contains an unsupported JSON type".to_string());
    }
    Ok(())
}
