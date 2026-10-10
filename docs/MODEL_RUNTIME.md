# 模型 Runtime 与模型管理

## Provider 接口

统一定义配置校验、连接测试、chat、stream_chat、工具调用能力、结构化输出能力和取消请求。响应包含文本/工具调用、finish
reason、usage、provider/model 标识和 request_id。Embedding
与聊天模型分开建模。

## 配置

Provider、base_url、credential_reference、启用状态、超时；ModelProfile
包含模型 ID、用途、temperature、max_tokens、能力和默认标记。用途区分
chat/planning/embedding/reranking。

## 安全

密钥存系统安全存储或加密存储；UI 仅显示脱敏状态；日志过滤
Authorization、API Key、token 和密码；测试失败不能静默替换旧配置；`.env`
不提交。

## 路由与流式输出

支持默认模型和按用途选择模型。科研预测不得静默路由到普通聊天模型。统一流事件并处理断线、重复事件、取消和最终消息持久化。

## UI

Provider/模型列表、用途、连接状态、默认标记、测试时间；新增/编辑/禁用/删除、设默认、连接测试。删除被
Agent 引用的模型时要求替代方案。

## 测试

无凭证、无效凭证、超时、服务端错误、流中断、取消、不支持工具调用、结构化输出错误、密钥脱敏和默认模型持久化。
