# Agent Runtime / Agent Loop

## 目标

实现真实、可观测、可取消、可测试的 Agent Runtime。Agent Loop
把用户目标转成受约束的模型调用、工具执行与结果整合。

## 实体

Task、Plan、PlanStep、AgentDefinition、AgentRun、ToolCall、TaskEvent、Checkpoint。保存状态、时间、输入输出摘要、错误码、重试次数和必要的来源信息；限制事件
payload 大小并过滤敏感内容。

## 状态机

Task：queued → planning → running → waiting_approval/paused →
completed/failed/cancelled。终态不能被普通进度事件覆盖。所有非法迁移都应返回明确错误。

## 循环

1.  校验输入和权限；
2.  创建任务并发出事件；
3.  生成有限计划，或对简单问题直接回答；
4.  校验 Agent、工具、步数、风险与预算；
5.  执行步骤并构造最小必要上下文；
6.  模型返回结构化响应：最终答复、工具调用或需要补充信息；
7.  对工具请求执行 schema、权限和审批检查；
8.  执行工具、记录结果并作为下一轮上下文；
9.  检查取消、超时、最大迭代数和工具次数；
10. 必要时有限重试或重新规划；
11. 保存最终结果、来源摘要和事件。

禁止直接执行模型生成的任意代码。Planner
输出目标、依赖、输入、成功条件和风险；Executor 只执行已批准步骤。子
Agent 必须通过 Registry 注册，不要在主循环硬编码所有分支。

## 事件

task_started、plan_created、agent_started、agent_progress、tool_started、tool_finished、approval_required、task_progress、task_paused、task_resumed、task_completed、task_failed、task_cancelled。事件包含
task_id、sequence、timestamp、type、payload 和 schema_version。

## 可靠性

取消要传播到实际请求；模型、工具和任务都有超时；重试有限且有退避；checkpoint
恢复时避免重复执行非幂等工具。支持事件重连、去重和历史补拉。

## 安全

工具最小权限；文件访问限定工作区；数据库工具默认只读；破坏性/昂贵操作需确认；模型输出视为不可信；不暴露隐藏推理链，只显示计划摘要、工具摘要、证据和结果。

## 测试

Fake LLM/Tool 覆盖直接回答、工具调用、schema
错误、超时、取消、重试上限、审批、事件顺序、checkpoint
恢复、最大步数。另做至少一个真实 Provider 的集成验收。
