<div align="center">

<img src="docs/assets/bloomery-banner.svg" alt="Bloomery" width="760">

<h1>Bloomery</h1>

**Windows 优先、本地优先的钢铁与材料工程智能体工作台。**

简体中文 · [English](README.en.md)

[GitHub](https://github.com/luckysus/bloomery) · [Gitee](https://gitee.com/neusu/bloomery)

[![质量检查](https://github.com/luckysus/bloomery/actions/workflows/quality.yml/badge.svg)](https://github.com/luckysus/bloomery/actions/workflows/quality.yml)
![许可证](https://img.shields.io/badge/license-Apache--2.0-blue)
![平台](https://img.shields.io/badge/platform-Windows%2010%2B-0078D4)
![桌面框架](https://img.shields.io/badge/Tauri-2-FFC131)
![语言](https://img.shields.io/badge/Rust-stable-b7410e)
![状态](https://img.shields.io/badge/status-开发中-orange)

</div>

> **项目状态**
>
> Bloomery 正处于公开发布前的工程开发阶段。`main` 分支提供可运行的开发构建，但不是已发布的稳定版安装包。

## 为材料工程而生

Bloomery 是面向钢铁、材料和工业研发场景的桌面智能体工作台。它将本地对话、文档解析、检索与引用、生产数据分析和受控工具执行整合到同一个工作区。

它不是通用聊天壳：你可以使用自己的模型和服务，把会话、知识库、索引、任务与记忆保留在自己的电脑上，并让每个结论尽可能回到原始文档、页码、表格或数据来源。

## 核心能力

- **本地工作区**：首次启动后即可开始配置和使用，不要求注册 Bloomery 账号。
- **用户自选模型**：支持 OpenAI 兼容接口与 Ollama；Embedding、重排和文档解析服务由用户自行配置。
- **本地知识库**：导入 PDF、Markdown、TXT、HTML、DOCX、CSV 与 XLSX，构建本地检索与证据引用。
- **上下文与记忆**：围绕会话、任务、摘要、草稿和长期记忆组织 Agent 的工作上下文。
- **钢铁与材料工作流**：覆盖生产数据导入、确定性计算、模型推理、预测和约束优化等场景。
- **可控扩展**：通过 MCP、Markdown Skills 和领域包扩展能力；写入文件、执行 Shell 等高风险操作需要明确确认。

## Provider 配置

| 能力 | 推荐起点 | 用途 |
| --- | --- | --- |
| LLM | OpenAI-compatible / Ollama | 对话与 Agent 推理 |
| Embedding | SiliconFlow `BAAI/bge-m3` | 知识库向量化 |
| Reranker | SiliconFlow `BAAI/bge-reranker-v2-m3` | 检索结果重排 |
| 文档解析 | MinerU | PDF 与复杂版面解析 |

SiliconFlow 的免费版和 Pro 版由用户自行选择。API Key 通过设置页保存到 Windows Credential Manager；本地数据库只保存 Provider 配置与凭据引用，不保存明文密钥。

## 快速开始

开发环境需要 Windows 10+、Node.js 20/22/24、Rust stable、Visual Studio Build Tools（`Desktop development with C++`）、WebView2 Runtime、Git 与 Tauri 2 的 Windows 前置依赖。

```powershell
git clone https://github.com/luckysus/bloomery.git
Set-Location bloomery/frontend
npm install
npm run build

Set-Location ../src-tauri
cargo tauri dev
```

如果尚未安装 Tauri CLI：

```powershell
cargo install tauri-cli --version "^2"
```

也可以从仓库根目录运行 `./start-desktop.bat` 启动开发环境。

## 本地数据、隐私与网络

Bloomery 将 `bloomery.sqlite3` 存放在操作系统应用数据目录，而不是仓库目录。会话、消息、摘要、记忆、设置、知识库元数据和任务状态默认保留在本机。

“本地优先”不等于自动离线：当你启用云端 LLM、SiliconFlow、MinerU、MCP 或更新检查时，相应请求会发送到你配置或启用的服务。请勿将 API Key 写入源码、`.env`、日志、截图、导出包或诊断包。

## 扩展与文档

- [贡献指南](CONTRIBUTING.md)
- [事件协议](docs/PROTOCOL.md)
- [MCP 扩展](docs/extensions/mcp.md)
- [Skills 扩展](docs/extensions/skills.md)
- [领域包](docs/extensions/domain-packages.md)
- [钢铁案例研究](docs/releases/case-study.md)
- [安全策略](SECURITY.md)

## 贡献

欢迎提交 Issue、文档、测试与代码。请先阅读 [贡献指南](CONTRIBUTING.md)，并不要提交 API Key、企业数据、真实用户会话、构建产物或生成目录。

## 许可证

Bloomery 使用 Apache License 2.0。完整条款见 [LICENSE](LICENSE)，相关声明见 [NOTICE](NOTICE)。
