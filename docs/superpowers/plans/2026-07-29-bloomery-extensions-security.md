# Bloomery Extensions Permissions And Security Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a typed tool ecosystem with enforceable permissions, MCP transports, Bloomery Skills, and safe declaration-only domain packages.

**Architecture:** Built-in and MCP tools normalize into one registry and pass through one permission engine. Skills contribute instructions only. Domain packages are validated manifests and resources; executable behavior is available solely through registered MCP tools or built-ins.

**Tech Stack:** Rust, JSON Schema, official Rust MCP SDK, stdio, Streamable HTTP, legacy SSE, SHA-256/signatures, Windows path APIs, SQLite.

---

## Target files

```text
src-tauri/src/tools/{mod,definition,registry,executor,output}.rs
src-tauri/src/permissions/{mod,model,policy,path}.rs
src-tauri/src/mcp/{mod,config,client,stdio,http,sse,supervisor}.rs
src-tauri/src/skills/{mod,discover,parse,merge}.rs
src-tauri/src/domains/{mod,manifest,installer,loader,signature}.rs
src-tauri/src/storage/migrations/0009_extensions.sql
src-tauri/src/app/{tool,mcp,skill,domain}_commands.rs
src-tauri/tests/{tools,permissions,mcp,skills,domains}.rs
docs/extensions/{mcp,skills,domain-packages}.md
```

### Task 1: Create the common tool definition and registry

**Files:** Create tool modules and `tests/tools.rs`.

- [x] **Step 1: Write registry tests** for stable ID/version, duplicate IDs, invalid schemas, enable/disable, domain filtering, source attribution, and deterministic ordering.
- [x] **Step 2: Run `cargo test --test tools registry`** and confirm the expected missing-module failure.
- [x] **Step 3: Implement:**

```rust
pub struct ToolDefinition {
    pub id: ToolId, pub version: Version, pub name: String,
    pub input_schema: Value, pub output_schema: Value,
    pub risk: RiskLevel, pub read_only: bool,
    pub concurrency: ConcurrencyPolicy, pub timeout: Duration,
    pub source: ToolSource,
}
```

The registry accepts only validated definitions and exposes immutable snapshots to each run.
- [x] **Step 4: Run registry and agent-loop tests** and confirm pass.

**Execution record (2026-08-05):** Added `tools::ToolDefinition`, strict stable `ToolId` and `ToolVersion` values, built-in/MCP/domain source attribution, JSON Schema shape validation, enabled-state management, global/domain filtering, and deterministic immutable snapshots. The registry contract suite passed 7 tests, the Agent loop regression passed 13 tests, the architecture suite passed 21 tests, `cargo check --offline -j 1` passed, and `cargo fmt --all -- --check` passed. Existing warnings for `ChatPreparation.packet` and `summary_result` remain unrelated to this task.

### Task 2: Execute tools with bounds and cancellation

**Files:** Create `tools/executor.rs`, `output.rs`; extend `tests/tools.rs`.

- [x] **Step 1: Write tests** for timeout, cancellation, large output, structured errors, read parallelism, write serialization, panic isolation, and artifact storage.
- [x] **Step 2: Run the focused executor tests and confirm the expected missing-type failure.**
- [x] **Step 3: Implement executor rules.** Store oversized complete results as local artifacts and return bounded model summaries. Convert handler failures into stable tool errors and never unwind through the runtime.
- [x] **Step 4: Run tool and agent-loop tests** and confirm pass.

**Execution record (2026-08-05):** Added the async tool executor with stable structured errors, timeout and cancellation races, panic isolation, a shared serial gate for writes/exclusive tools, parallel read execution, and SHA-256-addressed local artifact files for oversized JSON results. The complete tool suite passed 14 tests, the Agent loop regression passed 13 tests, the architecture suite passed 21 tests, `cargo check --offline -j 1` passed, and formatting passed. Existing unrelated dead-code warnings remain.

### Task 3: Implement permission policy and durable rules

**Files:** Create permission modules, `0009_extensions.sql`, and `tests/permissions.rs`.

- [x] **Step 1: Write tests** for automatic, confirm, dangerous-disabled, once, session, always, deny, revoked rules, tool-version change, source change, and parameter scopes.
- [x] **Step 2: Run the focused policy tests and confirm the expected missing-engine failure.**
- [x] **Step 3: Implement decisions:**

```rust
pub enum PermissionDecision {
    AllowAutomatic, RequireConfirmation(PermissionRequest), Deny(DenialReason),
}
```

Persist permanent rules by stable tool ID, version range, source identity, action, and normalized scope. Display names never grant authority.
- [x] **Step 4: Run permission and agent-state tests** and confirm pass.

**Execution record (2026-08-05):** Added the Tauri-independent permission policy with automatic read-only allowance, confirmation requests, dangerous-by-default denial, once/session/always approval, deny and revoke behavior, exact tool version/source matching, normalized parameter scopes, and persistent-rule restoration. Added migration 0014 and a workspace-scoped SQLite repository with active-rule filtering and revocation timestamps. Verification passed 10 policy tests, 2 repository tests, 9 migration tests, 14 tool tests, 13 Agent loop tests, `cargo check --offline -j 1`, and formatting. Existing unrelated dead-code warnings remain.

### Task 4: Enforce Windows path authorization

**Files:** Create `permissions/path.rs` and `tests/permission_paths.rs`.

- [x] **Step 1: Write Windows tests** for relative paths, `..`, UNC, device paths, alternate streams, symlink/junction escape, case folding, nonexistent targets, and post-open target verification.
- [x] **Step 2: Run `cargo test --test permission_paths`** and expect failure.
- [x] **Step 3: Implement authorized-root handles.** Canonicalize existing ancestors, reject device namespaces, verify resolved targets remain under granted roots, and re-check opened handles before effects.
- [x] **Step 4: Run path tests on Windows** and expect pass.

**Execution record (2026-08-05):** Added strict absolute-path validation, explicit UNC/device/alternate-stream rejection, canonicalization of existing ancestors with missing-tail support, case-insensitive root containment, symlink/reparse-point escape checks, and Windows `GetFinalPathNameByHandleW` verification for opened files. The Windows path suite passed 10 tests; the related permission, tool, Agent loop, architecture, migration, and library suites passed 46, 10, 14, 13, 21, 9, and 46 tests respectively. `cargo fmt --all -- --check` passed. A full unfiltered `cargo test` run exceeded five minutes and was split into bounded groups; no individual failure was observed.

### Task 5: Integrate MCP through the standard SDK

**Files:** Create MCP model/client modules, modify `Cargo.toml`, and create `tests/mcp.rs`.

- [x] **Step 1: Write common MCP tests** for initialize, capability discovery, tools/resources/prompts, schema conversion, call result, timeout, cancellation, protocol error, and server version change.
- [x] **Step 2: Run `cargo test --test mcp client`** and expect missing client.
- [x] **Step 3: Add the maintained Rust MCP SDK and a thin Bloomery adapter.** Convert discovered tools into `ToolDefinition`; keep SDK types out of Agent and UI contracts.
- [x] **Step 4: Run MCP client and tool registry tests** and expect pass.

**Execution record (2026-08-05):** Added the official `rmcp` 3.1.1 client SDK with Bloomery-owned configuration, server identity, capability, tool/resource/prompt, and call-result models. The adapter performs initialization, bounded requests, Agent cancellation polling, stable protocol errors, server-version fencing, and MCP-to-`ToolDefinition` conversion with confirmation-by-default permissions. The MCP contract suite passed 8 tests; the tool registry and Agent loop suites passed 14 and 13 tests; formatting passed.

### Task 6: Support stdio, Streamable HTTP, and legacy SSE

**Files:** Create `mcp/stdio.rs`, `http.rs`, `sse.rs`, `supervisor.rs`; extend `tests/mcp.rs`.

- [x] **Step 1: Write transport tests** using disposable local fixtures: process startup/exit, bounded stderr, environment allowlist, reconnect, HTTP auth injection, SSE resume, malformed frames, and shutdown.
- [x] **Step 2: Run `cargo test --test mcp transports`** and expect failure.
- [x] **Step 3: Implement transports.** Stdio receives only explicitly configured environment variables. HTTP/SSE credentials resolve inside Rust. Supervisor applies bounded restart backoff and requires manual action after repeated permanent failures.
- [x] **Step 4: Run all MCP tests** and expect pass.

**Execution record (2026-08-09):** Added the legacy SSE transport as a separate
`McpTransportConfig::LegacySse` branch. The adapter discovers and validates the
same-origin message endpoint, keeps bearer authentication inside Rust, bounds
event size, parses UTF-8 frames, reconnects with `Last-Event-ID`, honors bounded
retry intervals, and stops on malformed or permanently unauthorized streams.
Migration 0018 expands the persisted transport constraint without rewriting
the already-published migration 0017. Local integration fixtures covered
initialization, tool discovery, authentication, resume, and shutdown; the
transport suite passed 10 tests and the dedicated SSE suite passed 2 tests.

### Task 7: Load Bloomery Skills

**Files:** Create skill modules and `tests/skills.rs`.

- [x] **Step 1: Write tests** for `~/.bloomery/skills/<name>/SKILL.md`, frontmatter, UTF-8, malformed files, duplicate names, compatibility, and deterministic discovery.
- [x] **Step 2: Run `cargo test --test skills`** and expect missing loader.
- [x] **Step 3: Implement read-only Skill records.** Skills add instructions and metadata; they never grant file, Shell, network, MCP, or secret access. Record the exact enabled Skill versions in each run.
- [x] **Step 4: Run Skills and context tests** and expect pass.

**Execution record (2026-08-09):** Bloomery Skill discovery, strict
frontmatter parsing, UTF-8 and size bounds, user/workspace/domain precedence,
compatibility filtering, deterministic merge, bounded prompt rendering, and
content-fingerprint summaries are implemented. Skill bodies remain outside the
frontend contract and never grant capabilities. The focused Skills suite passed
7 tests, and the runtime prompt/context regressions passed in the existing
Agent suite. Added the bilingual public guide at `docs/extensions/skills.md`.

### Task 8: Define the domain package manifest

**Files:** Create `domains/manifest.rs`, `loader.rs`, and `tests/domains.rs`.

- [ ] **Step 1: Write manifest tests** for ID/version, app compatibility, license, prompts, terminology, retrieval policy, built-in tool allowlist, MCP recommendations, data mappings, evaluations, assets, unknown fields, and path traversal.
- [ ] **Step 2: Run `cargo test --test domains manifest`** and expect failure.
- [ ] **Step 3: Implement a strict declaration-only manifest.** Reject executables and script entry points. Resolve resources within the package root and cap file count, individual size, and expanded archive size.
- [ ] **Step 4: Run domain manifest tests** and expect pass.

### Task 9: Install, verify, activate, and roll back domain packages

**Files:** Create `domains/installer.rs`, `signature.rs`; extend migration/repository and `tests/domains.rs`.

- [ ] **Step 1: Write tests** for signed official, unsigned third-party, bad signature, hash mismatch, incompatible version, upgrade, active rollback, interrupted install, and uninstall impact preview.
- [ ] **Step 2: Run `cargo test --test domains installer`** and expect failure.
- [ ] **Step 3: Implement staged install.** Expand to a temporary directory, validate every file and manifest, verify signature/hash, write package record, then atomically activate. Preserve the previous validated version until activation succeeds.
- [ ] **Step 4: Run installer tests** and expect pass.

### Task 10: Expose extension commands and public guides

**Files:** Create app command modules and extension docs; modify command registration.

- [ ] **Step 1: Write command contract tests** for MCP CRUD/test/restart, Skills discovery/enable, domain install/activate/remove, tool listing, permission resolve/list/revoke, and impact previews.
- [ ] **Step 2: Run focused command tests** and expect missing commands.
- [ ] **Step 3: Implement thin commands and docs.** Documentation includes schemas, examples, permissions, trust indicators, signing, transport configuration, and the Bloomery Skills directory.
- [ ] **Step 4: Run Gate E verification:** tools, permissions, Windows paths, all MCP transports, Skills, domain packages, Agent integration, docs examples, `cargo check`, and frontend build.

## Completion evidence

- Tool and permission tests proving every execution passes the same policy engine.
- Windows path escape tests.
- MCP contract outputs for all three transports.
- Bloomery Skill fixtures and deterministic discovery reports.
- Signed/unsigned package install, rollback, and traversal-defense evidence.
