use super::{
    AgentHooks, AgentLoop, AgentLoopLimits, AgentLoopRequest, CancellationToken, ContextEntry,
    ModelAdapter, PermissionResolver, RuntimeHost, ToolExecutionError, ToolExecutor, ToolFuture,
    ToolHandler, ToolInvocation, ToolRegistration, TurnSnapshot,
};
use crate::agent::context::{ContextItem, ContextSource};
use crate::agent::protocol::RunOutcome;
use crate::agent::tool_repair::ToolSpec;
use crate::providers::profiles::ProviderCapability;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

#[path = "child_events.rs"]
mod child_events;
use child_events::ChildAgentEventSink;

#[path = "child_result.rs"]
mod child_result;
use child_result::{completed as child_result, details as child_result_details};

mod child_store;
pub use child_store::{ChildTurnStore, SqliteChildTurnStore};

mod snapshot;
pub use snapshot::{child_limits, SnapshotToolExecutor};

pub const MAX_SUBAGENT_TOOL_ROUNDS: usize = 30;
pub const MAX_SUBAGENT_MODEL_CALLS: usize = 32;
pub const MAX_SUBAGENT_TOOL_CALLS: usize = 64;
const TASK_TOOL_ID: &str = "agent.task";
const TASK_TOOL_NAME: &str = "task";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskRequest {
    task: String,
    agent_id: Option<String>,
}

#[derive(Clone)]
pub struct SubagentProfile {
    pub id: String,
    pub system_prompt: String,
    pub model: Arc<dyn ModelAdapter>,
    pub tools: Arc<dyn ToolExecutor>,
    pub limits: AgentLoopLimits,
    pub provider: String,
    pub model_name: String,
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
        Self::new_with_parent(
            model,
            tools,
            permissions,
            hooks,
            None,
            None,
            None,
            Vec::new(),
        )
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
            Vec::new(),
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
            Vec::new(),
        )
    }

    pub fn new_for_parent_with_profiles(
        model: Arc<dyn ModelAdapter>,
        tools: Arc<dyn ToolExecutor>,
        permissions: Arc<dyn PermissionResolver>,
        hooks: Arc<dyn AgentHooks>,
        runtime: RuntimeHost,
        parent_turn_id: Uuid,
        store: Arc<dyn ChildTurnStore>,
        profiles: Vec<SubagentProfile>,
    ) -> Self {
        Self::new_with_parent(
            model,
            tools,
            permissions,
            hooks,
            Some(runtime),
            Some(parent_turn_id),
            Some(store),
            profiles,
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
        profiles: Vec<SubagentProfile>,
    ) -> Self {
        let handler = SubagentHandler {
            model,
            tools,
            permissions,
            hooks,
            runtime,
            parent_turn_id,
            store,
            profiles: Arc::new(profiles),
        };
        Self {
            registration: ToolRegistration::new(
                ToolSpec {
                    id: TASK_TOOL_ID.to_string(),
                    name: TASK_TOOL_NAME.to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "task": {"type": "string", "minLength": 1, "maxLength": 16000},
                            "agent_id": {"type": "string", "minLength": 1, "maxLength": 128}
                        },
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
        if invocation.tool_id != TASK_TOOL_ID || invocation.tool_name != TASK_TOOL_NAME {
            return Box::pin(async {
                Err(ToolExecutionError::new(
                    "tool_not_registered",
                    "task tool is not registered",
                ))
            });
        }
        self.registration
            .handler
            .execute_for_invocation(invocation, cancellation)
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
    profiles: Arc<Vec<SubagentProfile>>,
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
        let profiles = self.profiles.clone();
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ToolExecutionError::cancelled());
            }
            let request = serde_json::from_value::<TaskRequest>(arguments)
                .map_err(|error| ToolExecutionError::new("invalid_task", error.to_string()))?;
            let task = request.task.trim().to_string();
            if task.is_empty() || task.chars().count() > 16000 {
                return Err(ToolExecutionError::new(
                    "invalid_task",
                    "task must contain between 1 and 16000 characters",
                ));
            }
            let profile = request
                .agent_id
                .as_deref()
                .map(|id| {
                    if id.trim().is_empty() || id.len() > 128 {
                        return Err(ToolExecutionError::new(
                            "invalid_task",
                            "agent_id is invalid",
                        ));
                    }
                    let mut matching = profiles.iter().filter(|profile| profile.id == id);
                    let profile = matching.next().ok_or_else(|| {
                        ToolExecutionError::new(
                            "agent_profile_disabled",
                            "requested expert is unavailable or disabled",
                        )
                    })?;
                    if matching.next().is_some() {
                        return Err(ToolExecutionError::new(
                            "subagent_configuration_error",
                            "duplicate expert profile",
                        ));
                    }
                    profile.limits.validate().map_err(|error| {
                        ToolExecutionError::new("subagent_configuration_error", error)
                    })?;
                    Ok(profile.clone())
                })
                .transpose()?;
            let model = profile
                .as_ref()
                .map(|profile| profile.model.clone())
                .unwrap_or(model);
            let mut selected_tools = profile
                .as_ref()
                .map(|profile| {
                    SnapshotToolExecutor::intersect(profile.tools.as_ref(), tools.as_ref())
                })
                .unwrap_or_else(|| SnapshotToolExecutor::from(tools.as_ref()));
            let parent = match (runtime.as_ref(), parent_turn_id) {
                (Some(runtime), Some(parent_turn_id)) => Some(
                    runtime
                        .snapshot(parent_turn_id)
                        .map_err(|error| ToolExecutionError::new("child_turn_start", error))?,
                ),
                _ => None,
            };
            if let Some(parent) = &parent {
                selected_tools.restrict_to_ids(&parent.tool_ids);
            }
            let tools: Arc<dyn ToolExecutor> = Arc::new(selected_tools);
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
                    let parent = parent.as_ref().expect("parent snapshot checked above");
                    let mut snapshot = TurnSnapshot {
                        turn_id: child_turn_id,
                        session_id: parent.session_id,
                        parent_turn_id: Some(parent_turn_id),
                        child_turn_limit: parent.child_turn_limit,
                        provider: profile
                            .as_ref()
                            .map(|profile| profile.provider.clone())
                            .unwrap_or_else(|| parent.provider.clone()),
                        model: profile
                            .as_ref()
                            .map(|profile| profile.model_name.clone())
                            .unwrap_or_else(|| parent.model.clone()),
                        model_context_window: model.capabilities().context_window,
                        output_reservation: parent.output_reservation,
                        reasoning_reservation: parent.reasoning_reservation,
                        limits: child_limits(
                            Some(&parent.limits),
                            profile.as_ref().map(|profile| &profile.limits),
                        ),
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
                        if let Err(error) =
                            store.begin_task(&snapshot, &task, request.agent_id.as_deref())
                        {
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
                        format!("You are a focused subagent. Complete only the delegated task and return a concise conclusion. Do not delegate further.\n\n{}", profile.as_ref().map(|profile| profile.system_prompt.as_str()).unwrap_or("")),
                    )),
                    ContextEntry::new(ContextItem::new(
                        "subagent-task",
                        ContextSource::CurrentRequest,
                        task,
                    )),
                ],
                output_reservation: child_runtime.as_ref().map(|(_, _, _, snapshot)| snapshot.output_reservation).unwrap_or(2_048),
                reasoning_reservation: child_runtime
                    .as_ref()
                    .map(|(_, _, _, snapshot)| snapshot.reasoning_reservation)
                    .unwrap_or(crate::agent::context::DEFAULT_REASONING_RESERVATION),
                evidence: None,
                attachments: Vec::new(),
                limits: child_runtime
                    .as_ref()
                    .map(|(_, _, _, snapshot)| snapshot.limits.clone())
                    .unwrap_or_else(|| child_limits(None, profile.as_ref().map(|profile| &profile.limits))),
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
                    ToolExecutionError::with_details(
                        code,
                        "subagent execution failed",
                        child_result_details(child_turn_id, None, sink.events()),
                    )
                })?;
                if result.outcome != RunOutcome::Completed {
                    return Err(ToolExecutionError::with_details(
                        "subagent_execution_error",
                        "subagent did not complete",
                        child_result_details(child_turn_id, Some(result.outcome), sink.events()),
                    ));
                }
                return Ok(child_result(child_turn_id, &result, sink.events()));
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
                    ToolExecutionError::with_details(
                        code,
                        "subagent execution failed",
                        child_result_details(child_turn_id, None, sink.events()),
                    )
                })?;
            if result.outcome != RunOutcome::Completed {
                return Err(ToolExecutionError::with_details(
                    "subagent_execution_error",
                    "subagent did not complete",
                    child_result_details(child_turn_id, Some(result.outcome), sink.events()),
                ));
            }
            Ok(child_result(child_turn_id, &result, sink.events()))
        })
    }
}

#[cfg(test)]
#[path = "../runtime_subagents_tests.rs"]
mod tests;
