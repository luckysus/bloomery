# Web 对话页面完整迁移实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Web 端完整钢铁智能体对话页面的结构、导航和入口迁移到 Bloomery，先保留所有页面功能，不提前删除任何 Web 元素。

**Architecture:** Bloomery 不运行时依赖 `Web/frontend`。新增一个独立的 Web 风格桌面对话壳，复用 Web 的布局层级、交互顺序和视觉规则；消息、会话、模型、RAG、权限和流式状态继续通过 Bloomery 现有 `useChatController` 与 Rust bridge 提供。Web 页面中的云端动作先由 Bloomery 的本地模块入口承接，后续用户指定删除项后再收缩，并将剩余入口逐项接入 Rust。

**Tech Stack:** React 18、TypeScript、Vite、Vitest、Testing Library、Lucide React、Bloomery Tauri bridge。

## Global Constraints

- Bloomery 是 `F:/steel-agent/bloomery` 的独立嵌套 Git 仓库，只修改该仓库范围内的文件。
- 使用 PowerShell；默认 UTF-8；不依赖 Web 登录、Web API 或私有云服务。
- 先完整保留 Web 页面入口：模式切换、知识库、模型训练、工艺优化、个人中心、最近会话、搜索、消息区和输入区。
- 不在本阶段删除 Web 功能；用户体验后单独给出删除清单。
- 生产代码变更必须先有能失败的行为测试；只新增必要组件和样式，避免引入新依赖。
- 不修改 Rust 协议和数据库契约；当前本地对话与流式 Agent 行为必须保持不变。

---

### Task 1: 为完整 Web 对话壳建立行为契约

**Files:**
- Create: `frontend/src/features/chat/WebChatWorkspace.test.tsx`
- Modify: `frontend/src/features/chat/ChatPage.test.tsx`

**Interfaces:**
- Consumes: `ChatPage` 可注入的 `onOpenSection?: (section: SectionId) => void`。
- Produces: 可验证的完整页面地标：模式切换、知识库、模型训练、工艺优化、个人中心、最近会话和现有对话输入。

- [ ] **Step 1: Write the failing test**

新增测试渲染 `ChatPage`，验证：

```tsx
render(<ChatPage onOpenSection={onOpenSection} />);
expect(await screen.findByRole("button", { name: "钢铁智能体" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "多模态智能检索" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "知识库" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "模型训练" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "工艺优化" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "个人中心" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "新聊天" })).toBeInTheDocument();
expect(screen.getByRole("textbox", { name: "输入消息" })).toBeInTheDocument();
```

同时点击顶部入口并断言回调收到对应 `SectionId`；现有 `ChatView` 的 Rust bridge 行为测试继续保留。

- [ ] **Step 2: Run test to verify it fails**

Run:

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test -- WebChatWorkspace.test.tsx
```

Expected: FAIL because `ChatPage` currently only renders the本地会话双栏视图，没有 Web 页面壳和顶部入口。

- [ ] **Step 3: Keep the failure focused**

如果测试因测试桩缺少已有 bridge 方法而报错，补齐测试桩；不得先修改生产组件来绕过失败。失败原因必须落在缺失的页面地标或入口回调上。

### Task 2: 实现 Web 风格完整页面壳

**Files:**
- Create: `frontend/src/features/chat/WebChatWorkspace.tsx`
- Modify: `frontend/src/features/chat/ChatPage.tsx`
- Modify: `frontend/src/features/chat/ChatView.tsx`

**Interfaces:**
- `WebChatWorkspaceProps` 接收 `ChatControllerProps` 和 `onOpenSection?: (section: SectionId) => void`。
- `ChatPage({ onOpenSection })` 调用 `useChatController()`，将控制器结果交给 `WebChatWorkspace`。
- `ChatView` 只负责中间消息与输入区；不把 Rust bridge 调用搬进布局组件。

- [ ] **Step 1: Implement the minimum shell**

在 `WebChatWorkspace.tsx` 中实现以下结构：

```tsx
<section className="bloomery-web-chat" aria-label="钢铁智能体">
  <aside className="bloomery-web-chat-sidebar">
    <button aria-label="钢铁智能体">...</button>
    <button aria-label="多模态智能检索">...</button>
    <button aria-label="新聊天">...</button>
    <button aria-label="搜索聊天">...</button>
    <RecentConversationList ... />
    <button aria-label="个人中心">...</button>
  </aside>
  <main className="bloomery-web-chat-main">
    <header className="bloomery-web-chat-top-actions">
      <button aria-label="知识库">...</button>
      <button aria-label="模型训练">...</button>
      <button aria-label="工艺优化">...</button>
    </header>
    <ChatView {...controllerProps} />
  </main>
</section>
```

模式切换先保持页面可见；点击“多模态智能检索”进入 Bloomery 的知识库模块。顶部“知识库”进入知识库模块，“模型训练”和“工艺优化”进入数据分析模块；“个人中心”进入设置模块。没有回调时入口仍可渲染，避免单独测试崩溃。

- [ ] **Step 2: 保持现有对话功能不变**

将 `ChatView` 的控制器 props 原样传入，保留会话管理、Markdown、引用、权限、模型切换、智能搜索、语音、导出、停止生成和事件回放。只补充 Web 页面需要的 `aria-label` 和布局容器，不改 bridge 请求参数。

- [ ] **Step 3: Add minimal Web layout styles**

在 `frontend/src/design/polish.css` 增加 `.bloomery-web-chat*` 样式，复用已有 token：

```css
.bloomery-web-chat {
  background: var(--bloomery-bg);
  display: grid;
  grid-template-columns: 296px minmax(0, 1fr);
  height: 100dvh;
  overflow: hidden;
}
```

样式覆盖 Web 的侧栏、模式入口、最近会话、个人入口、顶部动作、窄屏侧栏和深色主题；不引入新的 CSS 框架或颜色体系。

- [ ] **Step 4: Run focused tests to verify they pass**

Run:

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test -- WebChatWorkspace.test.tsx ChatPage.test.tsx
```

Expected: 新增完整页面契约与现有对话测试全部 PASS。

### Task 3: 让 Bloomery 在对话入口使用完整页面

**Files:**
- Modify: `frontend/src/app/BloomeryApp.tsx`
- Modify: `frontend/src/app/BloomeryApp.test.tsx`
- Modify: `frontend/src/app/BloomeryLayout.test.tsx`

**Interfaces:**
- 非对话模块继续使用 Bloomery 原有顶部栏和通用模块导航。
- 进入“对话”后切换为完整 Web 对话壳，避免出现重复的 Bloomery 外层导航。
- `onOpenSection` 负责从 Web 页面顶部入口返回 Bloomery 的本地知识库、数据分析或设置模块。

- [ ] **Step 1: Add the shell transition assertion**

在 `BloomeryApp.test.tsx` 中点击“对话”，断言：

```tsx
fireEvent.click(screen.getByRole("button", { name: "对话" }));
expect(await screen.findByRole("button", { name: "钢铁智能体" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "知识库" })).toBeInTheDocument();
expect(screen.queryByRole("navigation", { name: "主导航" })).not.toBeInTheDocument();
```

- [ ] **Step 2: Run the test to verify the transition assertion fails**

Run:

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test -- BloomeryApp.test.tsx
```

Expected: FAIL because the current chat route remains inside the generic Bloomery topbar/sidebar.

- [ ] **Step 3: Implement the conditional full-screen chat route**

在 `BloomeryAppShell` 中保留初始化和 provider context；当 `activeSection === "chat"` 时直接返回：

```tsx
<div className="bloomery-app bloomery-app-chat">
  <ChatPage onOpenSection={setActiveSection} />
</div>
```

其他模块继续走原有通用壳。这样只改变对话入口，不影响工作台、知识库、数据分析、扩展、设置和诊断。

- [ ] **Step 4: Run all frontend tests**

Run:

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test
npm run build
```

Expected: Vitest 全部通过，TypeScript 与 Vite 构建退出码为 0。

### Task 4: 视觉和可访问性回归

**Files:**
- Modify: `frontend/src/design/polish.css`
- Modify: `frontend/src/app/BloomeryLayout.test.tsx`
- Modify: `frontend/src/features/chat/WebChatWorkspace.test.tsx`

**Interfaces:**
- 支持 1024×720、1440×900、1920×1080 的完整 Web 对话壳。
- 支持深色主题、键盘焦点、按钮可访问名称、`prefers-reduced-motion`。

- [ ] **Step 1: Add style contract assertions**

断言样式包含完整壳的网格、窄屏折叠和深色主题选择器，并确保没有新的 `backdrop-filter` 或额外渐变背景。

- [ ] **Step 2: Run focused and full verification**

Run:

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test -- WebChatWorkspace.test.tsx BloomeryLayout.test.tsx
npm test
npm run build
Set-Location F:/steel-agent/bloomery/src-tauri
cargo check
cargo test
```

Expected: 所有前端测试、前端构建、Rust 检查和 Rust 测试均退出码为 0；Rust 代码无改动时只验证既有桥接行为没有回归。

- [ ] **Step 3: Review the diff**

运行：

```powershell
Set-Location F:/steel-agent/bloomery
git diff --stat
git diff -- frontend/src/features/chat/WebChatWorkspace.tsx frontend/src/features/chat/ChatPage.tsx frontend/src/app/BloomeryApp.tsx frontend/src/design/polish.css
```

检查只包含完整 Web 对话壳所需文件，不删除 Web 端任何内容，不把 `target/`、`dist/`、日志或运行时数据库加入暂存区。

