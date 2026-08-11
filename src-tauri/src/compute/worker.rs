use super::protocol::{read_frame, write_frame, FrameError, WorkerRequest};
use serde_json::Value;
use std::fmt::{Display, Formatter};
use std::io::BufReader;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub executable: PathBuf,
    pub args: Vec<std::ffi::OsString>,
    pub working_directory: Option<PathBuf>,
}

impl WorkerConfig {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            args: Vec::new(),
            working_directory: None,
        }
    }
}

#[derive(Debug)]
pub enum WorkerSupervisorError {
    InvalidConfig(String),
    Io(String),
    Protocol(FrameError),
    Callback(String),
    Cancelled,
    WorkerExited,
    Remote { code: String, message: String },
}

impl Display for WorkerSupervisorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => formatter.write_str(message),
            Self::Io(message) => write!(formatter, "worker I/O failed: {message}"),
            Self::Protocol(error) => write!(formatter, "worker protocol failed: {error}"),
            Self::Callback(message) => write!(formatter, "worker callback failed: {message}"),
            Self::Cancelled => formatter.write_str("worker request was cancelled"),
            Self::WorkerExited => formatter.write_str("worker exited before returning a response"),
            Self::Remote { code, message } => write!(formatter, "worker error {code}: {message}"),
        }
    }
}

impl std::error::Error for WorkerSupervisorError {}

impl From<FrameError> for WorkerSupervisorError {
    fn from(error: FrameError) -> Self {
        Self::Protocol(error)
    }
}

#[derive(Debug)]
pub struct WorkerClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl WorkerClient {
    pub fn spawn(config: WorkerConfig) -> Result<Self, WorkerSupervisorError> {
        if !config.executable.is_file() {
            return Err(WorkerSupervisorError::InvalidConfig(
                "worker executable does not exist".to_string(),
            ));
        }
        if let Some(directory) = &config.working_directory {
            if !directory.is_dir() {
                return Err(WorkerSupervisorError::InvalidConfig(
                    "worker working directory does not exist".to_string(),
                ));
            }
        }

        let mut command = Command::new(&config.executable);
        command
            .args(&config.args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for key in ["SystemRoot", "PATH", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        if let Some(directory) = &config.working_directory {
            command.current_dir(directory);
        }
        let mut child = command
            .spawn()
            .map_err(|error| WorkerSupervisorError::Io(error.to_string()))?;
        let stdin = child.stdin.take().ok_or_else(|| {
            WorkerSupervisorError::Io("worker stdin pipe was not created".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            WorkerSupervisorError::Io("worker stdout pipe was not created".to_string())
        })?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    pub fn request(&mut self, request: &WorkerRequest) -> Result<Value, WorkerSupervisorError> {
        self.request_with_progress(request, |_| Ok(()))
    }

    pub fn request_with_progress<F>(
        &mut self,
        request: &WorkerRequest,
        on_progress: F,
    ) -> Result<Value, WorkerSupervisorError>
    where
        F: FnMut(&Value) -> Result<(), WorkerSupervisorError>,
    {
        let value = serde_json::to_value(request).map_err(|error| {
            WorkerSupervisorError::Protocol(FrameError::InvalidJson(error.to_string()))
        })?;
        write_frame(&mut self.stdin, &value)?;
        read_response_with_progress(&mut self.stdout, &request.id, on_progress)
    }

    pub fn shutdown(mut self, request: &WorkerRequest) -> Result<Value, WorkerSupervisorError> {
        let response = self.request(request)?;
        self.child
            .wait()
            .map_err(|error| WorkerSupervisorError::Io(error.to_string()))?;
        Ok(response)
    }
}

pub fn read_response<R: Read>(
    reader: &mut R,
    request_id: &str,
) -> Result<Value, WorkerSupervisorError> {
    read_response_with_progress(reader, request_id, |_| Ok(()))
}

pub fn read_response_with_progress<
    R: Read,
    F: FnMut(&Value) -> Result<(), WorkerSupervisorError>,
>(
    reader: &mut R,
    request_id: &str,
    mut on_progress: F,
) -> Result<Value, WorkerSupervisorError> {
    loop {
        let response = read_frame(reader)?.ok_or(WorkerSupervisorError::WorkerExited)?;
        let object = response.as_object().ok_or_else(|| {
            WorkerSupervisorError::Protocol(FrameError::InvalidJson(
                "worker response must be an object".to_string(),
            ))
        })?;
        if object.get("id").is_none() && object.get("method").is_some() {
            if object.get("method").and_then(Value::as_str) == Some("progress") {
                let params = object.get("params").ok_or_else(|| {
                    WorkerSupervisorError::Protocol(FrameError::InvalidRequest(
                        "progress notification params are required".to_string(),
                    ))
                })?;
                on_progress(params)?;
            }
            continue;
        }
        if object.get("id") != Some(&Value::String(request_id.to_string())) {
            return Err(WorkerSupervisorError::Protocol(FrameError::InvalidRequest(
                "worker response ID does not match request".to_string(),
            )));
        }
        if let Some(error) = object.get("error").and_then(Value::as_object) {
            let code = error
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("worker_error")
                .to_string();
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("worker returned an error")
                .to_string();
            return Err(WorkerSupervisorError::Remote { code, message });
        }
        return Ok(response);
    }
}

impl Drop for WorkerClient {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
    }
}
