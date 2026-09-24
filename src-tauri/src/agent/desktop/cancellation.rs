use crate::agent::context::{ContextItem, ContextSource};
use crate::agent::protocol::PermissionDecision;
use crate::agent::runtime::{
    AgentInputKind, AgentInputQueue, CancellationToken, ContextEntry, PermissionFuture,
    PermissionRequest, PermissionResolver,
};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct LocalAgentState {
    cancelled_runs: Arc<Mutex<HashSet<String>>>,
    input_queues: Arc<Mutex<HashMap<String, AgentInputQueue>>>,
    pending_permissions: Arc<Mutex<HashMap<Uuid, PendingPermission>>>,
    session_permissions: Arc<Mutex<HashSet<String>>>,
    always_permissions: Arc<Mutex<HashSet<String>>>,
}

#[derive(Clone)]
pub struct InteractivePermissionResolver {
    pending_permissions: Arc<Mutex<HashMap<Uuid, PendingPermission>>>,
    session_permissions: Arc<Mutex<HashSet<String>>>,
    always_permissions: Arc<Mutex<HashSet<String>>>,
}

struct PendingPermission {
    request: PermissionRequest,
    sender: oneshot::Sender<PermissionDecision>,
}

impl LocalAgentState {
    pub fn cancel_run(&self, run_id: &str) -> Result<(), String> {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return Ok(());
        }
        self.cancelled_runs
            .lock()
            .map_err(|_| "local agent state poisoned")?
            .insert(run_id.to_string());
        Ok(())
    }

    pub fn is_cancelled(&self, run_id: &str) -> Result<bool, String> {
        Ok(self
            .cancelled_runs
            .lock()
            .map_err(|_| "local agent state poisoned")?
            .contains(run_id))
    }

    pub fn clear_cancelled(&self, run_id: &str) {
        if let Ok(mut cancelled) = self.cancelled_runs.lock() {
            cancelled.remove(run_id);
        }
    }

    pub fn cancellation_token(&self, run_id: &str) -> crate::agent::runtime::CancellationToken {
        let cancelled_runs = Arc::clone(&self.cancelled_runs);
        let run_id = run_id.trim().to_string();
        crate::agent::runtime::CancellationToken::new(move || {
            cancelled_runs
                .lock()
                .map(|cancelled| cancelled.contains(&run_id))
                .unwrap_or(true)
        })
    }

    pub fn register_input_queue(&self, run_id: &str) -> Result<AgentInputQueue, String> {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return Err("run_id is required".to_string());
        }
        let mut queues = self
            .input_queues
            .lock()
            .map_err(|_| "local agent state poisoned".to_string())?;
        Ok(queues
            .entry(run_id.to_string())
            .or_insert_with(AgentInputQueue::default)
            .clone())
    }

    pub fn remove_input_queue(&self, run_id: &str) {
        if let Ok(mut queues) = self.input_queues.lock() {
            queues.remove(run_id.trim());
        }
    }

    pub fn enqueue_input(
        &self,
        run_id: &str,
        kind: AgentInputKind,
        message: String,
    ) -> Result<(), String> {
        let message = message.trim().to_string();
        if message.is_empty() {
            return Err("message is required".to_string());
        }
        let queue = self
            .input_queues
            .lock()
            .map_err(|_| "local agent state poisoned".to_string())?
            .get(run_id.trim())
            .cloned()
            .ok_or_else(|| "agent run is not active".to_string())?;
        let kind_name = match kind {
            AgentInputKind::Steering => "steering",
            AgentInputKind::FollowUp => "follow_up",
        };
        queue.enqueue(
            kind,
            ContextEntry::new(ContextItem::new(
                format!("runtime-{kind_name}-{}", Uuid::new_v4()),
                ContextSource::CurrentRequest,
                message,
            )),
        )
    }

    pub fn permission_resolver(&self) -> InteractivePermissionResolver {
        InteractivePermissionResolver {
            pending_permissions: Arc::clone(&self.pending_permissions),
            session_permissions: Arc::clone(&self.session_permissions),
            always_permissions: Arc::clone(&self.always_permissions),
        }
    }

    pub fn resolve_permission(
        &self,
        permission_id: Uuid,
        decision: PermissionDecision,
    ) -> Result<PermissionRequest, String> {
        let pending = self
            .pending_permissions
            .lock()
            .map_err(|_| "local agent state poisoned")?
            .remove(&permission_id)
            .ok_or_else(|| "permission request not found or already resolved".to_string())?;
        let PendingPermission { request, sender } = pending;
        sender
            .send(decision)
            .map_err(|_| "permission request is no longer waiting".to_string())?;
        Ok(request)
    }

    pub fn restore_permission(
        &self,
        request: PermissionRequest,
        cancellation: CancellationToken,
    ) -> Result<PermissionFuture, String> {
        let permission_id = request.permission_id;
        let (sender, receiver) = oneshot::channel();
        let inserted = self
            .pending_permissions
            .lock()
            .map_err(|_| "local agent state poisoned".to_string())?
            .insert(permission_id, PendingPermission { request, sender })
            .is_none();
        if !inserted {
            return Err("permission request is already waiting".to_string());
        }
        let pending_permissions = Arc::clone(&self.pending_permissions);
        Ok(Box::pin(async move {
            tokio::select! {
                decision = receiver => decision.unwrap_or(PermissionDecision::Deny),
                _ = wait_for_cancellation(cancellation) => {
                    if let Ok(mut pending) = pending_permissions.lock() {
                        pending.remove(&permission_id);
                    }
                    PermissionDecision::Deny
                }
            }
        }))
    }

    pub fn pending_permission(&self, permission_id: Uuid) -> Result<PermissionRequest, String> {
        self.pending_permissions
            .lock()
            .map_err(|_| "local agent state poisoned".to_string())?
            .get(&permission_id)
            .map(|pending| pending.request.clone())
            .ok_or_else(|| "permission request not found or already resolved".to_string())
    }

    pub fn load_always_permission_keys(&self, keys: impl IntoIterator<Item = String>) {
        if let Ok(mut permissions) = self.always_permissions.lock() {
            permissions.clear();
            permissions.extend(keys);
        }
    }

    pub fn revoke_always_permission_key(&self, key: &str) {
        if let Ok(mut permissions) = self.always_permissions.lock() {
            permissions.remove(key);
        }
    }

    pub fn has_pending_permission(&self, permission_id: Uuid) -> bool {
        self.pending_permissions
            .lock()
            .map(|pending| pending.contains_key(&permission_id))
            .unwrap_or(false)
    }
}

impl PermissionResolver for InteractivePermissionResolver {
    fn decide(
        &self,
        request: PermissionRequest,
        cancellation: CancellationToken,
    ) -> PermissionFuture {
        let permission_id = request.permission_id;
        let key = permission_key(&request);
        let already_allowed = self
            .always_permissions
            .lock()
            .map(|permissions| permissions.contains(&key))
            .unwrap_or(false);
        if already_allowed {
            return Box::pin(async { PermissionDecision::AllowAlways });
        }
        let session_allowed = self
            .session_permissions
            .lock()
            .map(|permissions| permissions.contains(&key))
            .unwrap_or(false);
        if session_allowed {
            return Box::pin(async { PermissionDecision::AllowSession });
        }
        let (sender, receiver) = oneshot::channel();
        let inserted = self
            .pending_permissions
            .lock()
            .map(|mut pending| {
                pending
                    .insert(permission_id, PendingPermission { request, sender })
                    .is_none()
            })
            .unwrap_or(false);
        if !inserted {
            return Box::pin(async { PermissionDecision::Deny });
        }

        let pending_permissions = Arc::clone(&self.pending_permissions);
        let session_permissions = Arc::clone(&self.session_permissions);
        let always_permissions = Arc::clone(&self.always_permissions);
        Box::pin(async move {
            tokio::select! {
                decision = receiver => {
                    let decision = decision.unwrap_or(PermissionDecision::Deny);
                    match decision {
                        PermissionDecision::AllowSession => {
                            if let Ok(mut permissions) = session_permissions.lock() {
                                permissions.insert(key);
                            }
                        }
                        PermissionDecision::AllowAlways => {
                            if let Ok(mut permissions) = always_permissions.lock() {
                                permissions.insert(key);
                            }
                        }
                        PermissionDecision::AllowOnce | PermissionDecision::Deny => {}
                    }
                    decision
                },
                _ = wait_for_cancellation(cancellation) => {
                    if let Ok(mut pending) = pending_permissions.lock() {
                        pending.remove(&permission_id);
                    }
                    PermissionDecision::Deny
                }
            }
        })
    }
}

pub fn permission_key(request: &PermissionRequest) -> String {
    permission_key_for(&request.tool_id, &request.arguments)
}

pub fn permission_key_for(tool_id: &str, arguments: &serde_json::Value) -> String {
    format!(
        "{}:{}",
        tool_id,
        serde_json::to_string(&canonical_json(arguments)).unwrap_or_default()
    )
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        value => value.clone(),
    }
}

async fn wait_for_cancellation(cancellation: CancellationToken) {
    while !cancellation.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{permission_key_for, LocalAgentState};
    use crate::agent::protocol::{PermissionDecision, PermissionRisk};
    use crate::agent::runtime::{CancellationToken, PermissionRequest};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn permission_keys_are_stable_when_object_fields_are_reordered() {
        let first = permission_key_for("steel.tool", &json!({"b": 2, "a": 1}));
        let second = permission_key_for("steel.tool", &json!({"a": 1, "b": 2}));
        assert_eq!(first, second);
    }

    #[test]
    fn permission_keys_preserve_array_order_and_values() {
        let first = permission_key_for("steel.tool", &json!({"items": [1, 2]}));
        let reordered = permission_key_for("steel.tool", &json!({"items": [2, 1]}));
        let changed = permission_key_for("steel.tool", &json!({"items": [1, 3]}));
        assert_ne!(first, reordered);
        assert_ne!(first, changed);
    }

    #[test]
    fn restored_permission_can_be_resolved_by_the_desktop_command() {
        let state = LocalAgentState::default();
        let permission_id = Uuid::new_v4();
        let future = state
            .restore_permission(
                PermissionRequest {
                    permission_id,
                    tool_call_id: Uuid::new_v4(),
                    tool_id: "steel.write".to_string(),
                    tool_name: "write".to_string(),
                    risk: PermissionRisk::Dangerous,
                    arguments: json!({"value": 1}),
                },
                CancellationToken::new(|| false),
            )
            .expect("permission should be restored");
        state
            .resolve_permission(permission_id, PermissionDecision::AllowOnce)
            .expect("restored permission should accept a decision");
        let decision = tauri::async_runtime::block_on(future);
        assert_eq!(decision, PermissionDecision::AllowOnce);
        assert!(!state.has_pending_permission(permission_id));
    }
}
