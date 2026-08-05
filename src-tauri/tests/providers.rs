use bloomery::providers::capabilities::{
    ChatEvent, ChatProvider, ChatRequest, DocumentParseRequest, DocumentParserProvider,
    DocumentTaskState, EmbeddingProvider, ParsedDocumentArtifact, RemoteTaskId, RerankProvider,
};
use bloomery::providers::http::{ProviderError, ProviderErrorCode};
use bloomery::providers::mineru::MinerUProvider;
use bloomery::providers::ollama::{
    default_ollama_base_url, normalize_ollama_chat_url, OllamaProvider,
};
use bloomery::providers::openai::{
    default_openai_base_url, normalize_openai_chat_url, OpenAiProvider,
};
use bloomery::providers::profiles::{
    resolve_chat_profile, ProviderCapability, ProviderKind, ProviderProfile,
};
use bloomery::providers::siliconflow::{
    SiliconFlowPlan, SiliconFlowProvider, DEFAULT_EMBEDDING_MODEL, DEFAULT_RERANK_MODEL,
};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::provider_profiles;
use bloomery::storage::secrets::SecretValue;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;
use zip::write::SimpleFileOptions;

const WORKSPACE: &str = "local";
const MOCK_SERVER_TIMEOUT: Duration = Duration::from_secs(5);

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open memory database");
    migrate(&mut connection).expect("migrate database");
    connection
}

fn profile(kind: ProviderKind, base_url: &str, model_id: Option<&str>) -> ProviderProfile {
    ProviderProfile {
        id: Uuid::new_v4(),
        kind,
        display_name: "Primary provider".to_string(),
        base_url: base_url.to_string(),
        model_id: model_id.map(str::to_string),
        secret_ref: Some("profile/credential".to_string()),
        enabled: true,
    }
}

#[test]
fn profile_serialization_never_exposes_secret_value_fields() {
    let profile = profile(
        ProviderKind::SiliconFlow,
        "https://api.siliconflow.cn/v1",
        Some("BAAI/bge-m3"),
    );

    let serialized = serde_json::to_value(profile).expect("serialize profile");
    let object = serialized.as_object().expect("profile object");

    for forbidden in ["api_key", "token", "secret_value"] {
        assert!(
            !object.contains_key(forbidden),
            "forbidden field {forbidden}"
        );
    }
    assert_eq!(
        object.get("kind").and_then(|value| value.as_str()),
        Some("siliconflow")
    );
    assert_eq!(
        object.get("secret_ref").and_then(|value| value.as_str()),
        Some("profile/credential")
    );
}

#[test]
fn profile_validation_normalizes_urls_without_guessing_endpoint_paths() {
    let normalized = profile(
        ProviderKind::OpenAiCompatible,
        " https://models.example.com/custom/v1/ ",
        Some(" model-a "),
    )
    .validate()
    .expect("valid profile");

    assert_eq!(normalized.base_url, "https://models.example.com/custom/v1");
    assert_eq!(normalized.model_id.as_deref(), Some("model-a"));
    assert!(profile(ProviderKind::MinerU, "ftp://example.com", None)
        .validate()
        .is_err());
    assert!(profile(
        ProviderKind::Ollama,
        "http://127.0.0.1:11434",
        Some("qwen3")
    )
    .validate()
    .is_ok());
}

#[test]
fn bearer_credentials_reject_remote_plain_http() {
    let error = OpenAiProvider::new(
        profile(
            ProviderKind::OpenAiCompatible,
            "http://models.example.com/v1",
            Some("model-a"),
        ),
        Some(SecretValue::new("sk-secret").unwrap()),
    )
    .err()
    .expect("remote HTTP with credential must fail");

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    assert!(error.to_string().contains("HTTPS"));
}

#[test]
fn profile_repository_is_workspace_scoped() {
    let mut connection = database();
    let input = profile(
        ProviderKind::SiliconFlow,
        "https://api.siliconflow.cn/v1/",
        Some("BAAI/bge-reranker-v2-m3"),
    );

    let saved =
        provider_profiles::save(&mut connection, WORKSPACE, input).expect("save provider profile");

    assert_eq!(saved.base_url, "https://api.siliconflow.cn/v1");
    assert_eq!(
        provider_profiles::list(&connection, WORKSPACE)
            .unwrap()
            .len(),
        1
    );
    assert!(provider_profiles::list(&connection, "other")
        .unwrap()
        .is_empty());
    assert!(provider_profiles::get(&connection, "other", saved.id)
        .unwrap()
        .is_none());
    let columns = connection
        .prepare("PRAGMA table_info(provider_profiles)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for forbidden in ["api_key", "token", "secret_value"] {
        assert!(!columns.iter().any(|column| column == forbidden));
    }
}

#[test]
fn profile_revision_tracks_only_execution_configuration_changes() {
    let mut connection = database();
    let saved = provider_profiles::save_record(
        &mut connection,
        WORKSPACE,
        profile(
            ProviderKind::SiliconFlow,
            "https://api.siliconflow.cn/v1",
            Some("BAAI/bge-m3"),
        ),
    )
    .expect("create profile");
    assert_eq!((saved.revision, saved.secret_generation), (1, 0));

    let mut changed = saved.profile.clone();
    changed.display_name = "Renamed provider".to_string();
    changed.enabled = false;
    let unchanged = provider_profiles::save_record(&mut connection, WORKSPACE, changed)
        .expect("save presentation changes");
    assert_eq!(unchanged.revision, 1);
    assert_eq!(
        provider_profiles::save_record(&mut connection, WORKSPACE, unchanged.profile.clone())
            .expect("no-op save")
            .revision,
        1
    );

    let mut changed = unchanged.profile;
    changed.kind = ProviderKind::OpenAiCompatible;
    let changed = provider_profiles::save_record(&mut connection, WORKSPACE, changed).unwrap();
    assert_eq!(changed.revision, 2);
    let mut changed = changed.profile;
    changed.base_url = "https://models.example/v1".to_string();
    let changed = provider_profiles::save_record(&mut connection, WORKSPACE, changed).unwrap();
    assert_eq!(changed.revision, 3);
    let mut changed = changed.profile;
    changed.model_id = Some("model-b".to_string());
    let changed = provider_profiles::save_record(&mut connection, WORKSPACE, changed).unwrap();
    assert_eq!(changed.revision, 4);
    let mut changed = changed.profile;
    changed.secret_ref = Some("replacement_key".to_string());
    let changed = provider_profiles::save_record(&mut connection, WORKSPACE, changed).unwrap();
    assert_eq!(changed.revision, 5);
    assert_eq!(changed.secret_generation, 0);
}

#[test]
fn secret_generation_activation_is_workspace_scoped_compare_and_set() {
    let mut connection = database();
    let saved = provider_profiles::save_record(
        &mut connection,
        WORKSPACE,
        profile(
            ProviderKind::SiliconFlow,
            "https://api.siliconflow.cn/v1",
            Some("BAAI/bge-m3"),
        ),
    )
    .unwrap();

    let activated = provider_profiles::activate_secret_generation(
        &connection,
        WORKSPACE,
        saved.profile.id,
        "profile/credential",
        0,
    )
    .expect("activate generation one");
    assert_eq!(activated.secret_generation, 1);
    assert!(provider_profiles::activate_secret_generation(
        &connection,
        WORKSPACE,
        saved.profile.id,
        "profile/credential",
        0,
    )
    .unwrap_err()
    .contains("conflict"));
    assert!(provider_profiles::activate_secret_generation(
        &connection,
        "other",
        saved.profile.id,
        "profile/credential",
        1,
    )
    .is_err());
}

#[test]
fn one_default_profile_is_enforced_per_capability() {
    let mut connection = database();
    let first = provider_profiles::save(
        &mut connection,
        WORKSPACE,
        profile(
            ProviderKind::SiliconFlow,
            "https://api.siliconflow.cn/v1",
            Some("BAAI/bge-m3"),
        ),
    )
    .unwrap();
    let second = provider_profiles::save(
        &mut connection,
        WORKSPACE,
        profile(
            ProviderKind::SiliconFlow,
            "https://api.siliconflow.cn/v1",
            Some("custom-embedding-model"),
        ),
    )
    .unwrap();

    provider_profiles::set_default(
        &mut connection,
        WORKSPACE,
        ProviderCapability::Embedding,
        Some(first.id),
    )
    .unwrap();
    provider_profiles::set_default(
        &mut connection,
        WORKSPACE,
        ProviderCapability::Embedding,
        Some(second.id),
    )
    .unwrap();

    assert_eq!(
        provider_profiles::get_default(&connection, WORKSPACE, ProviderCapability::Embedding)
            .unwrap()
            .unwrap()
            .id,
        second.id
    );
    assert!(provider_profiles::set_default(
        &mut connection,
        WORKSPACE,
        ProviderCapability::DocumentParser,
        Some(second.id),
    )
    .is_err());

    provider_profiles::delete(&mut connection, WORKSPACE, second.id).unwrap();
    assert!(
        provider_profiles::get_default(&connection, WORKSPACE, ProviderCapability::Embedding)
            .unwrap()
            .is_none()
    );
}

#[test]
fn mineru_submit_poll_download_and_cancel_contract() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU provider");
    let address = listener.local_addr().unwrap();
    let archive = zip_fixture("paper/full.md", b"# Parsed paper");
    let checksum = format!("{:x}", Sha256::digest(&archive));
    let server_archive = archive.clone();
    let server = thread::spawn(move || {
        let mut submit_stream = accept_request(&listener);
        let submit_request = read_http_request(&mut submit_stream);
        let submit_text = String::from_utf8_lossy(&submit_request);
        assert!(submit_text.starts_with("POST /api/v4/file-urls/batch "));
        assert!(submit_text
            .to_ascii_lowercase()
            .contains("authorization: bearer mineru-secret"));
        let submit_body = json_request_body(&submit_request);
        assert_eq!(submit_body["model_version"], "vlm");
        assert_eq!(submit_body["files"][0]["name"], "paper.pdf");
        assert_eq!(submit_body["files"][0]["data_id"], "paper.pdf");
        let body = serde_json::json!({
            "code": 0,
            "data": {
                "batch_id": "batch-1",
                "file_urls": [format!("http://{address}/upload/paper.pdf")]
            }
        })
        .to_string();
        write_http_response(&mut submit_stream, "200 OK", "application/json", &[], &body);
        drop(submit_stream);

        let mut upload_stream = accept_request(&listener);
        let upload_request = read_http_request(&mut upload_stream);
        let upload_text = String::from_utf8_lossy(&upload_request);
        assert!(upload_text.starts_with("PUT /upload/paper.pdf "));
        assert!(!upload_text.to_ascii_lowercase().contains("authorization:"));
        let upload_body = request_body(&upload_request);
        assert_eq!(upload_body, b"%PDF-1.7");
        write_http_response(&mut upload_stream, "200 OK", "text/plain", &[], "");
        drop(upload_stream);

        let running = r#"{"code":0,"data":{"batch_id":"batch-1","extract_result":[{"file_name":"paper.pdf","state":"running","extract_progress":{"extracted_pages":2,"total_pages":5}}]}}"#;
        let mut poll_stream = accept_request(&listener);
        let request = read_http_request(&mut poll_stream);
        assert!(String::from_utf8_lossy(&request)
            .starts_with("GET /api/v4/extract-results/batch/batch-1 "));
        write_http_response(&mut poll_stream, "200 OK", "application/json", &[], running);
        drop(poll_stream);

        let done = format!(
            "{{\"code\":0,\"data\":{{\"batch_id\":\"batch-1\",\"extract_result\":[{{\"file_name\":\"paper.pdf\",\"state\":\"done\",\"full_zip_url\":\"http://{address}/artifact.zip\",\"sha256\":\"{checksum}\"}}]}}}}"
        );
        for _ in 0..2 {
            let mut poll_stream = accept_request(&listener);
            let request = read_http_request(&mut poll_stream);
            assert!(String::from_utf8_lossy(&request)
                .starts_with("GET /api/v4/extract-results/batch/batch-1 "));
            write_http_response(&mut poll_stream, "200 OK", "application/json", &[], &done);
            drop(poll_stream);
        }

        let mut download_stream = accept_request(&listener);
        let request = read_http_request(&mut download_stream);
        assert!(String::from_utf8_lossy(&request).starts_with("GET /artifact.zip "));
        write_http_response(
            &mut download_stream,
            "200 OK",
            "application/zip",
            &[("x-checksum-sha256", checksum.as_str())],
            &server_archive,
        );
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let id = tauri::async_runtime::block_on(provider.submit(DocumentParseRequest {
        file_name: "paper.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        bytes: b"%PDF-1.7".to_vec(),
    }))
    .unwrap();
    assert_eq!(id, RemoteTaskId("batch-1".to_string()));
    assert_eq!(provider.capabilities().model_id, "vlm");
    assert!(!serde_json::to_value(provider.profile())
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("bytes"));

    let running = tauri::async_runtime::block_on(provider.poll(&id)).unwrap();
    assert_eq!(running.state, DocumentTaskState::Running);
    assert_eq!(running.progress_percent, Some(40));
    let completed = tauri::async_runtime::block_on(provider.poll(&id)).unwrap();
    assert_eq!(completed.state, DocumentTaskState::Completed);
    assert_eq!(completed.progress_percent, Some(100));

    let artifact = tauri::async_runtime::block_on(provider.download(&id)).unwrap();
    assert_eq!(artifact.file_name, "batch-1.zip");
    assert_eq!(artifact.mime_type, "application/zip");
    assert_eq!(artifact.bytes, archive);

    let error = tauri::async_runtime::block_on(provider.cancel(&id)).unwrap_err();
    assert_eq!(error.code(), ProviderErrorCode::UnsupportedCapability);
    join_server(server);
}

#[test]
fn mineru_requires_a_credential() {
    let error = MinerUProvider::new(
        profile(ProviderKind::MinerU, "https://mineru.net", None),
        None,
    )
    .err()
    .expect("missing MinerU credential must fail");

    assert_eq!(error.code(), ProviderErrorCode::Authentication);
}

#[test]
fn mineru_submit_does_not_retry_uncertain_batch_creation() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU retry provider");
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        let mut stream = accept_request(&listener);
        let request = read_http_request(&mut stream);
        assert!(String::from_utf8_lossy(&request).starts_with("POST /api/v4/file-urls/batch "));
        server_attempts.fetch_add(1, Ordering::SeqCst);
        write_http_response(
            &mut stream,
            "503 Service Unavailable",
            "application/json",
            &[],
            r#"{"message":"uncertain"}"#,
        );
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let error = tauri::async_runtime::block_on(provider.submit(DocumentParseRequest {
        file_name: "retry.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        bytes: b"retry-body".to_vec(),
    }))
    .unwrap_err();

    assert_eq!(error.status(), Some(503));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    join_server(server);
}

#[test]
fn mineru_submit_does_not_retry_truncated_success_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU submit body provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut truncated_stream = accept_request(&listener);
        let _ = read_http_request(&mut truncated_stream);
        write_truncated_http_response(&mut truncated_stream, "application/json", b"{");
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let error = tauri::async_runtime::block_on(provider.submit(DocumentParseRequest {
        file_name: "body.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        bytes: b"body".to_vec(),
    }))
    .unwrap_err();

    assert!(matches!(
        error.code(),
        ProviderErrorCode::Network | ProviderErrorCode::ProviderResponse
    ));
    join_server(server);
}

#[test]
fn mineru_rejects_task_x_checksum_sha256_mismatch() {
    let archive = zip_fixture("paper/full.md", b"parsed");

    let error = mineru_download_from_mock(
        archive,
        "application/zip",
        vec![("x-checksum-sha256", "0".repeat(64))],
        Vec::new(),
        None,
    )
    .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    assert!(error.to_string().contains("checksum mismatch"));
}

#[test]
fn mineru_rejects_response_sha256_mismatch() {
    let archive = zip_fixture("paper/full.md", b"parsed");

    let error = mineru_download_from_mock(
        archive,
        "application/zip",
        Vec::new(),
        vec![("sha256", "0".repeat(64))],
        None,
    )
    .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    assert!(error.to_string().contains("checksum mismatch"));
}

#[test]
fn mineru_upload_retries_retryable_status() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU upload retry provider");
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        let mut submit_stream = accept_request(&listener);
        let _ = read_http_request(&mut submit_stream);
        let body = serde_json::json!({
            "code": 0,
            "data": {
                "batch_id": "upload-retry-batch",
                "file_urls": [format!("http://{address}/upload/retry.pdf")]
            }
        })
        .to_string();
        write_http_response(&mut submit_stream, "200 OK", "application/json", &[], body);
        drop(submit_stream);

        for (status, body) in [("500 Internal Server Error", "retry"), ("200 OK", "")] {
            let mut upload_stream = accept_request(&listener);
            let request = read_http_request(&mut upload_stream);
            assert!(String::from_utf8_lossy(&request).starts_with("PUT /upload/retry.pdf "));
            server_attempts.fetch_add(1, Ordering::SeqCst);
            write_http_response(&mut upload_stream, status, "text/plain", &[], body);
        }
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let id = tauri::async_runtime::block_on(provider.submit(DocumentParseRequest {
        file_name: "retry.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        bytes: b"retry-body".to_vec(),
    }))
    .unwrap();

    assert_eq!(id, RemoteTaskId("upload-retry-batch".to_string()));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    join_server(server);
}

#[test]
fn mineru_signed_upload_never_follows_redirects() {
    let target = TcpListener::bind("127.0.0.1:0").expect("bind redirect target");
    target.set_nonblocking(true).unwrap();
    let target_address = target.local_addr().unwrap();
    let target_server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match target.accept() {
                Ok((mut stream, _)) => {
                    let _ = read_http_request(&mut stream);
                    write_http_response(&mut stream, "200 OK", "text/plain", &[], "");
                    return true;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("accept redirect target: {error}"),
            }
        }
    });
    let api = TcpListener::bind("127.0.0.1:0").expect("bind MinerU redirect provider");
    let api_address = api.local_addr().unwrap();
    let api_server = thread::spawn(move || {
        let mut submit = accept_request(&api);
        let _ = read_http_request(&mut submit);
        let body = serde_json::json!({
            "code": 0,
            "data": {
                "batch_id": "redirect-batch",
                "file_urls": [format!("http://{api_address}/upload.pdf")]
            }
        })
        .to_string();
        write_http_response(&mut submit, "200 OK", "application/json", &[], body);
        drop(submit);

        let mut upload = accept_request(&api);
        let request = read_http_request(&mut upload);
        assert_eq!(request_body(&request), b"sensitive-document");
        write_http_response(
            &mut upload,
            "302 Found",
            "text/plain",
            &[("location", &format!("http://{target_address}/stolen"))],
            "",
        );
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{api_address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let result = tauri::async_runtime::block_on(provider.submit(DocumentParseRequest {
        file_name: "secret.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        bytes: b"sensitive-document".to_vec(),
    }));

    assert!(result.is_err());
    join_server(api_server);
    assert!(
        !target_server.join().unwrap(),
        "redirect target was contacted"
    );
}

#[test]
fn mineru_poll_classifies_permanent_http_failures_without_retry() {
    for (status, expected) in [
        ("400 Bad Request", ProviderErrorCode::ProviderResponse),
        ("401 Unauthorized", ProviderErrorCode::Authentication),
        ("403 Forbidden", ProviderErrorCode::Authentication),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU failure provider");
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut stream = accept_request(&listener);
            let request = read_http_request(&mut stream);
            assert!(String::from_utf8_lossy(&request)
                .starts_with("GET /api/v4/extract-results/batch/permanent-batch "));
            write_http_response(
                &mut stream,
                status,
                "application/json",
                &[],
                r#"{"message":"permanent"}"#,
            );
        });
        let provider = MinerUProvider::new(
            profile(ProviderKind::MinerU, &format!("http://{address}"), None),
            Some(SecretValue::new("mineru-secret").unwrap()),
        )
        .unwrap();

        let error = tauri::async_runtime::block_on(
            provider.poll(&RemoteTaskId("permanent-batch".to_string())),
        )
        .unwrap_err();

        assert_eq!(error.code(), expected);
        join_server(server);
    }
}

#[test]
fn mineru_preserves_authentication_for_truncated_401() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU truncated auth provider");
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        let mut stream = accept_request(&listener);
        let _ = read_http_request(&mut stream);
        server_attempts.fetch_add(1, Ordering::SeqCst);
        write_truncated_http_status_response(
            &mut stream,
            "401 Unauthorized",
            "application/json",
            br#"{"message":"unauthorized"}"#,
        );
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let error =
        tauri::async_runtime::block_on(provider.poll(&RemoteTaskId("truncated-auth".to_string())))
            .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::Authentication);
    assert_eq!(error.status(), Some(401));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    join_server(server);
}

#[test]
fn mineru_preserves_quota_for_final_truncated_429() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU truncated quota provider");
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let mut stream = accept_request(&listener);
            let _ = read_http_request(&mut stream);
            server_attempts.fetch_add(1, Ordering::SeqCst);
            write_truncated_http_status_response(
                &mut stream,
                "429 Too Many Requests",
                "application/json",
                br#"{"message":"quota"}"#,
            );
        }
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let error =
        tauri::async_runtime::block_on(provider.poll(&RemoteTaskId("truncated-quota".to_string())))
            .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::Quota);
    assert_eq!(error.status(), Some(429));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    join_server(server);
}

#[test]
fn mineru_declines_excessive_retry_after() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU Retry-After provider");
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        let mut stream = accept_request(&listener);
        let _ = read_http_request(&mut stream);
        server_attempts.fetch_add(1, Ordering::SeqCst);
        write_http_response(
            &mut stream,
            "429 Too Many Requests",
            "application/json",
            &[("retry-after", "60")],
            r#"{"message":"slow down"}"#,
        );
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let error = tauri::async_runtime::block_on(
        provider.poll(&RemoteTaskId("retry-after-batch".to_string())),
    )
    .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::Quota);
    assert_eq!(error.status(), Some(429));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    join_server(server);
}

#[test]
fn mineru_bounds_retryable_status_failures() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU bounded retry provider");
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let mut stream = accept_request(&listener);
            let _ = read_http_request(&mut stream);
            server_attempts.fetch_add(1, Ordering::SeqCst);
            write_http_response(
                &mut stream,
                "503 Service Unavailable",
                "application/json",
                &[],
                r#"{"message":"retry exhausted"}"#,
            );
        }
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let error =
        tauri::async_runtime::block_on(provider.poll(&RemoteTaskId("retry-exhausted".to_string())))
            .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    assert_eq!(error.status(), Some(503));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    join_server(server);
}

#[test]
fn mineru_poll_retries_truncated_response_body() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU poll body provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut truncated_stream = accept_request(&listener);
        let _ = read_http_request(&mut truncated_stream);
        write_truncated_http_response(&mut truncated_stream, "application/json", b"{");
        drop(truncated_stream);

        let mut poll_stream = accept_request(&listener);
        let _ = read_http_request(&mut poll_stream);
        let body = r#"{"code":0,"data":{"batch_id":"poll-body-retry","extract_result":[{"file_name":"paper.pdf","state":"running"}]}}"#;
        write_http_response(&mut poll_stream, "200 OK", "application/json", &[], body);
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let status =
        tauri::async_runtime::block_on(provider.poll(&RemoteTaskId("poll-body-retry".to_string())))
            .unwrap();

    assert_eq!(status.state, DocumentTaskState::Running);
    join_server(server);
}

#[test]
fn mineru_poll_classifies_remote_failed_state() {
    let body = r#"{"code":0,"data":{"batch_id":"failed-batch","extract_result":[{"file_name":"paper.pdf","state":"failed"}]}}"#;
    let (base_url, server) = mineru_poll_server(body, "failed-batch");
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &base_url, None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let status =
        tauri::async_runtime::block_on(provider.poll(&RemoteTaskId("failed-batch".to_string())))
            .unwrap();

    assert_eq!(status.state, DocumentTaskState::Failed);
    join_server(server);
}

#[test]
fn mineru_cancel_validates_id_before_reporting_unsupported() {
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, "http://127.0.0.1:9", None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let error =
        tauri::async_runtime::block_on(provider.cancel(&RemoteTaskId("../invalid".to_string())))
            .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
}

#[test]
fn mineru_rejects_bad_archive_content_type() {
    let error = mineru_download_from_mock(
        zip_fixture("paper/full.md", b"parsed"),
        "text/html",
        Vec::new(),
        Vec::new(),
        None,
    )
    .unwrap_err();

    assert!(error.to_string().contains("not a ZIP"));
}

#[test]
fn mineru_rejects_oversized_archive_content_length() {
    let error = mineru_download_from_mock(
        Vec::new(),
        "application/zip",
        Vec::new(),
        Vec::new(),
        Some(512 * 1024 * 1024 + 1),
    )
    .unwrap_err();

    assert!(error.to_string().contains("size limit"));
}

#[test]
fn mineru_rejects_invalid_zip() {
    let error = mineru_download_from_mock(
        b"not a zip".to_vec(),
        "application/zip",
        Vec::new(),
        Vec::new(),
        None,
    )
    .unwrap_err();

    assert!(error.to_string().contains("valid ZIP"));
}

#[test]
fn mineru_rejects_zip_path_traversal() {
    let error = mineru_download_from_mock(
        zip_fixture("../escape.txt", b"escape"),
        "application/zip",
        Vec::new(),
        Vec::new(),
        None,
    )
    .unwrap_err();

    assert!(error.to_string().contains("unsafe path"));
}

#[test]
fn mineru_rejects_zip_symlink() {
    let archive = zip_with_unix_mode("paper/link", b"target", 0o120777);
    let error = mineru_download_from_mock(archive, "application/zip", Vec::new(), Vec::new(), None)
        .unwrap_err();

    assert!(error.to_string().contains("unsafe path"));
}

#[test]
fn mineru_rejects_zip_expansion_bomb() {
    let mut archive = zip_fixture("paper/full.md", b"small");
    let local = zip_signature_offset(&archive, b"PK\x03\x04");
    let central = zip_signature_offset(&archive, b"PK\x01\x02");
    set_zip_u32(&mut archive, local + 22, 1024 * 1024 * 1024 + 1);
    set_zip_u32(&mut archive, central + 24, 1024 * 1024 * 1024 + 1);

    let error = mineru_download_from_mock(archive, "application/zip", Vec::new(), Vec::new(), None)
        .unwrap_err();

    assert!(error.to_string().contains("expanded size"));
}

#[test]
fn mineru_rejects_zip_crc_mismatch() {
    let mut archive = zip_fixture("paper/full.md", b"crc payload");
    let local = zip_signature_offset(&archive, b"PK\x03\x04");
    let central = zip_signature_offset(&archive, b"PK\x01\x02");
    set_zip_u32(&mut archive, local + 14, 0);
    set_zip_u32(&mut archive, central + 16, 0);

    let error = mineru_download_from_mock(archive, "application/zip", Vec::new(), Vec::new(), None)
        .unwrap_err();

    assert!(error.to_string().contains("integrity check"));
}

#[test]
fn mineru_download_retries_truncated_response_body() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU download body provider");
    let address = listener.local_addr().unwrap();
    let archive = zip_fixture("paper/full.md", b"parsed");
    let server_archive = archive.clone();
    let server = thread::spawn(move || {
        let mut poll_stream = accept_request(&listener);
        let _ = read_http_request(&mut poll_stream);
        let body = serde_json::json!({
            "code": 0,
            "data": {
                "batch_id": "download-body-retry",
                "extract_result": [{
                    "file_name": "paper.pdf",
                    "state": "done",
                    "full_zip_url": format!("http://{address}/artifact.zip")
                }]
            }
        })
        .to_string();
        write_http_response(&mut poll_stream, "200 OK", "application/json", &[], body);
        drop(poll_stream);

        let mut truncated_stream = accept_request(&listener);
        let _ = read_http_request(&mut truncated_stream);
        write_truncated_http_response(&mut truncated_stream, "application/zip", &server_archive);
        drop(truncated_stream);

        let mut download_stream = accept_request(&listener);
        let _ = read_http_request(&mut download_stream);
        write_http_response(
            &mut download_stream,
            "200 OK",
            "application/zip",
            &[],
            &server_archive,
        );
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let artifact = tauri::async_runtime::block_on(
        provider.download(&RemoteTaskId("download-body-retry".to_string())),
    )
    .unwrap();

    assert_eq!(artifact.bytes, archive);
    join_server(server);
}

#[test]
fn openai_chat_streams_text_tool_calls_and_usage() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"hello \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"function\":{\"name\":\"search\",\"arguments\":\"{\\\"q\\\":\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"Q355B\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":7,\"total_tokens\":18}}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, server) = chat_server("text/event-stream", body, "/v1/chat/completions");
    let provider = OpenAiProvider::new(
        profile(
            ProviderKind::OpenAiCompatible,
            &format!("{base_url}/v1"),
            Some("mock-model"),
        ),
        Some(SecretValue::new("sk-test").unwrap()),
    )
    .unwrap();
    assert!(provider.capabilities().tool_calls);
    assert!(provider.capabilities().json_schema);
    let mut events = Vec::new();

    let response = tauri::async_runtime::block_on(provider.chat(
        ChatRequest::single_turn("system", "user"),
        &mut |event| events.push(event),
        &|| false,
    ))
    .expect("stream chat");

    assert_eq!(response.text, "hello ");
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].name, "search");
    assert_eq!(response.tool_calls[0].arguments, r#"{"q":"Q355B"}"#);
    assert_eq!(response.usage.unwrap().total_tokens, 18);
    assert!(events
        .iter()
        .any(|event| matches!(event, ChatEvent::ToolCallDelta(_))));
    assert!(events
        .iter()
        .any(|event| matches!(event, ChatEvent::Usage(_))));
    join_server(server);
}

#[test]
fn openai_chat_skips_malformed_sse_and_accepts_json_response() {
    let stream_body = concat!(
        "data: {not-json}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"recovered\"}}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (stream_url, stream_server) =
        chat_server("text/event-stream", stream_body, "/chat/completions");
    let stream_provider = OpenAiProvider::new(
        profile(
            ProviderKind::OpenAiCompatible,
            &stream_url,
            Some("mock-model"),
        ),
        None,
    )
    .unwrap();
    let mut stream_events = Vec::new();
    let stream_response = tauri::async_runtime::block_on(stream_provider.chat(
        ChatRequest::single_turn("system", "user"),
        &mut |event| stream_events.push(event),
        &|| false,
    ))
    .unwrap();
    assert_eq!(stream_response.text, "recovered");
    join_server(stream_server);

    let json_body = r#"{"choices":[{"message":{"content":"json answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}"#;
    let (json_url, json_server) = chat_server("application/json", json_body, "/chat/completions");
    let json_provider = OpenAiProvider::new(
        profile(
            ProviderKind::OpenAiCompatible,
            &json_url,
            Some("mock-model"),
        ),
        None,
    )
    .unwrap();
    let mut json_events = Vec::new();
    let json_response = tauri::async_runtime::block_on(json_provider.chat(
        ChatRequest::single_turn("system", "user"),
        &mut |event| json_events.push(event),
        &|| false,
    ))
    .unwrap();
    assert_eq!(json_response.text, "json answer");
    assert_eq!(json_response.usage.unwrap().total_tokens, 5);
    join_server(json_server);
}

#[test]
fn openai_chat_rejects_truncated_or_fully_malformed_sse() {
    for body in [
        "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
        "data: {not-json}\n\n",
    ] {
        let (base_url, server) = chat_server("text/event-stream", body, "/chat/completions");
        let provider = OpenAiProvider::new(
            profile(
                ProviderKind::OpenAiCompatible,
                &base_url,
                Some("mock-model"),
            ),
            None,
        )
        .unwrap();

        let error = tauri::async_runtime::block_on(provider.chat(
            ChatRequest::single_turn("system", "user"),
            &mut |_| {},
            &|| false,
        ))
        .unwrap_err();

        assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
        join_server(server);
    }
}

#[test]
fn openai_json_keeps_multiple_tool_calls_without_indexes_separate() {
    let body = r#"{"choices":[{"message":{"content":"","tool_calls":[{"id":"call-a","function":{"name":"first","arguments":"{}"}},{"id":"call-b","function":{"name":"second","arguments":"{\"x\":1}"}}]},"finish_reason":"tool_calls"}]}"#;
    let (base_url, server) = chat_server("application/json", body, "/chat/completions");
    let provider = OpenAiProvider::new(
        profile(
            ProviderKind::OpenAiCompatible,
            &base_url,
            Some("mock-model"),
        ),
        None,
    )
    .unwrap();

    let response = tauri::async_runtime::block_on(provider.chat(
        ChatRequest::single_turn("system", "user"),
        &mut |_| {},
        &|| false,
    ))
    .unwrap();

    assert_eq!(response.tool_calls.len(), 2);
    assert_eq!(response.tool_calls[0].id, "call-a");
    assert_eq!(response.tool_calls[1].id, "call-b");
    join_server(server);
}

#[test]
fn ollama_streams_text_and_enforces_declared_capabilities() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"local \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let (base_url, server) = chat_server("text/event-stream", body, "/v1/chat/completions");
    let provider =
        OllamaProvider::new(profile(ProviderKind::Ollama, &base_url, Some("qwen3"))).unwrap();
    assert!(!provider.capabilities().tool_calls);
    assert!(!provider.capabilities().json_schema);

    let response = tauri::async_runtime::block_on(provider.chat(
        ChatRequest::single_turn("system", "user"),
        &mut |_| {},
        &|| false,
    ))
    .unwrap();

    assert_eq!(response.text, "local answer");
    join_server(server);

    let provider = OllamaProvider::new(profile(
        ProviderKind::Ollama,
        "http://127.0.0.1:9",
        Some("qwen3"),
    ))
    .unwrap();
    let mut tool_request = ChatRequest::single_turn("system", "user");
    tool_request.tools = Some(serde_json::json!([]));
    let error = tauri::async_runtime::block_on(provider.chat(tool_request, &mut |_| {}, &|| false))
        .unwrap_err();
    assert_eq!(error.code(), ProviderErrorCode::UnsupportedCapability);

    let mut json_request = ChatRequest::single_turn("system", "user");
    json_request.response_format = Some(serde_json::json!({"type": "json_object"}));
    let error = tauri::async_runtime::block_on(provider.chat(json_request, &mut |_| {}, &|| false))
        .unwrap_err();
    assert_eq!(error.code(), ProviderErrorCode::UnsupportedCapability);
}

#[test]
fn siliconflow_embedding_batches_inputs_and_preserves_index_order() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind embedding provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for expected_batch_size in [64, 1] {
            let mut stream = accept_request(&listener);
            let request = read_http_request(&mut stream);
            let request_text = String::from_utf8_lossy(&request);
            assert!(request_text.starts_with("POST /v1/embeddings "));
            assert!(request_text
                .to_ascii_lowercase()
                .contains("authorization: bearer sk-siliconflow"));
            let request = json_request_body(&request);
            assert_eq!(request["model"], "custom-embedding");
            assert_eq!(
                request["input"].as_array().unwrap().len(),
                expected_batch_size
            );
            let inputs = request["input"].as_array().unwrap();
            let data = inputs
                .iter()
                .enumerate()
                .rev()
                .map(|(index, input)| {
                    serde_json::json!({
                        "index": index,
                        "embedding": [index as f32, input.as_str().unwrap().len() as f32]
                    })
                })
                .collect::<Vec<_>>();
            let body = serde_json::json!({"data": data}).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-siliconflow").unwrap()),
        SiliconFlowPlan::Pro,
        Some("custom-embedding".to_string()),
        Some("custom-reranker".to_string()),
    )
    .unwrap();
    assert_eq!(provider.plan(), SiliconFlowPlan::Pro);
    assert_eq!(
        serde_json::to_value(provider.plan()).unwrap(),
        serde_json::json!("pro")
    );

    let inputs = (0..65).map(|index| format!("item-{index}"));
    let response = tauri::async_runtime::block_on(provider.embed(inputs.collect())).unwrap();

    assert_eq!(response.model_id, "custom-embedding");
    assert_eq!(response.vectors.len(), 65);
    for (index, vector) in response.vectors.iter().enumerate() {
        assert_eq!(
            vector,
            &vec![(index % 64) as f32, format!("item-{index}").len() as f32],
            "embedding vector {index}"
        );
    }
    join_server(server);
}

#[test]
fn siliconflow_sends_default_embedding_and_rerank_models() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind default model provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut embedding_stream = accept_request(&listener);
        let embedding_request = read_http_request(&mut embedding_stream);
        let embedding_request = json_request_body(&embedding_request);
        assert_eq!(embedding_request["model"], DEFAULT_EMBEDDING_MODEL);
        let embedding_body = r#"{"data":[{"index":0,"embedding":[1.0,2.0]}]}"#;
        let embedding_response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            embedding_body.len(),
            embedding_body
        );
        embedding_stream
            .write_all(embedding_response.as_bytes())
            .unwrap();
        drop(embedding_stream);

        let mut rerank_stream = accept_request(&listener);
        let rerank_request = read_http_request(&mut rerank_stream);
        let rerank_request = json_request_body(&rerank_request);
        assert_eq!(rerank_request["model"], DEFAULT_RERANK_MODEL);
        let rerank_body = r#"{"results":[{"index":0,"relevance_score":0.75}]}"#;
        let rerank_response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            rerank_body.len(),
            rerank_body
        );
        rerank_stream.write_all(rerank_response.as_bytes()).unwrap();
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    tauri::async_runtime::block_on(provider.embed(vec!["embedding".to_string()])).unwrap();
    tauri::async_runtime::block_on(provider.rerank(
        "query".to_string(),
        vec![bloomery::providers::capabilities::RerankDocument {
            id: "doc".to_string(),
            text: "document".to_string(),
        }],
    ))
    .unwrap();
    join_server(server);
}

#[test]
fn siliconflow_rejects_dimension_change_across_batches() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind dimension provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for expected_size in [64, 1] {
            let mut stream = accept_request(&listener);
            let request = read_http_request(&mut stream);
            let request = json_request_body(&request);
            assert_eq!(request["input"].as_array().unwrap().len(), expected_size);
            let data = (0..expected_size)
                .map(|index| {
                    let embedding = if expected_size == 64 {
                        vec![1.0, 2.0]
                    } else {
                        vec![3.0]
                    };
                    serde_json::json!({"index": index, "embedding": embedding})
                })
                .collect::<Vec<_>>();
            let body = serde_json::json!({"data": data}).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    let error = tauri::async_runtime::block_on(
        provider.embed((0..65).map(|index| format!("item-{index}")).collect()),
    )
    .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    assert!(error.to_string().contains("dimension"));
    join_server(server);
}

#[test]
fn siliconflow_rerank_normalizes_candidate_ids_and_scores() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind rerank provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept_request(&listener);
        let request = read_http_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request);
        assert!(request_text.starts_with("POST /v1/rerank "));
        assert!(request_text
            .to_ascii_lowercase()
            .contains("authorization: bearer sk-siliconflow"));
        let request = json_request_body(&request);
        assert_eq!(request["model"], "custom-reranker");
        let body = r#"{"results":[{"index":1,"relevance_score":0.91},{"index":0,"relevance_score":0.32}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-siliconflow").unwrap()),
        SiliconFlowPlan::Free,
        Some("custom-embedding".to_string()),
        Some("custom-reranker".to_string()),
    )
    .unwrap();

    let documents = vec![
        bloomery::providers::capabilities::RerankDocument {
            id: "doc-a".to_string(),
            text: "first".to_string(),
        },
        bloomery::providers::capabilities::RerankDocument {
            id: "doc-b".to_string(),
            text: "second".to_string(),
        },
    ];
    let results =
        tauri::async_runtime::block_on(provider.rerank("query".to_string(), documents)).unwrap();

    assert_eq!(results[0].id, "doc-b");
    assert_eq!(results[0].score, 0.91);
    assert_eq!(results[1].id, "doc-a");
    assert_eq!(results[1].score, 0.32);
    assert_eq!(provider.plan(), SiliconFlowPlan::Free);
    join_server(server);
}

#[test]
fn siliconflow_retries_quota_once_and_never_retries_authentication() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind retry provider");
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        for status in ["429 Too Many Requests", "200 OK"] {
            let mut stream = accept_request(&listener);
            let _ = read_http_request(&mut stream);
            server_attempts.fetch_add(1, Ordering::SeqCst);
            let body = if status.starts_with("200") {
                r#"{"data":[{"index":0,"embedding":[1.0,2.0]}]}"#
            } else {
                r#"{"message":"slow down"}"#
            };
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\nretry-after: 0\r\ncontent-length: {}\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-siliconflow").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        EmbeddingProvider::capabilities(&provider).model_id,
        DEFAULT_EMBEDDING_MODEL
    );
    assert_eq!(
        EmbeddingProvider::capabilities(&provider).max_batch_size,
        Some(64)
    );
    assert_eq!(
        RerankProvider::capabilities(&provider).model_id,
        DEFAULT_RERANK_MODEL
    );

    let response = tauri::async_runtime::block_on(provider.embed(vec!["retry".to_string()]))
        .expect("retry quota response");

    assert_eq!(response.vectors, vec![vec![1.0, 2.0]]);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    join_server(server);

    let (base_url, auth_server) = status_server(
        "401 Unauthorized",
        r#"{"message":"invalid token"}"#,
        "/v1/embeddings",
    );
    let provider = SiliconFlowProvider::with_models(
        profile(ProviderKind::SiliconFlow, &format!("{base_url}/v1"), None),
        Some(SecretValue::new("sk-invalid").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();
    let error =
        tauri::async_runtime::block_on(provider.embed(vec!["auth".to_string()])).unwrap_err();
    assert_eq!(error.code(), ProviderErrorCode::Authentication);
    join_server(auth_server);
}

#[test]
fn siliconflow_rejects_inconsistent_embedding_dimensions() {
    let body = r#"{"data":[{"index":0,"embedding":[1.0,2.0]},{"index":1,"embedding":[3.0]}]}"#;
    let (base_url, server) = status_server("200 OK", body, "/v1/embeddings");
    let provider = SiliconFlowProvider::with_models(
        profile(ProviderKind::SiliconFlow, &format!("{base_url}/v1"), None),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    let error = tauri::async_runtime::block_on(
        provider.embed(vec!["first".to_string(), "second".to_string()]),
    )
    .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    assert!(error.to_string().contains("dimension"));
    join_server(server);
}

#[test]
fn siliconflow_rejects_values_that_overflow_f32() {
    let embedding_body = r#"{"data":[{"index":0,"embedding":[1e300]}]}"#;
    let (base_url, embedding_server) = status_server("200 OK", embedding_body, "/v1/embeddings");
    let provider = SiliconFlowProvider::with_models(
        profile(ProviderKind::SiliconFlow, &format!("{base_url}/v1"), None),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();
    let error =
        tauri::async_runtime::block_on(provider.embed(vec!["value".to_string()])).unwrap_err();
    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    join_server(embedding_server);

    let rerank_body = r#"{"results":[{"index":0,"relevance_score":1e300}]}"#;
    let (base_url, rerank_server) = status_server("200 OK", rerank_body, "/v1/rerank");
    let provider = SiliconFlowProvider::with_models(
        profile(ProviderKind::SiliconFlow, &format!("{base_url}/v1"), None),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();
    let error = tauri::async_runtime::block_on(provider.rerank(
        "query".to_string(),
        vec![bloomery::providers::capabilities::RerankDocument {
            id: "doc".to_string(),
            text: "text".to_string(),
        }],
    ))
    .unwrap_err();
    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    join_server(rerank_server);
}

#[test]
fn siliconflow_rejects_nonzero_values_that_underflow_f32() {
    let embedding_body = r#"{"data":[{"index":0,"embedding":[1e-300]}]}"#;
    let (base_url, embedding_server) = status_server("200 OK", embedding_body, "/v1/embeddings");
    let provider = SiliconFlowProvider::with_models(
        profile(ProviderKind::SiliconFlow, &format!("{base_url}/v1"), None),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    let error =
        tauri::async_runtime::block_on(provider.embed(vec!["value".to_string()])).unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    join_server(embedding_server);

    let rerank_body = r#"{"results":[{"index":0,"relevance_score":1e-300}]}"#;
    let (base_url, rerank_server) = status_server("200 OK", rerank_body, "/v1/rerank");
    let provider = SiliconFlowProvider::with_models(
        profile(ProviderKind::SiliconFlow, &format!("{base_url}/v1"), None),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    let error = tauri::async_runtime::block_on(provider.rerank(
        "query".to_string(),
        vec![bloomery::providers::capabilities::RerankDocument {
            id: "doc".to_string(),
            text: "text".to_string(),
        }],
    ))
    .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    join_server(rerank_server);
}

#[test]
fn siliconflow_persistent_quota_stops_after_bounded_attempts() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind quota provider");
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let mut stream = accept_request(&listener);
            let _ = read_http_request(&mut stream);
            server_attempts.fetch_add(1, Ordering::SeqCst);
            let body = r#"{"message":"quota exhausted"}"#;
            let response = format!(
                "HTTP/1.1 429 Too Many Requests\r\nretry-after: 0\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    let error =
        tauri::async_runtime::block_on(provider.embed(vec!["quota".to_string()])).unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::Quota);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    join_server(server);
}

#[test]
fn siliconflow_retries_transient_server_failures() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind transient provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for status in ["503 Service Unavailable", "200 OK"] {
            let mut stream = accept_request(&listener);
            let _ = read_http_request(&mut stream);
            let body = if status.starts_with("200") {
                r#"{"data":[{"index":0,"embedding":[4.0,5.0]}]}"#
            } else {
                r#"{"message":"temporary failure"}"#
            };
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    let response = tauri::async_runtime::block_on(provider.embed(vec!["retry".to_string()]))
        .expect("retry transient response");

    assert_eq!(response.vectors, vec![vec![4.0, 5.0]]);
    join_server(server);
}

#[test]
fn siliconflow_retries_when_response_body_is_truncated() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind truncated body provider");
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&attempts);
    let server = thread::spawn(move || {
        for attempt in 0..2 {
            let mut stream = accept_request(&listener);
            let _ = read_http_request(&mut stream);
            server_attempts.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 100\r\n\r\n{\"data\":[",
                    )
                    .unwrap();
            } else {
                let body = r#"{"data":[{"index":0,"embedding":[7.0,8.0]}]}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(), body
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        }
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    let response = tauri::async_runtime::block_on(provider.embed(vec!["retry".to_string()]))
        .expect("retry truncated response body");

    assert_eq!(response.vectors, vec![vec![7.0, 8.0]]);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    join_server(server);
}

#[test]
fn siliconflow_preserves_status_category_when_error_body_is_truncated() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind truncated auth provider");
    let address = listener.local_addr().unwrap();
    let auth_attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&auth_attempts);
    let server = thread::spawn(move || {
        let mut stream = accept_request(&listener);
        let _ = read_http_request(&mut stream);
        server_attempts.fetch_add(1, Ordering::SeqCst);
        stream
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: 100\r\n\r\n{",
            )
            .unwrap();
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    let error =
        tauri::async_runtime::block_on(provider.embed(vec!["auth".to_string()])).unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::Authentication);
    assert_eq!(auth_attempts.load(Ordering::SeqCst), 1);
    join_server(server);

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind truncated quota provider");
    let address = listener.local_addr().unwrap();
    let quota_attempts = Arc::new(AtomicUsize::new(0));
    let server_attempts = Arc::clone(&quota_attempts);
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let mut stream = accept_request(&listener);
            let _ = read_http_request(&mut stream);
            server_attempts.fetch_add(1, Ordering::SeqCst);
            stream
                .write_all(
                    b"HTTP/1.1 429 Too Many Requests\r\nretry-after: 0\r\ncontent-type: application/json\r\ncontent-length: 100\r\n\r\n{",
                )
                .unwrap();
        }
    });
    let provider = SiliconFlowProvider::with_models(
        profile(
            ProviderKind::SiliconFlow,
            &format!("http://{address}/v1"),
            None,
        ),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();

    let error =
        tauri::async_runtime::block_on(provider.embed(vec!["quota".to_string()])).unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::Quota);
    assert_eq!(quota_attempts.load(Ordering::SeqCst), 3);
    join_server(server);
}

#[test]
fn siliconflow_rejects_duplicate_or_missing_rerank_indexes() {
    let body =
        r#"{"results":[{"index":0,"relevance_score":0.9},{"index":0,"relevance_score":0.8}]}"#;
    let (base_url, server) = status_server("200 OK", body, "/v1/rerank");
    let provider = SiliconFlowProvider::with_models(
        profile(ProviderKind::SiliconFlow, &format!("{base_url}/v1"), None),
        Some(SecretValue::new("sk-test").unwrap()),
        SiliconFlowPlan::Free,
        None,
        None,
    )
    .unwrap();
    let documents = vec![
        bloomery::providers::capabilities::RerankDocument {
            id: "first".to_string(),
            text: "first".to_string(),
        },
        bloomery::providers::capabilities::RerankDocument {
            id: "second".to_string(),
            text: "second".to_string(),
        },
    ];

    let error = tauri::async_runtime::block_on(provider.rerank("query".to_string(), documents))
        .unwrap_err();

    assert_eq!(error.code(), ProviderErrorCode::ProviderResponse);
    assert!(error.to_string().contains("index"));
    join_server(server);
}

#[test]
fn chat_cancellation_capabilities_and_endpoints_are_normalized() {
    let deepseek = resolve_chat_profile("deepseek", "", "deepseek-chat").unwrap();
    assert_eq!(deepseek.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(deepseek.base_url, "https://api.deepseek.com");
    let ollama = resolve_chat_profile("ollama", "", "qwen3").unwrap();
    assert_eq!(ollama.kind, ProviderKind::Ollama);
    assert_eq!(ollama.base_url, "http://127.0.0.1:11434");

    assert_eq!(
        default_openai_base_url("deepseek"),
        Some("https://api.deepseek.com")
    );
    assert_eq!(
        default_openai_base_url("openai"),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(default_openai_base_url("custom"), None);
    assert_eq!(default_ollama_base_url(), "http://127.0.0.1:11434");
    assert_eq!(
        normalize_openai_chat_url("https://api.deepseek.com"),
        "https://api.deepseek.com/chat/completions"
    );
    assert_eq!(
        normalize_openai_chat_url("https://api.openai.com/v1/"),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        normalize_ollama_chat_url("http://127.0.0.1:11434"),
        "http://127.0.0.1:11434/v1/chat/completions"
    );

    let provider = OpenAiProvider::new(
        profile(
            ProviderKind::OpenAiCompatible,
            "http://127.0.0.1:9",
            Some("mock-model"),
        ),
        None,
    )
    .unwrap();
    let mut events = Vec::new();
    let cancelled = tauri::async_runtime::block_on(provider.chat(
        ChatRequest::single_turn("system", "user"),
        &mut |event| events.push(event),
        &|| true,
    ))
    .unwrap();
    assert!(cancelled.cancelled);
    assert!(events.is_empty());

    let error = provider
        .capabilities()
        .require(ProviderCapability::DocumentParser)
        .unwrap_err();
    assert_eq!(error.code(), ProviderErrorCode::UnsupportedCapability);
}

#[test]
fn chat_cancellation_interrupts_an_idle_stream() {
    assert_idle_response_cancellable("200 OK", "text/event-stream");
}

#[test]
fn chat_cancellation_interrupts_idle_json_and_error_bodies() {
    assert_idle_response_cancellable("200 OK", "application/json");
    assert_idle_response_cancellable("500 Internal Server Error", "application/json");
}

fn assert_idle_response_cancellable(status: &'static str, content_type: &'static str) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind idle provider");
    let address = listener.local_addr().unwrap();
    let release = Arc::new(AtomicBool::new(false));
    let response_started = Arc::new(AtomicBool::new(false));
    let server_release = Arc::clone(&release);
    let server_response_started = Arc::clone(&response_started);
    let server = thread::spawn(move || {
        let mut stream = accept_request(&listener);
        let _ = read_http_request(&mut stream);
        let headers = format!(
            "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ntransfer-encoding: chunked\r\n\r\n"
        );
        stream.write_all(headers.as_bytes()).unwrap();
        server_response_started.store(true, Ordering::SeqCst);
        let fallback = Instant::now();
        while !server_release.load(Ordering::SeqCst) && fallback.elapsed() < Duration::from_secs(3)
        {
            thread::sleep(Duration::from_millis(10));
        }
    });
    let provider = OpenAiProvider::new(
        profile(
            ProviderKind::OpenAiCompatible,
            &format!("http://{address}"),
            Some("mock-model"),
        ),
        None,
    )
    .unwrap();
    let started = Instant::now();

    let result = tauri::async_runtime::block_on(provider.chat(
        ChatRequest::single_turn("system", "user"),
        &mut |_| {},
        &|| {
            response_started.load(Ordering::SeqCst)
                && started.elapsed() >= Duration::from_millis(50)
        },
    ));
    let elapsed = started.elapsed();
    release.store(true, Ordering::SeqCst);
    join_server(server);
    let response = result.expect("cancel idle response");

    assert!(response.cancelled);
    assert!(elapsed < Duration::from_secs(2));
}

fn chat_server(
    content_type: &'static str,
    body: &'static str,
    expected_path: &'static str,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept_request(&listener);
        let request = read_http_request(&mut stream);
        let request = String::from_utf8_lossy(&request);
        assert!(request.starts_with(&format!("POST {expected_path} ")));
        assert!(request.contains(r#""stream":true"#));
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{address}"), server)
}

fn status_server(
    status: &'static str,
    body: &'static str,
    expected_path: &'static str,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind status provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept_request(&listener);
        let request = read_http_request(&mut stream);
        let request = String::from_utf8_lossy(&request);
        assert!(request.starts_with(&format!("POST {expected_path} ")));
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{address}"), server)
}

fn accept_request(listener: &TcpListener) -> TcpStream {
    listener
        .set_nonblocking(true)
        .expect("configure mock listener");
    let deadline = Instant::now() + MOCK_SERVER_TIMEOUT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("configure blocking mock stream");
                stream
                    .set_read_timeout(Some(MOCK_SERVER_TIMEOUT))
                    .expect("configure mock read timeout");
                stream
                    .set_write_timeout(Some(MOCK_SERVER_TIMEOUT))
                    .expect("configure mock write timeout");
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

fn join_server(server: JoinHandle<()>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(server.join());
    });
    receiver
        .recv_timeout(MOCK_SERVER_TIMEOUT)
        .expect("mock server join timed out")
        .expect("mock server panicked");
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

fn json_request_body(request: &[u8]) -> serde_json::Value {
    serde_json::from_slice(request_body(request)).expect("JSON request body")
}

fn request_body(request: &[u8]) -> &[u8] {
    let body_start = request
        .windows(4)
        .position(|item| item == b"\r\n\r\n")
        .expect("request headers")
        + 4;
    &request[body_start..]
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

fn write_truncated_http_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    write_truncated_http_status_response(stream, "200 OK", content_type, body);
}

fn write_truncated_http_status_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) {
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len() + 1
    );
    stream.write_all(response.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
}

fn zip_fixture(path: &str, content: &[u8]) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    archive
        .start_file(path, SimpleFileOptions::default())
        .expect("start zip entry");
    archive.write_all(content).expect("write zip entry");
    archive.finish().expect("finish zip").into_inner()
}

fn mineru_download_from_mock(
    archive: Vec<u8>,
    content_type: &'static str,
    task_checksums: Vec<(&'static str, String)>,
    response_checksums: Vec<(&'static str, String)>,
    declared_length: Option<usize>,
) -> Result<ParsedDocumentArtifact, ProviderError> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU download provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut result = serde_json::json!({
            "file_name": "paper.pdf",
            "state": "done",
            "full_zip_url": format!("http://{address}/artifact.zip")
        });
        for (name, value) in task_checksums {
            result[name] = serde_json::Value::String(value);
        }
        let poll_body = serde_json::json!({
            "code": 0,
            "data": {
                "batch_id": "download-batch",
                "extract_result": [result]
            }
        })
        .to_string();
        let mut poll_stream = accept_request(&listener);
        let poll_request = read_http_request(&mut poll_stream);
        assert!(String::from_utf8_lossy(&poll_request)
            .starts_with("GET /api/v4/extract-results/batch/download-batch "));
        write_http_response(
            &mut poll_stream,
            "200 OK",
            "application/json",
            &[],
            poll_body,
        );
        drop(poll_stream);

        let mut download_stream = accept_request(&listener);
        let download_request = read_http_request(&mut download_stream);
        assert!(String::from_utf8_lossy(&download_request).starts_with("GET /artifact.zip "));
        let headers = response_checksums
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect::<Vec<_>>();
        if let Some(length) = declared_length {
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {length}\r\nconnection: close\r\n\r\n"
            );
            download_stream.write_all(response.as_bytes()).unwrap();
        } else {
            write_http_response(
                &mut download_stream,
                "200 OK",
                content_type,
                &headers,
                archive,
            );
        }
    });
    let provider = MinerUProvider::new(
        profile(ProviderKind::MinerU, &format!("http://{address}"), None),
        Some(SecretValue::new("mineru-secret").unwrap()),
    )
    .unwrap();

    let result = tauri::async_runtime::block_on(
        provider.download(&RemoteTaskId("download-batch".to_string())),
    );
    join_server(server);
    result
}

fn mineru_poll_server(body: &'static str, batch_id: &'static str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind MinerU poll provider");
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut stream = accept_request(&listener);
        let request = read_http_request(&mut stream);
        assert!(String::from_utf8_lossy(&request)
            .starts_with(&format!("GET /api/v4/extract-results/batch/{batch_id} ")));
        write_http_response(&mut stream, "200 OK", "application/json", &[], body);
    });
    (format!("http://{address}"), server)
}

fn zip_with_unix_mode(path: &str, content: &[u8], mode: u32) -> Vec<u8> {
    let mut archive = zip_fixture(path, content);
    let central = zip_signature_offset(&archive, b"PK\x01\x02");
    set_zip_u32(&mut archive, central + 38, mode << 16);
    archive
}

fn zip_signature_offset(archive: &[u8], signature: &[u8; 4]) -> usize {
    archive
        .windows(signature.len())
        .position(|window| window == signature)
        .expect("ZIP signature")
}

fn set_zip_u32(archive: &mut [u8], offset: usize, value: u32) {
    archive[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
