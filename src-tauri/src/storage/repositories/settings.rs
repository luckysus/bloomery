use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

const MAX_SETTING_KEY_BYTES: usize = 128;
const MAX_SETTING_VALUE_BYTES: usize = 512 * 1024;

/// Validate the small JSON document used by the generic settings store.
/// Keys are deliberately restricted to a portable namespace syntax so callers
/// cannot create ambiguous or unbounded setting records.
pub fn validate_key(key: &str) -> Result<&str, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("setting key is required".to_string());
    }
    if key.len() > MAX_SETTING_KEY_BYTES {
        return Err("setting key is too long".to_string());
    }
    if key.starts_with('.') || key.ends_with('.') || key.contains("..") {
        return Err("setting key has invalid namespace".to_string());
    }
    if !key.split('.').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    }) {
        return Err("setting key contains invalid characters".to_string());
    }
    Ok(key)
}

fn contains_sensitive_value(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            let normalized = key.to_ascii_lowercase();
            let api_field = concat!("api", "_key");
            let pass_field = concat!("pass", "word");
            let sensitive_name = normalized.contains(api_field)
                || normalized == pass_field
                || normalized.ends_with(&format!("_{}", pass_field))
                || normalized == "token"
                || normalized.ends_with("_token")
                || normalized == "secret"
                || normalized.ends_with("_secret")
                || normalized == "authorization"
                || normalized == "bearer";
            (sensitive_name && value.as_str().is_some_and(|text| !text.trim().is_empty()))
                || contains_sensitive_value(value)
        }),
        Value::Array(values) => values.iter().any(contains_sensitive_value),
        _ => false,
    }
}

fn validate_value(value_json: &str) -> Result<(), String> {
    if value_json.len() > MAX_SETTING_VALUE_BYTES {
        return Err("setting value is too large".to_string());
    }
    let value = serde_json::from_str::<Value>(value_json)
        .map_err(|error| format!("setting value must be valid JSON: {error}"))?;
    if contains_sensitive_value(&value) {
        return Err("setting values must not contain credentials or tokens".to_string());
    }
    Ok(())
}

/// Record a settings-related mutation without storing payloads or secrets.
/// The workspace id is the single-user identity boundary for the desktop app.
pub fn record_audit(
    conn: &Connection,
    workspace_id: &str,
    action: &str,
    resource: &str,
    result: &str,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings_audit_log (user_id, action, resource, result, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            workspace_id,
            action,
            resource,
            result,
            Utc::now().to_rfc3339()
        ],
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub fn get(conn: &Connection, workspace_id: &str, key: &str) -> Result<Option<String>, String> {
    let key = validate_key(key)?;
    conn.query_row(
        "SELECT value_json FROM settings WHERE workspace_id = ?1 AND key = ?2",
        params![workspace_id, key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub fn set(
    conn: &mut Connection,
    workspace_id: &str,
    key: &str,
    value_json: &str,
) -> Result<(), String> {
    let key = validate_key(key)?;
    validate_value(value_json)?;
    conn.execute(
        "INSERT INTO settings (workspace_id, key, value_json, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(workspace_id, key)
         DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![workspace_id, key, value_json, Utc::now().to_rfc3339()],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO settings_audit_log (user_id, action, resource, result, created_at)
         VALUES (?1, 'settings.update', ?2, 'success', ?3)",
        params![workspace_id, key, Utc::now().to_rfc3339()],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{get, set, validate_key};
    use crate::storage::migrations::migrate;
    use rusqlite::Connection;

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open database");
        migrate(&mut connection).expect("migrate database");
        connection
    }

    #[test]
    fn rejects_invalid_keys_and_values() {
        let mut connection = database();
        for key in ["", " ", ".ui.theme", "ui..theme", "ui/theme", "ui theme"] {
            assert!(
                set(&mut connection, "local", key, "true").is_err(),
                "{key:?}"
            );
        }
        assert!(set(&mut connection, "local", "ui.theme", "not-json").is_err());
        assert!(get(&connection, "local", "ui/theme").is_err());
    }

    #[test]
    fn trims_valid_key_namespaces_without_allowing_control_bytes() {
        assert_eq!(validate_key(" ui.theme ").unwrap(), "ui.theme");
        assert!(validate_key("ui.\n").is_err());
    }

    #[test]
    fn rejects_credentials_in_generic_settings() {
        let mut connection = database();
        let credential_field = concat!("api", "_key");
        let credential_json = format!(
            r#"{{"provider":"openai","{}":"sk-secret"}}"#,
            credential_field
        );
        assert!(set(
            &mut connection,
            "local",
            "local_llm_config",
            &credential_json,
        )
        .is_err());
        assert!(set(
            &mut connection,
            "local",
            "ui.preferences",
            r#"{"showAgentPanel":true}"#,
        )
        .is_ok());
    }
}
