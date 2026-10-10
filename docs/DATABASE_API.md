# 数据库、API 与事件契约

## 数据库

PostgreSQL 为默认持久化层；使用版本化迁移，不得启动时 drop/recreate
用户数据。建议实体：conversations、messages、attachments、tasks、task_steps、agent_definitions、agent_runs、tool_calls、task_events、checkpoints、model_providers、model_profiles、tools、mcp_servers、skills、knowledge_bases、knowledge_documents、document_chunks、datasets、prediction_runs、optimization_runs、experiment_runs、app_settings、audit_logs。按真实需求逐步建表，避免无用字段。

## API

统一 `/api/v1`。错误结构：
`{ "error": { "code": "...", "message": "...", "request_id": "...", "details": {} } }`
列表分页格式统一；输入必须验证；不把内部堆栈返回给用户。

## 最小接口

-   `POST/GET /api/v1/conversations`
-   `GET /api/v1/conversations/{id}`
-   `POST /api/v1/conversations/{id}/messages`
-   `POST /api/v1/tasks`
-   `GET /api/v1/tasks/{id}`
-   `POST /api/v1/tasks/{id}/cancel`
-   `POST /api/v1/tasks/{id}/pause`
-   `POST /api/v1/tasks/{id}/resume`
-   `GET /api/v1/tasks/{id}/events`
-   `GET /api/v1/models`
-   `GET/POST /api/v1/knowledge-bases`
-   `POST /api/v1/knowledge-bases/{id}/documents`
-   `POST /api/v1/knowledge-bases/{id}/search`

## 事件

事件含
event_id、task_id、sequence、timestamp、type、payload、schema_version。客户端需处理重连、去重和历史补拉。不得广播密钥、隐藏提示词或完整私密文档。

## 一致性与安全

任务状态和首条事件应保持一致；关键状态变更使用事务。SQL
参数化、文件路径校验、上传限制、权限检查和日志脱敏不可缺少。
