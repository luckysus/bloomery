use crate::agent::protocol::PermissionDecision;
use crate::agent::runtime::{CancellationToken, PermissionFuture, PermissionRequest, PermissionResolver};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct LocalAgentState {
    cancelled_runs: Arc<Mutex<HashSet<String>>>,
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
            permissions.extend(keys);
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
                    .insert(
                        permission_id,
                        PendingPermission {
                            request,
                            sender,
                        },
                    )
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
        serde_json::to_string(arguments).unwrap_or_default()
    )
}

async fn wait_for_cancellation(cancellation: CancellationToken) {
    while !cancellation.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
