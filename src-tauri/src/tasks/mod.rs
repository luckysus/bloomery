pub mod model;
pub mod repository;
pub mod scheduler;

pub use model::{NewTask, TaskError, TaskRecord, TaskState};

pub fn mineru_file_name(task: &TaskRecord) -> Option<String> {
    if task.kind != crate::rag::tasks::MINERU_TASK_KIND {
        return None;
    }
    let payload: serde_json::Value = serde_json::from_str(&task.payload_json).ok()?;
    let file_name = payload.get("file_name")?.as_str()?.trim();
    (!file_name.is_empty() && file_name.len() <= 256).then(|| file_name.to_string())
}
