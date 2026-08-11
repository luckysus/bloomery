use super::{MinerURemote, MinerURemoteFactory, MinerURemoteFuture, MinerUUploadTicket};
use crate::providers::capabilities::{
    DocumentParseRequest, DocumentParserProvider, DocumentTaskStatus, EmbeddingProvider,
    ParsedDocumentArtifact, RemoteTaskId,
};
use crate::providers::http::{ProviderError, ProviderErrorCode};
use crate::providers::mineru::MinerUProvider;
use crate::providers::profiles::{ProviderKind, ProviderProfile};
use crate::providers::siliconflow::{SiliconFlowPlan, SiliconFlowProvider};
use crate::rag::index::{
    EmbeddingError, EmbeddingRemote, EmbeddingRemoteFactory, EmbeddingRemoteFuture,
};
use crate::storage::repositories::{provider_profiles, settings};
use crate::storage::secrets::{SecretRef, SecretStore, SecretValue};
use crate::tasks::scheduler::HandlerError;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub struct RuntimeProviderFactory {
    database: PathBuf,
    secrets: Arc<dyn SecretStore>,
}

impl RuntimeProviderFactory {
    pub fn new(database: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        Self { database, secrets }
    }

    fn profile(
        &self,
        workspace_id: &str,
        profile_id: Uuid,
        expected_kind: ProviderKind,
        expected_revision: u64,
        expected_secret_generation: u64,
    ) -> Result<(ProviderProfile, SecretValue), FactoryFailure> {
        let connection = Connection::open(&self.database).map_err(sqlite_failure)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(sqlite_failure)?;
        let record = provider_profiles::get_record(&connection, workspace_id, profile_id)
            .map_err(|error| storage_failure(&error))?
            .ok_or(FactoryFailure::Missing)?;
        if record.revision != expected_revision {
            return Err(FactoryFailure::RevisionMismatch);
        }
        if record.secret_generation != expected_secret_generation {
            return Err(FactoryFailure::SecretGenerationMismatch);
        }
        let profile = record.profile;
        if profile.kind != expected_kind {
            return Err(FactoryFailure::KindMismatch);
        }
        if !profile.enabled {
            return Err(FactoryFailure::Disabled);
        }
        let secret_name = profile
            .secret_ref
            .as_deref()
            .ok_or(FactoryFailure::Authentication)?;
        let reference =
            SecretRef::at_generation(profile.id, secret_name, expected_secret_generation)
                .map_err(|_| FactoryFailure::Authentication)?;
        let credential = self.secrets.get(&reference).map_err(|error| {
            if error.is_not_found() {
                FactoryFailure::Authentication
            } else {
                FactoryFailure::TransientSecretStore
            }
        })?;
        Ok((profile, credential))
    }
}

impl MinerURemoteFactory for RuntimeProviderFactory {
    fn load(
        &self,
        workspace_id: &str,
        profile_id: Uuid,
        expected_revision: u64,
        expected_secret_generation: u64,
    ) -> Result<Arc<dyn MinerURemote>, HandlerError> {
        let (profile, credential) = self
            .profile(
                workspace_id,
                profile_id,
                ProviderKind::MinerU,
                expected_revision,
                expected_secret_generation,
            )
            .map_err(FactoryFailure::mineru)?;
        let provider = MinerUProvider::new(profile, Some(credential))
            .map_err(|error| provider_failure(error).mineru())?;
        Ok(Arc::new(MinerURemoteAdapter(Arc::new(provider))))
    }
}

impl EmbeddingRemoteFactory for RuntimeProviderFactory {
    fn load(
        &self,
        workspace_id: &str,
        profile_id: Uuid,
        expected_revision: u64,
        expected_secret_generation: u64,
    ) -> Result<Arc<dyn EmbeddingRemote>, EmbeddingError> {
        let plan = self
            .siliconflow_plan(workspace_id)
            .map_err(FactoryFailure::embedding)?;
        let (profile, credential) = self
            .profile(
                workspace_id,
                profile_id,
                ProviderKind::SiliconFlow,
                expected_revision,
                expected_secret_generation,
            )
            .map_err(FactoryFailure::embedding)?;
        let embedding_model = profile.model_id.clone();
        let provider = SiliconFlowProvider::with_models(
            profile,
            Some(credential),
            plan,
            embedding_model,
            None,
        )
        .map_err(|error| provider_failure(error).embedding())?;
        let provider = Arc::new(provider);
        let capabilities = EmbeddingProvider::capabilities(provider.as_ref());
        Ok(Arc::new(SiliconFlowEmbeddingAdapter {
            model_id: capabilities.model_id.clone(),
            max_batch_size: capabilities.max_batch_size.unwrap_or(1),
            provider,
        }))
    }
}

impl RuntimeProviderFactory {
    fn siliconflow_plan(&self, workspace_id: &str) -> Result<SiliconFlowPlan, FactoryFailure> {
        let connection = Connection::open(&self.database).map_err(sqlite_failure)?;
        let setting = settings::get(&connection, workspace_id, "onboarding.retrieval")
            .map_err(|error| storage_failure(&error))?;
        Ok(SiliconFlowPlan::from_setting(setting.as_deref()))
    }
}

struct MinerURemoteAdapter(Arc<MinerUProvider>);

impl MinerURemote for MinerURemoteAdapter {
    fn create_batch(
        &self,
        request: DocumentParseRequest,
    ) -> MinerURemoteFuture<MinerUUploadTicket> {
        let provider = Arc::clone(&self.0);
        Box::pin(async move {
            let (id, upload_url) = provider.create_batch(&request).await?;
            MinerUUploadTicket::new(id, upload_url, request.bytes)
        })
    }

    fn upload(&self, ticket: MinerUUploadTicket) -> MinerURemoteFuture<()> {
        let provider = Arc::clone(&self.0);
        let (upload_url, bytes) = ticket.into_upload();
        Box::pin(async move { provider.upload_batch(&upload_url, &bytes).await })
    }

    fn poll(&self, id: RemoteTaskId) -> MinerURemoteFuture<DocumentTaskStatus> {
        let provider = Arc::clone(&self.0);
        Box::pin(async move { provider.poll(&id).await })
    }

    fn download(&self, id: RemoteTaskId) -> MinerURemoteFuture<ParsedDocumentArtifact> {
        let provider = Arc::clone(&self.0);
        Box::pin(async move { provider.download(&id).await })
    }

    fn cancel(&self, id: RemoteTaskId) -> MinerURemoteFuture<()> {
        let provider = Arc::clone(&self.0);
        Box::pin(async move { provider.cancel(&id).await })
    }
}

struct SiliconFlowEmbeddingAdapter {
    model_id: String,
    max_batch_size: usize,
    provider: Arc<SiliconFlowProvider>,
}

impl EmbeddingRemote for SiliconFlowEmbeddingAdapter {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn max_batch_size(&self) -> usize {
        self.max_batch_size
    }

    fn embed(&self, inputs: Vec<String>) -> EmbeddingRemoteFuture {
        let provider = Arc::clone(&self.provider);
        Box::pin(async move { provider.embed(inputs).await })
    }
}

#[derive(Clone, Copy)]
enum FactoryFailure {
    Storage,
    TransientStorage,
    Missing,
    Disabled,
    KindMismatch,
    RevisionMismatch,
    SecretGenerationMismatch,
    Authentication,
    TransientSecretStore,
    Configuration,
}

impl FactoryFailure {
    fn mineru(self) -> HandlerError {
        let code = match self {
            Self::Storage => "mineru_storage",
            Self::TransientStorage => "mineru_storage",
            Self::Missing => "mineru_profile_missing",
            Self::Disabled => "mineru_profile_disabled",
            Self::KindMismatch => "mineru_profile_kind_mismatch",
            Self::RevisionMismatch => "mineru_profile_revision_mismatch",
            Self::SecretGenerationMismatch => "mineru_secret_generation_mismatch",
            Self::Authentication => "mineru_authentication",
            Self::TransientSecretStore => "mineru_secret_store",
            Self::Configuration => "mineru_configuration",
        };
        if matches!(self, Self::TransientStorage | Self::TransientSecretStore) {
            HandlerError::retryable(code)
        } else {
            HandlerError::permanent(code)
        }
    }

    fn embedding(self) -> EmbeddingError {
        let code = match self {
            Self::Storage => "embedding_storage",
            Self::TransientStorage => "embedding_storage",
            Self::Missing => "embedding_profile_missing",
            Self::Disabled => "embedding_profile_disabled",
            Self::KindMismatch => "embedding_profile_kind_mismatch",
            Self::RevisionMismatch => "embedding_profile_revision_mismatch",
            Self::SecretGenerationMismatch => "embedding_secret_generation_mismatch",
            Self::Authentication => "embedding_authentication",
            Self::TransientSecretStore => "embedding_secret_store",
            Self::Configuration => "embedding_configuration",
        };
        if matches!(self, Self::TransientStorage | Self::TransientSecretStore) {
            EmbeddingError::transient(code, "provider configuration is temporarily unavailable")
        } else {
            EmbeddingError::permanent(code, "provider configuration is unavailable")
        }
    }
}

fn sqlite_failure(error: rusqlite::Error) -> FactoryFailure {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            FactoryFailure::TransientStorage
        }
        _ => FactoryFailure::Storage,
    }
}

fn storage_failure(message: &str) -> FactoryFailure {
    let message = message.to_ascii_lowercase();
    if message.contains("database is locked") || message.contains("database is busy") {
        FactoryFailure::TransientStorage
    } else {
        FactoryFailure::Storage
    }
}

fn provider_failure(error: ProviderError) -> FactoryFailure {
    if error.code() == ProviderErrorCode::Authentication {
        FactoryFailure::Authentication
    } else {
        FactoryFailure::Configuration
    }
}
