use super::worker::{WorkerClient, WorkerConfig, WorkerSupervisorError};
use crate::tasks::scheduler::{
    HandlerContext, HandlerError, HandlerFuture, HandlerOutcome, TaskHandler,
};
use crate::tasks::TaskRecord;
use serde::Deserialize;
use serde_json::{json, Value};

pub const COMPUTE_TRAIN_LINEAR_REGRESSION_KIND: &str = "compute_train_linear_regression";

#[derive(Debug, Clone)]
pub struct ComputeTaskHandler {
    worker: Option<WorkerConfig>,
}

impl ComputeTaskHandler {
    pub fn new(worker: WorkerConfig) -> Self {
        Self {
            worker: Some(worker),
        }
    }

    pub fn unavailable() -> Self {
        Self { worker: None }
    }

    pub fn from_optional(worker: Option<WorkerConfig>) -> Self {
        Self { worker }
    }
}

impl TaskHandler for ComputeTaskHandler {
    fn kind(&self) -> &str {
        COMPUTE_TRAIN_LINEAR_REGRESSION_KIND
    }

    fn resumable(&self) -> bool {
        true
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let worker = self.worker.clone();
        Box::pin(async move { run_task(task, context, worker) })
    }
}

#[derive(Debug, Deserialize)]
struct ComputeTaskPayload {
    operation: String,
    payload: Value,
}

fn run_task(
    task: TaskRecord,
    context: HandlerContext,
    worker: Option<WorkerConfig>,
) -> Result<HandlerOutcome, HandlerError> {
    let worker = worker.ok_or_else(|| HandlerError::permanent("compute_worker_unavailable"))?;
    let payload: ComputeTaskPayload = serde_json::from_str(&task.payload_json)
        .map_err(|_| HandlerError::permanent("invalid_compute_payload"))?;
    if payload.operation != "train_linear_regression" {
        return Err(HandlerError::permanent("unsupported_compute_operation"));
    }
    if !payload.payload.is_object() {
        return Err(HandlerError::permanent("invalid_compute_payload"));
    }

    let mut client = WorkerClient::spawn(worker)
        .map_err(|error| map_worker_error(error, "compute_worker_unavailable"))?;
    let hello = client
        .request(&super::protocol::WorkerRequest::new(
            format!("{}-hello-{}", task.id, task.attempt),
            "hello",
            json!({}),
        ))
        .map_err(|error| map_worker_error(error, "compute_worker_protocol_error"))?;
    let operations = hello["result"]["operations"]
        .as_array()
        .ok_or_else(|| HandlerError::permanent("compute_worker_protocol_error"))?;
    if !operations
        .iter()
        .any(|operation| operation.as_str() == Some(payload.operation.as_str()))
    {
        return Err(HandlerError::permanent("unsupported_compute_operation"));
    }

    let task_id = task.id.to_string();
    let checkpoint_context = context.clone();
    let submit = super::protocol::WorkerRequest::new(
        format!("{}-submit-{}", task.id, task.attempt),
        "submit",
        json!({
            "task_id": task_id,
            "operation": payload.operation,
            "payload": payload.payload,
        }),
    );
    let response = match client.request_with_progress(&submit, move |params| {
        let progress = params
            .get("progress")
            .and_then(Value::as_u64)
            .and_then(|value| u8::try_from(value).ok())
            .filter(|value| *value <= 100)
            .ok_or_else(|| {
                WorkerSupervisorError::Protocol(super::protocol::FrameError::InvalidRequest(
                    "worker progress must be an integer between 0 and 100".to_string(),
                ))
            })?;
        if checkpoint_context
            .cancellation_requested()
            .map_err(|error| WorkerSupervisorError::Callback(error.to_string()))?
        {
            return Err(WorkerSupervisorError::Cancelled);
        }
        if checkpoint_context.shutdown_requested() {
            return Err(WorkerSupervisorError::Cancelled);
        }
        let stage = params
            .get("stage")
            .and_then(Value::as_str)
            .unwrap_or("running");
        let checkpoint = json!({
            "stage": stage,
            "worker_progress": progress,
        });
        let checkpoint_json = serde_json::to_string(&checkpoint)
            .map_err(|error| WorkerSupervisorError::Callback(error.to_string()))?;
        checkpoint_context
            .checkpoint(Some(&checkpoint_json), progress, None)
            .map_err(|error| WorkerSupervisorError::Callback(error.to_string()))?;
        Ok(())
    }) {
        Ok(response) => response,
        Err(WorkerSupervisorError::Cancelled) => return Ok(HandlerOutcome::Cancelled),
        Err(error) => return Err(map_worker_error(error, "compute_worker_failed")),
    };

    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| HandlerError::permanent("compute_worker_protocol_error"))?;
    if result["state"] != "completed" || result.get("artifact").is_none() {
        return Err(HandlerError::permanent("compute_worker_invalid_result"));
    }
    let checkpoint = json!({"stage": "completed", "result": result});
    let checkpoint_json = serde_json::to_string(&checkpoint)
        .map_err(|_| HandlerError::permanent("compute_result_encode_failed"))?;
    context
        .checkpoint(Some(&checkpoint_json), 100, None)
        .map_err(|_| HandlerError::permanent("compute_checkpoint_failed"))?;

    client
        .shutdown(&super::protocol::WorkerRequest::new(
            format!("{}-shutdown-{}", task.id, task.attempt),
            "shutdown",
            json!({}),
        ))
        .map_err(|error| map_worker_error(error, "compute_worker_shutdown_failed"))?;
    Ok(HandlerOutcome::Completed)
}

fn map_worker_error(error: WorkerSupervisorError, fallback: &'static str) -> HandlerError {
    match error {
        WorkerSupervisorError::Remote { code, .. } if is_safe_error_code(&code) => {
            HandlerError::permanent(format!("compute_worker_{code}"))
        }
        WorkerSupervisorError::Remote { .. }
        | WorkerSupervisorError::InvalidConfig(_)
        | WorkerSupervisorError::Io(_)
        | WorkerSupervisorError::Protocol(_)
        | WorkerSupervisorError::Callback(_)
        | WorkerSupervisorError::WorkerExited => HandlerError::permanent(fallback),
        WorkerSupervisorError::Cancelled => HandlerError::permanent("compute_cancelled"),
    }
}

fn is_safe_error_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}
