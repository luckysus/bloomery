import { useEffect, useState } from "react";
import { ArrowLeft, BookOpen, Eye, History, Link2, Network, Pencil, Plus, RotateCcw, Save, Trash2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  desktop,
  type PostgresWikiPage,
  type PostgresWikiRevision,
  type PostgresTag,
  type PostgresKnowledgeEdge,
  type PostgresDocument,
} from "../../bridge/desktop";

type Props = { onClose: () => void };

export default function PostgresWikiPanel({ onClose }: Props) {
  const [pages, setPages] = useState<PostgresWikiPage[]>([]);
  const [documents, setDocuments] = useState<PostgresDocument[]>([]);
  const [knowledgeBaseId, setKnowledgeBaseId] = useState("");
  const [selected, setSelected] = useState<PostgresWikiPage | null>(null);
  const [revisions, setRevisions] = useState<PostgresWikiRevision[]>([]);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [preview, setPreview] = useState(false);
  const [tags, setTags] = useState<PostgresTag[]>([]);
  const [tagText, setTagText] = useState("");
  const [sourceDocumentId, setSourceDocumentId] = useState<string | null>(null);
  const [edges, setEdges] = useState<PostgresKnowledgeEdge[]>([]);
  const [graphOpen, setGraphOpen] = useState(false);
  const [edgeSource, setEdgeSource] = useState("");
  const [edgeTarget, setEdgeTarget] = useState("");
  const [edgeRelation, setEdgeRelation] = useState("related_to");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async (id: string) => {
    if (!id) return;
    setBusy(true);
    setError(null);
    try {
      const [nextPages, nextEdges, nextDocuments] = await Promise.all([
        desktop.listPostgresWikiPages(id),
        desktop.listPostgresKnowledgeEdges(id),
        desktop.listPostgresDocuments(id),
      ]);
      setPages(nextPages);
      setEdges(nextEdges);
      setDocuments(nextDocuments);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void desktop.listPostgresKnowledgeBases().then((bases) => {
      const first = bases[0]?.id ?? "";
      setKnowledgeBaseId(first);
      if (first) void load(first);
    }).catch((cause) => setError(String(cause)));
  }, []);

  const selectPage = async (page: PostgresWikiPage) => {
    setSelected(page);
    setTitle(page.title);
    setBody(page.body_markdown);
    setSourceDocumentId(page.source_document_id);
    try {
      const [nextRevisions, nextTags] = await Promise.all([
        desktop.listPostgresWikiRevisions(page.id),
        desktop.listPostgresWikiPageTags(page.id),
      ]);
      setRevisions(nextRevisions);
      setTags(nextTags);
      setTagText(nextTags.map((tag) => tag.name).join(", "));
    } catch (cause) {
      setError(String(cause));
    }
  };

  const createPage = async () => {
    if (!knowledgeBaseId) return;
    setBusy(true);
    setError(null);
    try {
      const page = await desktop.createPostgresWikiPage({
        knowledge_base_id: knowledgeBaseId,
        slug: `page-${Date.now()}`,
        title: "新 Wiki 页面",
        body_markdown: "# 新 Wiki 页面\n\n",
        source_document_id: sourceDocumentId,
        tags: [],
      });
      await load(knowledgeBaseId);
      await selectPage(page);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const save = async () => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const names = tagText.split(",").map((value) => value.trim()).filter(Boolean);
      const page = await desktop.updatePostgresWikiPage(selected.id, title, body, names, sourceDocumentId);
      setSelected(page);
      setPages((current) => current.map((item) => item.id === page.id ? page : item));
      setRevisions(await desktop.listPostgresWikiRevisions(page.id));
      setTags(await desktop.listPostgresWikiPageTags(page.id));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const restore = async (revision: number) => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const page = await desktop.restorePostgresWikiRevision(selected.id, revision);
      setSelected(page);
      setTitle(page.title);
      setBody(page.body_markdown);
      setPages((current) => current.map((item) => item.id === page.id ? page : item));
      setRevisions(await desktop.listPostgresWikiRevisions(page.id));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const createEdge = async () => {
    if (!knowledgeBaseId || !edgeRelation.trim() || (!edgeSource && !edgeTarget)) return;
    setBusy(true);
    setError(null);
    try {
      await desktop.createPostgresKnowledgeEdge({
        knowledge_base_id: knowledgeBaseId,
        source_page_id: edgeSource || null,
        target_page_id: edgeTarget || null,
        relation: edgeRelation.trim(),
      });
      setEdges(await desktop.listPostgresKnowledgeEdges(knowledgeBaseId));
      setEdgeSource("");
      setEdgeTarget("");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const deleteEdge = async (edge: PostgresKnowledgeEdge) => {
    setBusy(true);
    setError(null);
    try {
      await desktop.deletePostgresKnowledgeEdge(edge.id);
      setEdges((current) => current.filter((item) => item.id !== edge.id));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const pageTitle = (id: string | null) => pages.find((page) => page.id === id)?.title ?? "未命名页面";

  return (
    <div className="fixed inset-0 z-[60] flex flex-col bg-[#f7f3ed] text-slate-950">
      <header className="flex h-16 shrink-0 items-center gap-3 border-b border-[#e5d8cc] bg-[#fffaf3] px-6">
        <BookOpen size={20} className="text-[#b85f43]" aria-hidden="true" />
        <h2 className="text-xl font-bold">PostgreSQL Wiki</h2>
        <button type="button" onClick={() => setGraphOpen((current) => !current)} className={`inline-flex items-center gap-1 rounded-lg px-3 py-2 text-sm font-semibold ${graphOpen ? "bg-[#f4dfd2] text-[#6f4a38]" : "hover:bg-[#f1e6dc]"}`} aria-pressed={graphOpen}>
          <Network size={16} aria-hidden="true" />知识图谱
        </button>
        <button type="button" onClick={onClose} className="ml-auto inline-flex items-center gap-1 rounded-lg px-3 py-2 hover:bg-[#f1e6dc]"><ArrowLeft size={16} />返回</button>
      </header>
      {graphOpen && (
        <section className="shrink-0 border-b border-[#e5d8cc] bg-[#fffaf3] px-6 py-4" aria-label="知识图谱">
          <div className="mb-3 flex items-center gap-2">
            <Link2 size={16} className="text-[#b85f43]" aria-hidden="true" />
            <h3 className="font-semibold">页面关系</h3>
            <span className="text-xs text-slate-500">{edges.length} 条边</span>
          </div>
          <div className="flex flex-wrap items-end gap-2">
            <label className="text-xs font-medium text-slate-600">起点
              <select value={edgeSource} onChange={(event) => setEdgeSource(event.target.value)} className="ml-1 h-9 rounded-lg border border-[#e4d6c8] bg-white px-2 text-sm">
                <option value="">不指定</option>
                {pages.map((page) => <option key={page.id} value={page.id}>{page.title}</option>)}
              </select>
            </label>
            <label className="text-xs font-medium text-slate-600">关系
              <input value={edgeRelation} onChange={(event) => setEdgeRelation(event.target.value)} className="ml-1 h-9 w-32 rounded-lg border border-[#e4d6c8] bg-white px-2 text-sm" />
            </label>
            <label className="text-xs font-medium text-slate-600">终点
              <select value={edgeTarget} onChange={(event) => setEdgeTarget(event.target.value)} className="ml-1 h-9 rounded-lg border border-[#e4d6c8] bg-white px-2 text-sm">
                <option value="">不指定</option>
                {pages.map((page) => <option key={page.id} value={page.id}>{page.title}</option>)}
              </select>
            </label>
            <button type="button" onClick={() => void createEdge()} disabled={busy || (!edgeSource && !edgeTarget)} className="inline-flex h-9 items-center gap-1 rounded-lg bg-[#c96f52] px-3 text-sm font-semibold text-white disabled:opacity-50"><Plus size={15} />添加关系</button>
          </div>
          <div className="mt-3 flex max-h-32 flex-wrap gap-2 overflow-y-auto">
            {edges.map((edge) => <div key={edge.id} className="inline-flex items-center gap-2 rounded-lg border border-[#eadccf] bg-white px-2.5 py-1.5 text-xs">
              <span>{pageTitle(edge.source_page_id)} <strong className="text-[#b85f43]">[{edge.relation}]</strong> {pageTitle(edge.target_page_id)}</span>
              <button type="button" onClick={() => void deleteEdge(edge)} disabled={busy} aria-label={`删除关系 ${edge.relation}`} title="删除关系" className="text-slate-400 hover:text-red-600"><Trash2 size={13} /></button>
            </div>)}
            {!edges.length && <span className="text-xs text-slate-500">暂无页面关系</span>}
          </div>
        </section>
      )}
      <div className="grid min-h-0 flex-1 grid-cols-[280px_minmax(0,1fr)_260px] gap-4 p-4">
        <aside className="min-h-0 overflow-y-auto rounded-xl border border-[#e4d6c8] bg-[#fffaf3] p-3">
          <label className="mb-2 block text-xs font-medium text-slate-600">新页面来源文档
            <select aria-label="新页面来源文档" value={sourceDocumentId ?? ""} onChange={(event) => setSourceDocumentId(event.target.value || null)} className="mt-1 h-9 w-full rounded-lg border border-[#e4d6c8] bg-white px-2 text-sm font-normal text-slate-900">
              <option value="">不关联来源文档</option>
              {documents.map((document) => <option key={document.id} value={document.id}>{document.display_name}</option>)}
            </select>
          </label>
          <button type="button" onClick={() => void createPage()} disabled={busy || !knowledgeBaseId} className="mb-3 inline-flex w-full items-center justify-center gap-2 rounded-lg bg-[#c96f52] px-3 py-2 text-sm font-semibold text-white disabled:opacity-50"><Plus size={16} />新建页面</button>
          {pages.map((page) => <button key={page.id} type="button" onClick={() => void selectPage(page)} className={`mb-1 block w-full rounded-lg px-3 py-2 text-left text-sm ${selected?.id === page.id ? "bg-[#f4dfd2] font-semibold" : "hover:bg-[#faf0e8]"}`}>{page.title}</button>)}
          {!pages.length && <p className="px-2 py-4 text-sm text-slate-500">暂无 Wiki 页面</p>}
        </aside>
        <main className="flex min-h-0 flex-col rounded-xl border border-[#e4d6c8] bg-[#fffaf3] p-4">
          {selected ? <>
            <input aria-label="Wiki 标题" value={title} onChange={(event) => setTitle(event.target.value)} className="mb-3 rounded-lg border border-[#e4d6c8] bg-white px-3 py-2 text-lg font-semibold" />
            <label className="mb-3 text-sm font-medium text-slate-600">来源文档
              <select aria-label="Wiki 来源文档" value={sourceDocumentId ?? ""} onChange={(event) => setSourceDocumentId(event.target.value || null)} className="ml-2 rounded-lg border border-[#e4d6c8] bg-white px-2 py-2 text-sm font-normal text-slate-900">
                <option value="">不关联来源文档</option>
                {documents.map((document) => <option key={document.id} value={document.id}>{document.display_name}</option>)}
              </select>
            </label>
            <input aria-label="Wiki 标签" value={tagText} onChange={(event) => setTagText(event.target.value)} placeholder="标签，用逗号分隔" className="mb-3 rounded-lg border border-[#e4d6c8] bg-white px-3 py-2 text-sm" />
            <div className="mb-2 flex items-center gap-1" role="tablist" aria-label="Wiki 正文视图">
              <button type="button" role="tab" aria-selected={!preview} onClick={() => setPreview(false)} className={`inline-flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-semibold ${!preview ? "bg-[#f4dfd2] text-[#6f4a38]" : "text-slate-600 hover:bg-[#faf0e8]"}`}><Pencil size={13} />编辑</button>
              <button type="button" role="tab" aria-selected={preview} onClick={() => setPreview(true)} className={`inline-flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-semibold ${preview ? "bg-[#f4dfd2] text-[#6f4a38]" : "text-slate-600 hover:bg-[#faf0e8]"}`}><Eye size={13} />预览</button>
            </div>
            {preview ? <div aria-label="Wiki Markdown 预览" className="min-h-0 flex-1 overflow-y-auto rounded-lg border border-[#e4d6c8] bg-white p-4 prose prose-slate max-w-none"><ReactMarkdown remarkPlugins={[remarkGfm]}>{body}</ReactMarkdown></div> : <textarea aria-label="Wiki Markdown 正文" value={body} onChange={(event) => setBody(event.target.value)} className="min-h-0 flex-1 resize-none rounded-lg border border-[#e4d6c8] bg-white p-3 font-mono text-sm" />}
            <button type="button" onClick={() => void save()} disabled={busy} className="mt-3 inline-flex w-fit items-center gap-2 rounded-lg bg-[#c96f52] px-4 py-2 text-sm font-semibold text-white disabled:opacity-50"><Save size={16} />保存 revision</button>
          </> : <div className="flex flex-1 items-center justify-center text-sm text-slate-500">选择或新建一个 Wiki 页面</div>}
          {error && <p role="alert" className="mt-3 text-sm text-red-700">{error}</p>}
        </main>
        <aside className="min-h-0 overflow-y-auto rounded-xl border border-[#e4d6c8] bg-[#fffaf3] p-3">
          <h3 className="mb-3 flex items-center gap-2 text-sm font-semibold"><History size={16} />版本历史</h3>
          {revisions.map((item) => <div key={item.id} className="mb-2 rounded-lg border border-[#eadccf] p-2 text-sm"><div className="flex items-center justify-between"><span>revision {item.revision}</span><button type="button" onClick={() => void restore(item.revision)} disabled={busy} title={`回滚到 revision ${item.revision}`} aria-label={`回滚到 revision ${item.revision}`}><RotateCcw size={14} /></button></div><time className="text-xs text-slate-500">{item.created_at}</time></div>)}
        </aside>
      </div>
    </div>
  );
}
