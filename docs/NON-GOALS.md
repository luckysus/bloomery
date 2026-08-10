# Non-goals / 非目标

Bloomery deliberately does **not** do the following. 以下事项 Bloomery 明确不做。

1. **No hosted services / 无托管服务**
   No Bloomery account, login, cloud task queue, telemetry, or author-maintained backend. All persistence is local SQLite under the OS application-data directory.
   没有 Bloomery 账号、登录、云任务队列、遥测或作者维护的后端。所有持久化都在操作系统应用数据目录下的本地 SQLite。

2. **No redistribution of restricted text / 不分发受限文本**
   Standards (GB/T, ISO, etc.) are referenced by identifier only; the terminology and source ledger never copy restricted standards text, and evaluation cases are authored or derived from public formula definitions.
   标准（GB/T、ISO 等）仅按标识符引用；术语表与来源台账从不复制受限标准文本，评测用例为自撰或源自公开公式定义。

3. **No silent dangerous execution / 无静默危险执行**
   Write, shell, and other dangerous tools always require an explicit permission decision; domain packages expose only allowlisted tools, and MCP tools default to confirmation-required.
   写入、Shell 等危险工具必须显式裁决；领域包仅暴露白名单工具，MCP 工具默认需确认。

4. **No inferred provider capabilities / 不推断 Provider 能力**
   Capabilities (tool calls, embeddings, rerank, parsing) are explicit per profile; XGBoost and similar optional compute capabilities are advertised only when explicitly installed.
   能力（工具调用、向量、重排、解析）按配置显式声明；XGBoost 等可选计算能力仅在显式安装时 advertised。

5. **No threshold weakening / 不削弱门槛**
   Evaluation thresholds, release gates, and signature checks are never lowered to hide a regression; failures are recorded verbatim.
   评测门槛、发布门禁与签名校验绝不为掩盖回归而降低；失败逐条记录。

6. **No cross-product coupling / 不跨产品耦合**
   The desktop client does not depend on the co-located Web application, its private backend APIs, or its authentication state.
   桌面客户端不依赖同置的 Web 应用、其私有后端 API 或其认证状态。
