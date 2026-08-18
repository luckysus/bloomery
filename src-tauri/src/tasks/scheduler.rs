use super::model::validate_identifier;
use super::{repository, TaskError, TaskRecord, TaskState};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::Connection;
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub type HandlerFuture =
    Pin<Box<dyn Future<Output = Result<HandlerOutcome, HandlerError>> + Send + 'static>>;

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerOutcome {
    Completed,
    WaitingExternal,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerError {
    code: String,
    retryable: bool,
}

impl HandlerError {
    pub fn retryable(code: impl Into<String>) -> Self {
        Self {
            code: normalize_handler_error_code(code.into()),
            retryable: true,
        }
    }

    pub fn permanent(code: impl Into<String>) -> Self {
        Self {
            code: normalize_handler_error_code(code.into()),
            retryable: false,
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn is_retryable(&self) -> bool {
        self.retryable
    }
}

fn normalize_handler_error_code(code: String) -> String {
    if validate_identifier("error_code", &code).is_ok() {
        return code;
    }
    code.split_once(':')
        .map(|(prefix, _)| prefix.trim())
        .filter(|prefix| validate_identifier("error_code", prefix).is_ok())
        .map(str::to_string)
        .unwrap_or_else(|| "handler_error".to_string())
}

pub trait TaskHandler: Send + Sync {
    fn kind(&self) -> &str;
    fn resumable(&self) -> bool;
    fn run(&self, task: TaskRecord, context: HandlerContext) -> HandlerFuture;
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TaskProgress {
    pub id: uuid::Uuid,
    pub kind: String,
    pub state: TaskState,
    pub progress: u8,
    pub attempt: u32,
    pub error_code: Option<String>,
    pub cancel_requested: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&TaskRecord> for TaskProgress {
    fn from(task: &TaskRecord) -> Self {
        Self {
            id: task.id,
            kind: task.kind.clone(),
            state: task.state,
            progress: task.progress,
            attempt: task.attempt,
            error_code: task.error_code.clone(),
            cancel_requested: task.cancel_requested,
            created_at: task.created_at.clone(),
            updated_at: task.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SchedulerEvent {
    Progress(TaskProgress),
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: SchedulerEvent);
}

#[cfg(test)]
pub struct NoopEventSink;

#[cfg(test)]
impl EventSink for NoopEventSink {
    fn emit(&self, _event: SchedulerEvent) {}
}

#[derive(Clone)]
pub struct HandlerContext {
    path: PathBuf,
    workspace_id: String,
    task_id: uuid::Uuid,
    attempt: u32,
    control: SchedulerControl,
    sink: Arc<dyn EventSink>,
}

impl HandlerContext {
    pub fn checkpoint(
        &self,
        checkpoint_json: Option<&str>,
        progress: u8,
        next_run_at: Option<DateTime<Utc>>,
    ) -> Result<TaskRecord, TaskError> {
        let next_run_at = next_run_at.map(format_time);
        let mut connection = open_connection(&self.path)?;
        let record = repository::checkpoint(
            &mut connection,
            &self.workspace_id,
            self.task_id,
            self.attempt,
            checkpoint_json,
            progress,
            next_run_at.as_deref(),
        )?;
        drop(connection);
        self.sink
            .emit(SchedulerEvent::Progress(TaskProgress::from(&record)));
        Ok(record)
    }

    pub fn cancellation_requested(&self) -> Result<bool, TaskError> {
        let connection = open_connection(&self.path)?;
        repository::get(&connection, &self.workspace_id, self.task_id)?
            .map(|task| task.cancel_requested)
            .ok_or_else(|| TaskError::new("task_not_found", "task not found"))
    }

    pub fn shutdown_requested(&self) -> bool {
        self.control.shutdown_requested()
    }

    pub fn completed_with_checkpoint(&self, checkpoint_json: &str) -> Result<bool, TaskError> {
        let connection = open_connection(&self.path)?;
        Ok(
            repository::get(&connection, &self.workspace_id, self.task_id)?.is_some_and(|task| {
                task.attempt == self.attempt
                    && task.state == TaskState::Completed
                    && task.progress == 100
                    && task.checkpoint_json.as_deref() == Some(checkpoint_json)
            }),
        )
    }
}

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub max_workers: usize,
    pub max_attempts: u32,
    pub retry_base: Duration,
    pub retry_max: Duration,
    pub poll_interval: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_workers: 2,
            max_attempts: 4,
            retry_base: Duration::from_secs(5),
            retry_max: Duration::from_secs(5 * 60),
            poll_interval: Duration::from_millis(250),
        }
    }
}

struct ControlState {
    accepting: Mutex<bool>,
    shutdown: AtomicBool,
    wake: Condvar,
}

#[derive(Clone)]
pub struct SchedulerControl {
    state: Arc<ControlState>,
}

impl SchedulerControl {
    pub fn request_shutdown(&self) {
        let mut accepting = self
            .state
            .accepting
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *accepting = false;
        self.state.shutdown.store(true, Ordering::SeqCst);
        self.state.wake.notify_all();
    }

    pub fn shutdown_requested(&self) -> bool {
        self.state.shutdown.load(Ordering::SeqCst)
    }

    fn wait(&self, duration: Duration) {
        let accepting = self
            .state
            .accepting
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if !self.shutdown_requested() {
            drop(
                self.state
                    .wake
                    .wait_timeout(accepting, duration)
                    .unwrap_or_else(|error| error.into_inner()),
            );
        }
    }
}

struct ActiveTask {
    claim: TaskRecord,
    result: mpsc::Receiver<Result<HandlerOutcome, HandlerError>>,
    outcome: Option<Result<HandlerOutcome, HandlerError>>,
    fenced: bool,
}

pub struct Scheduler {
    path: PathBuf,
    workspace_id: String,
    config: SchedulerConfig,
    clock: Arc<dyn Clock>,
    handlers: HashMap<String, Arc<dyn TaskHandler>>,
    sink: Arc<dyn EventSink>,
    control: SchedulerControl,
    active: Vec<ActiveTask>,
    #[cfg(test)]
    before_spawn: Option<Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    fail_worker_spawn: bool,
}

impl Scheduler {
    pub fn new(
        path: PathBuf,
        workspace_id: String,
        config: SchedulerConfig,
        clock: Arc<dyn Clock>,
        handlers: Vec<Arc<dyn TaskHandler>>,
        sink: Arc<dyn EventSink>,
    ) -> Result<Self, TaskError> {
        validate_identifier("workspace_id", &workspace_id)?;
        if config.max_workers == 0 || config.max_attempts == 0 {
            return Err(TaskError::new(
                "invalid_scheduler",
                "worker and attempt limits must be greater than zero",
            ));
        }
        if config.retry_base > config.retry_max {
            return Err(TaskError::new(
                "invalid_scheduler",
                "retry base cannot exceed retry maximum",
            ));
        }
        let mut registry = HashMap::new();
        for handler in handlers {
            validate_identifier("task kind", handler.kind())?;
            if registry
                .insert(handler.kind().to_string(), Arc::clone(&handler))
                .is_some()
            {
                return Err(TaskError::new(
                    "duplicate_handler",
                    format!("handler already registered for {}", handler.kind()),
                ));
            }
        }
        Ok(Self {
            path,
            workspace_id,
            config,
            clock,
            handlers: registry,
            sink,
            control: SchedulerControl {
                state: Arc::new(ControlState {
                    accepting: Mutex::new(true),
                    shutdown: AtomicBool::new(false),
                    wake: Condvar::new(),
                }),
            },
            active: Vec::new(),
            #[cfg(test)]
            before_spawn: None,
            #[cfg(test)]
            fail_worker_spawn: false,
        })
    }

    pub fn control(&self) -> SchedulerControl {
        self.control.clone()
    }

    pub fn recover(&mut self) -> Result<(), TaskError> {
        let mut connection = open_connection(&self.path)?;
        for task in repository::list(&connection, &self.workspace_id)? {
            if task.state == TaskState::Running {
                let target = if task.cancel_requested {
                    TaskState::Cancelled
                } else {
                    TaskState::Interrupted
                };
                repository::transition(
                    &mut connection,
                    &self.workspace_id,
                    task.id,
                    task.attempt,
                    TaskState::Running,
                    target,
                    None,
                )?;
            }
        }
        for task in repository::list(&connection, &self.workspace_id)? {
            if task.state != TaskState::Interrupted {
                continue;
            }
            if task.cancel_requested {
                repository::transition(
                    &mut connection,
                    &self.workspace_id,
                    task.id,
                    task.attempt,
                    TaskState::Interrupted,
                    TaskState::Cancelled,
                    None,
                )?;
            } else if self
                .handlers
                .get(&task.kind)
                .is_some_and(|handler| handler.resumable())
            {
                repository::transition(
                    &mut connection,
                    &self.workspace_id,
                    task.id,
                    task.attempt,
                    TaskState::Interrupted,
                    TaskState::Queued,
                    None,
                )?;
            }
        }
        Ok(())
    }

    pub fn tick(&mut self) -> Result<(), TaskError> {
        self.reap_finished()?;
        if self.control.shutdown_requested() {
            return self.interrupt_active();
        }
        self.cancel_active()?;
        if self.handlers.is_empty() {
            return Ok(());
        }

        let available_workers = self.config.max_workers - self.active.len();
        for _ in 0..available_workers {
            let control_state = Arc::clone(&self.control.state);
            let accepting_guard = control_state
                .accepting
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if !*accepting_guard {
                break;
            }
            let now = format_time(self.clock.now());
            let mut connection = open_connection(&self.path)?;
            let task = repository::claim_next(&mut connection, &self.workspace_id, &now)?;
            let Some(task) = task else {
                break;
            };
            let Some(handler) = self.handlers.get(&task.kind).cloned() else {
                self.fail_unknown(task)?;
                continue;
            };
            #[cfg(test)]
            if let Some(before_spawn) = &self.before_spawn {
                before_spawn();
            }
            self.spawn(task, handler);
        }
        Ok(())
    }

    pub fn start(mut self) -> Result<SchedulerHandle, TaskError> {
        self.recover()?;
        let control = self.control();
        let thread_control = control.clone();
        let poll_interval = self.config.poll_interval;
        let worker = thread::spawn(move || loop {
            if thread_control.shutdown_requested() {
                match self.interrupt_active() {
                    Ok(()) => break true,
                    Err(err) => {
                        eprintln!("scheduler shutdown persistence error: {err}");
                        thread_control.wait(poll_interval);
                        continue;
                    }
                }
            }
            if let Err(err) = self.tick() {
                // Log or persist error here; shutdown on first non-transient failure
                eprintln!("scheduler tick error: {}", err);
                thread_control.request_shutdown();
            }
            if !thread_control.shutdown_requested() {
                thread_control.wait(poll_interval);
            }
        });
        Ok(SchedulerHandle {
            control,
            worker: Mutex::new(Some(worker)),
            durable_stopped: AtomicBool::new(false),
        })
    }
    fn spawn(&mut self, task: TaskRecord, handler: Arc<dyn TaskHandler>) {
        let context = HandlerContext {
            path: self.path.clone(),
            workspace_id: self.workspace_id.clone(),
            task_id: task.id,
            attempt: task.attempt,
            control: self.control(),
            sink: Arc::clone(&self.sink),
        };
        let worker_task = task.clone();
        let (sender, result) = mpsc::channel();
        self.active.push(ActiveTask {
            claim: task,
            result,
            outcome: None,
            fenced: false,
        });
        let worker = move || {
            let outcome = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| HandlerError::permanent("handler_runtime_error"))
                .and_then(|runtime| runtime.block_on(handler.run(worker_task, context)));
            let _ = sender.send(outcome);
        };
        #[cfg(test)]
        let spawn_result = if self.fail_worker_spawn {
            Err(std::io::Error::other("injected worker spawn failure"))
        } else {
            thread::Builder::new().spawn(worker)
        };
        #[cfg(not(test))]
        let spawn_result = thread::Builder::new().spawn(worker);
        if let Err(error) = spawn_result {
            eprintln!("scheduler worker spawn error: {error}");
        }
    }

    fn reap_finished(&mut self) -> Result<(), TaskError> {
        for active in &mut self.active {
            if active.outcome.is_some() {
                continue;
            }
            match active.result.try_recv() {
                Ok(result) => active.outcome = Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    active.outcome = Some(Err(HandlerError::permanent("handler_panicked")));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        let finished = self
            .active
            .iter()
            .enumerate()
            .filter_map(|(index, active)| {
                active
                    .outcome
                    .clone()
                    .map(|outcome| (index, active.claim.clone(), outcome, active.fenced))
            })
            .collect::<Vec<_>>();
        for (_, claim, outcome, fenced) in &finished {
            if !fenced {
                self.finish(claim.clone(), outcome.clone())?;
            }
        }
        for (index, _, _, _) in finished.into_iter().rev() {
            self.active.remove(index);
        }
        Ok(())
    }

    fn finish(
        &self,
        claim: TaskRecord,
        result: Result<HandlerOutcome, HandlerError>,
    ) -> Result<(), TaskError> {
        let mut connection = open_connection(&self.path)?;
        let Some(current) = repository::get(&connection, &self.workspace_id, claim.id)? else {
            return Ok(());
        };
        if current.attempt != claim.attempt || current.state != TaskState::Running {
            return Ok(());
        }
        if current.cancel_requested {
            repository::transition(
                &mut connection,
                &self.workspace_id,
                current.id,
                current.attempt,
                TaskState::Running,
                TaskState::Cancelled,
                None,
            )?;
            return Ok(());
        }
        if self.control.shutdown_requested() {
            repository::transition(
                &mut connection,
                &self.workspace_id,
                current.id,
                current.attempt,
                TaskState::Running,
                TaskState::Interrupted,
                None,
            )?;
            return Ok(());
        }
        match result {
            Ok(HandlerOutcome::Completed) => {
                self.transition_active(&mut connection, &current, TaskState::Completed, None)
            }
            Ok(HandlerOutcome::WaitingExternal) => {
                self.transition_active(&mut connection, &current, TaskState::WaitingExternal, None)
            }
            Ok(HandlerOutcome::Cancelled) => {
                self.transition_active(&mut connection, &current, TaskState::Cancelled, None)
            }
            Ok(HandlerOutcome::Interrupted) => {
                self.transition_active(&mut connection, &current, TaskState::Interrupted, None)
            }
            Err(error) if error.retryable && current.attempt < self.config.max_attempts => {
                self.schedule_retry(&mut connection, &current)
            }
            Err(error) => self.transition_active(
                &mut connection,
                &current,
                TaskState::Failed,
                Some(error.code()),
            ),
        }
    }

    fn transition_active(
        &self,
        connection: &mut Connection,
        task: &TaskRecord,
        target: TaskState,
        error_code: Option<&str>,
    ) -> Result<(), TaskError> {
        repository::transition(
            connection,
            &self.workspace_id,
            task.id,
            task.attempt,
            TaskState::Running,
            target,
            error_code,
        )?;
        Ok(())
    }

    fn schedule_retry(
        &self,
        connection: &mut Connection,
        task: &TaskRecord,
    ) -> Result<(), TaskError> {
        let multiplier = 1u32
            .checked_shl(task.attempt.saturating_sub(1).min(31))
            .unwrap_or(u32::MAX);
        let delay = self
            .config
            .retry_base
            .saturating_mul(multiplier)
            .min(self.config.retry_max);
        let now = self.clock.now();
        let due = now
            + chrono::Duration::from_std(delay)
                .map_err(|value| TaskError::new("invalid_scheduler", value.to_string()))?;
        let due = format_time(due);
        repository::schedule_retry(
            connection,
            &self.workspace_id,
            task.id,
            task.attempt,
            task.checkpoint_json.as_deref(),
            task.progress,
            &due,
            &format_time(now),
        )?;
        Ok(())
    }

    fn fail_unknown(&mut self, task: TaskRecord) -> Result<(), TaskError> {
        let (_, result) = mpsc::channel();
        self.active.push(ActiveTask {
            claim: task,
            result,
            outcome: Some(Err(HandlerError::permanent("unknown_task_kind"))),
            fenced: false,
        });
        self.reap_finished()
    }

    fn cancel_active(&mut self) -> Result<(), TaskError> {
        let mut connection = open_connection(&self.path)?;
        for active in &mut self.active {
            if active.fenced {
                continue;
            }
            let Some(current) = repository::get(&connection, &self.workspace_id, active.claim.id)?
            else {
                active.fenced = true;
                continue;
            };
            if current.attempt != active.claim.attempt || current.state != TaskState::Running {
                active.fenced = true;
            } else if current.cancel_requested {
                repository::transition(
                    &mut connection,
                    &self.workspace_id,
                    current.id,
                    current.attempt,
                    TaskState::Running,
                    TaskState::Cancelled,
                    None,
                )?;
                active.fenced = true;
            }
        }
        Ok(())
    }

    fn interrupt_active(&mut self) -> Result<(), TaskError> {
        if self.active.iter().all(|active| active.fenced) {
            return Ok(());
        }
        let mut connection = open_connection(&self.path)?;
        for active in &mut self.active {
            if active.fenced {
                continue;
            }
            let Some(current) = repository::get(&connection, &self.workspace_id, active.claim.id)?
            else {
                active.fenced = true;
                continue;
            };
            if current.attempt != active.claim.attempt || current.state != TaskState::Running {
                active.fenced = true;
                continue;
            }
            let target = if current.cancel_requested {
                TaskState::Cancelled
            } else {
                TaskState::Interrupted
            };
            repository::transition(
                &mut connection,
                &self.workspace_id,
                current.id,
                current.attempt,
                TaskState::Running,
                target,
                None,
            )?;
            active.fenced = true;
        }
        Ok(())
    }
}

pub struct SchedulerHandle {
    control: SchedulerControl,
    worker: Mutex<Option<thread::JoinHandle<bool>>>,
    durable_stopped: AtomicBool,
}

impl SchedulerHandle {
    pub fn request_shutdown(&self) {
        self.control.request_shutdown();
    }

    pub fn is_stopped(&self) -> bool {
        self.worker
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .is_none_or(thread::JoinHandle::is_finished)
    }

    pub fn shutdown(&self, timeout: Duration) -> bool {
        self.request_shutdown();
        let deadline = Instant::now() + timeout;
        while !self.is_stopped() {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(5).min(deadline - now));
        }
        if let Some(worker) = self
            .worker
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            self.durable_stopped
                .store(worker.join().unwrap_or(false), Ordering::SeqCst);
        }
        self.durable_stopped.load(Ordering::SeqCst)
    }
}

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

fn open_connection(path: &PathBuf) -> Result<Connection, TaskError> {
    let connection = Connection::open(path)
        .map_err(|error| TaskError::new("storage_error", error.to_string()))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| TaskError::new("storage_error", error.to_string()))?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| TaskError::new("storage_error", error.to_string()))?;
    Ok(connection)
}

fn format_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub struct SchedulerState {
    handle: Mutex<Option<Arc<SchedulerHandle>>>,
}

impl Default for SchedulerState {
    fn default() -> Self {
        Self {
            handle: Mutex::new(None),
        }
    }
}

impl SchedulerState {
    pub fn start(&self, scheduler: Scheduler) -> Result<bool, TaskError> {
        let mut handle = self
            .handle
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(existing) = handle.as_ref() {
            if !existing.is_stopped() {
                return Ok(false);
            }
            if !existing.shutdown(Duration::ZERO) {
                return Ok(false);
            }
        }
        *handle = Some(Arc::new(scheduler.start()?));
        Ok(true)
    }

    pub fn request_shutdown(&self) {
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        if let Some(handle) = handle {
            handle.request_shutdown();
        }
    }

    pub fn shutdown(&self, timeout: Duration) -> bool {
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        handle.is_none_or(|handle| handle.shutdown(timeout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::migrations::migrate;
    use crate::tasks::model::NewTask;
    use std::fs;
    use std::sync::TryLockError;
    use uuid::Uuid;

    struct ImmediateHandler;

    impl TaskHandler for ImmediateHandler {
        fn kind(&self) -> &str {
            "claim-gate"
        }

        fn resumable(&self) -> bool {
            true
        }

        fn run(&self, _task: TaskRecord, _context: HandlerContext) -> HandlerFuture {
            Box::pin(async { Ok(HandlerOutcome::Completed) })
        }
    }

    #[test]
    fn claim_to_spawn_holds_the_shutdown_gate() {
        let path =
            std::env::temp_dir().join(format!("bloomery-claim-gate-{}.sqlite3", Uuid::new_v4()));
        let mut connection = Connection::open(&path).expect("open test database");
        migrate(&mut connection).expect("migrate test database");
        repository::create(
            &mut connection,
            NewTask {
                workspace_id: "workspace-a".to_string(),
                kind: "claim-gate".to_string(),
                payload_json: "{}".to_string(),
                checkpoint_json: None,
                next_run_at: None,
                progress: 0,
            },
        )
        .expect("create task");
        drop(connection);

        let (reached_sender, reached_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let release_receiver = Arc::new(Mutex::new(release_receiver));
        let mut scheduler = Scheduler::new(
            path.clone(),
            "workspace-a".to_string(),
            SchedulerConfig::default(),
            Arc::new(SystemClock),
            vec![Arc::new(ImmediateHandler)],
            Arc::new(NoopEventSink),
        )
        .expect("create scheduler");
        scheduler.before_spawn = Some(Arc::new({
            let release_receiver = Arc::clone(&release_receiver);
            move || {
                reached_sender.send(()).expect("signal claimed task");
                release_receiver
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .recv_timeout(Duration::from_secs(2))
                    .expect("release claim gate");
            }
        }));
        let control = scheduler.control();
        let worker = thread::spawn(move || {
            scheduler.tick().expect("scheduler tick");
            scheduler
        });

        reached_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("scheduler reached pre-spawn boundary");
        let gate_was_held = matches!(
            control.state.accepting.try_lock(),
            Err(TryLockError::WouldBlock)
        );
        release_sender.send(()).expect("release scheduler");
        let mut scheduler = worker.join().expect("join scheduler tick");
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            scheduler.tick().expect("reap handler");
            let task = repository::list(
                &open_connection(&path).expect("open task database"),
                "workspace-a",
            )
            .expect("list tasks")
            .pop()
            .expect("persisted task");
            if task.state == TaskState::Completed {
                break;
            }
            assert!(Instant::now() < deadline, "handler completion timed out");
            thread::yield_now();
        }
        drop(scheduler);
        fs::remove_file(&path).expect("remove test database");

        assert!(
            gate_was_held,
            "shutdown gate must remain held from claim through spawn"
        );
    }

    #[test]
    fn scheduler_connections_enable_wal_for_concurrent_task_progress() {
        let path =
            std::env::temp_dir().join(format!("bloomery-scheduler-wal-{}.sqlite3", Uuid::new_v4()));
        let mut connection = Connection::open(&path).expect("open test database");
        migrate(&mut connection).expect("migrate test database");
        drop(connection);

        let scheduler_connection = open_connection(&path).expect("open scheduler database");
        let journal_mode: String = scheduler_connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read journal mode");
        drop(scheduler_connection);
        fs::remove_file(&path).expect("remove test database");

        assert_eq!(
            journal_mode, "wal",
            "scheduler task connections must use WAL for concurrent progress reads"
        );
    }

    #[test]
    fn default_scheduler_limits_concurrent_workers_for_desktop_memory_budget() {
        assert_eq!(
            SchedulerConfig::default().max_workers,
            2,
            "the desktop scheduler must leave memory headroom for the app and two bounded compute workers"
        );
    }

    #[test]
    fn spawn_failure_keeps_claim_tracked_and_fails_without_panicking() {
        let path =
            std::env::temp_dir().join(format!("bloomery-spawn-failure-{}.sqlite3", Uuid::new_v4()));
        let mut connection = Connection::open(&path).expect("open test database");
        migrate(&mut connection).expect("migrate test database");
        let created = repository::create(
            &mut connection,
            NewTask {
                workspace_id: "workspace-a".to_string(),
                kind: "claim-gate".to_string(),
                payload_json: "{}".to_string(),
                checkpoint_json: None,
                next_run_at: None,
                progress: 0,
            },
        )
        .expect("create task");
        drop(connection);
        let mut scheduler = Scheduler::new(
            path.clone(),
            "workspace-a".to_string(),
            SchedulerConfig::default(),
            Arc::new(SystemClock),
            vec![Arc::new(ImmediateHandler)],
            Arc::new(NoopEventSink),
        )
        .expect("create scheduler");
        scheduler.fail_worker_spawn = true;

        scheduler.tick().expect("claim and handle spawn failure");
        assert_eq!(scheduler.active.len(), 1);
        assert_eq!(
            repository::get(
                &open_connection(&path).expect("open task database"),
                "workspace-a",
                created.id,
            )
            .expect("read task")
            .expect("persisted task")
            .state,
            TaskState::Running
        );
        scheduler.tick().expect("persist spawn failure");

        let failed = repository::get(
            &open_connection(&path).expect("open task database"),
            "workspace-a",
            created.id,
        )
        .expect("read task")
        .expect("persisted task");
        assert_eq!(failed.state, TaskState::Failed);
        assert_eq!(failed.error_code.as_deref(), Some("handler_panicked"));
        assert!(scheduler.active.is_empty());
        drop(scheduler);
        fs::remove_file(&path).expect("remove test database");
    }

    #[test]
    fn invalid_handler_error_codes_are_normalized_before_persistence() {
        let path = std::env::temp_dir().join(format!(
            "bloomery-invalid-handler-code-{}.sqlite3",
            Uuid::new_v4()
        ));
        let mut connection = Connection::open(&path).expect("open test database");
        migrate(&mut connection).expect("migrate test database");
        let created = repository::create(
            &mut connection,
            NewTask {
                workspace_id: "workspace-a".to_string(),
                kind: "claim-gate".to_string(),
                payload_json: "{}".to_string(),
                checkpoint_json: None,
                next_run_at: None,
                progress: 0,
            },
        )
        .expect("create task");
        let claim =
            repository::claim_next(&mut connection, "workspace-a", &Utc::now().to_rfc3339())
                .expect("claim task")
                .expect("claimed task");
        drop(connection);

        let scheduler = Scheduler::new(
            path.clone(),
            "workspace-a".to_string(),
            SchedulerConfig::default(),
            Arc::new(SystemClock),
            Vec::new(),
            Arc::new(NoopEventSink),
        )
        .expect("create scheduler");
        scheduler
            .finish(
                claim,
                Err(HandlerError::permanent("query_failed: 数据库 is locked")),
            )
            .expect("invalid handler code must not break task persistence");

        let failed = repository::get(
            &open_connection(&path).expect("open task database"),
            "workspace-a",
            created.id,
        )
        .expect("read task")
        .expect("persisted task");
        assert_eq!(failed.state, TaskState::Failed);
        assert_eq!(failed.error_code.as_deref(), Some("query_failed"));
        drop(scheduler);
        fs::remove_file(&path).expect("remove test database");
    }

    #[test]
    fn state_does_not_replace_a_stopped_non_durable_handle() {
        let path = std::env::temp_dir().join(format!(
            "bloomery-non-durable-handle-{}.sqlite3",
            Uuid::new_v4()
        ));
        let mut connection = Connection::open(&path).expect("open test database");
        migrate(&mut connection).expect("migrate test database");
        repository::create(
            &mut connection,
            NewTask {
                workspace_id: "workspace-a".to_string(),
                kind: "claim-gate".to_string(),
                payload_json: "{}".to_string(),
                checkpoint_json: None,
                next_run_at: None,
                progress: 0,
            },
        )
        .expect("create task");
        drop(connection);
        let mut scheduler = Scheduler::new(
            path.clone(),
            "workspace-a".to_string(),
            SchedulerConfig::default(),
            Arc::new(SystemClock),
            vec![Arc::new(ImmediateHandler)],
            Arc::new(NoopEventSink),
        )
        .expect("create scheduler");
        scheduler.before_spawn = Some(Arc::new(|| panic!("injected scheduler panic")));
        let state = SchedulerState::default();
        assert!(state.start(scheduler).expect("start scheduler"));
        let original = state
            .handle
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .expect("scheduler handle");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !original.is_stopped() {
            assert!(Instant::now() < deadline, "scheduler panic timed out");
            thread::yield_now();
        }
        assert!(!original.shutdown(Duration::ZERO));

        let replacement = Scheduler::new(
            path.clone(),
            "workspace-a".to_string(),
            SchedulerConfig::default(),
            Arc::new(SystemClock),
            Vec::new(),
            Arc::new(NoopEventSink),
        )
        .expect("create replacement scheduler");
        assert!(!state.start(replacement).expect("reject replacement"));
        let retained = state
            .handle
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .expect("retained scheduler handle");
        assert!(Arc::ptr_eq(&original, &retained));

        drop(retained);
        drop(original);
        drop(state);
        fs::remove_file(&path).expect("remove test database");
    }
}
