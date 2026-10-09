use super::{
    check_cancelled, display_path, invalid, io_error, LocalScope, DEFAULT_SHELL_TIMEOUT_MS,
    MAX_COMMAND_BYTES, MAX_SHELL_OUTPUT_BYTES, MAX_SHELL_TIMEOUT_MS,
};
use super::{CancellationToken, ToolExecutionError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PowerShellRequest {
    command: String,
    working_directory: Option<String>,
    timeout_ms: Option<u64>,
}

#[cfg(windows)]
pub(super) fn run_powershell(
    scope: &LocalScope,
    arguments: Value,
    cancellation: &CancellationToken,
) -> Result<Value, ToolExecutionError> {
    use crate::compute::worker::{WorkerProcessGroup, DEFAULT_WORKER_MEMORY_LIMIT_BYTES};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

    let request: PowerShellRequest = serde_json::from_value(arguments).map_err(invalid)?;
    if request.command.trim().is_empty()
        || request.command.len() > MAX_COMMAND_BYTES
        || request.command.contains('\0')
    {
        return Err(invalid("command is empty or exceeds the supported length"));
    }
    let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_SHELL_TIMEOUT_MS);
    if !(1..=MAX_SHELL_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(invalid("timeout_ms is outside the supported range"));
    }
    let working_directory = scope.resolve(request.working_directory.as_deref().unwrap_or("."))?;
    let _directory = scope.open_directory(&working_directory)?;
    let executable = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .ok_or_else(|| {
            ToolExecutionError::new("powershell_unavailable", "SystemRoot is unavailable")
        })?
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let bootstrap = "$ErrorActionPreference='Stop'; $utf8=New-Object System.Text.UTF8Encoding $false; [Console]::InputEncoding=$utf8; [Console]::OutputEncoding=$utf8; $OutputEncoding=$utf8; $script=[Console]::In.ReadToEnd(); $global:LASTEXITCODE=0; & ([scriptblock]::Create($script)); if (-not $?) {exit 1}; if ($LASTEXITCODE -ne 0) {exit $LASTEXITCODE}";
    check_cancelled(cancellation)?;
    let started = Instant::now();
    let mut command = Command::new(executable);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            bootstrap,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .current_dir(super::plain_path(working_directory.canonical_path()))
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in [
        "SystemRoot",
        "WINDIR",
        "PATH",
        "PATHEXT",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut child = command.spawn().map_err(io_error)?;
    let group = match WorkerProcessGroup::attach(&child, DEFAULT_WORKER_MEMORY_LIMIT_BYTES) {
        Ok(group) => group,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ToolExecutionError::new(
                "process_isolation_failed",
                error.to_string(),
            ));
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io_error("stdout pipe was not created"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io_error("stderr pipe was not created"))?;
    let stdout = std::thread::spawn(move || drain_output(stdout));
    let stderr = std::thread::spawn(move || drain_output(stderr));
    let script = request.command.into_bytes();
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| io_error("stdin pipe was not created"))?;
    let input = std::thread::spawn(move || std::io::Write::write_all(&mut stdin, &script));
    let status = loop {
        if cancellation.is_cancelled() {
            break Err(ToolExecutionError::cancelled());
        }
        if started.elapsed() >= Duration::from_millis(timeout_ms) {
            break Err(ToolExecutionError::new(
                "tool_timeout",
                "PowerShell execution timed out; process tree terminated",
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => break Err(io_error(error)),
        }
    };
    group.terminate();
    let _ = child.kill();
    let _ = child.wait();
    let input = input.join();
    let stdout = stdout.join();
    let stderr = stderr.join();
    let status = status?;
    input
        .map_err(|_| io_error("PowerShell input writer failed"))?
        .map_err(io_error)?;
    let stdout = stdout.map_err(|_| io_error("PowerShell stdout reader failed"))??;
    let stderr = stderr.map_err(|_| io_error("PowerShell stderr reader failed"))??;
    Ok(json!({
        "working_directory": display_path(working_directory.canonical_path()),
        "exit_code": status.code(), "success": status.success(),
        "stdout": String::from_utf8_lossy(&stdout.0), "stderr": String::from_utf8_lossy(&stderr.0),
        "stdout_truncated": stdout.1, "stderr_truncated": stderr.1,
        "duration_ms": started.elapsed().as_millis()
    }))
}

#[cfg(not(windows))]
pub(super) fn run_powershell(
    _scope: &LocalScope,
    _arguments: Value,
    _cancellation: &CancellationToken,
) -> Result<Value, ToolExecutionError> {
    Err(ToolExecutionError::new(
        "unsupported_platform",
        "PowerShell tool requires Windows",
    ))
}

#[cfg(windows)]
fn drain_output(mut reader: impl Read) -> Result<(Vec<u8>, bool), ToolExecutionError> {
    let mut retained = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; 4096];
    loop {
        let count = reader.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        let keep = count.min(MAX_SHELL_OUTPUT_BYTES.saturating_sub(retained.len()));
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok((retained, truncated))
}
