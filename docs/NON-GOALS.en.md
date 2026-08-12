# Product Boundaries

[简体中文](NON-GOALS.md) · English

Bloomery is a local-first domain agent workbench. This page describes what the product intentionally does not provide, so users and contributors can understand its boundaries.

## 1. No Bloomery account or project-specific cloud service required

The local workspace can be used without signing in to Bloomery. Users configure their own model, embedding, reranker, document-parsing, and other external services; the project does not require a maintainer-operated backend.

## 2. No redistribution of restricted standards text

Steel and materials standards may be referenced by identifier or source, but the project does not copy or redistribute standards text that is subject to copyright, licensing, or access restrictions. Users are responsible for confirming their rights to import and use source materials.

## 3. No high-risk actions without confirmation

File writes, shell commands, and other high-risk tool actions are not performed silently without the user's awareness. Extensions must also follow the tool-registration and permission-confirmation rules.

## 4. No default upload of local user data

Conversations, knowledge bases, memories, tasks, and configuration remain on the local machine by default. Network requests may be made only by model, retrieval, parsing, MCP, or update services that the user explicitly configures and enables.

## 5. No dependency on the co-located Web application

The desktop client runs as an independent local application. It does not depend on the Web frontend, private backend APIs, or Web authentication state from the same project.
