//! 合成密钥端到端扫描：注入一个已知的合成密钥，驱动典型流程（provider 调用
//! 失败路径、日志输出、diagnostics health/error JSON、备份导出），随后断言该
//! 密钥在六个输出面均不可见：
//!   1. SQLite 数据库文件内容
//!   2. 日志（脱敏后的日志行）
//!   3. crash/panic 消息
//!   4. 备份导出归档
//!   5. diagnostics（health/error JSON）
//!   6. 进程参数（process arguments）
//!
//! 这些测试共享进程级脱敏登记表并会安装/替换 panic hook，建议串行运行：
//!   cargo test --test secret_scan -- --test-threads=1

use bloomery::diagnostics::observability::{
    format_panic, format_panic_diagnostics, global_redactor, redact_json, redact_line,
    register_secret,
};
use bloomery::providers::http::ProviderError;
use bloomery::providers::profiles::{ProviderKind, ProviderProfile};
use bloomery::storage::backup::create_backup;
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::provider_profiles;
use bloomery::storage::secrets::SecretValue;
use reqwest::StatusCode;
use rusqlite::Connection;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

/// 合成的已知密钥。仅存在于本测试文件（tests/ 不在离线安全门禁的源扫描范围内）。
const SECRET: &str = "sk-bloomery-synthetic-canary-DEADBEEF01234567";

fn register() {
    register_secret(&SecretValue::new(SECRET).expect("synthetic secret must be non-empty"));
}

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("bloomery-secret-scan-{label}-{}", Uuid::new_v4()))
}

fn scan_dir_for(root: &Path, needle: &[u8]) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if scan_dir_for(&path, needle) {
                return true;
            }
        } else if let Ok(bytes) = fs::read(&path) {
            if bytes.windows(needle.len()).any(|window| window == needle) {
                return true;
            }
        }
    }
    false
}

fn zip_decompressed_contains(archive: &Path, needle: &[u8]) -> bool {
    let file = fs::File::open(archive).expect("open backup archive");
    let mut zip = zip::ZipArchive::new(file).expect("read backup archive");
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).expect("read backup entry");
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .expect("decompress backup entry");
        if bytes.windows(needle.len()).any(|window| window == needle) {
            return true;
        }
    }
    false
}

#[test]
fn synthetic_secret_absent_from_sqlite_and_backup_export() {
    register();
    let root = temp_root("sqlite-backup");
    fs::create_dir_all(&root).expect("create fixture root");
    let db_path = root.join("bloomery.sqlite3");
    let mut conn = Connection::open(&db_path).expect("open database");
    migrate(&mut conn).expect("migrate database");

    // provider profile 只保存密钥的引用名（credential_name），绝不保存明文。
    let profile = ProviderProfile {
        id: Uuid::new_v4(),
        kind: ProviderKind::OpenAiCompatible,
        display_name: "Synthetic provider".to_string(),
        base_url: "https://provider.example".to_string(),
        model_id: Some("gpt-x".to_string()),
        secret_ref: Some("api_key".to_string()),
        enabled: true,
    };
    provider_profiles::save(&mut conn, "local", profile).expect("save provider profile");
    conn.execute(
        "INSERT INTO settings (workspace_id, key, value_json, updated_at)
         VALUES ('local', 'provider.note', '{\"ref\":\"api_key\"}', 'now')",
        [],
    )
    .expect("write setting");
    // 检查点，确保数据落入主库文件而非仅存在于 WAL 中。
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

    // 备份导出。
    let content_root = root.join("content");
    let archive = root.join("bloomery.bloomery-backup");
    create_backup(&conn, &db_path, &content_root, &archive).expect("create backup");
    drop(conn);

    // 1. SQLite 数据库文件（含 wal/shm sidecar）内容不得包含密钥明文。
    assert!(
        !scan_dir_for(&root, SECRET.as_bytes()),
        "synthetic secret leaked into on-disk sqlite or backup bytes"
    );
    // 4. 备份导出归档解压后同样不得包含密钥明文。
    assert!(
        !zip_decompressed_contains(&archive, SECRET.as_bytes()),
        "synthetic secret leaked into decompressed backup export"
    );

    fs::remove_dir_all(&root).expect("remove fixture");
}

#[test]
fn synthetic_secret_absent_from_provider_failure_and_logs() {
    register();
    // provider 调用失败路径：401 响应体里含密钥，错误经 Redactor 构造。
    let body =
        format!(r#"{{"error":{{"message":"invalid api key {SECRET}","code":"unauthorized"}}}}"#);
    let error = ProviderError::from_status(StatusCode::UNAUTHORIZED, &body, &global_redactor());
    let display = error.to_string();
    let serialized = serde_json::to_string(&error).expect("serialize provider error");
    assert!(
        !display.contains(SECRET),
        "secret leaked in provider error display"
    );
    assert!(
        !serialized.contains(SECRET),
        "secret leaked in serialized provider error"
    );

    // 2. 日志：模拟把失败详情写入日志的路径，经脱敏日志助手处理。
    let log_line = redact_line(&format!("openai chat failed: {body}"));
    assert!(!log_line.contains(SECRET), "secret leaked in log line");
    assert!(log_line.contains("[REDACTED]"));
}

#[test]
fn synthetic_secret_absent_from_diagnostics_json() {
    register();
    // 5. diagnostics health/error JSON：既覆盖普通字符串字段，也覆盖敏感键。
    let error_json = serde_json::json!({
        "status": "error",
        "message": format!("provider handshake failed with {SECRET}"),
        "detail": {
            "authorization": format!("Bearer {SECRET}"),
            "attempts": 3,
        }
    });
    let redacted = redact_json(&error_json).to_string();
    assert!(
        !redacted.contains(SECRET),
        "secret leaked in diagnostics JSON"
    );
    assert!(redacted.contains("[REDACTED]"));
}

static PANIC_CAPTURE: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn panic_capture() -> &'static Mutex<Vec<String>> {
    PANIC_CAPTURE.get_or_init(|| Mutex::new(Vec::new()))
}

#[test]
fn synthetic_secret_never_appears_in_panic_messages() {
    register();
    panic_capture()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();

    // 安装一个走脱敏路径（format_panic）的 hook，并把结果收集到缓冲区。
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        let line = format_panic(info);
        panic_capture()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(line);
    }));
    let result = std::panic::catch_unwind(|| {
        panic!("provider exploded while using {SECRET}");
    });
    std::panic::set_hook(previous);

    // 3. crash/panic 消息不得包含密钥明文。
    assert!(result.is_err(), "the closure must have panicked");
    let messages = panic_capture()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert!(!messages.is_empty(), "panic hook captured nothing");
    for line in messages.iter() {
        assert!(
            !line.contains(SECRET),
            "secret leaked in panic message: {line}"
        );
    }
    assert!(
        messages.iter().any(|line| line.contains("[REDACTED]")),
        "panic message was not redacted"
    );
}

#[test]
fn synthetic_secret_absent_from_panic_backtrace_branch() {
    register();
    // 显式启用 backtrace 分支：即便额外打印栈帧回溯，输出也必须全部经过脱敏，
    // 绝不能包含合成密钥；栈帧回溯本身不含原始 panic 消息文本。
    std::env::set_var("BLOOMERY_PANIC_BACKTRACE", "1");

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        let lines = format_panic_diagnostics(info);
        let mut captured = panic_capture()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        captured.clear();
        captured.extend(lines);
    }));
    let result = std::panic::catch_unwind(|| {
        panic!("provider exploded while using {SECRET}");
    });
    std::panic::set_hook(previous);
    std::env::remove_var("BLOOMERY_PANIC_BACKTRACE");

    assert!(result.is_err(), "the closure must have panicked");
    let messages = panic_capture()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    // backtrace 分支应至少产出脱敏消息行 + 栈帧回溯行两行。
    assert!(
        messages.len() >= 2,
        "backtrace branch did not emit the extra backtrace line"
    );
    for line in messages.iter() {
        assert!(
            !line.contains(SECRET),
            "secret leaked in backtrace-branch output: {line}"
        );
    }
    assert!(
        messages.iter().any(|line| line.contains("[REDACTED]")),
        "panic message was not redacted in backtrace branch"
    );
    assert!(
        messages
            .iter()
            .any(|line| line.contains("panic backtrace:")),
        "backtrace branch did not emit a backtrace section"
    );
}

#[test]
fn synthetic_secret_absent_from_process_arguments() {
    register();
    // 6. 进程参数：密钥绝不通过命令行参数传递。
    assert!(
        std::env::args().all(|arg| !arg.contains(SECRET)),
        "synthetic secret leaked into process arguments"
    );
    // 附加：环境变量同样不得携带密钥明文。
    assert!(
        std::env::vars().all(|(_, value)| !value.contains(SECRET)),
        "synthetic secret leaked into environment variables"
    );
}
