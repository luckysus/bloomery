# Codex 阶段执行提示词

## Prompt 0：仓库审计（先执行）

请阅读 docs 中的
PRODUCT、ARCHITECTURE、UI_DESIGN、DEVELOPMENT_RULES、ROADMAP。只审计，不要大规模写代码：检查目录、依赖、现有
UI、Tauri/Rust、Python 服务、数据库迁移、测试和 Git
状态；记录当前基线错误；输出
`docs/IMPLEMENTATION_STATUS.md`，区分已实现、部分实现、未实现和环境阻塞；不删除/覆盖用户数据；给出有依赖顺序的阶段计划。完成后停止等待确认。

## Prompt 1：工程基线

按审计结果修复最小启动阻塞，统一配置、日志和错误处理。不得重写无关模块。运行检查并更新状态文档。

## Prompt 2：UI Shell

按 UI_DESIGN 和 CHAT_WORKSPACE 实现路由、Design
Token、Sidebar、Header、三栏布局、主题与空/加载/错误状态。复用现有组件，参考用户
UI 截图；提供构建结果和偏差清单。

## Prompt 3：数据库/API

按 DATABASE_API
实现最小必要迁移、Repository、健康检查、会话/消息/任务/事件
API。迁移不能清空旧数据，补充集成测试。

## Prompt 4：模型 Runtime

按 MODEL_RUNTIME 实现 Provider
抽象、凭证安全、连接测试和流式输出，至少接入一个真实
Provider。无密钥时明确提示，不伪造成功。

## Prompt 5：Agent Loop

按 AGENT_RUNTIME
实现状态机、Planner/Executor/Dispatcher、事件、取消、超时、有限重试和测试。先跑通一个最小闭环，不要一次实现所有
Agent。

## Prompt 6：Tools/MCP/Skills

按 TOOLS_MCP_SKILLS 先做只读工具、权限和审计，再实现一种 MCP 传输方式与
Skill 版本管理。禁止任意 Shell。

## Prompt 7：知识中心

按 KNOWLEDGE_RAG 实现真实
CRUD、上传、解析、切分、Embedding、pgvector、检索、引用、失败重试和状态。Mock
必须显式标记。

## Prompt 8：文献/数据

分别按 LITERATURE_RESEARCH 和 DATA_LAB
实现真实文件导入、解析/搜索或数据导入、分析、导出。一次聚焦一个模块。

## Prompt 9：性能预测

按 PREDICTION
接入真实屈服强度服务，严格执行意图路由和参数校验。未支持指标不得误调用。

## Prompt 10：优化/实验

按 OPTIMIZATION_EXPERIMENT
先用可复现测试函数跑通算法，再接真实预测器，保存约束和结果。

## Prompt 11：设置管理

按 SETTINGS_ADMIN 实现所有设置页，保证设置持久化并影响运行行为。

## Prompt 12：发布验收

按 TESTING_DEPLOYMENT 执行适用测试、构建、干净环境启动和端到端验证，输出
`docs/RELEASE_CHECKLIST.md`，列出通过/失败、未实现、Mock
与外部配置要求。

## 每阶段结束报告

说明实现内容、修改文件、执行命令和测试结果、真实服务与 Mock
边界、未完成项和下一阶段建议。不能把"代码写完"当成"功能完成"；测试没运行必须说明原因。
