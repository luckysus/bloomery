<div align="center">

<img src="docs/assets/bloomery-banner.svg" alt="Bloomery" width="760">

<h1>Bloomery</h1>

**Windows 优先、本地优先的钢铁与材料工程智能体工作台。**

简体中文 · [English](README.en.md)

[GitHub](https://github.com/luckysus/bloomery) · [Gitee](https://gitee.com/neusu/bloomery)

[![Bloomery quality](https://github.com/luckysus/bloomery/actions/workflows/quality.yml/badge.svg)](https://github.com/luckysus/bloomery/actions/workflows/quality.yml)
[![Release candidate](https://github.com/luckysus/bloomery/actions/workflows/release.yml/badge.svg)](https://github.com/luckysus/bloomery/actions/workflows/release.yml)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
![Windows](https://img.shields.io/badge/platform-Windows%2010%2B-0078D4)
![Tauri](https://img.shields.io/badge/Tauri-2-FFC131)
![Rust](https://img.shields.io/badge/Rust-stable-b7410e)
![Status](https://img.shields.io/badge/status-engineering%20build-orange)

</div>

> **项目状态**
>
> Bloomery 正在积极开发中。当前 `main` 分支是工程构建，不是已发布的稳定正式版。本 README 描述首个公开正式版的边界和质量目标；只有发布门禁全部通过后才会提供正式安装包。

## 项目简介

Bloomery 是一个 Windows 优先、本地优先的领域智能体工作台，面向钢铁、材料和工业研发场景。它把本地对话、文档解析、混合检索、证据引用、生产数据分析和可控工具执行放在同一个桌面工作区中。

Bloomery 不是又一个通用聊天窗口。它让用户能够：

- 使用自己的模型、Embedding、Reranker 和文档解析服务；
- 将会话、知识库、索引、任务和记忆保存在自己的电脑上；
- 追溯模型回答所依据的文档、页码、表格或源文本；
- 通过 MCP、Skills 和领域包扩展能力；
- 在执行写入、Shell 或其他高风险操作前获得明确的权限确认。

## 核心能力

当前开发版已具备以下基础能力：

- 本地 `local` 工作区，不依赖 Bloomery 账号或项目专属云端服务；模型、Embedding、Reranker 和文档解析服务由用户自行配置；
- Tauri 2 + Rust 驱动的桌面 Agent 主机，React 只通过单一 Tauri bridge 调用本地能力；
- SQLite 本地持久化会话、消息、摘要、草稿、记忆、设置、知识库和后台任务；
- OpenAI-compatible LLM 与 Ollama Provider 配置；
- SiliconFlow Embedding 与 Reranker 配置，默认模型为 `BAAI/bge-m3` 和 `BAAI/bge-reranker-v2-m3`；
- MinerU 文档解析 Provider，以及可恢复的异步解析任务；
- PDF、Markdown、TXT、HTML、DOCX、CSV 和 XLSX 文档导入与本地索引基础；
- 白色浅色工业风桌面界面、首次启动配置流程和 Provider 连接测试。

首个公开正式版的完整目标还包括：

- 混合检索、重排、证据包和页码/工作表/标题级引用闭环；
- 可恢复的 Agent Run、上下文预算、短期与长期记忆、取消和异常恢复；
- Tool-Call Repair、结构化工具协议和自动/确认/危险权限等级；
- MCP stdio、Streamable HTTP 和 SSE 传输；
- 基于 Markdown `SKILL.md` 文件的可扩展 Skills 体系；
- 可校验的领域包机制，以及官方钢铁与材料领域包；
- 生产数据导入、确定性计算、模型推理、预测和受约束优化；
- 会话导出、完整备份恢复、诊断页、更新和 Windows 安装发布流程。

## 产品定位与架构

Bloomery 由两个层次组成：

- **通用智能体核心**：对话、上下文、记忆、Provider、RAG、工具、权限、任务和扩展协议；
- **官方钢铁领域包**：钢种、牌号、标准、工序、缺陷、单位、生产数据和材料工程工作流。

钢铁是第一个官方领域，不会限制社区构建其他工程领域包。

~~~mermaid
flowchart LR
    UI[React UI] --> Bridge[Tauri bridge]
    Bridge --> Runtime[Rust agent runtime]
    Runtime --> Context[Context and memory]
    Runtime --> RAG[Local RAG and citations]
    Runtime --> Tools[Tools and permissions]
    Runtime --> Tasks[Persistent tasks]
    Runtime --> Providers[LLM / SiliconFlow / MinerU]
    Runtime --> Storage[SQLite and local indexes]
    Tools --> MCP[MCP and Skills]
    Runtime --> Domain[Steel domain package]
~~~

关键边界：

- React 负责界面和短生命周期状态；
- Rust 负责 Agent、上下文、Provider、检索、权限、任务和持久化；
- SQLite 是权威数据源，向量索引是可重建的派生数据；
- Provider、工具和领域包都通过受控接口接入；
- 前端不直接调用外部 Provider，也不持有完整 API Key。

## Provider 配置

| 能力 | 默认选择 | 说明 |
| --- | --- | --- |
| LLM | OpenAI-compatible / Ollama | 对话和 Agent 推理；Ollama 可连接本地模型 |
| Embedding | SiliconFlow `BAAI/bge-m3` | 知识库向量化 |
| Reranker | SiliconFlow `BAAI/bge-reranker-v2-m3` | 候选证据重排 |
| 文档解析 | MinerU | 高质量 PDF 和版面解析，可稍后配置 |

SiliconFlow 的免费版与 Pro 版由用户自行选择。Bloomery 不代理计费，也不判断订阅类型；应用只保存用户选择的模型和连接结果。API Key 通过设置页输入并保存到 Windows Credential Manager。SQLite 只保存 Provider 配置和凭据引用，不保存明文密钥。

## 快速开始

环境要求：Windows 10+、Node.js 20/22/24、Rust stable、Visual Studio Build Tools 的 `Desktop development with C++`、WebView2 Runtime、Git 和 Tauri 2 Windows 前置依赖。

~~~powershell
git clone https://github.com/luckysus/bloomery.git
Set-Location bloomery

Set-Location frontend
npm install
npm run test
npm run test:boundaries
npm run build

Set-Location ../src-tauri
cargo check
cargo test
cargo tauri dev
~~~

如果没有 Tauri CLI：

~~~powershell
cargo install tauri-cli --version "^2"
~~~

也可以从仓库根目录运行 `./start-desktop.bat`。它只启动开发环境，不是正式安装包。

## 本地数据、隐私与网络

Bloomery 将 `bloomery.sqlite3` 放在操作系统应用数据目录，而不是仓库目录。会话、消息、摘要、记忆、设置、知识库元数据、任务状态和索引管理信息保存在本机。

Bloomery 是本地优先，不是自动完全离线。云端 LLM、SiliconFlow、MinerU 或 MCP 请求可能离开本机；网络访问应仅来自用户配置的 Provider、启用的扩展和更新检查。API Key 不得出现在源代码、`.env`、日志、截图、导出包或诊断包中。

## 仓库结构

~~~text
bloomery/
|-- frontend/                 React 18、TypeScript、Vite 和桌面界面
|-- src-tauri/
|   |-- src/app/              Tauri 命令和应用装配
|   |-- src/agent/            Agent、上下文、记忆和协议
|   |-- src/providers/        LLM、SiliconFlow 和 MinerU
|   |-- src/rag/              导入、解析、索引、检索和引用
|   |-- src/storage/          SQLite、迁移和凭据引用
|   |-- src/tasks/            可恢复后台任务
|   +-- tests/                Rust 集成与架构测试
|-- docs/                     协议、扩展指南、基准和发布文档
|-- scripts/                  开发辅助脚本
|-- start-desktop.bat         Windows 开发启动入口
|-- LICENSE                   Apache License 2.0
+-- README.md                 默认中文 README
+-- README.en.md              English README
~~~

公开事件协议位于 [`docs/PROTOCOL.md`](docs/PROTOCOL.md)。

更多文档：[`CONTRIBUTING.md`](CONTRIBUTING.md) 贡献指南、[`docs/extensions/mcp.md`](docs/extensions/mcp.md) MCP 扩展、[`docs/extensions/skills.md`](docs/extensions/skills.md) Skills、[`docs/extensions/domain-packages.md`](docs/extensions/domain-packages.md) 领域包、[`docs/releases/building.md`](docs/releases/building.md) 发布构建、[`docs/releases/case-study.md`](docs/releases/case-study.md) 可复现钢铁案例研究。

## 发布路线图

| 阶段 | 状态 |
| --- | --- |
| 基础解耦、SQLite、系统密钥、Provider 和任务持久化 | 开发版已覆盖 |
| 本地知识库、MinerU、混合检索、重排和引用 | 持续完善 |
| 模块化 Agent、事件协议、上下文、记忆、恢复和 Tool-Call Repair | 开发中 |
| 工具权限、MCP、Skills 和领域包 | 发布目标 |
| 官方钢铁领域包、生产数据、计算、预测和优化 | 发布目标 |
| 工作台、诊断、导出、恢复、Windows 安装与正式发布 | 发布目标 |

这些阶段是工程依赖顺序，不代表单独发布的 MVP、Alpha 或 Pro 版本。只有全部发布门禁通过后，Bloomery 才会标记为首个公开正式版。

## 贡献指南

欢迎提交问题、文档、测试和代码贡献。请先阅读 `docs/PROTOCOL.md` 及相关扩展文档，保持单一 Tauri bridge 边界，新 Provider 和工具遵循现有接口与权限策略，并且不要提交 API Key、企业数据、真实用户会话、构建产物或生成目录。

~~~powershell
Set-Location frontend
npm run test
npm run test:boundaries
npm run build

Set-Location ../src-tauri
cargo check
cargo test
~~~

安全问题不要公开发布凭据或生产数据，请通过仓库维护者提供的安全渠道报告漏洞。

## License

Bloomery 使用 Apache License 2.0。完整条款见 [`LICENSE`](LICENSE)，相关声明见 [`NOTICE`](NOTICE)。既往以 MIT 许可发布版本所授予的权利予以保留。
