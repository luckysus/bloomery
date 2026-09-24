use chrono::Utc;
use rusqlite::Connection;
use std::time::Duration;
use std::{collections::HashSet, fs, path::PathBuf, sync::Arc, sync::Mutex};
use tauri::Manager;

use crate::tasks::scheduler::SchedulerState;
use crate::tasks::scheduler::TaskHandler;

pub struct DbState {
    conn: Mutex<Option<Connection>>,
}

impl Default for DbState {
    fn default() -> Self {
        Self {
            conn: Mutex::new(None),
        }
    }
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn current_workspace_id() -> &'static str {
    crate::app::identity::LocalIdentity.workspace_id()
}

pub(crate) fn with_conn<T>(
    db: &tauri::State<'_, DbState>,
    operation: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let guard = db.conn.lock().map_err(|_| "db state poisoned")?;
    let conn = guard.as_ref().ok_or("database not initialized")?;
    operation(conn)
}

pub(crate) fn with_conn_mut<T>(
    db: &tauri::State<'_, DbState>,
    operation: impl FnOnce(&mut Connection) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = db.conn.lock().map_err(|_| "db state poisoned")?;
    let conn = guard.as_mut().ok_or("database not initialized")?;
    operation(conn)
}

pub(crate) fn configured_data_directory(
    default: PathBuf,
    override_path: Option<PathBuf>,
) -> Result<PathBuf, String> {
    let directory = override_path.unwrap_or(default);
    if directory.as_os_str().is_empty() {
        return Err("BLOOMERY_DATA_DIR must not be empty".to_string());
    }
    if !directory.is_absolute() {
        return Err("BLOOMERY_DATA_DIR must be an absolute path".to_string());
    }
    Ok(directory)
}

pub(crate) fn app_data_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let default = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data dir failed: {error}"))?;
    let override_path = std::env::var_os("BLOOMERY_DATA_DIR").map(PathBuf::from);
    let directory = configured_data_directory(default, override_path)?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("create app data dir failed: {error}"))?;
    Ok(directory)
}

pub(crate) fn database_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_directory(app)?.join("bloomery.sqlite3"))
}

fn content_root_for(database: &PathBuf) -> Result<PathBuf, String> {
    database
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "resolve RAG content root failed".to_string())
}
#[tauri::command]
pub fn db_init(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    agent_state: tauri::State<crate::agent::runtime::RuntimeHost>,
    scheduler_state: tauri::State<SchedulerState>,
) -> Result<(), String> {
    let path = database_path(&app)?;
    let (mut connection, _migration_report) =
        crate::storage::database::open(&path).map_err(|error| error.to_string())?;
    crate::storage::repositories::child_turns::interrupt_orphans(
        &connection,
        current_workspace_id(),
        Utc::now(),
    )
    .map_err(|error| format!("interrupt orphan child turns failed: {error}"))?;
    let mut recovery =
        crate::agent::runtime::AgentRecoveryService::new(&mut connection, current_workspace_id())?;
    let recovered_runs = recovery
        .recover_active(&HashSet::new(), Utc::now())
        .map_err(|error| format!("recover agent runs failed: {error}"))?;
    *db.conn.lock().map_err(|_| "db state poisoned")? = Some(connection);
    let bundled_result = crate::app::bundled_domain::ensure_bundled_steel_package(&app, &db);
    let status_result = with_conn_mut(&db, |connection| {
        let existing = crate::storage::repositories::settings::get(
            connection,
            current_workspace_id(),
            "onboarding.completed",
        )?;
        let status = bundled_result.as_ref().map(|_| ()).map_err(String::as_str);
        let value =
            crate::app::bundled_domain::merge_bundled_steel_status(existing.as_deref(), status)?;
        crate::storage::repositories::settings::set(
            connection,
            current_workspace_id(),
            "onboarding.completed",
            &value,
        )
    });
    if let Err(error) = status_result {
        eprintln!("persist bundled steel domain status failed: {error}");
    }
    if let Err(error) = bundled_result {
        eprintln!("ensure bundled steel domain package failed: {error}");
    }
    // Start scheduler with Tauri event sink for real progress updates
    use crate::app::event_sink::TauriEventSink;
    use crate::tasks::scheduler::{Scheduler, SchedulerConfig, SystemClock};
    let sink = Arc::new(TauriEventSink::new(app.clone()));
    let content_root = content_root_for(&path)?;
    let scheduler = Scheduler::new(
        path.clone(),
        current_workspace_id().to_string(),
        SchedulerConfig::default(),
        Arc::new(SystemClock),
        rag_task_handlers_with_compute(path.clone(), content_root, compute_worker_config(&app)),
        sink,
    )
    .map_err(|error| error.to_string())?;
    scheduler_state
        .start(scheduler)
        .map_err(|error| error.to_string())?;
    let runtime_host = agent_state.inner().clone();
    let app_for_recovery = app.clone();
    for recovered in recovered_runs {
        if matches!(
            &recovered.action,
            crate::agent::runtime::RecoveryAction::ResumeFromCheckpoint(_)
        ) {
            let app_for_run = app_for_recovery.clone();
            let runtime_host_for_run = runtime_host.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = crate::app::desktop_agent_runtime::resume_recovered_agent(
                    &app_for_run,
                    &runtime_host_for_run,
                    current_workspace_id(),
                    recovered,
                )
                .await
                {
                    eprintln!("resume agent run after startup failed: {error}");
                }
            });
        }
    }
    Ok(())
}

#[tauri::command]
pub fn create_backup_archive(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    archive_path: String,
) -> Result<crate::storage::backup::BackupSummary, String> {
    let archive_path = PathBuf::from(archive_path.trim());
    if archive_path.as_os_str().is_empty() {
        return Err("backup archive path is required".to_string());
    }
    let database = database_path(&app)?;
    let content_root = content_root_for(&database)?;
    with_conn(&db, |connection| {
        crate::storage::backup::create_backup(connection, &database, &content_root, &archive_path)
    })
}

#[tauri::command]
pub fn preview_backup_archive(
    archive_path: String,
) -> Result<crate::storage::backup::BackupSummary, String> {
    let archive_path = PathBuf::from(archive_path.trim());
    if archive_path.as_os_str().is_empty() {
        return Err("backup archive path is required".to_string());
    }
    crate::storage::backup::preview_backup(&archive_path)
}

#[tauri::command]
pub fn restore_backup_archive(
    app: tauri::AppHandle,
    db: tauri::State<DbState>,
    agent_state: tauri::State<crate::agent::runtime::RuntimeHost>,
    scheduler_state: tauri::State<SchedulerState>,
    archive_path: String,
) -> Result<crate::storage::backup::BackupSummary, String> {
    let archive_path = PathBuf::from(archive_path.trim());
    if archive_path.as_os_str().is_empty() {
        return Err("backup archive path is required".to_string());
    }
    if !scheduler_state.shutdown(Duration::from_secs(10)) {
        return Err("background scheduler did not stop before restore".to_string());
    }
    let connection = {
        let mut guard = db.conn.lock().map_err(|_| "db state poisoned")?;
        guard.take().ok_or("database not initialized")?
    };
    drop(connection);

    let database = database_path(&app)?;
    let content_root = content_root_for(&database)?;
    let domains_root = app_data_directory(&app)?.join("domains");
    let result = crate::storage::backup::restore_backup_with_domain_validation(
        &archive_path,
        &database,
        &content_root,
        &domains_root,
        env!("CARGO_PKG_VERSION"),
        &crate::domains::official_trust_store(),
    );
    let reinitialized = db_init(app, db, agent_state, scheduler_state);
    match (result, reinitialized) {
        (Ok(summary), Ok(())) => Ok(summary),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(format!(
            "backup restored but database restart failed: {error}"
        )),
        (Err(error), Err(restart_error)) => {
            Err(format!("{error}; database restart failed: {restart_error}"))
        }
    }
}

fn rag_task_handlers_with_compute(
    database: PathBuf,
    content_root: PathBuf,
    compute_worker: Option<crate::compute::worker::WorkerConfig>,
) -> Vec<Arc<dyn TaskHandler>> {
    use crate::rag::index::rebuild::IndexRebuildHandler;
    use crate::rag::tasks::{LocalRagPostprocessor, MinerUTaskHandler, RuntimeProviderFactory};
    use crate::storage::secrets::KeyringSecretStore;

    let providers = Arc::new(RuntimeProviderFactory::new(
        database.clone(),
        Arc::new(KeyringSecretStore),
    ));
    let embedding_factory: Arc<dyn crate::rag::index::EmbeddingRemoteFactory> = providers.clone();
    let remote_factory: Arc<dyn crate::rag::tasks::MinerURemoteFactory> = providers;
    let postprocessor = Arc::new(LocalRagPostprocessor::new(
        database.clone(),
        content_root.clone(),
        embedding_factory,
    ));
    vec![
        Arc::new(crate::compute::handler::ComputeTaskHandler::from_optional(
            compute_worker.clone(),
        )),
        Arc::new(
            crate::compute::handler::ComputePredictionTaskHandler::from_optional(
                compute_worker.clone(),
            ),
        ),
        Arc::new(
            crate::compute::handler::ComputeOnnxPredictionTaskHandler::from_optional(
                compute_worker.clone(),
            ),
        ),
        Arc::new(
            crate::compute::handler::ComputeOptimizationTaskHandler::from_optional(
                compute_worker.clone(),
            ),
        ),
        Arc::new(
            crate::compute::handler::ComputeExportOnnxTaskHandler::from_optional(
                compute_worker.clone(),
            ),
        ),
        Arc::new(
            crate::compute::handler::ComputeTrainedPredictionTaskHandler::from_optional(
                compute_worker.clone(),
            ),
        ),
        Arc::new(
            crate::compute::handler::ComputeSklearnTrainingTaskHandler::from_optional(
                compute_worker,
            ),
        ),
        Arc::new(MinerUTaskHandler::new(
            content_root.clone(),
            remote_factory,
            postprocessor,
            std::time::Duration::from_secs(2),
        )),
        Arc::new(IndexRebuildHandler::new(database.clone(), content_root)),
        Arc::new(crate::database::query_task::DatabaseQueryTaskHandler::new(
            database,
        )),
    ]
}

fn compute_worker_config(app: &tauri::AppHandle) -> Option<crate::compute::worker::WorkerConfig> {
    let resource_worker = app
        .path()
        .resource_dir()
        .ok()
        .map(|directory| {
            directory
                .join("compute-worker")
                .join("bloomery-compute-worker.exe")
        })
        .filter(|path| path.is_file());
    if let Some(executable) = resource_worker {
        let manifest = executable.with_file_name("worker-artifact-manifest.json");
        return Some(
            crate::compute::worker::WorkerConfig::new(executable)
                .with_artifact_manifest(manifest)
                .with_process_tree_isolation(),
        );
    }

    let executable = std::env::var_os("BLOOMERY_COMPUTE_WORKER_PYTHON")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())?;
    let working_directory = std::env::var_os("BLOOMERY_COMPUTE_WORKER_DIR")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir())?;
    Some(python_worker_config(executable, working_directory))
}

fn python_worker_config(
    executable: PathBuf,
    working_directory: PathBuf,
) -> crate::compute::worker::WorkerConfig {
    let mut config =
        crate::compute::worker::WorkerConfig::new(executable).with_process_tree_isolation();
    config.args = vec!["-m".into(), "bloomery_worker".into()];
    config.working_directory = Some(working_directory);
    config
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;
