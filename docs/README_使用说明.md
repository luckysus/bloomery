# Suna 完整开发提示词包

本包把 Suna 拆成独立、互相引用的开发提示词，不是一个超级 Prompt。请先让
Codex 执行 `CODEX_EXECUTION_PROMPTS.md` 的 Prompt
0，审计现有仓库，再按阶段开发。

## 文档

-   `PRODUCT.md`：产品定位、功能边界、业务流程。
-   `ARCHITECTURE.md`：Tauri、React、Rust Core、Python
    科研服务、数据库。
-   `UI_DESIGN.md`：统一视觉规范和页面清单。
-   `DEVELOPMENT_RULES.md`：Codex 开发纪律。
-   `ROADMAP.md`：阶段顺序和验收门禁。
-   `AGENT_RUNTIME.md`：Agent Loop、任务状态机、事件和取消。
-   `MODEL_RUNTIME.md`：模型 Provider、密钥、流式输出。
-   `TOOLS_MCP_SKILLS.md`：工具、MCP、Skill。
-   `DATABASE_API.md`：数据模型、API、事件契约。
-   `KNOWLEDGE_RAG.md`：知识中心和 RAG。
-   `LITERATURE_RESEARCH.md`：文献研究。
-   `DATA_LAB.md`：数据实验室。
-   `PREDICTION.md`：性能预测。
-   `OPTIMIZATION_EXPERIMENT.md`：工艺优化和实验助手。
-   `SETTINGS_ADMIN.md`：设置中心及管理页面。
-   `CHAT_WORKSPACE.md`：主界面、会话和 Agent Workspace。
-   `TESTING_DEPLOYMENT.md`：测试、可靠性和打包。
-   `CODEX_EXECUTION_PROMPTS.md`：可直接复制给 Codex 的阶段指令。

## 默认技术路线

桌面：Tauri 2 + Rust；前端：React + TypeScript + Vite；UI
优先复用已有组件，新项目可使用 Tailwind/shadcn；状态：Zustand；核心
Agent Runtime：Rust；科研与机器学习服务：Python
FastAPI；数据库：PostgreSQL；向量检索：pgvector 适配层。

若现有仓库技术栈已经确定，Codex
必须先审计并采用最小改动策略，不得为遵守文档而无理由重写项目。

**重要：提示词可以定义完整开发范围，但不等于软件已经实现。必须以代码、测试、真实服务联调和验收为准。**
