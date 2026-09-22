# Bloomery PostgreSQL 知识库实施计划

## Summary

Bloomery 的知识库使用单个 PostgreSQL 作为唯一知识库后端。SQLite 继续保存客户端设置、会话、Agent 状态和普通应用任务；原始文件和解析资产继续保存在本地应用数据目录。

首期交付文档治理、RAG、Wiki 和知识图谱，不实现旧 SQLite 知识库兼容接口，不引入其他数据库或基础设施。

## PostgreSQL 配置

- 客户端检测并引导用户安装、配置 PostgreSQL，不通过 `winget` 自动安装。
- 配置地址、端口、数据库名、用户名和密码。
- 密码使用 Windows Credential Manager 保存；密码、完整连接串不得写入 SQLite 或日志。
- 初始化前校验连接、权限、版本和 `pgvector`；缺失 `pgvector` 时阻止 RAG 初始化。
- 初始化失败时保留原配置，不标记知识库可用。

Tauri 接口：`test_knowledge_database`、`configure_knowledge_database`、`initialize_knowledge_database`、`get_knowledge_database_health`、`disconnect_knowledge_database`。

## 数据模型与规则

PostgreSQL 使用单数据库、单 schema，包含知识库、源文档、文档版本、资产、Chunk、向量、Wiki 页面与版本、标签、知识边、导入任务、失败记录和检索审计表。

- 文档按 SHA-256 去重，内容变化创建不可变新版本。
- 只有激活版本参与检索。
- Chunk 保存顺序、标题路径、页码和原始位置。
- Wiki 正文、revision、标签和关系在同一事务中提交。
- 删除默认软删除或标记孤儿，不删除原始文件和历史数据。

## 统一流程

```text
本地文件 -> hash -> 解析/OCR -> 文档版本 -> parent/child chunks
-> PostgreSQL 全文索引 -> Embedding + pgvector -> 激活版本
-> RAG / Wiki / 图谱
```

任务状态：`pending`、`running`、`completed`、`failed`、`retrying`、`quarantined`。

## RAG、Wiki 与图谱

RAG 使用 PostgreSQL `tsvector`、pgvector、parent-child chunk、RRF 混合排序和现有 Reranker，返回 citation、source location，并记录 retrieval audit。Embedding 或 Reranker 失败时返回明确降级状态。

Wiki 支持 Markdown 创建、编辑、预览、标签、源文档关联、页面链接、revision、差异和回滚。知识图谱从 Wiki 链接和显式关系重建，并提供知识边查询。

## 实施顺序与验收

1. PostgreSQL 驱动、连接池、Credential Manager、迁移、pgvector 检测和健康检查。
2. PostgreSQL 知识库状态与 Tauri 配置命令。
3. 文档解析、Chunk、版本和全文索引写入 PostgreSQL。
4. Embedding、pgvector、全文/向量/混合检索、Reranker、citation 和 Agent 接入。
5. Wiki 页面、revision、标签、来源关联、知识边和图谱查询。
6. 前端配置、导入、Wiki、图谱和失败任务界面。
7. Rust、前端、集成测试，中文提交，推送 GitHub/Gitee，并检查 Actions。

验收必须覆盖连接和权限错误、初始化失败保护、迁移幂等、Hash 去重、版本不可变、Chunk 一致性、检索排序、降级、引用、Wiki revision/回滚、标签与关系事务、任务重试隔离和密码不落 SQLite/日志。

明确不做：旧 SQLite 知识库迁移或兼容、删除现有 SQLite/原始文件/数据库文件/向量数据，以及引入 Milvus、Qdrant、Elasticsearch、Neo4j 或 Redis。
