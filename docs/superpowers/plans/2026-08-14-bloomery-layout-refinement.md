# Bloomery Layout Refinement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Bloomery's Windows-first desktop shell visually consistent with the Web application while keeping the current local-first agent workflow and three-column workbench.

**Architecture:** Keep the existing React route composition and single Tauri bridge. Consolidate the visual cascade around one warm-white/steel-blue token set, then refine only the shell, workbench home, chat workbench, and responsive rules. No provider, API, storage, or Rust changes are part of this plan.

**Tech Stack:** React 18, TypeScript, Vite, CSS custom properties, Lucide React, Vitest, Testing Library.

## Global Constraints

- Windows 10 is the supported target for this iteration; do not add Windows 11-specific behavior.
- Do not add a dependency for visual changes.
- Do not modify `src-tauri/target/debug`, databases, models, indexes, or user data.
- Keep the global shell at a 64px top bar, a 216px expanded rail, and a 72px collapsed rail.
- Keep the chat workbench at 220px / flexible / 286px on wide desktop widths.
- Preserve keyboard focus order and visible focus rings.
- Preserve both Chinese and English locale rendering.
- Run the frontend test suite, runtime-boundary check, and production build before claiming completion.

---

### Task 1: Freeze the layout contract with focused tests

**Files:**

- Modify: `frontend/src/app/BloomeryLayout.test.tsx`
- Modify: `frontend/src/features/chat/ChatPage.test.tsx`
- Test: existing Vitest suites in the same files

**Interfaces:**

- Consumes: the existing `BloomeryApp` and `ChatPage` accessible labels.
- Produces: regression coverage for the shell regions, chat inspector, and responsive CSS contract.

- [x] **Step 1: Add shell assertions**

Assert that the rendered app contains exactly one main navigation, one main content region, and the workbench action group. Use `getByRole`/`getByLabelText` for user-visible structure.

- [x] **Step 2: Add chat workbench assertions**

Assert that `ChatPage` renders the conversation list, message composer, runtime inspector, and permission action labels without requiring implementation-specific class selectors.

- [x] **Step 3: Run focused tests**

Run from `F:/steel-agent/bloomery/frontend`:

```powershell
npm run test -- src/app/BloomeryLayout.test.tsx src/features/chat/ChatPage.test.tsx
```

Expected: the existing tests pass; any missing contract assertion fails before implementation.

### Task 2: Consolidate the visual token and shell cascade

**Files:**

- Modify: `frontend/src/design/tokens.css`
- Modify: `frontend/src/design/polish.css`
- Modify: `frontend/src/index.css`
- Test: `frontend/src/app/BloomeryLayout.test.tsx`

**Interfaces:**

- Consumes: existing `--bloomery-*` tokens and component class names.
- Produces: one canonical shell cascade with no contradictory topbar/sidebar/background definitions.

- [x] **Step 1: Define the canonical token values**

Keep the warm-white Web family as the canvas and use steel blue for primary actions:

```css
--bloomery-bg: #faf9f5;
--bloomery-bg-raised: #fffdf9;
--bloomery-bg-soft: #f5f0e8;
--bloomery-sidebar: #f7f4ef;
--bloomery-line: #eadfd2;
--bloomery-text: #2b2118;
--bloomery-text-soft: #5b5048;
--bloomery-text-muted: #8a7665;
--bloomery-steel: #557684;
--bloomery-amber: #b85f43;
--bloomery-green: #3f8b75;
--bloomery-topbar-height: 64px;
--bloomery-sidebar-width: 216px;
--bloomery-sidebar-collapsed-width: 72px;
```

- [x] **Step 2: Remove superseded visual blocks**

Delete duplicated declarations whose only purpose is to override an earlier declaration. Keep one definition for the app background, topbar, body grid, sidebar, buttons, cards, and chat workbench. Do not remove feature-specific selectors that have no replacement.

- [x] **Step 3: Remove decorative grid/noise treatment**

Keep the background quiet and readable. Do not use a grid texture, heavy gradient, glow, or backdrop blur as a structural surface.

- [x] **Step 4: Keep the global CSS import order stable**

Keep tokens before shared feature styles and preserve Tailwind imports after the project reset. Do not introduce another stylesheet import.

- [x] **Step 5: Run the layout contract**

Run:

```powershell
npm run test -- src/app/BloomeryLayout.test.tsx
```

Expected: PASS, including the 64px/216px/72px and reduced-motion assertions.

### Task 3: Refine the workbench home information hierarchy

**Files:**

- Modify: `frontend/src/app/WorkbenchHome.tsx`
- Modify: `frontend/src/design/polish.css`
- Modify: `frontend/src/i18n/locale.tsx` only if a missing Chinese/English label is required
- Test: `frontend/src/app/WorkbenchHome.test.tsx`

**Interfaces:**

- Consumes: `useWorkbenchOverview`, provider status, conversations, and background task data.
- Produces: a primary “start steel task” area, clear runtime state, and recent work without adding a new backend call.

- [x] **Step 1: Preserve data loading and degraded states**

Keep the existing independent overview sources and warning behavior. Do not replace real local data with static cards.

- [x] **Step 2: Group actions by intent**

Keep “new conversation” as the only primary button. Keep document and production-data import as secondary actions. Ensure every icon button has an accessible label.

- [x] **Step 3: Make status rows scannable**

Use cards only for the runtime group and recent-work group. Keep status labels, values, and semantic status icons aligned in one row, with long values truncating instead of widening the layout.

- [x] **Step 4: Add responsive assertions**

Keep the existing empty, loaded, and degraded tests; add one assertion that a recent conversation and an active task remain visible in the same page.

- [x] **Step 5: Run focused tests**

Run:

```powershell
npm run test -- src/app/WorkbenchHome.test.tsx
```

Expected: PASS.

### Task 4: Refine the chat workbench without changing agent behavior

**Files:**

- Modify: `frontend/src/features/chat/ChatView.tsx` only for semantic grouping or missing labels
- Modify: `frontend/src/design/polish.css`
- Test: `frontend/src/features/chat/ChatPage.test.tsx`

**Interfaces:**

- Consumes: existing agent events, permissions, citations, messages, and export actions.
- Produces: a readable session rail, conversation reading flow, composer, and runtime inspector.

- [x] **Step 1: Keep the three-column ownership**

The session rail owns local conversations, the center owns messages/citations/composer, and the inspector owns tools/permissions/tasks/evidence. Do not move runtime actions into an overflow menu.

- [x] **Step 2: Make the center column dominant**

Use white content surfaces, a maximum readable message width, pale steel-blue user messages, plain assistant content, and a composer that stays visually attached to the message flow.

- [x] **Step 3: Make runtime states explicit**

Keep text plus icon for loading, generating, failure, permission, and completed states. Preserve streaming output and permission decisions.

- [x] **Step 4: Preserve evidence proximity**

Keep `CitationPanel` beside the assistant answer and show only compact citation identifiers in the inspector.

- [x] **Step 5: Run focused chat tests**

Run:

```powershell
npm run test -- src/features/chat/ChatPage.test.tsx
```

Expected: PASS for loading, send, evidence, streaming, replay, export, and permission flows.

### Task 5: Verify Win10-oriented responsive behavior

**Files:**

- Modify: `frontend/src/design/polish.css` only if a breakpoint rule is needed
- Modify: `frontend/src/app/BloomeryLayout.test.tsx` if a contract assertion is missing

**Interfaces:**

- Consumes: shell and chat grid rules.
- Produces: stable layout behavior at 1440x900, 1366x768, 1024x720, 980px, 760px, and 640px widths.

- [x] **Step 1: Verify the wide shell**

Confirm the expanded navigation and chat three-column layout remain readable at 1366x768 and 1440x900.

- [x] **Step 2: Verify the compact shell**

Confirm the inspector stacks below the conversation at 980px and the session rail remains usable.

- [x] **Step 3: Verify the narrow flow**

Confirm all chat columns become one flow at 640px, buttons remain reachable, and no horizontal overflow is introduced.

- [x] **Step 4: Verify reduced motion**

Keep the `prefers-reduced-motion: reduce` rule and ensure all decorative transitions become effectively static.

### Task 6: Run the full frontend quality gate

**Files:**

- No new files.

- [x] **Step 1: Run frontend tests**

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm run test
```

- [x] **Step 2: Run runtime boundaries**

```powershell
npm run test:boundaries
```

- [x] **Step 3: Run production build**

```powershell
npm run build
```

Expected: all tests and boundary checks pass, and the build succeeds. Existing chunk-size warnings may remain and must be reported separately.

- [x] **Step 4: Inspect the final diff**

```powershell
Set-Location F:/steel-agent/bloomery
git diff --check
git status --short
```

Do not stage generated build artifacts or anything under `src-tauri/target`.
