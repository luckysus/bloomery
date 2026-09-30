use crate::agent::runtime::CancellationToken;
use crate::app::steel_agent_gateway::DesktopSteelAgentGateway;
use crate::db::current_workspace_id;
use crate::steel::SteelAgentGateway;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

#[derive(Debug, Deserialize)]
pub struct LiteratureSearchRequest {
    pub query: String,
    #[serde(default)]
    pub knowledge_base_ids: Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct LiteratureSectionRequest {
    pub query: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub document_hint: Option<String>,
    #[serde(default)]
    pub chapter_number: Option<u64>,
    #[serde(default)]
    pub knowledge_base_ids: Vec<String>,
    #[serde(default = "default_section_limit")]
    pub limit: usize,
    #[serde(default = "default_max_chars")]
    pub max_chars: usize,
}

fn default_limit() -> usize {
    20
}
fn default_section_limit() -> usize {
    8
}
fn default_max_chars() -> usize {
    12_000
}

fn gateway(app: &AppHandle) -> Result<DesktopSteelAgentGateway, String> {
    let gateway =
        DesktopSteelAgentGateway::new(crate::db::database_path(app)?, current_workspace_id());
    if let Ok(pool) = crate::knowledge_db::pool_for_query(
        app.state::<crate::knowledge_db::KnowledgeDatabaseState>()
            .inner(),
    ) {
        Ok(gateway.with_postgres_pool(app.clone(), pool))
    } else {
        Ok(gateway)
    }
}

#[tauri::command]
pub async fn search_literature(
    app: AppHandle,
    request: LiteratureSearchRequest,
) -> Result<Value, String> {
    let query = request.query.trim().to_string();
    if query.is_empty() {
        return Err("请输入文献检索词".to_string());
    }
    gateway(&app)?.execute("search_literature", json!({
        "query": query, "limit": request.limit.clamp(1, 50), "knowledge_base_ids": request.knowledge_base_ids,
    }), CancellationToken::new(|| false)).await
}

#[tauri::command]
pub async fn read_literature_section(
    app: AppHandle,
    request: LiteratureSectionRequest,
) -> Result<Value, String> {
    let query = request.query.trim().to_string();
    if query.is_empty() {
        return Err("请输入要阅读的文献或章节".to_string());
    }
    gateway(&app)?.execute("read_literature_section", json!({
        "query": query, "mode": request.mode.unwrap_or_else(|| "section".to_string()),
        "document_hint": request.document_hint.unwrap_or_default(), "chapter_number": request.chapter_number,
        "limit": request.limit.clamp(1, 20), "max_chars": request.max_chars.clamp(1000, 60_000),
        "knowledge_base_ids": request.knowledge_base_ids,
    }), CancellationToken::new(|| false)).await
}

#[tauri::command]
pub async fn process_literature(app: AppHandle, arguments: Value) -> Result<Value, String> {
    gateway(&app)?
        .execute(
            "process_literature",
            arguments,
            CancellationToken::new(|| false),
        )
        .await
}
