#![cfg(windows)]

use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use suna::agent::protocol::PermissionRisk;
use suna::agent::runtime::{CancellationToken, LocalToolExecutor, ToolExecutor, ToolInvocation};
use uuid::Uuid;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("suna-local-tools-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("workspace/nested")).unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        fs::write(root.join("workspace/nested/text.txt"), "你好，local tools").unwrap();
        fs::write(root.join("outside/secret.txt"), "outside").unwrap();
        Self(root)
    }

    fn tools(&self, file: bool, shell: bool) -> LocalToolExecutor {
        LocalToolExecutor::new(self.0.join("workspace"), Vec::new(), file, shell).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn invocation(name: &str, arguments: Value) -> ToolInvocation {
    ToolInvocation {
        tool_call_id: Uuid::new_v4(),
        tool_id: format!("builtin.{name}"),
        tool_name: name.to_string(),
        arguments,
    }
}

#[tokio::test]
async fn file_tools_read_list_write_utf8_and_bound_output() {
    let fixture = Fixture::new();
    let tools = fixture.tools(true, false);
    tools.snapshot().unwrap();
    let read = tools
        .execute(
            invocation(
                "read_file",
                json!({"path": "nested/text.txt", "max_bytes": 7}),
            ),
            CancellationToken::new(|| false),
        )
        .await
        .unwrap();
    assert_eq!(read["content"], "你好");
    assert_eq!(read["next_offset_bytes"], 6);
    assert_eq!(read["truncated"], true);
    let write = tools
        .execute(
            invocation(
                "write_file",
                json!({"path": "draft.txt", "content": "UTF-8 草稿"}),
            ),
            CancellationToken::new(|| false),
        )
        .await
        .unwrap();
    assert_eq!(write["bytes_written"], "UTF-8 草稿".len());
    assert_eq!(
        fs::read_to_string(fixture.0.join("workspace/draft.txt")).unwrap(),
        "UTF-8 草稿"
    );
    let listed = tools
        .execute(
            invocation("list_directory", json!({"path": ".", "max_entries": 1})),
            CancellationToken::new(|| false),
        )
        .await
        .unwrap();
    assert_eq!(listed["entries"].as_array().unwrap().len(), 1);
    assert_eq!(listed["truncated"], true);
    let error = tools
        .execute(
            invocation(
                "write_file",
                json!({"path": "draft.txt", "content": "replacement"}),
            ),
            CancellationToken::new(|| false),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "file_exists");
    tools
        .execute(
            invocation(
                "write_file",
                json!({"path": "draft.txt", "content": "replacement", "overwrite": true}),
            ),
            CancellationToken::new(|| false),
        )
        .await
        .unwrap();
    assert_eq!(
        fs::read_to_string(fixture.0.join("workspace/draft.txt")).unwrap(),
        "replacement"
    );
}

#[tokio::test]
async fn traversal_device_streams_and_ungranted_absolute_paths_are_rejected() {
    let fixture = Fixture::new();
    let tools = fixture.tools(true, true);
    for path in [
        "../outside/secret.txt".to_string(),
        "missing/../../outside/new.txt".to_string(),
        fixture
            .0
            .join("outside/secret.txt")
            .to_string_lossy()
            .to_string(),
        r"\\.\C:\secret.txt".to_string(),
        "nested/text.txt:secret".to_string(),
        r"C:secret.txt".to_string(),
    ] {
        let error = tools
            .execute(
                invocation(
                    "write_file",
                    json!({"path": path, "content": "unsafe", "overwrite": true}),
                ),
                CancellationToken::new(|| false),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "path_not_authorized", "{path}");
    }
    let error = tools.execute(
        invocation("powershell", json!({"command": "Write-Output 'unsafe'", "working_directory": fixture.0.join("outside")})),
        CancellationToken::new(|| false),
    ).await.unwrap_err();
    assert_eq!(error.code, "path_not_authorized");
    assert_eq!(
        fs::read_to_string(fixture.0.join("outside/secret.txt")).unwrap(),
        "outside"
    );
}

#[tokio::test]
async fn explicit_caller_grants_authorize_additional_roots() {
    let fixture = Fixture::new();
    let tools = LocalToolExecutor::new(
        fixture.0.join("workspace"),
        vec![fixture.0.join("outside")],
        true,
        false,
    )
    .unwrap();
    let result = tools
        .execute(
            invocation(
                "read_file",
                json!({"path": fixture.0.join("outside/secret.txt")}),
            ),
            CancellationToken::new(|| false),
        )
        .await
        .unwrap();
    assert_eq!(result["content"], "outside");
}

#[tokio::test]
async fn capability_switches_and_confirmation_metadata_are_enforced() {
    let fixture = Fixture::new();
    let disabled = fixture.tools(false, true);
    assert!(disabled.registrations().is_empty());
    for name in ["read_file", "list_directory", "write_file", "powershell"] {
        assert_eq!(
            disabled
                .execute(
                    invocation(name, json!({})),
                    CancellationToken::new(|| false)
                )
                .await
                .unwrap_err()
                .code,
            "agent_capability_disabled"
        );
    }
    let files = fixture.tools(true, false);
    assert_eq!(files.registrations().len(), 3);
    let all = fixture.tools(true, true);
    for registration in all.registrations() {
        let read = matches!(
            registration.spec.name.as_str(),
            "read_file" | "list_directory"
        );
        assert_eq!(registration.read_only, read);
        assert_eq!(
            registration.spec.risk,
            if read {
                PermissionRisk::Automatic
            } else {
                PermissionRisk::ConfirmationRequired
            }
        );
    }
    let cancelled = all
        .execute(
            invocation(
                "write_file",
                json!({"path": "cancelled.txt", "content": "no"}),
            ),
            CancellationToken::new(|| true),
        )
        .await
        .unwrap_err();
    assert!(cancelled.cancelled);
    assert!(!fixture.0.join("workspace/cancelled.txt").exists());
}

#[tokio::test]
async fn powershell_preserves_utf8_and_bounds_both_output_streams() {
    let fixture = Fixture::new();
    let result = fixture.tools(true, true).execute(
        invocation("powershell", json!({"command": "[Console]::Out.Write('你好'); [Console]::Out.Write(('x' * 100000)); [Console]::Error.Write(('e' * 100000))"})),
        CancellationToken::new(|| false),
    ).await.unwrap();
    assert_eq!(result["success"], true);
    assert!(result["stdout"].as_str().unwrap().starts_with("你好"));
    assert!(result["stdout"].as_str().unwrap().len() <= 65536);
    assert!(result["stderr"].as_str().unwrap().len() <= 65536);
    assert_eq!(result["stdout_truncated"], true);
    assert_eq!(result["stderr_truncated"], true);
}

fn spawn_child_script() -> &'static str {
    r#"$start=New-Object System.Diagnostics.ProcessStartInfo; $start.FileName=[IO.Path]::Combine([Environment]::SystemDirectory,'ping.exe'); $start.Arguments='-n 60 127.0.0.1'; $start.UseShellExecute=$false; $start.CreateNoWindow=$true; $child=[Diagnostics.Process]::Start($start); [IO.File]::WriteAllText((Join-Path (Get-Location) 'child.pid'), [string]$child.Id); Start-Sleep -Seconds 60"#
}

fn assert_child_stopped(fixture: &Fixture) {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    let pid: u32 = fs::read_to_string(fixture.0.join("workspace/child.pid"))
        .expect("PowerShell did not write the child process fixture PID")
        .parse()
        .unwrap();
    let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if !process.is_null() {
        let waited = unsafe { WaitForSingleObject(process, 2000) };
        unsafe {
            CloseHandle(process);
        }
        assert_eq!(waited, WAIT_OBJECT_0, "child process remains alive");
    }
}

#[tokio::test]
async fn powershell_timeout_terminates_its_process_tree() {
    let fixture = Fixture::new();
    let started = Instant::now();
    let result = fixture
        .tools(true, true)
        .execute(
            invocation(
                "powershell",
                json!({"command": spawn_child_script(), "timeout_ms": 10000}),
            ),
            CancellationToken::new(|| false),
        )
        .await
        .unwrap_err();
    assert_eq!(result.code, "tool_timeout");
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_child_stopped(&fixture);
}

#[tokio::test]
async fn powershell_cancellation_terminates_its_process_tree() {
    let fixture = Fixture::new();
    let cancelled = Arc::new(AtomicBool::new(false));
    let task = tokio::spawn({
        let tools = fixture.tools(true, true);
        let cancelled = cancelled.clone();
        async move {
            tools
                .execute(
                    invocation("powershell", json!({"command": spawn_child_script()})),
                    CancellationToken::new(move || cancelled.load(Ordering::SeqCst)),
                )
                .await
        }
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !fixture.0.join("workspace/child.pid").exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let child_started = fixture.0.join("workspace/child.pid").exists();
    cancelled.store(true, Ordering::SeqCst);
    let error = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.cancelled);
    assert!(
        child_started,
        "PowerShell did not start the child process fixture"
    );
    assert_child_stopped(&fixture);
}
