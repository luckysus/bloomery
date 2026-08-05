use super::{
    bound_output, ArtifactStore, ConcurrencyPolicy, RegistryError, ToolDefinition, ToolError,
    ToolId, ToolRegistry, ToolSnapshot,
};
use crate::agent::runtime::CancellationToken;
use futures_util::{
    future::{select, Either},
    lock::Mutex,
    FutureExt,
};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, panic::AssertUnwindSafe, pin::Pin, sync::Arc};

pub type ToolFuture = Pin<Box<dyn Future<Output = Result<super::ToolOutput, ToolError>> + Send>>;

pub trait ToolHandler: Send + Sync {
    fn execute(&self, arguments: Value, cancellation: CancellationToken) -> HandlerFuture;
}

pub type HandlerFuture = Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + 'static>>;

pub struct ToolRegistration {
    pub definition: ToolDefinition,
    pub handler: Arc<dyn ToolHandler>,
}

impl ToolRegistration {
    pub fn new(definition: ToolDefinition, handler: Arc<dyn ToolHandler>) -> Self {
        Self {
            definition,
            handler,
        }
    }
}

pub struct ToolExecutor {
    registrations: BTreeMap<ToolId, ToolRegistration>,
    snapshot: ToolSnapshot,
    artifact_store: Arc<dyn ArtifactStore>,
    serial_gate: Arc<Mutex<()>>,
}

impl ToolExecutor {
    pub fn new(
        registrations: Vec<ToolRegistration>,
        artifact_store: Arc<dyn ArtifactStore>,
    ) -> Result<Self, RegistryError> {
        let mut registry = ToolRegistry::new();
        let mut by_id = BTreeMap::new();
        for registration in registrations {
            registry.register(registration.definition.clone())?;
            by_id.insert(registration.definition.id.clone(), registration);
        }
        Ok(Self {
            snapshot: registry.snapshot(),
            registrations: by_id,
            artifact_store,
            serial_gate: Arc::new(Mutex::new(())),
        })
    }

    pub fn snapshot(&self) -> &ToolSnapshot {
        &self.snapshot
    }

    pub fn execute(
        &self,
        id: &ToolId,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> ToolFuture {
        let requested_id = id.clone();
        let Some(registration) = self.registrations.get(id) else {
            return Box::pin(async move {
                Err(ToolError::new(
                    "unknown_tool",
                    format!("tool is not registered: {requested_id}"),
                ))
            });
        };
        let definition = registration.definition.clone();
        let handler = registration.handler.clone();
        let artifact_store = self.artifact_store.clone();
        let serial_gate = self.serial_gate.clone();
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(ToolError::cancelled());
            }
            let timeout = definition.timeout;
            let tool_id = definition.id.clone();
            let concurrency = definition.concurrency;
            let run_cancellation = cancellation.clone();
            let run = async move {
                let _guard = match concurrency {
                    ConcurrencyPolicy::ParallelRead => None,
                    ConcurrencyPolicy::SerialWrite | ConcurrencyPolicy::Exclusive => {
                        Some(serial_gate.lock().await)
                    }
                };
                run_handler(handler, arguments, run_cancellation).await
            };
            let timed = Box::pin(tokio::time::timeout(timeout, run));
            let cancelled = Box::pin(wait_for_cancellation(cancellation));
            let value = match select(timed, cancelled).await {
                Either::Left((result, _)) => result.map_err(|_| {
                    ToolError::new(
                        "tool_timeout",
                        format!("tool {tool_id} exceeded its timeout"),
                    )
                })?,
                Either::Right((_, _)) => return Err(ToolError::cancelled()),
            }?;
            bound_output(value, artifact_store.as_ref())
        })
    }
}

async fn run_handler(
    handler: Arc<dyn ToolHandler>,
    arguments: Value,
    cancellation: CancellationToken,
) -> Result<Value, ToolError> {
    match AssertUnwindSafe(async move { handler.execute(arguments, cancellation).await })
        .catch_unwind()
        .await
    {
        Ok(result) => result,
        Err(_) => Err(ToolError::new(
            "tool_panicked",
            "tool handler panicked during execution",
        )),
    }
}

async fn wait_for_cancellation(cancellation: CancellationToken) {
    loop {
        if cancellation.is_cancelled() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}
