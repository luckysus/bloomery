//! Opt-in smoke test for the real chat, PostgreSQL/pgvector, embedding, and
//! reranker integrations. CI keeps this test compiled but skips the network
//! calls unless `SUNA_LIVE_SMOKE=1` is explicitly set.

use std::env;
use std::time::Duration;

use suna::providers::capabilities::{
    ChatMessage, ChatProvider, ChatRequest, EmbeddingProvider, RerankDocument, RerankProvider,
};
use suna::providers::profiles::{resolve_chat_profile, ProviderKind, ProviderProfile};
use suna::providers::{
    configured_chat_provider, configured_embedding_provider, configured_rerank_provider,
    SiliconFlowPlan,
};
use suna::storage::secrets::SecretValue;

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("live smoke requires environment variable {name}"))
}

fn optional_secret(name: &str) -> Option<SecretValue> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| SecretValue::new(value).expect("live smoke secret is valid"))
}

fn siliconflow_profile(base_url: String, model_id: String) -> ProviderProfile {
    ProviderProfile {
        id: uuid::Uuid::new_v4(),
        kind: ProviderKind::SiliconFlow,
        display_name: "Suna live smoke".to_string(),
        base_url,
        model_id: Some(model_id),
        secret_ref: None,
        enabled: true,
    }
}

#[test]
fn real_stack_smoke() {
    if env::var("SUNA_LIVE_SMOKE").ok().as_deref() != Some("1") {
        return;
    }

    let chat_kind = required("SUNA_LIVE_CHAT_PROVIDER");
    let chat_base_url = required("SUNA_LIVE_CHAT_BASE_URL");
    let chat_model = required("SUNA_LIVE_CHAT_MODEL");
    let chat_profile = resolve_chat_profile(&chat_kind, &chat_base_url, &chat_model)
        .expect("live chat provider profile is valid");
    let chat_credential = optional_secret("SUNA_LIVE_CHAT_API_KEY");
    let chat = configured_chat_provider(chat_profile, chat_credential)
        .expect("live chat provider can be constructed");
    let mut events = Vec::new();
    let answer = tauri::async_runtime::block_on(chat.chat(
        ChatRequest {
            messages: vec![
                ChatMessage::new("system", "Reply with a short health check."),
                ChatMessage::new("user", "Reply exactly: SUNA_LIVE_OK"),
            ],
            temperature: 0.0,
            tools: None,
            response_format: None,
            reasoning_effort: None,
            max_tokens: Some(32),
            stop: None,
        },
        &mut |event| events.push(event),
        &|| false,
    ))
    .expect("live chat request succeeds");
    assert!(
        !answer.text.trim().is_empty(),
        "live chat returned an empty answer"
    );
    assert!(!events.is_empty(), "live chat returned no stream events");

    let postgres_url = required("SUNA_LIVE_POSTGRES_URL");
    let pool = tauri::async_runtime::block_on(
        sqlx::postgres::PgPoolOptions::new()
            .acquire_timeout(Duration::from_secs(10))
            .max_connections(2)
            .connect(&postgres_url),
    )
    .expect("live PostgreSQL connection succeeds");
    let server_version: i64 = tauri::async_runtime::block_on(
        sqlx::query_scalar("SELECT current_setting('server_version_num')::bigint").fetch_one(&pool),
    )
    .expect("PostgreSQL version query succeeds");
    assert!(
        server_version >= 140_000,
        "PostgreSQL 14 or newer is required"
    );
    let vector_enabled: bool = tauri::async_runtime::block_on(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector')")
            .fetch_one(&pool),
    )
    .expect("pgvector extension query succeeds");
    assert!(
        vector_enabled,
        "PostgreSQL pgvector extension is not enabled"
    );
    let knowledge_schema_ready: bool = tauri::async_runtime::block_on(
        sqlx::query_scalar("SELECT to_regclass('public.knowledge_bases') IS NOT NULL")
            .fetch_one(&pool),
    )
    .expect("knowledge schema query succeeds");
    assert!(
        knowledge_schema_ready,
        "Suna knowledge migrations are not installed"
    );
    tauri::async_runtime::block_on(pool.close());

    let siliconflow_base_url = required("SUNA_LIVE_SILICONFLOW_BASE_URL");
    let siliconflow_key = SecretValue::new(required("SUNA_LIVE_SILICONFLOW_API_KEY"))
        .expect("live SiliconFlow credential is valid");
    let embedding_model = required("SUNA_LIVE_EMBEDDING_MODEL");
    let embedding = configured_embedding_provider(
        siliconflow_profile(siliconflow_base_url.clone(), embedding_model),
        Some(siliconflow_key.clone()),
        SiliconFlowPlan::Free,
        Some(env::var("SUNA_LIVE_EMBEDDING_MODEL").expect("embedding model")),
    )
    .expect("live embedding provider can be constructed");
    let embedding_result = tauri::async_runtime::block_on(
        embedding.embed(vec!["Suna pgvector embedding smoke test".to_string()]),
    )
    .expect("live embedding request succeeds");
    assert_eq!(embedding_result.vectors.len(), 1);
    assert!(!embedding_result.vectors[0].is_empty());

    let reranker = configured_rerank_provider(
        siliconflow_profile(siliconflow_base_url, required("SUNA_LIVE_RERANK_MODEL")),
        Some(siliconflow_key),
        SiliconFlowPlan::Free,
        Some(required("SUNA_LIVE_RERANK_MODEL")),
    )
    .expect("live reranker provider can be constructed");
    let rerank_result = tauri::async_runtime::block_on(reranker.rerank(
        "steel yield strength".to_string(),
        vec![RerankDocument {
            id: "smoke".to_string(),
            text: "Steel yield strength is specified by grade and thickness.".to_string(),
        }],
    ))
    .expect("live reranker request succeeds");
    assert_eq!(rerank_result.len(), 1);
}
