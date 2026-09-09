use super::{CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation, ToolRegistration};
use crate::agent::protocol::PermissionRisk;
use crate::agent::tool_repair::ToolSpec;
use crate::skills::{default_skill_roots, discover_skills};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

const SKILL_TOOL_ID: &str = "agent.load_skill";
const SKILL_TOOL_NAME: &str = "load_skill";
const MAX_SKILL_NAME_LENGTH: usize = 64;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadSkillRequest {
    name: String,
}

pub struct SkillTool {
    registration: ToolRegistration,
}

impl SkillTool {
    pub fn new() -> Self {
        Self {
            registration: ToolRegistration::new(
                ToolSpec {
                    id: SKILL_TOOL_ID.to_string(),
                    name: SKILL_TOOL_NAME.to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {"name": {"type": "string", "minLength": 1, "maxLength": MAX_SKILL_NAME_LENGTH}},
                        "required": ["name"],
                        "additionalProperties": false
                    }),
                    risk: PermissionRisk::Automatic,
                },
                true,
                Arc::new(LoadSkillHandler),
            ),
        }
    }
}

impl Default for SkillTool {
    fn default() -> Self { Self::new() }
}

impl ToolExecutor for SkillTool {
    fn registrations(&self) -> &[ToolRegistration] {
        std::slice::from_ref(&self.registration)
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        self.registration.handler.execute(invocation.arguments, cancellation)
    }
}

struct LoadSkillHandler;

impl ToolHandler for LoadSkillHandler {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ToolExecutionError::cancelled());
            }
            let request = serde_json::from_value::<LoadSkillRequest>(arguments)
                .map_err(|error| ToolExecutionError::new("invalid_skill", error.to_string()))?;
            let name = request.name.trim();
            if !valid_skill_name(name) {
                return Err(ToolExecutionError::new("invalid_skill", "skill name is invalid"));
            }
            let report = discover_skills(&default_skill_roots(), env!("CARGO_PKG_VERSION"));
            let skill = report.skills.into_iter().find(|skill| skill.name == name).ok_or_else(|| {
                ToolExecutionError::new("skill_not_found", "requested skill is not available")
            })?;
            Ok(json!({"name": skill.name, "version": skill.version, "content": skill.body}))
        })
    }
}

fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_SKILL_NAME_LENGTH
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_names_cannot_escape_the_catalog() {
        assert!(valid_skill_name("steel-review"));
        assert!(!valid_skill_name("../outside"));
        assert!(!valid_skill_name(""));
    }
}
