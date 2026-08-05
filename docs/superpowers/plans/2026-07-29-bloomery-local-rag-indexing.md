# Bloomery Local RAG Indexing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Bloomery local RAG with FTS5, versioned HNSW, RRF, reranking, auditable evidence, citation resolution, commands, and a 100k-chunk gate.

**Architecture:** SQLite owns text, metadata, vectors, and watermarks; HNSW is a rebuildable sidecar with Flat fallback. Retrieval independently obtains lexical and dense ranks, merges by RRF, optionally reranks, and stores every selected source in a retrieval audit.

**Tech Stack:** Rust, SQLite FTS5, hnsw_rs, SiliconFlow reranker, serde, criterion-style benchmark runner.

---

### Task 1: Build safe FTS5 indexing

**Files:** Create `0006_knowledge_fts.sql`, `rag/index/fts.rs`, and `tests/rag_fts.rs`.

- [x] **Step 1: Write failing tests** for English, steel grades, CJK fallback, phrases, filters, inactive versions, snippets, empty queries, and stable IDs.
- [x] **Step 2: Run `cargo test --test rag_fts`** and expect missing virtual table.
- [x] **Step 3: Implement FTS writes and BM25 search.** Index text, title path, source name, and normalized grade aliases. Use a structured query builder and restrict all results to selected knowledge bases and active versions.
- [x] **Step 4: Run `cargo test --test rag_fts`** and expect pass.

**Execution record (2026-08-02):** Added migration 0009 to transactionally rebuild and backfill the existing FTS5 table without rewriting prior migrations. Runtime indexing now stores workspace/base/document/version/chunk identity, heading path, source name, normalized steel-grade aliases, and text. Search builds quoted FTS expressions instead of accepting raw MATCH syntax, applies workspace and knowledge-base allowlists, joins only active document versions, caps results, produces escaped snippets, and uses a deterministic CJK substring fallback. TDD RED first proved the missing module. Verification passed 3 FTS, 8 migration, 9 knowledge repository, 1 local pipeline, and 16 architecture tests.

### Task 2: Implement versioned HNSW and Flat fallback

**Files:** Create `rag/index/vector.rs`, `lifecycle.rs`, `tests/rag_vector.rs`; modify `Cargo.toml`.

- [x] **Step 1: Write lifecycle tests** for build, reopen, insert, checksum/truncation/model mismatch, temp cleanup, atomic activation, old-index retention, and Flat fallback.
- [x] **Step 2: Run `cargo test --test rag_vector`** and expect missing index.
- [x] **Step 3: Implement this boundary:**

```rust
pub trait VectorIndex {
    fn search(&self, query: &[f32], limit: usize, filter: &CandidateFilter)
        -> Result<Vec<VectorHit>, IndexError>;
    fn watermark(&self) -> &IndexWatermark;
}
```

The manifest stores format version, provider/model, dimension, chunk count, SQLite watermark, and checksum. Build beside the target, fsync, reopen and validate, then atomically replace. Flat reads authoritative vectors if HNSW is unavailable.
- [x] **Step 4: Run `cargo test --test rag_vector`** and expect pass.

**Execution record (2026-08-03):** Added `hnsw_rs` 0.3.4 with versioned sidecar generations, checksummed manifests, fsynced temporary builds, reopen validation, atomic `CURRENT` activation, old-generation retention, startup temp cleanup, a resident HNSW search worker, and authoritative SQLite Flat fallback. TDD RED first proved the index modules were absent. Verification passed 3 vector lifecycle, 5 embedding, 1 local pipeline, and 16 architecture tests; `cargo check -j 1 --offline --tests`, `cargo fmt --all -- --check`, and the full Rust suite with 255 passed and 1 ignored also passed.

### Task 3: Implement filtered hybrid retrieval

**Files:** Create `rag/retrieve/filter.rs`, `rrf.rs`, `mod.rs`, and `tests/rag_retrieval.rs`.

- [x] **Step 1: Write rank tests** for missing lists, ties, filters, duplicate chunks, inactive sources, and candidate limits.
- [x] **Step 2: Run `cargo test --test rag_retrieval`** and expect failure.
- [x] **Step 3: Implement deterministic RRF:**

```rust
score(chunk) = sum(1.0 / (rrf_k + rank))
```

Run lexical and dense retrieval independently, deduplicate by chunk ID, sort ties by stable chunk ID, and fetch authoritative text only for the bounded candidate set.
- [x] **Step 4: Run `cargo test --test rag_retrieval`** and expect pass.

**Execution record (2026-08-03):** Added deterministic 1-based RRF with per-list deduplication, missing-list support, stable tie ordering, and bounded candidates. Hybrid retrieval independently calls FTS and the active vector index, filters dense hits to selected active versions before fusion, and fetches only fused candidates from authoritative SQLite text and source metadata. Verification passed 3 retrieval, 3 FTS, 3 vector, and 16 architecture tests; `cargo check -j 1 --offline --tests` and formatting passed.

### Task 4: Add optional reranking and explicit degradation

**Files:** Create `rag/rerank.rs`; extend `tests/rag_retrieval.rs`.

- [x] **Step 1: Write tests** for configured reranker, missing key, quota, malformed scores, timeouts, cancellation, and maximum documents.
- [x] **Step 2: Run `cargo test --test rag_retrieval rerank`** and expect failure.
- [x] **Step 3: Implement bounded reranking.** Preserve candidate IDs, reject count mismatches, and fall back to RRF with a structured degradation reason rather than failing the whole query.
- [x] **Step 4: Run retrieval and SiliconFlow mock tests** and expect pass.

**Execution record (2026-08-03):** Added an object-safe rerank boundary that accepts existing provider implementations through `Arc`, caps each request by the caller, provider, and a 64-document hard limit, and uses composite version/chunk IDs. Successful responses must contain exactly one finite score for every requested candidate; otherwise retrieval preserves the original RRF order and returns a structured degradation reason. Missing credentials, quota, timeout, cancellation, network, authentication, unsupported capability, provider response, and invalid configuration are non-blocking. TDD RED first proved the rerank module was absent. Verification passed 7 retrieval tests, 13 SiliconFlow provider mock tests, and 16 architecture tests; `cargo check -j 1 --offline --tests` and formatting passed.

### Task 5: Persist evidence and resolve citations

**Files:** Create `0007_retrieval_audit.sql`, `rag/citation.rs`, and `tests/rag_citation.rs`.

- [x] **Step 1: Write citation tests** for PDF pages, sheet ranges, headings, tables, images, text offsets, inactive versions, and deleted sources.
- [x] **Step 2: Run `cargo test --test rag_citation`** and expect missing audit layer.
- [x] **Step 3: Implement `EvidencePack`.** Persist query, configuration snapshot, lexical/dense ranks, RRF/rerank scores, selected chunks, locations, and provider/model IDs. Only `EvidencePack` enters Agent context.
- [x] **Step 4: Run `cargo test --test rag_citation`** and expect pass.

**Execution record (2026-08-03):** Added migration 0010 with immutable, workspace-indexed retrieval audit snapshots. `EvidencePack` stores the query, retrieval limits and selected knowledge bases, embedding/rerank provider and model IDs, structured rerank degradation, selected authoritative chunks, all rank scores, source locations, and matching image assets. Citation resolution formats PDF pages, sheet/table ranges, heading paths, and text offsets; image asset metadata is snapshotted, and historical citations remain resolvable with explicit `active`, `inactive`, or `deleted` source state. TDD RED first proved the citation module was absent. Verification passed 3 citation, 8 migration, 7 retrieval, and 16 architecture tests; `cargo check -j 1 --offline --tests` and formatting passed.

### Task 6: Expose local knowledge commands

**Files:** Create `app/knowledge_commands.rs`, modify `app/commands.rs`, create `tests/rag_commands.rs`.

- [x] **Step 1: Write command tests** for knowledge CRUD, document import/version list, retry/cancel, index rebuild, query, citation resolution, and health.
- [x] **Step 2: Run `cargo test --test rag_commands`** and expect missing commands.
- [x] **Step 3: Implement thin commands.** Long work returns task IDs. Destructive requests first return an impact preview and require a separate confirmed command; tests use temporary data.
- [x] **Step 4: Run `cargo test --test rag_commands` and `cargo test`** and expect pass.

**Execution record (2026-08-03):** Added the complete local Tauri knowledge surface for base CRUD, document/version listing, existing task-backed import, task retry/cancel, delete impact preview plus a separate confirmed delete, index rebuild, hybrid query, citation resolution, and health. Index rebuild is a resumable scheduler task that verifies SQLite vector checksums, builds the versioned HNSW sidecar, and atomically activates it; query loads the configured SiliconFlow embedding/rerank profiles and Windows credential references, falls back to authoritative Flat search when needed, and returns only a persisted `EvidencePack`. The frontend's single bridge now exposes typed methods for every command without direct component-level Tauri calls. TDD RED first identified the missing catalog, rebuild, and command registrations. Verification passed 4 command tests including a real scheduler/HNSW reopen test, the full Rust suite with 269 passed and 1 ignored credential smoke test, `cargo check -j 1 --offline --tests`, formatting, frontend runtime boundaries, and the production frontend build.

### Task 7: Add index repair and diagnostics

**Files:** Modify `rag/index/lifecycle.rs`, `diagnostics/mod.rs`; create `tests/rag_repair.rs`.

- [x] **Step 1: Write tests** for database/index watermark divergence, missing sidecar, low disk, interrupted build, and model change.
- [x] **Step 2: Run `cargo test --test rag_repair`** and expect failure.
- [x] **Step 3: Implement repair states** `healthy`, `degraded_flat`, `rebuild_required`, `rebuilding`, and `failed`. Never destroy the last validated index before a replacement passes reopen and checksum checks.
- [x] **Step 4: Run repair, vector, and migration tests** and expect pass.

**Execution record (2026-08-03):** Added workspace-scoped index health inspection with exact rebuild-space estimates, validated HNSW/Flat serving modes, distinct missing/corrupt/watermark/model/interrupted/low-disk/task-failure reasons, and durable `healthy`, `degraded_flat`, `rebuild_required`, `rebuilding`, and `failed` states. Interrupted temporary files can be removed without touching the validated `CURRENT` generation. Diagnostics now exposes the report through a registered Tauri command and the frontend's single typed bridge. TDD added missing workspace and dimension coverage before the production fix. Verification passed 6 repair, 3 vector, 8 migration, 33 task recovery, 16 architecture, and 4 command tests; `cargo check -j 1 --offline --tests`, formatting, frontend runtime boundaries, and the production frontend build passed.

### Task 8: Enforce the 100k-chunk gate

**Files:** Create `benches/retrieval.rs`, `scripts/benchmark-retrieval.ps1`, and `docs/benchmarks/retrieval.md`.

- [x] **Step 1: Generate a deterministic 100,000-chunk steel corpus** with grade, process, defect, noise, known relevant sets, and deterministic vectors.
- [x] **Step 2: Run the benchmark** and report FTS, HNSW, fusion, and total local latency separately.
- [x] **Step 3: Add threshold checks** for recall and local candidate retrieval P95 at or below one second on the documented reference machine; exclude network reranking.
- [x] **Step 4: Run Gate C verification:** all RAG tests, migrations, task recovery, provider mocks, benchmark, `cargo check`, frontend build, PDF-to-citation smoke test, and restart smoke test.

**Execution record (2026-08-03):** Added a deterministic 100,000-chunk steel corpus with 20 grades, 17 processes, 19 defects, operational noise, known relevant sets, and 64-dimensional deterministic vectors. The release benchmark reports FTS, resident HNSW, RRF fusion, and total authoritative local retrieval independently, excludes network reranking, writes a machine-readable JSON report, and fails below 0.95 minimum recall or above 1,000 ms total P95. The acceptance run used 10 query cases and 50 measured queries; minimum recall was 1.00, FTS P95 was 6.10 ms, HNSW P95 was 17.71 ms, RRF P95 was 0.12 ms, and total local retrieval P95 was 30.17 ms. Gate C verification passed the complete offline Rust test suite (including migrations, provider mocks, task recovery, PDF-to-page citation, and database/HNSW restart smoke), `cargo check -j 1 --offline --tests --benches`, formatting, frontend runtime boundaries, and the production frontend build.

## Completion evidence

- FTS/HNSW corruption, model switch, and Flat fallback outputs.
- Persisted retrieval audits with resolvable source locations.
- PDF-to-page citation smoke evidence.
- Reproducible 100,000-chunk report meeting retrieval correctness and latency thresholds.
