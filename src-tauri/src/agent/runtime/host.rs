use super::{AgentInputKind, AgentInputQueue, AgentLoopLimits, CancellationToken};
use crate::agent::desktop::LocalAgentState;
use crate::agent::runtime::{PermissionRequest, PermissionResolver, ToolExecutor};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Immutable configuration captured when a foreground turn starts.
///
/// The host owns this snapshot so later provider, tool, or permission changes
/// cannot silently alter an already running turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnSnapshot {
    pub turn_id: Uuid,
    pub session_id: Uuid,
    #[serde(default)]
    pub parent_turn_id: Option<Uuid>,
    #[serde(default = "default_child_turn_limit")]
    pub child_turn_limit: usize,
    pub provider: String,
    pub model: String,
    pub model_context_window: Option<usize>,
    pub output_reservation: usize,
    pub limits: AgentLoopLimits,
    pub tool_ids: Vec<String>,
    pub tool_snapshot: Vec<ToolSnapshotEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSnapshotEntry {
    pub id: String,
    pub name: String,
    pub version: crate::tools::ToolVersion,
    pub source: crate::tools::ToolSource,
    pub risk: crate::agent::protocol::PermissionRisk,
    pub read_only: bool,
    pub concurrency: crate::tools::ConcurrencyPolicy,
    pub timeout_ms: u64,
    pub idempotent: bool,
    pub retryable: bool,
    pub input_schema_json: String,
}

#[derive(Debug, Clone)]
pub struct TurnHandle {
    pub snapshot: TurnSnapshot,
    pub input_queue: AgentInputQueue,
}

struct ActiveTurn {
    snapshot: TurnSnapshot,
}

fn default_child_turn_limit() -> usize {
    4
}

/// Application-owned runtime boundary for all interactive Agent Turns.
///
/// `AgentLoop` remains a pure execution kernel. This host owns the process
/// state that must survive between Tauri commands: cancellation, input queues,
/// interactive permissions, and immutable Turn snapshots.
#[derive(Clone, Default)]
pub struct RuntimeHost {
    local: LocalAgentState,
    active: Arc<Mutex<HashMap<Uuid, ActiveTurn>>>,
    child_events: Arc<Mutex<HashMap<Uuid, Vec<crate::agent::protocol::AgentEventEnvelope>>>>,
}

impl RuntimeHost {
    pub fn begin_turn(&self, snapshot: TurnSnapshot) -> Result<TurnHandle, String> {
        if snapshot.turn_id.is_nil() {
            return Err("turn_id is required".to_string());
        }
        if snapshot.session_id.is_nil() {
            return Err("session_id is required".to_string());
        }
        if snapshot.child_turn_limit == 0 {
            return Err("child_turn_limit must be greater than zero".to_string());
        }
        snapshot.limits.validate()?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| "agent runtime host is poisoned".to_string())?;
        if active.contains_key(&snapshot.turn_id) {
            return Err("turn is already active".to_string());
        }
        if let Some(parent_id) = snapshot.parent_turn_id {
            let parent = active
                .get(&parent_id)
                .ok_or_else(|| "parent turn is not active".to_string())?;
            if parent.snapshot.session_id != snapshot.session_id {
                return Err("child turn must use the parent session".to_string());
            }
            let child_count = active
                .values()
                .filter(|turn| turn.snapshot.parent_turn_id == Some(parent_id))
                .count();
            if child_count >= parent.snapshot.child_turn_limit {
                return Err("parent child turn limit exceeded".to_string());
            }
        } else if active.values().any(|turn| {
            turn.snapshot.session_id == snapshot.session_id
                && turn.snapshot.parent_turn_id.is_none()
        }) {
            return Err("session already has an active turn".to_string());
        }
        let queue = self
            .local
            .register_input_queue(&snapshot.turn_id.to_string())?;
        active.insert(
            snapshot.turn_id,
            ActiveTurn {
                snapshot: snapshot.clone(),
            },
        );
        Ok(TurnHandle {
            snapshot,
            input_queue: queue,
        })
    }

    pub fn set_tool_snapshot(
        &self,
        turn_id: Uuid,
        tools: &dyn ToolExecutor,
    ) -> Result<TurnSnapshot, String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "agent runtime host is poisoned".to_string())?;
        let turn = active
            .get_mut(&turn_id)
            .ok_or_else(|| "agent turn is not active".to_string())?;
        let runtime_snapshot = tools.snapshot()?;
        let mut snapshot = runtime_snapshot
            .registrations()
            .iter()
            .map(|registration| ToolSnapshotEntry {
                id: registration.spec.id.clone(),
                name: registration.spec.name.clone(),
                version: registration.version,
                source: registration.source.clone(),
                risk: registration.spec.risk,
                read_only: registration.read_only,
                concurrency: registration.concurrency,
                timeout_ms: registration.timeout.as_millis().min(u64::MAX as u128) as u64,
                idempotent: registration.idempotent,
                retryable: registration.retryable,
                input_schema_json: serde_json::to_string(&registration.spec.input_schema)
                    .unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        snapshot.sort_by(|left, right| left.id.cmp(&right.id));
        turn.snapshot.tool_ids = snapshot.iter().map(|tool| tool.id.clone()).collect();
        turn.snapshot.tool_snapshot = snapshot;
        Ok(turn.snapshot.clone())
    }

    pub fn finish_turn(&self, turn_id: Uuid) {
        if let Ok(mut active) = self.active.lock() {
            let children = active
                .values()
                .filter(|turn| turn.snapshot.parent_turn_id == Some(turn_id))
                .map(|turn| turn.snapshot.turn_id)
                .collect::<Vec<_>>();
            for child_id in children {
                let _ = self.local.cancel_run(&child_id.to_string());
                active.remove(&child_id);
                self.local.remove_input_queue(&child_id.to_string());
                self.local.clear_cancelled(&child_id.to_string());
            }
            active.remove(&turn_id);
        }
        self.local.remove_input_queue(&turn_id.to_string());
        self.local.clear_cancelled(&turn_id.to_string());
    }

    pub fn cancel_turn(&self, turn_id: Uuid) -> Result<(), String> {
        let mut ids = BTreeSet::from([turn_id]);
        if let Ok(active) = self.active.lock() {
            let mut changed = true;
            while changed {
                changed = false;
                for (candidate_id, candidate) in active.iter() {
                    if let Some(parent_id) = candidate.snapshot.parent_turn_id {
                        if ids.contains(&parent_id) && ids.insert(*candidate_id) {
                            changed = true;
                        }
                    }
                }
            }
        }
        for id in ids {
            self.local.cancel_run(&id.to_string())?;
        }
        Ok(())
    }

    pub fn clear_cancelled(&self, turn_id: &str) {
        self.local.clear_cancelled(turn_id);
    }

    pub fn is_cancelled(&self, turn_id: &str) -> Result<bool, String> {
        self.local.is_cancelled(turn_id)
    }

    pub fn cancellation_token(&self, turn_id: Uuid) -> CancellationToken {
        self.local.cancellation_token(&turn_id.to_string())
    }

    pub fn enqueue_input(
        &self,
        turn_id: Uuid,
        kind: AgentInputKind,
        message: String,
    ) -> Result<(), String> {
        self.local
            .enqueue_input(&turn_id.to_string(), kind, message)
    }

    pub fn permission_resolver(&self) -> impl PermissionResolver {
        self.local.permission_resolver()
    }

    pub fn resolve_permission(
        &self,
        permission_id: Uuid,
        decision: crate::agent::protocol::PermissionDecision,
    ) -> Result<PermissionRequest, String> {
        self.local.resolve_permission(permission_id, decision)
    }

    pub fn restore_permission(
        &self,
        request: PermissionRequest,
        cancellation: CancellationToken,
    ) -> Result<crate::agent::runtime::PermissionFuture, String> {
        self.local.restore_permission(request, cancellation)
    }

    pub fn pending_permission(&self, permission_id: Uuid) -> Result<PermissionRequest, String> {
        self.local.pending_permission(permission_id)
    }

    pub fn has_pending_permission(&self, permission_id: Uuid) -> bool {
        self.local.has_pending_permission(permission_id)
    }

    pub fn load_always_permission_keys(&self, keys: impl IntoIterator<Item = String>) {
        self.local.load_always_permission_keys(keys);
    }

    pub fn revoke_always_permission_key(&self, key: &str) {
        self.local.revoke_always_permission_key(key);
    }

    pub fn snapshot(&self, turn_id: Uuid) -> Result<TurnSnapshot, String> {
        self.active
            .lock()
            .map_err(|_| "agent runtime host is poisoned".to_string())?
            .get(&turn_id)
            .map(|turn| turn.snapshot.clone())
            .ok_or_else(|| "agent turn is not active".to_string())
    }

    pub fn active_turns(&self) -> Result<Vec<TurnSnapshot>, String> {
        Ok(self
            .active
            .lock()
            .map_err(|_| "agent runtime host is poisoned".to_string())?
            .values()
            .map(|turn| turn.snapshot.clone())
            .collect())
    }

    pub fn active_children(&self, parent_turn_id: Uuid) -> Result<Vec<TurnSnapshot>, String> {
        Ok(self
            .active
            .lock()
            .map_err(|_| "agent runtime host is poisoned".to_string())?
            .values()
            .filter(|turn| turn.snapshot.parent_turn_id == Some(parent_turn_id))
            .map(|turn| turn.snapshot.clone())
            .collect())
    }

    pub fn record_child_events(
        &self,
        turn_id: Uuid,
        events: Vec<crate::agent::protocol::AgentEventEnvelope>,
    ) -> Result<(), String> {
        if events.is_empty() {
            return Ok(());
        }
        let mut stored = self
            .child_events
            .lock()
            .map_err(|_| "agent runtime host is poisoned".to_string())?;
        let history = stored.entry(turn_id).or_default();
        history.extend(events);
        const MAX_CHILD_EVENTS: usize = 512;
        if history.len() > MAX_CHILD_EVENTS {
            let remove = history.len() - MAX_CHILD_EVENTS;
            history.drain(..remove);
        }
        Ok(())
    }

    pub fn child_events(
        &self,
        turn_id: Uuid,
    ) -> Result<Vec<crate::agent::protocol::AgentEventEnvelope>, String> {
        Ok(self
            .child_events
            .lock()
            .map_err(|_| "agent runtime host is poisoned".to_string())?
            .get(&turn_id)
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
#[path = "../runtime_host_tests.rs"]
mod tests;
