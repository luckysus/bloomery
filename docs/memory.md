# Memory

Bloomery keeps short-term and long-term memory local to the desktop workspace.
The Rust host reads and writes memory through the local SQLite database; the Web
application, Web login state, and private cloud APIs are not part of this path.

## What enters context

For each agent turn, Bloomery builds a bounded context packet from:

- the current conversation summary;
- recent messages from the current conversation;
- relevant confirmed memories;
- relevant cross-conversation history hits.

Bloomery does not inject the full memory catalog into the prompt. Only memories
that are both `confirmed` and relevant to the current query are selected.
The final assistant `response_json` stores a compact `memory.selected` audit
with the selected memory count and layers, so the chat UI can show what affected
the answer without exposing the full memory catalog.

## Long-term memory layers

Long-term memories are grouped by type so ordinary facts do not become system
instructions:

| Layer | Stored as | Typical content |
| --- | --- | --- |
| User profile | `user_profile` | Response style, preferred defaults, common provider choices |
| Domain memory | `domain_memory` | Steel grades, process parameters, standards, material facts |
| Task memory | `task_memory` | Long-running tasks, unfinished work, follow-up state |
| Reflection memory | `reflection_memory` | Corrections, failed assumptions, user feedback, lessons learned |

Legacy memory types are mapped into these layers when context is built.

## Candidate lifecycle

Automatic extraction creates memory candidates, not durable instructions:

| Status | Enters prompt | Enabled | Meaning |
| --- | --- | --- | --- |
| `pending` | No | No | Suggested by the agent and waiting for user confirmation |
| `confirmed` | Yes, when relevant | Yes | Approved by the user or manually saved |
| `rejected` | No | No | Rejected by the user and skipped by future duplicate suggestions |

Users can confirm, reject, disable, archive, restore, or delete memory records
from the local memory UI. Pending and rejected candidates cannot be enabled
directly; they must be confirmed first.

## Recall fallback

Bloomery currently keeps keyword recall as the local baseline. SiliconFlow
embedding and reranker providers can improve retrieval when configured, but a
missing embedding key must never block normal chat. The agent falls back to local
keyword recall and surfaces provider configuration separately.

## 中文说明

Bloomery 的记忆全部归属本地工作区，由 Rust 主机写入本机 SQLite。每轮对话只会
注入当前会话摘要、最近消息、相关的已确认记忆，以及跨会话命中的少量历史片段，
不会把完整记忆列表塞进 prompt。
回答保存时会在 `response_json.memory.selected` 里记录本轮选中的记忆摘要和
层级，前端可显示“本轮用了几条记忆”，但不会展示完整记忆目录。

长期记忆分为四类：用户偏好 `user_profile`、钢铁领域事实
`domain_memory`、长期任务状态 `task_memory`、修正与复盘
`reflection_memory`。自动抽取出来的内容默认是 `pending` 候选，用户确认后才
变成 `confirmed` 并允许进入上下文；`rejected` 候选不会进入 prompt，也会用于
去重，避免同一条被反复提示。

没有配置 SiliconFlow embedding 或 reranker 时，对话不能被阻塞；Bloomery 会退回
本地关键词召回，并在需要时提示用户补充 Provider 配置。
