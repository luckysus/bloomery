use bloomery::providers::capabilities::{
    DocumentParseRequest, DocumentParserProvider, RemoteTaskId,
};
use bloomery::providers::http::ProviderErrorCode;
use bloomery::providers::mineru::MinerUProvider;
use bloomery::providers::profiles::{ProviderKind, ProviderProfile};
use bloomery::storage::secrets::SecretValue;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;

const SERVER_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn mineru_upload_rejects_redirect_without_sending_document_to_target() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU upload redirect");
    let address = listener.local_addr().unwrap();
    let target = TcpListener::bind("127.0.0.1:0").expect("bind upload redirect target");
    let target_address = target.local_addr().unwrap();
    let target_server = redirect_target_server(target, "text/plain", Vec::new());
    let server = thread::spawn(move || {
        let mut submit_stream = accept_request(&listener);
        let _ = read_http_request(&mut submit_stream);
        let body = serde_json::json!({
            "code": 0,
            "data": {
                "batch_id": "upload-redirect-batch",
                "file_urls": [format!("http://localhost:{}/upload/private.pdf", address.port())]
            }
        })
        .to_string();
        write_http_response(&mut submit_stream, "200 OK", "application/json", &[], body);
        drop(submit_stream);

        let mut upload_stream = accept_request(&listener);
        let request = read_http_request(&mut upload_stream);
        assert!(String::from_utf8_lossy(&request).starts_with("PUT /upload/private.pdf "));
        write_http_response(
            &mut upload_stream,
            "307 Temporary Redirect",
            "text/plain",
            &[(
                "location",
                &format!("http://{target_address}/private-upload"),
            )],
            "",
        );
    });
    let provider = mineru_provider(address);
    let document = b"confidential document body";

    let result = tauri::async_runtime::block_on(provider.submit(DocumentParseRequest {
        file_name: "private.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        bytes: document.to_vec(),
    }));

    server.join().expect("join upload redirect server");
    let redirected_request = target_server.join().expect("join upload redirect target");
    assert!(
        redirected_request.is_none(),
        "signed upload followed a cross-host redirect"
    );
    let error = result.unwrap_err();
    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    assert_eq!(error.status(), Some(307));
    assert!(!error.to_string().contains("confidential document body"));
}

#[test]
fn mineru_download_rejects_redirect_without_reading_target() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU download redirect");
    let address = listener.local_addr().unwrap();
    let target = TcpListener::bind("127.0.0.1:0").expect("bind download redirect target");
    let target_address = target.local_addr().unwrap();
    let target_server = redirect_target_server(target, "application/zip", b"not read".to_vec());
    let server = thread::spawn(move || {
        let mut poll_stream = accept_request(&listener);
        let _ = read_http_request(&mut poll_stream);
        let body = serde_json::json!({
            "code": 0,
            "data": {
                "batch_id": "download-redirect-batch",
                "extract_result": [{
                    "file_name": "paper.pdf",
                    "state": "done",
                    "full_zip_url": format!("http://localhost:{}/artifact.zip", address.port())
                }]
            }
        })
        .to_string();
        write_http_response(&mut poll_stream, "200 OK", "application/json", &[], body);
        drop(poll_stream);

        let mut download_stream = accept_request(&listener);
        let request = read_http_request(&mut download_stream);
        assert!(String::from_utf8_lossy(&request).starts_with("GET /artifact.zip "));
        write_http_response(
            &mut download_stream,
            "307 Temporary Redirect",
            "text/plain",
            &[(
                "location",
                &format!("http://{target_address}/private-artifact.zip"),
            )],
            "",
        );
    });
    let provider = mineru_provider(address);

    let result = tauri::async_runtime::block_on(
        provider.download(&RemoteTaskId("download-redirect-batch".to_string())),
    );

    server.join().expect("join download redirect server");
    let redirected_request = target_server.join().expect("join download redirect target");
    assert!(
        redirected_request.is_none(),
        "signed download followed a cross-host redirect"
    );
    let error = result.unwrap_err();
    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    assert_eq!(error.status(), Some(307));
}

fn mineru_provider(address: std::net::SocketAddr) -> MinerUProvider {
    MinerUProvider::new(
        ProviderProfile {
            id: Uuid::new_v4(),
            kind: ProviderKind::MinerU,
            display_name: "MinerU redirect test".to_string(),
            base_url: format!("http://{address}"),
            model_id: None,
            secret_ref: Some("profile/credential".to_string()),
            enabled: true,
        },
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap()
}

fn accept_request(listener: &TcpListener) -> TcpStream {
    listener
        .set_nonblocking(true)
        .expect("configure mock listener");
    let deadline = Instant::now() + SERVER_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_read_timeout(Some(SERVER_TIMEOUT))
                    .expect("configure mock stream");
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "mock server accept timed out");
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accept mock request: {error}"),
        }
    }
}

fn redirect_target_server(
    listener: TcpListener,
    content_type: &'static str,
    body: Vec<u8>,
) -> JoinHandle<Option<Vec<u8>>> {
    thread::spawn(move || {
        listener
            .set_nonblocking(true)
            .expect("configure redirect target");
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(SERVER_TIMEOUT))
                        .expect("configure redirect target");
                    let request = read_http_request(&mut stream);
                    write_http_response(&mut stream, "200 OK", content_type, &[], body);
                    return Some(request);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return None;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("accept redirect target request: {error}"),
            }
        }
    })
}

fn read_http_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer).expect("read provider request");
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
        if let Some(index) = request.windows(4).position(|item| item == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
    let content_length = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while request.len() < header_end + content_length {
        let read = stream.read(&mut buffer).expect("read provider body");
        assert_ne!(read, 0);
        request.extend_from_slice(&buffer[..read]);
    }
    request
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    headers: &[(&str, &str)],
    body: impl AsRef<[u8]>,
) {
    let body = body.as_ref();
    let extra_headers = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\n{extra_headers}content-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
}
