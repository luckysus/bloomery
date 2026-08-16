# Bloomery Web 风格对话界面复刻实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不依赖 Web 运行时的前提下，将 Bloomery 对话界面完整复刻为 Web 端对话工作区的布局、样式、交互和流式回答体验。

**Architecture:** Web 对话组件是唯一视觉标准，Bloomery 保留现有 ChatPage、本地 SQLite、Rust Agent 事件流和 RAG；ChatView 采用 Web AgentSidebar/AgentChatPanel 的完整本地化结构。通过 desktop bridge 适配会话操作和本地 Provider，删除 Web 专属账户、模式和云端功能，并使用 `message_delta` 事件实现逐字流式渲染。

**Tech Stack:** React 18, TypeScript, Vite, Tailwind CSS, lucide-react, React Markdown, Tauri 2, Rust, SQLite, Vitest.

## Global Constraints

- 只修改 Bloomery 独立仓库，不从 `Web/frontend` 运行时导入模块。
- 不修改 Web 端业务行为。
- 保留现有本地 Agent、权限确认、事件回放、RAG、导出和主题能力。
- 不把 Web 登录、Cookie、云端会话同步或 `/api/agent/*` 请求带入 Bloomery。
- 不覆盖工作区中与本任务无关的并行改动。
- 所有代码和测试使用 UTF-8。
- 先写失败测试，再写生产代码。
- `desktopAgentChat` 返回前，`message_delta` 必须已经驱动界面显示增量回答。

### Task 1: 暴露本地会话操作

**Files:**

- Modify: `frontend/src/bridge/desktop.ts`
- Test: `frontend/src/features/chat/ChatPage.test.tsx`

**Interfaces:**

- Produces `desktop.updateConversationTitle`, `desktop.updateConversationPinned`, `desktop.archiveConversation`, `desktop.deleteConversationLocal`, `desktop.searchHistory`.

- [ ] **Step 1: 为本地会话操作写失败测试**

断言聊天侧栏的重命名、置顶、删除回调最终调用对应的 bridge 方法。

- [ ] **Step 2: 运行聊天测试确认失败**

运行：

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test -- ChatPage.test.tsx
```

预期：新增断言因 bridge 方法或视图操作尚未存在而失败。

- [ ] **Step 3: 添加最小 bridge 方法**

使用已有 Rust command 名称：

```ts
updateConversationTitle: (conversationId: string, title: string) =>
  call<void>("update_conversation_title", { conversationId, title }),
updateConversationPinned: (conversationId: string, pinned: boolean) =>
  call<void>("update_conversation_pinned", { conversationId, pinned }),
archiveConversation: (conversationId: string) =>
  call<void>("archive_conversation", { conversationId }),
deleteConversationLocal: (conversationId: string) =>
  call<void>("delete_conversation_local", { conversationId }),
searchHistory: (query: string, limit = 20) =>
  call<HistoryHit[]>("search_history", { query, limit }),
```

同时补齐 `HistoryHit` 类型，字段与 Rust 返回结构一致。

- [ ] **Step 4: 运行测试确认通过**

运行同一条 Vitest 命令，预期聊天测试通过。

- [ ] **Step 5: 提交**

```powershell
git -C F:/steel-agent/bloomery add frontend/src/bridge/desktop.ts frontend/src/features/chat/ChatPage.test.tsx
git -C F:/steel-agent/bloomery commit -m "客户端：接通本地会话操作"
```

### Task 2: 建立 Web 风格聊天视图的失败测试

**Files:**

- Modify: `frontend/src/features/chat/ChatView.tsx`
- Modify: `frontend/src/features/chat/ChatPage.test.tsx`

**Interfaces:**

- `ChatView` 接收本地会话和消息数据；
- `ChatPage` 提供新建、选择、搜索、重命名、置顶、归档、删除、导出和 Agent 回调。

- [ ] **Step 1: 写布局行为测试**

覆盖：

- 渲染“新聊天”“搜索聊天”“最近”；
- 不渲染模式切换、知识库、模型训练、工艺优化、个人中心；
- 会话操作保留本地回调；
- 发送和停止生成仍使用现有回调；
- 权限确认仍出现在消息区域。

- [ ] **Step 2: 运行测试确认失败**

运行：

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test -- ChatPage.test.tsx
```

预期：删除项或 Web 风格结构断言失败。

- [ ] **Step 3: 保持 ChatPage 业务状态，替换 ChatView 布局**

将当前简化侧栏和输入框替换为 Web 风格结构；不复制 Web 云端 hook。使用 `Conversation`、`Message`、`AgentRunView` 和本地回调。

- [ ] **Step 4: 运行测试确认通过**

运行同一条命令，预期聊天页面测试通过。

- [ ] **Step 5: 提交**

```powershell
git -C F:/steel-agent/bloomery add frontend/src/features/chat/ChatView.tsx frontend/src/features/chat/ChatPage.test.tsx
git -C F:/steel-agent/bloomery commit -m "客户端：复刻 Web 对话布局"
```

### Task 3: 接入本地会话操作与 Web 风格交互

**Files:**

- Modify: `frontend/src/features/chat/ChatPage.tsx`
- Modify: `frontend/src/features/chat/ChatView.tsx`
- Modify: `frontend/src/bridge/desktop.ts`
- Modify: `frontend/src/i18n/locale.tsx`
- Test: `frontend/src/features/chat/ChatPage.test.tsx`

**Interfaces:**

- ChatPage 使用 bridge 执行会话操作并刷新本地会话列表；
- ChatView 只通过 props 触发业务，不直接调用 Tauri。

- [ ] **Step 1: 为异步操作补充失败测试**

覆盖操作失败时显示错误、当前会话删除后清空消息、重命名后列表标题更新、置顶后列表顺序更新。

- [ ] **Step 2: 运行测试确认失败**

运行：

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test -- ChatPage.test.tsx
```

- [ ] **Step 3: 实现最小本地适配**

在 ChatPage 中实现操作回调、错误处理和列表刷新；在 ChatView 中移植 Web 侧栏的操作菜单，并移除账户和模式分支。

- [ ] **Step 4: 运行测试确认通过**

运行同一条命令，预期所有聊天行为测试通过。

- [ ] **Step 5: 提交**

```powershell
git -C F:/steel-agent/bloomery add frontend/src/features/chat/ChatPage.tsx frontend/src/features/chat/ChatView.tsx frontend/src/bridge/desktop.ts frontend/src/i18n/locale.tsx frontend/src/features/chat/ChatPage.test.tsx
git -C F:/steel-agent/bloomery commit -m "客户端：接入本地会话交互"
```

### Task 4: 对齐输入区、模型和本地检索

**Files:**

- Modify: `frontend/src/features/chat/ChatView.tsx`
- Modify: `frontend/src/features/chat/ChatPage.tsx`
- Modify: `frontend/src/bridge/desktop.ts`
- Modify: `frontend/src/i18n/locale.tsx`
- Test: `frontend/src/features/chat/ChatPage.test.tsx`

**Interfaces:**

- 输入区保留 Web 的按钮顺序和视觉；
- 智能检索调用 Bloomery 本地 RAG；
- 模型选择显示本地可用 Provider/模型；
- 语音能力不可用时按钮明确禁用。

- [ ] **Step 1: 写模型与检索行为失败测试**

断言提交问题前会使用本地知识库，模型菜单显示本地 Provider，未配置语音时控件不可操作。

- [ ] **Step 2: 运行测试确认失败**

运行：

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test -- ChatPage.test.tsx
```

- [ ] **Step 3: 实现最小适配**

复用现有 `knowledgeBaseIds` 和 `queryLocalKnowledge`；读取已有 Provider bridge，避免添加新的依赖或云端请求。

- [ ] **Step 4: 运行测试确认通过**

运行同一条命令，预期通过。

- [ ] **Step 5: 提交**

```powershell
git -C F:/steel-agent/bloomery add frontend/src/features/chat/ChatView.tsx frontend/src/features/chat/ChatPage.tsx frontend/src/bridge/desktop.ts frontend/src/i18n/locale.tsx frontend/src/features/chat/ChatPage.test.tsx
git -C F:/steel-agent/bloomery commit -m "客户端：对齐本地模型与检索输入区"
```

### Task 5: 验证 Rust 流式回答在 Web 风格消息区中逐字显示

**Files:**

- Modify: `frontend/src/features/chat/ChatPage.tsx`
- Modify: `frontend/src/features/chat/ChatView.tsx`
- Modify: `frontend/src/features/chat/ChatPage.test.tsx`

**Interfaces:**

- `desktop.listenAgentEvents` 产生 `message_delta`；
- `reduceAgentEvent` 累积 `AgentRunView.assistantText`；
- `ChatView` 在 `pendingQuestion` 存在时渲染当前累积回答。

- [ ] **Step 1: 写流式显示失败测试**

在 `desktopAgentChat` 尚未完成时发布两个 `message_delta`，断言消息区先后显示两个增量拼接后的文本，并且停止生成仍可调用取消接口。

- [ ] **Step 2: 运行测试确认失败**

运行：

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test -- ChatPage.test.tsx
```

预期：如果视图仍只显示持久化消息，流式回答断言失败。

- [ ] **Step 3: 使用现有 AgentRunView 渲染增量**

不新增第二套流式状态；沿用 `agentEvents.ts` 的序列校验、乱序缓存和增量累积。仅调整 Web 风格消息布局，使临时 assistant 消息、工具轨迹和权限面板出现在与最终消息相同的内容列中。

- [ ] **Step 4: 运行测试确认通过**

运行同一条命令，预期流式、取消、权限和现有聊天测试全部通过。

- [ ] **Step 5: 提交**

```powershell
git -C F:/steel-agent/bloomery add frontend/src/features/chat/ChatPage.tsx frontend/src/features/chat/ChatView.tsx frontend/src/features/chat/ChatPage.test.tsx
git -C F:/steel-agent/bloomery commit -m "客户端：接入 Web 风格流式回答"
```

### Task 6: 视觉主题和回归验证

**Files:**

- Modify: `frontend/src/design/polish.css`
- Modify: `frontend/src/design/tokens.css`
- Modify: `frontend/src/index.css`
- Test: `frontend/src/features/chat/ChatPage.test.tsx`

- [ ] **Step 1: 写主题回归测试**

覆盖浅色/深色主题下聊天区域、侧栏、输入框和消息状态均使用 Bloomery 主题 token。

- [ ] **Step 2: 运行测试确认失败**

运行：

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test
```

- [ ] **Step 3: 最小化调整 CSS**

只调整聊天相关选择器，保留现有其他页面主题和响应式规则；不引入新的 UI 库。

- [ ] **Step 4: 运行完整验证**

```powershell
Set-Location F:/steel-agent/bloomery/frontend
npm test
npm run build
npm run test:boundaries
git diff --check
```

- [ ] **Step 5: 提交**

```powershell
git -C F:/steel-agent/bloomery add frontend/src/design/polish.css frontend/src/design/tokens.css frontend/src/index.css frontend/src/features/chat/ChatPage.test.tsx
git -C F:/steel-agent/bloomery commit -m "客户端：完成 Web 风格对话主题适配"
```
