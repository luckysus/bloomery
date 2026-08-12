# Skills 扩展

Bloomery 使用基于 Markdown 的 `SKILL.md` 文件描述可复用的提示词和流程说明。为兼容现有生态，当前加载器支持 Claude Code 常用的 `.claude/skills/<skill-name>/SKILL.md` 目录布局。

Bloomery reads read-only instruction packs from a standard `SKILL.md`
directory layout. For compatibility with existing ecosystems, the current
loader accepts the commonly used Claude Code layout:

```text
.claude/skills/<skill-name>/SKILL.md
```

The loader checks three scopes in precedence order: user, workspace, and active
domain. A higher-precedence Skill with the same name shadows lower scopes. The
catalog reports the shadowed file as a non-fatal duplicate error so a malformed
or incompatible Skill never prevents other Skills from loading.

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

## Safety and runtime behavior

Skills contribute prompt instructions and metadata only. They do not receive
file, Shell, network, MCP, or secret permissions. Enabled Skill bodies are
bounded before they enter the Agent context, and each run records the exact
`name@version#sha256` values that were enabled. The frontend receives summaries
and content fingerprints, never the private prompt body.

## 目录布局与兼容性

Bloomery 支持 `.claude/skills/<name>/SKILL.md` 目录结构，并按用户、工作区、
当前领域包三个范围加载。用户范围优先级最高，领域包范围最低；同名文件
会产生可见的非致命冲突记录。

Skill 只能提供提示词和流程说明，不能获得文件、Shell、网络、MCP 或密钥
权限。启用的版本和 SHA-256 指纹会记录在 Agent 运行上下文中，前端只显示
摘要和指纹，不返回完整提示词正文。
