use super::model::{
    SessionSnapshot, SessionSummary, StartRunOutcome, StartRunRequest, StartedRun,
    SESSION_SNAPSHOT_FORMAT_VERSION,
};
use crate::models::{Conversation, ConversationSnapshotMessage, HistoryHit, Message};
use crate::storage::repositories::{conversations, events, runs};
use rusqlite::{Connection, TransactionBehavior};

pub struct SessionService<'a> {
    connection: &'a mut Connection,
    workspace_id: &'a str,
}

impl<'a> SessionService<'a> {
    pub fn new(connection: &'a mut Connection, workspace_id: &'a str) -> Result<Self, String> {
        if workspace_id.trim().is_empty() || workspace_id.trim() != workspace_id {
            return Err("workspace_id is invalid".to_string());
        }
        Ok(Self {
            connection,
            workspace_id,
        })
    }

    pub fn list_conversations(&self, archived: bool) -> Result<Vec<Conversation>, String> {
        conversations::list(self.connection, self.workspace_id, archived)
    }

    pub fn create_conversation(&mut self, title: &str) -> Result<Conversation, String> {
        conversations::create(self.connection, self.workspace_id, title)
    }

    pub fn get_conversation(&self, conversation_id: &str) -> Result<Conversation, String> {
        self.require_conversation(conversation_id)
    }

    pub fn rename_conversation(
        &mut self,
        conversation_id: &str,
        title: &str,
    ) -> Result<(), String> {
        conversations::update_title(self.connection, self.workspace_id, conversation_id, title)
    }

    pub fn set_conversation_pinned(
        &mut self,
        conversation_id: &str,
        pinned: bool,
    ) -> Result<(), String> {
        conversations::set_pinned(self.connection, self.workspace_id, conversation_id, pinned)
    }

    pub fn archive_conversation(&mut self, conversation_id: &str) -> Result<(), String> {
        conversations::set_archived(self.connection, self.workspace_id, conversation_id, true)
    }

    pub fn restore_conversation(&mut self, conversation_id: &str) -> Result<(), String> {
        conversations::set_archived(self.connection, self.workspace_id, conversation_id, false)
    }

    pub fn delete_conversation(&mut self, conversation_id: &str) -> Result<(), String> {
        conversations::delete(self.connection, self.workspace_id, conversation_id)
    }

    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<Message>, String> {
        self.require_conversation(conversation_id)?;
        conversations::list_messages(self.connection, self.workspace_id, conversation_id)
    }

    pub fn search_history(
        &self,
        query: &str,
        conversation_id: Option<&str>,
        exclude_current: bool,
        limit: usize,
    ) -> Result<Vec<HistoryHit>, String> {
        if let Some(conversation_id) = conversation_id.filter(|value| !value.trim().is_empty()) {
            self.require_conversation(conversation_id)?;
        }
        conversations::search_history(
            self.connection,
            self.workspace_id,
            query,
            conversation_id,
            exclude_current,
            limit,
        )
    }

    pub fn append_message(
        &mut self,
        conversation_id: &str,
        role: &str,
        content: &str,
        response_json: Option<String>,
    ) -> Result<Message, String> {
        conversations::append_message(
            self.connection,
            self.workspace_id,
            conversation_id,
            role,
            content,
            response_json,
        )
    }

    pub fn import_snapshot(
        &mut self,
        conversation_id: &str,
        title: &str,
        messages: Vec<ConversationSnapshotMessage>,
    ) -> Result<(), String> {
        self.require_conversation(conversation_id)?;
        conversations::save_snapshot(
            self.connection,
            self.workspace_id,
            conversation_id,
            title,
            messages,
        )
    }

    pub fn edit_message_and_truncate(
        &mut self,
        message_id: &str,
        content: &str,
    ) -> Result<(), String> {
        conversations::replace_after_edit(self.connection, self.workspace_id, message_id, content)
    }

    pub fn truncate_after_message(&mut self, message_id: &str) -> Result<(), String> {
        conversations::truncate_after_message(self.connection, self.workspace_id, message_id)
    }

    pub fn fork_conversation_from_message(
        &mut self,
        message_id: &str,
    ) -> Result<Conversation, String> {
        conversations::fork_from_anchor(self.connection, self.workspace_id, message_id)
    }

    pub fn load_summary(&self, conversation_id: &str) -> Result<String, String> {
        self.require_conversation(conversation_id)?;
        conversations::get_summary(self.connection, self.workspace_id, conversation_id)
    }

    pub fn save_summary(
        &mut self,
        conversation_id: &str,
        summary: &str,
        covered_message_id: Option<String>,
    ) -> Result<(), String> {
        conversations::save_summary(
            self.connection,
            self.workspace_id,
            conversation_id,
            summary,
            covered_message_id,
        )
    }

    pub fn load_draft(&self, conversation_id: &str) -> Result<String, String> {
        self.require_conversation_or_new_draft(conversation_id)?;
        conversations::get_draft(self.connection, self.workspace_id, conversation_id)
    }

    pub fn save_draft(&mut self, conversation_id: &str, content: &str) -> Result<(), String> {
        self.require_conversation_or_new_draft(conversation_id)?;
        if content.trim().is_empty() {
            conversations::clear_draft(self.connection, self.workspace_id, conversation_id)
        } else {
            conversations::save_draft(self.connection, self.workspace_id, conversation_id, content)
        }
    }

    pub fn clear_draft(&mut self, conversation_id: &str) -> Result<(), String> {
        self.require_conversation_or_new_draft(conversation_id)?;
        conversations::clear_draft(self.connection, self.workspace_id, conversation_id)
    }

    pub fn export_snapshot(&self, conversation_id: &str) -> Result<SessionSnapshot, String> {
        let conversation = self.require_conversation(conversation_id)?;
        let messages =
            conversations::list_messages(self.connection, self.workspace_id, conversation_id)?;
        let summary =
            conversations::latest_summary(self.connection, self.workspace_id, conversation_id)?
                .map(|summary| SessionSummary {
                    text: summary.summary,
                    covered_message_id: summary.covered_message_id,
                    source_message_ids: summary.source_message_ids,
                });
        Ok(SessionSnapshot {
            format_version: SESSION_SNAPSHOT_FORMAT_VERSION,
            conversation,
            messages,
            summary,
        })
    }

    pub fn start_run(&mut self, request: StartRunRequest) -> Result<StartedRun, String> {
        if request.content.trim().is_empty() {
            return Err("user message is required".to_string());
        }
        let conversation_id = request.conversation_id.to_string();
        self.require_conversation(&conversation_id)?;
        let timestamp = request
            .timestamp
            .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let user_message = conversations::append_message_in_transaction(
            &transaction,
            self.workspace_id,
            &conversation_id,
            request.user_message_id,
            "user",
            &request.content,
            None,
            &timestamp,
        )?;
        let run = runs::create_in_transaction(
            &transaction,
            runs::NewAgentRun {
                id: request.run_id,
                workspace_id: self.workspace_id.to_string(),
                conversation_id: request.conversation_id,
                user_message_id: request.user_message_id,
                event_id: request.event_id,
                timestamp: request.timestamp,
            },
        )
        .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(StartedRun { user_message, run })
    }

    pub fn start_or_replay(&mut self, request: StartRunRequest) -> Result<StartRunOutcome, String> {
        if request.content.trim().is_empty() {
            return Err("user message is required".to_string());
        }
        if let Some(run) = runs::get(self.connection, self.workspace_id, request.run_id)
            .map_err(|error| error.to_string())?
        {
            if run.conversation_id != request.conversation_id
                || run.user_message_id != request.user_message_id
            {
                return Err("run_id belongs to a different conversation or message".to_string());
            }
            let events = events::replay(self.connection, self.workspace_id, request.run_id, 0)
                .map_err(|error| error.to_string())?;
            return Ok(StartRunOutcome::Existing { run, events });
        }
        self.start_run(request).map(StartRunOutcome::Started)
    }

    fn require_conversation(&self, conversation_id: &str) -> Result<Conversation, String> {
        conversations::get(self.connection, self.workspace_id, conversation_id)?
            .ok_or_else(|| "conversation not found".to_string())
    }

    fn require_conversation_or_new_draft(&self, conversation_id: &str) -> Result<(), String> {
        if matches!(conversation_id, "__new__" | "__agent_new__") {
            return Ok(());
        }
        self.require_conversation(conversation_id).map(|_| ())
    }
}
