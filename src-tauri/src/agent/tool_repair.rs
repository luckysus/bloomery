use super::protocol::PermissionRisk;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{fmt, str};

pub const MAX_REPAIR_RETRIES: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub id: String,
    pub name: String,
    pub input_schema: Value,
    pub risk: PermissionRisk,
}

impl ToolSpec {
    /// Built-in and MCP tools share one typed-schema contract: every input
    /// schema must be a JSON Schema object declaration so repair, permission
    /// summaries, and model adapters can rely on it.
    pub fn validate_schema(&self) -> Result<(), String> {
        let schema = self
            .input_schema
            .as_object()
            .ok_or_else(|| format!("tool {} input schema must be an object", self.id))?;
        if schema.get("type").and_then(Value::as_str) != Some("object") {
            return Err(format!(
                "tool {} input schema must declare \"type\": \"object\"",
                self.id
            ));
        }
        if let Some(properties) = schema.get("properties") {
            if !properties.is_object() {
                return Err(format!(
                    "tool {} input schema properties must be an object",
                    self.id
                ));
            }
        }
        if let Some(required) = schema.get("required") {
            let required = required.as_array().ok_or_else(|| {
                format!("tool {} input schema required must be an array", self.id)
            })?;
            if !required.iter().all(Value::is_string) {
                return Err(format!(
                    "tool {} input schema required must list property names",
                    self.id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxRepair {
    RemovedJsonFence,
    RemovedTrailingCommas,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RepairedToolCall {
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub risk: PermissionRisk,
    pub syntax_repairs: Vec<SyntaxRepair>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairFeedback {
    pub attempt: usize,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolRepairError {
    InvalidJson {
        message: String,
    },
    InvalidEnvelope {
        message: String,
    },
    MissingToolName,
    MissingArguments,
    InvalidArguments {
        message: String,
    },
    UnknownTool {
        name: String,
    },
    AmbiguousTool {
        name: String,
    },
    AmbiguousArguments {
        message: String,
        path: Option<String>,
    },
    InvalidSchema {
        message: String,
    },
    RequiredArgumentMissing {
        path: String,
    },
    TypeMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    UnknownArgument {
        path: String,
    },
    ConstraintViolation {
        path: String,
        message: String,
    },
    RetryExhausted {
        attempts: usize,
        last: Box<ToolRepairError>,
    },
}

impl ToolRepairError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidJson { .. } => "invalid_json",
            Self::InvalidEnvelope { .. } => "invalid_tool_call_envelope",
            Self::MissingToolName => "missing_tool_name",
            Self::MissingArguments => "missing_arguments",
            Self::InvalidArguments { .. } => "invalid_arguments",
            Self::UnknownTool { .. } => "unknown_tool",
            Self::AmbiguousTool { .. } => "ambiguous_tool",
            Self::AmbiguousArguments { .. } => "ambiguous_arguments",
            Self::InvalidSchema { .. } => "schema_invalid",
            Self::RequiredArgumentMissing { .. } => "schema_required_missing",
            Self::TypeMismatch { .. } => "schema_type_mismatch",
            Self::UnknownArgument { .. } => "schema_unknown_argument",
            Self::ConstraintViolation { .. } => "schema_constraint_failed",
            Self::RetryExhausted { .. } => "repair_retry_exhausted",
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            Self::AmbiguousArguments { path, .. } => path.as_deref(),
            Self::RequiredArgumentMissing { path }
            | Self::TypeMismatch { path, .. }
            | Self::UnknownArgument { path }
            | Self::ConstraintViolation { path, .. } => Some(path),
            Self::RetryExhausted { last, .. } => last.path(),
            _ => None,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::InvalidJson { message } => format!("tool call is not valid JSON: {message}"),
            Self::InvalidEnvelope { message } => message.clone(),
            Self::MissingToolName => "tool call must contain an explicit tool name".to_string(),
            Self::MissingArguments => "tool call must contain explicit arguments".to_string(),
            Self::InvalidArguments { message } => message.clone(),
            Self::UnknownTool { name } => format!("tool is not registered: {name}"),
            Self::AmbiguousTool { name } => format!("tool name resolves to multiple tools: {name}"),
            Self::AmbiguousArguments { message, .. } => message.clone(),
            Self::InvalidSchema { message } => format!("tool input schema is invalid: {message}"),
            Self::RequiredArgumentMissing { path } => {
                format!("required argument {path} is missing")
            }
            Self::TypeMismatch {
                path,
                expected,
                actual,
            } => format!("argument {path} expected {expected}, got {actual}"),
            Self::UnknownArgument { path } => {
                format!("argument {path} is not allowed by the schema")
            }
            Self::ConstraintViolation { path, message } => {
                format!("argument {path} {message}")
            }
            Self::RetryExhausted { attempts, last } => {
                format!("tool call repair failed after {attempts} attempts: {last}")
            }
        }
    }
}

impl fmt::Display for ToolRepairError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.message())
    }
}

impl std::error::Error for ToolRepairError {}

pub fn repair_tool_call(
    raw: &str,
    tools: &[ToolSpec],
) -> Result<RepairedToolCall, ToolRepairError> {
    let (envelope, mut syntax_repairs) = parse_json_document(raw)?;
    let (tool_name, raw_arguments) = extract_call_parts(envelope)?;
    let matching = tools
        .iter()
        .filter(|tool| tool.name == tool_name)
        .collect::<Vec<_>>();
    let tool = match matching.as_slice() {
        [] => return Err(ToolRepairError::UnknownTool { name: tool_name }),
        [tool] => *tool,
        _ => return Err(ToolRepairError::AmbiguousTool { name: tool_name }),
    };
    let arguments = parse_arguments(raw_arguments, &mut syntax_repairs)?;
    reject_parameter_aliases(&arguments, &tool.input_schema)?;
    validate_schema(&arguments, &tool.input_schema, "$")?;

    Ok(RepairedToolCall {
        tool_id: tool.id.clone(),
        tool_name: tool.name.clone(),
        arguments,
        risk: tool.risk,
        syntax_repairs,
    })
}

pub fn repair_tool_call_with_retries<F>(
    initial: &str,
    tools: &[ToolSpec],
    mut retry: F,
) -> Result<RepairedToolCall, ToolRepairError>
where
    F: FnMut(&RepairFeedback) -> Option<String>,
{
    let mut raw = initial.to_string();
    let mut retries = 0usize;
    loop {
        match repair_tool_call(&raw, tools) {
            Ok(repaired) => return Ok(repaired),
            Err(error) if retries >= MAX_REPAIR_RETRIES => {
                return Err(ToolRepairError::RetryExhausted {
                    attempts: retries + 1,
                    last: Box::new(error),
                })
            }
            Err(error) => {
                retries += 1;
                let feedback = RepairFeedback {
                    attempt: retries,
                    code: error.code().to_string(),
                    message: error.to_string(),
                    path: error.path().map(str::to_string),
                };
                let Some(next) = retry(&feedback) else {
                    return Err(error);
                };
                raw = next;
            }
        }
    }
}

fn parse_json_document(raw: &str) -> Result<(Value, Vec<SyntaxRepair>), ToolRepairError> {
    let (body, fenced) = strip_json_fence(raw)?;
    let (normalized, trailing_commas) = remove_trailing_commas(body);
    let value = serde_json::from_str::<Value>(&normalized).map_err(|error| {
        ToolRepairError::InvalidJson {
            message: error.to_string(),
        }
    })?;
    let mut repairs = Vec::new();
    if fenced {
        repairs.push(SyntaxRepair::RemovedJsonFence);
    }
    if trailing_commas {
        repairs.push(SyntaxRepair::RemovedTrailingCommas);
    }
    Ok((value, repairs))
}

fn strip_json_fence(raw: &str) -> Result<(&str, bool), ToolRepairError> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return Ok((trimmed, false));
    }
    let Some(first_newline) = trimmed.find('\n') else {
        return Err(ToolRepairError::InvalidJson {
            message: "unterminated JSON code fence".to_string(),
        });
    };
    let language = trimmed[3..first_newline].trim();
    if !language.is_empty() && !language.eq_ignore_ascii_case("json") {
        return Err(ToolRepairError::InvalidJson {
            message: format!("unsupported code fence language: {language}"),
        });
    }
    let body_start = first_newline + 1;
    let body = &trimmed[body_start..];
    let Some(closing) = body.rfind("```") else {
        return Err(ToolRepairError::InvalidJson {
            message: "unterminated JSON code fence".to_string(),
        });
    };
    if !body[closing + 3..].trim().is_empty() {
        return Err(ToolRepairError::InvalidJson {
            message: "text after JSON code fence is not allowed".to_string(),
        });
    }
    Ok((body[..closing].trim(), true))
}

fn remove_trailing_commas(input: &str) -> (String, bool) {
    let mut output = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut removed = false;
    for (index, character) in input.char_indices() {
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            output.push(character);
            continue;
        }
        if character == ',' {
            let next = input[index + character.len_utf8()..]
                .chars()
                .find(|candidate| !candidate.is_whitespace());
            if matches!(next, Some('}' | ']')) {
                removed = true;
                continue;
            }
        }
        output.push(character);
    }
    (output, removed)
}

fn extract_call_parts(value: Value) -> Result<(String, Value), ToolRepairError> {
    let Some(object) = value.as_object() else {
        return Err(ToolRepairError::InvalidEnvelope {
            message: "tool call envelope must be a JSON object".to_string(),
        });
    };
    for key in [
        "permission",
        "permission_intent",
        "risk",
        "allow",
        "dangerous",
    ] {
        if object.contains_key(key) {
            return Err(ToolRepairError::AmbiguousArguments {
                message: format!("tool call cannot declare permission intent in field {key}"),
                path: Some(format!("$.{key}")),
            });
        }
    }

    let mut names = Vec::new();
    let mut arguments = Vec::new();
    if let Some(function) = object.get("function") {
        let Some(function) = function.as_object() else {
            return Err(ToolRepairError::InvalidEnvelope {
                message: "function must be a JSON object".to_string(),
            });
        };
        collect_name(function, "name", &mut names)?;
        collect_argument(function, "arguments", &mut arguments)?;
    }
    for key in ["name", "tool", "tool_name"] {
        collect_name(object, key, &mut names)?;
    }
    for key in ["arguments", "parameters", "input"] {
        collect_argument(object, key, &mut arguments)?;
    }

    let name = unique_name(names)?;
    let raw_arguments = unique_value(arguments)?.ok_or(ToolRepairError::MissingArguments)?;
    Ok((name, raw_arguments))
}

fn collect_name(
    object: &Map<String, Value>,
    key: &str,
    names: &mut Vec<String>,
) -> Result<(), ToolRepairError> {
    let Some(value) = object.get(key) else {
        return Ok(());
    };
    let Some(value) = value.as_str() else {
        return Err(ToolRepairError::InvalidEnvelope {
            message: format!("tool name field {key} must be a string"),
        });
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(ToolRepairError::MissingToolName);
    }
    names.push(value.to_string());
    Ok(())
}

fn collect_argument(
    object: &Map<String, Value>,
    key: &str,
    arguments: &mut Vec<Value>,
) -> Result<(), ToolRepairError> {
    if let Some(value) = object.get(key) {
        arguments.push(value.clone());
    }
    Ok(())
}

fn unique_name(names: Vec<String>) -> Result<String, ToolRepairError> {
    let Some(first) = names.first() else {
        return Err(ToolRepairError::MissingToolName);
    };
    if names.iter().any(|name| name != first) {
        return Err(ToolRepairError::AmbiguousArguments {
            message: "tool call contains conflicting tool names".to_string(),
            path: Some("$.name".to_string()),
        });
    }
    Ok(first.clone())
}

fn unique_value(values: Vec<Value>) -> Result<Option<Value>, ToolRepairError> {
    let Some(first) = values.first() else {
        return Ok(None);
    };
    if values.iter().any(|value| value != first) {
        return Err(ToolRepairError::AmbiguousArguments {
            message: "tool call contains conflicting argument objects".to_string(),
            path: Some("$.arguments".to_string()),
        });
    }
    Ok(Some(first.clone()))
}

fn parse_arguments(
    value: Value,
    syntax_repairs: &mut Vec<SyntaxRepair>,
) -> Result<Value, ToolRepairError> {
    match value {
        Value::Object(_) => Ok(value),
        Value::String(raw) => {
            let (value, repairs) = parse_json_document(&raw)?;
            syntax_repairs.extend(repairs);
            if value.is_object() {
                Ok(value)
            } else {
                Err(ToolRepairError::InvalidArguments {
                    message: "arguments must decode to a JSON object".to_string(),
                })
            }
        }
        _ => Err(ToolRepairError::InvalidArguments {
            message: "arguments must be a JSON object or an encoded JSON object".to_string(),
        }),
    }
}

fn reject_parameter_aliases(arguments: &Value, schema: &Value) -> Result<(), ToolRepairError> {
    let Some(arguments) = arguments.as_object() else {
        return Ok(());
    };
    let Some(schema) = schema.as_object() else {
        return Ok(());
    };
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Ok(());
    };
    if properties.contains_key("path") && arguments.contains_key("path") {
        for alias in ["file", "file_path", "filepath"] {
            if arguments.contains_key(alias) {
                return Err(ToolRepairError::AmbiguousArguments {
                    message: "path and a path alias were supplied; no path can be inferred"
                        .to_string(),
                    path: Some("$.path".to_string()),
                });
            }
        }
    }
    if properties.contains_key("command") && arguments.contains_key("command") {
        for alias in ["cmd", "shell_command"] {
            if arguments.contains_key(alias) {
                return Err(ToolRepairError::AmbiguousArguments {
                    message:
                        "command and a command alias were supplied; no command can be inferred"
                            .to_string(),
                    path: Some("$.command".to_string()),
                });
            }
        }
    }
    Ok(())
}

fn validate_schema(value: &Value, schema: &Value, path: &str) -> Result<(), ToolRepairError> {
    let Some(schema) = schema.as_object() else {
        return Err(ToolRepairError::InvalidSchema {
            message: "schema must be a JSON object".to_string(),
        });
    };
    if let Some(expected) = schema.get("type") {
        if !matches_type(value, expected)? {
            return Err(ToolRepairError::TypeMismatch {
                path: path.to_string(),
                expected: expected_type_name(expected),
                actual: value_type_name(value).to_string(),
            });
        }
    }
    if let Some(enum_values) = schema.get("enum") {
        let Some(enum_values) = enum_values.as_array() else {
            return Err(ToolRepairError::InvalidSchema {
                message: "enum must be an array".to_string(),
            });
        };
        if !enum_values.iter().any(|candidate| candidate == value) {
            return Err(ToolRepairError::ConstraintViolation {
                path: path.to_string(),
                message: "is not one of the permitted values".to_string(),
            });
        }
    }
    if let Some(object) = value.as_object() {
        validate_object(object, schema, path)?;
    }
    if let Some(array) = value.as_array() {
        validate_array(array, schema, path)?;
    }
    if let Some(string) = value.as_str() {
        validate_string(string, schema, path)?;
    }
    Ok(())
}

fn validate_object(
    value: &Map<String, Value>,
    schema: &Map<String, Value>,
    path: &str,
) -> Result<(), ToolRepairError> {
    let properties = match schema.get("properties") {
        None => Map::new(),
        Some(value) => {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| ToolRepairError::InvalidSchema {
                    message: "properties must be an object".to_string(),
                })?
        }
    };
    if let Some(required) = schema.get("required") {
        let required = required
            .as_array()
            .ok_or_else(|| ToolRepairError::InvalidSchema {
                message: "required must be an array".to_string(),
            })?;
        for field in required {
            let Some(field) = field.as_str() else {
                return Err(ToolRepairError::InvalidSchema {
                    message: "required entries must be strings".to_string(),
                });
            };
            if !value.contains_key(field) {
                return Err(ToolRepairError::RequiredArgumentMissing {
                    path: join_path(path, field),
                });
            }
        }
    }
    if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
        for field in value.keys() {
            if !properties.contains_key(field) {
                return Err(ToolRepairError::UnknownArgument {
                    path: join_path(path, field),
                });
            }
        }
    }
    for (field, field_value) in value {
        if let Some(field_schema) = properties.get(field) {
            validate_schema(field_value, field_schema, &join_path(path, field))?;
        }
    }
    Ok(())
}

fn validate_array(
    value: &[Value],
    schema: &Map<String, Value>,
    path: &str,
) -> Result<(), ToolRepairError> {
    if let Some(items) = schema.get("items") {
        for (index, item) in value.iter().enumerate() {
            validate_schema(item, items, &format!("{path}[{index}]"))?;
        }
    }
    if let Some(minimum) = schema.get("minItems").and_then(Value::as_u64) {
        if value.len() < minimum as usize {
            return Err(ToolRepairError::ConstraintViolation {
                path: path.to_string(),
                message: format!("must contain at least {minimum} items"),
            });
        }
    }
    if let Some(maximum) = schema.get("maxItems").and_then(Value::as_u64) {
        if value.len() > maximum as usize {
            return Err(ToolRepairError::ConstraintViolation {
                path: path.to_string(),
                message: format!("must contain at most {maximum} items"),
            });
        }
    }
    Ok(())
}

fn validate_string(
    value: &str,
    schema: &Map<String, Value>,
    path: &str,
) -> Result<(), ToolRepairError> {
    if let Some(minimum) = schema.get("minLength").and_then(Value::as_u64) {
        if value.chars().count() < minimum as usize {
            return Err(ToolRepairError::ConstraintViolation {
                path: path.to_string(),
                message: format!("must contain at least {minimum} characters"),
            });
        }
    }
    if let Some(maximum) = schema.get("maxLength").and_then(Value::as_u64) {
        if value.chars().count() > maximum as usize {
            return Err(ToolRepairError::ConstraintViolation {
                path: path.to_string(),
                message: format!("must contain at most {maximum} characters"),
            });
        }
    }
    Ok(())
}

fn matches_type(value: &Value, expected: &Value) -> Result<bool, ToolRepairError> {
    match expected {
        Value::String(expected) => match expected.as_str() {
            "null" => Ok(value.is_null()),
            "boolean" => Ok(value.is_boolean()),
            "number" => Ok(value.is_number()),
            "integer" => Ok(value
                .as_number()
                .is_some_and(|number| number.is_i64() || number.is_u64())),
            "string" => Ok(value.is_string()),
            "array" => Ok(value.is_array()),
            "object" => Ok(value.is_object()),
            _ => Err(ToolRepairError::InvalidSchema {
                message: format!("unsupported JSON Schema type: {expected}"),
            }),
        },
        Value::Array(expected) => {
            if expected.is_empty() {
                return Err(ToolRepairError::InvalidSchema {
                    message: "type array cannot be empty".to_string(),
                });
            }
            expected.iter().try_fold(false, |matched, item| {
                Ok(matched || matches_type(value, item)?)
            })
        }
        _ => Err(ToolRepairError::InvalidSchema {
            message: "type must be a string or an array".to_string(),
        }),
    }
}

fn expected_type_name(expected: &Value) -> String {
    match expected {
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" or "),
        _ => "valid JSON type".to_string(),
    }
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn join_path(parent: &str, field: &str) -> String {
    if parent == "$" {
        format!("$.{field}")
    } else {
        format!("{parent}.{field}")
    }
}
