use crate::models::{Conversation, Message};
use crate::storage::repositories::runs::RunWithEvent;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

pub const SESSION_SNAPSHOT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub text: String,
    pub covered_message_id: Option<String>,
    pub source_message_ids: Vec<String>,
}

impl<'de> Deserialize<'de> for SessionSummary {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            text: String,
            covered_message_id: Option<String>,
            source_message_ids: Option<Vec<String>>,
        }

        let helper = Helper::deserialize(deserializer)?;
        Ok(Self {
            text: helper.text,
            covered_message_id: helper.covered_message_id,
            source_message_ids: helper.source_message_ids.unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub format_version: u32,
    pub conversation: Conversation,
    pub messages: Vec<Message>,
    pub summary: Option<SessionSummary>,
}

#[derive(Debug, Clone)]
pub struct StartRunRequest {
    pub conversation_id: Uuid,
    pub user_message_id: Uuid,
    pub run_id: Uuid,
    pub event_id: Uuid,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StartedRun {
    pub user_message: Message,
    pub run: RunWithEvent,
}
