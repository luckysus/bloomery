use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailboxMessageKind {
    Task,
    Message,
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub id: Uuid,
    pub sender: String,
    pub recipient: String,
    pub kind: MailboxMessageKind,
    pub content: String,
}

pub struct MailboxStore {
    root: PathBuf,
}

impl MailboxStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, String> {
        let store = Self { root: root.into() };
        for recipient in ["lead"] {
            store.ensure_recipient(recipient)?;
        }
        Ok(store)
    }

    pub fn ensure_recipient(&self, recipient: &str) -> Result<(), String> {
        validate_recipient(recipient)?;
        for state in ["ready", "processing", "done", "quarantine"] {
            fs::create_dir_all(self.directory(recipient, state))
                .map_err(|error| format!("create mailbox directory failed: {error}"))?;
        }
        Ok(())
    }

    pub fn send(&self, message: &MailboxMessage) -> Result<(), String> {
        validate_message(message)?;
        self.ensure_recipient(&message.recipient)?;
        let destination = self.path(&message.recipient, "ready", message.id);
        if destination.exists()
            || self
                .path(&message.recipient, "processing", message.id)
                .exists()
            || self.path(&message.recipient, "done", message.id).exists()
        {
            return Err(format!("mailbox message id already exists: {}", message.id));
        }
        write_atomically(&destination, message)
    }

    pub fn claim(&self, recipient: &str) -> Result<Option<MailboxMessage>, String> {
        self.ensure_recipient(recipient)?;
        let ready = first_message(&self.directory(recipient, "ready"))?;
        let Some(path) = ready else { return Ok(None) };
        let id = parse_id(&path)?;
        let processing = self.path(recipient, "processing", id);
        fs::rename(&path, &processing)
            .map_err(|error| format!("claim mailbox message failed: {error}"))?;
        match read_message(&processing) {
            Ok(message) if message.recipient == recipient && message.id == id => Ok(Some(message)),
            Ok(_) | Err(_) => {
                let quarantine = self.path(recipient, "quarantine", id);
                let _ = fs::rename(&processing, quarantine);
                Ok(None)
            }
        }
    }

    pub fn ack(&self, message: &MailboxMessage) -> Result<bool, String> {
        validate_message(message)?;
        let processing = self.path(&message.recipient, "processing", message.id);
        let done = self.path(&message.recipient, "done", message.id);
        if done.exists() {
            return match read_message(&done)? == *message {
                true => Ok(true),
                false => Err(format!("mailbox message content mismatch: {}", message.id)),
            };
        }
        if !processing.exists() {
            return Err(format!("mailbox message is not processing: {}", message.id));
        }
        let stored = read_message(&processing)?;
        if stored != *message {
            return Err(format!("mailbox message content mismatch: {}", message.id));
        }
        fs::rename(processing, done)
            .map_err(|error| format!("ack mailbox message failed: {error}"))?;
        Ok(true)
    }

    pub fn release(&self, message: &MailboxMessage) -> Result<(), String> {
        validate_message(message)?;
        let processing = self.path(&message.recipient, "processing", message.id);
        let ready = self.path(&message.recipient, "ready", message.id);
        if processing.exists() {
            fs::rename(processing, ready)
                .map_err(|error| format!("release mailbox message failed: {error}"))?;
        }
        Ok(())
    }

    pub fn recover_processing(&self, recipient: &str) -> Result<usize, String> {
        self.ensure_recipient(recipient)?;
        let mut count = 0;
        for entry in fs::read_dir(self.directory(recipient, "processing"))
            .map_err(|error| format!("read processing mailbox failed: {error}"))?
        {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let id = parse_id(&path)?;
            fs::rename(path, self.path(recipient, "ready", id))
                .map_err(|error| format!("recover mailbox message failed: {error}"))?;
            count += 1;
        }
        Ok(count)
    }

    fn directory(&self, recipient: &str, state: &str) -> PathBuf {
        self.root.join(recipient).join(state)
    }

    fn path(&self, recipient: &str, state: &str, id: Uuid) -> PathBuf {
        self.directory(recipient, state).join(format!("{id}.json"))
    }
}

fn validate_message(message: &MailboxMessage) -> Result<(), String> {
    validate_recipient(&message.sender)?;
    validate_recipient(&message.recipient)?;
    if message.content.is_empty() {
        return Err("mailbox message content must not be empty".to_string());
    }
    Ok(())
}

fn validate_recipient(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || value.starts_with('-')
        || value.ends_with('-')
    {
        return Err(format!("invalid mailbox recipient: {value}"));
    }
    Ok(())
}

fn first_message(directory: &Path) -> Result<Option<PathBuf>, String> {
    let mut paths = fs::read_dir(directory)
        .map_err(|error| format!("read mailbox failed: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths.into_iter().next())
}

fn parse_id(path: &Path) -> Result<Uuid, String> {
    let name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "invalid mailbox filename".to_string())?;
    Uuid::parse_str(name).map_err(|error| format!("invalid mailbox filename: {error}"))
}

fn read_message(path: &Path) -> Result<MailboxMessage, String> {
    let bytes = fs::read(path).map_err(|error| format!("read mailbox message failed: {error}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("decode mailbox message failed: {error}"))
}

fn write_atomically(path: &Path, message: &MailboxMessage) -> Result<(), String> {
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    fs::write(&temporary, bytes)
        .map_err(|error| format!("write mailbox message failed: {error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("install mailbox message failed: {error}"))
}
