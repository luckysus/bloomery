# Bloomery Storage Providers And Durable Tasks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the durable local platform for all Bloomery data, OS-protected credentials, direct external providers, and restart-safe background work.

**Architecture:** SQLite is authoritative and evolves through ordered transactions. Repositories expose domain records without Tauri types, secrets are referenced by ID and resolved only inside Rust, providers implement capability-specific traits, and a persistent task scheduler checkpoints every recoverable step.

**Tech Stack:** Rust, rusqlite bundled SQLite/FTS5, keyring, reqwest/rustls, tokio, async-trait, serde, Tauri 2.

---

## Target file map

```text
src-tauri/src/storage/mod.rs
src-tauri/src/storage/database.rs
src-tauri/src/storage/migrations.rs
src-tauri/src/storage/migrations/0001_initial.sql
src-tauri/src/storage/migrations/0002_local_workspace.sql
src-tauri/src/storage/repositories/{conversations,memories,settings,tasks}.rs
src-tauri/src/storage/secrets.rs
src-tauri/src/providers/mod.rs
src-tauri/src/providers/capabilities.rs
src-tauri/src/providers/profiles.rs
src-tauri/src/providers/openai.rs
src-tauri/src/providers/ollama.rs
src-tauri/src/providers/siliconflow.rs
src-tauri/src/providers/mineru.rs
src-tauri/src/providers/http.rs
src-tauri/src/tasks/{mod,model,repository,scheduler}.rs
src-tauri/tests/{migrations,providers,tasks,secrets}.rs
```

The old `db.rs`, `schema.sql`, and provider logic inside `local_agent.rs` are deleted only after their callers use the new modules.

### Task 1: Add ordered database migrations

**Files:**
- Create: `src-tauri/src/storage/database.rs`
- Create: `src-tauri/src/storage/migrations.rs`
- Create: `src-tauri/src/storage/migrations/0001_initial.sql`
- Create: `src-tauri/src/storage/migrations/0002_local_workspace.sql`
- Create: `src-tauri/tests/migrations.rs`
- Modify: `src-tauri/src/app/mod.rs`

- [x] **Step 1: Write migration tests**

Cover empty, legacy, current, failed, and future databases:

```rust
#[test]
fn migrates_legacy_schema_without_losing_messages() { /* seed legacy schema, migrate, compare rows */ }

#[test]
fn rejects_database_from_newer_bloomery() {
    assert_eq!(open_with_version(999).unwrap_err().code(), "database_too_new");
}
```

- [x] **Step 2: Run tests and verify missing migrator failure**

Run `cargo test --test migrations`; expect unresolved storage modules.

- [x] **Step 3: Implement a dependency-free migrator**

Use `PRAGMA user_version`, `BEGIN IMMEDIATE`, and embedded SQL:

```rust
pub struct Migration { pub version: u32, pub sql: &'static str }

pub fn migrate(conn: &mut Connection) -> Result<(), StorageError> {
    // reject future version, run each migration in one transaction, set user_version last
}
```

`0002_local_workspace.sql` copies legacy rows into tables using `workspace_id = 'local'`, preserving UUIDs, timestamps, messages, memories, summaries, drafts, and settings. Legacy cloud jobs and cloud base settings are not migrated into active product tables; record their counts in a migration report rather than deleting the old database before backup.

- [x] **Step 4: Run migration and existing data tests**

Run `cargo test --test migrations` and all current DB tests.
**Execution record (2026-07-29):** Added transactional `PRAGMA user_version` migrations 0001-0002, schema history/report tables, legacy setting archive, private-cloud setting exclusion, retained `cloud_jobs` reporting, workspace-column conversion, WAL connections, and future-version rejection. Removed the duplicate `src/schema.sql` after every caller moved to the migrator. Verification passed with 35 library tests, 8 architecture tests, 7 migration tests, `cargo fmt --check`, and `cargo check -j 1`.

### Task 2: Split repositories from Tauri commands

**Files:**
- Create: `src-tauri/src/storage/repositories/mod.rs`
- Create: `src-tauri/src/storage/repositories/conversations.rs`
- Create: `src-tauri/src/storage/repositories/memories.rs`
- Create: `src-tauri/src/storage/repositories/settings.rs`
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/app/commands.rs`

- [x] **Step 1: Add repository contract tests**

Test create/list/archive/restore/delete conversation, append/edit/truncate/fork message, memory CRUD, drafts, summaries, and settings against an in-memory migrated database. Include transaction rollback tests.

- [x] **Step 2: Run tests and verify modules are missing**

Run `cargo test storage::repositories --lib` and expect compilation failure.

- [x] **Step 3: Implement focused repositories**

Each repository accepts `&Connection` or `&Transaction` and domain IDs. It does not import `tauri` or global state:

```rust
pub fn append_message(
    tx: &Transaction<'_>, workspace_id: &str, input: NewMessage
) -> Result<Message, StorageError>;
```

Keep Tauri command functions as argument-validation adapters. Reduce transitional `db.rs` below 400 lines, then delete it when all commands move to feature modules.

- [x] **Step 4: Run repository and command tests**

Run `cargo test storage::repositories --lib` and `cargo test`.
**Execution record (2026-07-29):** Added Tauri-independent conversation, memory, and settings repositories; transactional delete/edit/truncate/fork/snapshot/append paths; workspace-scoped ownership; thin command adapters; Agent/runtime repository reuse; and architecture guards forbidding Tauri in repositories and SQL in command adapters. Reduced `db.rs` from 1545 to 177 lines. A red-green regression also fixed partial steel-grade retrieval (`Q355` matching `Q355B`) in the shared BM25 layer. Verification passed with 29 library tests, 9 architecture tests, 7 migration tests, 6 repository tests, full `cargo test -j 1`, and `cargo check -j 1`.


### Task 3: Store provider profiles without secrets

**Files:**
- Create: `src-tauri/src/providers/profiles.rs`
- Create: `src-tauri/src/storage/repositories/provider_profiles.rs`
- Add migration: `src-tauri/src/storage/migrations/0003_provider_profiles.sql`
- Create: `src-tauri/tests/providers.rs`

- [x] **Step 1: Write serialization and persistence tests**

Use this public shape:

```rust
pub struct ProviderProfile {
    pub id: Uuid,
    pub kind: ProviderKind,
    pub display_name: String,
    pub base_url: String,
    pub model_id: Option<String>,
    pub secret_ref: Option<String>,
    pub enabled: bool,
}
```

Assert serialized profiles never contain a field named `api_key`, `token`, or `secret_value`.

- [x] **Step 2: Run test and verify missing table/type failure**

Run `cargo test --test providers profile`.

- [x] **Step 3: Implement validation and repository**

Normalize base URLs without appending guessed endpoint paths. Reject non-HTTP schemes except provider-specific local transports. Enforce unique profile IDs and one optional default per capability.

- [x] **Step 4: Run provider profile tests**

Run `cargo test --test providers profile` and expect pass.
**Execution record (2026-07-29):** Added migration 0003, workspace-scoped provider profiles, separate per-capability defaults, URL normalization without guessed endpoint paths, explicit brand serialization, and enabled/capability validation. SQLite stores only metadata and `secret_ref`; no API key/token/secret value columns exist. Fixed migration-report aggregation so later migrations cannot overwrite 0002 legacy counts. Verification passed with 4 provider tests, 7 migration tests, 9 architecture tests, full 55-test `cargo test -j 1`, and `cargo check -j 1`.


### Task 4: Integrate Windows Credential Manager

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/storage/secrets.rs`
- Create: `src-tauri/tests/secrets.rs`
- Add Tauri secret commands that return status only

- [x] **Step 1: Write tests against an injected secret backend**

Define:

```rust
pub trait SecretStore: Send + Sync {
    fn set(&self, reference: &SecretRef, value: &SecretValue) -> Result<(), SecretError>;
    fn get(&self, reference: &SecretRef) -> Result<SecretValue, SecretError>;
    fn delete(&self, reference: &SecretRef) -> Result<(), SecretError>;
}
```

Use an in-memory implementation in tests and assert `Debug` output redacts `SecretValue`.

- [x] **Step 2: Run tests and verify failure**

Run `cargo test --test secrets`.

- [x] **Step 3: Implement the keyring backend**

Add `keyring` and use service name `io.bloomery.desktop`, account `<provider-profile-id>/<credential-name>`. Commands expose `secret_set`, `secret_status`, and `secret_delete`; there is no command that returns a secret value to React.

- [x] **Step 4: Run unit tests and a Windows manual smoke test**

Run `cargo test --test secrets`. In a dev run, save a disposable credential, verify status is configured, delete it, and verify the credential is absent.
**Execution record (2026-07-29):** Added redacted `SecretValue`, validated `SecretRef`, injectable `SecretStore`, keyring 4.1.5 backed by Windows Credential Manager, and Tauri set/status/delete commands with no get-secret command. The ignored real-store smoke test wrote a random disposable credential, read it back, deleted it, and verified `secret_not_found`. Verification passed with 2 secret contract tests, 10 architecture tests, full 58-test `cargo test -j 1`, `cargo check -j 1`, and the explicit Windows smoke test.


### Task 5: Build redacting HTTP infrastructure

**Files:**
- Create: `src-tauri/src/providers/http.rs`
- Create: `src-tauri/src/diagnostics/redaction.rs`
- Create: `src-tauri/tests/http_redaction.rs`

- [x] **Step 1: Write redaction tests**

Test authorization headers, query tokens, JSON error bodies containing keys, and known values from `SecretStore`. Expected diagnostics contain `[REDACTED]` and never the input secret.

- [x] **Step 2: Run tests and verify failure**

Run `cargo test --test http_redaction`.

- [x] **Step 3: Implement one shared client factory**

Build reqwest clients with rustls, bounded connect/request timeouts, optional proxy, product user agent, and no implicit redirect from HTTPS to HTTP. Convert errors into stable categories: `network`, `authentication`, `quota`, `timeout`, `provider_response`, and `cancelled`.

- [x] **Step 4: Run redaction and provider tests**

Run `cargo test --test http_redaction --test providers`.
**Execution record (2026-07-30):** Added structured header, URL-query, JSON, response-body, and known-secret redaction; stable provider error categories; one rustls client factory with bounded timeouts/proxy/user-agent; and HTTPS-to-HTTP redirect blocking. Migrated the existing local LLM call to the shared factory and redacted status errors. Architecture tests now reject any second reqwest client constructor. Verification passed with 4 redaction tests, 30 library tests, 11 architecture tests, full 64-test `cargo test -j 1`, and `cargo check -j 1`.


### Task 6: Implement normalized provider capabilities

**Files:**
- Create: `src-tauri/src/providers/capabilities.rs`
- Create: `src-tauri/src/providers/openai.rs`
- Create: `src-tauri/src/providers/ollama.rs`
- Modify: `src-tauri/src/local_agent.rs`
- Modify: `src-tauri/tests/providers.rs`

- [x] **Step 1: Write provider mock tests**

Cover streaming text, tool-call deltas, usage, cancellation, malformed SSE, JSON-only responses, unsupported capabilities, and Ollama/OpenAI endpoint normalization.

- [x] **Step 2: Run tests and verify missing provider implementation**

Run `cargo test --test providers chat`.

- [x] **Step 3: Implement capability types**

Define `ChatProvider`, `EmbeddingProvider`, `RerankProvider`, and `DocumentParserProvider`. Provider instances return a `ProviderCapabilities` value including context size, streaming, tools, JSON schema, batch size, and model identity. The agent asks capabilities rather than matching provider names.

- [x] **Step 4: Migrate LLM streaming and verify**

Move URL normalization, request structs, SSE parsing, and default presets out of `local_agent.rs`. Run `cargo test --test providers chat` and local-agent regression tests.
**Execution record (2026-07-31):** Added normalized chat, embedding, rerank, and document-parser capabilities; OpenAI-compatible SSE/JSON parsing with tool-call deltas, usage, cancellation, and bounded bodies; an Ollama adapter with explicit unsupported-capability gates; and one enum-backed provider factory so the local agent checks `ProviderCapability::Chat` instead of branching on provider brands. Verification passed with 26 provider tests, 12 architecture tests, the local-agent streaming regression, full 92-test `cargo test -j 1 --offline`, and `cargo check -j 1 --offline`.

### Task 7: Implement SiliconFlow embedding and reranking

**Files:**
- Create: `src-tauri/src/providers/siliconflow.rs`
- Modify: `src-tauri/tests/providers.rs`

- [x] **Step 1: Write mock-server request tests**

Assert `BAAI/bge-m3` embedding batches preserve input order and vector dimensions. Assert `BAAI/bge-reranker-v2-m3` returns normalized candidate IDs and scores. Cover free/Pro labels as UI metadata, not different network code paths.

- [x] **Step 2: Run tests and verify failure**

Run `cargo test --test providers siliconflow`.

- [x] **Step 3: Implement direct SiliconFlow calls**

Resolve credentials inside Rust, batch by provider limits, respect Retry-After, use bounded retries only for idempotent requests, and surface quota/auth errors without retry storms. Allow custom model IDs.

- [x] **Step 4: Run SiliconFlow tests**

Run `cargo test --test providers siliconflow`.
**Execution record (2026-07-31):** Added direct `BAAI/bge-m3` embedding and `BAAI/bge-reranker-v2-m3` reranking with custom model IDs, 64-item batching, response-index order restoration, cross-batch dimension validation, candidate-ID normalization, and Free/Pro metadata on one network path. Requests use bounded idempotent retries with Retry-After parsing and jitter, preserve auth/quota categories across truncated bodies, cap responses at 16 MiB, and reject non-finite plus f64-to-f32 overflow/underflow values. Windows mock servers now bound accept/read/write/join waits. Independent specification and quality reviews approved the implementation; the full Rust verification above passed.

### Task 8: Implement MinerU provider operations

**Files:**
- Create: `src-tauri/src/providers/mineru.rs`
- Modify: `src-tauri/tests/providers.rs`

- [x] **Step 1: Write MinerU lifecycle tests**

Test submit, poll running, complete, download artifact, cancel, retryable failure, permanent failure, invalid archive, and credentials absent. Use recorded mock responses with no real key.

- [x] **Step 2: Run tests and verify failure**

Run `cargo test --test providers mineru`.

- [x] **Step 3: Implement `DocumentParserProvider`**

Return stable `RemoteTaskId`; persist no file bytes in provider profiles. Validate download content type, size, archive paths, and checksum before handing artifacts to RAG.

- [x] **Step 4: Run MinerU provider tests**

Run `cargo test --test providers mineru`.

**Execution record (2026-07-31):** Implemented the MinerU v4 batch lifecycle (`file-urls/batch`, signed PUT upload, result polling, and ZIP download) with stable task IDs, explicit unsupported remote cancellation, bounded network/status/body retries, `Retry-After` limits, authentication/quota/permanent failure classification, and no file bytes in provider profiles. Added bounded response reads plus ZIP content-type, compressed/expanded size, entry-count, path traversal, symlink, CRC, and optional SHA-256 validation. Security review also hardened shared provider-error redaction for signed URL query credentials, URL userinfo/fragments, adjacent URLs, Unicode prose delimiters, and raw/encoded nested URLs. TDD red evidence covered missing implementation, status/body classification, archive attacks, checksum mismatches, signed URL leaks, and nested URL parsing. Independent reviews ended `SPEC_APPROVED` and `QUALITY_APPROVED`. Final verification passed with 49 provider tests, 11 HTTP-redaction tests, 12 architecture tests, `cargo check -j 1 --offline`, and `cargo fmt --all -- --check`.

### Task 9: Build persistent background task storage

**Files:**
- Add migration: `src-tauri/src/storage/migrations/0004_background_tasks.sql`
- Create: `src-tauri/src/tasks/model.rs`
- Create: `src-tauri/src/tasks/repository.rs`
- Create: `src-tauri/tests/tasks.rs`

- [x] **Step 1: Write state transition tests**

Use states `queued`, `running`, `waiting_external`, `paused`, `completed`, `failed`, `cancelled`, and `interrupted`. Reject illegal transitions such as completed-to-running.

- [x] **Step 2: Run tests and verify failure**

Run `cargo test --test tasks transitions`.

- [x] **Step 3: Implement task records and atomic claims**

Persist task kind, payload, checkpoint, attempt, next run time, progress, error code, created/updated timestamps, and cancellation request. Claim work through one transaction so two workers cannot run the same task.

- [x] **Step 4: Run task repository tests**

Run `cargo test --test tasks transitions repository`.

**Execution record (2026-07-31):** Added migration 0004, the eight-state serializable task model, workspace-scoped repository operations, durable checkpoints, cancellation requests, validated transitions, and `BEGIN IMMEDIATE` atomic claims. Claim ownership is fenced by attempt and expected state; queued work can enter running only through the due-time and cancellation-aware claim path, preventing delayed workers from mutating retried tasks. SQLite and Rust both enforce failed/completed cross-field invariants, corrupt persisted rows surface `storage_error`, and architecture tests keep task storage Tauri-independent. TDD red evidence covered missing APIs, real two-connection claim competition, stale writes before and after reclaim, explicit fake-clock timestamps, malformed schedules, terminal-state corruption, cross-workspace mutations, and claim bypass attempts. Independent reviews ended `SPEC_APPROVED` and `QUALITY_APPROVED`. Final verification passed with 18 task tests, 8 migration tests, 12 architecture tests, `cargo check -j 1 --offline`, and `cargo fmt --all -- --check`.

### Task 10: Build the restart-safe scheduler

**Files:**
- Create: `src-tauri/src/tasks/scheduler.rs`
- Modify: `src-tauri/src/app/mod.rs`
- Modify: `src-tauri/tests/tasks.rs`

- [x] **Step 1: Write scheduler tests with a fake clock and handlers**

Verify bounded concurrency, checkpoint persistence, exponential retry, cancellation, shutdown interruption, restart recovery, and no retry for permanent errors.

- [x] **Step 2: Run scheduler tests and verify failure**

Run focused scheduler tests and confirm each missing behavior fails for the intended reason before implementation.

- [x] **Step 3: Implement scheduler lifecycle**

Start after migration, recover stale `running` tasks, resume only handlers declaring `resumable()`, stop accepting new work during shutdown, and emit progress only after checkpoint persistence succeeds.

- [x] **Step 4: Run task and application tests**

Run task, scheduler-unit, architecture, formatting, and compile verification. Run the repository-wide release checks before closing Gate B.

**Execution record (2026-08-02):** Implemented the restart-safe scheduler with bounded concurrency, checkpoint-before-event ordering, fake-clock capped exponential retry, permanent-error handling, cooperative cancellation, atomic `waiting_external` reclaim, resumable restart recovery, attempt/state fencing, migration-after-start ordering, and the production `TauriEventSink`. TDD red evidence covered retry writes that would overwrite a committed cancellation, interrupt persistence failure during shutdown, unknown-kind failure writes after claim, worker-thread spawn failure after claim, and replacement of a stopped but non-durable scheduler handle. The fixes atomically choose `cancelled` versus `queued` inside `BEGIN IMMEDIATE`, retain every successful claim in durable tracking before fallible handler work, retry shutdown persistence until every current claim is interrupted/cancelled or fenced, preserve non-durable handle failure state, and compile `NoopEventSink` only for tests. Independent reviews ended `SPEC_APPROVED` and `QUALITY_APPROVED`. Final Gate B verification passed with the full offline Rust suite (38 library, 13 architecture, 11 HTTP-redaction, 8 migration, 49 provider, 6 repository, 2 secret, 33 task, and doc tests; the disposable Windows Credential Manager smoke remains intentionally ignored), `cargo fmt --all -- --check`, warning-free `cargo check -j 1 --offline`, and the frontend production build (1,580 modules transformed).

### Task 11: Add provider and storage diagnostics commands

**Files:**
- Create: `src-tauri/src/diagnostics/mod.rs`
- Create: `src-tauri/src/app/provider_commands.rs`
- Create: `src-tauri/src/app/task_commands.rs`
- Modify: `src-tauri/src/app/commands.rs`

- [x] **Step 1: Write command serialization tests**

Assert profile responses contain `secret_configured: bool`, never a secret. Task responses expose stable IDs, states, progress, and recoverable error codes.

- [x] **Step 2: Run tests and verify missing commands**

Run `cargo test provider_commands task_commands --lib`.

- [x] **Step 3: Implement commands**

Add list/save/test/delete provider profile, set/delete secret, list/cancel/retry task, database health, migration version, and disk-space checks. Keep command bodies thin.

- [x] **Step 4: Run Gate B verification**

Run `cargo test`, `cargo check`, frontend build, and a development smoke test that restarts a fake long task. Update Gate B only when OS secret and restart evidence exist.

**Execution record (2026-08-02):** Added workspace-scoped list/save/test/delete provider commands; existing set/status/delete secret commands remain the only credential surface. Provider responses expose `secret_configured` without secret references or credential names, accept omitted/null/empty IDs for Rust UUID generation, remove configured credentials on profile deletion, and probe the configured base URL with five-second connect/request bounds without reading response bodies or invoking paid inference. Probe results use stable `authentication`, `quota`, `timeout`, `network`, `provider_response`, and `insecure_transport` categories; 404/405 count as reachable. Added background-task list/cancel/retry commands whose DTOs exclude payloads and checkpoints. Running cancellation is cooperative and atomically fenced by state/attempt; manual retry atomically clears stale schedules, errors, and cancellation while retaining checkpoint/progress. Added SQLite quick-check, current/latest migration versions, database size, reclaimable page bytes, and native Windows free-disk diagnostics without paths or provider metadata. TDD RED evidence covered all missing command APIs, unsafe response fields, invalid credential names, transport error instability, empty-string IDs, secret cleanup, running-cancellation races, and delayed manual retries. Main-thread specification and quality reviews approved the implementation after those fixes; both attempted independent reviews were unavailable due a local model-proxy 503. Final Gate B verification passed with the full offline Rust suite (52 library, 13 architecture, 11 HTTP-redaction, 8 migration, 49 provider, 6 repository, 2 secret, 33 task, and doc tests; the disposable Credential Manager test remains intentionally ignored in the suite but its explicit Windows smoke result is recorded in Task 4), `cargo fmt --all -- --check`, warning-free `cargo check -j 1 --offline`, and the frontend production build (1,580 modules transformed). The restart-safe fake long-task recovery test passed in the 33-task suite.

## Completion evidence

- Migration tests proving legacy data preservation and future-version rejection.
- Keyring tests and a Windows Credential Manager smoke result.
- Provider mock tests for OpenAI-compatible, Ollama, SiliconFlow, and MinerU.
- Restart-safe task scheduler integration tests.
- Secret scan proving profiles, SQLite, logs, diagnostics, and command responses contain no key values.
