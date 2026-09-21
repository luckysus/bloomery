use super::mailbox::{ProtocolMailboxMessage, ProtocolMessageKind};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolKind {
    Shutdown,
    PlanApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolResolution {
    pub message_id: Uuid,
    pub approved: bool,
    pub content: String,
    pub resolved_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolRequest {
    pub id: Uuid,
    pub kind: ProtocolKind,
    pub sender: String,
    pub target: String,
    pub status: ProtocolStatus,
    pub content: String,
    pub created_at_utc: DateTime<Utc>,
    pub expires_at_utc: DateTime<Utc>,
    pub resolution: Option<ProtocolResolution>,
}

pub struct ProtocolStore {
    path: PathBuf,
}

impl ProtocolStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, String> {
        let path = root.into().join("protocol-state.json");
        if !path.exists() {
            write_state(&path, &[])?;
        }
        Ok(Self { path })
    }

    pub fn create(
        &self,
        kind: ProtocolKind,
        sender: &str,
        target: &str,
        content: &str,
        now: DateTime<Utc>,
    ) -> Result<ProtocolRequest, String> {
        if sender.is_empty() || target.is_empty() || content.is_empty() {
            return Err("protocol request fields must not be empty".to_string());
        }
        let mut requests = self.read()?;
        let request = ProtocolRequest {
            id: Uuid::new_v4(),
            kind,
            sender: sender.to_string(),
            target: target.to_string(),
            status: ProtocolStatus::Pending,
            content: content.to_string(),
            created_at_utc: now,
            expires_at_utc: now + Duration::hours(24),
            resolution: None,
        };
        requests.push(request.clone());
        write_state(&self.path, &requests)?;
        Ok(request)
    }

    pub fn get(&self, id: Uuid) -> Result<ProtocolRequest, String> {
        self.read()?
            .into_iter()
            .find(|request| request.id == id)
            .ok_or_else(|| format!("protocol request not found: {id}"))
    }

    pub fn latest_plan(&self, sender: &str) -> Result<Option<ProtocolRequest>, String> {
        Ok(self
            .read()?
            .into_iter()
            .filter(|request| {
                request.kind == ProtocolKind::PlanApproval && request.sender == sender
            })
            .max_by_key(|request| request.created_at_utc))
    }

    pub fn allows_effectful(&self, sender: &str) -> Result<bool, String> {
        Ok(self
            .latest_plan(sender)?
            .is_none_or(|request| request.status == ProtocolStatus::Approved))
    }

    pub fn consume_response(
        &self,
        message: &ProtocolMailboxMessage,
        now: DateTime<Utc>,
    ) -> Result<ProtocolRequest, String> {
        let mut requests = self.read()?;
        let request = requests
            .iter_mut()
            .find(|request| request.id == message.request_id)
            .ok_or_else(|| "protocol request not found".to_string())?;
        if request.status != ProtocolStatus::Pending {
            if request
                .resolution
                .as_ref()
                .is_some_and(|resolution| resolution.message_id == message.id)
            {
                return Ok(request.clone());
            }
            return Err("protocol request is already resolved".to_string());
        }
        if now >= request.expires_at_utc
            || message.recipient != request.sender
            || message.sender != request.target
        {
            return Err("protocol response does not match request".to_string());
        }
        let approved = message
            .approved
            .ok_or_else(|| "protocol response requires approved".to_string())?;
        request.status = if approved {
            ProtocolStatus::Approved
        } else {
            ProtocolStatus::Rejected
        };
        request.resolution = Some(ProtocolResolution {
            message_id: message.id,
            approved,
            content: message.content.clone(),
            resolved_at_utc: now,
        });
        let result = request.clone();
        write_state(&self.path, &requests)?;
        Ok(result)
    }

    pub fn to_message(request: &ProtocolRequest) -> ProtocolMailboxMessage {
        ProtocolMailboxMessage {
            id: Uuid::new_v4(),
            sender: request.sender.clone(),
            recipient: request.target.clone(),
            kind: match request.kind {
                ProtocolKind::Shutdown => ProtocolMessageKind::ShutdownRequest,
                ProtocolKind::PlanApproval => ProtocolMessageKind::PlanApprovalRequest,
            },
            content: request.content.clone(),
            request_id: request.id,
            approved: None,
        }
    }

    fn read(&self) -> Result<Vec<ProtocolRequest>, String> {
        let bytes = fs::read(&self.path).map_err(|error| error.to_string())?;
        serde_json::from_slice(&bytes).map_err(|error| format!("invalid protocol state: {error}"))
    }
}

fn write_state(path: &PathBuf, requests: &[ProtocolRequest]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(requests).map_err(|error| error.to_string())?;
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}
