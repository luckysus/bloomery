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
compatibility: bloomery>=0.1.0
---

Use source evidence and preserve units in every conclusion.
```

The name uses ASCII letters, digits, `-`, `_`, or `.`, and the version is
`major.minor.patch`. The optional compatibility field can be a Bloomery version
constraint or a comma-separated list. Files are size bounded and invalid UTF-8,
frontmatter, versions, and compatibility constraints are isolated as catalog
errors.

## 安全与运行行为

Skill 只能提供提示词和流程说明，不能获得文件、Shell、网络、MCP 或密钥权限。
启用的版本和 SHA-256 指纹会记录在 Agent 运行上下文中；前端只显示摘要和指纹，
不返回完整提示词正文。
