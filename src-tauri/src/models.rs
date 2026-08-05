use serde::{Deserialize, Serialize};

use crate::agent::context::MemoryStatus;

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationSnapshotMessage {
    pub role: String,
    pub content: String,
    pub response_json: Option<String>,
}

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
    pub source_message_id: Option<String>,
    pub source_run_id: Option<String>,
    pub confidence: f64,
    pub status: MemoryStatus,
    pub dedup_key: String,
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
