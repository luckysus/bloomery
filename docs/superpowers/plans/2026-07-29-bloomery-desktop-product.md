# Bloomery Desktop Core Product Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the tested Windows desktop shell, onboarding, workbench, protocol-driven chat, and evidence inspection experience.

**Architecture:** React consumes one typed desktop bridge; components never invoke Tauri or providers directly. UI state derives from persisted backend records and replayable Agent events.

**Tech Stack:** React 18, TypeScript, Vite, lucide-react, pdfjs-dist, Vitest, Testing Library, Playwright.

---

### Task 1: Add frontend unit and E2E infrastructure

**Files:** Modify `package.json`; create Vitest setup, fake desktop bridge, Playwright config, and smoke tests.

- [x] **Step 1: Write a failing test** that renders `BloomeryApp` with fake initialization and expects the workbench landmark.
- [x] **Step 2: Run `npm run test`** and expect missing script/config.
- [x] **Step 3: Add Vitest, Testing Library, jsdom, and Playwright scripts.** The fake bridge records calls and emits deterministic protocol events; production imports are forbidden.
- [x] **Step 4: Run `npm run test`, `npm run test:boundaries`, and `npm run build`** and expect pass.

### Task 2: Establish the industrial design system and shell

**Files:** Create `design/tokens.css`, `theme.css`, app shell/router, layout/common components, and tests.

- [x] **Step 1: Write layout tests** for every navigation section, active/collapsed states, focus, 1024x720, 1440x900, 1920x1080, and long text.
- [x] **Step 2: Run tests** and expect missing shell.
- [x] **Step 3: Implement a restrained steel palette** with neutral metal gray, safety amber, process green, error red, and one limited accent. Use lucide icons, radii at most 8px, no nested cards, decorative gradients/orbs, viewport-scaled fonts, or negative letter spacing.
- [x] **Step 4: Run tests and Playwright screenshots** at supported sizes and verify no overlap/clipping.

### Task 3: Build first-run setup without login

**Files:** Create `features/onboarding/`, setup bridge methods, tests, and E2E spec.

- [ ] **Step 1: Write tests** for data path, LLM/key/test, optional SiliconFlow/MinerU, steel package install, skipped/degraded states, retry, restart resume, and first chat/document actions.
- [ ] **Step 2: Run tests** and expect missing flow.
- [ ] **Step 3: Implement persisted setup.** Never echo keys. Surface Rust auth/quota/network/capability errors. Skipped optional services stay visible with exact setup actions.
- [ ] **Step 4: Run unit/E2E tests** and verify mocked first chat within three minutes.

### Task 4: Build the workbench home

**Files:** Create `features/workbench/` summary hook/components and tests.

- [ ] **Step 1: Write tests** for recent conversations/tasks/datasets, knowledge/index/provider health, primary actions, and empty/loading/error/degraded states.
- [ ] **Step 2: Run tests** and expect failure.
- [ ] **Step 3: Implement a compact full-width workbench.** Prefer status rows and lists over marketing cards. Primary actions are new conversation, import document, and import data.
- [ ] **Step 4: Run tests and screenshots** with empty, normal, degraded, and long Chinese fixtures.

### Task 5: Build protocol-driven Agent chat

**Files:** Create `features/chat/` store, conversation list, composer, messages, run timeline, permission dialog, evidence drawer, usage view, and tests.

- [ ] **Step 1: Write reducer tests** for ordered/out-of-order/duplicate events, replay, streaming, tools, permission, evidence, cancel, complete, interrupt, and retry.
- [ ] **Step 2: Run tests** and expect missing feature.
- [ ] **Step 3: Implement chat.** Keep dimensions stable during streaming. Show tool source/input/permission/progress/result/error. Permission actions are Allow once/session/always and Deny with explicit impact.
- [ ] **Step 4: Run unit/E2E tests** for direct, RAG, multi-tool, denial, repair failure, cancellation, crash replay, and long citation runs.

### Task 6: Build citation and source inspection

**Files:** Create source resolver, evidence drawer, PDF page viewer, spreadsheet range viewer, and tests.

- [ ] **Step 1: Write tests** for PDF page, sheet range, heading, table, image, missing/deleted source, and inactive version.
- [ ] **Step 2: Run tests** and expect missing resolver.
- [ ] **Step 3: Implement exact source navigation.** Display rank/score/provenance without presenting retrieval scores as probability confidence.
- [ ] **Step 4: Run citation E2E/screenshots** for PDF, XLSX, image, and missing-source states.

## Handoff and evidence

Continue immediately with `2026-07-29-bloomery-desktop-management.md`. Evidence requires unit/E2E outputs, supported-window screenshots, first-run recording, protocol replay, permission flow, and source-resolving citations.
