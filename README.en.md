<div align="center">

![Bloomery](docs/assets/bloomery-banner.png)

<h1>Bloomery</h1>

**A Windows-first, local-first agent workbench for steel and materials engineering.**

[简体中文](README.md) · English

[GitHub](https://github.com/luckysus/bloomery) · [Gitee](https://gitee.com/neusu/bloomery)

[![Quality checks](https://github.com/luckysus/bloomery/actions/workflows/quality.yml/badge.svg)](https://github.com/luckysus/bloomery/actions/workflows/quality.yml)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
![Platform](https://img.shields.io/badge/platform-Windows%2010-0078D4)
![Desktop framework](https://img.shields.io/badge/Tauri-2-FFC131)
![Language](https://img.shields.io/badge/Rust-stable-b7410e)
![Status](https://img.shields.io/badge/status-in%20development-orange)

</div>

> **Project status**
>
> Bloomery is in the engineering phase before its public release. The `main` branch provides runnable development builds, not a published stable installer.

## Built for materials engineering

Bloomery is a desktop agent workbench for steel, materials, and industrial R&D. It brings local conversations, document parsing, retrieval with citations, production-data analysis, and controlled tool execution into one workspace.

It is not a generic chat shell. Use your own providers, keep conversations, knowledge bases, indexes, tasks, and memories on your own computer, and trace conclusions back to source documents, pages, tables, or data.

## Core capabilities

- **Local workspace**: open the client directly into the workbench; configure models and retrieval services in Settings when needed, without registering a Bloomery account.
- **Your choice of models**: OpenAI-compatible APIs and Ollama are supported; embedding, reranking, and parsing services are configured by the user.
- **Local knowledge base**: import PDF, Markdown, TXT, HTML, DOCX, CSV, and XLSX to build local retrieval and evidence citations.
- **Context and memory**: organize the agent's work around conversations, tasks, summaries, drafts, and long-term memory.
- **Steel and materials workflows**: support production-data import, deterministic calculations, model inference, prediction, and constrained optimization.
- **Controlled extensibility**: extend with MCP, Markdown Skills, and domain packages; high-risk actions such as file writes and shell execution require explicit confirmation.

## Provider setup

| Capability | Recommended start | Purpose |
| --- | --- | --- |
| LLM | OpenAI-compatible / Ollama | Conversations and agent reasoning |
| Embedding | SiliconFlow `BAAI/bge-m3` | Knowledge-base vectorization |
| Reranker | SiliconFlow `BAAI/bge-reranker-v2-m3` | Retrieval reranking |
| Document parsing | MinerU | PDF and complex-layout parsing |

Users choose SiliconFlow Free or Pro themselves. API keys are entered in Settings and stored in Windows Credential Manager; the local database stores provider configuration and secret references, never plaintext keys.

## Quick start

The development environment requires Windows 10, Node.js 20/22/24, Rust stable, Visual Studio Build Tools with `Desktop development with C++`, WebView2 Runtime, Git, and the Tauri 2 Windows prerequisites.

```powershell
git clone https://github.com/luckysus/bloomery.git
Set-Location bloomery/frontend
npm install
npm run build

Set-Location ../src-tauri
cargo tauri dev
```

If the Tauri CLI is not installed:

```powershell
cargo install tauri-cli --version "^2"
```

You can also run `./start-desktop.bat` from the repository root to start the development environment.

## Local data, privacy, and network

Bloomery stores `bloomery.sqlite3` in the operating system application-data directory rather than in the repository. Conversations, messages, summaries, memories, settings, knowledge metadata, and task state remain local by default.

Local-first does not mean automatically offline. When you enable a cloud LLM, SiliconFlow, MinerU, or MCP, related requests are sent to the service you configured or enabled. The current version does not include in-app automatic updates; users install newer Windows 10 builds manually.

## Extensions and documentation

- [Contributing](CONTRIBUTING.md)
- [Event protocol](docs/PROTOCOL.md)
- [MCP extensions](docs/extensions/mcp.md)
- [Skills extensions](docs/extensions/skills.md)
- [Domain packages](docs/extensions/domain-packages.md)
- [Steel case study](docs/releases/case-study.md)
- [Security policy](SECURITY.md)

## Contributing

Issues, documentation, tests, and code contributions are welcome. Read the [contribution guide](CONTRIBUTING.md) first, and do not submit API keys, enterprise data, real user sessions, build artifacts, or generated directories.

## License

Bloomery is released under the Apache License 2.0. See [LICENSE](LICENSE) for the full text and [NOTICE](NOTICE) for attribution.
