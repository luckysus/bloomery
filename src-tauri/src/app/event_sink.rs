use crate::tasks::scheduler::{EventSink, SchedulerEvent};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone)]
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: SchedulerEvent) {
        // Emit task progress events to the frontend (no-op if window closed)
        let _ = self.app.emit("scheduler:progress", &event);
    }
}
