use bloomery::agent::runtime::{
    BackgroundTasksTool, CancellationToken, ToolExecutor, ToolInvocation,
};
use bloomery::storage::migrations::migrate;
use bloomery::tasks::{repository, NewTask, TaskState};
use rusqlite::Connection;
use serde_json::{json, Value};
use uuid::Uuid;

fn invocation(arguments: Value) -> ToolInvocation {
    ToolInvocation {
        tool_call_id: Uuid::new_v4(),
        tool_id: "agent.list_background_tasks".to_string(),
        tool_name: "list_background_tasks".to_string(),
        arguments,
    }
}

#[test]
fn background_queries_read_current_commits_and_keep_workspace_and_input_boundaries() {
    let path = std::env::temp_dir().join(format!("bloomery-task-query-{}.sqlite3", Uuid::new_v4()));
    let mut writer = Connection::open(&path).unwrap();
    migrate(&mut writer).unwrap();
    let tool = BackgroundTasksTool::from_connection(&writer, "local").unwrap();
    let query = || {
        tauri::async_runtime::block_on(
            tool.execute(invocation(json!({})), CancellationToken::new(|| false)),
        )
        .unwrap()
    };
    assert_eq!(query()["tasks"], json!([]));

    // Both records are created after the tool: a construction-time snapshot misses them.
    let mut local_id = None;
    for workspace in ["local", "other"] {
        let task = repository::create(
            &writer,
            NewTask {
                workspace_id: workspace.to_string(),
                kind: "demo".to_string(),
                payload_json: r#"{"private":"not part of task listings"}"#.to_string(),
                checkpoint_json: None,
                next_run_at: None,
                progress: 0,
            },
        )
        .unwrap();
        if workspace == "local" {
            local_id = Some(task.id);
        }
    }
    let first = query();
    assert_eq!(first["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(first["tasks"][0]["state"], "queued");
    assert!(first["tasks"][0].get("payload_json").is_none());
    repository::transition(
        &mut writer,
        "local",
        local_id.unwrap(),
        0,
        TaskState::Queued,
        TaskState::Cancelled,
        None,
    )
    .unwrap();
    assert_eq!(query()["tasks"][0]["state"], "cancelled");
    assert_eq!(
        tauri::async_runtime::block_on(tool.execute(
            invocation(json!({"workspace_id": "other"})),
            CancellationToken::new(|| false)
        ))
        .unwrap_err()
        .code,
        "invalid_task_query"
    );
    assert!(
        tauri::async_runtime::block_on(
            tool.execute(invocation(json!({})), CancellationToken::new(|| true))
        )
        .unwrap_err()
        .cancelled
    );
    let mut wrong_tool = invocation(json!({}));
    wrong_tool.tool_id = "different.tool".to_string();
    assert_eq!(
        tauri::async_runtime::block_on(tool.execute(wrong_tool, CancellationToken::new(|| false)))
            .unwrap_err()
            .code,
        "tool_not_registered"
    );

    // Missing storage must fail instead of creating a new empty database.
    drop(writer);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        tauri::async_runtime::block_on(
            tool.execute(invocation(json!({})), CancellationToken::new(|| false))
        )
        .unwrap_err()
        .code,
        "task_query_failed"
    );
    assert!(!path.exists());
}
