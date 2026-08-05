use crate::permissions::{ParameterScope, PermissionAction, PermissionRule, RuleEffect};
use crate::tools::{ToolId, ToolSource, ToolVersion};
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub fn insert(
    connection: &mut Connection,
    workspace_id: &str,
    rule: &PermissionRule,
) -> Result<(), String> {
    validate_workspace(workspace_id)?;
    let source_json = serde_json::to_string(&rule.source).map_err(|error| error.to_string())?;
    let scope_json = serde_json::to_string(&rule.scope).map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO permission_rules
             (id, workspace_id, tool_id, tool_version, source_json, action, scope_json, effect,
              created_at, revoked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
            params![
                rule.id.to_string(),
                workspace_id,
                rule.tool_id.as_str(),
                rule.tool_version.to_string(),
                source_json,
                action_name(rule.action),
                scope_json,
                effect_name(&rule.effect),
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list(connection: &Connection, workspace_id: &str) -> Result<Vec<PermissionRule>, String> {
    validate_workspace(workspace_id)?;
    let mut statement = connection
        .prepare(
            "SELECT id, tool_id, tool_version, source_json, action, scope_json, effect
             FROM permission_rules
             WHERE workspace_id = ?1 AND revoked_at IS NULL
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], row_to_rule)
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub fn revoke(connection: &mut Connection, workspace_id: &str, id: Uuid) -> Result<(), String> {
    validate_workspace(workspace_id)?;
    let changed = connection
        .execute(
            "UPDATE permission_rules
             SET revoked_at = ?1
             WHERE workspace_id = ?2 AND id = ?3 AND revoked_at IS NULL",
            params![Utc::now().to_rfc3339(), workspace_id, id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("permission rule not found or already revoked".to_string());
    }
    Ok(())
}

fn row_to_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<PermissionRule> {
    let id = parse_uuid(row.get::<_, String>(0)?, "id")?;
    let tool_id = ToolId::new(row.get::<_, String>(1)?).map_err(to_sql_error)?;
    let tool_version = ToolVersion::parse(&row.get::<_, String>(2)?).map_err(to_sql_error)?;
    let source =
        serde_json::from_str::<ToolSource>(&row.get::<_, String>(3)?).map_err(to_sql_error)?;
    let action = parse_action(&row.get::<_, String>(4)?)?;
    let scope =
        serde_json::from_str::<ParameterScope>(&row.get::<_, String>(5)?).map_err(to_sql_error)?;
    let effect = parse_effect(&row.get::<_, String>(6)?)?;
    Ok(PermissionRule {
        id,
        tool_id,
        tool_version,
        source,
        action,
        scope,
        effect,
    })
}

fn validate_workspace(workspace_id: &str) -> Result<(), String> {
    if workspace_id.trim().is_empty() || workspace_id.trim() != workspace_id {
        return Err("workspace id must be a non-empty trimmed value".to_string());
    }
    Ok(())
}

fn action_name(action: PermissionAction) -> &'static str {
    match action {
        PermissionAction::Execute => "execute",
    }
}

fn effect_name(effect: &RuleEffect) -> &'static str {
    match effect {
        RuleEffect::Allow => "allow",
        RuleEffect::Deny => "deny",
    }
}

fn parse_action(value: &str) -> rusqlite::Result<PermissionAction> {
    match value {
        "execute" => Ok(PermissionAction::Execute),
        other => Err(to_sql_error(format!(
            "unsupported permission action: {other}"
        ))),
    }
}

fn parse_effect(value: &str) -> rusqlite::Result<RuleEffect> {
    match value {
        "allow" => Ok(RuleEffect::Allow),
        "deny" => Ok(RuleEffect::Deny),
        other => Err(to_sql_error(format!(
            "unsupported permission effect: {other}"
        ))),
    }
}

fn parse_uuid(value: String, field: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| to_sql_error(format!("invalid {field}: {error}")))
}

fn to_sql_error(error: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}
