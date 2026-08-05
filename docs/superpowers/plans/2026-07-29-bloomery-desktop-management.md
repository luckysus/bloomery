# Bloomery Desktop Management And Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Bloomery desktop with knowledge, production analysis, extensions, settings, diagnostics, export, backup, restore, accessibility, and responsive verification.

**Architecture:** Every long operation is task-backed and restart-safe. Management features consume typed bridge commands, show impact previews for destructive actions, and never expose secret values.

**Tech Stack:** React, TypeScript, Recharts, pdfjs-dist, Vitest, Testing Library, Playwright, Tauri.

---

### Task 1: Build knowledge management

**Files:** Create `features/knowledge/` list/detail/import/task/index/source components and tests.

- [ ] **Step 1: Write tests** for CRUD, dedup, MinerU/local choice, progress/retry/cancel, versions/tags, index health/rebuild, search preview, delete impact, and errors.
- [ ] **Step 2: Run tests** and expect missing feature.
- [ ] **Step 3: Implement local-only workflows.** Long operations show persistent task IDs and recover after restart. Delete requires impact preview plus separate confirmation.
- [ ] **Step 4: Run unit/E2E tests** from PDF import through citation answer and model-switch rebuild.

### Task 2: Build production-data analysis

**Files:** Create `features/analysis/` import/mapping, quality, profile, model, training, inference, optimization, and export views.

- [ ] **Step 1: Write tests** for CSV/XLSX mapping/units/errors, activation, profiles, training recovery, inference warnings, constraints, infeasible optimization, and exports.
- [ ] **Step 2: Run tests** and expect missing feature.
- [ ] **Step 3: Implement task-backed surfaces.** Keep inputs, filters, model provenance, constraints, applicability, and exclusions visible. Use charts for comparisons and tables for exact values.
- [ ] **Step 4: Run E2E tests** for import-profile, train-restart-result, ONNX inference, and constrained optimization.

### Task 3: Build extension management

**Files:** Create `features/extensions/` MCP, Skills, package, tool, permission views and tests.

- [ ] **Step 1: Write tests** for MCP transports/health/restart, secret status, tools, Skill precedence/errors, signed/unsigned packages, compatibility, permission revoke, and rollback.
- [ ] **Step 2: Run tests** and expect missing feature.
- [ ] **Step 3: Implement trust-aware UI.** Unsigned packages and dangerous tools show source and impact. Status badges communicate state only.
- [ ] **Step 4: Run unit/E2E tests** against local fixture servers and packages.

### Task 4: Build complete settings

**Files:** Create `features/settings/` provider, data, memory, permission, appearance, proxy, and update sections with tests.

- [ ] **Step 1: Write tests** for OpenAI/Ollama/SiliconFlow/MinerU, free/Pro metadata, custom models, secret set/delete, proxy/timeouts, data path, memory policy, theme, permissions, updates, and validation.
- [ ] **Step 2: Run tests** and expect missing settings.
- [ ] **Step 3: Implement through bridge commands.** Secret controls expose configured/replacement/delete status only. Provider tests distinguish auth, quota, network, capability, and model errors.
- [ ] **Step 4: Run settings tests and provider fixture E2E** and expect pass.

### Task 5: Build diagnostics

**Files:** Create `features/diagnostics/` database/index/task/provider/MCP/disk/log views and tests.

- [ ] **Step 1: Write tests** for all health states, repair actions, redacted logs, diagnostics consent, copy/export, and absent optional services.
- [ ] **Step 2: Run tests** and expect missing diagnostics.
- [ ] **Step 3: Implement minimal-by-default diagnostics.** Optional configuration attachment requires explicit consent and still excludes credentials.
- [ ] **Step 4: Run diagnostics E2E** with injected corruption, low disk, provider failures, and known secret strings.

### Task 6: Build conversation export and full backup/restore

**Files:** Create `features/backup/`, export/restore bridge contracts, and tests.

- [ ] **Step 1: Write tests** for Markdown/JSON/PDF conversations, backup manifest/hash, optional originals, restore preview, version migration, corrupt archive rollback, and secret exclusion.
- [ ] **Step 2: Run tests** and expect missing workflows.
- [ ] **Step 3: Implement export/restore.** Validate restore in a temporary location, run migrations and index checks, then atomically replace; preserve existing data on every failure.
- [ ] **Step 4: Run E2E restore tests** including a backup containing a synthetic secret marker that must be rejected or excluded.

### Task 7: Complete accessibility and responsive behavior

**Files:** Add accessibility helpers and Playwright visual suite; modify feature CSS/components.

- [ ] **Step 1: Add checks** for keyboard order, focus traps, labels, contrast, reduced motion, zoom 125/150/200%, minimum window, long CJK/English text, and every loading/empty/error state.
- [ ] **Step 2: Run the suite** and save baseline failures.
- [ ] **Step 3: Fix every overlap, clipping, unstable dimension, inaccessible icon, nested card, and hidden focus.** Add tooltips for unfamiliar icon-only controls.
- [ ] **Step 4: Run all visual/accessibility tests** and inspect desktop/mobile-size screenshots even though the public target is desktop.

### Task 8: Complete Gate G

**Files:** Update E2E suites, product docs, and roadmap evidence.

- [ ] **Step 1: Run frontend unit, boundary, build, E2E, visual, and accessibility suites.** Capture exact output.
- [ ] **Step 2: Run Rust command contract and integration suites.** Capture exact output.
- [ ] **Step 3: Run manual Windows workflows** for first setup, task restart, citation, extensions, data analysis, diagnostics, export, and restore.
- [ ] **Step 4: Update Gate G only when all evidence is current** and source scans prove no component directly invokes Tauri or an external provider.

## Completion evidence

- E2E recordings for knowledge, analysis, extensions, settings, diagnostics, and backup/restore.
- Visual screenshots at supported sizes/zoom with no overlap or clipping.
- Secret-exclusion and corrupt-restore rollback results.
- Gate G checklist tied to exact test and manual workflow output.
