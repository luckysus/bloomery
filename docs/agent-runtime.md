# Bloomery Agent Runtime

Bloomery 的桌面智能体由 Rust Runtime 拥有运行事实。React 只消费 `agent-event` 事件并通过 sequence replay 重建视图；`frontend/web-source` 不参与启动或运行。

## 运行边界

- 一个会话只能有一个前台 Turn。
- Turn 开始时固定 provider、model、上下文限制和工具快照。
- Steering 在下一次模型调用前加入当前 Turn；Follow-up 在自然停止点加入同一 Turn。
- 父 Turn 取消会传播到所有子 Turn，子 Turn 继承父会话和工具边界。
- 只读工具可以并行，写工具按串行策略执行；工具超时、权限和错误都进入同一事件流。

子 Agent 的 `task` 工具结果会把 `child_turn_id`、最终 `outcome`、`conclusion`、
`evidence` 和 `errors` 一起交给父 Turn。子 Turn 的完整事件仍按独立 sequence
写入 child-turn 存储，因此父 Agent 可以先使用摘要继续推理，桌面端也可以按 ID
查询、重放或取消子 Turn；子模型失败时，父工具错误会保留同一份结构化详情。

## 上下文预算

每次模型调用都重新生成 provider-facing context。Provider 声明的
`context_window` 优先使用；未知时采用保守的 `8192` token 默认值。默认桌面
Turn 预留 `2048` token 给可见回答、`1024` token 给 reasoning，剩余 `5120`
token 才用于系统提示、工具 Schema、当前请求和历史消息。发给 Provider 的
`max_tokens` 是两项输出预留之和，旧 Turn snapshot 缺少 reasoning 字段时按
`1024` 恢复。内部完整历史不会因 provider-facing context 的裁剪而丢失。

## 事件与重连

事件先写入 SQLite，再通过 Tauri `agent-event` 发布。每个 Run 的 `sequence` 单调递增。窗口重连时调用 `replay_agent_run` 并传入最后确认的 sequence；前端 reducer 会丢弃重复事件、缓存缺口事件，补齐后再应用。

## Checkpoint 与恢复

模型调用前、自然回答和模型错误会保存脱敏 checkpoint。每次恢复领取都有唯一
`recovery_id`；同一进程的重复领取会被当前租约拦住，租约超过 10 分钟后可接管，
且旧进程迟到的完成事件不会关闭新租约。应用启动时 `db_init` 检查非终态 Run：
可安全恢复的 checkpoint 会继续执行；未确认的权限保留为等待状态；非幂等工具不会
自动重复执行，无法安全恢复的 Run 标记为 `interrupted`。

桌面控制栏提供 `Steer`、`Follow-up`、`Cancel`、`Resume` 和 `Retry`。Retry 会用同一用户消息创建新的 Run；Resume 重新触发 checkpoint 恢复并继续通过事件流更新界面。

## 本地启动

在仓库根目录运行 `start-desktop.bat`。脚本启动 Tauri 桌面壳，入口是本地桌面智能体工作区。开发验证可以分别运行：

```powershell
Set-Location F:\bloomery\frontend
npm test -- --run
npm run build

Set-Location F:\bloomery\src-tauri
cargo check
cargo test
```

`target/` 和 `frontend/dist/` 只属于构建产物，不应加入 Git；本地运行数据库、原始文件和凭据存储不属于清理范围。
