# Bloomery Agent Runtime And Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the monolithic local agent with a persistent, cancellable, evidence-aware Rust runtime and a public versioned event protocol.

**Architecture:** A session service owns conversations; each run is a persisted state machine. The runtime consumes normalized provider events and registered tools, stores ordered protocol events before publishing them, and builds bounded context from messages, summaries, memories, Skills, domain rules, and RAG evidence.

**Tech Stack:** Rust, serde tagged enums, SQLite, tokio, JSON Schema, Tauri events, generated TypeScript contracts.

---

## Target files

```text
src-tauri/src/agent/protocol/{mod,command,event,error}.rs
src-tauri/src/agent/session/{mod,model,service}.rs
src-tauri/src/agent/runtime/{mod,state_machine,loop,model_adapter}.rs
src-tauri/src/agent/context/{mod,budget,summary,memory,evidence}.rs
src-tauri/src/agent/tool_repair.rs
src-tauri/src/storage/repositories/{runs,events}.rs
src-tauri/src/storage/migrations/0008_agent_runs.sql
src-tauri/src/app/agent_commands.rs
frontend/src/bridge/generated/protocol.ts
docs/PROTOCOL.md
src-tauri/tests/agent_*.rs
```

### Task 1: Define the versioned protocol

**Files:** Create protocol modules and `tests/agent_protocol.rs`.

- [x] **Step 1: Write serialization snapshots** for run, message, tool, permission, evidence, usage, task, completion, and error events.
- [x] **Step 2: Run `cargo test --test agent_protocol`** and expect missing protocol types.
- [x] **Step 3: Implement tagged enums:**

```rust
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum AgentEventData {
    RunCreated(RunCreated), RunStateChanged(RunStateChanged),
    MessageDelta(MessageDelta), MessageCompleted(MessageCompleted),
    ToolRequested(ToolRequested), ToolProgress(ToolProgress), ToolCompleted(ToolCompleted),
    PermissionRequested(PermissionRequested), PermissionResolved(PermissionResolved),
    EvidenceAttached(EvidenceAttached), UsageUpdated(UsageUpdated),
    RunCompleted(RunCompleted), ErrorRaised(ErrorRaised),
}
```

Every envelope includes protocol version, event ID, run ID, conversation ID, sequence, and UTC timestamp.
- [x] **Step 4: Run protocol snapshots** and expect stable JSON.

**Execution record (2026-08-03):** Added a Tauri-independent protocol boundary with protocol version 1, UUID event/run/conversation identity, monotonic sequence field, UTC timestamp, structured public errors, and Serde tagged payloads for all required run, message, tool, permission, evidence, usage, task, completion, and error events. `tool_started` and `task_progress` from the approved release design are included. Exact JSON snapshots also deserialize back to the original envelope. TDD RED first failed because `bloomery::agent` did not exist; GREEN passed 5 protocol snapshot groups covering 15 event variants. Formatting and all 16 architecture tests passed, and an independent specification review reported full compliance.

### Task 2: Persist runs and ordered events

**Files:** Create `0008_agent_runs.sql`, run/event repositories, and `tests/agent_persistence.rs`.

- [x] **Step 1: Write tests** for atomic run creation, monotonic sequence allocation, duplicate event ID, replay, completion, and transaction rollback.
- [x] **Step 2: Run `cargo test --test agent_persistence`** and expect missing tables.
- [x] **Step 3: Implement repositories.** Persist state/event before publication. Use a transaction to reserve the next per-run sequence and insert the event; uniqueness is `(run_id, sequence)` and `event_id`.
- [x] **Step 4: Run persistence and migration tests** and expect pass.

**Execution record (2026-08-03):** Added ordered migration 0011 (the current schema already used the plan's former 0008 filename), workspace/conversation/message/run composite foreign keys, terminal-state checks, and replay indexes. Run creation and its sequence-1 event, terminal completion and its event, per-run sequence allocation, and caller-composed message/run creation are transactional. Generic event append cannot bypass run state operations. Replay is ordered, incremental, workspace-scoped, identity-checked, and preserves nanosecond timestamps. Tests cover duplicate IDs without sequence loss, injected rollback, conversation-delete cascading, invalid cross-workspace identities, and simultaneous append from two WAL connections. TDD RED first failed because run/event repositories were absent; the completed verification passed 11 persistence, 8 migration, 5 protocol, and 16 architecture tests, warning-free `cargo check -j 1 --offline --tests`, and formatting. Independent review findings were fixed and re-review reported no remaining issues.

### Task 3: Implement the run state machine

**Files:** Create `agent/runtime/state_machine.rs` and `tests/agent_state_machine.rs`.

- [x] **Step 1: Write transition tests** for:

```text
created -> preparing -> generating -> awaiting_permission
-> executing_tools -> verifying -> completing
-> completed | cancelled | failed | interrupted
```

Reject terminal-state exits, tool execution before permission, and completion with unresolved calls.
- [x] **Step 2: Run `cargo test --test agent_state_machine`** and expect missing machine.
- [x] **Step 3: Implement explicit transition validation** returning stable `invalid_run_transition` errors.
- [x] **Step 4: Run state tests** and expect pass.

**Execution record (2026-08-03):** Added a pure Rust state machine with explicit legal edges, immutable failures, stable `invalid_run_transition` errors, terminal-state closure, and cancellation/failure/interruption exits from every nonterminal state. Guarded permission waiting, automatic and approved tool execution, mixed permission batches, denied tools, unresolved work, and completion. TDD first failed because the machine was absent; review-driven regressions then demonstrated and closed denied-tool and mixed-batch permission bypasses. The final 12 state tests include the complete nonterminal transition matrix and every terminal exit target. Independent re-review approved the result; the final 52-test protocol/persistence/state/migration/architecture regression, warning-free `cargo check -j 1 --offline --tests`, and formatting passed.

### Task 4: Build the session service

**Files:** Create session modules, modify conversation repository, and create `tests/agent_session.rs`.

- [x] **Step 1: Write tests** for create/title/list/archive/restore, append message, edit-and-truncate, fork, draft, export snapshot, and no cross-workspace access.
- [x] **Step 2: Run `cargo test --test agent_session`** and expect missing service.
- [x] **Step 3: Implement domain-level operations.** Commands call the service; runtime uses the same service. A user message and created run are stored in one transaction.
- [x] **Step 4: Run session and repository tests** and expect pass.

**Execution record (2026-08-04):** Added a SQL-free, Tauri-independent `SessionService` for conversation lifecycle, messages/history, edit/truncate, fork, drafts, summaries, versioned export, and atomic run start. Routed the registered local-agent chat path, assistant persistence, summary loading/saving, and storage commands through the service. Canonical UUIDs now identify persisted and streamed runs; snapshot import requires an owned existing conversation and uses uniform not-found errors for missing and foreign IDs. User message, run, and first event are written in one `IMMEDIATE` transaction, with rollback and production-helper coverage. Truncation removes affected user runs/events, summary coverage is workspace/conversation validated, and equal timestamp boundaries use `rowid`. TDD RED captured the missing-import and runtime-bypass regressions before implementation. Main-process verification passed 13 session, 18 architecture, 16 local-agent unit, and 73 combined protocol/persistence/state/session/architecture/migration/repository tests; `cargo check -j 1 --offline --tests` and `cargo fmt --all -- --check` also passed. Final quality review reported no Critical or Important issues. A future snapshot-import implementation must explicitly reject unsupported `format_version` values.

### Task 5: Build deterministic context budgeting

**Files:** Create context modules and `tests/agent_context.rs`.

- [x] **Step 1: Write budget tests** for CJK/Latin token estimates, system rules, current request, recent messages, evidence, memories, summary, truncation, and provider context limits.
- [x] **Step 2: Run `cargo test --test agent_context`** and expect missing budgeter.
- [x] **Step 3: Implement priority allocation:**

```text
security/system/domain -> current request -> necessary recent turns
-> tool/evidence -> explicit memories -> historical summary
```

Return a `ContextReport` with included IDs, omitted IDs, token estimates, truncations, and model limit. Do not silently truncate system or permission rules.
- [x] **Step 4: Run context tests** and compare deterministic reports.

**Execution record (2026-08-04):** Added a pure Rust, SQL-free and Tauri-independent deterministic context budgeter. It reserves provider output tokens, defaults unknown model limits to 8,192, rejects invalid reservations, requires exactly one nonempty current request, preserves security/system/domain/permission rules without truncation, and returns typed overflow and duplicate-input errors. Allocation follows required rules, current request, contiguous atomic recent turns, evidence, explicit memory, and historical summary. Reports include model/input limits, selected and omitted IDs, per-item estimates, included content, and UTF-8-safe truncation records. The conservative estimator counts ASCII word runs at four characters per token and every other character, including whitespace, at one token; optional truncation shares the same single-pass linear counter. TDD RED covered the absent context boundary, missing permission source, invalid provider reservation, current-request validation, atomic recent turns, the 400-line architecture budget, and whitespace undercounting. Review findings were fixed and both specification and quality re-reviews reported no Critical or Important issues. Final verification passed 14 context tests, 19 architecture tests, the 88-test Task 1-5 combined regression, `cargo check -j 1 --offline --tests`, and `cargo fmt --all -- --check`. The report intentionally preserves allocation order; Task 8 must restore selected recent turns to chronological order when rendering provider messages.

### Task 6: Implement summaries and user-visible memory

**Files:** Create `context/summary.rs`, `context/memory.rs`, modify memory repositories, create `tests/agent_memory.rs`.

- [x] **Step 1: Write tests** for summary coverage, source preservation, memory candidate extraction, duplicate rejection, confirmation policy, edit, disable, archive, and delete.
- [x] **Step 2: Run `cargo test --test agent_memory`** and expect failure.
- [x] **Step 3: Implement explicit memory candidates.** Store source message/run, confidence, status, and normalized dedup key. Automatic writes are allowed only under an explicit user preference; all records remain visible and removable.
- [x] **Step 4: Run memory, summary, and context tests** and expect pass.

**Execution record (2026-08-05):** Added deterministic summary planning and steel-domain summary prompts, preserved summary coverage message IDs through SQLite, session snapshots, and migration 13 compatibility backfill, and added explicit memory candidates with source message/run provenance, confidence bounds, normalized deduplication, pending/confirmed/rejected lifecycle, explicit opt-in automatic confirmation, workspace validation, and visible removable records. Registered confirmation, rejection, enable/disable, archive/restore, delete, listing, search, and suggestion commands through the local Tauri boundary. Qoder's follow-up fixes added ASCII word-boundary matching for English markers, UTF-8-safe marker offsets, v1 snapshot deserialization defaults, and protection against enabling pending or rejected candidates. Fresh verification passed 20 memory, 13 session, and 9 migration tests; protocol, persistence, state-machine, context, repository, and architecture targets passed 5, 11, 12, 14, 6, and 20 tests respectively; 16 local-agent unit tests passed, `cargo check -j 1 --offline --tests` passed, and `cargo fmt --all -- --check` passed.

### Task 7: Implement bounded Tool-Call Repair

**Files:** Create `agent/tool_repair.rs` and `tests/agent_tool_repair.rs`.

- [x] **Step 1: Write tests** for clean JSON, fenced JSON, trailing comma, escaped content, wrong types, missing required fields, unknown tools, path/command ambiguity, and retry exhaustion.
- [x] **Step 2: Run `cargo test --test agent_tool_repair`** and expect missing repair layer.
- [x] **Step 3: Implement the repair pipeline:** strict parse, unambiguous syntax normalization, Schema validation, exact validation feedback, maximum two model retries. Never infer paths, commands, destructive booleans, or permission intent.
- [x] **Step 4: Run repair tests** and expect pass.

**Execution record (2026-08-05):** Added a Tauri-independent Tool-Call Repair boundary backed only by the registered tool ID, input JSON Schema, and registry-owned risk level. It accepts clean JSON, complete JSON fences, and trailing commas outside strings; preserves escaped argument content; parses encoded argument objects; rejects surrounding prose, conflicting envelopes, unknown or duplicate tools, and model-supplied permission intent. The validator handles recursive object/array/string constraints, required fields, closed object schemas, enum constraints, JSON type alternatives, and safe canonical JSON values without coercing path, command, or destructive boolean inputs. Structured feedback exposes stable code, message, and argument path; retries are bounded to two model callbacks and surface the last validation error. TDD RED first failed because the module was absent, then a numeric JSON Schema regression first failed because `number` rejected integer JSON values before the shared matcher was corrected. Final verification passed 12 repair tests, `cargo check -j 1 --offline --tests`, 20 architecture tests, and `cargo fmt --all -- --check`.

### Task 8: Implement the provider-normalized Agent loop

**Files:** Create `runtime/model_adapter.rs`, `runtime/loop.rs`, `runtime/mod.rs`, and `tests/agent_loop.rs`.

- [ ] **Step 1: Write scripted-provider tests** for direct answer, RAG answer, one/multiple tools, parallel read tools, serial write tools, repair, provider error, cancellation, and context overflow.
- [ ] **Step 2: Run `cargo test --test agent_loop`** and expect missing runtime.
- [ ] **Step 3: Implement the loop.** Ask `ProviderCapabilities`; stream normalized deltas; persist events before publish; execute only registered/authorized tools; append observations with output bounds; restore selected recent turns to chronological provider-message order; verify citations; complete exactly once.
- [ ] **Step 4: Run loop, protocol, context, and provider tests** and expect pass.

### Task 9: Recover interrupted runs and replay UI state

**Files:** Modify runtime, add `app/agent_commands.rs`, create `tests/agent_recovery.rs`.

- [ ] **Step 1: Write tests** for shutdown during generation/tool/permission, replay after lost UI events, duplicate command submission, cancel, retry from user message, and stale run cleanup.
- [ ] **Step 2: Run `cargo test --test agent_recovery`** and expect failure.
- [ ] **Step 3: Implement recovery semantics.** Generating runs become interrupted and can regenerate; idempotent tool calls can resume from checkpoints; unresolved dangerous calls return to permission; terminal runs only replay.
- [ ] **Step 4: Run recovery and task scheduler tests** and expect pass.

### Task 10: Generate TypeScript contracts and document the protocol

**Files:** Create protocol exporter, `frontend/src/bridge/generated/protocol.ts`, `docs/PROTOCOL.md`, and contract tests.

- [ ] **Step 1: Add a failing freshness test** comparing generated output with tracked TypeScript and protocol schema files.
- [ ] **Step 2: Run the freshness test** and expect generated artifacts absent.
- [ ] **Step 3: Implement deterministic export.** Document compatibility, envelope, event ordering, replay, cancellation, permissions, errors, examples, and versioning. Generated files include a do-not-edit header.
- [ ] **Step 4: Run exporter, TypeScript build, and protocol tests** and expect pass.

### Task 11: Remove the monolithic runtime

**Files:** Remove `src-tauri/src/local_agent.rs`; modify composition and old context/retrieval modules.

- [ ] **Step 1: Tighten architecture budgets** to 500 lines per runtime file, 400 per repository, and 150 for Tauri command modules; reject legacy module names.
- [ ] **Step 2: Run architecture tests** and expect `local_agent.rs` failure.
- [ ] **Step 3: Migrate remaining summary, prompt, cancellation, and response behavior into focused modules, then delete `local_agent.rs`.** Delete compatibility commands after frontend bridge migration.
- [ ] **Step 4: Run Gate D verification:** all agent tests, all Rust tests, `cargo check`, protocol freshness, frontend boundaries/build, event replay smoke test, and restart smoke test.

## Completion evidence

- Public `PROTOCOL.md` and matching generated TypeScript.
- Deterministic protocol, state, context, memory, repair, loop, and recovery tests.
- No `local_agent.rs`, no Tauri dependency in Agent domain modules, and enforced file budgets.
- Desktop replay evidence showing persisted events reconstruct an interrupted run.
