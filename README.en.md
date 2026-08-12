<div align="center">

<img src="docs/assets/bloomery-banner.svg" alt="Bloomery" width="760">

<h1>Bloomery</h1>

**Windows-first, local-first agent workbench for steel and materials engineering.**

[简体中文](README.md) · English

[Guide](#quick-start) · [Protocol](docs/PROTOCOL.md) · [Extensions](docs/extensions/mcp.md) · [Release](docs/releases/building.md) · [Case study](docs/releases/case-study.md) · [Contributing](CONTRIBUTING.md)

[GitHub](https://github.com/luckysus/bloomery) · [Gitee](https://gitee.com/neusu/bloomery) · [Non-goals](docs/NON-GOALS.md) · [Security](SECURITY.md)

[![Bloomery quality](https://github.com/luckysus/bloomery/actions/workflows/quality.yml/badge.svg)](https://github.com/luckysus/bloomery/actions/workflows/quality.yml)
[![Release candidate](https://github.com/luckysus/bloomery/actions/workflows/release.yml/badge.svg)](https://github.com/luckysus/bloomery/actions/workflows/release.yml)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)
![Windows](https://img.shields.io/badge/platform-Windows%2010%2B-0078D4)
![Tauri](https://img.shields.io/badge/Tauri-2-FFC131)
![Rust](https://img.shields.io/badge/Rust-stable-b7410e)
![Status](https://img.shields.io/badge/status-engineering%20build-orange)

</div>

> **Project status**
>
> Bloomery is under active development. The current `main` branch is an engineering build, not a published stable release. This README describes the boundary and quality bar of the first public release; release installers will be added only after the release gates are met.

## Overview

Bloomery is a Windows-first, local-first desktop agent workbench for steel, materials, and industrial engineering workflows. It combines local conversations, document parsing, hybrid retrieval, evidence citations, production-data analysis, and controlled tool execution in one desktop workspace.

Bloomery is not another generic chat window. It helps users:

- use their own LLM, embedding, reranker, and document parsing providers;
- keep conversations, knowledge bases, indexes, tasks, and memories on their own computer;
- trace model answers back to documents, pages, tables, or source text;
- extend the workbench through MCP, Skills, and domain packages;
- require explicit confirmation before write, shell, or other high-risk tool actions.

## Core capabilities

The current development build includes:

- a local `local` workspace without a Bloomery account, login page, or author-operated backend;
- a Tauri 2 + Rust desktop agent host, with React using a single Tauri bridge;
- SQLite persistence for conversations, messages, summaries, drafts, memories, settings, knowledge data, and background tasks;
- OpenAI-compatible LLM and Ollama provider profiles;
- SiliconFlow embedding and reranking profiles, defaulting to `BAAI/bge-m3` and `BAAI/bge-reranker-v2-m3`;
- MinerU document parsing provider support and recoverable async parsing tasks;
- local ingestion and indexing foundations for PDF, Markdown, TXT, HTML, DOCX, CSV, and XLSX;
- a white light industrial desktop UI, first-run setup, and provider connection checks.

The first complete public release additionally targets:

- hybrid retrieval, reranking, evidence packs, and page/sheet/heading-level citations;
- recoverable Agent Runs, context budgets, short- and long-term memory, cancellation, and failure recovery;
- tool-call repair, a structured tool protocol, and automatic/confirmation/dangerous permission levels;
- MCP stdio, Streamable HTTP, and SSE transports;
- Skills compatible with `.claude/skills/<name>/SKILL.md`;
- verifiable domain packages and the official steel and materials package;
- production-data import, deterministic calculations, model inference, prediction, and constrained optimization;
- conversation export, full backup and restore, diagnostics, updates, and Windows release packaging.

## Product positioning and architecture

Bloomery has two layers:

- **General agent core**: conversations, context, memory, providers, RAG, tools, permissions, tasks, and extension protocols;
- **Official steel domain package**: steel grades, standards, processes, defects, units, production data, and materials engineering workflows.

Steel is the first official domain. It does not prevent the community from building packages for other engineering domains.

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

Key boundaries:

- React owns presentation and short-lived UI state;
- Rust owns agent execution, context, providers, retrieval, permissions, tasks, and persistence;
- SQLite is authoritative, while vector indexes are rebuildable derived data;
- providers expose explicit capabilities instead of relying on provider-name heuristics;
- domain packages cannot bypass the tool registry or permission system, and cannot ship arbitrary executable code;
- the frontend must not call external providers directly or hold full API keys.

## Provider setup

| Capability | Default choice | Notes |
| --- | --- | --- |
| LLM | OpenAI-compatible / Ollama | Conversation and agent reasoning; Ollama can connect local models |
| Embedding | SiliconFlow `BAAI/bge-m3` | Knowledge-base vectorization |
| Reranker | SiliconFlow `BAAI/bge-reranker-v2-m3` | Candidate evidence reranking |
| Document parsing | MinerU | High-quality PDF and layout parsing, configurable later |

SiliconFlow Free and Pro choices are up to the user. Bloomery does not proxy billing or infer subscription status; it only stores the selected models and connection results. API keys are entered through settings and stored in Windows Credential Manager. SQLite stores provider configuration and secret references, not plaintext secrets.

## Quick start

Requirements: Windows 10+, Node.js 20/22/24, Rust stable, Visual Studio Build Tools with `Desktop development with C++`, WebView2 Runtime, Git, and Tauri 2 Windows prerequisites.

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

If the Tauri CLI is not installed:

~~~powershell
cargo install tauri-cli --version "^2"
~~~

You can also run `./start-desktop.bat` from the repository root. It starts the development environment; it is not a release installer.

## Local data, privacy, and network

Bloomery stores `bloomery.sqlite3` in the operating system application-data directory, not in the repository. Conversations, messages, summaries, memories, settings, knowledge metadata, task state, and index-management data remain on the local machine.

Bloomery is local-first, not automatically offline. Cloud LLM, SiliconFlow, MinerU, or MCP requests may leave the machine. Network access should only come from user-configured providers, enabled extensions, and update checks. API keys must not appear in source code, `.env` files, logs, screenshots, exports, or diagnostics.

## Repository layout

~~~text
bloomery/
|-- frontend/                 React 18, TypeScript, Vite, and desktop UI
|-- src-tauri/
|   |-- src/app/              Tauri commands and application assembly
|   |-- src/agent/            Agent, context, memory, and protocol
|   |-- src/providers/        LLM, SiliconFlow, and MinerU
|   |-- src/rag/              Ingestion, parsing, indexing, retrieval, citations
|   |-- src/storage/          SQLite, migrations, and secret references
|   |-- src/tasks/            Recoverable background tasks
|   +-- tests/                Rust integration and architecture tests
|-- docs/                     Protocol, design specs, benchmarks, and release plans
|-- scripts/                  Development helper scripts
|-- start-desktop.bat         Windows development entry point
|-- LICENSE                   Apache License 2.0
+-- README.md                 Default Chinese README
+-- README.en.md              English README
~~~

The public event protocol is documented in [`docs/PROTOCOL.md`](docs/PROTOCOL.md). Design specs and implementation plans live under `docs/superpowers/`; current source code and tests are the authority for implementation status.

More docs: [`SECURITY.md`](SECURITY.md) security policy, [`CONTRIBUTING.md`](CONTRIBUTING.md) contributing guide, [`docs/NON-GOALS.md`](docs/NON-GOALS.md) non-goals, and [`docs/releases/case-study.md`](docs/releases/case-study.md) reproducible steel case study.

## Release roadmap

| Phase | Status |
| --- | --- |
| Foundation decoupling, SQLite, system secrets, providers, and durable tasks | Covered in development builds |
| Local knowledge base, MinerU, hybrid retrieval, reranking, and citations | In progress |
| Modular agent, event protocol, context, memory, recovery, and tool-call repair | In development |
| Tool permissions, MCP, Skills, and domain packages | Release target |
| Official steel domain package, production data, compute, prediction, and optimization | Release target |
| Workbench, diagnostics, export, restore, Windows installation, and formal release | Release target |

These phases are engineering dependencies, not separate MVP, Alpha, or Pro editions. Bloomery will only be labeled as the first public release after all release gates pass.

## Non-goals

Bloomery explicitly does not:

- provide accounts, login, membership, admin consoles, or hosted team workspaces;
- reuse Steel Agent Web login, sessions, backend APIs, or private data sources;
- require a server maintained by the project author;
- host user API keys, knowledge bases, production data, or chat history;
- make terminal coding, patch generation, or repository automation the core product;
- execute arbitrary binaries or scripts bundled inside domain packages;
- run write, shell, or other high-risk tools without user confirmation;
- promise that third-party cloud-provider requests stay on the local machine;
- position itself as another generic chat client.

## Contributing

Issues, documentation, tests, and code contributions are welcome. Please read `docs/PROTOCOL.md` and the relevant design specs first, preserve the single Tauri bridge boundary, declare provider capabilities and error types, route new tools through the registry and permission policy, and do not commit API keys, enterprise data, real user sessions, build artifacts, or generated directories.

~~~powershell
Set-Location frontend
npm run test
npm run test:boundaries
npm run build

Set-Location ../src-tauri
cargo check
cargo test
~~~

Do not publish credentials or production data in public security reports. Use the maintainer's security channel with the minimum reproducible information.

## License

Bloomery is released under the Apache License 2.0. See [`LICENSE`](LICENSE) for the full text and [`NOTICE`](NOTICE) for attribution. Rights granted under earlier MIT-licensed releases are preserved.
