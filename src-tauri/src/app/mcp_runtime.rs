use crate::mcp::McpSupervisor;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

pub(crate) type ActiveSupervisor = Arc<AsyncMutex<McpSupervisor>>;

pub(crate) struct McpRuntimeState {
    supervisors: std::sync::Mutex<HashMap<Uuid, ActiveSupervisor>>,
    order: std::sync::Mutex<Vec<Uuid>>,
}

impl Default for McpRuntimeState {
    fn default() -> Self {
        Self {
            supervisors: std::sync::Mutex::new(HashMap::new()),
            order: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl McpRuntimeState {
    pub(crate) fn get(&self, id: Uuid) -> Result<Option<ActiveSupervisor>, String> {
        self.supervisors
            .lock()
            .map(|supervisors| supervisors.get(&id).cloned())
            .map_err(|_| "MCP runtime state poisoned".to_string())
    }

    pub(crate) fn insert(&self, id: Uuid, supervisor: McpSupervisor) -> Result<(), String> {
        let inserted = self
            .supervisors
            .lock()
            .map(|mut supervisors| {
                let inserted = !supervisors.contains_key(&id);
                supervisors.insert(id, Arc::new(AsyncMutex::new(supervisor)));
                inserted
            })
            .map_err(|_| "MCP runtime state poisoned".to_string())?;
        if inserted {
            self.order
                .lock()
                .map(|mut order| order.push(id))
                .map_err(|_| "MCP runtime order state poisoned".to_string())?;
        }
        Ok(())
    }

    pub(crate) fn remove(&self, id: Uuid) -> Result<Option<ActiveSupervisor>, String> {
        let removed = self
            .supervisors
            .lock()
            .map(|mut supervisors| supervisors.remove(&id))
            .map_err(|_| "MCP runtime state poisoned".to_string())?;
        if removed.is_some() {
            self.order
                .lock()
                .map(|mut order| order.retain(|current| *current != id))
                .map_err(|_| "MCP runtime order state poisoned".to_string())?;
        }
        Ok(removed)
    }

    pub(crate) async fn shutdown_all(&self) -> Result<(), String> {
        let ids = self
            .order
            .lock()
            .map(|mut order| {
                let ids = order.iter().rev().copied().collect::<Vec<_>>();
                order.clear();
                ids
            })
            .map_err(|_| "MCP runtime order state poisoned".to_string())?;
        let supervisors = self
            .supervisors
            .lock()
            .map(|mut supervisors| {
                ids.iter()
                    .filter_map(|id| supervisors.remove(id))
                    .collect::<Vec<_>>()
            })
            .map_err(|_| "MCP runtime state poisoned".to_string())?;
        let mut errors = Vec::new();
        for supervisor in supervisors {
            let result = {
                let mut guard = supervisor.lock().await;
                guard.shutdown().await
            };
            if let Err(error) = result {
                errors.push(error.to_string());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("MCP shutdown failures: {}", errors.join("; ")))
        }
    }
}
