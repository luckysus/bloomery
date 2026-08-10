# Bloomery Release Implementation Roadmap

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the complete public Bloomery desktop release defined by the approved release design, with no Web dependency and no reduced MVP release boundary.

**Architecture:** Preserve the Tauri/React/SQLite desktop shell while replacing the Web-derived behavior with a modular Rust core. SQLite remains authoritative, external providers are called directly with OS-protected credentials, and executable extensions pass through versioned protocols and explicit permissions.

**Tech Stack:** Tauri 2, Rust 2021, React 18, TypeScript, Vite, SQLite/FTS5, HNSW, reqwest/rustls, Windows Credential Manager, MCP, Python 3.12 compute worker.

---

## Execution rules

- This roadmap and all linked subsystem plans together define the first public release. No subsystem is optional.
- Milestones are dependency checkpoints, not Alpha, Beta, MVP, Community, or Pro feature cuts.
- Use PowerShell on Windows 10. Use repository package scripts rather than raw Vite commands.
- Follow TDD for behavior changes: failing test, focused implementation, passing test, relevant regression suite.
- Do not commit, deploy, delete application data, or alter server resources without separate explicit authorization.
- Preserve the existing worktree. Never clean or revert changes outside the active task.
- At the end of each task, update its checkboxes and the coverage matrix in this roadmap.

## Plan set

| Order | Plan | Scope | Depends on |
| --- | --- | --- | --- |
| 1 | `2026-07-29-bloomery-foundation-decoupling.md` | Architecture tests, login/Web removal, local app identity, module shell | approved design |
| 2 | `2026-07-29-bloomery-storage-providers.md` | Migrations, repositories, OS secrets, providers, durable tasks | 1 |
| 3 | `2026-07-29-bloomery-local-rag.md` | Ingest, parse, chunk, FTS5, vectors, fusion, rerank, citations | 2 |
| 4 | `2026-07-29-bloomery-agent-protocol.md` | Modular agent loop, context, memory, repair, public protocol | 2, 3 |
| 5 | `2026-07-29-bloomery-extensions-security.md` | Tools, permissions, MCP transports, Claude-compatible Skills, domain runtime | 4 |
| 6 | `2026-07-29-bloomery-steel-compute.md` | Official steel package, datasets, ONNX, training and optimization worker | 3, 4, 5 |
| 7 | `2026-07-29-bloomery-desktop-product.md` | Onboarding, workbench, chat, knowledge, analysis, extension and diagnostics UI | 2-6 |
| 8 | `2026-07-29-bloomery-release-quality.md` | Security, performance, E2E, packaging, signing, updater, docs and case study | 1-7 |

## Cross-plan architecture map

```text
frontend/src/app
  -> frontend/src/bridge
    -> src-tauri/src/app
      -> src-tauri/src/agent/protocol
        -> agent / rag / tools / domains
          -> providers / mcp / compute
            -> storage / tasks / diagnostics
```

The React side consumes a generated TypeScript protocol contract. Tauri adapters are the only code allowed to invoke Rust commands or listen for Rust events. Rust domain modules do not depend on Tauri and can be tested without a desktop runtime.

## Milestone gates

### Gate A: independent local shell

- [x] No login, profile, admin, captcha, cloud task, or Web API route remains reachable.
- [x] A new local installation creates a singleton local workspace and opens the workbench.
- [x] Architecture tests reject `/api/`, `cloud_api_base`, auth imports, and Web OpenAPI types.
- [x] `cargo test`, `cargo check`, and `npm run build` pass.

Evidence (2026-07-29): production-only scans found no private route/auth/cloud/server symbols; 35 Rust library tests and 7 architecture tests passed; TypeScript/Vite produced a 148.87 kB JS and 27.73 kB CSS build; a Tauri smoke launch created the `Bloomery` window and local SQLite database. Screenshot capture was not used because Windows app approval timed out.

### Gate B: durable local platform

- [x] Ordered SQLite migrations replace monolithic create-if-not-exists initialization.
- [x] API secrets are stored in Windows Credential Manager and never returned to React.
- [x] LLM, SiliconFlow, MinerU, and Ollama profiles expose normalized capability checks.
- [x] Background tasks resume after restart and use persisted checkpoints.

Evidence (2026-08-02): ordered transactional migrations 0001-0004, workspace-scoped repositories, Windows Credential Manager commands and explicit disposable smoke, normalized provider capability contracts, MinerU/SiliconFlow/OpenAI/Ollama tests, restart-safe background tasks, provider/task/health command DTOs, and secret-response redaction are complete. Gate B verification passed the full offline Rust suite (52 library, 13 architecture, 11 HTTP-redaction, 8 migration, 49 provider, 6 repository, 2 secret, 33 task, and doc tests), warning-free `cargo check -j 1 --offline`, formatting, and the 1,580-module frontend production build.

### Gate C: evidence-grounded local RAG

- [x] Supported documents import, deduplicate, parse, chunk, embed, and index locally.
- [x] BM25 and dense candidates merge through RRF and optionally rerank.
- [x] Citation records resolve to page, sheet, heading, table, image, or source text.
- [x] Index corruption, model change, cancellation, and restart all recover safely.

Evidence (2026-08-03): content-addressed PDF/Markdown/TXT/HTML/DOCX/CSV/XLSX ingest, deduplication, normalized parsing, restart-safe MinerU tasks, deterministic chunking, resumable SiliconFlow embedding, FTS5 BM25, versioned HNSW with authoritative Flat fallback, deterministic RRF, optional bounded reranking, immutable evidence audits, and page/sheet/heading/table/image/text citation resolution are complete. Index health distinguishes corruption, watermark/model drift, interrupted builds, low disk, cancellation, and task failure without deleting the last validated generation. The full offline Rust test suite passed, including migrations, provider mocks, task recovery, PDF-to-page citation and SQLite/HNSW restart smoke; `cargo check -j 1 --offline --tests --benches`, formatting, frontend runtime boundaries, and the production build passed. The reproducible 100,000-chunk benchmark reached 1.00 minimum recall and 30.17 ms total local P95 against gates of 0.95 and 1,000 ms.

### Gate D: modular agent runtime

- [x] The current monolithic local agent is removed after behavior migration.
- [x] Runs persist state and ordered protocol events before UI delivery.
- [x] Context budgeting, memory, tool repair, cancellation, and recovery pass tests.
- [x] `PROTOCOL.md` and generated TypeScript types match Rust serialization.

Evidence (2026-08-10): the modular Agent runtime, persisted event sink,
bounded context and memory lifecycle, Tool-Call Repair, cancellation/recovery,
protocol exporter, generated TypeScript contracts, and the removal of
`local_agent.rs` are covered by the current Rust suite. Full offline Rust,
protocol freshness, frontend runtime boundaries, and production build checks
passed on the current `main` worktree.

### Gate E: controlled extension ecosystem

- [ ] Built-in and MCP tools share typed schemas and permission enforcement.
- [x] stdio, Streamable HTTP, and legacy SSE MCP transports pass contract tests.
- [x] Claude-compatible Skills load from user, workspace, and domain scopes.
- [ ] Signed official and clearly marked unsigned third-party domain packages install safely.

### Gate F: complete steel workbench

- [ ] Official steel terminology, standards, retrieval presets, calculations, and evaluations ship.
- [x] CSV/XLSX production datasets support mapping, validation, profiling, and analysis.
- [x] ONNX inference records model, inputs, ranges, constraints, and confidence metadata.
- [x] The supervised local compute worker performs training and constrained optimization without private cloud access.

Evidence (2026-08-10): the current Rust suite covers CSV/XLSX preview and
mapping, activation, profiling, correlations, outlier evidence, and
workspace-scoped dataset persistence. Linear regression training and
artifact-backed inference now run through the local Python Worker with
persisted, restart-safe tasks; supported model families, ONNX lifecycle,
constrained optimization, and packaged-worker release evidence remain open.

Evidence (2026-08-10): the ONNX inference loop is closed end to end. The
Worker validates model hash, opset window 7-21, an explicit operator
whitelist, and I/O schemas, runs chunked batch inference with staged
progress, and records applicability warnings plus manifest-declared
applicability-distance confidence. Rust persists the task, pins the model
hash through `hash_onnx_model_file`, and enforces the result contract; the
analysis workbench adds model selection, manifest editing, task lifecycle,
and result display. Constrained optimization and packaged-worker release
evidence remain open.

Evidence (2026-08-10): constrained process optimization is closed end to
end. The Worker runs constraint-aware TPE (single-objective) and NSGA-II
(multi-objective) with deterministic seeds, cooperative cancellation, fixed
values, deterministic equality projection, and re-evaluation of every
recommendation through the active model and hard constraints; infeasible
problems are rejected with violation details. Rust registers
`compute_optimize_constrained`, enforces the recommendation contract, and
passes the end-to-end scheduler test. The analysis workbench exposes
direction, objectives, bounds, fixed values, linear constraints, and
recommendation display. Worker suite passes 45 tests, the Rust offline
suite passes, and the frontend suite passes 81 tests. Steel terminology,
evaluations, the Agent optimization tool adapter, and packaged-worker
release evidence remain open.

Evidence (2026-08-10): the steel terminology source now ships versioned
and license-audited: 32 authored terms covering grades, composition
elements, properties, defects, standards, and all five main process stages,
with Chinese and English aliases, unit declarations, disambiguation rules,
a standard source ledger that never redistributes restricted text, SHA-256
pinned assets, and six enforcing Rust tests. Full calculator coverage and
versioned evaluations remain open for Gate F item one.

Evidence (2026-08-10): the Agent now exposes constrained optimization
through `steel.optimize_constrained` (confirmation-required submission) and
`steel.optimization_status` tools wired to the workspace task database via
the `OptimizationGateway` trait, with argument validation, cancellation
handling, typed error surfacing, allowlist exposure in the steel manifest,
and eight new integration tests passing in the full offline Rust suite.

Evidence (2026-08-10): the versioned steel evaluation suite
`steel-evaluations-v1.json` is pinned in the manifest and executed by both
the Rust runner (calculators, dataset mapping, dataset profiling,
terminology) and the Worker suite (inference vectors, training
reproducibility, optimization feasibility), each category against its
recorded threshold with verbatim failure recording; provider categories
keep provider/model/run_at fields. Gate F item one now waits only on the
full Gate F verification sweep (signature, 100k-row import, ONNX parity,
packaged evidence).

### Gate G: finished desktop product

- [ ] First-run setup, workbench, chat, knowledge, analysis, extensions, settings, and diagnostics are complete.
- [ ] Permission requests, task recovery, provider degradation, citations, and errors have dedicated UI states.
- [ ] Conversation and full backup export/import work without exporting secrets.
- [ ] Windows desktop layouts pass automated and visual checks at supported window sizes.

### Gate H: public release

- [ ] Windows 10 and 11 install, upgrade, uninstall, and data-preservation tests pass.
- [ ] Security, dependency, secret, protocol, migration, performance, and steel evaluation gates pass.
- [ ] Signed installers, portable package, updater metadata, SBOM, notices, and checksums exist.
- [ ] Chinese and English docs, Non-goals, security policy, contributor guide, extension guides, demo, and reproducible case study exist.

## Required verification commands

Run from `src-tauri/`:

```powershell
cargo test
cargo check
```

Run from `frontend/`:

```powershell
npm run build
```

Additional plans add focused test commands. Gate H must run all commands from a clean checkout and a new Windows user profile.

## Completion audit

The release is complete only when every checkbox in all eight subsystem plans and every gate above is checked with current evidence. A passing narrow test cannot substitute for a broader gate. Missing signing credentials, unavailable evaluation data, unsupported Windows behavior, or absent documentation keeps the release incomplete rather than redefining the release scope.
