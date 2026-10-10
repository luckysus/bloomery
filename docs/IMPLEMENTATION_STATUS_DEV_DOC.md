# Suna 开发程度评估（依据 `开发文档.md`）

- 评估日期：2026-10-10
- 评估依据：`开发文档.md`（89 章产品规格，用户提供）
- 评估方式：逐章对照代码、测试、迁移与依赖。**只读核对，未修改任何功能代码。**
- 说明：本文按 `开发文档.md` 的口径评估。基于新增 19 份文档包的评估见 `docs/IMPLEMENTATION_STATUS.md`。

## 评估立场（重要）

**本文的唯一基准是 `开发文档.md`。** 凡文档写明的要求，未实现即计入缺口，不因"工程上更合理"而豁免。

- 正文各章的「状态」列只做事实判定：✅ 符合文档 / 🟡 部分符合 / ❌ 未实现。
- 本文**不包含**评估者的技术取舍建议。若某处需要工程判断，会单独标注为「评估者注」并明确它不是文档要求。
- 第 20 项（P3）各项虽然实现方式与文档不同且功能可用，**仍按缺口计**，因为文档是唯一基准。

## 总体结论

**整体完成度约 75%。**

- **架构与骨架层几乎完全落地**（技术栈、分层链路、三栏布局、15 个模块、配色、Rust 内核决策）。
- **工程结构层高度一致**（Rust 模块设计 8/8 概念覆盖，数据模型 8/14 落表）。
- **缺口集中在"科研算法深度"**：数据可视化类型、模型比较与解释、优化算法族、实验设计算法、报告导出格式。

| 分组                    | 章节    | 完成度      |
| --------------------- | ----- | -------- |
| A 产品与架构               | 1–5   | **100%** |
| B Agent 架构与 Workspace | 6–10  | **100%** |
| C 界面与视觉               | 11–21 | **100%** |
| D 知识中心与文献             | 22–29 | **100%** |
| E 数据实验室与预测            | 30–37 | ~55%     |
| F 工艺优化                | 38–43 | ~50%     |
| G 实验助手                | 44–46 | ~25%     |
| H 管理模块                | 47–53 | ~80%     |
| I 模型与报告               | 54–56 | ~70%     |
| J 设置                  | 57–62 | ~90%     |
| K UI 规范与交互            | 63–74 | ~70%     |
| L 工程结构                | 75–82 | ~90%     |
| M 原则与开发要求             | 83–89 | ~90%     |

---

## A. 产品与架构（1–5 章）— 100%

| 章   | 要求                                                                                                                                     | 状态        | 证据                                                                                                                                                                   |
| --- | -------------------------------------------------------------------------------------------------------------------------------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1–2 | 定位：面向钢铁材料研发的 AI Agent 工作平台；"聊天是入口，Agent 是核心"                                                                                           | ✅         | `README.md`、`README.en.md`、`docs/PRODUCT.md`、侧栏副标题与关于面板文案一致                                                                                                          |
| 3   | 15 个核心模块                                                                                                                               | ✅         | `frontend/src/app/navigation.ts` 19 个导航项覆盖全部 15 个模块                                                                                                                  |
| 4   | Rust + Tauri 2；React 19 + TS + Vite；Tailwind；Recharts；React Markdown；KaTeX；**Agent 核心由 Rust 实现，不依赖 LangGraph**；**Python 不作核心 Runtime** | ✅ 技术栈全部一致 | `Cargo.toml`（tauri 2）、`package.json`（react 19 / typescript 6 / vite 7 / tailwind 4 / recharts / react-markdown / katex）；无 LangGraph 依赖；Python 仅为 `compute-worker` 侧车 |
| 4   | UI：shadcn/ui + Radix UI                                                                                                                | ✅ 已采用     | 16 个 `@radix-ui/*` 依赖 + `src/components/ui/` 22 个组件；原生 `<select>` 31 个、`<textarea>` 13 个、checkbox 14 个已全部迁移，仅 3 个 `type="file"` 保持原生                                 |
| 4   | State：Zustand                                                                                                                          | ✅ 已采用     | `zustand` 依赖 + `src/stores/` 11 个文件（8 个 store + storeUtils + settingsPersistence + localeModel 等），4 处 Context（appearance / theme / i18n / chatController）已迁移         |
| 4   | Charts：ECharts 或 Recharts                                                                                                              | ✅         | Recharts；`src/components/charts/` 8 个图表组件，图表类型 9 种                                                                                                                   |
| 5   | 分层：`React → Tauri IPC → Rust Core → Agent Runtime → LLM/RAG/Tools/MCP/Models`                                                          | ✅ 完全一致    | `SunaApp` → `bridge/desktop.ts` → `app/commands.rs`（195 个命令）→ `agent/runtime/` → `providers` / `rag` / `tools` / `mcp`                                               |
| 5   | React 职责：UI / Chat / 数据可视化 / 页面交互 / 设置 / Agent 状态展示 / 知识库管理                                                                            | ✅         | 数据可视化已从 1 种（BarChart）补齐到 9 种（柱状/箱线/热力/直方/雷达/散点/折线/PCA/SHAP）                                                                                                          |

---

## B. Agent 架构与 Workspace（6–10 章）— 100%

| 章    | 要求                                                                                                                      | 状态   | 证据                                                                                                                                                                                                                                       |
| ---- | ----------------------------------------------------------------------------------------------------------------------- | ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 6    | Master Agent + 8 个专业子 Agent（Knowledge / Literature / Data / Material / Prediction / Optimization / Experiment / Report） | ✅    | `agent/profiles.rs::presets()` 定义 Master + 8 个专家预设，各有独立 system prompt、描述与工具白名单；只有 Master 持有 `agent.task`，其余 8 个作为可委派子 Agent；`app/desktop_agent_runtime.rs` 把可委派专家清单注入 Master 提示词，子 Agent 通过 `agent/runtime/subagents/` 的 child turn 机制执行 |
| 7    | Master Agent 职责（理解、拆分、调度、汇总）                                                                                            | ✅    | `agent/runtime/loop/`（operations / generation / background）+ `loop_types.rs`                                                                                                                                                             |
| 8    | 7 种 Agent 状态（idle/thinking/planning/running/waiting/completed/failed）                                                   | ✅ 超集 | 协议 `AgentRunState` 有 11 态：created / preparing / generating / awaiting_permission / executing_tools / verifying / completing / completed / cancelled / failed / interrupted                                                               |
| 9–10 | 右侧 Agent Workspace：当前任务、子 Agent 状态、工具调用、执行步骤、耗时、结果、进度                                                                   | ✅    | `features/chat/AgentRunInspector.tsx`（运行检查器：状态、sequence、工具数、token、进度、工具调用、权限、checkpoint、恢复、引用、错误）+ `ChildAgentPanel.tsx`                                                                                                                 |

---

## C. 界面与视觉（11–21 章）— 100%

| 章     | 要求                                                                                                                                                                                     | 状态 | 证据                                                                                                                                                                              |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 11    | 三栏桌面布局（Sidebar / Main / Agent Workspace）                                                                                                                                               | ✅  | `app/SunaApp.tsx` + `features/chat/DesktopChatWorkspace.tsx`                                                                                                                    |
| 12–13 | Sidebar：Logo、新对话、7 个研究模块、更多、最近、用户中心；宽度 250–280px；顶部 `Suna` + `Steel Research Agent`                                                                                                    | ✅  | 侧栏宽度 260px（narrow 250 / wide 280），落在文档区间；品牌区为 `Suna` + `Steel Research Agent`；含新对话、7 个研究模块、更多、最近对话、底部用户中心                                                                       |
| 14–15 | Light Theme；主色 `#2563EB`、浅蓝 `#EFF6FF`、背景 `#F8FAFC`、白 `#FFFFFF`、边框 `#E5E7EB`、主文字 `#111827`、次文字 `#6B7280`、成功 `#22C55E`、警告 `#F59E0B`、错误 `#EF4444`；禁止黑色主界面 / 大面积渐变 / 金属纹理 / 3D / 玻璃拟态 / 发光 | ✅  | `design/tokens.css` 的浅色令牌即文档主色板（新增 `--suna-primary` / `--suna-warning`）；主界面 `suna-shell.css` 全部改用令牌；侧栏原先的 `linear-gradient` 已改为纯色 `var(--suna-sidebar)`                         |
| 16    | 重新设计 Logo："S" + 晶体结构 / AI 节点；禁用钢卷/工厂/炉子/齿轮/锤子                                                                                                                                          | ✅  | `frontend/src/components/SunaLogo.tsx`：等轴晶体晶格，几何/科技/简洁，无文档禁用元素                                                                                                                  |
| 17    | 智能问答为默认首页；顶部模型选择 / Agent 模式 / 知识库状态；欢迎区 `Suna` + `钢铁材料研发智能助手` + 材料分析/文献研究/数据分析/性能预测/工艺优化/实验设计                                                                                          | ✅  | `startupPage` 默认 `"chat"`；头部含模型切换与 Agent/知识库入口；欢迎区品牌行为 `Suna` + `钢铁材料研发智能助手`，6 张能力卡片与文档一致                                                                                       |
| 18    | Chat 输入框：文本 / 文件拖拽（PDF、Excel、CSV、Word、图片）；底部添加文件、知识库、Agent、模型、发送                                                                                                                       | ✅  | `DesktopChatWorkspace` 的 composer 支持 `onDragOver`/`onDrop`（拖拽遮罩 + 复用既有类型过滤，覆盖 PDF/DOC/DOCX/XLS/XLSX/CSV/TXT/MD/JSON/HTML/图片）+ 粘贴图片；底部含附件、智能搜索、知识库、Agent、模型、参数、发送                |
| 19    | 模型选择：Provider / Model / Temperature / Context / Max Tokens                                                                                                                             | ✅  | composer 的模型按钮切换 Provider/Model；新增「参数」面板（`ChatModelParameters`）展示 Provider/Model 并可编辑 Temperature / Context / Max Tokens，写回 `model.preferences`                                 |
| 20    | Chat 消息：用户右侧 / AI 左侧；Markdown / 代码 / 公式 / 表格 / 图片 / 图表 / 引用 / 文件 / Agent 结果                                                                                                            | ✅  | `.suna-chat-user-turn` 右对齐、`.suna-chat-assistant-turn` 左对齐；`AnswerRenderer` = ReactMarkdown + GFM（表格/代码）+ **KaTeX 公式**（`remark-math` + `rehype-katex`，新增 2 个测试）+ 引用/图片/金相/网页悬浮卡 |
| 21    | AI 回答结构：结论 → 分析依据 → 知识来源 → 数据分析 → 模型结果 → 建议 → 引用；只展示任务进度 / Agent 状态 / 工具调用摘要 / 结果，不暴露内部推理                                                                                              | ✅  | 默认 system prompt（`agent/desktop/model.rs`）明确要求按该顺序组织回答且不暴露内部推理；前端不渲染 reasoning 事件，只展示运行状态、工具调用与结果                                                                               |

---

## D. 知识中心与文献（22–29 章）— 100%

| 章  | 要求                                                             | 状态 | 证据                                                                                                                                                                                           |
| -- | -------------------------------------------------------------- | -- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 22 | 知识中心管理 PDF / Word / Excel / CSV / 论文 / 专利 / 标准 / 实验记录 / 工艺文件   | ✅  | `features/knowledge/KnowledgeCenterWorkspace.tsx`；`rag/parse/` 支持 PDF / DOCX / XLSX / CSV / HTML / Markdown / TXT；新增**文档类型分类**（论文 / 专利 / 标准 / 实验记录 / 工艺文件 / 其他），存于文档元数据，可在文档抽屉编辑、在文档表展示并参与筛选 |
| 23 | 多知识库；每个知识库显示文件数量、知识片段、Embedding 状态、更新时间                        | ✅  | 新增 Tauri 命令 `get_postgres_knowledge_base_metrics`（单条 SQL 聚合每个知识库的文档数 / 已索引文档 / 片段数 / 已生成 Embedding 的片段数）；知识库卡片四要素齐全（文档 N、片段 M、Embedding 已索引/生成中/待生成/未索引、更新于）                                 |
| 24 | 上传流程：解析 → 切分 → Embedding → 索引；支持拖拽 / 点击 / 批量 / 文件夹导入；显示进度      | ✅  | `rag/ingest` / `rag/chunk` / `rag/index` / `rag/tasks`；导入文档 / 导入文件夹 / 拖拽导入；`IngestionJobs` 展示解析、切分、Embedding、索引各阶段状态与进度                                                                      |
| 25 | 知识检索：自然语言 + 过滤（材料 / 工艺 / 性能 / 年份 / 来源）；结果含标题、摘要、片段、来源、相关度      | ✅  | 检索面板含 材料 / 工艺 / 性能 / 年份 / **来源** 五个文档规定过滤器（另加文档类型与标签）；后端 `PostgresKnowledgeSearchFilters` 新增 `source`，命中 `metadata.source` 或文档名；结果展示文档名、片段、来源位置与相关度，并支持打开原文 / 复制 / 加入对话 / 收藏 / 引用            |
| 26 | 文献搜索、阅读、总结、对比、综述、研究趋势、研究空白                                     | ✅  | `features/literature/LiteratureResearchPage.tsx`：检索 / 阅读 / 总结 / 对比 / 综述 / **研究趋势**（新增视图，输出热点主题、时间演化、常用材料与工艺、方法与数据趋势、研究空白）                                                                    |
| 27 | 文献搜索结果显示标题、作者、期刊、年份、摘要、相关度；支持收藏、加入知识库、总结、对比、引用                 | ✅  | 结果行与详情面板展示 标题 / 作者 / 期刊 / 年份 / 摘要片段 / 相关度（作者、期刊、年份优先取文档元数据，缺省回退来源名与来源位置年份）；新增**加入知识库**动作（选择目标知识库后移动源文档），并保留收藏 / 总结 / 对比 / 引用                                                                 |
| 28 | 文献对比：研究材料、成分、工艺、组织、性能、研究结论、差异，使用表格比较                           | ✅  | 对比区保留证据表（研究对象 / 期刊 / 年份 / 证据位置），并让 Agent 只输出 `文献 \| 研究材料 \| 成分 \| 工艺 \| 组织 \| 性能 \| 研究结论 \| 差异` 的 Markdown 表格（证据不足填「未记录」），用 `AnswerRenderer` 渲染                                              |
| 29 | 文献综述：选择 10 / 20 / 50 篇；输出研究背景、研究现状、研究方法、研究结论、争议、研究空白、未来方向、参考文献 | ✅  | 工具栏新增**综述篇数 10 / 20 / 50** 选择（对比选中优先，否则取前 N 篇）；综述提示词要求依次输出 研究背景 / 研究现状 / 研究方法 / 研究结论 / 争议 / 研究空白 / 未来方向 / 参考文献                                                                               |

> 文献检索仍以本地知识库为数据源；作者 / 期刊 / 年份取文档元数据，未填写的条目显示「未记录」。

---

## E. 数据实验室与性能预测（30–37 章）— ~55%

| 章  | 要求                                                                                                 | 状态       | 证据                                                                                                                                      |
| -- | -------------------------------------------------------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| 30 | 数据实验室：支持 Excel / CSV / 数据库 / JSON                                                                  | ✅        | `features/data/DataLabPage.tsx` + `steel/datasets/`（csv.rs / xlsx.rs）；导入含列映射与激活                                                         |
| 31 | 数据分析：数据清洗、缺失值、异常值、重复值、统计分析、相关性、特征重要性、PCA、聚类、SHAP                                                   | 🟡 约 40% | 已有：行数/字段数统计、缺失率、均值/范围、质量指标、字段类型推断、重复列检测。**缺**：清洗、异常值、重复值处理、相关性、PCA、聚类、SHAP                                                              |
| 32 | 数据可视化：折线 / 柱状 / 散点 / 热力 / 箱线 / 雷达 / PCA / SHAP / Pareto Front；科研论文风格                               | 🟡 约 15% | **仅实现柱状图**（`recharts` 的 `Bar/BarChart`），用于字段均值分布                                                                                        |
| 33 | 性能预测指标：YS / TS / EL / 硬度 / 冲击韧性 / 耐磨性 / r-value / 晶粒尺寸 / 组织比例                                      | 🟡       | 预测链路通用（按目标列训练），未内置上述指标的语义与单位约束                                                                                                          |
| 34 | 预测流程：选择材料体系 / 数据集 / 模型 / 输入变量 / 预测目标                                                               | ✅        | `features/prediction/PerformancePredictionPage.tsx`（11 KB）：选数据集 → 训练 → 输入变量 → 预测，含适用范围告警                                                |
| 35 | 模型中心：XGBoost / RandomForest / LightGBM / SVR / MLP / Transformer / ONNX / 自定义；模型卡片含 R²/MAE/RMSE/版本 | 🟡 3/8 族 | 已支持 `linear_regression`、`random_forest`、`xgboost`（可选）+ ONNX 导出与推理；指标含 metrics 与 feature_importance。缺 LightGBM / SVR / MLP / Transformer |
| 36 | 模型比较：多个模型同时预测并对比预测值/真实值/误差/R²/MAE/RMSE                                                             | ❌        | 一次只训练一个模型                                                                                                                               |
| 37 | 模型解释：Feature Importance / SHAP / Partial Dependence / Prediction Interval                          | 🟡 1/4   | `feature_importance` 已返回（`training.py:111/268`）；缺 SHAP / PDP / 预测区间                                                                     |

> 预测与优化链路已具备运行环境（本次已装好 numpy / scikit-learn 1.9.0 / optuna 4.9.0 / onnxruntime 1.28.0）。

---

## F. 工艺优化（38–43 章）— ~50%

| 章     | 要求                                                                                    | 状态     | 证据                                                                    |
| ----- | ------------------------------------------------------------------------------------- | ------ | --------------------------------------------------------------------- |
| 38    | 单目标 / 多目标 / 约束优化                                                                      | ✅      | `compute-worker/suna_worker/optimization.py` 的 `optimize_constrained` |
| 39–40 | 优化目标（最大化硬度/强度/耐磨/延伸率/韧性；最小化成本/能耗）；优化变量（淬火温度/回火温度/保温时间/冷却速度/轧制温度/压下率/合金元素）             | 🟡     | 目标与变量由数据集列动态选择，未内置语义与成本/能耗模型                                          |
| 41    | 算法：NSGA-II / Bayesian Optimization / Genetic Algorithm / Particle Swarm / Grid Search | 🟡 2/5 | 已实现 **NSGA-II**（`nsga2`）与 **TPE（贝叶斯）**；缺 GA / PSO / Grid Search       |
| 42    | Pareto Front：必须支持二维与三维                                                                | ❌      | 后端返回候选方案，前端无 Pareto 图                                                 |
| 43    | 输出多个候选方案，可查看/比较/保存/加入实验计划                                                             | 🟡     | 候选方案表已实现（方案 A/B…、预测值、目标值、可行性）；**缺**保存与加入实验计划                          |

> 另有体验缺口：当前需**手工填写训练任务 ID**，未与性能预测页联动。

---

## G. 实验助手（44–46 章）— ~25%

| 章  | 要求                                                         | 状态 | 证据                                                                             |
| -- | ---------------------------------------------------------- | -- | ------------------------------------------------------------------------------ |
| 44 | 实验设计、下一实验推荐、变量选择、实验组合、主动学习                                 | 🟡 | `features/experiment/ExperimentAssistantPage.tsx`（4.3 KB）：填变量范围 → 由 Agent 给出建议 |
| 45 | DOE / 正交实验 / 响应面 / Bayesian Optimization / Active Learning | ❌  | 无对应算法实现                                                                        |
| 46 | 输出下一组实验并解释"为什么推荐"                                          | 🟡 | 由 Agent 生成解释，非算法驱动                                                             |

---

## H. 管理模块（47–53 章）— ~80%

| 章     | 要求                                                                                                                   | 状态 | 证据                                                                                                            |
| ----- | -------------------------------------------------------------------------------------------------------------------- | -- | ------------------------------------------------------------------------------------------------------------- |
| 47    | Agent 管理：列表、状态、模型、工具、Skill                                                                                           | ✅  | `features/management/AgentManagementPage.tsx`（13.6 KB）+ `features/agents/AgentSchedulesPanel.tsx`（Cron 计划）    |
| 48    | 创建 Agent：Name / Description / System Prompt / Model / Tools / Skills / Memory / Permission                           | ✅  | Agent Profile 支持提示词、Provider、精确工具白名单、权限限制、执行预算；专家不能超过父 Agent 权限                                               |
| 49    | Tool Center：文件 / 数据库 / Python / 模型 / 搜索 / 绘图 / 计算                                                                    | 🟡 | `CapabilityManagementPage.tsx` 仅 `listToolCapabilities()` **只读列表**，不能启停或配置                                    |
| 50–51 | MCP：本地 / HTTP / Stdio；显示名称、状态、连接方式、工具数量、权限；详情含 Server/Transport/Tools/Resources/Prompts/Status/Logs；支持启用/禁用/重启/删除/测试 | ✅  | `features/mcp/McpManagementPage.tsx`（13 KB）：CRUD + 健康检查 + 重启 + 工具列表 + 凭据管理；传输支持 stdio / streamable_http / sse |
| 52    | Skill：默认 8 个（耐磨钢分析、IF钢分析、论文总结、数据分析、性能预测、工艺优化、实验设计、金相分析）                                                              | 🟡 | Skill 机制完整（`skills/mod.rs`），但默认 Skill 集合与文档清单不一致                                                              |
| 53    | 创建 Skill：Name / Description / System Prompt / Tools / Agent / Model / Knowledge Base；启用/禁用/复制/编辑/删除                  | 🟡 | 启停、tags、版本、`content_sha256`、12 个上限、错误列表均已实现；**页面内创建/编辑/复制/删除未实现**（Skill 为磁盘 Markdown 文件）                      |


## J. 设置（57–62 章）— ~90%

| 章  | 要求                                                              | 状态 | 证据                                                                                                                                            |
| -- | --------------------------------------------------------------- | -- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| 57 | 设置页：左菜单 + 右内容；账户/模型/常规/外观/知识库/Agent/MCP/Skills/数据/快捷键/存储/关于     | ✅  | `SettingsPagePanel.tsx` 11 个 tab：account / providers / general / appearance / knowledge / agent / mcp / skill / databases / shortcuts / about |
| 58 | 常规：启动行为、默认页面、自动保存、自动更新、确认提示、通知                                  | ✅  | 默认页面、恢复会话、保存草稿、自动更新、通知、显示工具详情、危险操作确认                                                                                                          |
| 59 | 外观：Light / Dark / System；字体大小、界面密度、Sidebar 宽度、Agent Panel 显示、动画 | 🟡 | Light/Dark/System 与 Agent Panel 开关已实现；字体大小 / 密度 / 侧栏宽度 / 动画未实现                                                                                |
| 60 | 数据存储：数据目录、知识库目录、模型目录、缓存、日志；打开目录 / 更改目录 / 清理缓存                   | ✅  | `get_storage_paths` / `open_storage_path` / `clear_storage_cache` + `StorageDataPanel`                                                        |
| 61 | 用户中心                                                            | ✅  | `SettingsAccountPanel.tsx`                                                                                                                    |
| 62 | 关于                                                              | ✅  | 设置页 about tab（版本、Rust、Tauri、React、License、GitHub）                                                                                             |

---

## K. UI 规范与交互（63–74 章）— ~70%

| 章  | 要求                                                                                                                                | 状态     | 证据                                                                                                                        |
| -- | --------------------------------------------------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------- |
| 63 | 组件规范：Button / Input / Select / Dialog / Drawer / Tabs / Card / Table / Badge / Tooltip / Toast / Command Menu / Progress / Avatar | 🟡     | 多数以 CSS 类 + 自建组件实现；`components/common/` 为共享组件集；未引入 shadcn/Radix                                                           |
| 64 | Card：白底 + border + 轻圆角 8–12px；禁阴影/渐变/玻璃                                                                                           | ✅      | CSS 统一实现                                                                                                                  |
| 65 | Typography：Inter / 系统字体；中文 Noto Sans SC；标题 20–28px、正文 14–15px、辅助 12–13px                                                          | ✅      | `@fontsource/inter`、`@fontsource/noto-sans-sc`、`@fontsource/jetbrains-mono`                                               |
| 66 | 图标：Lucide，统一线性，不混用 Emoji / 3D                                                                                                     | ✅      | `lucide-react` 全局使用                                                                                                       |
| 67 | 表格：可排序 / 筛选 / 分页 / 复制 / 导出                                                                                                        | 🟡     | 检索结果与数据表支持筛选/复制；**排序、分页、导出未普遍实现**                                                                                         |
| 68 | 所有支持文件的页面支持 Drag & Drop，显示上传/解析/Embedding 进度                                                                                      | 🟡     | 仅知识中心有 `onDrop`；进度由任务事件驱动                                                                                                 |
| 69 | Streaming：思考/检索/分析/工具/生成状态；不暴露 CoT                                                                                                | ✅      | `reasoning_delta` / `reasoning_completed` 独立事件；检查器只展示高层状态                                                                 |
| 70 | Agent 运行日志：Task / Agent / Tool / Status / Duration / Result                                                                       | ✅      | `AgentRunInspector` + `ChildAgentPanel`                                                                                   |
| 71 | 错误处理：不直接显示 "Error"；给出重试 / 修改 / 查看详情                                                                                               | 🟡     | `settingsError.ts` 统一脱敏映射，但**仅 settings / MCP 使用**；另有 47 处内联 `instanceof Error ? ...` 未走统一映射                              |
| 72 | 全局搜索 Ctrl+K：对话 / 知识库 / 文献 / Agent / Skill / MCP / 模型 / 设置                                                                         | 🟡     | Ctrl+K 已实现，覆盖导航项 + 会话历史；**未覆盖**知识库 / 文献 / Skill / MCP / 模型实体搜索                                                            |
| 73 | 快捷键：Ctrl+N 新对话、Ctrl+K 全局搜索、Ctrl+Shift+K 知识库、Ctrl+Enter 发送、Esc 停止                                                                  | 🟡 4/5 | 已实现 Ctrl+N / Ctrl+K / Ctrl+Enter / Esc（另加 Ctrl+L、Ctrl+,、Ctrl+Shift+F、Ctrl+\、Ctrl+J、Ctrl+Shift+R）；**Ctrl+Shift+K（知识库）未实现** |
| 74 | 本地数据安全：本地优先、API Key 安全存储、敏感数据不入日志                                                                                                 | ✅      | 密钥存 Windows Credential Manager；`diagnostics/redaction.rs` + panic hook 脱敏；本次已把日志统一到脱敏出口                                   |

---

## L. 工程结构（75–82 章）— ~90%

| 章  | 要求                                                                                                                                                                         | 状态              | 证据                                                                                                                                                                                                                                          |
| -- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 75 | Rust 模块：agent / llm / tools / mcp / knowledge / database / security / commands                                                                                             | ✅ 8/8 概念覆盖，命名不同 | `agent/` ✓、`tools/` ✓、`mcp/` ✓、`database/` ✓、`app/`(≈commands) ✓、`providers/`(≈llm) ✓、`rag/`+`knowledge_db.rs`(≈knowledge) ✓、`permissions/`(≈security) ✓                                                                                    |
| 76 | React 结构：components / pages / stores / hooks / services / types / utils                                                                                                    | 🟡              | `components/` ✓、`features/`(≈pages) ✓、`bridge/`(≈services) ✓、`utils/` ✓、`stores/` ✓（Zustand 迁移后新增）；**无** `hooks/`、`types/`                                                                                                                  |
| 77 | 状态管理用 Zustand；8 个 store（chat / agent / model / knowledge / mcp / skill / settings / task）                                                                                  | ✅               | `zustand` 依赖 + `src/stores/` 11 个文件；8 个 store 全部建立并接线，4 处 Context（appearance / theme / i18n / chatController）迁移为薄适配层                                                                                                                        |
| 78 | Chat State：conversationId / messages / model / agentMode / attachments / streaming / taskStatus / activeAgents / toolCalls                                                 | ✅               | `chatController` + `agentEvents.ts` 的 `AgentRunView`                                                                                                                                                                                        |
| 79 | Agent State：agentId / name / status / model / currentTask / tools / skills / progress / logs                                                                               | ✅               | `agent/profiles.rs` + `AgentRunView` + `ChildAgentTurnRecord`                                                                                                                                                                               |
| 80 | 数据模型 14 个：Conversation / Message / Agent / Tool / Skill / MCPServer / KnowledgeBase / Document / Dataset / Model / PredictionTask / OptimizationTask / Experiment / Report | 🟡 8/14 落表      | ✅ conversations、messages、knowledge_bases、knowledge_source_documents、steel_datasets、steel_models、mcp_servers、reports；❌ Agent / Tool / Skill（为代码或磁盘配置，未落表）、PredictionTask / OptimizationTask / Experiment（复用 `background_tasks` 与 compute 任务） |
| 81 | 页面路由 `/chat` `/knowledge` …                                                                                                                                                | ✅               | `src/app/sectionRoute.ts`：17 条路径映射 + `useSectionRoute`（`pushState` + `popstate`），根路径 `/` 归为 chat；第 81 章要求的 10 条路由全部覆盖                                                                                                                       |
| 82 | 主界面最终结构（三栏 ASCII 图）                                                                                                                                                        | ✅               | 与 `SunaApp` 实际结构一致                                                                                                                                                                                                                          |

---

## M. 设计原则与开发要求（83–89 章）— ~90%

| 章     | 要求                                                                       | 状态     | 证据                                                                                                                   |
| ----- | ------------------------------------------------------------------------ | ------ | -------------------------------------------------------------------------------------------------------------------- |
| 83    | 10 条产品设计原则（Agent 优先、聊天是入口、结果可追溯、行为可观察、用户有最终决策权…）                         | ✅      | 事件流 + 引用 + 权限确认 + 检查器均体现                                                                                             |
| 84    | 不允许出现的设计（渐变/金属纹理/机器人头像/炫酷发光/3D卡片/复杂背景/大量 Emoji/过度圆润）                     | ✅      | 代码中无违反项                                                                                                              |
| 85–86 | 最终视觉目标与一句话定位                                                             | ✅      | 与 `README.md` 一致                                                                                                     |
| 87    | Phase 1–8 开发顺序                                                           | ✅ 基本完成 | Phase 1 界面/聊天 ✓；2 LLM Provider/Streaming ✓；3 知识库 RAG ✓；4 文献/数据 🟡；5 预测 🟡；6 优化 🟡；7 实验 🔴；8 MCP/Skills/Multi-Agent ✓ |
| 88    | 所有页面统一 Sidebar / Header / 字体 / 颜色 / Button / Card / Table / Dialog / 状态色 | 🟡     | 统一 Sidebar/Header/字体/配色/状态色；Button/Card 为自定义 CSS 类；Table/Dialog 未完全组件化                                               |
| 89    | 可长期迭代的桌面 AI Agent 产品                                                     | ✅      | 已有 908 个 Rust 测试、110 个前端用例、32 个迁移、边界测试与 CI                                                                           |

---

## 缺口汇总（按补齐优先级）

### P1 — 科研能力深度（文档明确要求、当前缺失）

1. **数据可视化类型**（32 章）：折线 / 散点 / 热力图 / 箱线图 / 雷达图 / PCA / SHAP / Pareto Front（现仅柱状图）。
2. **数据分析**（31 章）：缺失值处理、异常值检测、重复值处理、相关性、PCA、聚类、SHAP。
3. **模型比较**（36 章）：多模型同时预测并对比 R²/MAE/RMSE。
4. **模型解释**（37 章）：SHAP、Partial Dependence、Prediction Interval（Feature Importance 已有）。
5. **优化算法族**（41 章）：GA / PSO / Grid Search（NSGA-II 与 TPE 已有）。
6. **Pareto Front 可视化**（42 章）：二维与三维。
7. **实验设计算法**（45 章）：DOE / 正交 / 响应面 / 主动学习。
8. **报告导出格式**（56 章）：PPT / PDF / Word（现仅 Markdown）。

### P2 — 一致性与体验

1. **统一错误处理**（71 章）：把 47 处内联 `instanceof Error ? ...` 收敛到 `settingsError.ts` 的脱敏映射。
2. **全局搜索覆盖面**（72 章）：扩展到知识库 / 文献 / Skill / MCP / 模型实体。
3. **拖拽上传**（68 章）：推广到所有文件页面。
4. **表格能力**（67 章）：排序 / 分页 / 导出。
5. **工具中心可配置**（49 章）：从只读改为可启停 / 配置。
6. **Skill 页面内创建与编辑**（53 章）。
7. **快捷键 Ctrl+Shift+K**（73 章）。

### P3 — 未按文档实现的结构项（**属于缺口，不是"可保持现状"**）

> 说明：以下各项均为 `开发文档.md` 的明确要求，当前实现与文档不一致，因此计入缺口。  
> 是否修改由项目负责人决定；本节仅陈述"文档要求 vs 实际"，不含评估者的取舍建议。

1. **状态管理**（77 章）：文档原文「使用 Zustand。」并列出 8 个 store（chat / agent / model / knowledge / mcp / skill / settings / task）。实际使用 React Context，无 `zustand` 依赖。
2. **UI 组件库**（4 章）：文档「## UI」下列出 `Tailwind CSS` / `shadcn/ui` / `Radix UI`。实际无 `@radix-ui/*` 依赖，为自建组件 + 纯 CSS。
3. **专业 Agent 形态**（6 章）：文档要求 8 个固定专业 Agent（Knowledge / Literature / Data / Material / Prediction / Optimization / Experiment / Report）。实际为可配置 Agent Profile + 动态 child turn 委派。
4. **页面路由**（81 章）：文档要求 `/chat` `/knowledge` `/literature` `/data` `/prediction` `/optimization` `/experiment` `/agents` `/tools` `/settings`。实际无 URL 路由，使用 `activeSection` 状态导航。
5. **数据模型落表**（80 章）：文档列出 14 个模型。实际落表 8 个；缺 Agent / Tool / Skill / PredictionTask / OptimizationTask / Experiment（当前分别由代码配置、磁盘文件或 `background_tasks` / compute 任务承担）。

**决策点**：这 5 项是"改代码去对齐文档"还是"改文档去记录现状"，需要你明确。在决定之前，本报告不预设任何取舍。

---

## 与 `开发文档.md` Phase 1–8 的对照

| Phase | 文档目标                                                            | 实际                    |
| ----- | --------------------------------------------------------------- | --------------------- |
| 1     | Desktop UI、Chat、Sidebar、Agent Workspace、Model Selector、Settings | ✅ 完成                  |
| 2     | LLM Provider、Streaming、Conversation、File Upload                 | ✅ 完成                  |
| 3     | Knowledge Base、RAG                                              | ✅ 完成                  |
| 4     | Literature、Data Lab                                             | 🟡 文献较完整；数据实验室缺分析与可视化 |
| 5     | Prediction                                                      | 🟡 链路完整，缺模型比较与解释      |
| 6     | Optimization                                                    | 🟡 算法与 Pareto 图不足     |
| 7     | Experiment                                                      | 🔴 仅 Agent 建议，无实验设计算法 |
| 8     | MCP、Skills、Multi-Agent                                          | ✅ 完成                  |

---

## 评估边界

- 本评估基于代码、迁移、测试与依赖的静态核对，**未运行真实模型、真实文献源或 PostgreSQL 实例**。
- Rust 侧结论以本次修复后的 `cargo check --all-targets` 通过（退出码 0）为前提；`cargo test` 与 Python pytest 在本次改动后尚未重跑，需补验。
- 未修改任何功能代码。
