use super::{
    CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation,
    ToolRegistration,
};
use crate::agent::protocol::PermissionRisk;
use crate::agent::tool_repair::ToolSpec;
use crate::skills::{
    default_skill_roots, discover_skills, load_enabled_names, SkillRecord, SkillRoot,
};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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
    /// No workspace authorization means no skills may be loaded.
    pub fn new() -> Self {
        Self::from_snapshot(BTreeMap::new(), None, String::new())
    }

    pub fn from_connection(connection: &Connection, workspace_id: &str) -> Result<Self, String> {
        Self::for_workspace(connection, workspace_id, &default_skill_roots())
    }

    pub fn for_workspace(
        connection: &Connection,
        workspace_id: &str,
        roots: &[SkillRoot],
    ) -> Result<Self, String> {
        let database = connection
            .path()
            .filter(|path| !path.is_empty())
            .ok_or("skill authorization requires a persistent database")?;
        let database = std::fs::canonicalize(database).map_err(|error| error.to_string())?;
        let enabled = load_enabled_names(connection, workspace_id)?;
        let skills = discover_skills(roots, env!("CARGO_PKG_VERSION"))
            .skills
            .into_iter()
            .filter(|skill| enabled.contains(&skill.name))
            .map(|skill| (skill.name.clone(), skill))
            .collect();
        Ok(Self::from_snapshot(
            skills,
            Some(database),
            workspace_id.to_string(),
        ))
    }

    fn from_snapshot(
        skills: BTreeMap<String, SkillRecord>,
        database: Option<PathBuf>,
        workspace_id: String,
    ) -> Self {
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
                Arc::new(LoadSkillHandler {
                    skills: Arc::new(skills),
                    database,
                    workspace_id,
                }),
            ),
        }
    }
}

impl Default for SkillTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolExecutor for SkillTool {
    fn registrations(&self) -> &[ToolRegistration] {
        std::slice::from_ref(&self.registration)
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        if invocation.tool_id != SKILL_TOOL_ID || invocation.tool_name != SKILL_TOOL_NAME {
            return Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_not_registered",
                    "skill tool is not registered",
                ))
            });
        }
        self.registration
            .handler
            .execute(invocation.arguments, cancellation)
    }
}

struct LoadSkillHandler {
    skills: Arc<BTreeMap<String, SkillRecord>>,
    database: Option<PathBuf>,
    workspace_id: String,
}

impl ToolHandler for LoadSkillHandler {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        let skills = self.skills.clone();
        let database = self.database.clone();
        let workspace_id = self.workspace_id.clone();
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ToolExecutionError::cancelled());
            }
            let request = serde_json::from_value::<LoadSkillRequest>(arguments)
                .map_err(|error| ToolExecutionError::new("invalid_skill", error.to_string()))?;
            let name = request.name.trim();
            if !valid_skill_name(name) {
                return Err(ToolExecutionError::new(
                    "invalid_skill",
                    "skill name is invalid",
                ));
            }
            let skill = skills.get(name).ok_or_else(|| {
                ToolExecutionError::new(
                    "skill_not_enabled",
                    "requested skill is not enabled in this workspace snapshot",
                )
            })?;
            let database = database.ok_or_else(|| {
                ToolExecutionError::new("skill_not_enabled", "skill authorization is unavailable")
            })?;
            let connection =
                Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(
                    |error| {
                        ToolExecutionError::new("skill_authorization_failed", error.to_string())
                    },
                )?;
            connection
                .busy_timeout(Duration::from_millis(100))
                .map_err(|error| {
                    ToolExecutionError::new("skill_authorization_failed", error.to_string())
                })?;
            let enabled = load_enabled_names(&connection, &workspace_id)
                .map_err(|error| ToolExecutionError::new("skill_authorization_failed", error))?;
            if !enabled.contains(name) {
                return Err(ToolExecutionError::new(
                    "skill_not_enabled",
                    "requested skill is no longer enabled",
                ));
            }
            if cancellation.is_cancelled() {
                return Err(ToolExecutionError::cancelled());
            }
            Ok(
                json!({"name": skill.name, "version": skill.version, "content_sha256": skill.content_sha256, "content": skill.body}),
            )
        })
    }
}

fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_SKILL_NAME_LENGTH
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
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
