use super::{
    AgentHooks, AgentLoop, AgentLoopLimits, AgentLoopRequest, CancellationToken, ContextEntry,
    ModelAdapter, PermissionResolver, RuntimeHost, ToolExecutionError, ToolExecutor, ToolFuture,
    ToolHandler, ToolInvocation, ToolRegistration, TurnSnapshot,
};
use crate::agent::context::{ContextItem, ContextSource};
use crate::agent::protocol::{AgentEventEnvelope, RunOutcome};
use crate::agent::tool_repair::ToolSpec;
use crate::providers::profiles::ProviderCapability;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[path = "child_events.rs"]
mod child_events;
use child_events::ChildAgentEventSink;

pub const MAX_SUBAGENT_TOOL_ROUNDS: usize = 30;
pub const MAX_SUBAGENT_MODEL_CALLS: usize = 32;
pub const MAX_SUBAGENT_TOOL_CALLS: usize = 64;
const TASK_TOOL_ID: &str = "agent.task";
const TASK_TOOL_NAME: &str = "task";

pub trait ChildTurnStore: Send + Sync {
    fn begin(&self, snapshot: &TurnSnapshot) -> Result<(), String>;
    fn append(&self, event: &AgentEventEnvelope) -> Result<(), String>;
    fn finish(&self, child_turn_id: Uuid, outcome: RunOutcome) -> Result<(), String>;
}

#[derive(Clone)]
pub struct SqliteChildTurnStore {
    database: PathBuf,
    workspace_id: String,
}

impl SqliteChildTurnStore {
    pub fn new(database: impl Into<PathBuf>, workspace_id: impl Into<String>) -> Self {
        Self {
            database: database.into(),
            workspace_id: workspace_id.into(),
        }
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

    fn append(&self, event: &AgentEventEnvelope) -> Result<(), String> {
        self.with_connection(|connection| {
            crate::storage::repositories::child_turns::append(connection, &self.workspace_id, event)
                .map(|_| ())
        })
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskRequest {
    task: String,
}

pub struct SubagentTool {
    registration: ToolRegistration,
}

impl SubagentTool {
    pub fn new(
        model: Arc<dyn ModelAdapter>,
        tools: Arc<dyn ToolExecutor>,
        permissions: Arc<dyn PermissionResolver>,
        hooks: Arc<dyn AgentHooks>,
    ) -> Self {
        Self::new_with_parent(model, tools, permissions, hooks, None, None, None)
    }

    pub fn new_for_parent(
        model: Arc<dyn ModelAdapter>,
        tools: Arc<dyn ToolExecutor>,
        permissions: Arc<dyn PermissionResolver>,
        hooks: Arc<dyn AgentHooks>,
        runtime: RuntimeHost,
        parent_turn_id: Uuid,
    ) -> Self {
        Self::new_with_parent(
            model,
            tools,
            permissions,
            hooks,
            Some(runtime),
            Some(parent_turn_id),
            None,
        )
    }

    pub fn new_for_parent_with_store(
        model: Arc<dyn ModelAdapter>,
        tools: Arc<dyn ToolExecutor>,
        permissions: Arc<dyn PermissionResolver>,
        hooks: Arc<dyn AgentHooks>,
        runtime: RuntimeHost,
        parent_turn_id: Uuid,
        store: Arc<dyn ChildTurnStore>,
    ) -> Self {
        Self::new_with_parent(
            model,
            tools,
            permissions,
            hooks,
            Some(runtime),
            Some(parent_turn_id),
            Some(store),
        )
    }

    fn new_with_parent(
        model: Arc<dyn ModelAdapter>,
        tools: Arc<dyn ToolExecutor>,
        permissions: Arc<dyn PermissionResolver>,
        hooks: Arc<dyn AgentHooks>,
        runtime: Option<RuntimeHost>,
        parent_turn_id: Option<Uuid>,
        store: Option<Arc<dyn ChildTurnStore>>,
    ) -> Self {
        let handler = SubagentHandler {
            model,
            tools,
            permissions,
            hooks,
            runtime,
            parent_turn_id,
            store,
        };
        Self {
            registration: ToolRegistration::new(
                ToolSpec {
                    id: TASK_TOOL_ID.to_string(),
                    name: TASK_TOOL_NAME.to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {"task": {"type": "string", "minLength": 1}},
                        "required": ["task"],
                        "additionalProperties": false
                    }),
                    risk: crate::agent::protocol::PermissionRisk::Automatic,
                },
                true,
                Arc::new(handler),
            ),
        }
    }
}

impl ToolExecutor for SubagentTool {
    fn registrations(&self) -> &[ToolRegistration] {
        std::slice::from_ref(&self.registration)
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        self.registration
            .handler
            .execute(invocation.arguments, cancellation)
    }
}

struct SubagentHandler {
    model: Arc<dyn ModelAdapter>,
    tools: Arc<dyn ToolExecutor>,
    permissions: Arc<dyn PermissionResolver>,
    hooks: Arc<dyn AgentHooks>,
    runtime: Option<RuntimeHost>,
    parent_turn_id: Option<Uuid>,
    store: Option<Arc<dyn ChildTurnStore>>,
}

impl ToolHandler for SubagentHandler {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        let model = Arc::clone(&self.model);
        let tools = Arc::clone(&self.tools);
        let permissions = Arc::clone(&self.permissions);
        let hooks = Arc::clone(&self.hooks);
        let runtime = self.runtime.clone();
        let parent_turn_id = self.parent_turn_id;
        let child_store = self.store.clone();
        Box::pin(async move {
            let task = serde_json::from_value::<TaskRequest>(arguments)
                .map_err(|error| ToolExecutionError::new("invalid_task", error.to_string()))?;
            let task = task.task.trim().to_string();
            if task.is_empty() {
                return Err(ToolExecutionError::new(
                    "invalid_task",
                    "task must not be empty",
                ));
            }
            if tools.registrations().iter().any(|registration| {
                registration.spec.id == TASK_TOOL_ID || registration.spec.name == TASK_TOOL_NAME
            }) {
                return Err(ToolExecutionError::new(
                    "subagent_configuration_error",
                    "subagent tools must not include task",
                ));
            }
            if !model.capabilities().supports(ProviderCapability::Chat) {
                return Err(ToolExecutionError::new(
                    "subagent_configuration_error",
                    "subagent model does not support chat",
                ));
            }
            let child_turn_id = Uuid::new_v4();
            let child_runtime = match (runtime.clone(), parent_turn_id) {
                (Some(runtime), Some(parent_turn_id)) => {
                    let parent = runtime
                        .snapshot(parent_turn_id)
                        .map_err(|error| ToolExecutionError::new("child_turn_start", error))?;
                    let mut snapshot = TurnSnapshot {
                        turn_id: child_turn_id,
                        session_id: parent.session_id,
                        parent_turn_id: Some(parent_turn_id),
                        child_turn_limit: parent.child_turn_limit,
                        provider: parent.provider,
                        model: parent.model,
                        model_context_window: parent.model_context_window,
                        output_reservation: 2_048,
                        limits: AgentLoopLimits {
                            max_model_calls: Some(MAX_SUBAGENT_MODEL_CALLS),
                            max_tool_calls: Some(MAX_SUBAGENT_TOOL_CALLS),
                            max_tool_rounds: Some(MAX_SUBAGENT_TOOL_ROUNDS),
                            ..AgentLoopLimits::default()
                        },
                        tool_ids: Vec::new(),
                        tool_snapshot: Vec::new(),
                    };
                    let handle = runtime
                        .begin_turn(snapshot.clone())
                        .map_err(|error| ToolExecutionError::new("child_turn_start", error))?;
                    snapshot = match runtime.set_tool_snapshot(child_turn_id, tools.as_ref()) {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            runtime.finish_turn(child_turn_id);
                            return Err(ToolExecutionError::new("child_turn_start", error));
                        }
                    };
                    if let Some(store) = child_store.as_ref() {
                        if let Err(error) = store.begin(&snapshot) {
                            runtime.finish_turn(child_turn_id);
                            return Err(ToolExecutionError::new("child_turn_start", error));
                        }
                    }
                    Some((runtime, parent_turn_id, handle, snapshot))
                }
                _ => None,
            };
            let request = AgentLoopRequest {
                assistant_message_id: Uuid::new_v4(),
                context: vec![
                    ContextEntry::new(ContextItem::new(
                        "subagent-system",
                        ContextSource::System,
                        "You are a focused subagent. Complete only the delegated task and return a concise conclusion. Do not delegate further.",
                    )),
                    ContextEntry::new(ContextItem::new(
                        "subagent-task",
                        ContextSource::CurrentRequest,
                        task,
                    )),
                ],
                output_reservation: 2_048,
                evidence: None,
                attachments: Vec::new(),
                limits: child_runtime
                    .as_ref()
                    .map(|(_, _, _, snapshot)| snapshot.limits.clone())
                    .unwrap_or(AgentLoopLimits {
                        max_model_calls: Some(MAX_SUBAGENT_MODEL_CALLS),
                        max_tool_calls: Some(MAX_SUBAGENT_TOOL_CALLS),
                        max_tool_rounds: Some(MAX_SUBAGENT_TOOL_ROUNDS),
                        ..AgentLoopLimits::default()
                    }),
                input_queue: child_runtime
                    .as_ref()
                    .map(|(_, _, handle, _)| handle.input_queue.clone())
                    .unwrap_or_default(),
                resume: None,
            };
            let runner = AgentLoop::new_with_hooks(
                model.as_ref(),
                tools.as_ref(),
                permissions.as_ref(),
                hooks.as_ref(),
            );
            let mut sink = child_runtime
                .as_ref()
                .map(|(_, _, _, snapshot)| {
                    ChildAgentEventSink::new(
                        snapshot.turn_id,
                        snapshot.session_id,
                        child_store.clone(),
                    )
                })
                .unwrap_or_else(|| ChildAgentEventSink::new(Uuid::nil(), Uuid::nil(), None));
            let child_cancellation = if let Some((runtime, _, _, snapshot)) = &child_runtime {
                let runtime_token = runtime.cancellation_token(snapshot.turn_id);
                let parent_cancellation = cancellation.clone();
                CancellationToken::new(move || {
                    parent_cancellation.is_cancelled() || runtime_token.is_cancelled()
                })
            } else {
                cancellation.clone()
            };
            if let Some((runtime, _, _, snapshot)) = &child_runtime {
                let run_result = runner.run(request, &mut sink, child_cancellation).await;
                let event_error = runtime
                    .record_child_events(snapshot.turn_id, sink.events().to_vec())
                    .err();
                let child_outcome = run_result
                    .as_ref()
                    .map(|result| result.outcome)
                    .unwrap_or_else(|_| {
                        if runtime
                            .is_cancelled(&snapshot.turn_id.to_string())
                            .unwrap_or(false)
                        {
                            RunOutcome::Cancelled
                        } else {
                            RunOutcome::Failed
                        }
                    });
                let store_error = child_store
                    .as_ref()
                    .and_then(|store| store.finish(snapshot.turn_id, child_outcome).err());
                runtime.finish_turn(snapshot.turn_id);
                if let Some(error) = event_error {
                    return Err(ToolExecutionError::new("child_turn_events", error));
                }
                if let Some(error) = sink.error().or(store_error) {
                    return Err(ToolExecutionError::new("child_turn_events", error));
                }
                let result = run_result.map_err(|error| {
                    let code = if error.to_string().contains("limit exceeded") {
                        "subagent_turn_limit"
                    } else {
                        "subagent_execution_error"
                    };
                    ToolExecutionError::new(code, "subagent execution failed")
                })?;
                if result.outcome != RunOutcome::Completed {
                    return Err(ToolExecutionError::new(
                        "subagent_execution_error",
                        "subagent did not complete",
                    ));
                }
                return Ok(json!({"child_turn_id": child_turn_id, "conclusion": result.answer}));
            }
            let result = runner
                .run(request, &mut sink, child_cancellation)
                .await
                .map_err(|error| {
                    let code = if error.to_string().contains("limit exceeded") {
                        "subagent_turn_limit"
                    } else {
                        "subagent_execution_error"
                    };
                    ToolExecutionError::new(code, "subagent execution failed")
                })?;
            if result.outcome != RunOutcome::Completed {
                return Err(ToolExecutionError::new(
                    "subagent_execution_error",
                    "subagent did not complete",
                ));
            }
            Ok(json!({"child_turn_id": child_turn_id, "conclusion": result.answer}))
        })
    }
}

pub struct SnapshotToolExecutor {
    registrations: Vec<ToolRegistration>,
}

impl SnapshotToolExecutor {
    pub fn from(source: &dyn ToolExecutor) -> Self {
        Self {
            registrations: source
                .registrations()
                .iter()
                .filter(|registration| {
                    registration.spec.id != TASK_TOOL_ID && registration.spec.name != TASK_TOOL_NAME
                })
                .cloned()
                .collect(),
        }
    }
}

impl ToolExecutor for SnapshotToolExecutor {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        let Some(registration) = self.registrations.iter().find(|registration| {
            registration.spec.id == invocation.tool_id
                && registration.spec.name == invocation.tool_name
        }) else {
            return Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_not_registered",
                    "tool is not registered",
                ))
            });
        };
        registration
            .handler
            .execute(invocation.arguments, cancellation)
    }
}

#[cfg(test)]
#[path = "../runtime_subagents_tests.rs"]
mod tests;
