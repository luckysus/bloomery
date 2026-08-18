use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DatabaseConnectionInput {
    pub id: Option<String>,
    pub display_name: String,
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub password: Option<String>,
    pub timeout_ms: Option<u64>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DatabaseConnectionSummary {
    pub id: String,
    pub display_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub timeout_ms: u64,
    pub enabled: bool,
    pub secret_configured: bool,
    pub last_checked_at: Option<String>,
    pub last_latency_ms: Option<i64>,
    pub last_version: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DatabaseQuerySubmitInput {
    pub connection_id: String,
    pub database: Option<String>,
    pub sql: String,
    pub row_limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DatabaseQueryResultResponse {
    pub task_id: String,
    pub connection_id: String,
    pub database_name: String,
    pub query_text: String,
    pub row_count: i64,
    pub truncated: bool,
    pub duration_ms: i64,
    pub csv_path: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct DatabaseQuerySummaryResponse {
    pub task_id: String,
    pub database_name: String,
    pub query_text: String,
    pub row_count: i64,
    pub truncated: bool,
    pub duration_ms: i64,
    pub created_at: String,
}

pub(crate) fn parse_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value.trim()).map_err(|_| "database connection id must be a UUID".to_string())
}
