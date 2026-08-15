use super::protocol::{
    read_frame, write_frame, FrameError, WorkerNotification, WorkerRequest, WorkerResponse,
    PROTOCOL_VERSION,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::{Display, Formatter};
use std::io::BufReader;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const MAX_STDERR_BYTES: usize = 64 * 1024;
const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub executable: PathBuf,
    pub args: Vec<std::ffi::OsString>,
    pub working_directory: Option<PathBuf>,
    pub artifact_manifest: Option<PathBuf>,
    pub isolate_process_tree: bool,
}

impl WorkerConfig {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            args: Vec::new(),
            working_directory: None,
            artifact_manifest: None,
            isolate_process_tree: false,
        }
    }

    pub fn with_artifact_manifest(mut self, manifest: PathBuf) -> Self {
        self.artifact_manifest = Some(manifest);
        self
    }

    pub fn with_process_tree_isolation(mut self) -> Self {
        self.isolate_process_tree = true;
        self
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
    child: Arc<Mutex<Child>>,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    _process_group: Option<Arc<WorkerProcessGroup>>,
    _stderr_drain: Option<JoinHandle<usize>>,
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
        if let Some(manifest) = &config.artifact_manifest {
            verify_worker_manifest(&config.executable, manifest)?;
        }

        let mut command = Command::new(&config.executable);
        command
            .args(&config.args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
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
        let process_group = if config.isolate_process_tree {
            match WorkerProcessGroup::attach(&child) {
                Ok(group) => Some(Arc::new(group)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            }
        } else {
            None
        };
        let stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(WorkerSupervisorError::Io(
                    "worker stdin pipe was not created".to_string(),
                ));
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(WorkerSupervisorError::Io(
                    "worker stdout pipe was not created".to_string(),
                ));
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(WorkerSupervisorError::Io(
                    "worker stderr pipe was not created".to_string(),
                ));
            }
        };
        let stderr_drain = std::thread::spawn(move || drain_worker_stderr(stderr));
        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            stdin,
            stdout: BufReader::new(stdout),
            _process_group: process_group,
            _stderr_drain: Some(stderr_drain),
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

    pub fn request_with_progress_and_cancel<F, C>(
        &mut self,
        request: &WorkerRequest,
        on_progress: F,
        should_cancel: C,
    ) -> Result<Value, WorkerSupervisorError>
    where
        F: FnMut(&Value) -> Result<(), WorkerSupervisorError>,
        C: Fn() -> bool + Send + 'static,
    {
        let stopped = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let child = Arc::clone(&self.child);
        let process_group = self._process_group.as_ref().map(Arc::clone);
        let monitor_stopped = Arc::clone(&stopped);
        let monitor_cancelled = Arc::clone(&cancelled);
        let monitor = std::thread::spawn(move || {
            while !monitor_stopped.load(Ordering::SeqCst) {
                if should_cancel() {
                    monitor_cancelled.store(true, Ordering::SeqCst);
                    if let Some(process_group) = &process_group {
                        process_group.terminate();
                    }
                    if let Ok(mut child) = child.lock() {
                        let _ = child.kill();
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });

        let result = self.request_with_progress(request, on_progress);
        stopped.store(true, Ordering::SeqCst);
        let _ = monitor.join();
        if cancelled.load(Ordering::SeqCst) {
            Err(WorkerSupervisorError::Cancelled)
        } else {
            result
        }
    }

    pub fn shutdown(self, request: &WorkerRequest) -> Result<Value, WorkerSupervisorError> {
        self.shutdown_with_timeout(request, DEFAULT_SHUTDOWN_TIMEOUT)
    }

    pub fn shutdown_with_timeout(
        mut self,
        request: &WorkerRequest,
        timeout: Duration,
    ) -> Result<Value, WorkerSupervisorError> {
        let deadline = Instant::now() + timeout;
        let response = match self.request_with_progress_and_cancel(
            request,
            |_| Ok(()),
            move || Instant::now() >= deadline,
        ) {
            Ok(response) => response,
            Err(WorkerSupervisorError::Cancelled) => {
                self.terminate_process();
                self.finish_stderr(false);
                return Err(WorkerSupervisorError::Io(
                    "worker shutdown timed out; process was terminated".to_string(),
                ));
            }
            Err(error) => return Err(error),
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !self.wait_for_exit(remaining)? {
            self.terminate_process();
            let _ = self.wait_for_exit(Duration::from_secs(2));
            self.finish_stderr(false);
            return Err(WorkerSupervisorError::Io(
                "worker shutdown timed out; process was terminated".to_string(),
            ));
        }
        self.finish_stderr(true);
        Ok(response)
    }

    fn wait_for_exit(&self, timeout: Duration) -> Result<bool, WorkerSupervisorError> {
        let deadline = Instant::now() + timeout;
        loop {
            let exited = self
                .child
                .lock()
                .map_err(|_| {
                    WorkerSupervisorError::Io("worker process lock was poisoned".to_string())
                })?
                .try_wait()
                .map_err(|error| WorkerSupervisorError::Io(error.to_string()))?
                .is_some();
            if exited {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn terminate_process(&self) {
        if let Some(process_group) = &self._process_group {
            process_group.terminate();
        }
        if let Ok(mut child) = self.child.lock() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
        }
    }

    fn finish_stderr(&mut self, wait: bool) {
        let Some(stderr_drain) = self._stderr_drain.take() else {
            return;
        };
        if wait {
            let _ = stderr_drain.join();
        } else if stderr_drain.is_finished() {
            let _ = stderr_drain.join();
        }
    }
}

fn drain_worker_stderr<R: Read>(mut reader: R) -> usize {
    let mut retained: usize = 0;
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                retained = retained.saturating_add(count).min(MAX_STDERR_BYTES);
            }
            Err(_) => break,
        }
    }
    retained
}

#[cfg(windows)]
#[derive(Debug)]
struct WorkerProcessGroup {
    handle: usize,
}

#[cfg(not(windows))]
#[derive(Debug)]
struct WorkerProcessGroup;

impl WorkerProcessGroup {
    fn attach(child: &Child) -> Result<Self, WorkerSupervisorError> {
        #[cfg(windows)]
        {
            use std::mem::{size_of, zeroed};
            use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
            use windows_sys::Win32::System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            };
            use windows_sys::Win32::System::Threading::{
                OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
            };

            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                return Err(WorkerSupervisorError::Io(format!(
                    "create worker job object failed with Windows error {}",
                    unsafe { GetLastError() }
                )));
            }

            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const std::ffi::c_void,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if configured == 0 {
                let error = unsafe { GetLastError() };
                unsafe {
                    CloseHandle(job);
                }
                return Err(WorkerSupervisorError::Io(format!(
                    "configure worker job object failed with Windows error {error}"
                )));
            }

            let process =
                unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, child.id()) };
            if process.is_null() {
                let error = unsafe { GetLastError() };
                unsafe {
                    CloseHandle(job);
                }
                return Err(WorkerSupervisorError::Io(format!(
                    "open worker process failed with Windows error {error}"
                )));
            }
            let assigned = unsafe { AssignProcessToJobObject(job, process) };
            let process_error = if assigned == 0 {
                Some(unsafe { GetLastError() })
            } else {
                None
            };
            unsafe {
                CloseHandle(process);
            }
            if let Some(error) = process_error {
                unsafe {
                    CloseHandle(job);
                }
                return Err(WorkerSupervisorError::Io(format!(
                    "assign worker process to job object failed with Windows error {error}"
                )));
            }
            Ok(Self {
                handle: job as usize,
            })
        }

        #[cfg(not(windows))]
        {
            let _ = child;
            Ok(Self)
        }
    }

    fn terminate(&self) {
        #[cfg(windows)]
        {
            unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(
                    self.handle as windows_sys::Win32::Foundation::HANDLE,
                    1,
                );
            }
        }
    }
}

#[cfg(windows)]
impl Drop for WorkerProcessGroup {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(
                    self.handle as windows_sys::Win32::Foundation::HANDLE,
                );
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct WorkerArtifactManifest {
    schema_version: String,
    artifact: String,
    executable: String,
    sha256: String,
}

pub fn verify_worker_manifest(
    executable: &std::path::Path,
    manifest_path: &std::path::Path,
) -> Result<(), WorkerSupervisorError> {
    if !manifest_path.is_file() {
        return Err(WorkerSupervisorError::InvalidConfig(
            "worker artifact manifest does not exist".to_string(),
        ));
    }
    let bytes =
        std::fs::read(executable).map_err(|error| WorkerSupervisorError::Io(error.to_string()))?;
    let manifest_bytes = std::fs::read(manifest_path)
        .map_err(|error| WorkerSupervisorError::Io(error.to_string()))?;
    let manifest: WorkerArtifactManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|error| {
            WorkerSupervisorError::InvalidConfig(format!(
                "worker artifact manifest is invalid: {error}"
            ))
        })?;
    if manifest.schema_version != "1.0.0" || manifest.artifact != "bloomery-compute-worker" {
        return Err(WorkerSupervisorError::InvalidConfig(
            "worker artifact manifest identity is invalid".to_string(),
        ));
    }
    let executable_name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            WorkerSupervisorError::InvalidConfig(
                "worker executable name is not valid UTF-8".to_string(),
            )
        })?;
    if !manifest.executable.eq_ignore_ascii_case(executable_name) {
        return Err(WorkerSupervisorError::InvalidConfig(
            "worker artifact manifest executable does not match".to_string(),
        ));
    }
    if manifest.sha256.len() != 64 || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(WorkerSupervisorError::InvalidConfig(
            "worker artifact manifest checksum is invalid".to_string(),
        ));
    }
    let actual_hash = format!("{:x}", Sha256::digest(bytes));
    if !actual_hash.eq_ignore_ascii_case(&manifest.sha256) {
        return Err(WorkerSupervisorError::InvalidConfig(
            "worker artifact checksum does not match its manifest".to_string(),
        ));
    }
    Ok(())
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
        if response.get("id").is_none() && response.get("method").is_some() {
            let notification: WorkerNotification =
                serde_json::from_value(response).map_err(|error| {
                    WorkerSupervisorError::Protocol(FrameError::InvalidRequest(format!(
                        "invalid worker notification: {error}"
                    )))
                })?;
            if notification.jsonrpc != "2.0" {
                return Err(WorkerSupervisorError::Protocol(FrameError::InvalidRequest(
                    "worker notification jsonrpc must be 2.0".to_string(),
                )));
            }
            if notification.protocol_version != PROTOCOL_VERSION {
                return Err(WorkerSupervisorError::Protocol(FrameError::InvalidRequest(
                    format!("worker notification protocol_version must be {PROTOCOL_VERSION}"),
                )));
            }
            if notification.method != "progress" {
                return Err(WorkerSupervisorError::Protocol(FrameError::InvalidRequest(
                    "unsupported worker notification method".to_string(),
                )));
            }
            on_progress(&notification.params)?;
            continue;
        }

        let parsed: WorkerResponse = serde_json::from_value(response.clone()).map_err(|error| {
            WorkerSupervisorError::Protocol(FrameError::InvalidRequest(format!(
                "invalid worker response: {error}"
            )))
        })?;
        if parsed.jsonrpc != "2.0" {
            return Err(WorkerSupervisorError::Protocol(FrameError::InvalidRequest(
                "worker response jsonrpc must be 2.0".to_string(),
            )));
        }
        if parsed.protocol_version != PROTOCOL_VERSION {
            return Err(WorkerSupervisorError::Protocol(FrameError::InvalidRequest(
                format!("worker response protocol_version must be {PROTOCOL_VERSION}"),
            )));
        }
        if parsed.id != request_id {
            return Err(WorkerSupervisorError::Protocol(FrameError::InvalidRequest(
                "worker response ID does not match request".to_string(),
            )));
        }
        match (&parsed.result, &parsed.error) {
            (Some(_), Some(_)) | (None, None) => {
                return Err(WorkerSupervisorError::Protocol(FrameError::InvalidRequest(
                    "worker response must contain exactly one result or error".to_string(),
                )));
            }
            (Some(_), None) => return Ok(response),
            (None, Some(error)) => {
                return Err(WorkerSupervisorError::Remote {
                    code: error.code.clone(),
                    message: error.message.clone(),
                })
            }
        }
    }
}

impl Drop for WorkerClient {
    fn drop(&mut self) {
        self.terminate_process();
        if let Some(stderr_drain) = self._stderr_drain.take() {
            let _ = stderr_drain.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{drain_worker_stderr, verify_worker_manifest, WorkerConfig, MAX_STDERR_BYTES};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::Path;

    fn write_manifest(path: &Path, executable: &Path, hash: &str) {
        let manifest = serde_json::json!({
            "schema_version": "1.0.0",
            "artifact": "bloomery-compute-worker",
            "executable": executable.file_name().unwrap().to_string_lossy(),
            "sha256": hash,
        });
        fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    }

    #[test]
    fn worker_manifest_accepts_the_declared_executable_hash() {
        let root =
            std::env::temp_dir().join(format!("bloomery-worker-manifest-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let executable = root.join("bloomery-compute-worker.exe");
        let manifest = root.join("worker-artifact-manifest.json");
        let bytes = b"trusted worker fixture";
        fs::write(&executable, bytes).unwrap();
        let hash = format!("{:x}", Sha256::digest(bytes));
        write_manifest(&manifest, &executable, &hash);

        verify_worker_manifest(&executable, &manifest).expect("matching manifest must pass");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn worker_manifest_rejects_a_tampered_executable() {
        let root =
            std::env::temp_dir().join(format!("bloomery-worker-manifest-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let executable = root.join("bloomery-compute-worker.exe");
        let manifest = root.join("worker-artifact-manifest.json");
        let original = b"trusted worker fixture";
        fs::write(&executable, original).unwrap();
        let hash = format!("{:x}", Sha256::digest(original));
        write_manifest(&manifest, &executable, &hash);
        fs::write(&executable, b"tampered worker fixture").unwrap();

        let error = verify_worker_manifest(&executable, &manifest)
            .expect_err("tampered worker must be rejected");
        assert!(error.to_string().contains("checksum"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn process_tree_isolation_is_opt_in_for_worker_configs() {
        let config = WorkerConfig::new(std::path::PathBuf::from("worker.exe"));
        assert!(!config.isolate_process_tree);
        assert!(
            WorkerConfig::new(std::path::PathBuf::from("worker.exe"))
                .with_process_tree_isolation()
                .isolate_process_tree
        );
    }

    #[test]
    fn worker_stderr_drain_keeps_only_the_bounded_prefix() {
        use std::io::Cursor;

        let bytes = vec![b'x'; MAX_STDERR_BYTES + 17];
        assert_eq!(drain_worker_stderr(Cursor::new(bytes)), MAX_STDERR_BYTES);
    }

    #[cfg(windows)]
    #[test]
    fn windows_process_tree_guard_terminates_a_child_when_dropped() {
        use super::WorkerProcessGroup;
        use std::process::Command;
        use std::time::Duration;

        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .spawn()
            .expect("spawn worker child fixture");
        let guard = WorkerProcessGroup::attach(&child).expect("attach worker child to job");
        drop(guard);

        for _ in 0..50 {
            if child.try_wait().expect("poll worker child").is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("worker child survived process-tree guard drop");
    }
}
