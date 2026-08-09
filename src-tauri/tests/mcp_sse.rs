use bloomery::mcp::{McpClientConfig, McpLegacySseConfig, McpSupervisor, McpTransportConfig};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
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
    sync::{broadcast, Mutex},
    task::JoinHandle,
};

#[tokio::test]
async fn legacy_sse_initializes_lists_tools_and_injects_auth() {
    let (url, state, server) = spawn_fixture(false).await;
    let config = McpLegacySseConfig::new(url)
        .with_bearer_token("fixture-token")
        .with_max_event_bytes(4096);
    let mut supervisor =
        McpSupervisor::connect(McpTransportConfig::LegacySse(config), client_config())
            .await
            .expect("legacy SSE server should initialize");

    let tools = supervisor
        .client()
        .expect("supervisor should be connected")
        .list_tools()
        .await
        .expect("legacy SSE server should list tools");
    assert_eq!(tools.len(), 1);
    assert!(state.auth_seen.load(Ordering::SeqCst));

    supervisor
        .shutdown()
        .await
        .expect("SSE shutdown should succeed");
    server.abort();
}

#[tokio::test]
async fn legacy_sse_reconnects_with_last_event_id() {
    let (url, state, server) = spawn_fixture(true).await;
    let config = McpLegacySseConfig::new(url).with_sse(bloomery::mcp::McpSseConfig::new(
        Some(2),
        Duration::from_millis(5),
    ));
    let mut supervisor =
        McpSupervisor::connect(McpTransportConfig::LegacySse(config), client_config())
            .await
            .expect("legacy SSE server should initialize");

    let tools = supervisor
        .client()
        .expect("supervisor should be connected")
        .list_tools()
        .await
        .expect("response should arrive after SSE resume");
    assert_eq!(tools.len(), 1);
    assert!(state.reconnect_seen.load(Ordering::SeqCst));

    supervisor
        .shutdown()
        .await
        .expect("SSE shutdown should succeed");
    server.abort();
}

fn client_config() -> McpClientConfig {
    McpClientConfig {
        server_id: "legacy-sse-fixture".to_string(),
        request_timeout: Duration::from_secs(2),
        ..McpClientConfig::default()
    }
}

#[derive(Clone)]
struct WireEvent {
    id: &'static str,
    data: String,
    close_after: bool,
}

struct FixtureState {
    events: broadcast::Sender<WireEvent>,
    reconnect: bool,
    first_tools_request: AtomicBool,
    auth_seen: AtomicBool,
    reconnect_seen: AtomicBool,
    pending_response: Mutex<Option<String>>,
}

async fn spawn_fixture(reconnect: bool) -> (String, Arc<FixtureState>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("fixture should bind");
    let address = listener.local_addr().expect("fixture address");
    let (events, _) = broadcast::channel(32);
    let state = Arc::new(FixtureState {
        events,
        reconnect,
        first_tools_request: AtomicBool::new(true),
        auth_seen: AtomicBool::new(false),
        reconnect_seen: AtomicBool::new(false),
        pending_response: Mutex::new(None),
    });
    let task_state = state.clone();
    let server = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let state = task_state.clone();
            tokio::spawn(async move {
                let _ = handle_request(stream, state).await;
            });
        }
    });
    (format!("http://{address}/sse"), state, server)
}

async fn handle_request(mut stream: TcpStream, state: Arc<FixtureState>) -> io::Result<()> {
    let request = read_request(&mut stream).await?;
    if request.headers.get("authorization").map(String::as_str) == Some("Bearer fixture-token") {
        state.auth_seen.store(true, Ordering::SeqCst);
    }
    if request.method == "GET" && request.path == "/sse" {
        return handle_sse(stream, state, request.headers).await;
    }
    if request.method == "POST" && request.path == "/message" {
        let method = request
            .body
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let id = request.body.get("id").cloned().unwrap_or(Value::from(1));
        match method {
            "initialize" => publish(
                &state,
                WireEvent {
                    id: "init-1",
                    data: json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocolVersion": "2025-03-26",
                            "capabilities": {"tools": {}},
                            "serverInfo": {"name": "legacy-fixture", "version": "1.0.0"}
                        }
                    })
                    .to_string(),
                    close_after: false,
                }),
            "tools/list" if state.reconnect && state.first_tools_request.swap(false, Ordering::SeqCst) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {"tools": [{"name": "steel.lookup", "description": "fixture", "inputSchema": {"type": "object"}}]}
                })
                .to_string();
                *state.pending_response.lock().await = Some(response);
                publish(
                    &state,
                    WireEvent {
                        id: "evt-1",
                        data: json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/progress",
                            "params": {"progressToken": "fixture", "progress": 1}
                        })
                        .to_string(),
                        close_after: true,
                    },
                );
            }
            "tools/list" => publish(
                &state,
                WireEvent {
                    id: "tools-1",
                    data: json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"tools": [{"name": "steel.lookup", "description": "fixture", "inputSchema": {"type": "object"}}]}
                    })
                    .to_string(),
                    close_after: false,
                },
            ),
            _ => {}
        }
        return write_status(&mut stream, 202).await;
    }
    write_status(&mut stream, 404).await
}

async fn handle_sse(
    mut stream: TcpStream,
    state: Arc<FixtureState>,
    headers: HashMap<String, String>,
) -> io::Result<()> {
    let mut events = state.events.subscribe();
    write_stream_headers(&mut stream).await?;
    stream
        .write_all(b"event: endpoint\ndata: /message\n\n")
        .await?;
    stream.flush().await?;
    if headers.get("last-event-id").map(String::as_str) == Some("evt-1") {
        state.reconnect_seen.store(true, Ordering::SeqCst);
        if let Some(data) = state.pending_response.lock().await.take() {
            write_event(
                &mut stream,
                WireEvent {
                    id: "evt-2",
                    data,
                    close_after: false,
                },
            )
            .await?;
        }
    }
    loop {
        match events.recv().await {
            Ok(event) => {
                write_event(&mut stream, event.clone()).await?;
                if event.close_after {
                    return Ok(());
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return Ok(()),
        }
    }
}

fn publish(state: &FixtureState, event: WireEvent) {
    let _ = state.events.send(event);
}

struct FixtureRequest {
    method: String,
    path: String,
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
                "request ended",
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
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let path = request_parts.next().unwrap_or_default().to_string();
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
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "body ended"));
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
        path,
        headers,
        body,
    })
}

async fn write_stream_headers(stream: &mut TcpStream) -> io::Result<()> {
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n",
        )
        .await
}

async fn write_event(stream: &mut TcpStream, event: WireEvent) -> io::Result<()> {
    stream
        .write_all(format!("id: {}\nevent: message\ndata: {}\n\n", event.id, event.data).as_bytes())
        .await?;
    stream.flush().await
}

async fn write_status(stream: &mut TcpStream, status: u16) -> io::Result<()> {
    let reason = if status == 202 {
        "Accepted"
    } else if status == 404 {
        "Not Found"
    } else {
        "OK"
    };
    stream
        .write_all(
            format!("HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
}
