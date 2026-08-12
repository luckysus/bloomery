# Bloomery Foundation And Web Decoupling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the current Web-derived desktop repository into an independently bootable local Bloomery shell with no login, private backend, cloud task, or Web API runtime path.

**Architecture:** Introduce a singleton local workspace identity, a thin Tauri application composition layer, and a new React application shell. Keep existing local SQLite data readable while removing authentication and cloud behavior before later storage migrations.

**Tech Stack:** Rust, Tauri 2, rusqlite, React, TypeScript, Vite, PowerShell, Node architecture tests.

---

## Target file map

Create:

```text
src-tauri/src/app/mod.rs                 Tauri builder composition
src-tauri/src/app/identity.rs            singleton local workspace identity
src-tauri/src/app/commands.rs            stable command registration boundary
src-tauri/tests/architecture.rs          Rust module and command boundary checks
frontend/src/app/BloomeryApp.tsx         desktop application root
frontend/src/app/AppShell.tsx             navigation and routed surface
frontend/src/app/appNavigation.ts         navigation model
frontend/src/bridge/desktop.ts            only Tauri invoke/listen entry point
frontend/src/bridge/contracts.ts          temporary hand-written bridge contracts
frontend/scripts/test-boundaries.mjs      source and dependency boundary checks
```

Modify:

```text
src-tauri/src/main.rs
src-tauri/src/db.rs
src-tauri/src/context.rs
src-tauri/src/local_agent.rs
src-tauri/src/models.rs
frontend/src/main.tsx
frontend/package.json
frontend/tsconfig.app.json
README.md
```

Remove after callers migrate:

```text
src-tauri/src/auth.rs
src-tauri/src/cloud_tasks.rs
frontend/src/LoginPage.tsx
frontend/src/ModeSelectPage.tsx
frontend/src/context/AuthContext.tsx
frontend/src/hooks/useAuthSession.ts
frontend/src/services/auth.ts
frontend/src/desktop/DesktopRuntimeBridge.tsx
frontend/src/desktop/services/cloudTasks.ts
frontend/src/desktop/services/cloudJobs.ts
```

Cloud-only Web pages, hooks, services, captcha components, profile/admin components, and generated Web OpenAPI types are removed in Task 9 after the new reachable graph builds.

### Task 1: Add executable architecture boundaries

**Files:**
- Create: `frontend/scripts/test-boundaries.mjs`
- Modify: `frontend/package.json`
- Create: `src-tauri/tests/architecture.rs`

- [x] **Step 1: Write the failing frontend boundary script**

Create a Node script that recursively reads reachable production source under `frontend/src/app`, `frontend/src/bridge`, and `frontend/src/desktop`, then rejects these patterns:

```js
const forbidden = [
  /\/api\//,
  /cloud_api_base/,
  /AuthProvider/,
  /useAuthSession/,
  /DesktopCloudTask/,
];
```

The script must also assert that `frontend/src/main.tsx` imports `./app/BloomeryApp` and that only `frontend/src/bridge/desktop.ts` imports `@tauri-apps/api/core` or `@tauri-apps/api/event`.

- [x] **Step 2: Register and run the failing script**

Add `"test:boundaries": "node scripts/test-boundaries.mjs"` to `frontend/package.json`.

Run:

```powershell
Set-Location frontend
npm run test:boundaries
```

Expected: failure because the new app and bridge do not exist.

- [x] **Step 3: Write the failing Rust architecture test**

The test reads `src/main.rs` and fails while it contains:

```rust
for forbidden in ["mod auth;", "mod cloud_tasks;", "auth_get_session", "sync_cloud_jobs"] {
    assert!(!main_source.contains(forbidden), "forbidden command: {forbidden}");
}
```

It also asserts `local_agent.rs` and `db.rs` are below their transitional limits of 3,000 and 2,000 lines. Later plans reduce these budgets.

- [x] **Step 4: Run the failing Rust test**

Run:

```powershell
Set-Location src-tauri
cargo test --test architecture
```

Expected: failure naming auth and cloud task registrations.

### Task 2: Introduce singleton local identity

**Files:**
- Create: `src-tauri/src/app/mod.rs`
- Create: `src-tauri/src/app/identity.rs`
- Modify: `src-tauri/src/main.rs`

- [x] **Step 1: Write identity unit tests**

Test these invariants:

```rust
#[test]
fn local_identity_is_stable() {
    assert_eq!(LocalIdentity::default().workspace_id(), "local");
}

#[test]
fn local_identity_has_no_token() {
    assert_eq!(LocalIdentity::default().credential(), None);
}
```

- [x] **Step 2: Run the identity tests and verify failure**

Run `cargo test app::identity --lib` and expect unresolved `LocalIdentity`.

- [x] **Step 3: Implement the local identity**

Use a zero-secret state:

```rust
pub const LOCAL_WORKSPACE_ID: &str = "local";

#[derive(Debug, Default)]
pub struct LocalIdentity;

impl LocalIdentity {
    pub fn workspace_id(&self) -> &'static str { LOCAL_WORKSPACE_ID }
    pub fn credential(&self) -> Option<&str> { None }
}
```

Register it with Tauri state. Do not read or create `auth-session.json`.

- [x] **Step 4: Run the focused tests**

Run `cargo test app::identity --lib` and expect both tests to pass.

### Task 3: Replace authenticated ownership with local workspace ownership

**Files:**
- Modify: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/context.rs`
- Modify: `src-tauri/src/local_agent.rs`

- [x] **Step 1: Add failing repository tests for singleton ownership**

Create tests that call pure repository helpers with `LOCAL_WORKSPACE_ID`, insert a conversation, and verify a second local call reads the same conversation without an auth session. Preserve an explicit ownership argument internally until the migration plan renames columns.

- [x] **Step 2: Run focused tests and verify auth-state compilation failure**

Run:

```powershell
cargo test local_workspace --lib
```

Expected: helpers still require `AuthState`.

- [x] **Step 3: Replace auth state parameters**

Replace `current_user_id(&AuthState)` with:

```rust
pub(crate) fn current_workspace_id() -> &'static str {
    crate::app::identity::LOCAL_WORKSPACE_ID
}
```

Tauri commands obtain no identity argument. They pass the constant only at repository boundaries. Rename local variables from `user_id` to `workspace_id` in every modified function, while leaving SQL column names unchanged until ordered migrations exist.

- [x] **Step 4: Run conversation, memory, summary, draft, and context tests**

Run `cargo test --lib` and expect all non-cloud tests to pass.

### Task 4: Remove cloud routing from the local agent

**Files:**
- Modify: `src-tauri/src/local_agent.rs`
- Modify: `src-tauri/src/models.rs`

- [x] **Step 1: Replace cloud behavior tests with local-only routing tests**

Add tests asserting that steel search, training, optimization, and literature wording never creates a cloud confirmation:

```rust
#[test]
fn domain_requests_never_route_to_private_cloud() {
    for input in ["检索 Q355B 文献", "训练强度模型", "优化轧制工艺"] {
        let route = classify_desktop_intent(input);
        assert!(!route.requires_external_job());
    }
}
```

- [x] **Step 2: Run the test and verify failure**

Run `cargo test domain_requests_never_route_to_private_cloud --lib` and expect a failure for existing cloud routes.

- [x] **Step 3: Remove cloud branches and types**

Delete `ConfirmedCloudTask`, `CloudJobOutcome`, `fetch_cloud_knowledge`, cloud URL helpers, cloud job submission, cloud confirmation response builders, and `desktop_confirm_cloud_job`. Keep local LLM streaming and local conversation persistence. Domain intents that require unfinished local tools return a structured `capability_unavailable` result, never a Web URL.

- [x] **Step 4: Run local agent tests**

Run `cargo test local_agent --lib` and expect passing local behavior with no cloud tests.

### Task 5: Establish a narrow Tauri composition root

**Files:**
- Create: `src-tauri/src/app/commands.rs`
- Modify: `src-tauri/src/app/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Remove: `src-tauri/src/auth.rs`
- Remove: `src-tauri/src/cloud_tasks.rs`

- [x] **Step 1: Make the architecture test require the new composition root**

Assert `main.rs` contains only module declaration, Windows subsystem attribute, and `app::run()`.

- [x] **Step 2: Run the architecture test and verify failure**

Run `cargo test --test architecture` and expect the composition assertion to fail.

- [x] **Step 3: Move builder and command registration**

Expose:

```rust
pub fn run() {
    tauri::Builder::default()
        .manage(identity::LocalIdentity::default())
        .manage(crate::db::DbState::default())
        .manage(crate::local_agent::LocalAgentState::default())
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("failed to run Bloomery");
}
```

`commands::handler()` registers only local database, local context, local LLM, and diagnostics commands. Remove auth and cloud files only after no module imports remain.

- [x] **Step 4: Run Rust verification**

Run `cargo test`, then `cargo check`. Both must pass.

### Task 6: Create the independent React application root

**Files:**
- Create: `frontend/src/app/BloomeryApp.tsx`
- Create: `frontend/src/app/AppShell.tsx`
- Create: `frontend/src/app/appNavigation.ts`
- Modify: `frontend/src/main.tsx`

- [x] **Step 1: Add a boundary assertion for the root**

Require exactly one production root import in `main.tsx`:

```js
assert.match(main, /from "\.\/app\/BloomeryApp"/);
assert.doesNotMatch(main, /AuthProvider|RagAppPage|DesktopApp/);
```

- [x] **Step 2: Run `npm run test:boundaries` and verify failure**

Expected: `main.tsx` still imports `DesktopApp`.

- [x] **Step 3: Implement the root and stable navigation**

Define navigation IDs, not URL routes:

```ts
export type AppSection =
  | "workbench" | "chat" | "knowledge" | "analysis"
  | "extensions" | "settings" | "diagnostics";
```

`BloomeryApp` owns the selected section and renders `AppShell`. Initial placeholders are explicit product states such as “知识库尚未建立”, not Web fallbacks or feature marketing.

- [x] **Step 4: Run frontend build**

Run `npm run test:boundaries` and `npm run build`. Both must pass for the new reachable graph.

### Task 7: Consolidate the desktop bridge

**Files:**
- Create: `frontend/src/bridge/contracts.ts`
- Create: `frontend/src/bridge/desktop.ts`
- Modify: reachable desktop services migrated into the bridge

- [x] **Step 1: Write a boundary test for Tauri imports**

Scan reachable source and fail when any file except `bridge/desktop.ts` imports `@tauri-apps/api/core` or `@tauri-apps/api/event`.

- [x] **Step 2: Run the boundary test and verify existing service imports fail**

Expected offenders include local agent, local ask, and desktop service modules.

- [x] **Step 3: Implement a typed bridge**

Expose a single object:

```ts
export const desktop = {
  initialize: () => invoke<void>("db_init"),
  listConversations: () => invoke<Conversation[]>("list_conversations"),
  chat: (request: AgentChatRequest) => invoke<AgentResponse>("desktop_agent_chat", { request }),
  cancelRun: (runId: string) => invoke<void>("desktop_cancel_llm_run", { runId }),
};
```

Move event listeners behind bridge functions returning cleanup callbacks. Never expose raw `invoke` to components.

- [x] **Step 4: Run boundary and build verification**

Run `npm run test:boundaries` and `npm run build`.

### Task 8: Remove authentication and cloud UI adapters

**Files:**
- Remove: `frontend/src/LoginPage.tsx`
- Remove: `frontend/src/ModeSelectPage.tsx`
- Remove: `frontend/src/context/AuthContext.tsx`
- Remove: `frontend/src/hooks/useAuthSession.ts`
- Remove: `frontend/src/services/auth.ts`
- Remove: `frontend/src/desktop/DesktopRuntimeBridge.tsx`
- Remove: `frontend/src/desktop/services/cloudTasks.ts`
- Remove: `frontend/src/desktop/services/cloudJobs.ts`
- Modify: all remaining imports reported by `rg`

- [x] **Step 1: Add source-wide forbidden symbol assertions**

Reject `LoginPage`, `AuthProvider`, `authHeaders`, `cloud_api_base`, `DesktopCloudTask`, `CloudJob`, and `/api/` across production TypeScript and Rust, excluding tests that contain the forbidden-token list itself.

- [x] **Step 2: Run boundary tests and capture all offenders**

Run `npm run test:boundaries` and `cargo test --test architecture`; both should fail with a complete offender list.

- [x] **Step 3: Remove adapters and update local callers**

Delete the listed files. Replace any loading gate based on `authChecked` with application initialization state from `desktop.initialize()`. Replace task-mirror terminology with local background task terminology only when the durable task implementation exists; until then do not expose the unfinished task page.

- [x] **Step 4: Verify no forbidden runtime symbol remains**

Run:

```powershell
rg -n '/api/|cloud_api_base|AuthProvider|useAuthSession|DesktopCloudTask|CloudJob' frontend/src src-tauri/src
```

Expected: no production matches.

### Task 9: Remove unreachable Web application code

**Files:**
- Remove: Web-only pages, hooks, services, captcha/profile/admin components, `frontend/src/types/openapi.json`
- Preserve or move: answer rendering, PDF viewer, raw document viewer, common controls, steel charts that have no Web dependency
- Modify: `frontend/tsconfig.app.json`

- [x] **Step 1: Generate a reviewed reachability inventory**

Use Vite entry imports and `rg` to classify each source file as `keep`, `move`, or `remove`. Record the inventory in this plan under an execution note before deleting files. Files are kept only when imported by the new app or explicitly assigned to a later plan.

- [x] **Step 2: Run the build before removal**

Run `npm run build` and retain the successful output as the baseline.

- [x] **Step 3: Delete Web-only code and move reusable components**

Remove `services/api.ts`, Web agent conversation services, training/search/literature/optimizer/lab services, old controller hooks, old page composition, login/captcha/profile components, and generated Web API contract. Keep reusable visual components under neutral feature directories and remove their Web-shaped props.

- [x] **Step 4: Run source scan and build**

Run `npm run test:boundaries`, `npm run build`, and a source-wide `/api/` scan. All must pass or return no matches.

### Task 10: Replace the cloud-oriented README and diagnostics language

**Files:**
- Modify: `README.md`
- Modify: `src-tauri/src/db.rs`
- Modify: diagnostics UI if present

- [x] **Step 1: Add a UTF-8 and forbidden-copy check**

The boundary script reads README as UTF-8 and rejects replacement characters plus the phrases `云端 API 地址`, `登录`, `任务镜像`, and private-cloud configuration instructions.

- [x] **Step 2: Run the script and verify failure against the current README**

Run `npm run test:boundaries` and expect cloud-oriented README copy to fail.

- [x] **Step 3: Write the independent-product README skeleton**

Document local-first positioning, current development commands, local data location, no-login behavior, user-owned providers, product boundaries, and the fact that incomplete development builds are not the public release. Remove cloud and authenticated diagnostic fields.

- [x] **Step 4: Run full Gate A verification**

Run:

```powershell
Set-Location src-tauri
cargo test
cargo check
Set-Location ../frontend
npm run test:boundaries
npm run build
```

Expected: all commands pass and source scans find no private backend runtime path.

## Completion evidence

- Architecture test output for Rust and TypeScript boundaries.
- `cargo test`, `cargo check`, and `npm run build` output.
- Source-wide forbidden-token scan with no production offenders.
- A fresh local launch that reaches the workbench without login or network traffic.
- Updated Gate A checkboxes in the roadmap.
## Execution record (2026-07-29)

- Reachability inventory kept the new `app/` root, the single `bridge/desktop.ts` Tauri boundary, answer/document renderers, common controls, export utilities, and neutral date/render helpers.
- Removed the old Web application composition, login/captcha/profile UI, Web API services and OpenAPI types, cloud task adapters, old desktop runtime adapters, unreachable agent pages, and the Rust auth/cloud-task modules. The existing SQLite file remains readable; no user data or legacy table was deleted.
- Consolidated frontend enforcement in `frontend/scripts/check-runtime-boundaries.mjs`; removed three unreferenced or obsolete boundary scripts.
- TDD startup regression: `cargo tauri dev` first failed because `npm --prefix frontend` resolved `frontend/frontend/package.json`; the new architecture test failed on the old hooks, then passed after both hooks were corrected to run from Tauri's frontend working directory.
- Gate verification passed: `npm run test:boundaries`, `npm run build`, `cargo test` (35 library tests and 7 architecture tests after the startup regression), `cargo check`, and a production-only forbidden-symbol scan.
- Runtime smoke created the sole `Bloomery` window and `C:/Users/Administrator/AppData/Roaming/com.bloomery.desktop/bloomery.sqlite3`, then stopped only the Bloomery/Cargo/Vite/esbuild processes started by the smoke test. Windows screenshot approval timed out, so this gate records launch and initialization evidence rather than a visual-polish acceptance.
