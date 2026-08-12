use super::worker::{WorkerClient, WorkerConfig, WorkerSupervisorError};
use crate::tasks::scheduler::{
    HandlerContext, HandlerError, HandlerFuture, HandlerOutcome, TaskHandler,
};
use crate::tasks::TaskRecord;
use serde::Deserialize;
use serde_json::{json, Value};

pub const COMPUTE_TRAIN_LINEAR_REGRESSION_KIND: &str = "compute_train_linear_regression";
pub const COMPUTE_PREDICT_LINEAR_REGRESSION_KIND: &str = "compute_predict_linear_regression";
pub const COMPUTE_PREDICT_ONNX_KIND: &str = "compute_predict_onnx";
pub const COMPUTE_OPTIMIZE_CONSTRAINED_KIND: &str = "compute_optimize_constrained";
pub const COMPUTE_EXPORT_ONNX_KIND: &str = "compute_export_onnx";
pub const COMPUTE_PREDICT_TRAINED_KIND: &str = "compute_predict_trained_model";
pub const COMPUTE_TRAIN_SKLEARN_KIND: &str = "compute_train_sklearn_model";

pub fn is_training_task_kind(kind: &str) -> bool {
    matches!(
        kind,
        COMPUTE_TRAIN_LINEAR_REGRESSION_KIND | COMPUTE_TRAIN_SKLEARN_KIND
    )
}

pub fn is_prediction_task_kind(kind: &str) -> bool {
    matches!(
        kind,
        COMPUTE_PREDICT_LINEAR_REGRESSION_KIND | COMPUTE_PREDICT_TRAINED_KIND
    )
}

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
        Box::pin(async move { run_task(task, context, worker, "train_linear_regression") })
    }
}

#[derive(Debug, Clone)]
pub struct ComputePredictionTaskHandler {
    worker: Option<WorkerConfig>,
}

impl ComputePredictionTaskHandler {
    pub fn from_optional(worker: Option<WorkerConfig>) -> Self {
        Self { worker }
    }
}

impl TaskHandler for ComputePredictionTaskHandler {
    fn kind(&self) -> &str {
        COMPUTE_PREDICT_LINEAR_REGRESSION_KIND
    }

    fn resumable(&self) -> bool {
        true
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let worker = self.worker.clone();
        Box::pin(async move { run_task(task, context, worker, "predict_linear_regression") })
    }
}

#[derive(Debug, Clone)]
pub struct ComputeOnnxPredictionTaskHandler {
    worker: Option<WorkerConfig>,
}

impl ComputeOnnxPredictionTaskHandler {
    pub fn from_optional(worker: Option<WorkerConfig>) -> Self {
        Self { worker }
    }
}

impl TaskHandler for ComputeOnnxPredictionTaskHandler {
    fn kind(&self) -> &str {
        COMPUTE_PREDICT_ONNX_KIND
    }

    fn resumable(&self) -> bool {
        true
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let worker = self.worker.clone();
        Box::pin(async move { run_task(task, context, worker, "predict_onnx") })
    }
}

#[derive(Debug, Clone)]
pub struct ComputeOptimizationTaskHandler {
    worker: Option<WorkerConfig>,
}

impl ComputeOptimizationTaskHandler {
    pub fn from_optional(worker: Option<WorkerConfig>) -> Self {
        Self { worker }
    }
}

impl TaskHandler for ComputeOptimizationTaskHandler {
    fn kind(&self) -> &str {
        COMPUTE_OPTIMIZE_CONSTRAINED_KIND
    }

    fn resumable(&self) -> bool {
        true
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let worker = self.worker.clone();
        Box::pin(async move { run_task(task, context, worker, "optimize_constrained") })
    }
}

#[derive(Debug, Clone)]
pub struct ComputeExportOnnxTaskHandler {
    worker: Option<WorkerConfig>,
}

impl ComputeExportOnnxTaskHandler {
    pub fn from_optional(worker: Option<WorkerConfig>) -> Self {
        Self { worker }
    }
}

impl TaskHandler for ComputeExportOnnxTaskHandler {
    fn kind(&self) -> &str {
        COMPUTE_EXPORT_ONNX_KIND
    }

    fn resumable(&self) -> bool {
        true
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let worker = self.worker.clone();
        Box::pin(async move { run_task(task, context, worker, "export_linear_onnx") })
    }
}

#[derive(Debug, Clone)]
pub struct ComputeTrainedPredictionTaskHandler {
    worker: Option<WorkerConfig>,
}

impl ComputeTrainedPredictionTaskHandler {
    pub fn from_optional(worker: Option<WorkerConfig>) -> Self {
        Self { worker }
    }
}

impl TaskHandler for ComputeTrainedPredictionTaskHandler {
    fn kind(&self) -> &str {
        COMPUTE_PREDICT_TRAINED_KIND
    }

    fn resumable(&self) -> bool {
        true
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let worker = self.worker.clone();
        Box::pin(async move { run_task(task, context, worker, "predict_trained_model") })
    }
}

#[derive(Debug, Clone)]
pub struct ComputeSklearnTrainingTaskHandler {
    worker: Option<WorkerConfig>,
}

impl ComputeSklearnTrainingTaskHandler {
    pub fn from_optional(worker: Option<WorkerConfig>) -> Self {
        Self { worker }
    }
}

impl TaskHandler for ComputeSklearnTrainingTaskHandler {
    fn kind(&self) -> &str {
        COMPUTE_TRAIN_SKLEARN_KIND
    }

    fn resumable(&self) -> bool {
        true
    }

    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture {
        let worker = self.worker.clone();
        Box::pin(async move { run_task(task, context, worker, "train_sklearn_model") })
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
    expected_operation: &'static str,
) -> Result<HandlerOutcome, HandlerError> {
    let worker = worker.ok_or_else(|| HandlerError::permanent("compute_worker_unavailable"))?;
    let payload: ComputeTaskPayload = serde_json::from_str(&task.payload_json)
        .map_err(|_| HandlerError::permanent("invalid_compute_payload"))?;
    if payload.operation != expected_operation {
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
    let cancellation_context = context.clone();
    let submit = super::protocol::WorkerRequest::new(
        format!("{}-submit-{}", task.id, task.attempt),
        "submit",
        json!({
            "task_id": task_id,
            "operation": payload.operation,
            "payload": payload.payload,
        }),
    );
    let response = match client.request_with_progress_and_cancel(
        &submit,
        move |params| {
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
        },
        move || {
            cancellation_context
                .cancellation_requested()
                .unwrap_or(true)
                || cancellation_context.shutdown_requested()
        },
    ) {
        Ok(response) => response,
        Err(WorkerSupervisorError::Cancelled) => return Ok(HandlerOutcome::Cancelled),
        Err(error) => return Err(map_worker_error(error, "compute_worker_failed")),
    };

    let mut result = response
        .get("result")
        .cloned()
        .ok_or_else(|| HandlerError::permanent("compute_worker_protocol_error"))?;
    let valid_result = if expected_operation == "predict_linear_regression"
        || expected_operation == "predict_onnx"
        || expected_operation == "predict_trained_model"
    {
        result["state"] == "completed" && result.get("predictions").is_some()
    } else if expected_operation == "optimize_constrained" {
        result["state"] == "completed" && result.get("recommendations").is_some()
    } else if expected_operation == "export_linear_onnx" {
        result["state"] == "completed" && result.get("model_base64").is_some()
    } else {
        result["state"] == "completed" && result.get("artifact").is_some()
    };
    if !valid_result {
        return Err(HandlerError::permanent("compute_worker_invalid_result"));
    }
    if expected_operation == "predict_linear_regression" {
        result = annotate_prediction_result(result, &payload.payload)?;
    } else if expected_operation == "predict_onnx" {
        if result.get("model_sha256").and_then(Value::as_str).is_none()
            || result.get("predictions").is_none()
            || result
                .get("opset_version")
                .and_then(Value::as_u64)
                .is_none()
            || !result
                .get("applicability_warnings")
                .map(Value::is_array)
                .unwrap_or(false)
            || !result
                .as_object()
                .map(|object| object.contains_key("confidence"))
                .unwrap_or(false)
        {
            return Err(HandlerError::permanent("compute_worker_invalid_result"));
        }
    } else if expected_operation == "optimize_constrained" {
        validate_optimization_result(&result)?;
    } else if expected_operation == "export_linear_onnx" {
        validate_export_result(&result)?;
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

fn annotate_prediction_result(mut result: Value, payload: &Value) -> Result<Value, HandlerError> {
    let names = payload["artifact"]["feature_names"]
        .as_array()
        .ok_or_else(|| HandlerError::permanent("compute_worker_invalid_model"))?;
    let ranges = payload["artifact"]["applicability_range"]
        .as_array()
        .ok_or_else(|| HandlerError::permanent("compute_worker_invalid_model"))?;
    let input = payload["features"]
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(Value::as_array)
        .ok_or_else(|| HandlerError::permanent("compute_worker_invalid_payload"))?;
    if names.len() != input.len() || ranges.len() != input.len() {
        return Err(HandlerError::permanent("compute_worker_invalid_model"));
    }

    let mut warnings = Vec::new();
    for (index, value) in input.iter().enumerate() {
        let Some(number) = value.as_f64() else {
            continue;
        };
        let range = &ranges[index];
        let min = range.get("min").and_then(Value::as_f64);
        let max = range.get("max").and_then(Value::as_f64);
        if min.is_some_and(|bound| number < bound) || max.is_some_and(|bound| number > bound) {
            warnings.push(json!({
                "feature": names[index],
                "index": index,
                "value": number,
                "min": min,
                "max": max,
                "code": "outside_applicability_range",
            }));
        }
    }
    result["input_values"] = Value::Array(input.clone());
    result["applicability_range"] = Value::Array(ranges.clone());
    result["applicability_warnings"] = Value::Array(warnings);
    result["confidence"] = Value::Null;
    result["constraints"] = json!([]);
    Ok(result)
}

fn validate_optimization_result(result: &Value) -> Result<(), HandlerError> {
    if result.get("method").and_then(Value::as_str).is_none()
        || result.get("direction").and_then(Value::as_str).is_none()
        || result.get("model_id").and_then(Value::as_str).is_none()
        || result
            .get("trials_completed")
            .and_then(Value::as_u64)
            .is_none()
        || !result
            .as_object()
            .map(|object| object.contains_key("deterministic_seed"))
            .unwrap_or(false)
    {
        return Err(HandlerError::permanent("compute_worker_invalid_result"));
    }
    let recommendations = result
        .get("recommendations")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| HandlerError::permanent("compute_worker_invalid_result"))?;
    for recommendation in recommendations {
        if !recommendation
            .get("values")
            .map(Value::is_object)
            .unwrap_or(false)
            || !recommendation
                .get("objectives")
                .map(Value::is_array)
                .unwrap_or(false)
            || recommendation
                .get("prediction")
                .and_then(Value::as_f64)
                .is_none()
            || recommendation.get("feasible").and_then(Value::as_bool) != Some(true)
        {
            return Err(HandlerError::permanent("compute_worker_invalid_result"));
        }
    }
    Ok(())
}

fn validate_export_result(result: &Value) -> Result<(), HandlerError> {
    let model = result
        .get("model_base64")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| HandlerError::permanent("compute_worker_invalid_result"))?;
    if !model.len().is_multiple_of(4)
        || !model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
    {
        return Err(HandlerError::permanent("compute_worker_invalid_result"));
    }
    if result
        .get("model_sha256")
        .and_then(Value::as_str)
        .map(str::len)
        != Some(64)
    {
        return Err(HandlerError::permanent("compute_worker_invalid_result"));
    }
    if !result
        .get("manifest")
        .map(Value::is_object)
        .unwrap_or(false)
        || !result
            .get("operators")
            .map(Value::is_array)
            .unwrap_or(false)
        || result
            .get("opset_version")
            .and_then(Value::as_u64)
            .is_none()
    {
        return Err(HandlerError::permanent("compute_worker_invalid_result"));
    }
    Ok(())
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
