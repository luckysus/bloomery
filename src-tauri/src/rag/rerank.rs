use crate::providers::capabilities::{RerankDocument, RerankProvider, RerankResult};
use crate::providers::http::{ProviderError, ProviderErrorCode};
use crate::rag::retrieve::RetrievedChunk;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub const MAX_RERANK_DOCUMENTS: usize = 64;

pub type RerankRemoteFuture =
    Pin<Box<dyn Future<Output = Result<Vec<RerankResult>, ProviderError>> + Send + 'static>>;

pub trait RerankRemote: Send + Sync {
    fn max_documents(&self) -> usize;
    fn rerank(&self, query: String, documents: Vec<RerankDocument>) -> RerankRemoteFuture;
}

impl<P> RerankRemote for Arc<P>
where
    P: RerankProvider + 'static,
{
    fn max_documents(&self) -> usize {
        RerankProvider::capabilities(self.as_ref())
            .max_batch_size
            .unwrap_or(1)
    }

    fn rerank(&self, query: String, documents: Vec<RerankDocument>) -> RerankRemoteFuture {
        let provider = Arc::clone(self);
        Box::pin(async move { provider.rerank(query, documents).await })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RerankDegradationReason {
    MissingCredential,
    InvalidConfiguration,
    Network,
    Authentication,
    Quota,
    Timeout,
    Cancelled,
    UnsupportedCapability,
    ProviderResponse,
    MalformedResponse,
}

pub enum RerankProviderState<'a> {
    Disabled,
    Ready(&'a dyn RerankRemote),
    Unavailable(RerankDegradationReason),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RerankOutcome {
    pub chunks: Vec<RetrievedChunk>,
    pub degradation: Option<RerankDegradationReason>,
}

pub async fn rerank_candidates(
    query: &str,
    chunks: Vec<RetrievedChunk>,
    provider: RerankProviderState<'_>,
    max_documents: usize,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> RerankOutcome {
    if chunks.is_empty() {
        return outcome(chunks, None);
    }
    let provider = match provider {
        RerankProviderState::Disabled => return outcome(chunks, None),
        RerankProviderState::Unavailable(reason) => return outcome(chunks, Some(reason)),
        RerankProviderState::Ready(provider) => provider,
    };
    if is_cancelled() {
        return outcome(chunks, Some(RerankDegradationReason::Cancelled));
    }
    let limit = chunks
        .len()
        .min(max_documents)
        .min(provider.max_documents())
        .min(MAX_RERANK_DOCUMENTS);
    if limit == 0 {
        return outcome(chunks, Some(RerankDegradationReason::InvalidConfiguration));
    }
    let documents = chunks[..limit]
        .iter()
        .map(|chunk| RerankDocument {
            id: candidate_id(chunk),
            text: chunk.text.clone(),
        })
        .collect::<Vec<_>>();
    let expected = documents
        .iter()
        .map(|document| document.id.clone())
        .collect::<HashSet<_>>();
    let results = match provider.rerank(query.trim().to_string(), documents).await {
        Ok(results) if !is_cancelled() => results,
        Ok(_) => return outcome(chunks, Some(RerankDegradationReason::Cancelled)),
        Err(error) => return outcome(chunks, Some(provider_degradation(error))),
    };
    let scores = match validate_results(results, &expected) {
        Some(scores) => scores,
        None => return outcome(chunks, Some(RerankDegradationReason::MalformedResponse)),
    };

    let mut reranked = chunks[..limit].to_vec();
    for chunk in &mut reranked {
        chunk.rerank_score = scores.get(&candidate_id(chunk)).copied();
    }
    reranked.sort_by(|left, right| {
        right
            .rerank_score
            .unwrap_or_default()
            .total_cmp(&left.rerank_score.unwrap_or_default())
    });
    reranked.extend_from_slice(&chunks[limit..]);
    outcome(reranked, None)
}

fn validate_results(
    results: Vec<RerankResult>,
    expected: &HashSet<String>,
) -> Option<HashMap<String, f32>> {
    if results.len() != expected.len() {
        return None;
    }
    let mut scores = HashMap::with_capacity(results.len());
    for result in results {
        if !expected.contains(&result.id)
            || !result.score.is_finite()
            || scores.insert(result.id, result.score).is_some()
        {
            return None;
        }
    }
    (scores.len() == expected.len()).then_some(scores)
}

fn candidate_id(chunk: &RetrievedChunk) -> String {
    format!("{}:{}", chunk.version_id, chunk.chunk_id)
}

fn provider_degradation(error: ProviderError) -> RerankDegradationReason {
    match error.code() {
        ProviderErrorCode::Network => RerankDegradationReason::Network,
        ProviderErrorCode::Authentication => RerankDegradationReason::Authentication,
        ProviderErrorCode::Quota => RerankDegradationReason::Quota,
        ProviderErrorCode::Timeout => RerankDegradationReason::Timeout,
        ProviderErrorCode::ProviderResponse => RerankDegradationReason::ProviderResponse,
        ProviderErrorCode::Cancelled => RerankDegradationReason::Cancelled,
        ProviderErrorCode::UnsupportedCapability => RerankDegradationReason::UnsupportedCapability,
    }
}

fn outcome(
    chunks: Vec<RetrievedChunk>,
    degradation: Option<RerankDegradationReason>,
) -> RerankOutcome {
    RerankOutcome {
        chunks,
        degradation,
    }
}
