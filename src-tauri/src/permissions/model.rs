use crate::agent::protocol::PermissionRisk;
use crate::tools::{ToolDefinition, ToolId, ToolSource, ToolVersion};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fmt};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Execute,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ParameterScope {
    Any,
    Exact(Value),
    Fields(BTreeMap<String, Value>),
}

impl ParameterScope {
    pub fn any() -> Self {
        Self::Any
    }

    pub fn exact(arguments: Value) -> Self {
        Self::Exact(arguments)
    }

    pub fn fields(arguments: Value) -> Result<Self, ScopeError> {
        let Some(object) = arguments.as_object() else {
            return Err(ScopeError::ExpectedObject);
        };
        Ok(Self::Fields(
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ))
    }

    pub fn matches(&self, arguments: &Value) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(expected) => expected == arguments,
            Self::Fields(fields) => {
                let Some(arguments) = arguments.as_object() else {
                    return false;
                };
                fields
                    .iter()
                    .all(|(key, expected)| arguments.get(key) == Some(expected))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeError {
    ExpectedObject,
}

impl fmt::Display for ScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("permission parameter scope must be a JSON object")
    }
}

impl std::error::Error for ScopeError {}

#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRequest {
    pub permission_id: Uuid,
    pub tool_id: ToolId,
    pub tool_version: ToolVersion,
    pub source: ToolSource,
    pub action: PermissionAction,
    pub risk: PermissionRisk,
    pub read_only: bool,
    pub arguments: Value,
}

impl PermissionRequest {
    pub fn from_tool(definition: &ToolDefinition, arguments: Value) -> Self {
        Self {
            permission_id: Uuid::new_v4(),
            tool_id: definition.id.clone(),
            tool_version: definition.version,
            source: definition.source.clone(),
            action: PermissionAction::Execute,
            risk: definition.risk,
            read_only: definition.read_only,
            arguments,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuleEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRule {
    pub id: Uuid,
    pub tool_id: ToolId,
    pub tool_version: ToolVersion,
    pub source: ToolSource,
    pub action: PermissionAction,
    pub scope: ParameterScope,
    pub effect: RuleEffect,
}

impl PermissionRule {
    pub fn matches(&self, request: &PermissionRequest) -> bool {
        self.tool_id == request.tool_id
            && self.tool_version == request.tool_version
            && self.source == request.source
            && self.action == request.action
            && self.scope.matches(&request.arguments)
    }

    pub fn for_request(
        request: &PermissionRequest,
        scope: ParameterScope,
        effect: RuleEffect,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tool_id: request.tool_id.clone(),
            tool_version: request.tool_version,
            source: request.source.clone(),
            action: request.action,
            scope,
            effect,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenialReason {
    DangerousDisabled,
    ExplicitRule { rule_id: Uuid },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    AllowAutomatic,
    RequireConfirmation(PermissionRequest),
    Deny(DenialReason),
}
