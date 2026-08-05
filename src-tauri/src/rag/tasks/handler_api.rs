use super::{MinerUTaskPayload, StoredObjectRef};
use crate::providers::capabilities::{
    DocumentParseRequest, DocumentTaskStatus, ParsedDocumentArtifact, RemoteTaskId,
};
use crate::providers::http::ProviderError;
use crate::rag::model::DocumentVersionId;
use crate::tasks::scheduler::HandlerError;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

pub const MINERU_TASK_KIND: &str = "mineru_parse";

pub type MinerURemoteFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, ProviderError>> + Send + 'static>>;
pub type MinerUProcessFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, HandlerError>> + Send + 'static>>;
pub type CancellationCheck = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Debug)]
pub struct MinerUUploadTicket {
    id: RemoteTaskId,
    upload_url: String,
    bytes: Vec<u8>,
}

impl MinerUUploadTicket {
    pub fn new(
        id: RemoteTaskId,
        upload_url: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<Self, ProviderError> {
        let upload_url = upload_url.into();
        if id.0.trim().is_empty() || upload_url.trim().is_empty() {
            return Err(ProviderError::new(
                crate::providers::http::ProviderErrorCode::ProviderResponse,
                None,
                "MinerU upload ticket is invalid",
            ));
        }
        Ok(Self {
            id,
            upload_url,
            bytes,
        })
    }

    pub fn id(&self) -> &RemoteTaskId {
        &self.id
    }

    pub fn into_upload(self) -> (String, Vec<u8>) {
        (self.upload_url, self.bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFinalization {
    task_id: Uuid,
    attempt: u32,
    checkpoint_json: String,
}

impl TaskFinalization {
    pub fn new(task_id: Uuid, attempt: u32, checkpoint_json: String) -> Self {
        Self {
            task_id,
            attempt,
            checkpoint_json,
        }
    }

    pub fn task_id(&self) -> Uuid {
        self.task_id
    }

    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    pub fn checkpoint_json(&self) -> &str {
        &self.checkpoint_json
    }
}

pub trait MinerURemote: Send + Sync {
    fn create_batch(&self, request: DocumentParseRequest)
        -> MinerURemoteFuture<MinerUUploadTicket>;
    fn upload(&self, ticket: MinerUUploadTicket) -> MinerURemoteFuture<()>;
    fn poll(&self, id: RemoteTaskId) -> MinerURemoteFuture<DocumentTaskStatus>;
    fn download(&self, id: RemoteTaskId) -> MinerURemoteFuture<ParsedDocumentArtifact>;
    fn cancel(&self, id: RemoteTaskId) -> MinerURemoteFuture<()>;
}

pub trait MinerURemoteFactory: Send + Sync {
    fn load(
        &self,
        workspace_id: &str,
        profile_id: Uuid,
        expected_revision: u64,
        expected_secret_generation: u64,
    ) -> Result<Arc<dyn MinerURemote>, HandlerError>;
}

pub trait MinerUPostprocessor: Send + Sync {
    fn chunk(
        &self,
        workspace_id: String,
        payload: MinerUTaskPayload,
        parsed_ast: StoredObjectRef,
    ) -> MinerUProcessFuture<String>;
    fn embed(
        &self,
        workspace_id: String,
        payload: MinerUTaskPayload,
        is_cancelled: CancellationCheck,
    ) -> MinerUProcessFuture<String>;
    fn index(
        &self,
        workspace_id: String,
        payload: MinerUTaskPayload,
    ) -> MinerUProcessFuture<String>;
    fn activate(
        &self,
        workspace_id: String,
        payload: MinerUTaskPayload,
        finalization: TaskFinalization,
    ) -> MinerUProcessFuture<DocumentVersionId>;
}
