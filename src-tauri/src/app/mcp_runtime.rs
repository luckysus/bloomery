use crate::mcp::McpSupervisor;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

pub(crate) type ActiveSupervisor = Arc<AsyncMutex<McpSupervisor>>;

pub(crate) struct McpRuntimeState {
    supervisors: std::sync::Mutex<HashMap<Uuid, ActiveSupervisor>>,
}

impl Default for McpRuntimeState {
    fn default() -> Self {
        Self {
            supervisors: std::sync::Mutex::new(HashMap::new()),
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
        self.supervisors
            .lock()
            .map(|mut supervisors| {
                supervisors.insert(id, Arc::new(AsyncMutex::new(supervisor)));
            })
            .map_err(|_| "MCP runtime state poisoned".to_string())
    }

    pub(crate) fn remove(&self, id: Uuid) -> Result<Option<ActiveSupervisor>, String> {
        self.supervisors
            .lock()
            .map(|mut supervisors| supervisors.remove(&id))
            .map_err(|_| "MCP runtime state poisoned".to_string())
    }

    pub(crate) async fn shutdown_all(&self) -> Result<(), String> {
        let supervisors = self
            .supervisors
            .lock()
            .map(|mut supervisors| {
                supervisors
                    .drain()
                    .map(|(_, value)| value)
                    .collect::<Vec<_>>()
            })
            .map_err(|_| "MCP runtime state poisoned".to_string())?;
        let mut first_error = None;
        for supervisor in supervisors {
            let result = {
                let mut guard = supervisor.lock().await;
                guard.shutdown().await
            };
            if let Err(error) = result {
                first_error.get_or_insert_with(|| error.to_string());
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}
