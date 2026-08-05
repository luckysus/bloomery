use std::collections::HashSet;
use std::sync::Mutex;

#[derive(Default)]
pub struct LocalAgentState {
    cancelled_runs: Mutex<HashSet<String>>,
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
}
