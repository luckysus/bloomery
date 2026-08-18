# Skills 扩展

Bloomery 的 Skills 是只读的 Markdown 指令包：每个 Skill 由一个 `SKILL.md`
文件描述可复用的提示词、工作流程和兼容信息。它是 Bloomery 自己的扩展机制，
不要求安装或使用其他智能体产品。

将一个 Skill 放入以下目录即可被发现：

```text
~/.bloomery/skills/<skill-name>/SKILL.md
```

Bloomery 只从这个用户目录加载 Skills。将一个子目录连同其中的 `SKILL.md`
复制到这里，即可在应用的扩展页中发现和启用它。

## File format

`SKILL.md` must be UTF-8 and begin with a small YAML-like frontmatter block:

```markdown
---
name: steel-review
description: Review steel process evidence
version: 1.0.0
tags: [steel, review]
compatibility: bloomery>=0.1.0
---

Use source evidence and preserve units in every conclusion.
```

The name and tags use ASCII letters, digits, `-`, `_`, or `.`, and the version
is `major.minor.patch`. The optional compatibility field can be a Bloomery
version constraint or a comma-separated list. Files are size bounded and invalid
UTF-8, frontmatter, names, tags, versions, and compatibility constraints are
isolated as catalog errors.

## Discovery and loading

At startup and when the Extensions page refreshes, Bloomery scans only metadata:

- name;
- description;
- version;
- tags;
- compatibility;
- content SHA-256.

The full Markdown body is loaded into the agent prompt only after the Skill is
enabled and selected for the current run. Each run records the loaded Skill
name, version, content hash, and trigger reason so the UI can explain which
instructions affected the answer.

Selection is metadata-first. Bloomery compares the current user request with
the enabled Skill name, description, and tags before reading the full body into
the prompt. If no enabled Skill matches, Bloomery falls back to the enabled
set so a user-selected Skill still works predictably.

## 安全与运行行为

Skill 只能提供提示词和流程说明，不能获得文件、Shell、网络、MCP 或密钥权限。
如果 Skill 需要外部动作，必须通过 Bloomery 内置工具或 MCP 工具，并继续走本地
权限确认。Bloomery 会先用 name、description、tags 和当前问题做轻量匹配，
命中后才把正文加入本轮 prompt；如果没有命中，会退回已启用列表，避免用户主动
启用的 Skill 被完全忽略。启用的版本、SHA-256 指纹、本轮加载记录和触发原因会
记录在 Agent 运行上下文中；前端默认显示摘要、标签、版本和指纹，不返回完整提示词正文。
