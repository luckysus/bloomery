import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    type Base = { id: string; name: string; description: string; library_type: string; tags: string[]; visibility: string; embedding_model: string; chunk_strategy: string; created_at: string; updated_at: string };
    const now = "2026-09-28T00:00:00Z";
    const bases: Base[] = [];
    const docs = [
      { id: "doc-local-1", knowledge_base_id: "base-local-1", display_name: "Q355B研究.pdf", source_kind: "pdf", file_size: 245760, active_version_id: "version-local-1", created_at: now, updated_at: now },
      { id: "doc-local-xls", knowledge_base_id: "base-local-1", display_name: "实验数据汇总.xlsx", source_kind: "xlsx", file_size: 873472, active_version_id: "version-local-1", created_at: now, updated_at: now },
      { id: "doc-local-docx", knowledge_base_id: "base-local-1", display_name: "热处理工艺.docx", source_kind: "docx", file_size: 65536, active_version_id: "version-local-1", created_at: now, updated_at: now },
    ];
    const jobs = [{ id: "job-local-1", knowledge_base_id: "base-local-1", source_document_id: null, state: "pending", attempts: 0, error_message: null, next_attempt_at: null, created_at: now, updated_at: now }];
    const wikiPages: Array<Record<string, unknown>> = [];
    const knowledgeEdges: Array<Record<string, unknown>> = [];
    const wikiPageTags = new Map<string, string[]>();
    const emptyHealth = { knowledge_base_count: 0, document_count: 0, active_document_count: 0, version_count: 0, chunk_count: 0, indexed_chunk_count: 0, active_task_count: 0 };
    const base = () => ({ id: "base-local-1", name: "高强钢研究", description: "单用户本地研究资料", library_type: "research", tags: ["钢铁"], visibility: "private", embedding_model: "", chunk_strategy: "semantic", created_at: now, updated_at: now });
    const callbacks = new Map<number, (payload: unknown) => void>();
    let callbackId = 1;
    window.__TAURI_INTERNALS__ = {
      transformCallback: (callback: (payload: unknown) => void, once = false) => {
        const id = callbackId++;
        callbacks.set(id, once ? (payload) => { callbacks.delete(id); callback(payload); } : callback);
        return id;
      },
      unregisterCallback: (id: number) => { callbacks.delete(id); },
      invoke: async (command: string, args?: Record<string, unknown>) => {
        const input = (args?.input ?? {}) as Record<string, unknown>;
        if (command === "plugin:event|listen") return 1;
        if (command === "plugin:dialog|open") return "C:\\fixtures\\Q690热处理.pdf";
        if (command === "plugin:event|unlisten" || command === "db_init" || command === "set_setting") return null;
        if (command === "get_setting") return null;
        if (command === "list_conversations" || command === "list_provider_profiles") return [];
        if (command === "list_postgres_knowledge_bases") return bases;
        if (command === "create_postgres_knowledge_base") {
          const created = { ...base(), name: String(input.name ?? "新知识库"), description: String(input.description ?? ""), library_type: String(input.library_type ?? "research"), tags: Array.isArray(input.tags) ? input.tags : [], visibility: String(input.visibility ?? "private"), embedding_model: String(input.embedding_model ?? ""), chunk_strategy: String(input.chunk_strategy ?? "semantic") };
          bases.splice(0, bases.length, created);
          return created;
        }
        if (command === "update_postgres_knowledge_base_settings") return bases[0];
        if (command === "list_postgres_documents") return bases.length ? docs : [];
        if (command === "delete_postgres_document") {
          const index = docs.findIndex((document) => document.id === args?.documentId);
          if (index >= 0) docs.splice(index, 1);
          return null;
        }
        if (command === "import_postgres_document") {
          const request = (args?.request ?? {}) as Record<string, unknown>;
          const document = { id: "doc-local-imported", knowledge_base_id: String(request.knowledge_base_id ?? "base-local-1"), display_name: "Q690热处理.pdf", source_kind: "pdf", file_size: 1024, active_version_id: null, created_at: now, updated_at: now };
          docs.push(document);
          jobs.push({ id: "job-local-imported", knowledge_base_id: document.knowledge_base_id, source_document_id: document.id, state: "queued", progress: 0, attempts: 0, error_message: null, next_attempt_at: null, created_at: now, updated_at: now });
          return { knowledge_base_id: document.knowledge_base_id, document_id: document.id, version_id: "version-local-imported", chunk_count: 0, asset_count: 0, duplicate_content: false };
        }
        if (command === "list_postgres_wiki_pages") return wikiPages;
        if (command === "list_postgres_knowledge_edges") return knowledgeEdges;
        if (command === "list_postgres_knowledge_chunks") return [];
        if (command === "list_postgres_tags") return Array.from(new Set(Array.from(wikiPageTags.values()).flat())).map((name) => ({ id: `tag-${name}`, knowledge_base_id: "base-local-1", name }));
        if (command === "create_postgres_wiki_page") { const created = { id: `wiki-local-${wikiPages.length + 1}`, knowledge_base_id: "base-local-1", slug: String(input.slug ?? "page"), title: String(input.title ?? "研究笔记"), body_markdown: String(input.body_markdown ?? ""), source_document_id: input.source_document_id ?? null, revision: 1, created_at: now, updated_at: now }; wikiPages.push(created); wikiPageTags.set(String(created.id), Array.isArray(input.tags) ? input.tags.map(String) : []); return created; }
        if (command === "list_postgres_wiki_revisions") return [{ id: "revision-local-1", page_id: "wiki-local-1", revision: 1, title: "研究笔记", body_markdown: "# 研究笔记", created_at: now }];
        if (command === "list_postgres_wiki_page_tags") return (wikiPageTags.get(String(args?.pageId)) || []).map((name) => ({ id: `tag-${name}`, knowledge_base_id: "base-local-1", name }));
        if (command === "update_postgres_wiki_page") { const current = wikiPages[0]; const updated = { ...current, title: String(args?.title ?? current?.title ?? ""), body_markdown: String(args?.bodyMarkdown ?? current?.body_markdown ?? ""), source_document_id: args?.sourceDocumentId ?? null, revision: Number(current?.revision ?? 1) + 1, updated_at: now }; wikiPages.splice(0, wikiPages.length, updated); wikiPageTags.set(String(updated.id), Array.isArray(args?.tags) ? args.tags.map(String) : []); return updated; }
        if (command === "create_postgres_knowledge_edge") { const created = { id: `edge-local-${knowledgeEdges.length + 1}`, knowledge_base_id: String(input.knowledge_base_id), source_page_id: input.source_page_id, target_page_id: input.target_page_id, relation: String(input.relation), metadata: input.metadata ?? {}, created_at: now }; knowledgeEdges.push(created); return created; }
        if (command === "delete_postgres_knowledge_edge") { const index = knowledgeEdges.findIndex((edge) => edge.id === args?.edgeId); if (index >= 0) knowledgeEdges.splice(index, 1); return null; }
        if (command === "get_postgres_document_preview" && args?.documentId === "doc-local-xls") return { processed: true, document_name: "实验数据汇总.xlsx", mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", updated_at: now, source: "xlsx", content: "Heat ID | Yield Strength", blocks: [], raw_sheets: [{ name: "拉伸实验", html: "<table><tbody><tr><td>炉次</td><td>屈服强度</td></tr><tr><td>H-01</td><td>355 MPa</td></tr></tbody></table>", truncated: false }, { name: "化学成分", html: "<table><tbody><tr><td>元素</td><td>含量</td></tr><tr><td>C</td><td>0.18%</td></tr></tbody></table>", truncated: false }] };
        if (command === "get_postgres_document_preview" && args?.documentId === "doc-local-docx") return { processed: true, document_name: "热处理工艺.docx", mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document", updated_at: now, source: "docx", content: "正火工艺", blocks: [], structured_blocks: [{ kind: "heading", level: 1, text: "热处理工艺" }, { kind: "paragraph", text: "加热至 900℃ 后空冷。" }, { kind: "list", ordered: false, items: ["升温", "保温", "空冷"] }] };
        if (command === "update_postgres_document_metadata") return null;
        if (command === "get_postgres_document_preview") return { processed: true, document_name: "Q355B研究.pdf", mime_type: "application/pdf", updated_at: now, source: "pdf", content: "Q355B 的屈服强度与热处理条件。", pages: [{ page: 12, content: "Q355B 的屈服强度与热处理条件。" }, { page: 13, content: "热处理后的组织观察结果。" }], blocks: [{ ordinal: 0, content: "Q355B 的屈服强度与热处理条件。", source_location: { page: 12 } }] };
        if (command === "list_postgres_document_versions") return [{ id: "version-local-1", document_id: "doc-local-1", content_sha256: "hash", mime_type: "application/pdf", parser: "pdf", parser_version: "1", chunk_policy_version: "1", embedding_profile_id: "", embedding_model_id: "", embedding_dimension: 0, expected_asset_count: 0, expected_chunk_count: 1, manifest_sealed: true, created_at: now, activated_at: now }];
        if (command === "list_postgres_ingestion_jobs") return jobs;
        if (command === "cancel_postgres_ingestion_job") { jobs[0].state = "cancelled"; return jobs[0]; }
        if (command === "get_postgres_knowledge_health") return { ...emptyHealth, knowledge_base_count: bases.length };
        if (command === "search_postgres_knowledge") return [{ knowledge_base_id: "base-local-1", chunk_id: "chunk-local-1", version_id: "version-local-1", document_id: "doc-local-1", document_name: "Q355B研究.pdf", text: "Q355B 淬火后形成马氏体组织。", source_location: { kind: "pdf_page", page: 12 }, rank: 0.93 }];
        if (command === "query_local_knowledge") return { id: "audit-local-1", workspace_id: "local", query: String(input.query ?? ""), configuration: { knowledge_base_ids: ["base-local-1"], lexical_limit: 60, dense_limit: 60, candidate_limit: 20, rrf_k: 60, embedding_provider_profile_id: "postgresql", embedding_model_id: "tsvector", rerank_provider_profile_id: null, rerank_model_id: null, rerank_degradation: null }, evidence: [{ citation_number: 1, chunk: { knowledge_base_id: "base-local-1", document_id: "doc-local-1", version_id: "version-local-1", chunk_id: "chunk-local-1", source_name: "Q355B研究.pdf", source_location: { kind: "pdf_page", page: 12 }, text: "Q355B 淬火后形成马氏体组织。", lexical_rank: 1, dense_rank: null, rrf_score: 0.93, rerank_score: null }, assets: [] }], created_at: now };
        if (command === "resolve_postgres_citation") return { audit_id: String(args?.auditId), citation_number: Number(args?.citationNumber), source_state: "active", hit: { knowledge_base_id: "base-local-1", chunk_id: "chunk-local-1", version_id: "version-local-1", document_id: "doc-local-1", document_name: "Q355B研究.pdf", text: "Q355B 淬火后形成马氏体组织。", source_location: { kind: "pdf_page", page: 12 }, rank: 0.93 } };
        if (command === "preview_delete_postgres_knowledge_base") return { knowledge_base_id: String(args?.id ?? ""), name: bases[0]?.name ?? "", document_count: 0, version_count: 0, chunk_count: 0, asset_count: 0, active_task_count: 0 };
        if (command === "delete_postgres_knowledge_base") { bases.splice(0, bases.length); return null; }
        throw new Error(`Unexpected command: ${command}`);
      },
    };
  });
});

test("知识中心可以创建知识库并切换核心工作区", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /知识中心/ }).first().click();
  await page.getByRole("button", { name: "新建知识库" }).click();
  await page.getByLabel("知识库名称").fill("高强钢研究");
  await page.getByLabel("描述").fill("单用户本地研究资料");
  await page.getByRole("button", { name: "创建知识库" }).click();
  await expect(page.getByRole("button", { name: "高强钢研究" })).toBeVisible();
  await page.getByText("Q355B研究.pdf").click();
  await expect(page.getByText("Q355B 的屈服强度与热处理条件。").first()).toBeVisible();
  await expect(page.getByRole("heading", { name: "第 12 页" })).toBeVisible();
  await page.getByLabel("选择 PDF 页码").selectOption("13");
  await expect(page.getByText("热处理后的组织观察结果。")).toBeVisible();
  await page.getByLabel("选择 PDF 页码").selectOption("12");
  await expect(page.getByRole("button", { name: "复制" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "文档元数据 JSON" })).toBeVisible();
  await page.getByRole("button", { name: "保存元数据" }).click();
  await page.getByRole("button", { name: "关闭" }).last().click();
  await page.getByText("实验数据汇总.xlsx").click();
  await expect(page.getByRole("heading", { name: "拉伸实验" })).toBeVisible();
  await expect(page.getByText("355 MPa")).toBeVisible();
  await page.getByLabel("选择工作表").selectOption("化学成分");
  await expect(page.getByText("0.18%")).toBeVisible();
  await page.getByRole("button", { name: "关闭" }).last().click();
  await page.getByText("热处理工艺.docx").click();
  await expect(page.getByRole("heading", { name: "热处理工艺" })).toBeVisible();
  await expect(page.getByText("加热至 900℃ 后空冷。")).toBeVisible();
  await expect(page.getByText("保温")).toBeVisible();
  await page.getByRole("button", { name: "关闭" }).last().click();
  const librarySearch = page.getByPlaceholder("搜索知识库、文档、标签...");
  await librarySearch.fill("高强钢");
  await page.waitForTimeout(240);
  await expect(page.getByRole("button", { name: "高强钢研究" })).toBeVisible();
  await librarySearch.fill("不存在的知识");
  await page.waitForTimeout(240);
  await expect(page.getByRole("button", { name: "高强钢研究" })).toHaveCount(0);
  await librarySearch.fill("");
  await expect(page.getByRole("heading", { name: "处理任务" })).toBeVisible();
  await page.getByRole("button", { name: "取消任务" }).click();
  await expect(page.getByText("已取消处理任务")).toBeVisible();

  await page.getByRole("button", { name: "知识片段" }).click();
  await expect(page.getByRole("heading", { name: "知识片段", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "知识检索" }).click();
  await expect(page.getByPlaceholder("搜索材料、工艺、性能和文档内容...")).toBeVisible();
  await page.getByPlaceholder("搜索材料、工艺、性能和文档内容...").fill("Q355B");
  await page.getByRole("button", { name: "开始检索" }).click();
  await expect(page.getByText("Q355B 淬火后形成马氏体组织。")).toBeVisible();
  await expect(page.getByRole("button", { name: "打开原文" })).toBeVisible();
  await page.getByRole("button", { name: "打开原文" }).click();
  await expect(page.getByRole("heading", { name: "第 12 页" })).toBeVisible();
  await page.getByRole("button", { name: "关闭" }).last().click();
  await expect(page.getByRole("button", { name: "复制片段" })).toBeVisible();
  await expect(page.getByRole("button", { name: "加入对话" })).toBeVisible();
  await page.getByRole("button", { name: "收藏" }).click();
  await expect(page.getByRole("button", { name: "已收藏" })).toBeVisible();
  await page.getByRole("button", { name: "引用" }).click();
  await page.getByLabel("材料筛选").fill("Q355B");
  await page.getByRole("button", { name: "Wiki" }).click();
  await expect(page.getByRole("heading", { name: "Wiki 页面", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "新建" }).click();
  await page.getByLabel("页面标题").fill("Q355B 热处理笔记");
  await page.getByRole("button", { name: "创建", exact: true }).click();
  await page.getByLabel("Wiki 来源文档").selectOption("doc-local-1");
  await page.getByLabel("Wiki 标签").fill("淬火");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expect(page.getByRole("button", { name: "淬火 ×" })).toBeVisible();
  await page.getByRole("button", { name: "保存" }).click();
  await expect(page.getByText("Wiki 页面已保存")).toBeVisible();
  await page.getByRole("button", { name: "知识图谱" }).click();
  await expect(page.getByRole("heading", { name: "知识关系图谱" })).toBeVisible();
  await page.locator(".suna-knowledge-center").getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByRole("heading", { name: "知识库设置" })).toBeVisible();
  await page.getByRole("button", { name: "公共知识库" }).click();
  await expect(page.getByText("单用户客户端不提供共享知识库。")).toBeVisible();
  await expect(page.getByRole("button", { name: "导入文档" })).toBeDisabled();
  await page.getByRole("button", { name: "我的知识库" }).click();
  await expect(page.getByRole("button", { name: "高强钢研究" })).toBeVisible();
  const documentRow = page.getByRole("row", { name: /Q355B研究\.pdf/ });
  await documentRow.getByRole("button", { name: "删除" }).click();
  await expect(page.getByText(/确认删除“Q355B研究\.pdf”/)).toBeVisible();
  await page.getByRole("button", { name: "确认删除" }).click();
  await expect(page.getByRole("row", { name: /Q355B研究\.pdf/ })).toHaveCount(0);
});

test("知识中心提交文档导入并显示处理状态", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /知识中心/ }).first().click();
  await page.getByRole("button", { name: "新建知识库" }).click();
  await page.getByLabel("知识库名称").fill("导入流程测试库");
  await page.getByRole("button", { name: "创建知识库" }).click();
  await page.getByRole("button", { name: "导入文档" }).first().click();
  await expect(page.getByText("已提交 1 个文档处理任务")).toBeVisible();
  await expect(page.getByTitle("C:\\fixtures\\Q690热处理.pdf")).toBeVisible();
  await expect(page.getByRole("cell", { name: "PDF Q690热处理.pdf" })).toBeVisible();
  await expect(page.getByText("处理中").first()).toBeVisible();
  await expect(page.getByRole("heading", { name: "处理任务" })).toBeVisible();
});

test("知识库管理需要删除二次确认", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /知识中心/ }).first().click();
  await page.getByRole("button", { name: "新建知识库" }).click();
  await page.getByLabel("知识库名称").fill("待删除知识库");
  await page.getByRole("button", { name: "创建知识库" }).click();
  await page.getByRole("button", { name: "知识库管理" }).click();
  await page.getByRole("button", { name: "删除" }).click();
  await expect(page.getByText(/确认删除“待删除知识库”/)).toBeVisible();
  await page.getByRole("button", { name: "取消" }).click();
  await expect(page.getByText("待删除知识库")).toBeVisible();
  await page.getByRole("button", { name: "删除" }).click();
  await page.getByRole("button", { name: "确认删除" }).click();
  await expect(page.getByText("待删除知识库")).toHaveCount(0);
});

test("知识图谱展示节点关系并支持删除手工关系", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /知识中心/ }).first().click();
  await page.getByRole("button", { name: "新建知识库" }).click();
  await page.getByLabel("知识库名称").fill("图谱测试库");
  await page.getByRole("button", { name: "创建知识库" }).click();
  await page.getByRole("button", { name: "Wiki" }).click();
  await page.getByRole("button", { name: "新建" }).click();
  await page.getByLabel("页面标题").fill("材料性能");
  await page.getByRole("button", { name: "创建", exact: true }).click();
  await page.getByRole("button", { name: "新建" }).click();
  await page.getByLabel("页面标题").fill("热处理工艺");
  await page.getByRole("button", { name: "创建", exact: true }).click();
  await page.getByRole("button", { name: "知识图谱" }).click();
  await expect(page.getByText("2 个 Wiki 节点 · 0 条关系边")).toBeVisible();
  await page.getByLabel("关系源页面").selectOption({ label: "材料性能" });
  await page.getByLabel("关系类型").fill("影响");
  await page.getByLabel("关系目标页面").selectOption({ label: "热处理工艺" });
  await page.getByRole("button", { name: "添加关系" }).click();
  await expect(page.getByText("知识边已创建")).toBeVisible();
  await expect(page.getByText("2 个 Wiki 节点 · 1 条关系边")).toBeVisible();
  await page.getByRole("button", { name: "材料性能", exact: true }).click();
  await expect(page.getByText("节点详情")).toBeVisible();
  await page.locator(".suna-graph-edge-labels button").click();
  await expect(page.getByText("关系详情")).toBeVisible();
  await expect(page.getByRole("definition").filter({ hasText: "影响" })).toBeVisible();
  await page.getByRole("button", { name: "删除这条关系" }).click();
  await expect(page.getByText("知识边已软删除")).toBeVisible();
  await expect(page.getByText("2 个 Wiki 节点 · 0 条关系边")).toBeVisible();
});
