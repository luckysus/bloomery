use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub pinned: bool,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub response_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryHit {
    pub conversation_id: String,
    pub conversation_title: String,
    pub message_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub score: f64,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub scope: String,
    pub r#type: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub tags_json: String,
    pub enabled: bool,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInput {
    pub id: Option<String>,
    pub scope: String,
    pub r#type: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub tags_json: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySuggestion {
    pub id: String,
    pub scope: String,
    pub r#type: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub tags_json: String,
    pub reason: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudJob {
    pub id: String,
    pub conversation_id: Option<String>,
    pub cloud_job_id: String,
    pub r#type: String,
    pub status: String,
    pub payload_json: String,
    pub result_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudJobInput {
    pub id: Option<String>,
    pub conversation_id: Option<String>,
    pub cloud_job_id: String,
    pub r#type: String,
    pub status: String,
    pub payload_json: Option<String>,
    pub result_json: Option<String>,
}
