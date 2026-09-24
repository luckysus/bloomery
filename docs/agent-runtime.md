# Bloomery Agent Runtime

Bloomery 的桌面智能体由 Rust Runtime 拥有运行事实。React 只消费 `agent-event` 事件并通过 sequence replay 重建视图；`frontend/web-source` 不参与启动或运行。

## 运行边界

- 一个会话只能有一个前台 Turn。
- Turn 开始时固定 provider、model、上下文限制和工具快照。
- Steering 在下一次模型调用前加入当前 Turn；Follow-up 在自然停止点加入同一 Turn。
- 父 Turn 取消会传播到所有子 Turn，子 Turn 继承父会话和工具边界。
- 只读工具可以并行，写工具按串行策略执行；工具超时、权限和错误都进入同一事件流。

## 事件与重连

事件先写入 SQLite，再通过 Tauri `agent-event` 发布。每个 Run 的 `sequence` 单调递增。窗口重连时调用 `replay_agent_run` 并传入最后确认的 sequence；前端 reducer 会丢弃重复事件、缓存缺口事件，补齐后再应用。

## Checkpoint 与恢复

模型调用前、自然回答和模型错误会保存脱敏 checkpoint。应用启动时 `db_init` 检查非终态 Run：可安全恢复的 checkpoint 会继续执行；未确认的权限保留为等待状态；非幂等工具不会自动重复执行，无法安全恢复的 Run 标记为 `interrupted`。

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
