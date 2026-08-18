# Bloomery 本地 Rust 对话适配执行计划

## 目标

保留 `frontend/web-source/` 中已确认的 Web 对话视觉和交互结构，但让 Bloomery 对话页完全由本地 Rust/Tauri 能力驱动。对话入口不得调用 Web 后端、LangGraph Web 运行时、Web 登录状态或 Web `fetch` API。

## 边界

- 保留：Web 对话页的侧栏、会话列表、历史搜索弹窗、消息渲染、引用/推荐/确认/进度/问题导航、输入框布局和视觉样式。
- 本地化：会话列表与消息、标题/置顶/归档/删除、历史搜索、模型选择、智能搜索、本地知识库检索、Rust agent 流式事件、取消生成、权限确认、导出。
- 移除：对话入口对 Web `useAgentRuntime`、`useSearchMode`、`useRagAppController`、`AgentPage`、`SearchPage` 及 Web API 服务的业务依赖。
- 不改：Web 应用本身、数据库/用户数据、安装包与 Release 流程。

## 执行步骤

1. 审计当前 Bloomery 对话入口、Web 复用组件和本地 Bridge，记录实际 Web API 残留及无效回调。
2. 先增加回归测试，约束对话发送/搜索/RAG/流式事件只能经过本地 Bridge，并验证 Web 页面组件仍正确呈现本地响应。
3. 将复用的展示层收敛为可注入本地 props 的组件，替换 `RagMainContent` 中依赖 Web 控制器的页面路由，保留对话页面的 Web 视觉结构。
4. 清理对话链路中的 Web 运行时导入、Web API hook、空回调和 Web 专属导航动作；保留其他非对话 Web-source 文件作为视觉资产，但不让 Bloomery 对话入口加载它们。
5. 验证本地流程：初始加载、会话切换、历史搜索、发送、知识库检索、事件流式归约、取消、权限确认、模型选择和导出。
6. 运行前端边界测试、对话测试、全量 Vitest、TypeScript/Vite 构建；再运行 Rust `cargo check` 与 `cargo test`。失败时只修复本任务相关问题，不覆盖并发改动。

## 验收条件

- 对话页入口源码不出现 Web `fetch`、`API_BASE`、Web 登录/会话 hook 或 `useAgentRuntime` 的运行时调用。
- 发送消息只调用 `desktop.desktopAgentChat`，本地智能搜索只调用 `desktop.queryLocalKnowledge`，历史搜索只调用 `desktop.searchHistory`。
- `agent-event` 的 `message_delta` 能更新正在生成的回答，完成后本地消息能刷新。
- 侧栏和页面内容仍保留既有 Web 复刻布局，导航不会因旧 Web 页面控制器缺失而卡住。
- 前端测试和构建、Rust check/test 均通过。
