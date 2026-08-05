# Bloomery

Bloomery 是一个 Windows 优先、本地优先的钢铁与材料领域智能体工作台，使用 Tauri 2、Rust、React 和 SQLite 构建。

当前 `main` 正在按正式发行规格重构。开发构建用于工程验证，不代表可公开分发的稳定版本。

## 产品边界

- 无需注册或登录，启动后直接进入固定的 `local` 工作区。
- 对话、消息、摘要、草稿、记忆和设置保存在本机 SQLite。
- 智能体运行、上下文组装、流式输出和数据访问由 Rust 进程负责。
- 模型与解析服务使用用户自己选择、自己付费并自己保管凭据的 Provider。
- React 只通过单一 Tauri bridge 调用本地命令，不依赖旧 Web 应用或项目作者的私有后端。

## 本地数据

数据库文件名为 `bloomery.sqlite3`，位于操作系统为 Bloomery 分配的应用数据目录。程序不会在仓库目录保存用户会话，也不会创建 `auth-session.json`。

现阶段不自动迁移、上传或删除已有用户数据。后续存储迁移必须保持可回滚并经过版本化测试。

## Provider

正式发行版将支持用户自有 Provider，包括：

- OpenAI-compatible LLM 与本地模型服务；
- SiliconFlow 的 BGE-M3 向量模型与 BGE-Reranker-V2-M3 重排模型；
- MinerU 文档解析服务；
- 可选的本地实现和社区 Provider。

API Key 只允许通过系统凭据库保存。配置文件和 SQLite 不得存储明文密钥。

## 仓库结构

```text
bloomery/
|-- frontend/     React 18、Vite、桌面交互界面与唯一 Tauri bridge
|-- src-tauri/    Tauri 2、Rust Agent、本地 SQLite、Provider 与安全边界
`-- docs/         正式设计规格、实施计划和发布门禁
```

## 开发

环境要求：Windows 10 或更高版本、Node.js 20/22/24、Rust stable，以及 Tauri 2 的 Windows 前置依赖。

```powershell
Set-Location frontend
npm install
npm run test:boundaries
npm run build

Set-Location ../src-tauri
cargo test
cargo check
cargo tauri dev
```

也可以从仓库根目录运行 `start-desktop.bat` 启动开发环境。

## Non-goals

Bloomery 不做以下事情：

- 不复用旧 Web 登录、会话或后端 API；
- 不要求连接项目作者维护的服务器；
- 不托管用户密钥、知识库或聊天记录；
- 不在用户未确认的情况下执行高风险工具；
- 不以“又一个通用聊天客户端”为产品定位。

## License

MIT，详见 `LICENSE`。