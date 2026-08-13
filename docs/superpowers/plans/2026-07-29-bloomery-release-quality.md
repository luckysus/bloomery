# Bloomery Release Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a secure, performant, signed, documented, reproducible Windows public release after every product subsystem is complete.

**Architecture:** CI and local release scripts enforce architecture, tests, migrations, protocol freshness, secrets, dependencies, steel evaluations, performance, packaging, signing, and updater integrity. Release evidence is generated from a clean checkout and fresh Windows profiles.

**Tech Stack:** GitHub Actions, PowerShell, Cargo/npm, Tauri bundler/updater, cargo-audit/deny, SBOM tooling, Playwright, Windows Authenticode.

---

### Task 1: Consolidate repository quality commands

**Files:** Create `scripts/check.ps1`, `scripts/test.ps1`, `scripts/release-check.ps1`; modify package/Cargo config and contributor docs.

- [x] **Step 1: Write a script contract test** that checks exit-code propagation and rejects missing required commands.
- [x] **Step 2: Run it** and expect scripts absent.
- [x] **Step 3: Implement PowerShell entry points.** `check.ps1` runs fast boundaries/checks; `test.ps1` runs all deterministic tests; `release-check.ps1` adds E2E, performance, security, worker, package, and docs gates.
- [x] **Step 4: Run each script with one injected failure and then cleanly** to prove it fails closed.

**Execution record (2026-08-07):** Added strict PowerShell entry points, release
script contracts with an injected `npm` failure, unsigned artifact packaging,
SHA-256 manifests, local/CI release documentation, and Windows quality and
release-candidate workflows. The deterministic suite passed through the new
`test.ps1 -Offline` entry point, including 12 frontend files/43 tests, 21
architecture tests, Rust integration suites, protocol, RAG, MCP, backup,
provider, migration, permissions, steel, task, and tool tests. The first
release build compiled the Rust host successfully; NSIS bundling remains an
external-download verification item when `nsis-3.11.zip` is reachable.

**Execution record (2026-08-08):** Extended the release script contract to
inject exit code `37` into `test.ps1`, `release-check.ps1`, and
`build-release.ps1` in addition to `check.ps1`. Each entry point fails with
the named stage and does not continue. Clean verification passed with
`check.ps1 -SkipFrontendBuild -Offline`, `test.ps1 -Stage contracts`, and
`release-check.ps1 -Offline -AllowDirty`; the latter passed 43 frontend tests,
runtime boundaries, the complete Rust suite, migrations, backup/restore, and
lifecycle checks.

### Task 2: Enforce architecture and source budgets

**Files:** Expand Rust/Node architecture tests and add dependency rule fixtures.

- [x] **Step 1: Add failing assertions** for Tauri imports outside bridge/app, Rust reverse dependencies, direct provider calls, `/api/`, private hosts, secrets in SQLite types, executable domain packages, and file budgets.
- [x] **Step 2: Run architecture tests** and capture offenders.
- [x] **Step 3: Resolve every offender or add a narrow documented exception with expiry version.** Stable budgets: runtime files 500 lines, repositories 400, Tauri commands 150, React pages 300, hooks/stores 250.
- [x] **Step 4: Run source-wide boundary checks** and expect no unapproved exceptions.

**Execution record (2026-08-08):** Added the frontend page budget gate and
split Chat, Knowledge, Onboarding, and Settings state orchestration from their
rendering components. The affected pages are now 230, 237, 155, and 228 lines;
the runtime boundary suite checks 29 files. The full deterministic suite passed
43 frontend tests, the production build, Rust formatting, and all Rust tests.

**Execution record (2026-08-08):** Added source-wide Rust boundary assertions
for reverse dependencies, concrete provider construction, Web/private-cloud
paths, persisted credential values, and recursive budgets. Moved native file
dialogs behind the single frontend desktop bridge and moved SiliconFlow
embedding/rerank construction behind the provider capability wrappers. The
architecture suite passed 26 tests, frontend boundaries passed 29 files and
43 tests, and the targeted provider/RAG suites passed. Hooks/stores budgets are
now enforced at 250 lines when those directories are present.

### Task 3: Complete application security testing

**Files:** Create security integration/fuzz tests and `docs/security-model.md`.

- [ ] **Step 1: Add tests** for path/junction/device escape, archive bombs/traversal, malicious HTML/Markdown, remote resource loading, key leakage, redirect downgrade, MCP environment leakage, permission bypass, SQL/FTS injection, oversized tool/provider output, and corrupt backups.
- [ ] **Step 2: Run the suite** and preserve failures.
- [ ] **Step 3: Fix root causes** at parser, path, HTTP, permission, renderer, repository, and restore boundaries; do not add UI-only guards.
- [ ] **Step 4: Run security tests plus secret scan** using synthetic known keys; SQLite, logs, crash errors, exports, diagnostics, and process arguments must contain none.

**Execution record (2026-08-08):** Added `scripts/security-check.ps1` and
`docs/security-model.md`. The gate scans production source for private
endpoints and high-confidence credential material, runs the frontend bridge
boundary check, and runs the security-sensitive Rust binaries serially to
avoid Windows cross-binary scheduler timing interference. The gate passed
source scanning, 26 architecture tests, backup/restore, domain package,
redaction, MCP, path, permission, provider, MinerU, and tool suites. The
broader release scenarios and clean-profile evidence remain open.

**Security evidence matrix (2026-08-08):** The tables below map every Step 1
attack scenario and every Step 4 output surface to its authoritative test
file/script and the boundary hardened per Step 3 (parser / path / HTTP /
permission / renderer / repository / restore). Documentation only; acceptance
checkboxes are unchanged.

_Step 1 attack scenarios → test file/script → hardened boundary:_

| 安全场景 (Step 1) | 测试文件 / 脚本 | 加固边界 |
| --- | --- | --- |
| Path / junction / device escape | `src-tauri/tests/permission_paths.rs`, `src-tauri/tests/rag_import.rs` | path |
| Archive bombs / traversal (backup import) | `src-tauri/tests/backup.rs` | restore, path |
| Malicious HTML / Markdown rendering | `frontend/src/components/answer/__tests__/AnswerRenderer.security.test.tsx` (via `scripts/security-check.ps1`) | renderer |
| Remote resource loading (`proxyImg` scheme allow-list) | `frontend/src/components/answer/__tests__/AnswerRenderer.security.test.tsx` | renderer |
| Redirect downgrade (HTTPS→HTTP) | `src-tauri/tests/mineru_redirect.rs`, `src-tauri/tests/http_redaction.rs` | HTTP |
| Key / credential leakage | `src-tauri/tests/secret_scan.rs`, `src-tauri/tests/http_redaction.rs` | HTTP, repository, restore |
| MCP environment / transport leakage | `src-tauri/tests/mcp_transports.rs`, `src-tauri/tests/mcp.rs` | permission, HTTP |
| Permission bypass (commands / repository) | `src-tauri/tests/permissions.rs`, `src-tauri/tests/permission_repository.rs`, `src-tauri/tests/domain_commands.rs` | permission, repository |
| SQL / FTS injection | `src-tauri/tests/rag_fts.rs`, `src-tauri/tests/rag_repository.rs` | repository, parser |
| Oversized / malformed tool / provider output | `src-tauri/tests/tools.rs`, `src-tauri/tests/providers.rs`, `src-tauri/tests/rag_parse.rs` | parser, HTTP |
| Corrupt backups / restore | `src-tauri/tests/backup.rs` | restore |

_Step 4 output surfaces (synthetic key must be absent everywhere) → evidence:_

| 输出面 (Step 4) | 测试断言 | 加固边界 |
| --- | --- | --- |
| SQLite database file | `secret_scan.rs::synthetic_secret_absent_from_sqlite_and_backup_export` | repository |
| Logs (redacted log lines) | `secret_scan.rs::synthetic_secret_absent_from_provider_failure_and_logs` | HTTP, repository |
| Crash / panic messages (incl. backtrace branch) | `secret_scan.rs::synthetic_secret_never_appears_in_panic_messages`, `secret_scan.rs::synthetic_secret_absent_from_panic_backtrace_branch` | renderer (diagnostics redaction) |
| Backup export archive | `secret_scan.rs::synthetic_secret_absent_from_sqlite_and_backup_export` | restore |
| Diagnostics health / error JSON | `secret_scan.rs::synthetic_secret_absent_from_diagnostics_json` | HTTP |
| Process arguments / environment | `secret_scan.rs::synthetic_secret_absent_from_process_arguments` | permission |

The panic diagnostics path keeps redaction mandatory: `format_panic` always
redacts the panic message, and the optional `std::backtrace::Backtrace` frames
(enabled only under `cfg!(debug_assertions)` or `BLOOMERY_PANIC_BACKTRACE`) are
also passed through the global `Redactor`, verified by the backtrace-branch
assertion above.

### Task 4: Lock dependencies, licenses, and SBOM

**Files:** Create `deny.toml`, dependency policy, notice generator, SBOM scripts; modify licenses/notices.

- [ ] **Step 1: Run baseline license/vulnerability scans** for Rust, npm, Python worker, domain assets, and copied code ledger.
- [ ] **Step 2: Resolve unknown, incompatible, unmaintained, vulnerable, or duplicate-heavy dependencies.** Exceptions require exact advisory/license, reason, owner, and review date.
- [ ] **Step 3: Migrate Bloomery original code from current MIT to approved Apache-2.0** only after verifying sole-owner authority; preserve MIT/Apache third-party notices and existing contributor rights.
- [ ] **Step 4: Generate CycloneDX/SPDX SBOMs and THIRD_PARTY_NOTICES** from locked manifests; verify release artifacts contain them.

**Execution record (2026-08-09):** Rust baseline scans passed with
`cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`) and
`cargo audit` (`vulnerabilities = 0`; 17 unmaintained and 2 informational
unsound warnings are documented). The npm baseline initially found one low
severity transitive `@babel/core` advisory; `npm audit fix
--package-lock-only` updated the lockfile and `npm audit` now reports zero
vulnerabilities. `src-tauri/deny.toml`, `about.toml`,
`THIRD_PARTY_NOTICES.hbs`, `NOTICE`, and `scripts/generate-sbom.ps1` now make
the policy and generation steps reproducible. The generator validated Rust
CycloneDX, frontend CycloneDX, frontend SPDX, and non-empty plain-text
`THIRD_PARTY_NOTICES.txt` outputs. Actual signed/unsigned installer contents
remain open until Task 7 can build a Windows release candidate; Python worker
coverage and sole-owner relicensing authority also remain open.

### Task 5: Enforce performance and resource gates

**Files:** Create startup/memory/import/agent benchmarks and `docs/benchmarks/` reports.

- [x] **Step 1: Measure baseline** on the documented Windows reference machine: cold start, idle memory, 100k chunks, 100k-row import, event throughput, and large conversation replay.
- [x] **Step 2: Add machine-readable thresholds:** cold start P95 <= 3s, idle memory target <= 300MB, local retrieval P95 <= 1s, no unconditional full index scan, and responsive UI during import.
- [x] **Step 3: Optimize measured bottlenecks only** and preserve correctness/recall checks.
- [x] **Step 4: Run each benchmark multiple times** and publish median/P95, hardware, dataset seed, software versions, and raw JSON.

**Execution record (2026-08-13):** Added deterministic release gates for cold
start, idle working set, agent event persistence, and large conversation
replay. The Windows 10 reference run passed all six measured dimensions:
startup P95 405.62 ms, idle working-set P95 28.79 MB, 100,000-chunk
retrieval P95 21.36 ms with minimum recall 1.0, 100,000-row import P95
409.06 ms with 66.68 MB peak working set, event append minimum throughput
19,558 events/s, and 10,000-message replay P95 11.43 ms with 20.49 MB peak
working set. Raw JSON is written under `src-tauri/target/` and the reproducible
reports are in `docs/benchmarks/`.

### Task 6: Test Windows data lifecycle

**Files:** Create Windows install/upgrade/uninstall scripts, migration fixtures, and release matrix docs.

- [ ] **Step 1: Build fixtures** for empty, legacy initial commit, every migration version, current, future, corrupt, and restored databases.
- [ ] **Step 2: Test install/launch/uninstall** on fresh Windows 10 and 11 profiles with Unicode usernames and non-default data paths.
- [ ] **Step 3: Test upgrade and downgrade protection.** Upgrades preserve data and credentials; uninstall preserves or explicitly offers removal; older binaries open newer DBs read-only with a clear error.
- [ ] **Step 4: Run backup/restore and index-rebuild verification** after each supported upgrade path.

### Task 7: Build and sign release artifacts

**Files:** Modify Tauri config; create `scripts/build-release.ps1`, signing/checksum scripts, artifact manifests.

- [ ] **Step 1: Produce unsigned development artifacts** for lightweight installer, Full installer with compute worker, portable package, and compute add-on.
- [ ] **Step 2: Verify clean-machine installation and contents** before signing.
- [ ] **Step 3: Integrate Authenticode/equivalent trusted signing and Tauri updater signing.** Keys remain only in approved signing infrastructure; logs never print key material.
- [ ] **Step 4: Verify signatures, SHA-256 checksums, SBOM, notices, version metadata, and reproducible manifest** for every artifact.

### Task 8: Implement safe updates

**Files:** Configure Tauri updater, create update integration tests and rollback docs.

- [ ] **Step 1: Write tests** for valid update, bad signature, wrong platform, interrupted download, proxy/offline, worker/domain compatibility, and database preflight failure.
- [ ] **Step 2: Run tests** and expect updater integration incomplete.
- [ ] **Step 3: Implement opt-in stable update checks** against a public signed release source. Download, signature/hash verify, data preflight, and explicit user installation occur before restart.
- [ ] **Step 4: Test old-to-new update on Windows 10/11** and verify data, credentials, packages, worker, and rollback guidance.

### Task 9: Build blocking CI/CD

**Files:** Create GitHub Actions workflows for check, test, security, worker, E2E, benchmark, and release.

- [ ] **Step 1: Add workflow validation** and branch/path matrix tests.
- [ ] **Step 2: Implement PR checks** for Rust/frontend/Python, protocol freshness, migrations, architecture, secrets, licenses, and security.
- [ ] **Step 3: Implement protected release workflow** requiring all gates, environment approval for signing, tag/version match, clean tree, and artifact verification before publication.
- [ ] **Step 4: Run workflows on a release candidate commit** and retain links/hashes as release evidence.

### Task 10: Complete bilingual open-source documentation

**Files:** Rewrite README and create English counterpart, architecture, protocol, security, privacy, contributing, code of conduct, setup, MCP/Skills/domain/steel guides, troubleshooting, and release docs.

- [ ] **Step 1: Add docs link/code/config validation** and UTF-8 checks.
- [ ] **Step 2: Write Chinese and English docs** with the same product promises, Windows steps, no-login/no-private-server boundary, provider ownership, SmartScreen/signing expectations, and data locations.
- [ ] **Step 3: Add contributor-ready issues/templates** with context, exact code area, tests, and acceptance criteria.
- [ ] **Step 4: Run docs validation and follow setup from a clean checkout** without relying on undocumented local state.

### Task 11: Produce the reproducible public case study

**Files:** Create `case-studies/steel-release/` fixtures, scripts, report, screenshots, and result JSON.

- [ ] **Step 1: Select redistributable steel documents/data** and record provenance/license.
- [ ] **Step 2: Script first setup, PDF import/citation, 100k retrieval, dataset profiling, model training, inference, optimization, restart recovery, and key-leak check.
- [ ] **Step 3: Run on the reference Windows machine** and publish raw inputs, configs without secrets, provider/model/version, hardware, timings, quality metrics, costs, and failures.
- [ ] **Step 4: Verify every README metric is generated by the case-study scripts** and remove unsupported marketing claims.

### Task 12: Perform the requirement-by-requirement release audit

**Files:** Create `docs/releases/1.0.0-audit.md`; update roadmap and all plan checkboxes.

- [ ] **Step 1: Map every approved design requirement and plan checkbox** to authoritative current evidence: source, test, command, artifact, screenshot, protocol, migration, benchmark, or manual Windows result.
- [ ] **Step 2: Classify each item** as proven, contradicted, incomplete, weak, or missing. Continue implementation for every item not proven.
- [ ] **Step 3: Run `scripts/release-check.ps1` from a clean checkout and fresh profile** and verify no private-server traffic.
- [ ] **Step 4: Mark Gate H complete only when every requirement is proven.** Publish no formal release while signing, evaluation, Windows, data recovery, security, documentation, or artifact evidence is incomplete.

## Completion evidence

- Clean CI and local `release-check.ps1` output.
- Security/secret/license/SBOM reports.
- Windows install/upgrade/update matrices and signed artifact verification.
- Reproducible benchmark and steel case-study outputs.
- `1.0.0-audit.md` mapping every requirement to current authoritative evidence.
