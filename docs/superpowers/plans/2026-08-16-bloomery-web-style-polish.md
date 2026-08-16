# Bloomery Web Style Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the Bloomery desktop UI into the Web application's warm paper, terracotta, and editorial visual language while tightening the workbench layout and removing non-essential explanatory copy.

**Architecture:** Keep the existing React page/component boundaries and Tauri bridge unchanged. Put shared visual decisions in the existing design token, theme, and polish CSS layers; make only the workbench and shell markup changes needed to remove redundant copy and preserve accessible labels.

**Tech Stack:** React 18, TypeScript, Vite, CSS custom properties, Lucide icons, Vitest and Testing Library.

## Global Constraints

- Bloomery remains a Windows-first Tauri desktop client and stays decoupled from the Web runtime and content.
- The Web frontend's palette is the visual source of truth: `#faf9f5`, `#fffdf9`, `#f5f0e8`, `#f1e8de`, `#141413`, `#252523`, `#6c6a64`, `#cc785c`, `#a9583e`, and `#e6dfd8`.
- Do not introduce indigo, cyan, purple, glassmorphism, large decorative gradients, or new UI dependencies.
- Remove only redundant explanatory copy; retain functional labels, errors, loading states, and accessibility names.
- Preserve visible keyboard focus, reduced-motion behavior, and existing navigation/test IDs.
- Use the existing Inter, Noto Sans SC, and Lucide icon system.

### Task 1: Lock the copy and layout contract with tests

**Files:**
- Modify: `frontend/src/app/BloomeryApp.test.tsx`
- Modify: `frontend/src/app/BloomeryLayout.test.tsx`
- Modify: `frontend/src/app/WorkbenchHome.test.tsx`

**Interfaces:**
- Consumes the existing `BloomeryApp`, `BloomeryLayout`, and `WorkbenchHome` render contracts.
- Produces regression assertions for concise shell copy, preserved accessible navigation, and compact workbench structure.

- [x] **Step 1: Add a failing shell-copy test**

  Render the default app and assert that the visual-only brand tagline, sidebar footer, and workbench lede are absent while the workbench heading and navigation remain present.

- [x] **Step 2: Run the focused frontend tests and verify the expected failure**

  Run from `F:/steel-agent/bloomery/frontend`:

  ```powershell
  npm run test -- src/app/BloomeryApp.test.tsx src/app/WorkbenchHome.test.tsx
  ```

  Expected: the new absence assertions fail because the current shell still renders the explanatory copy.

- [x] **Step 3: Add a failing layout contract assertion**

  Assert that the workbench exposes a compact heading/action region and that the status and recent-work sections share the same grid container without introducing a second content wrapper.

- [x] **Step 4: Run the focused layout test and verify the expected failure**

  ```powershell
  npm run test -- src/app/BloomeryLayout.test.tsx
  ```

  Expected: the new class/structure assertion fails before the markup change.

### Task 2: Align shared tokens with the Web visual system

**Files:**
- Modify: `frontend/src/design/tokens.css`
- Modify: `frontend/src/design/theme.css`
- Modify: `frontend/src/design/polish.css`

**Interfaces:**
- Consumes the existing `--bloomery-*` token names used by all desktop pages.
- Produces a single warm-light visual system for shell, cards, controls, states, focus, hover, and responsive layout.

- [x] **Step 1: Update the semantic token values**

  Map the existing background, surface, text, muted text, border, accent, hover, and shadow tokens to the Web palette. Keep aliases such as `--bloomery-steel` only when existing selectors require them, but map them to the warm editorial palette instead of adding a competing blue system.

- [x] **Step 2: Tighten the global shell rhythm**

  Reduce topbar/sidebar excess, use the existing 4/8px spacing rhythm, keep content gutters adaptive, and retain the current collapsed-sidebar behavior. Use transitions between 150ms and 300ms and preserve `prefers-reduced-motion`.

- [x] **Step 3: Restyle common controls and surfaces**

  Apply the Web palette to navigation, buttons, badges, status rows, empty states, cards, inputs, and focus rings. Keep borders visible and shadows soft; do not add gradients or new dependencies.

- [x] **Step 4: Run the existing CSS/runtime boundary tests**

  ```powershell
  npm run test:boundaries
  ```

  Expected: runtime boundary checks pass and no forbidden frontend integration is introduced.

### Task 3: Simplify the shell and optimize the workbench layout

**Files:**
- Modify: `frontend/src/app/BloomeryApp.tsx`
- Modify: `frontend/src/app/WorkbenchHome.tsx`
- Modify: `frontend/src/app/BloomeryLayout.test.tsx`

**Interfaces:**
- Consumes the existing localization keys and `onOpenSection` callbacks.
- Produces the same navigation, status, action, and data behavior with a shorter visual hierarchy.

- [x] **Step 1: Remove redundant visual copy without deleting functional state**

  Remove the rendered brand tagline, sidebar explanatory footer, hero lede, and repeated eyebrow labels from the workbench. Keep the accessible `aria-label`, page heading, status values, error text, loading text, and button labels.

- [x] **Step 2: Make the workbench header and actions one compact region**

  Keep the workbench title as the single visual heading, place the three existing actions in the same responsive header/action band, and preserve their current callbacks and accessible names.

- [x] **Step 3: Normalize the status/recent-work grid**

  Keep the status and recent-work sections as equal-height cards in a responsive two-column grid, reduce empty vertical space, and retain all existing `data-testid` hooks and loading/error branches.

- [x] **Step 4: Run the focused tests and verify they pass**

  ```powershell
  npm run test -- src/app/BloomeryApp.test.tsx src/app/BloomeryLayout.test.tsx src/app/WorkbenchHome.test.tsx
  ```

  Expected: all focused tests pass, including the new copy and layout contracts.

### Task 4: Verify the complete frontend deliverable

**Files:**
- No new production files.

- [x] **Step 1: Run the complete frontend unit/integration suite**

  ```powershell
  npm run test
  ```

- [x] **Step 2: Run runtime boundaries**

  ```powershell
  npm run test:boundaries
  ```

- [x] **Step 3: Run the production build**

  ```powershell
  npm run build
  ```

- [x] **Step 4: Inspect the final diff**

  Confirm only Bloomery frontend design, shell/workbench copy, tests, and this plan changed; confirm no Web source, backend, bridge, or generated artifact was modified.

- [x] **Step 5: Commit the completed visual milestone**

  ```powershell
  git add frontend/src/design/tokens.css frontend/src/design/theme.css frontend/src/design/polish.css frontend/src/app/BloomeryApp.tsx frontend/src/app/WorkbenchHome.tsx frontend/src/app/BloomeryApp.test.tsx frontend/src/app/BloomeryLayout.test.tsx frontend/src/app/WorkbenchHome.test.tsx docs/superpowers/plans/2026-08-16-bloomery-web-style-polish.md
  git commit -m "美化客户端界面并优化工作台布局"
  git push origin main
  git push gitee main
  ```
