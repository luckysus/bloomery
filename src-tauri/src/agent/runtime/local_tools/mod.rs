use super::{
    CancellationToken, ToolExecutionError, ToolExecutor, ToolFuture, ToolHandler, ToolInvocation,
    ToolRegistration,
};
use crate::agent::protocol::PermissionRisk;
use crate::agent::tool_repair::ToolSpec;
use crate::permissions::path::{AuthorizedPath, AuthorizedRoots};
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

mod file_ops;
mod powershell;

use file_ops::{list_directory, read_file, write_file};
use powershell::run_powershell;

pub(super) const MAX_PATH_BYTES: usize = 4096;
pub(super) const MAX_READ_BYTES: usize = 256 * 1024;
pub(super) const MAX_WRITE_BYTES: usize = 1024 * 1024;
pub(super) const MAX_DIRECTORY_ENTRIES: usize = 200;
pub(super) const MAX_COMMAND_BYTES: usize = 16 * 1024;
pub(super) const MAX_SHELL_OUTPUT_BYTES: usize = 64 * 1024;
pub(super) const DEFAULT_READ_BYTES: usize = 64 * 1024;
pub(super) const DEFAULT_SHELL_TIMEOUT_MS: u64 = 30_000;
pub(super) const MAX_SHELL_TIMEOUT_MS: u64 = 120_000;

/// Caller-supplied roots are trusted grants; tool arguments can never add roots.
pub struct LocalToolExecutor {
    registrations: Vec<ToolRegistration>,
}

impl LocalToolExecutor {
    pub fn new(
        workspace_root: PathBuf,
        mut authorized_roots: Vec<PathBuf>,
        allow_file_access: bool,
        allow_shell: bool,
    ) -> Result<Self, String> {
        #[cfg(windows)]
        let workspace_root = PathBuf::from(workspace_root.to_string_lossy().replace('/', "\\"));
        #[cfg(windows)]
        for root in &mut authorized_roots {
            *root = PathBuf::from(root.to_string_lossy().replace('/', "\\"));
        }
        authorized_roots.push(workspace_root.clone());
        let roots = AuthorizedRoots::new(authorized_roots).map_err(|error| error.to_string())?;
        let workspace = roots
            .authorize(&workspace_root)
            .map_err(|error| error.to_string())?;
        let scope = Arc::new(LocalScope {
            workspace_root: plain_path(workspace.canonical_path()),
            roots,
            allow_file_access,
            allow_shell,
        });
        let mut registrations = Vec::new();
        if allow_file_access {
            for operation in [Operation::Read, Operation::List, Operation::Write] {
                registrations.push(registration(operation, scope.clone()));
            }
        }
        #[cfg(windows)]
        if allow_file_access && allow_shell {
            registrations.push(registration(Operation::PowerShell, scope));
        }
        Ok(Self { registrations })
    }
}

impl ToolExecutor for LocalToolExecutor {
    fn registrations(&self) -> &[ToolRegistration] {
        &self.registrations
    }

    fn execute(&self, invocation: ToolInvocation, cancellation: CancellationToken) -> ToolFuture {
        match self.registrations.iter().find(|registration| {
            registration.spec.id == invocation.tool_id
                && registration.spec.name == invocation.tool_name
        }) {
            Some(registration) => registration
                .handler
                .execute(invocation.arguments, cancellation),
            None => Box::pin(async {
                Err(ToolExecutionError::new(
                    "agent_capability_disabled",
                    "local tool is unavailable or disabled by Agent capability settings",
                ))
            }),
        }
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Read,
    List,
    Write,
    PowerShell,
}

impl Operation {
    fn name(self) -> &'static str {
        match self {
            Self::Read => "read_file",
            Self::List => "list_directory",
            Self::Write => "write_file",
            Self::PowerShell => "powershell",
        }
    }
}

fn registration(operation: Operation, scope: Arc<LocalScope>) -> ToolRegistration {
    let name = operation.name();
    let path = json!({"type": "string", "minLength": 1, "maxLength": MAX_PATH_BYTES});
    let schema = match operation {
        Operation::Read => json!({
            "type": "object",
            "properties": {
                "path": path,
                "offset_bytes": {"type": "integer", "minimum": 0},
                "max_bytes": {"type": "integer", "minimum": 1, "maximum": MAX_READ_BYTES}
            },
            "required": ["path"], "additionalProperties": false
        }),
        Operation::List => json!({
            "type": "object",
            "properties": {
                "path": path,
                "max_entries": {"type": "integer", "minimum": 1, "maximum": MAX_DIRECTORY_ENTRIES}
            },
            "required": ["path"], "additionalProperties": false
        }),
        Operation::Write => json!({
            "type": "object",
            "properties": {
                "path": path,
                "content": {"type": "string", "maxLength": MAX_WRITE_BYTES},
                "overwrite": {"type": "boolean"}
            },
            "required": ["path", "content"], "additionalProperties": false
        }),
        Operation::PowerShell => json!({
            "type": "object",
            "properties": {
                "command": {"type": "string", "minLength": 1, "maxLength": MAX_COMMAND_BYTES},
                "working_directory": path,
                "timeout_ms": {"type": "integer", "minimum": 1, "maximum": MAX_SHELL_TIMEOUT_MS}
            },
            "required": ["command"], "additionalProperties": false
        }),
    };
    let read_only = matches!(operation, Operation::Read | Operation::List);
    let mut registration = ToolRegistration::new(
        ToolSpec {
            id: format!("builtin.{name}"),
            name: name.to_string(),
            input_schema: schema,
            risk: if read_only {
                PermissionRisk::Automatic
            } else {
                PermissionRisk::ConfirmationRequired
            },
        },
        read_only,
        Arc::new(LocalHandler { operation, scope }),
    );
    if matches!(operation, Operation::PowerShell) {
        registration.timeout = Duration::from_millis(MAX_SHELL_TIMEOUT_MS);
    }
    registration
}

pub(super) struct LocalScope {
    workspace_root: PathBuf,
    pub(super) roots: AuthorizedRoots,
    allow_file_access: bool,
    allow_shell: bool,
}

impl LocalScope {
    pub(super) fn resolve(&self, raw: &str) -> Result<AuthorizedPath, ToolExecutionError> {
        if raw.is_empty() || raw.len() > MAX_PATH_BYTES || raw.contains('\0') {
            return Err(invalid("path is empty or exceeds the supported length"));
        }
        #[cfg(windows)]
        let raw = raw.replace('/', "\\");
        let path = Path::new(&raw);
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(path_error("parent traversal is not allowed"));
        }
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            if path
                .components()
                .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
            {
                return Err(path_error(
                    "path must be workspace-relative or fully absolute",
                ));
            }
            self.workspace_root.join(path)
        };
        self.roots.authorize(&absolute).map_err(path_error)
    }

    pub(super) fn verify_file(
        &self,
        path: &AuthorizedPath,
        file: &File,
    ) -> Result<(), ToolExecutionError> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
            if file.metadata().map_err(io_error)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err(path_error(
                    "reparse point handles cannot be used for local file effects",
                ));
            }
            self.roots
                .verify_opened_file(path, file)
                .map_err(path_error)?;
        }
        #[cfg(not(windows))]
        self.roots
            .verify_target(path, path.canonical_path())
            .map_err(path_error)?;
        Ok(())
    }

    pub(super) fn open_directory(&self, path: &AuthorizedPath) -> Result<File, ToolExecutionError> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
                FILE_SHARE_WRITE,
            };
            options
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }
        let directory = options.open(path.canonical_path()).map_err(io_error)?;
        self.verify_file(path, &directory)?;
        if !directory.metadata().map_err(io_error)?.is_dir() {
            return Err(invalid("path must refer to an existing directory"));
        }
        Ok(directory)
    }
}

struct LocalHandler {
    operation: Operation,
    scope: Arc<LocalScope>,
}

impl ToolHandler for LocalHandler {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> ToolFuture {
        let scope = self.scope.clone();
        let operation = self.operation;
        Box::pin(async move {
            check_cancelled(&cancellation)?;
            if !scope.allow_file_access
                || (matches!(operation, Operation::PowerShell) && !scope.allow_shell)
            {
                return Err(ToolExecutionError::new(
                    "agent_capability_disabled",
                    "local capability is disabled",
                ));
            }
            tokio::task::spawn_blocking(move || match operation {
                Operation::Read => read_file(&scope, arguments, &cancellation),
                Operation::List => list_directory(&scope, arguments, &cancellation),
                Operation::Write => write_file(&scope, arguments, &cancellation),
                Operation::PowerShell => run_powershell(&scope, arguments, &cancellation),
            })
            .await
            .map_err(|error| ToolExecutionError::new("local_tool_failed", error.to_string()))?
        })
    }
}

pub(super) fn plain_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(path) = path.to_string_lossy().strip_prefix(r"\\?\") {
        return PathBuf::from(path);
    }
    path.to_path_buf()
}

pub(super) fn display_path(path: &Path) -> String {
    plain_path(path).to_string_lossy().replace('\\', "/")
}

pub(super) fn check_cancelled(cancellation: &CancellationToken) -> Result<(), ToolExecutionError> {
    if cancellation.is_cancelled() {
        Err(ToolExecutionError::cancelled())
    } else {
        Ok(())
    }
}

pub(super) fn invalid(error: impl std::fmt::Display) -> ToolExecutionError {
    ToolExecutionError::new("invalid_arguments", error.to_string())
}

pub(super) fn path_error(error: impl std::fmt::Display) -> ToolExecutionError {
    ToolExecutionError::new("path_not_authorized", error.to_string())
}

pub(super) fn io_error(error: impl std::fmt::Display) -> ToolExecutionError {
    ToolExecutionError::new("local_io_failed", error.to_string())
}
