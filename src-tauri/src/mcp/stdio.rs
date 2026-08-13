use std::{collections::BTreeMap, path::PathBuf, process::Stdio, sync::Arc};

use rmcp::transport::TokioChildProcess;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::{Mutex, Notify},
};

use super::McpError;

pub type McpStdioEnv = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub struct McpStdioConfig {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub inherited_env: Vec<String>,
    pub env: McpStdioEnv,
    pub max_stderr_bytes: usize,
}

pub(crate) fn validate_inherited_env_name(name: &str) -> Result<(), McpError> {
    if name.is_empty()
        || name.contains('=')
        || name.contains('\0')
        || !matches!(
            name,
            "SystemRoot" | "windir" | "ComSpec" | "COMSPEC" | "PATHEXT" | "TEMP" | "TMP"
        )
    {
        return Err(McpError::InvalidTransport(format!(
            "stdio inherited environment variable is not allowed: {name}"
        )));
    }
    Ok(())
}

impl McpStdioConfig {
    pub(crate) fn validate(&self) -> Result<(), McpError> {
        if self.executable.as_os_str().is_empty() {
            return Err(McpError::InvalidTransport(
                "stdio executable is required".to_string(),
            ));
        }
        if self.max_stderr_bytes == 0 {
            return Err(McpError::InvalidTransport(
                "stdio stderr limit must be positive".to_string(),
            ));
        }
        for (name, value) in &self.env {
            if name.is_empty() || name.contains('=') || name.contains('\0') || value.contains('\0')
            {
                return Err(McpError::InvalidTransport(
                    "stdio environment names and values must be valid".to_string(),
                ));
            }
        }
        for name in &self.inherited_env {
            validate_inherited_env_name(name)?;
        }
        Ok(())
    }
}

pub struct StdioTransport {
    transport: TokioChildProcess,
    pub stderr: McpStderrCapture,
}

#[derive(Clone)]
pub struct McpStderrCapture {
    state: Arc<Mutex<StderrState>>,
    complete: Arc<Notify>,
}

#[derive(Debug, Default)]
struct StderrState {
    bytes: Vec<u8>,
    truncated: bool,
    finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpStderrSnapshot {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

impl McpStderrCapture {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(StderrState::default())),
            complete: Arc::new(Notify::new()),
        }
    }

    pub async fn wait(&self) {
        loop {
            let notified = self.complete.notified();
            if self.state.lock().await.finished {
                return;
            }
            notified.await;
        }
    }

    pub async fn snapshot(&self) -> McpStderrSnapshot {
        let state = self.state.lock().await;
        McpStderrSnapshot {
            bytes: state.bytes.clone(),
            truncated: state.truncated,
        }
    }
}

impl std::fmt::Debug for McpStderrCapture {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("StderrCapture(..)")
    }
}

pub fn spawn(config: McpStdioConfig) -> Result<StdioTransport, McpError> {
    config.validate()?;
    let mut command = Command::new(&config.executable);
    command.args(&config.args).env_clear();
    for name in &config.inherited_env {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.envs(&config.env);
    if let Some(directory) = &config.working_directory {
        command.current_dir(directory);
    }

    let (transport, stderr) = TokioChildProcess::builder(command)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            McpError::Transport(format!("failed to start MCP stdio server: {error}"))
        })?;
    let capture = McpStderrCapture::new();
    if let Some(stderr) = stderr {
        tokio::spawn(read_stderr(
            stderr,
            capture.clone(),
            config.max_stderr_bytes,
        ));
    }
    Ok(StdioTransport {
        transport,
        stderr: capture,
    })
}

impl StdioTransport {
    pub fn spawn(config: McpStdioConfig) -> Result<Self, McpError> {
        spawn(config)
    }

    pub fn id(&self) -> Option<u32> {
        self.transport.id()
    }

    pub async fn graceful_shutdown(&mut self) -> Result<(), McpError> {
        self.transport.graceful_shutdown().await.map_err(|error| {
            McpError::Transport(format!("failed to stop MCP stdio server: {error}"))
        })
    }

    pub(crate) fn into_transport(self) -> TokioChildProcess {
        self.transport
    }
}

async fn read_stderr<R>(mut reader: R, capture: McpStderrCapture, max_bytes: usize)
where
    R: AsyncRead + Unpin,
{
    let mut buffer = [0u8; 4096];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let mut state = capture.state.lock().await;
                let remaining = max_bytes.saturating_sub(state.bytes.len());
                if remaining > 0 {
                    state
                        .bytes
                        .extend_from_slice(&buffer[..read.min(remaining)]);
                }
                if read > remaining {
                    state.truncated = true;
                }
            }
        }
    }
    capture.state.lock().await.finished = true;
    capture.complete.notify_waiters();
}

#[cfg(test)]
mod tests {
    use super::validate_inherited_env_name;

    #[test]
    fn inherited_environment_allows_only_runtime_variables() {
        for name in [
            "SystemRoot",
            "windir",
            "ComSpec",
            "COMSPEC",
            "PATHEXT",
            "TEMP",
            "TMP",
        ] {
            assert!(validate_inherited_env_name(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn inherited_environment_rejects_credentials_and_unknown_names() {
        for name in [
            "OPENAI_API_KEY",
            "GH_TOKEN",
            "PATH",
            "CUSTOM_RUNTIME_SETTING",
        ] {
            assert!(
                validate_inherited_env_name(name).is_err(),
                "{name} must not be inherited"
            );
        }
    }
}
