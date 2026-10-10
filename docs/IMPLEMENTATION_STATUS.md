# Suna 实施状态审计（M0）

- 审计日期：2026-10-10
- 审计范围：`docs/PRODUCT.md`、`ARCHITECTURE.md`、`UI_DESIGN.md`、`DEVELOPMENT_RULES.md`、`ROADMAP.md`（对应 `CODEX_EXECUTION_PROMPTS.md` 的 Prompt 0）
- 审计方式：只读盘点。**本次未修改、未删除任何代码或用户数据。**
- 结论性质：本文只陈述当前可验证事实。测试通过不等于发布完成；未运行的项目一律标记为未验证。

---

## 1. 总体结论

仓库**不是**一个待开发的新项目，而是一个**已高度实现的 Rust-first 桌面应用**：

- Rust 后端约 **7.2 万行**，15 个模块，**195 个 Tauri 命令**，**32 个数据库迁移**。
- 前端 React 19 + TypeScript，**92 个 ts/tsx**，14 个业务页面。
- 已有 **908 个 Rust 测试**、**110 个前端测试用例**、**9 个 Python 测试文件**、**3 个 Playwright E2E**。
- 存在一个自洽的本地优先运行时（Agent Loop、事件流、权限、检查点恢复、RAG）。

**但当前基线不可构建**：Rust 编译在依赖解析阶段失败（见第 2 节）。这意味着**任何 Rust 侧验收目前都无法执行**。

同时，新文档包描述的**目标架构与现有实现存在结构性偏差**（见第 6 节）。`ARCHITECTURE.md` 自己写明：

> 先审计现有项目再实施，不要为了符合新项目模板而重写已有代码。

因此后续计划应以**对齐与补齐**为主，而不是按文档从零重建。

---

## 2. 基线错误（必须先解决，否则无法验收）

### B1 — Rust 构建阻断（P0，阻断全部 Rust 工作）

| 项 | 内容 |
| --- | --- |
| 现象 | `cargo check` 与 `cargo check --all-targets` 均以退出码 **101** 失败 |
| 错误 | `error[E0107]: struct takes 3 generic arguments but 2 generic arguments were supplied` |
| 位置 | `schemars-0.8.22/src/lib.rs:12` → `pub type Map<K, V> = indexmap::IndexMap<K, V>;` |
| 根因 | `Cargo.lock` 把 `schemars 0.8.22` 解析到 **`indexmap 1.9.3`**（`IndexMap<K, V, S>` 需要 3 个泛型），而 `schemars` 该特性期望 `indexmap 2.x` |
| 依赖链 | `suna → tauri-build 2.6.3 → schemars 0.8.22 → indexmap 1.9.3`（`tauri-plugin 2.6.3` 同） |
| 佐证 | `cargo tree -i indexmap@1.9.3` 显示唯一上游就是 `schemars 0.8.22`；`Cargo.lock` 时间戳 2026-09-28 |
| 建议 | 重新解析锁文件（`cargo update -p indexmap` 或按需 `-p schemars`），或对 `indexmap` 施加 2.x 约束。**修复前不要动其他 Rust 代码** |

> 注意：这不是业务代码缺陷，而是依赖解析不一致。修复应是最小变更。

### B2 — 协议契约产物缺失（P1，必然导致测试失败）

| 项 | 内容 |
| --- | --- |
| 现象 | `docs/protocol.schema.json` 已在工作区被删除（`git status` 显示为 `D`） |
| 影响 | `src-tauri/tests/agent_protocol_contract.rs:15` 读取该文件并在缺失时 `panic!`；`src-tauri/src/bin/export_protocol.rs:13` 也以它为输出目标 |
| 后果 | 即使 B1 修好，`cargo test` 仍会失败；协议导出/校验链路断裂 |
| 建议 | 用仓库自带的导出器重建产物：`cargo run --bin export_protocol`，再用 `--check` 校验；**不要手工编辑生成文件** |

### B3 — Python 依赖未安装（P1，环境阻塞）

| 项 | 内容 |
| --- | --- |
| 现象 | `compute-worker/.venv` 不存在；`numpy` / `scikit-learn` / `optuna` / `onnxruntime` 均 `import` 失败 |
| 可用基础 | Python **3.13.14** 与 `uv` **0.11.19** 已就绪 |
| 声明要求 | `compute-worker/pyproject.toml` 要求 `requires-python >= 3.12`，依赖 `numpy`、`onnx`、`onnxruntime`、`optuna`、`scikit-learn`；可选 `pytest`、`pyinstaller` |
| 后果 | 训练 / 预测 / ONNX / 优化四条链路无法验证；性能预测与工艺优化页面在后端不可用 |
| 建议 | 按 `CONTRIBUTING.md` 执行 `uv sync --frozen --extra packaging`（含测试需额外安装 `test` extra） |

### B4 — 工作区存在大量未提交的文档变更（P1，需你决策）

| 项 | 内容 |
| --- | --- |
| 统计 | 工作区：**删除 68 项**、**新增未跟踪 23 项**、修改 0 项 |
| 删除内容 | 旧 `docs/`（PROTOCOL、agent-runtime、memory、extensions、steel、superpowers、benchmarks、releases、security-model、protocol.schema.json、assets、testing 等）与 `界面UI/` 全部 16 张图 |
| 新增内容 | 新 19 份提示词包文档 + `docs/ui-reference/`（16 张图，与 `界面UI/` 内容同名） |
| 风险 | ① `docs/protocol.schema.json` 被删即 B2；② `界面UI/` 内容已迁移到 `docs/ui-reference/`，属**移动**而非丢失；③ 旧文档中 `dependency-exceptions.md` 仍被 `src-tauri/deny.toml:58` 以注释引用 |
| 建议 | 由你决定**提交**还是**回滚**。在决定前我不会动这些文件。若确认迁移完成，应一并提交删除与新增，避免仓库长期处于半删状态 |

### B5 — 低风险遗留项（P3，非阻断）

- 空目录 5 个：`frontend/src/features/{analysis,databases,extensions,workbench}`、`src-tauri/examples`。
- 死代码 1 个：`frontend/src/features/research/ResearchModulePage.tsx` 不可达（`SunaApp.tsx:9` 引入，但 `ModuleView` 末尾 fallback 分支永不命中；内部 `const list: Row[] = []` 恒为空）。删除需同步修改 `SunaApp.tsx`。
- 孤儿翻译键：`frontend/src/i18n/locale.tsx` 中约 15 个 `workbench*` 键无任何调用点。
- 临时残留：根目录 3 个 `.codex-agent-loop-*.log`、`compute-worker/suna_worker/__pycache__/`。

### 已验证通过的门禁（对照组）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| 前端类型检查 | `tsc --noEmit` | **通过**（0 错误） |
| 前端单元测试 | `vitest run` | **通过**（17 文件 / 110 用例） |
| 运行边界检查 | `check-runtime-boundaries.mjs` | **通过**（64 文件） |
| Rust 构建 | `cargo check` | **失败**（B1，exit 101） |
| Python 测试 | `pytest` | **未运行**（B3，依赖缺失） |
| Playwright E2E | `playwright test` | **未运行**（需已构建的桌面端） |

---

## 3. 环境阻塞汇总

| 编号 | 阻塞项 | 影响范围 | 解除方式 |
| --- | --- | --- | --- |
| B1 | Rust 依赖解析冲突 | 全部 Rust 编译 / 测试 / 打包 | 重解析 `Cargo.lock` |
| B2 | `docs/protocol.schema.json` 缺失 | 协议契约测试、协议导出 | 运行 `export_protocol` |
| B3 | Python 依赖未安装 | 预测、优化、ONNX、训练链路 | `uv sync` |
| — | 无真实 Provider 密钥 | 模型对话为未验证状态 | 由用户配置（本审计不涉及） |

> 本审计**未验证**任何真实模型调用、真实文献源或 PostgreSQL 实例连通性。这些均需外部配置，标记为未验证而非未实现。

---

## 4. 分模块实施状态

图例：**已实现**（功能完整且可验证）／**部分实现**（主链路存在，范围不完整）／**未实现**（无对应实现）／**阻塞**（受环境限制无法验证）。

### 4.1 工程基线

| 项 | 状态 | 证据 |
| --- | --- | --- |
| Tauri 2 桌面壳 | 已实现 | `src-tauri/tauri.conf.json`（1280×820，min 1080×720，`identifier com.suna.desktop`） |
| 前端构建链 | 已实现 | React 19 / TypeScript 6 / Vite 7 / Tailwind 4；`dist/` 已产出 |
| Rust 编译 | **阻塞** | B1 |
| Python 科研服务 | **部分实现 + 阻塞** | 存在 `compute-worker/`（stdio JSON-RPC 子进程），但依赖未装（B3），且**不是** `ARCHITECTURE.md` 要求的 FastAPI HTTP 服务 |
| 统一日志 / 错误处理 | 部分实现 | 各模块有错误类型与脱敏，未见统一日志层 |
| CI | 部分实现 | `.github/workflows/{quality,release}.yml` 存在，但当前无法本地复现通过（B1） |

### 4.2 UI Shell 与页面

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 三栏布局 | 已实现 | `SunaApp.tsx`（Sidebar + Main + 右栏 `AgentRunInspector`，受 `showAgentPanel` 控制） |
| 侧栏 / 顶栏 / 全局搜索 | 已实现 | 17 个导航项、最近会话、Ctrl+K 搜索面板 |
| 主题（Light/Dark/System） | 已实现 | `theme/` + 设置页外观 tab |
| 空 / 加载 / 错误态 | 已实现 | 各页面均有 empty/loading/error 分支，且已有对应测试 |
| 页面清单（对照 `UI_DESIGN.md`） | 部分实现 | 见下表 |
| Design Token 集中化 | 部分实现 | 存在 `design-system/suna/MASTER.md` 与 CSS 变量，但 `UI_DESIGN.md` 建议的色值（`#1677ff` 等）与现状 `#2563EB` 不一致，未统一 |

页面级状态（前端实现规模，按字节）：

| 页面 | 规模 | 状态 |
| --- | --- | --- |
| 对话中心 / 主界面 | 39 KB `DesktopChatWorkspace` + 4 子面板 | 已实现 |
| 知识中心 | 72 KB `KnowledgeCenterWorkspace` | 已实现 |
| 设置中心 | 11 个面板约 95 KB | 已实现 |
| 运行记录 / 诊断 | 10 KB + 5 组件 | 已实现 |
| MCP 管理 | 13 KB（CRUD + 健康检查 + 工具列表） | 已实现 |
| Agent 管理 | 14 KB + 7.5 KB 计划面板 | 已实现 |
| Skill / 工具中心 | 3 KB + 4 KB 面板 | 部分实现（工具中心为只读列表，无启停/配置） |
| 文献研究 | 18 KB | 部分实现（依赖外部文献源） |
| 性能预测 | 11 KB | 部分实现（缺多模型对比、SHAP、PDP） |
| 工艺优化 | 6.7 KB | 部分实现（需手工填写训练任务 ID，无 Pareto 图） |
| 数据实验室 | 6.6 KB | 部分实现（缺清洗/异常值/PCA/聚类/SHAP） |
| 实验助手 | 4.3 KB | 部分实现（无 DOE/正交/响应面/主动学习算法） |
| 科研报告 | 2.9 KB | 部分实现（仅 Markdown 导出，缺 PPT/PDF/Word） |
| 关于 | 设置页 tab | 已实现 |

### 4.3 持久化与 API

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 迁移体系 | 已实现 | `src-tauri/src/storage/migrations/`，**32 个**版本化迁移（`0001_initial.sql` → `0032_agent_automations.sql`） |
| 会话 / 消息 / 任务 / 事件 | 已实现 | 195 个 Tauri 命令，覆盖 conversations、history、tasks、agent、knowledge 等 |
| 主库 | 已实现（与文档不同） | **SQLite** 为本机主库；PostgreSQL 仅用于知识库向量检索 |
| PostgreSQL + pgvector | 部分实现 | `knowledge_db.rs:81` 声明 `postgresql_pgvector`，`:84` 使用 `hnsw`，`:562` 明确"当前仅支持 PostgreSQL + pgvector"；但会话/任务/配置**不在** PostgreSQL |
| 健康检查 | 已实现 | 诊断页 + `check_vector` / 连接检查命令 |
| 迁移不清空旧数据 | 已实现（设计上） | 迁移为增量 `0001..0032`，无清库语句 |

### 4.4 模型 Runtime

| 项 | 状态 | 证据 |
| --- | --- | --- |
| Provider 抽象 | 已实现 | `providers/profiles.rs:10` 定义 7 种：OpenAI 兼容 / Anthropic / Qwen / DeepSeek / Ollama / SiliconFlow / MinerU |
| 密钥安全 | 已实现 | 密钥存 Windows Credential Manager，SQLite 只存引用（`secret_generation` / `secret_configured`） |
| 连接测试 | 已实现 | 设置页 Provider 卡片测试连接 |
| 流式输出 | 已实现 | `agent-event` + `message_delta` / `reasoning_delta` |
| 真实模型联调 | **未验证** | 需用户配置密钥 |

### 4.5 Agent Runtime

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 状态机 | 已实现 | `created → preparing → generating → executing_tools → verifying → completing → completed`，含 `awaiting_permission` 与终态 |
| Planner / Executor / Dispatcher | 已实现 | `agent/runtime/loop/`（operations、generation、background）+ `subagents/` |
| 事件模型 | 已实现 | 协议 v1，`sequence` 单调递增，先写库再发布，支持 replay |
| 取消 / 超时 / 有限重试 | 已实现 | 父取消传播子 Turn；默认 run 超时 1800s、请求超时 120s；重试与恢复各默认 2 次 |
| 检查点与恢复 | 已实现 | `checkpoint_saved` / `recovery_*`，租约 10 分钟可接管 |
| 子 Agent | 已实现 | `child_turn_id` 独立事件序列 + 查询/重放/取消命令 |
| 8 个固定专业 Agent | **未实现（按文档口径）** | 现状是**可配置 Agent Profile**（`agent/profiles.rs`：权限开关 + max_turns/max_tool_calls/context_budget/超时），而非 `PRODUCT.md` 列出的 8 个硬编码角色 |
| 屈服强度特殊路由 | 部分实现 | 存在 `steel/optimization_tool.rs`、`app/steel_agent_gateway.rs`，但"仅当意图明确为预测屈服强度且提供成分/工艺参数才路由、其余指标明确告知不支持"这一约束**未逐条验证** |

### 4.6 Tools / MCP / Skills

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 工具注册表与权限 | 已实现 | `tools/` + `permissions/`；工具分 automatic / confirmation_required / dangerous |
| 只读工具 | 已实现 | 碳当量 IIW / Pcm（`steel/calculators.rs`）、知识检索等 |
| MCP 传输 | 已实现 | stdio / streamable_http / sse（含同源校验）；`mcp/` 2146 行 |
| MCP 凭据与环境隔离 | 已实现 | 子进程环境白名单，仅允许固定变量 |
| Skill 版本管理 | 已实现 | 启停、tags、version、`content_sha256`、12 个上限、错误列表 |
| 禁止任意 Shell | 部分实现 | 存在 PowerShell 工具（受工作目录约束 + 确认），与文档"禁止任意 Shell"需对齐口径 |

### 4.7 知识中心 / RAG

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 上传 / 解析 / 切分 / Embedding / 索引 / 检索 / 引用 | 已实现 | `rag/`（47 文件，8746 行）+ `knowledge_db.rs`（5622 行） |
| 引用与来源定位 | 已实现 | citation 编号 + 源位置（页码等），前端可打开原文 |
| 失败重试与状态 | 已实现 | 任务表 + 进度事件 |
| pgvector 适配层 | 部分实现 | 直接绑定 PostgreSQL + pgvector + hnsw/ivfflat，未见可替换适配接口 |
| 降级检索 | 已实现 | 缺 Embedding/Reranker 时退回关键词召回，不阻塞对话 |

### 4.8 文献研究 / 数据实验室 / 预测 / 优化与实验

| 模块 | 状态 | 缺口 |
| --- | --- | --- |
| 文献研究 | 部分实现 | 检索/解析/总结主链路在；综述、趋势、研究空白未实现；依赖外部文献源 |
| 数据实验室 | 部分实现 | 导入 + 质量指标 + 均值图；缺清洗、异常值、重复值、PCA、聚类、SHAP、导出 |
| 性能预测 | 部分实现 | 线性回归 / RandomForest / XGBoost + ONNX 导出与推理；缺多模型对比、SHAP、PDP、预测区间；**阻塞**于 B3 |
| 工艺优化 | 部分实现 | Optuna 约束优化（单/多目标）；算法只有一种，无 Pareto 图；**阻塞**于 B3 |
| 实验助手 | 部分实现 | 表单 → Agent 建议；无 DOE / 正交 / 响应面 / 主动学习；**阻塞**于 B3 |

### 4.9 设置 / 报告 / 打包

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 设置持久化 | 已实现 | 11 个 tab：账户 / 模型 / 常规 / 外观 / 知识库 / Agent / MCP / Skill / 数据库 / 快捷键 / 关于 |
| 诊断 | 已实现 | `diagnostics/` + 运行记录页 |
| 备份恢复 | 部分实现 | 有存储路径管理与缓存清理；未见完整备份/还原 |
| 科研报告 | 部分实现 | 仅 Markdown 导出 |
| 打包 | 部分实现 | `tauri.conf.json` bundle 配置 + 资源映射在；**阻塞**于 B1，当前无法产出安装包 |

---

## 5. 目录 / 依赖 / 迁移 / 测试 / Git 盘点

### 5.1 目录

| 路径 | 内容 | 与 `ARCHITECTURE.md` 建议的差异 |
| --- | --- | --- |
| `frontend/` | React 前端 | 文档建议 `src/` —— 现状为 `frontend/` |
| `src-tauri/` | Rust + Tauri | 一致 |
| `compute-worker/` | Python 计算子进程 | 文档建议 `research_service/`（FastAPI）—— **目录与形态均不同** |
| `src-tauri/src/storage/migrations/` | 32 个迁移 | 文档建议顶层 `migrations/` —— 位置不同 |
| `docs/` | 文档 | 一致；但 `docs/adr/` **缺失** |
| `domain-packs/`、`case-studies/`、`design-system/`、`scripts/` | 领域包 / 案例 / 设计系统 / 脚本 | 文档未提及，属现有资产 |
| `界面UI/` | 已空（内容移至 `docs/ui-reference/`） | — |

后端模块规模（行）：`agent` 13674、`app` 14766、`storage` 9095、`rag` 8746、`knowledge_db.rs` 5622、`providers` 4272、`tasks` 3613、`steel` 3050、`mcp` 2146、`compute` 1519、`domains` 1316、`diagnostics` 894、`tools` 852、`skills` 819、`permissions` 617、`database` 577。

### 5.2 依赖

- **Rust**（`src-tauri/Cargo.toml`）：`tauri 2`、`rusqlite`（bundled）、`sqlx`（postgres）、`tiberius`（SQL Server）、`rmcp 3.1.1`、`hnsw_rs`、`keyring`、`ed25519-dalek`、`calamine`、`pdf-extract`、`tokio`、`reqwest`（含 0.13 双版本）。**已登记一处刻意例外**：`tiberius` 首版不带 TLS（`Cargo.toml` 内 `ponytail:` 注释）。
- **前端**（`frontend/package.json`）：React 19、react-markdown + remark/rehype（GFM/math/raw）+ KaTeX、recharts、pdfjs-dist、docx-preview、lucide-react、`@tauri-apps/api`；测试用 vitest + @testing-library + Playwright。无 Zustand（`README_使用说明.md` 建议 Zustand，现状为 React Context）。
- **Python**（`compute-worker/pyproject.toml`）：numpy、onnx、onnxruntime、optuna、scikit-learn；可选 pyinstaller、pytest。**未安装**（B3）。

### 5.3 数据库迁移

32 个版本化 SQL 迁移，覆盖：初始、本地工作区、Provider Profile（含 revision）、后台任务、知识库、Embedding 向量、FTS、检索审计、Agent Run / 记忆 / 检查点 / Turn 快照 / 子 Turn、权限规则、领域包、钢铁数据集 / 模型、MCP（含 legacy SSE）、数据库连接、任务计时 / 领取、Cron、自动化、设置审计日志。

### 5.4 测试

| 类别 | 规模 | 状态 |
| --- | --- | --- |
| Rust 单元 + 集成 `#[test]` | 908 | 阻塞（B1） |
| Rust 集成测试文件 | 79（含 `architecture.rs` 边界测试、`agent_protocol_contract.rs`） | 阻塞（B1）+ B2 |
| 前端测试 | 17 文件 / 110 用例 | 通过 |
| 前端边界测试 | 64 文件 | 通过 |
| Python 测试 | 9 文件 | 未运行（B3） |
| Playwright E2E | 3 个 spec（smoke / startup / knowledge） | 未运行 |

### 5.5 Git

- 分支 `main`，最近提交 `7fb0d4d 增加Windows子进程测试启动诊断`（2026-10-10）。
- 工作区：**68 删除 / 23 新增 / 0 修改**，全部未提交（见 B4）。
- 仓库跟踪 696 文件；磁盘实际 39998 文件，差额为已忽略的构建产物（`src-tauri/target` 9.4 GB、`frontend/node_modules` 367 MB、`frontend/dist` 21 MB）。
- `.workbuddy-ai/` 为本地记忆目录，**不属于清理范围**。

---

## 6. 文档 vs 现状偏差（需你确认取舍）

| 文档要求 | 现状 | 建议 |
| --- | --- | --- |
| Python **FastAPI** 科研服务（HTTP） | `compute-worker/` stdio JSON-RPC 子进程 | 保留现状（更符合本地优先、无端口暴露）；若确需 HTTP，作为新阶段单独立项 |
| **PostgreSQL** 承载会话/消息/任务/事件/配置 | SQLite 为主库，PostgreSQL 仅知识库 | 保留现状；`ARCHITECTURE.md` 自身要求"不要为符合模板而重写" |
| 顶层 `migrations/`、`research_service/`、`src/` | 迁移在 `src-tauri/src/storage/migrations/`，前端在 `frontend/` | 不建议为改名而移动，收益低、回归风险高 |
| `docs/adr/` 记录架构决策 | 缺失 | **建议采纳**，成本低、收益明确 |
| `UI_DESIGN.md` 色值 `#1677ff` 等 | 现状主色 `#2563EB` | 需你决定以哪套为准，再统一 Token |
| 8 个固定专业 Agent | 可配置 Agent Profile | 建议在文档中改为"可配置专业 Agent"，或补齐角色预设 |
| Zustand 状态管理 | React Context | 现状可用，建议不改 |
| 禁止任意 Shell | 存在受约束的 PowerShell 工具 | 需对齐口径（是"禁止"还是"受控允许"） |

---

## 7. 阶段计划（按依赖顺序）

> 原则：先解除阻塞，再对齐文档，最后补功能。每一步都必须能独立验收。

### P0 — 解除 Rust 构建阻断（最高优先，其余 Rust 工作全部依赖它）

1. 修复 `Cargo.lock` 中 `schemars 0.8.22 → indexmap 1.9.3` 的解析冲突。
2. 验收：`cargo check` 与 `cargo check --all-targets` 退出码为 0。
3. 产出：修复说明 + 命令输出。

### P1 — 恢复协议契约与文档一致性

1. 重建 `docs/protocol.schema.json`（用 `cargo run --bin export_protocol`，禁止手工编辑），并用 `--check` 校验。
2. 就 B4 做出决定：提交或回滚 68 删 / 23 增。若确认 `界面UI/` 已迁移至 `docs/ui-reference/`，一并提交删除。
3. 验收：`agent_protocol_contract.rs` 通过；`git status` 干净或变更已按你的决定提交。

### P2 — 建立 Python 环境

1. `uv sync --frozen --extra packaging`（测试另加 `test` extra）。
2. 验收：`python -m pytest -q` 通过；`hello` / `train_linear_regression` / `predict_linear_regression` / `optimize_constrained` 可调用。
3. 依赖：无（可与 P0/P1 并行）。

### P3 — 低风险清理（B5）

1. 删除 5 个空目录与临时日志/`__pycache__`。
2. 处理 `ResearchModulePage.tsx` 死代码（需同步改 `SunaApp.tsx`）与孤儿 `workbench*` 翻译键。
3. 验收：`tsc --noEmit` + 110 个前端用例仍通过。
4. 约束：分批执行，每批 ≤10 项，逐批验证。

### P4 — 文档对齐决策

1. 就第 6 节逐条确认取舍，建立 `docs/adr/` 记录决定。
2. 统一 Design Token（色值以你指定的一套为准）。
3. 验收：`ARCHITECTURE.md` / `UI_DESIGN.md` 与实际实现不再互相矛盾。

### P5 — 补齐功能缺口（对应 ROADMAP M7–M10，按价值排序）

| 顺序 | 目标 | 前置 |
| --- | --- | --- |
| P5.1 | 数据实验室：清洗 / 异常值 / 重复值 / PCA / 聚类 / SHAP / 导出 | P2 |
| P5.2 | 性能预测：多模型对比、SHAP、PDP、预测区间 | P5.1 |
| P5.3 | 工艺优化：算法可选（NSGA-II / Bayesian / GA / PSO / Grid）+ Pareto 图 + 免手工填任务 ID | P5.2 |
| P5.4 | 实验助手：DOE / 正交 / 响应面 / 主动学习 | P5.3 |
| P5.5 | 科研报告：PPT / PDF / Word 导出 | P5.1 |
| P5.6 | 工具中心：启停与配置（当前只读） | P3 |
| P5.7 | 知识中心：可替换向量适配层 | P4 |
| P5.8 | 备份恢复 + 干净环境安装验收 | P5.1–P5.7 |

### 不进入本计划的事项

- 按文档重建技术栈（违反 `ARCHITECTURE.md` 与 `DEVELOPMENT_RULES.md` 的最小变更原则）。
- 删除或覆盖用户数据、原始文件、凭据存储。

---

## 8. 本次审计的边界声明

- **未修改任何代码、文档或数据**；仅新增本文件。
- 未运行需要外部配置的验证：真实模型调用、真实文献源、PostgreSQL 实例连通性、桌面安装包。
- 未执行 B4 涉及的删除/回滚——该决定权在你。
- Rust 侧全部结论均标注为"阻塞"，因为 B1 使 `cargo test` 无法执行；**修好 B1 后应重跑并更新本文档**。

**下一步：等待你对 P0–P4 的确认，再开始动手。**
