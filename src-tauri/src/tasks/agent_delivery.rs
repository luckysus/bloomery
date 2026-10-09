use super::{repository, TaskState};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TaskDelivery {
    pub task_id: Uuid,
    pub run_id: Uuid,
    pub tool_call_id: Uuid,
    pub kind: String,
    pub state: TaskState,
    pub progress: u8,
    pub error_code: Option<String>,
    pub result: Value,
}

impl TaskDelivery {
    pub fn message(&self) -> String {
        json!({
            "background_task_id": self.task_id,
            "source_run_id": self.run_id,
            "source_tool_call_id": self.tool_call_id,
            "kind": self.kind,
            "state": self.state,
            "success": self.state == TaskState::Completed,
            "progress": self.progress,
            "error_code": self.error_code,
            "result": self.result,
            "instruction": "这是刚才提交的后台任务的最终结果。继续原用户请求，基于结果完成分析和后续步骤。失败、暂停或取消时请明确反馈，不要把它当成成功。"
        }).to_string()
    }
}

pub fn is_terminal(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::Completed
            | TaskState::Failed
            | TaskState::Cancelled
            | TaskState::Interrupted
            | TaskState::Paused
    )
}

/// The outbox is the existing source row. A cancelled/finished original run
/// never causes an unrelated turn to be started by a late task result.
pub fn pending_for_run(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
) -> Result<Vec<TaskDelivery>, String> {
    let mut statement = connection
        .prepare(
            "SELECT s.task_id, s.tool_call_id FROM agent_task_sources s
         JOIN agent_runs r ON r.workspace_id = s.workspace_id AND r.id = s.run_id
         WHERE s.workspace_id = ?1 AND s.run_id = ?2 AND s.delivered_at IS NULL
           AND r.state NOT IN ('completed', 'cancelled', 'failed', 'interrupted')
         ORDER BY s.rowid",
        )
        .map_err(|error| error.to_string())?;
    let sources = statement
        .query_map(params![workspace_id, run_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    sources
        .into_iter()
        .map(|(task_id, tool_call_id)| {
            let task_id = Uuid::parse_str(&task_id).map_err(|error| error.to_string())?;
            let task = repository::get(connection, workspace_id, task_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "linked task disappeared".to_string())?;
            let checkpoint = task
                .checkpoint_json
                .as_deref()
                .map(serde_json::from_str::<Value>)
                .transpose()
                .map_err(|error| format!("invalid background result: {error}"))?
                .unwrap_or(Value::Null);
            let mut summary_only = false;
            let result = compact_result(
                checkpoint.get("result").cloned().unwrap_or(checkpoint),
                &mut summary_only,
            );
            let result = crate::diagnostics::observability::redact_json(&result);
            let result = bounded_result(result, task_id, summary_only);
            Ok(TaskDelivery {
                task_id,
                run_id,
                tool_call_id: Uuid::parse_str(&tool_call_id).map_err(|error| error.to_string())?,
                kind: task.kind,
                state: task.state,
                progress: task.progress,
                error_code: task.error_code,
                result,
            })
        })
        .collect()
}

pub fn ready_for_run(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
) -> Result<Vec<TaskDelivery>, String> {
    Ok(pending_for_run(connection, workspace_id, run_id)?
        .into_iter()
        .filter(|delivery| is_terminal(delivery.state))
        .collect())
}

/// Call inside the same transaction that persists the receiving context
/// checkpoint. Reading a result alone must never acknowledge its delivery.
pub fn acknowledge(
    connection: &Connection,
    workspace_id: &str,
    run_id: Uuid,
    task_ids: &[Uuid],
) -> Result<(), String> {
    for task_id in task_ids {
        connection.execute(
            "UPDATE agent_task_sources SET delivered_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE workspace_id = ?1 AND run_id = ?2 AND task_id = ?3 AND delivered_at IS NULL
               AND EXISTS (SELECT 1 FROM background_tasks t WHERE t.workspace_id = ?1
                   AND t.id = ?3 AND t.state IN ('completed','failed','cancelled','paused','interrupted'))",
            params![workspace_id, run_id.to_string(), task_id.to_string()],
        ).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn compact_result(value: Value, summary_only: &mut bool) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .filter(|(key, _)| !key.ends_with("_base64"))
                .map(|(key, value)| (key, compact_result(value, summary_only)))
                .collect(),
        ),
        Value::Array(values) => {
            *summary_only |= values.len() > 100;
            Value::Array(
                values
                    .into_iter()
                    .take(100)
                    .map(|value| compact_result(value, summary_only))
                    .collect(),
            )
        }
        Value::String(text) if text.len() > 16_000 => {
            *summary_only = true;
            Value::String(text.chars().take(8_000).collect())
        }
        value => value,
    }
}

fn bounded_result(mut result: Value, task_id: Uuid, summary_only: bool) -> Value {
    if result.to_string().len() <= 24 * 1024 {
        if summary_only {
            if let Some(fields) = result.as_object_mut() {
                fields.insert("delivery_summary_only".into(), json!(true));
                fields.insert("full_result_task_id".into(), json!(task_id));
            }
        }
        return result;
    }
    let mut summary = serde_json::Map::new();
    summary.insert("delivery_summary_only".into(), json!(true));
    summary.insert("full_result_task_id".into(), json!(task_id));
    let mut used = 512;
    let mut omitted = Vec::new();
    if let Value::Object(fields) = result {
        let priority = [
            "state",
            "metrics",
            "warnings",
            "validation",
            "algorithm",
            "method",
            "predictions",
            "recommendations",
        ];
        let mut fields = fields.into_iter().collect::<Vec<_>>();
        fields.sort_by_key(|(key, _)| {
            priority
                .iter()
                .position(|name| name == key)
                .unwrap_or(priority.len())
        });
        for (key, value) in fields {
            let bytes = value.to_string().len() + key.len() + 8;
            if used + bytes < 22 * 1024 {
                used += bytes;
                summary.insert(key, value);
            } else {
                omitted.push(key);
            }
        }
    }
    summary.insert("omitted_field_count".into(), json!(omitted.len()));
    summary.insert(
        "omitted_fields".into(),
        json!(omitted.into_iter().take(30).collect::<Vec<_>>()),
    );
    Value::Object(summary)
}
