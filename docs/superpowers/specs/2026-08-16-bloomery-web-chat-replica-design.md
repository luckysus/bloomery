# Bloomery Web 风格对话界面复刻设计

## 目标

将 Bloomery 客户端的对话工作区完整迁移为 Web 端智能体对话界面的本地复刻版：Web 已完成的对话布局、视觉样式和主要交互作为唯一标准，不再保留当前 Bloomery 的简化聊天布局。仅替换底层数据、会话、RAG、Provider 和 Agent 调用为 Rust/Tauri 本地实现，同时删除 Web 专属的模式切换、知识库/模型训练/工艺优化入口，以及左下角账户中心。

## 明确保留与删除

保留：

- 会话侧栏、折叠、 新建聊天、搜索聊天、最近会话；
- 会话选择、置顶、重命名、归档/删除等本地会话操作；
- 欢迎状态、用户消息、智能体回答、流式状态和工具进度；
- Markdown、文献/图片引用、思考过程、权限确认、复制、编辑、反馈；
- 输入框、智能检索、本地语音入口、模型选择、发送/停止和图片粘贴；
- Bloomery 本地导出、SQLite 持久化、Rust Agent 事件流和本地 RAG。

删除：

- 左上角“钢铁智能体/多模态检索”模式切换；
- 右上角知识库、模型训练、工艺优化入口；
- 左下角个人中心、账户设置、退出登录；
- Web 登录态、云端会话同步、Web SSE 和 Web API 调用。

## 架构

Web 端组件只作为视觉和交互基准，复制到 Bloomery 后成为独立的客户端实现，不从 Bloomery 构建时导入 `Web/frontend`。

`ChatPage` 继续负责本地会话、草稿、RAG 查询、运行取消和 Agent 事件订阅。新的聊天视图只消费本地化的展示数据和回调。所有跨边界操作都通过 Bloomery 的 Tauri bridge 进入 Rust。

数据映射：

- Web `AgentConversation` → Bloomery `Conversation`；
- Web `AgentMessage` → Bloomery `Message` 与运行中的 `AgentRunView`；
- Web Agent 流式事件 → Bloomery `AgentEventEnvelope`；
- Web 联网搜索状态 → Bloomery 本地知识检索状态；
- Web 云端模型切换 → Bloomery 本地 Provider 默认模型。

## 组件边界

以以下 Web 文件为视觉参考，按 Bloomery 组件边界移植：

- `Web/frontend/src/components/agent/AgentSidebar.tsx`
- `Web/frontend/src/components/agent/AgentChatPanel.tsx`
- `Web/frontend/src/components/agent/AgentAnswerRenderer.tsx`

不移植 `RagAppShell` 的整体实现，因为它包含 Web 的认证、知识库、训练、优化和检索工作区。

Bloomery 侧继续使用：

- `frontend/src/features/chat/ChatPage.tsx`：本地状态和业务流程；
- `frontend/src/features/chat/ChatView.tsx`：Web 风格聊天布局；
- `frontend/src/features/chat/agentEvents.ts`：本地 Agent 运行视图；
- `frontend/src/components/answer/AnswerRenderer.tsx`：本地证据和引用渲染；
- `frontend/src/bridge/desktop.ts`：Tauri 调用与本地会话操作。

## 本地适配要求

前端 bridge 需要暴露 Rust 已存在的会话命令：

- 更新标题；
- 更新置顶状态；
- 归档；
- 删除；
- 历史搜索。

模型选择器必须连接本地 Provider 配置；不能只复制 Web 的下拉菜单外观而不改变实际模型。智能检索按钮保留 Web 的视觉位置，但调用 `queryLocalKnowledge`。语音入口在 Windows 10 能力不可用时必须提供明确的禁用状态。

流式回答必须保留 Web 端“回答逐字出现”的体验。Rust Agent 已经通过 `message_delta`、`message_completed`、`tool_progress` 和 `run_completed` 事件发布运行状态；Bloomery 前端通过 `listenAgentEvents` 接收事件并由 `AgentRunView` 累积回答文本。复刻后的消息区必须直接渲染这个累积文本，不能等待 `desktopAgentChat` 完成后才一次性显示答案。运行结束后再用 SQLite 持久化消息覆盖临时流式消息。

## 视觉验收标准

- Bloomery 与 Web 在相同窗口尺寸下，聊天侧栏、消息区、输入区的结构和间距一致；
- 删除项不留下空白导航位；
- 客户端浅色/深色主题均使用 Bloomery 主题 token，不把 Web 的全局样式污染到其他页面；
- 所有图标按钮具备可访问名称、键盘焦点和禁用状态；
- 本地权限确认、错误、检索证据和流式状态仍能在对话区域正确显示；
- 客户端不发起 Web 登录、云端会话或 `/api/agent/*` 请求。

## 风险与处理

- Web 与 Tauri WebView2 的字体抗锯齿可能造成像素级差异；以布局、尺寸、颜色和交互一致为验收标准；
- Web 的联网搜索、云端附件协议不能直接复用；只复用控件外观，并接入本地能力；
- Web 组件中存在与账户、模式和优化相关的 props，移植时删除这些 props，不使用无操作回调掩盖耦合。
