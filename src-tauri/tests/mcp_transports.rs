use bloomery::mcp::{
    McpClientConfig, McpError, McpHttpConfig, McpLegacySseConfig, McpSseConfig, McpStdioConfig,
    McpStdioEnv, McpSupervisor, McpTransportConfig, StdioTransport,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task::JoinHandle,
};

#[cfg(windows)]
#[tokio::test]
async fn stdio_process_starts_and_shuts_down_cleanly() {
    let mut spawned = StdioTransport::spawn(powershell_config("[Console]::Out.WriteLine('ready')"))
        .expect("PowerShell fixture should start");
    assert!(spawned.id().is_some());

    spawned
        .graceful_shutdown()
        .await
        .expect("child should shut down");
}

#[cfg(windows)]
#[tokio::test]
async fn stdio_stderr_is_bounded_but_fully_drained() {
    let mut config = powershell_config("[Console]::Error.Write(('x' * 4096))");
    config.max_stderr_bytes = 64;
    let mut spawned = StdioTransport::spawn(config).expect("PowerShell fixture should start");

    spawned.stderr.wait().await;
    let snapshot = spawned.stderr.snapshot().await;
    assert_eq!(snapshot.bytes.len(), 64);
    assert!(snapshot.truncated);

    spawned
        .graceful_shutdown()
        .await
        .expect("child should shut down");
}

#[cfg(windows)]
#[tokio::test]
async fn stdio_environment_contains_only_explicit_values() {
    let script = "[Console]::Error.Write(\"allowed=$env:BLOOMERY_ALLOWED;inherited=$env:BLOOMERY_NOT_ALLOWED\")";
    let mut config = powershell_config(script);
    config.inherited_env = vec!["SystemRoot".to_string(), "windir".to_string()];
    config.env = McpStdioEnv::from([("BLOOMERY_ALLOWED".to_string(), "yes".to_string())]);
    let mut spawned = StdioTransport::spawn(config).expect("PowerShell fixture should start");

    spawned.stderr.wait().await;
    let output = String::from_utf8_lossy(&spawned.stderr.snapshot().await.bytes).to_string();
    assert!(output.contains("allowed=yes"));
    assert!(output.contains("inherited="));
    assert!(!output.contains("inherited=secret"));

    spawned
        .graceful_shutdown()
        .await
        .expect("child should shut down");
}

#[cfg(windows)]
#[test]
fn stdio_rejects_inherited_credential_environment_names() {
    let mut config = powershell_config("");
    config.inherited_env = vec!["OPENAI_API_KEY".to_string()];

    let error = match StdioTransport::spawn(config) {
        Ok(_) => panic!("credential inheritance must fail"),
        Err(error) => error,
    };
    assert!(matches!(error, McpError::InvalidTransport(_)));
    assert!(error.to_string().contains("not allowed"));
}

#[test]
fn http_config_requires_http_url_and_positive_event_limit() {
    let valid = McpHttpConfig::new("http://127.0.0.1:8080/mcp")
        .with_sse(McpSseConfig::new(Some(2), Duration::from_millis(25)));
    assert!(valid.validate().is_ok());
    assert!(McpHttpConfig::new("file:///tmp/mcp").validate().is_err());
    assert!(McpHttpConfig::new("http://127.0.0.1:8080/mcp")
        .with_max_sse_event_bytes(0)
        .validate()
        .is_err());
}

#[test]
fn sse_retry_configuration_is_bounded_and_deterministic() {
    let config = McpSseConfig::new(Some(2), Duration::from_millis(25));
    assert_eq!(config.delay_for_attempt(0), Some(Duration::from_millis(25)));
    assert_eq!(config.delay_for_attempt(1), Some(Duration::from_millis(50)));
    assert_eq!(config.delay_for_attempt(2), None);
}

#[test]
fn transport_config_keeps_stdio_and_http_disjoint() {
    let stdio = McpTransportConfig::Stdio(powershell_config(""));
    assert!(matches!(stdio, McpTransportConfig::Stdio(_)));
    let http = McpTransportConfig::Http(McpHttpConfig::new("https://example.com/mcp"));
    assert!(matches!(http, McpTransportConfig::Http(_)));
}

#[test]
fn legacy_sse_config_has_a_separate_transport_and_bounded_events() {
    let config = McpLegacySseConfig::new("https://example.com/sse")
        .with_bearer_token("secret-token")
        .with_max_event_bytes(4096);
    assert!(config.validate().is_ok());
    assert!(McpLegacySseConfig::new("file:///tmp/sse")
        .validate()
        .is_err());
    assert!(McpLegacySseConfig::new("https://example.com/sse")
        .with_max_event_bytes(0)
        .validate()
        .is_err());

    let transport = McpTransportConfig::LegacySse(config);
    assert!(matches!(transport, McpTransportConfig::LegacySse(_)));
}

#[tokio::test]
async fn http_transport_injects_auth_and_custom_headers() {
    let (url, state, server) = spawn_http_fixture(FixtureMode::Json).await;
    let config = McpHttpConfig::new(url)
        .with_bearer_token("secret-token")
        .with_header("x-bloomery-test", "fixture")
        .with_sse(McpSseConfig::new(Some(1), Duration::from_millis(10)));
    let mut supervisor =
        McpSupervisor::connect(McpTransportConfig::Http(config), fixture_client_config())
            .await
            .expect("HTTP MCP fixture should initialize");

    supervisor
        .client()
        .expect("supervisor should be connected")
        .list_tools()
        .await
        .expect("HTTP MCP fixture should list tools");
    assert!(state.auth_seen.load(Ordering::SeqCst));
    assert!(state.custom_header_seen.load(Ordering::SeqCst));

    tokio::time::timeout(Duration::from_secs(2), supervisor.shutdown())
        .await
        .expect("HTTP shutdown should finish")
        .expect("HTTP shutdown should succeed");
    server.abort();
}

#[tokio::test]
async fn http_sse_reconnect_resumes_with_last_event_id() {
    let (url, state, server) = spawn_http_fixture(FixtureMode::Resume).await;
    let config =
        McpHttpConfig::new(url).with_sse(McpSseConfig::new(Some(2), Duration::from_millis(10)));
    let mut supervisor =
        McpSupervisor::connect(McpTransportConfig::Http(config), fixture_client_config())
            .await
            .expect("HTTP MCP fixture should initialize");

    supervisor
        .client()
        .expect("supervisor should be connected")
        .list_tools()
        .await
        .expect("resumed SSE response should complete the request");
    assert!(state.resume_seen.load(Ordering::SeqCst));

    supervisor
        .shutdown()
        .await
        .expect("HTTP shutdown should succeed");
    server.abort();
}

#[tokio::test]
async fn malformed_sse_frame_returns_a_bounded_error() {
    let (url, _state, server) = spawn_http_fixture(FixtureMode::Malformed).await;
    let config =
        McpHttpConfig::new(url).with_sse(McpSseConfig::new(Some(0), Duration::from_millis(10)));
    let mut supervisor =
        McpSupervisor::connect(McpTransportConfig::Http(config), fixture_client_config())
            .await
            .expect("HTTP MCP fixture should initialize");

    let result = tokio::time::timeout(
        Duration::from_secs(2),
        supervisor
            .client()
            .expect("supervisor should be connected")
            .list_tools(),
    )
    .await
    .expect("malformed frame must not hang");
    assert!(matches!(
        result,
        Err(McpError::Transport(_)) | Err(McpError::Timeout)
    ));

    supervisor
        .shutdown()
        .await
        .expect("HTTP shutdown should succeed");
    server.abort();
}

fn fixture_client_config() -> McpClientConfig {
    McpClientConfig {
        server_id: "http-fixture".to_string(),
        request_timeout: Duration::from_secs(2),
        ..McpClientConfig::default()
    }
}

#[derive(Clone, Copy)]
enum FixtureMode {
    Json,
    Resume,
    Malformed,
}

struct FixtureState {
    mode: FixtureMode,
    auth_seen: AtomicBool,
    custom_header_seen: AtomicBool,
    resume_seen: AtomicBool,
    first_tools_request: AtomicBool,
    tools_request_id: Mutex<Option<Value>>,
}

async fn spawn_http_fixture(mode: FixtureMode) -> (String, Arc<FixtureState>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("fixture should bind");
    let address = listener.local_addr().expect("fixture address");
    let state = Arc::new(FixtureState {
        mode,
        auth_seen: AtomicBool::new(false),
        custom_header_seen: AtomicBool::new(false),
        resume_seen: AtomicBool::new(false),
        first_tools_request: AtomicBool::new(true),
        tools_request_id: Mutex::new(None),
    });
    let task_state = state.clone();
    let server = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let state = task_state.clone();
            tokio::spawn(async move {
                let _ = handle_http_fixture(stream, state).await;
            });
        }
    });
    (format!("http://{address}/mcp"), state, server)
}

async fn handle_http_fixture(mut stream: TcpStream, state: Arc<FixtureState>) -> io::Result<()> {
    let request = read_request(&mut stream).await?;
    if request.headers.get("authorization").map(String::as_str) == Some("Bearer secret-token") {
        state.auth_seen.store(true, Ordering::SeqCst);
    }
    if request.headers.get("x-bloomery-test").map(String::as_str) == Some("fixture") {
        state.custom_header_seen.store(true, Ordering::SeqCst);
    }

    if request.method == "GET" && matches!(state.mode, FixtureMode::Resume) {
        if request.headers.get("last-event-id").map(String::as_str) == Some("evt-1") {
            state.resume_seen.store(true, Ordering::SeqCst);
            let id = state
                .tools_request_id
                .lock()
                .await
                .clone()
                .unwrap_or(Value::from(1));
            return write_sse(
                &mut stream,
                &format!(
                    "id: evt-2\nevent: message\ndata: {}\n\n",
                    json!({"jsonrpc":"2.0","id":id,"result":{"tools":[]}})
                ),
            )
            .await;
        }
        return write_sse(&mut stream, "").await;
    }

    let method = request
        .body
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match method {
        "initialize" => {
            write_json(
                &mut stream,
                json!({
                    "jsonrpc": "2.0",
                    "id": request.body.get("id").cloned().unwrap_or(Value::from(1)),
                    "result": {
                        "protocolVersion": "2025-03-26",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "http-fixture", "version": "1.0.0"}
                    }
                }),
            )
            .await
        }
        "notifications/initialized" => write_status(&mut stream, 202).await,
        "tools/list" => {
            *state.tools_request_id.lock().await = request.body.get("id").cloned();
            match state.mode {
                FixtureMode::Json => write_json(
                    &mut stream,
                    json!({"jsonrpc":"2.0","id":request.body.get("id").cloned().unwrap_or(Value::from(1)),"result":{"tools":[]}}),
                )
                .await,
                FixtureMode::Resume => {
                    if state.first_tools_request.swap(false, Ordering::SeqCst) {
                        write_sse(
                            &mut stream,
                            "id: evt-1\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{\"progressToken\":\"fixture\",\"progress\":1}}\n\n",
                        )
                        .await
                    } else {
                        write_status(&mut stream, 500).await
                    }
                }
                FixtureMode::Malformed => write_sse(
                    &mut stream,
                    "id: bad-1\nevent: message\ndata: this-is-not-json\n\n",
                )
                .await,
            }
        }
        _ if request.method == "DELETE" => write_status(&mut stream, 204).await,
        _ => write_status(&mut stream, 202).await,
    }
}

struct FixtureRequest {
    method: String,
    headers: HashMap<String, String>,
    body: Value,
}

async fn read_request(stream: &mut TcpStream) -> io::Result<FixtureRequest> {
    let mut bytes = Vec::new();
    let header_end;
    loop {
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "fixture request ended",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }
    let header_text = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let method = request_line
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    let mut headers = HashMap::new();
    let mut content_length = 0usize;
    for line in lines.filter(|line| !line.is_empty()) {
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if name == "content-length" {
                content_length = value.parse().unwrap_or(0);
            }
            headers.insert(name, value);
        }
    }
    while bytes.len() < header_end + content_length {
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "fixture body ended",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    let body = if content_length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[header_end..header_end + content_length])
            .unwrap_or(Value::Null)
    };
    Ok(FixtureRequest {
        method,
        headers,
        body,
    })
}

async fn write_json(stream: &mut TcpStream, body: Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(&body).expect("fixture JSON should serialize");
    write_response(stream, 200, "application/json", &bytes).await
}

async fn write_sse(stream: &mut TcpStream, body: &str) -> io::Result<()> {
    write_response(stream, 200, "text/event-stream", body.as_bytes()).await
}

async fn write_status(stream: &mut TcpStream, status: u16) -> io::Result<()> {
    write_response(stream, status, "text/plain", &[]).await
}

async fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        500 => "Internal Server Error",
        _ => "Fixture",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await
}

#[cfg(windows)]
fn powershell_config(script: &str) -> McpStdioConfig {
    McpStdioConfig {
        executable: "powershell.exe".into(),
        args: vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            script.into(),
        ],
        working_directory: None,
        inherited_env: Vec::new(),
        env: BTreeMap::new(),
        max_stderr_bytes: 1024,
    }
}

#[cfg(not(windows))]
fn powershell_config(_script: &str) -> McpStdioConfig {
    McpStdioConfig {
        executable: "fixture".into(),
        args: Vec::new(),
        working_directory: None,
        inherited_env: Vec::new(),
        env: BTreeMap::new(),
        max_stderr_bytes: 1024,
    }
}
