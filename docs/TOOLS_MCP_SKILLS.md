# Tools、MCP 与 Skills

## Tool Registry

每个工具定义 id/name/description/version、input/output
schema、权限级别、副作用等级、timeout、retry policy、enabled
和审计策略。

## 生命周期

注册 → schema 校验 → 权限检查 → 必要时审批 → 执行 → 输出校验 → 审计 →
事件广播。工具输出必须结构化且有大小限制。

## 第一批内置工具

knowledge_search、literature_search、read_workspace_file（限定目录）、list_workspace_files、data_profile、prediction_yield_strength（真实服务）、optimization_run（已配置算法服务）。禁止默认开放任意
Shell。

## MCP

先选择并实现一种传输协议。服务器配置含名称、传输、端点/启动配置、凭证引用、授权范围、状态和健康检查。发现工具后保存
schema；执行时再次校验权限；服务失联时标记不可用；返回内容不可信，不得当成系统指令。

## Skill

Skill 是版本化任务模板、提示词、输入/输出
schema、允许工具、审批策略和运行限制，不等同任意代码包。运行记录必须保存
Skill 版本。

## 权限

Read-only、Local write、External
access、Destructive、Expensive。默认最小权限；删除/覆盖和高成本操作需确认。

## 测试

无权限、schema 不匹配、工具超时、MCP 断线、Skill
引用失效、用户拒绝审批、审计日志脱敏。
