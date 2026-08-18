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
}

pub(crate) fn parse_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value.trim()).map_err(|_| "database connection id must be a UUID".to_string())
}
