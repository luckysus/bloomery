use super::super::TurnSnapshot;
use crate::agent::protocol::{AgentEventEnvelope, RunOutcome};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

pub trait ChildTurnStore: Send + Sync {
    fn begin(&self, snapshot: &TurnSnapshot) -> Result<(), String>;
    fn begin_task(
        &self,
        snapshot: &TurnSnapshot,
        _task: &str,
        _agent_id: Option<&str>,
    ) -> Result<(), String> {
        self.begin(snapshot)
    }
    fn append(&self, event: &AgentEventEnvelope) -> Result<(), String>;
    fn finish(&self, child_turn_id: Uuid, outcome: RunOutcome) -> Result<(), String>;
}

#[derive(Clone)]
pub struct SqliteChildTurnStore {
    database: PathBuf,
    workspace_id: String,
    publisher: Option<Arc<dyn Fn(&AgentEventEnvelope) -> Result<(), String> + Send + Sync>>,
}

impl SqliteChildTurnStore {
    pub fn new(database: impl Into<PathBuf>, workspace_id: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            workspace_id: workspace_id.into(),
            publisher: None,
        }
    }

    pub fn with_event_publisher(
        mut self,
        publisher: impl Fn(&AgentEventEnvelope) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        self.publisher = Some(Arc::new(publisher));
        self
    }

    pub fn with_app_handle(self, app: tauri::AppHandle) -> Self {
        use tauri::Emitter;
        self.with_event_publisher(move |event| {
            app.emit("child-agent-event", event)
                .map_err(|error| error.to_string())
        })
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut rusqlite::Connection) -> Result<T, crate::storage::StorageError>,
    ) -> Result<T, String> {
        let (mut connection, _) =
            crate::storage::database::open(&self.database).map_err(|error| error.to_string())?;
        operation(&mut connection).map_err(|error| error.to_string())
    }
}

impl ChildTurnStore for SqliteChildTurnStore {
    fn begin(&self, snapshot: &TurnSnapshot) -> Result<(), String> {
        self.with_connection(|connection| {
            crate::storage::repositories::child_turns::create(
                connection,
                &self.workspace_id,
                snapshot,
                chrono::Utc::now(),
            )
        })
    }

    fn begin_task(
        &self,
        snapshot: &TurnSnapshot,
        task: &str,
        agent_id: Option<&str>,
    ) -> Result<(), String> {
        self.with_connection(|connection| {
            crate::storage::repositories::child_turns::create_for_task(
                connection,
                &self.workspace_id,
                snapshot,
                task,
                agent_id,
                chrono::Utc::now(),
            )
        })
    }

    fn append(&self, event: &AgentEventEnvelope) -> Result<(), String> {
        let stored = self.with_connection(|connection| {
            crate::storage::repositories::child_turns::append(connection, &self.workspace_id, event)
        })?;
        if let Some(publish) = &self.publisher {
            publish(&stored)?;
        }
        Ok(())
    }

    fn finish(&self, child_turn_id: Uuid, outcome: RunOutcome) -> Result<(), String> {
        self.with_connection(|connection| {
            crate::storage::repositories::child_turns::finish(
                connection,
                &self.workspace_id,
                child_turn_id,
                outcome,
                chrono::Utc::now(),
            )
        })
    }
}
