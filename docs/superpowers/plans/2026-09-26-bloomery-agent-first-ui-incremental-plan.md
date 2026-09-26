# Bloomery Agent-first UI 增量开发计划

## 目标

参考两张 Suna UI 图和 `开发文档.md` 的信息架构，把 Bloomery 继续收敛成 Agent-first 的 Windows 桌面智能体。保留现有 Agent Loop、RAG、知识库、分析、数据库、MCP、Skill、设置和诊断能力；只调整入口、信息组织和运行状态呈现，不复制参考产品的代码或页面。

## 当前基线

### 已有并继续复用

| 能力 | 当前实现 | 继续策略 |
| --- | --- | --- |
| Agent Run 事实 | Rust Runtime、SQLite 运行记录、事件 sequence | 不改运行事实，前端只消费事件 |
| 流式回答 | `agent-event`、`agentEvents` reducer、Markdown 渲染 | 保留消息和流式逻辑 |
| 控制 | 停止、Steering、Follow-up、Retry、Resume | 保留现有 Tauri 命令和控制器 |
| 工具与权限 | ToolRegistry、MCP、脱敏、权限决策 | 继续在消息和检查器中投影 |
| 子 Agent | Rust child turn 查询、重放和取消命令 | 先接入检查器，再补充树状详情 |
| 证据与 RAG | evidence pack、citation、PostgreSQL 检索 | 保留现有 citation 和来源定位 |
| 业务模块 | knowledge、analysis、databases、extensions、settings、diagnostics | 作为 Agent 上下文和独立页面保留 |
| 前端技术栈 | React 19、TypeScript 6、Vite 7、Tailwind CSS 4 | 不回退开发文档中的旧版本 |

## 当前差距

1. `BloomeryApp` 默认进入工作台，目标是最近会话或新任务输入区。
2. `DesktopChatWorkspace` 当前只有会话栏和主对话区，CSS 已预留三栏尺寸，但缺少真正的右侧运行检查器。
3. 工具、权限、上下文预算、checkpoint、恢复、引用和错误已经有数据，尚未在同一侧栏中集中呈现。
4. 业务导航仍是一级模块列表，后续需要把知识、数据和扩展更多地作为当前任务的上下文入口，同时保留独立页面。
5. `开发文档.md` 写的是 React 18/Tailwind CSS，实施时以仓库实际的 React 19/Tailwind 4 为准。

## 增量阶段

### 阶段 1：运行检查器（当前实施）

- 在现有聊天三栏网格中加入 `AgentRunInspector`。
- 展示当前 Run 状态、sequence、工具数、token 使用、任务进度、工具调用、权限请求、checkpoint、恢复信息、引用编号和错误摘要。
- 复用 `AgentRunView`、`PermissionDecision`、现有重试/恢复/授权回调。
- Run 为空时显示安静的空状态；不新增后端状态源。
- 验收：事件流更新时检查器同步更新，权限按钮仍调用原命令，聊天现有测试不回归。

### 阶段 2：Agent-first 启动和壳层

- 保留工作台路由和组件，把默认选择改为最近会话、可恢复 Run 或新任务。
- 将顶部一级模块导航降为低频入口，聊天页承担默认入口。
- 侧栏补充项目/工作区分组、活动 Run 和设置入口；现有会话搜索、置顶、归档、删除继续复用。
- 在最小窗口和 125% 缩放下验证布局。

### 阶段 3：消息时间线与任务抽屉

- 在回答附近增加工具调用摘要、思考摘要折叠和子 Agent 摘要卡。
- 新增任务抽屉/命令面板，统一显示对话 Run、知识导入、索引和分析任务，并可返回来源会话。
- 所有状态继续由 Rust 事件或已有任务查询提供，前端不维护第二套运行事实。

### 阶段 4：上下文入口和来源面板

- 在输入区加入项目、知识库、文件、数据集、MCP/Skill 上下文选择器。
- 复用现有附件、`smartSearchEnabled`、PostgreSQL 知识库和 citation 命令。
- 右侧来源页签提供证据包、文档位置、Wiki 页面和数据范围入口。

### 阶段 5：业务模块对齐

- 将知识中心、文献研究、数据实验室、性能预测、工艺优化、实验助手、Agent 管理、工具中心映射到现有 `knowledge`、`analysis`、`extensions`、`settings` 和新增薄 UI 页面。
- 页面只负责表单、结果和跳转；计算、任务和权限继续走 Rust。
- 不删除现有数据库、诊断和知识库页面。

### 阶段 6：命令、恢复和可用性

- 增加 Ctrl/Cmd+K、Ctrl/Cmd+N、Ctrl/Cmd+Shift+P、Ctrl/Cmd+F、Esc 快捷键。
- 补齐启动恢复、断线 replay、通知和错误操作入口。
- 验证键盘导航、减少动效、Windows 10 100%/125% 缩放和 1180×720 最小窗口。

### 阶段 7：验证与发布

- 运行前端类型检查、单元测试、边界测试和构建；运行 `cargo check`、`cargo test`。
- 检查现有 RAG、知识库、数据库、MCP、Agent Loop 测试未被 UI 改造破坏。
- 按既定策略每小时统一推送 GitHub 和 Gitee，不按单个功能频繁推送。

## 第一批修改文件

- `frontend/src/features/chat/AgentRunInspector.tsx`：新增右侧检查器。
- `frontend/src/features/chat/DesktopChatWorkspace.tsx`：挂载检查器，复用现有回调。
- `frontend/src/design/polish.css`：只在需要时补充检查器交互样式；已有样式优先复用。
- `frontend/src/features/chat/AgentRunInspector.test.tsx`：覆盖空状态、工具状态、权限操作和 usage 展示。

## 明确保留

- `WorkbenchHome` 和工作台导航。
- `KnowledgePage`、`AnalysisPage`、`DatabasePage`、`ExtensionsPage`、`SettingsPage`、`DiagnosticsPage`。
- Rust Agent Runtime、PostgreSQL 知识库、SQLite 客户端状态和原始文件资产。
- 现有测试、协议生成文件和权限边界。
- 用户提供的 `开发文档.md`，不删除、不覆盖。

## 不在本轮做

- 不嵌入 NexusPilot。
- 不删除旧页面或后端模块。
- 不改产品名、Logo 或整体品牌色；先完成结构和运行状态，再单独评审视觉主题。
- 不新增一套独立的 Agent 状态存储。
