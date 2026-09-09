use super::{
    CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation,
    AgentHooks, ToolRegistration,
};
use crate::agent::protocol::PermissionRisk;
use crate::agent::tool_repair::ToolSpec;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const MAX_TODOS: usize = 50;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TodoWriteRequest {
    #[serde(deserialize_with = "deserialize_todos")]
    todos: Vec<TodoItem>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct TodoItem {
    content: String,
    status: TodoStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

fn deserialize_todos<'de, D>(deserializer: D) -> Result<Vec<TodoItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let todos = Vec::<TodoItem>::deserialize(deserializer)?;
    if todos.len() > MAX_TODOS {
        return Err(serde::de::Error::custom(format!(
            "at most {MAX_TODOS} TODO items are allowed"
        )));
    }
    if todos.iter().any(|item| item.content.trim().is_empty()) {
        return Err(serde::de::Error::custom("todo content must not be empty"));
    }
    Ok(todos
        .into_iter()
        .map(|mut item| {
            item.content = item.content.trim().to_string();
            item
        })
        .collect())
}

#[derive(Clone)]
pub struct TodoTracker {
    todos: Arc<Mutex<Vec<TodoItem>>>,
    rounds_without_update: Arc<Mutex<usize>>,
    registration: ToolRegistration,
}

impl TodoTracker {
    pub fn new() -> Self {
        let todos = Arc::new(Mutex::new(Vec::new()));
        let registration = ToolRegistration::new(
            ToolSpec {
                id: "agent.todo_write".to_string(),
                name: "todo_write".to_string(),
                input_schema: todo_schema(),
                risk: PermissionRisk::Automatic,
            },
            true,
            Arc::new(TodoHandler {
                todos: Arc::clone(&todos),
            }),
        );
        Self {
            todos,
            rounds_without_update: Arc::new(Mutex::new(0)),
            registration,
        }
    }

    pub fn tool(&self) -> ToolRegistration {
        self.registration.clone()
    }

    pub fn snapshot(&self) -> Vec<Value> {
        self.todos
            .lock()
            .map(|todos| todos.iter().map(|todo| json!(todo)).collect())
            .unwrap_or_default()
    }
}

impl ToolExecutor for TodoTracker {
    fn registrations(&self) -> &[ToolRegistration] {
        std::slice::from_ref(&self.registration)
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        if invocation.tool_id != "agent.todo_write" || invocation.tool_name != "todo_write" {
            return Box::pin(async {
                Err(ToolExecutionError::new("tool_not_registered", "TODO tool is not registered"))
            });
        }
        let handler = TodoHandler {
            todos: Arc::clone(&self.todos),
        };
        handler.execute(invocation.arguments, cancellation)
    }
}

impl AgentHooks for TodoTracker {
    fn before_model(&self) -> Option<crate::providers::capabilities::ChatMessage> {
        let mut rounds = self.rounds_without_update.lock().ok()?;
        if *rounds < 3 {
            return None;
        }
        *rounds = 0;
        Some(crate::providers::capabilities::ChatMessage::new(
            "system",
            "Keep the TODO list current. Call todo_write with the complete task snapshot when the plan changes.",
        ))
    }

    fn after_tool_round(&self, tool_names: &[String]) {
        if let Ok(mut rounds) = self.rounds_without_update.lock() {
            if tool_names.iter().any(|name| name == "todo_write") {
                *rounds = 0;
            } else if !tool_names.is_empty() {
                *rounds += 1;
            }
        }
    }
}

impl Default for TodoTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn todo_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "todos": {
                "type": "array",
                "maxItems": MAX_TODOS,
                "items": {
                    "type": "object",
                    "properties": {
                        "content": {"type": "string", "minLength": 1},
                        "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]}
                    },
                    "required": ["content", "status"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["todos"],
        "additionalProperties": false
    })
}

struct TodoHandler {
    todos: Arc<Mutex<Vec<TodoItem>>>,
}

impl ToolHandler for TodoHandler {
    fn execute(&self, arguments: Value, _cancellation: CancellationToken) -> ToolFuture {
        let todos = Arc::clone(&self.todos);
        Box::pin(async move {
            let request = serde_json::from_value::<TodoWriteRequest>(arguments)
                .map_err(|error| ToolExecutionError::new("invalid_todo", error.to_string()))?;
            let snapshot = request.todos;
            let output = json!({"todos": snapshot});
            todos
                .lock()
                .map_err(|_| ToolExecutionError::new("todo_state", "todo state is unavailable"))?
                .clone_from(&serde_json::from_value(output["todos"].clone()).map_err(|error| {
                    ToolExecutionError::new("todo_state", error.to_string())
                })?);
            Ok(json!({"todos": output["todos"].clone()}))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_exposes_strict_bounded_todo_schema() {
        let tracker = TodoTracker::default();
        let tool = tracker.tool();
        assert_eq!(tool.spec.name, "todo_write");
        assert_eq!(tool.spec.input_schema["properties"]["todos"]["maxItems"], MAX_TODOS);
    }

    #[test]
    fn tracker_replaces_snapshot_and_emits_one_shot_stale_reminder() {
        let tracker = TodoTracker::default();
        let handler = TodoHandler {
            todos: Arc::clone(&tracker.todos),
        };
        let result = tauri::async_runtime::block_on(handler.execute(
            json!({"todos": [{"content": "  inspect data  ", "status": "in_progress"}]}),
            CancellationToken::new(|| false),
        ))
        .expect("todo write succeeds");
        assert_eq!(result, json!({"todos": [{"content": "inspect data", "status": "in_progress"}]}));
        assert_eq!(tracker.snapshot(), vec![json!({"content": "inspect data", "status": "in_progress"})]);

        for _ in 0..3 {
            tracker.after_tool_round(&["search".to_string()]);
        }
        assert!(tracker.before_model().is_some());
        assert!(tracker.before_model().is_none());
    }

    #[test]
    fn tracker_rejects_empty_content_and_unknown_fields() {
        let tracker = TodoTracker::default();
        let handler = TodoHandler {
            todos: Arc::clone(&tracker.todos),
        };
        let empty = tauri::async_runtime::block_on(handler.execute(
            json!({"todos": [{"content": " ", "status": "pending"}]}),
            CancellationToken::new(|| false),
        ));
        assert!(empty.is_err());
        let extra = tauri::async_runtime::block_on(handler.execute(
            json!({"todos": [{"content": "ok", "status": "pending", "extra": true}]}),
            CancellationToken::new(|| false),
        ));
        assert!(extra.is_err());
    }
}
