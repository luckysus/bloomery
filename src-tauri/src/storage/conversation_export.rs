use crate::agent::session::model::SessionSnapshot;
use crate::permissions::path::authorize_output_path;
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationExportFormat {
    Markdown,
    Json,
}

impl ConversationExportFormat {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "markdown" | "md" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            _ => Err("conversation export format must be markdown or json".to_string()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConversationExportSummary {
    pub format: String,
    pub output_path: String,
    pub message_count: usize,
    pub bytes: u64,
}

pub fn render_markdown(snapshot: &SessionSnapshot) -> String {
    let title = if snapshot.conversation.title.trim().is_empty() {
        "Bloomery conversation"
    } else {
        snapshot.conversation.title.trim()
    };
    let mut output = format!(
        "# {title}\n\n> Bloomery local conversation export\n> Conversation ID: {}\n> Updated: {}\n\n",
        snapshot.conversation.id, snapshot.conversation.updated_at
    );
    if let Some(summary) = snapshot
        .summary
        .as_ref()
        .filter(|value| !value.text.trim().is_empty())
    {
        output.push_str("## Conversation summary\n\n");
        output.push_str(summary.text.trim());
        output.push_str("\n\n");
    }
    for message in &snapshot.messages {
        output.push_str("### ");
        output.push_str(match message.role.as_str() {
            "user" => "User",
            "agent" | "assistant" => "Bloomery",
            "system" => "System",
            role if !role.trim().is_empty() => role,
            _ => "Message",
        });
        output.push_str(" / ");
        output.push_str(&message.created_at);
        output.push_str("\n\n");
        output.push_str(message.content.trim_end());
        output.push_str("\n\n");
    }
    output
}

pub fn write_snapshot(
    snapshot: &SessionSnapshot,
    output_path: &Path,
    format: ConversationExportFormat,
) -> Result<ConversationExportSummary, String> {
    let display_path = output_path.to_path_buf();
    let authorized_output = authorize_output_path(output_path)
        .map_err(|error| format!("conversation export path is not authorized: {error}"))?;
    let output_path = authorized_output.canonical_path();
    let file_name = output_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "conversation export file name is required".to_string())?;
    if output_path.exists() {
        return Err("conversation export destination already exists".to_string());
    }
    let parent = output_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("create conversation export directory failed: {error}"))?;
    let bytes = match format {
        ConversationExportFormat::Markdown => render_markdown(snapshot).into_bytes(),
        ConversationExportFormat::Json => serde_json::to_vec_pretty(snapshot)
            .map_err(|error| format!("serialize conversation export failed: {error}"))?,
    };
    let temporary = parent.join(format!(".{file_name}.tmp-{}", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| {
                format!("create conversation export temporary file failed: {error}")
            })?;
        file.write_all(&bytes)
            .map_err(|error| format!("write conversation export failed: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("flush conversation export failed: {error}"))?;
        drop(file);
        fs::rename(&temporary, output_path)
            .map_err(|error| format!("finalize conversation export failed: {error}"))?;
        Ok::<(), String>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|()| ConversationExportSummary {
        format: format.as_str().to_string(),
        output_path: display_path.to_string_lossy().into_owned(),
        message_count: snapshot.messages.len(),
        bytes: bytes.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::{render_markdown, write_snapshot, ConversationExportFormat};
    use crate::agent::session::model::{SessionSnapshot, SessionSummary};
    use crate::models::{Conversation, Message};
    use std::fs;
    use uuid::Uuid;

    fn snapshot() -> SessionSnapshot {
        SessionSnapshot {
            format_version: 1,
            conversation: Conversation {
                id: "conversation-1".to_string(),
                title: "\u{7089}\u{6b21}\u{5206}\u{6790}".to_string(),
                created_at: "2026-08-09T00:00:00Z".to_string(),
                updated_at: "2026-08-09T00:01:00Z".to_string(),
                pinned: false,
                archived: false,
            },
            messages: vec![
                Message {
                    id: "message-1".to_string(),
                    conversation_id: "conversation-1".to_string(),
                    role: "user".to_string(),
                    content: "\u{5206}\u{6790}\u{8fd9}\u{6279}\u{94a2}\u{6c34}\u{7684}\u{78b3}\u{5f53}\u{91cf}".to_string(),
                    response_json: None,
                    created_at: "2026-08-09T00:00:01Z".to_string(),
                },
                Message {
                    id: "message-2".to_string(),
                    conversation_id: "conversation-1".to_string(),
                    role: "assistant".to_string(),
                    content: "\u{8bf7}\u{63d0}\u{4f9b} C\u{3001}Mn \u{548c} Cr \u{7684}\u{8d28}\u{91cf}\u{5206}\u{6570}\u{3002}".to_string(),
                    response_json: Some("{\"run_id\":\"run-1\"}".to_string()),
                    created_at: "2026-08-09T00:00:02Z".to_string(),
                },
            ],
            summary: Some(SessionSummary {
                text: "\u{6b63}\u{5728}\u{6536}\u{96c6}\u{6210}\u{5206}\u{6570}\u{636e}".to_string(),
                covered_message_id: Some("message-1".to_string()),
                source_message_ids: vec!["message-1".to_string()],
            }),
        }
    }

    #[test]
    fn markdown_render_keeps_title_summary_and_message_order() {
        let markdown = render_markdown(&snapshot());

        assert!(markdown.starts_with("# \u{7089}\u{6b21}\u{5206}\u{6790}\n"));
        assert!(markdown.contains("## Conversation summary\n\n\u{6b63}\u{5728}\u{6536}\u{96c6}\u{6210}\u{5206}\u{6570}\u{636e}"));
        assert!(markdown.find("\u{5206}\u{6790}\u{8fd9}\u{6279}\u{94a2}\u{6c34}\u{7684}\u{78b3}\u{5f53}\u{91cf}").unwrap()
            < markdown.find("\u{8bf7}\u{63d0}\u{4f9b} C\u{3001}Mn \u{548c} Cr").unwrap());
        assert!(markdown.contains("### User"));
        assert!(markdown.contains("### Bloomery"));
    }

    #[test]
    fn json_export_writes_snapshot_without_partial_target() {
        let root =
            std::env::temp_dir().join(format!("bloomery-conversation-export-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create export root");
        let output = root.join("conversation.json");

        let summary = write_snapshot(&snapshot(), &output, ConversationExportFormat::Json)
            .expect("write conversation JSON");
        let content = fs::read_to_string(&output).expect("read conversation JSON");
        assert_eq!(summary.message_count, 2);
        assert!(content.contains("\"format_version\": 1"));
        assert!(fs::read_dir(&root)
            .expect("read export root")
            .flatten()
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".conversation.json.tmp-")));

        fs::remove_dir_all(root).expect("remove export root");
    }
}
