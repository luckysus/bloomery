# Open Vetta 风格 Agent Loop 重写实施计划

> 本计划针对 Bloomery 的桌面智能体 Runtime。它以 Open Vetta 源码和运行时设计为参照，使用 Bloomery 当前的 Rust、Tauri 和 React 技术栈重新实现，不保留旧 Agent API 兼容层，也不把 Web 页面当作桌面智能体入口。

## 目标

- 在 Bloomery 中建立由 Rust 拥有的正式 Agent Runtime。
- 复刻 Open Vetta 的核心运行语义：Turn、模型调用、工具轮次、Steering、Follow-up、Checkpoint、恢复、事件流和子 Agent。
- 保留 Bloomery 已有的 Provider、Agent 证据结构、权限系统、Tauri 桌面壳和本地数据边界。
- 让前端只消费统一的 Agent 事件投影，不直接维护第二套运行状态。

## 参照原则

- Open Vetta 的 `contextWindow`、`maxTokens`、`transformContext`、context checkpoint、message queue 和 runtime ownership 是设计参照。
- Open Vetta 的 TypeScript 实现不直接复制到 Bloomery；领域逻辑和运行时核心使用 Rust 重写。
- RAG 是 Agent 的一个工具和证据来源，不替代 Agent Loop。
- 知识库 PostgreSQL 计划与本计划并行，二者通过工具、证据和来源接口连接。
- 每小时统一提交并同时推送 GitHub、Gitee，不按单个功能完成即时推送。

## 阶段总览

本计划是“阶段 1 加后续 6 个阶段”，共 7 个阶段。

| 阶段 | 名称 | 当前状态 |
| --- | --- | --- |
| 1 | Agent Loop 内核与 RuntimeHost | 已完成 |
| 2 | 上下文预算、Checkpoint 与恢复 | 已完成 |
| 3 | 事件协议、持久化状态与 Replay | 已完成 |
| 4 | ToolRegistry、工具快照与 MCP 生命周期 | 已完成 |
| 5 | 子 Agent、并发、取消与权限继承 | 已完成 |
| 6 | 桌面前端事件投影、重连与控制 UI | 已完成 |
| 7 | 全量验证、清理、文档与发布 | 已完成 |

## 阶段 1：Agent Loop 内核与 RuntimeHost

### 目标

建立一次前台 Agent Turn 的唯一运行边界，让模型调用、工具调用、取消、权限、Steering 和 Follow-up 都由 RuntimeHost 管理。

### 实施内容

- 模型调用与工具调用轮次。
- Turn deadline、取消信号和终止状态。
- Runtime-owned Steering/Follow-up 输入队列。
- 同一 Session 禁止并发前台 Turn。
- Turn 开始时固定 provider、model、工具和限制快照。
- Tauri 命令：`steer_agent_run`、`follow_up_agent_run`。
- 子 Agent 的基础工具调用上限。

### 主要文件

- `src-tauri/src/agent/runtime/host.rs`
- `src-tauri/src/agent/runtime/loop.rs`
- `src-tauri/src/agent/runtime/loop/execution.rs`
- `src-tauri/src/app/desktop_agent_runtime.rs`
- `src-tauri/src/app/agent_commands.rs`

### 完成标准

- 同一 Session 不产生两个活动前台 Turn。
- 取消、deadline 和工具失败都能产生明确终态。
- Steering 会在下一个模型调用前进入同一 Turn。
- Follow-up 只在自然停止点继续执行。
- 每次模型调用都使用启动时确定的模型和工具快照。

## 阶段 2：上下文预算、Checkpoint 与恢复

### 目标

采用 Open Vetta 的“宿主在每次模型调用前准备上下文”设计，区分 provider-facing context 和 Runtime 内部完整历史。

### 实施内容

- 读取模型 `contextWindow`，未知时使用明确的保守默认值。
- 预算拆分为系统提示、工具 Schema、历史消息、当前请求、输出预留和推理预留。
- 按完整消息组裁剪，不能拆开 assistant tool call 与 tool result。
- 旧消息压缩和 provider 请求视图裁剪。
- `model_call`、`assistant_result`、`assistant_error` checkpoint。
- checkpoint 持久化、失败保护和最大恢复次数。
- 从 checkpoint 重新构造 Turn 并继续执行。
- 图片和敏感凭据不得进入 checkpoint。

### 主要文件

- `src-tauri/src/agent/runtime/loop/helpers.rs`
- `src-tauri/src/agent/runtime/persistence.rs`
- `src-tauri/src/agent/runtime/recovery.rs`
- `src-tauri/src/storage/repositories/checkpoints.rs`
- `src-tauri/src/storage/migrations/0028_agent_checkpoints.sql`

### 完成标准

- provider 请求不会超过模型窗口或能进入明确的恢复路径。
- 内部历史和本次请求上下文视图边界清楚。
- 模型错误、应用重启和进程中断后可以从合法 checkpoint 继续。
- checkpoint 大小、超时、恢复次数和脱敏规则有测试覆盖。

## 阶段 3：事件协议、持久化状态与 Replay

### 目标

让 Agent 的事实来源成为有序、可持久化、可重放的事件流，前端断线后能够从 sequence 继续恢复。

### 实施内容

- 统一 `agent-event` 事件类型和版本字段。
- 每个 Run 使用单调递增 sequence。
- 事件先落盘，再向 Tauri 前端发布。
- Run、Turn、模型调用、工具调用、权限、checkpoint 和恢复事件统一关联。
- 前端按 sequence replay，去重重复事件。
- 重启后从最后确认的 sequence 继续推送。
- 终态事件只能提交一次。

### 主要文件

- `src-tauri/src/agent/protocol/`
- `src-tauri/src/storage/repositories/runs.rs`
- `src-tauri/src/storage/repositories/events.rs`
- `src-tauri/src/app/desktop_stream.rs`
- `frontend/src/bridge/`

### 完成标准

- 丢失前端事件不改变 Runtime 事实状态。
- 相同 sequence 重放不会重复渲染消息或工具结果。
- 页面刷新、Tauri 窗口重连和应用重启都能恢复活动 Run 的可见状态。
- 协议生成文件和 Rust 定义保持一致。

## 阶段 4：ToolRegistry、工具快照与 MCP 生命周期

### 目标

把所有工具纳入 RuntimeHost 控制面，工具的能力、风险和生命周期对每个 Turn 可审计、可取消、可恢复。

### 实施内容

- 正式 ToolRegistry 和不可变 ToolSnapshot。
- 工具属性：只读、幂等、风险等级、是否可重试、超时策略。
- 工具 Schema 和版本锁定。
- 并行只读工具与串行写工具调度。
- MCP server 初始化、能力发现、调用、取消、超时和关闭。
- MCP 工具与内置工具使用同一权限、事件和错误模型。
- 工具输出上限、Artifact 引用和敏感信息过滤。

### 完成标准

- Turn 中途工具注册变化不会悄悄改变当前模型调用的工具集合。
- 危险工具默认经过权限决策。
- 幂等工具可以按策略恢复，非幂等工具不会自动重复执行。
- MCP server 关闭后不会留下悬挂任务或活动句柄。

## 阶段 5：子 Agent、并发、取消与权限继承

### 目标

建立 Open Vetta 风格的 parent/child Turn 关系，使子 Agent 成为可观察、可取消、受权限边界约束的正式运行单元。

### 实施内容

- parent Turn、child Turn 和任务身份模型。
- 子 Agent 的独立上下文、模型快照和事件关联。
- 父 Turn 取消时向子 Turn 传播取消信号。
- 权限边界不能由子 Agent 提升。
- 子 Agent 并发上限和工具并发上限。
- 子 Agent 结果、证据和错误回传父 Turn。
- 子 Agent 重启恢复和孤儿 Turn 清理。

### 完成标准

- 父子 Turn 可以分别查询、取消和重放。
- 父 Turn 结束后不会留下未受管控的子任务。
- 子 Agent 无法绕过父 Turn 的工具权限和工作区边界。
- 并发和失败行为有确定的事件顺序与终态。

## 阶段 6：桌面前端事件投影、重连与控制 UI

### 目标

让 React 只通过统一 Agent 事件 reducer 显示桌面智能体状态，完整支持控制和断线恢复。

### 实施内容

- 移除独立的 `desktop-agent-delta` 状态通道。
- 所有消息、工具、权限、checkpoint、恢复和终态都进入 `agent-event` reducer。
- 活动 Turn、工具状态、权限请求、失败和恢复状态显示。
- Steering、Follow-up、Retry、Resume、Cancel 控制。
- 断线后的 sequence replay 和重连。
- 桌面窗口启动时恢复活动 Run，而不是重新创建网页会话。
- 保持 Tauri 桌面入口，不回退到 `frontend/web-source`。

### 完成标准

- 页面刷新或窗口重连不会丢失已持久化事件。
- UI 不再维护与 Runtime 相冲突的第二份 Agent 状态。
- 用户能够从界面取消、转向、继续、重试和恢复运行。
- 桌面启动后打开的是桌面智能体工作区，而不是嵌入式 Web 页面。

## 阶段 7：全量验证、清理、文档与发布

### 目标

完成正式实现的验证和收口，删除阶段性 API、无用缓存和冗余代码，形成可维护的桌面智能体版本。

### 实施内容

- Rust 单元、协议、Runtime、工具、MCP、恢复和集成测试。
- 前端类型检查、构建、事件 reducer 和重连测试。
- Tauri 启动、取消、重启恢复和桌面入口 smoke test。
- 检查协议生成文件、迁移顺序和旧 API 引用。
- 删除已经没有引用的兼容接口、临时文件和构建缓存。
- 更新 Agent Runtime、桌面启动、开发和故障排查文档。
- 通过 `cargo check`、`cargo test`、前端构建和相关 Actions。
- 到每小时节点统一提交，并同时推送 GitHub、Gitee；推送后检查 Actions。

### 完成标准

- 不存在未迁移的旧 Agent Loop 入口或双重事件状态通道。
- 完整测试和构建通过。
- 桌面入口、Runtime、前端和恢复链路可以从干净环境验证。
- GitHub、Gitee 两个远程仓库内容一致，Actions 失败项已处理。

## 当前执行位置

- 阶段 1：RuntimeHost、Turn 快照、取消、deadline、Steering 和 Follow-up 已由 Rust 统一拥有。
- 阶段 2：上下文预算、脱敏 checkpoint、恢复次数限制和启动恢复已完成。
- 阶段 3：`agent-event` 持久化、单调 sequence、replay、前端 reducer 去重和缺口补齐已完成。
- 阶段 4：ToolRegistry 校验、不可变工具快照、风险/并发/超时/幂等元数据和 MCP 工具纳入同一执行边界已完成。
- 阶段 5：父子 Turn、并发上限、取消传播、权限继承、子事件持久化和孤儿 Turn 中断已完成。
- 阶段 6：桌面事件投影、断线 replay、Steering、Follow-up、Cancel、Resume 和 Retry 控制已完成。
- 阶段 7：全量测试、旧入口引用检查、文档同步和构建缓存清理已完成。

## 不属于本计划的内容

- 不迁移旧 SQLite 知识库兼容接口。
- 不把 RAG 重新设计成 Agent Loop。
- 不恢复 `frontend/web-source`。
- 不删除数据库、数据库文件、原始用户文件或知识库数据。
