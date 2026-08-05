# Bloomery Local RAG Ingest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the authoritative document, parsing, chunking, and embedding half of Bloomery local RAG.

**Architecture:** Every source becomes an immutable normalized document version. Structure-aware chunks and embedding batches are persisted before the document version can become active; MinerU work uses restart-safe task checkpoints.

**Tech Stack:** Rust, rusqlite, SHA-256, MinerU provider, pdf-extract/lopdf, zip/quick-xml, calamine, scraper, serde.

---

## Target files

```text
src-tauri/src/rag/{mod,model,tasks}.rs
src-tauri/src/rag/ingest/{mod,detect,hash}.rs
src-tauri/src/rag/parse/{mod,text,markdown,html,pdf,docx,spreadsheet,mineru}.rs
src-tauri/src/rag/chunk/{mod,policy,table}.rs
src-tauri/src/storage/repositories/knowledge.rs
src-tauri/src/storage/migrations/0005_knowledge.sql
src-tauri/tests/rag_{repository,ingest,parse,mineru,chunk,embedding}.rs
```

### Task 1: Define authoritative knowledge records

**Files:** Create `rag/model.rs`, `0005_knowledge.sql`, `repositories/knowledge.rs`, and `tests/rag_repository.rs`.

- [x] **Step 1: Write failing repository tests** for knowledge bases, source documents, immutable versions, assets, chunks, embedding metadata, attempts, and atomic active-version switching.
- [x] **Step 2: Run `cargo test --test rag_repository`** and expect missing records.
- [x] **Step 3: Implement typed IDs and locations:**

```rust
pub enum SourceLocation {
    PdfPage { page: u32, bbox: Option<Rect> },
    SheetRange { sheet: String, range: String },
    Heading { path: Vec<String> },
    TextOffsets { start: u64, end: u64 },
}
```

Only switch `active_version_id` after all assets, chunks, embeddings, FTS rows, and vector watermarks validate.
- [x] **Step 4: Run `cargo test --test migrations --test rag_repository`** and expect pass.

**Execution record (2026-08-02):** Added migration 0005 and workspace-scoped authoritative records for knowledge bases, source documents, immutable content-deduplicated versions, assets, structure-aware chunks, embedding metadata, ingest attempts, FTS membership, and vector watermarks. Typed IDs and serializable source locations preserve PDF pages/bounding boxes, sheet ranges, heading paths, and text offsets. Active-version switching runs in one transaction and requires exact asset/chunk/embedding/FTS counts plus a matching provider/model/dimension vector watermark; injected write failure proves the previous active version survives rollback. Ingest attempts transition once from running to a consistent completed, failed, or cancelled terminal state. The repository was split into bounded Tauri-independent modules. TDD RED evidence covered missing records, globally over-constrained vector keys, incomplete activation, mismatched watermarks, rollback, and invalid attempt terminal states. A full-suite run also reproduced and fixed a Windows-only test-server issue where a nonblocking listener yielded a nonblocking accepted socket. Final verification passed with 52 library, 14 architecture, 11 HTTP-redaction, 8 migration, 49 provider, 6 RAG repository, 6 general repository, 2 secret, 33 task, and doc tests; `cargo fmt --all -- --check`, warning-free `cargo check -j 1 --offline`, and the 1,580-module frontend production build also passed.

### Task 2: Implement safe ingest and deduplication

**Files:** Create `rag/ingest/detect.rs`, `hash.rs`, `mod.rs`, and `tests/rag_ingest.rs`.

- [x] **Step 1: Write failing tests** for MIME/extension disagreement, duplicate bytes, zero/oversized files, changed files, unsupported formats, and Chinese filenames.
- [x] **Step 2: Run `cargo test --test rag_ingest`** and expect missing ingest service.
- [x] **Step 3: Implement bounded streaming SHA-256 ingest.** Detect from signature plus extension, copy to a content-addressed application path, and never use the selected filename as a storage path.
- [x] **Step 4: Run `cargo test --test rag_ingest`** and expect pass.

**Execution record (2026-08-02):** Added a Tauri-independent ingest core for PDF, Markdown, TXT, HTML, DOCX, CSV, and XLSX sources using only the standard library and the existing SHA-256 dependency. One bounded read validates regular-file input and size, captures an 8 KiB signature window, hashes bytes, and writes a UUID-named staged object; format validation combines the selected extension with PDF, ZIP, binary, and UTF-8 evidence before atomically persisting to `objects/sha256/<prefix>/<digest>`. Selected and Chinese filenames never enter storage keys. Duplicate objects are length-and-hash verified before reuse, changed content creates a new immutable object, zero/oversized/unsupported/mismatched files leave no partial object, and RAII removes staged files on every failure path. TDD RED evidence first proved the missing ingest module, then exposed an 8 KiB UTF-8 sniff window splitting a Chinese character; the detector now distinguishes a truncated prefix from an invalid complete file. Verification passed with 7 ingest, 6 RAG repository, 8 migration, and 14 architecture tests, plus `cargo fmt --all -- --check` and warning-free `cargo check -j 1 --offline`.

### Task 3: Normalize supported formats into a Document AST

**Files:** Create parser modules, add parser crates to `Cargo.toml`, fixtures under `tests/fixtures/documents/`, and `tests/rag_parse.rs`.

- [x] **Step 1: Write fixture snapshots** requiring headings, paragraphs, lists, tables, formulas, images, page/sheet locations, Unicode, and warnings:

```rust
pub struct ParsedDocument {
    pub blocks: Vec<DocumentBlock>,
    pub assets: Vec<ParsedAsset>,
    pub warnings: Vec<ParseWarning>,
}
```

- [x] **Step 2: Run `cargo test --test rag_parse`** and expect parser modules missing.
- [x] **Step 3: Implement one adapter per PDF, Markdown, TXT, HTML, DOCX, CSV, and XLSX.** Local PDF parsing reports text-layer limitations; HTML never fetches remote resources; archive entries reject traversal and expansion limits.
- [x] **Step 4: Run `cargo test --test rag_parse`** and compare deterministic snapshots.

**Execution record (2026-08-02):** Added a serializable Document AST with heading, paragraph, list, table, formula, and image blocks; embedded assets; stable warnings; and PDF page, sheet range, heading path, or source-byte locations. Deterministic fixtures cover Unicode Markdown/TXT/HTML/CSV plus generated PDF/DOCX/XLSX archives. Markdown preserves byte offsets, formulas, tables, and non-loaded image references; HTML uses cached browser-grade `html5ever` tokenization and never owns a network client; CSV handles quoted separators; DOCX preserves heading context, numbered lists, tables, Office Math, and embedded media; XLSX resolves workbook relationships, shared strings, formulas, sparse cells, and sheet ranges. The local PDF fallback extracts basic plain/Flate text-show streams with page locations and always emits an explicit quality warning directing structured documents to MinerU. A shared Office ZIP boundary rejects path traversal, backslashes, symbolic links, duplicate central-directory names, excessive entries, and expanded-byte limits before reading XML. TDD RED evidence covered every missing adapter, parse-size bounds, unsafe archives, and the fact that the ZIP library folds duplicate names before iteration; raw central-directory counting closes that ambiguity. All RAG modules remain under 400 lines and Tauri/reqwest-independent. Final verification passed the full offline Rust suite (52 library, 15 architecture, 11 HTTP-redaction, 8 migration, 49 provider, 7 ingest, 10 parse, 6 RAG repository, 6 general repository, 2 secret, 33 task, and doc tests; one explicit Credential Manager write smoke remains ignored), `cargo fmt --all -- --check`, warning-free `cargo check -j 1 --offline`, and the 1,580-module frontend production build.

### Task 4: Integrate restart-safe MinerU parsing

**Files:** Create `rag/parse/mineru.rs`, `rag/tasks.rs`, and `tests/rag_mineru.rs`.

- [x] **Step 1: Write lifecycle tests** for submit, restart, polling, download, archive validation, AST conversion, cancellation, retry, and activation.
- [x] **Step 2: Run `cargo test --test rag_mineru`** and expect missing handler.
- [x] **Step 3: Implement checkpoints:**

```text
source_stored -> submitting -> batch_created -> submitted -> polling -> artifact_downloaded
-> parsed -> chunked -> embedded -> indexed -> activated
```

Each checkpoint stores remote IDs and hashes required for idempotent continuation. Partial output never replaces the active version.
- [x] **Step 4: Run `cargo test --test rag_mineru --test tasks`** and expect pass.

**Execution record (2026-08-02):** Added a durable MinerU task state machine with content-addressed source, artifact, and parsed-AST references. Submission intent is persisted before the non-idempotent create call, the returned batch ID is persisted before upload, and unknown create or upload outcomes never trigger an unsafe duplicate submission. Polling, cancellation, retryable provider failures, archive validation, local-stage restart, chunking, embedding, indexing, and atomic activation all resume from checkpoints. Signed upload/download clients reject redirects. Provider profile revision and secret generation are pinned in each payload and validated before credential access. Verification passed 12 MinerU lifecycle tests, 24 MinerU provider tests, 33 durable-task tests, the end-to-end local RAG pipeline test, the complete 16-test architecture suite, and warning-free `cargo check -j 1 --offline --tests`.

### Task 5: Implement deterministic structure-aware chunking

**Files:** Create `rag/chunk/policy.rs`, `table.rs`, `mod.rs`, and `tests/rag_chunk.rs`.

- [x] **Step 1: Write chunk snapshots** for CJK/Latin text, headings, long paragraphs, formulas, captions, overlap, and oversized tables.
- [x] **Step 2: Run `cargo test --test rag_chunk`** and expect failure.
- [x] **Step 3: Implement policy-driven chunking.** Policies define target/max tokens, overlap, heading context, and table row windows. Stable chunk IDs derive from block content, source location, and policy version.
- [x] **Step 4: Run `cargo test --test rag_chunk`** and expect pass.

**Execution record (2026-08-02):** Added deterministic policy-versioned chunking for CJK and Latin text, heading context, overlap, long paragraphs, formulas, image captions, and bounded table row windows with repeated headers. Stable IDs include normalized content, source location, structure, and policy identity. Verification passed all 6 chunk tests and the local postprocessing pipeline test.

### Task 6: Embed chunks with resumable batches

**Files:** Create `rag/index/mod.rs`, modify `rag/tasks.rs`, and create `tests/rag_embedding.rs`.

- [x] **Step 1: Write tests** for ordering, provider limits, dimensions, partial retry, cancellation, model identity, duplicate reuse, and incomplete activation.
- [x] **Step 2: Run `cargo test --test rag_embedding`** and expect missing orchestration.
- [x] **Step 3: Implement batch persistence.** Reuse vectors only for identical provider, model, dimension, normalized text hash, and policy version. Reject response count or dimension mismatches.
- [x] **Step 4: Run `cargo test --test rag_embedding --test providers siliconflow`** and expect pass.

**Execution record (2026-08-02):** Added ordered, provider-bounded embedding batches with durable per-batch commits, restart reuse, cancellation checks, finite-f32 validation, response count/dimension enforcement, exact provider/model/dimension/text/policy reuse identity, vector links, flat-index finalization, and activation guards. Verification passed all 5 embedding tests, 13 SiliconFlow tests, 9 authoritative knowledge repository tests, and the local chunk-to-activation pipeline test.

## Handoff and evidence

This ingest plan is complete: fixture snapshots, MinerU restart evidence, deterministic chunk IDs, and embedding batch recovery tests are verified. Continue immediately with `2026-07-29-bloomery-local-rag-indexing.md`.
