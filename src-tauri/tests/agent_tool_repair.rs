use bloomery::agent::protocol::PermissionRisk;
use bloomery::agent::tool_repair::{
    repair_tool_call, repair_tool_call_with_retries, ToolRepairError, ToolSpec, MAX_REPAIR_RETRIES,
};
use serde_json::{json, Value};

fn read_file_spec() -> ToolSpec {
    ToolSpec {
        id: "builtin.read_file.v1".to_string(),
        name: "read_file".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {"path": {"type": "string", "minLength": 1}},
            "required": ["path"],
            "additionalProperties": false
        }),
        risk: PermissionRisk::ConfirmationRequired,
    }
}

fn shell_spec() -> ToolSpec {
    ToolSpec {
        id: "builtin.shell.v1".to_string(),
        name: "shell".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "minLength": 1},
                "destructive": {"type": "boolean"}
            },
            "required": ["command"],
            "additionalProperties": false
        }),
        risk: PermissionRisk::Dangerous,
    }
}

fn registry() -> Vec<ToolSpec> {
    vec![read_file_spec(), shell_spec()]
}

fn call(name: &str, arguments: Value) -> String {
    serde_json::to_string(&json!({"name": name, "arguments": arguments})).unwrap()
}

#[test]
fn clean_json_extracts_name_and_typed_arguments() {
    let repaired = repair_tool_call(
        &call("read_file", json!({"path": "manual.txt"})),
        &registry(),
    )
    .unwrap();

    assert_eq!(repaired.tool_id, "builtin.read_file.v1");
    assert_eq!(repaired.tool_name, "read_file");
    assert_eq!(repaired.arguments, json!({"path": "manual.txt"}));
    assert_eq!(repaired.risk, PermissionRisk::ConfirmationRequired);
    assert!(repaired.syntax_repairs.is_empty());
}

#[test]
fn fenced_json_is_unwrapped_without_accepting_surrounding_text() {
    let repaired = repair_tool_call(
        "```json\n{\"name\":\"read_file\",\"arguments\":{\"path\":\"manual.txt\"}}\n```",
        &registry(),
    )
    .unwrap();

    assert_eq!(repaired.arguments["path"], "manual.txt");
    assert!(!repaired.syntax_repairs.is_empty());
    assert!(repair_tool_call(
        "Here is the call: {\"name\":\"read_file\",\"arguments\":{\"path\":\"x\"}}",
        &registry(),
    )
    .is_err());
}

#[test]
fn trailing_commas_are_removed_only_outside_strings() {
    let repaired = repair_tool_call(
        r#"{"name":"read_file","arguments":{"path":"a,}"},}"#,
        &registry(),
    )
    .unwrap();

    assert_eq!(repaired.arguments["path"], "a,}");
    assert_eq!(repaired.syntax_repairs.len(), 1);
}

#[test]
fn escaped_argument_content_survives_nested_json_parsing() {
    let raw =
        r#"{"name":"read_file","arguments":"{\"path\":\"C:\\\\steel\\\\draft \\\"A\\\".txt\"}"}"#;
    let repaired = repair_tool_call(raw, &registry()).unwrap();

    assert_eq!(repaired.arguments["path"], "C:\\steel\\draft \"A\".txt");
}

#[test]
fn wrong_types_return_exact_schema_feedback() {
    let error = repair_tool_call(
        &call("read_file", json!({"path": ["manual.txt"]})),
        &registry(),
    )
    .unwrap_err();

    assert_eq!(error.code(), "schema_type_mismatch");
    assert_eq!(error.path(), Some("$.path"));
    assert!(error.to_string().contains("expected string"));
}

#[test]
fn missing_required_fields_are_not_inferred() {
    let error = repair_tool_call(&call("read_file", json!({})), &registry()).unwrap_err();

    assert_eq!(error.code(), "schema_required_missing");
    assert_eq!(error.path(), Some("$.path"));
}

#[test]
fn unknown_tools_are_rejected_before_arguments_are_considered() {
    let error = repair_tool_call(
        &call("delete_everything", json!({"path": "C:\\"})),
        &registry(),
    )
    .unwrap_err();

    assert_eq!(error.code(), "unknown_tool");
    assert!(error.to_string().contains("delete_everything"));
}

#[test]
fn path_and_command_aliases_are_rejected_instead_of_guessed() {
    let path_error = repair_tool_call(
        &call("read_file", json!({"file": "manual.txt"})),
        &registry(),
    )
    .unwrap_err();
    assert_eq!(path_error.code(), "schema_required_missing");

    let command_error =
        repair_tool_call(&call("shell", json!({"cmd": "echo safe"})), &registry()).unwrap_err();
    assert_eq!(command_error.code(), "schema_required_missing");
}

#[test]
fn destructive_boolean_and_permission_intent_are_never_coerced() {
    let error = repair_tool_call(
        &call(
            "shell",
            json!({"command": "remove-temp", "destructive": "true"}),
        ),
        &registry(),
    )
    .unwrap_err();

    assert_eq!(error.code(), "schema_type_mismatch");
    assert_eq!(error.path(), Some("$.destructive"));

    let repaired =
        repair_tool_call(&call("shell", json!({"command": "echo safe"})), &registry()).unwrap();
    assert_eq!(repaired.risk, PermissionRisk::Dangerous);
}

#[test]
fn model_feedback_allows_at_most_two_retries() {
    let mut retries = 0;
    let repaired =
        repair_tool_call_with_retries(&call("read_file", json!({})), &registry(), |feedback| {
            retries += 1;
            assert_eq!(feedback.attempt, retries);
            assert_eq!(feedback.code, "schema_required_missing");
            match retries {
                1 => Some(call("read_file", json!({"path": "fixed.txt"}))),
                _ => Some(call("read_file", json!({"path": "final.txt"}))),
            }
        })
        .unwrap();

    assert_eq!(retries, 1);
    assert_eq!(repaired.arguments["path"], "fixed.txt");
}

#[test]
fn retry_exhaustion_is_bounded_and_reports_the_last_error() {
    let mut retries = 0;
    let error =
        repair_tool_call_with_retries(&call("read_file", json!({})), &registry(), |_feedback| {
            retries += 1;
            Some(call("read_file", json!({})))
        })
        .unwrap_err();

    assert_eq!(retries, MAX_REPAIR_RETRIES);
    match error {
        ToolRepairError::RetryExhausted { attempts, last } => {
            assert_eq!(attempts, MAX_REPAIR_RETRIES + 1);
            assert_eq!(last.code(), "schema_required_missing");
        }
        other => panic!("expected bounded retry error, got {other:?}"),
    }
}

#[test]
fn json_schema_number_accepts_integer_json_values() {
    let tools = vec![ToolSpec {
        id: "builtin.wait.v1".to_string(),
        name: "wait".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {"seconds": {"type": "number"}},
            "required": ["seconds"],
            "additionalProperties": false
        }),
        risk: PermissionRisk::Automatic,
    }];

    let repaired = repair_tool_call(&call("wait", json!({"seconds": 30})), &tools).unwrap();

    assert_eq!(repaired.arguments["seconds"], 30);
}
