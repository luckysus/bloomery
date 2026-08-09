use crate::mcp::{McpServerConfig, McpTransportKind};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

pub fn list(conn: &Connection, workspace_id: &str) -> Result<Vec<McpServerConfig>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, display_name, server_id, transport, url, executable, args_json,
                    working_directory, inherited_env_json, env_names_json, timeout_ms, enabled
             FROM mcp_servers
             WHERE workspace_id = ?1
             ORDER BY display_name ASC, id ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], decode)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|value| value.map_err(|error| error.to_string()))
        .collect()
}

pub fn get(
    conn: &Connection,
    workspace_id: &str,
    id: Uuid,
) -> Result<Option<McpServerConfig>, String> {
    conn.query_row(
        "SELECT id, display_name, server_id, transport, url, executable, args_json,
                working_directory, inherited_env_json, env_names_json, timeout_ms, enabled
         FROM mcp_servers
         WHERE workspace_id = ?1 AND id = ?2",
        params![workspace_id, id.to_string()],
        decode,
    )
    .optional()
    .map_err(|error| error.to_string())?
    .map(|value| value.map_err(|error| error.to_string()))
    .transpose()
}

pub fn save(
    conn: &mut Connection,
    workspace_id: &str,
    config: &McpServerConfig,
) -> Result<(), String> {
    config.validate().map_err(|error| error.to_string())?;
    let owner = conn
        .query_row(
            "SELECT workspace_id FROM mcp_servers WHERE id = ?1",
            params![config.id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if owner.as_deref().is_some_and(|value| value != workspace_id) {
        return Err("MCP server belongs to another workspace".to_string());
    }
    let timeout_ms = config.timeout_ms().map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO mcp_servers
          (id, workspace_id, display_name, server_id, transport, url, executable, args_json,
           working_directory, inherited_env_json, env_names_json, timeout_ms, enabled,
           created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
         ON CONFLICT(id) DO UPDATE SET
           display_name = excluded.display_name,
           server_id = excluded.server_id,
           transport = excluded.transport,
           url = excluded.url,
           executable = excluded.executable,
           args_json = excluded.args_json,
           working_directory = excluded.working_directory,
           inherited_env_json = excluded.inherited_env_json,
           env_names_json = excluded.env_names_json,
           timeout_ms = excluded.timeout_ms,
           enabled = excluded.enabled,
           updated_at = excluded.updated_at",
        params![
            config.id.to_string(),
            workspace_id,
            config.display_name,
            config.server_id,
            transport_name(config.transport),
            config.url.as_deref(),
            config
                .executable
                .as_ref()
                .map(|value| value.to_string_lossy()),
            serde_json::to_string(&config.args).map_err(|error| error.to_string())?,
            config
                .working_directory
                .as_ref()
                .map(|value| value.to_string_lossy()),
            serde_json::to_string(&config.inherited_env).map_err(|error| error.to_string())?,
            serde_json::to_string(&config.env_names).map_err(|error| error.to_string())?,
            i64::try_from(timeout_ms).map_err(|_| "MCP timeout is too large".to_string())?,
            i64::from(config.enabled),
            Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete(conn: &mut Connection, workspace_id: &str, id: Uuid) -> Result<(), String> {
    let deleted = conn
        .execute(
            "DELETE FROM mcp_servers WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, id.to_string()],
        )
        .map_err(|error| error.to_string())?;
    if deleted == 0 {
        return Err("MCP server not found".to_string());
    }
    Ok(())
}

fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<McpServerConfig, String>> {
    let id = row.get::<_, String>(0)?.parse::<Uuid>().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let transport = match row.get::<_, String>(3)?.as_str() {
        "stdio" => McpTransportKind::Stdio,
        "streamable_http" => McpTransportKind::StreamableHttp,
        "sse" => McpTransportKind::Sse,
        value => return Ok(Err(format!("unsupported MCP transport: {value}"))),
    };
    let config = McpServerConfig {
        id,
        display_name: row.get(1)?,
        server_id: row.get(2)?,
        transport,
        url: row.get(4)?,
        executable: row.get::<_, Option<String>>(5)?.map(PathBuf::from),
        args: serde_json::from_str(&row.get::<_, String>(6)?)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?,
        working_directory: row.get::<_, Option<String>>(7)?.map(PathBuf::from),
        inherited_env: serde_json::from_str(&row.get::<_, String>(8)?)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?,
        env_names: serde_json::from_str(&row.get::<_, String>(9)?)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?,
        timeout: Duration::from_millis(
            u64::try_from(row.get::<_, i64>(10)?)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?,
        ),
        enabled: row.get::<_, i64>(11)? != 0,
    };
    Ok(Ok(config))
}

fn transport_name(value: McpTransportKind) -> &'static str {
    match value {
        McpTransportKind::Stdio => "stdio",
        McpTransportKind::StreamableHttp => "streamable_http",
        McpTransportKind::Sse => "sse",
    }
}
