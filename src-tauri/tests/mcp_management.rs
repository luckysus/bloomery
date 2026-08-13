use bloomery::{
    mcp::{McpServerConfig, McpTransportKind},
    storage::{migrations, repositories::mcp},
};
use rusqlite::Connection;
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

fn config() -> McpServerConfig {
    McpServerConfig {
        id: Uuid::new_v4(),
        display_name: "Steel standards".to_string(),
        server_id: "steel-standards".to_string(),
        transport: McpTransportKind::Stdio,
        url: None,
        executable: Some(PathBuf::from("powershell.exe")),
        args: vec!["-NoProfile".to_string()],
        working_directory: None,
        inherited_env: vec!["SystemRoot".to_string()],
        env_names: vec!["STEEL_API_KEY".to_string()],
        timeout: Duration::from_secs(30),
        enabled: true,
    }
}

#[test]
fn mcp_server_config_round_trips_without_persisting_secrets() {
    let mut connection = Connection::open_in_memory().expect("open sqlite");
    migrations::migrate(&mut connection).expect("migrate sqlite");
    let expected = config();

    mcp::save(&mut connection, "local", &expected).expect("save MCP server");
    let records = mcp::list(&connection, "local").expect("list MCP servers");

    assert_eq!(records, vec![expected.clone()]);
    let raw: String = connection
        .query_row(
            "SELECT args_json || inherited_env_json || env_names_json FROM mcp_servers",
            [],
            |row| row.get(0),
        )
        .expect("read persisted MCP config");
    assert!(!raw.contains("STEEL_API_KEY_VALUE"));
}

#[test]
fn mcp_server_config_rejects_invalid_transport_shape() {
    let mut invalid = config();
    invalid.executable = None;
    assert!(invalid.validate().is_err());

    let mut http = config();
    http.transport = McpTransportKind::StreamableHttp;
    http.executable = None;
    http.url = Some("file:///private/mcp".to_string());
    assert!(http.validate().is_err());
}

#[test]
fn mcp_server_env_names_are_deduplicated_and_sorted() {
    let mut value = config();
    value.env_names = vec![
        "Z_KEY".to_string(),
        "A_KEY".to_string(),
        "Z_KEY".to_string(),
    ];
    value.normalize().expect("normalize MCP config");
    assert_eq!(value.env_names, vec!["A_KEY", "Z_KEY"]);
}

#[test]
fn mcp_server_config_does_not_accept_secret_values_as_environment_names() {
    let mut value = config();
    value.env_names = vec!["STEEL_API_KEY=secret".to_string()];
    assert!(value.validate().is_err());
}

#[test]
fn mcp_server_config_rejects_unallowlisted_inherited_environment_names() {
    for name in ["OPENAI_API_KEY", "GH_TOKEN", "CUSTOM_RUNTIME_SETTING"] {
        let mut value = config();
        value.inherited_env = vec![name.to_string()];
        assert!(
            value.validate().is_err(),
            "inherited environment variable must be rejected: {name}"
        );
    }
}
