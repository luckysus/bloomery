# Suna 系统架构

## 默认分层

1.  **Tauri 2 Desktop**：窗口、文件对话框、桌面权限和打包。
2.  **React + TypeScript + Vite**：页面、交互、表单、状态呈现。
3.  **Rust Core**：任务生命周期、Agent
    Loop、调度、工具权限、事件与运行上下文。
4.  **Python FastAPI Research
    Service**：文档解析、Embedding、数据分析、机器学习预测和优化算法。
5.  **PostgreSQL**：会话、消息、任务、事件、配置、文档元数据与运行记录。
6.  **File
    Storage**：本地可配置目录存储原文件；数据库保存元数据、哈希和状态。
7.  **pgvector adapter**：提供可替换的向量检索接口。

## 边界

UI 不直接实现复杂规划、文件解析或科研算法。Domain 不依赖 React
或具体模型
SDK。模型、Embedding、VectorStore、Parser、预测器、优化算法均通过接口隔离。

## 通信

选择并记录一条主通信路径。推荐 UI → Tauri/应用 API → Rust Core；Rust
Core 通过版本化 HTTP API 或定义清楚的 IPC 调用 Python
服务；任务事件由统一事件模型桥接到 UI。不要无理由重复实现
REST、SSE、WebSocket 三套通道。

## 目录建议

`src/` 前端；`src-tauri/` Rust/Tauri；`research_service/`
Python；`migrations/` 数据库迁移；`docs/` 文档与 ADR；`tests/` 测试。

## 安全与数据

迁移必须版本化，不得静默清空旧数据。路径要规范化，防止路径穿越；SQL
参数化；密钥使用安全存储或加密方案，日志脱敏。所有耗时任务有超时、取消和错误状态。

## 架构决策

重要变更写入
`docs/adr/`，说明背景、决定、替代方案与影响。先审计现有项目再实施，不要为了符合新项目模板而重写已有代码。
