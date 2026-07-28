import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Archive, ArchiveRestore, Brain, RefreshCw, Search } from "lucide-react";
import {
  archiveMemory,
  listArchivedMemories,
  listMemories,
  restoreMemory,
  saveMemory,
  searchMemories,
  suggestMemories,
  type DesktopMemory,
  type DesktopMemorySuggestion,
} from "./services/memories";

const emptyMemory: DesktopMemory = {
  scope: "global",
  type: "user",
  title: "",
  description: "",
  body: "",
  tags_json: "[]",
  enabled: true,
};

export default function DesktopMemoryPage() {
  const [memories, setMemories] = useState<DesktopMemory[]>([]);
  const [suggestions, setSuggestions] = useState<DesktopMemorySuggestion[]>([]);
  const [draft, setDraft] = useState<DesktopMemory>(emptyMemory);
  const [query, setQuery] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  const loadMemories = async () => {
    if (showArchived) {
      setMemories(await listArchivedMemories());
      return;
    }
    setMemories(query.trim() ? await searchMemories(query.trim()) : await listMemories());
  };

  const loadSuggestions = async () => {
    setSuggestions(await suggestMemories());
  };

  useEffect(() => {
    void loadMemories().catch((err) => setError(String(err)));
    if (!showArchived) void loadSuggestions().catch(() => setSuggestions([]));
  }, [showArchived]);

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setError("");
    setNotice("");
    try {
      await saveMemory(draft);
      setDraft(emptyMemory);
      setNotice("记忆已保存。");
      await loadMemories();
      await loadSuggestions().catch(() => setSuggestions([]));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleAcceptSuggestion = async (suggestion: DesktopMemorySuggestion) => {
    setError("");
    setNotice("");
    try {
      await saveMemory({
        scope: suggestion.scope,
        type: suggestion.type,
        title: suggestion.title,
        description: suggestion.description,
        body: suggestion.body,
        tags_json: suggestion.tags_json,
        enabled: true,
      });
      setSuggestions((items) => items.filter((item) => item.id !== suggestion.id));
      setNotice("候选记忆已采用。");
      await loadMemories();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const setTags = (value: string) => {
    const tags = value.split(",").map((item) => item.trim()).filter(Boolean);
    setDraft((item) => ({ ...item, tags_json: JSON.stringify(tags) }));
  };

  const tagsText = (() => {
    try {
      return JSON.parse(draft.tags_json || "[]").join(", ");
    } catch {
      return "";
    }
  })();

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[1fr_360px] bg-slate-950">
      <section className="min-h-0 overflow-auto p-5">
        {!showArchived && suggestions.length > 0 && (
          <div className="mb-4 rounded-md border border-cyan-900 bg-cyan-950/20 p-3">
            <div className="mb-2 flex items-center justify-between">
              <h2 className="inline-flex items-center gap-2 text-sm font-semibold text-cyan-100">
                <Brain className="h-4 w-4" />
                候选记忆
              </h2>
              <button
                type="button"
                onClick={() => void loadSuggestions().catch((err) => setError(String(err)))}
                className="inline-flex items-center gap-1 rounded-md border border-cyan-800 px-2 py-1 text-xs text-cyan-100 hover:bg-cyan-900/40"
              >
                <RefreshCw className="h-3.5 w-3.5" />
                刷新
              </button>
            </div>
            <div className="space-y-2">
              {suggestions.map((suggestion) => (
                <article key={suggestion.id} className="rounded-md border border-cyan-900/70 bg-slate-950 p-3">
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <h3 className="text-sm font-medium text-slate-100">{suggestion.title}</h3>
                      <p className="mt-1 text-xs text-slate-500">
                        {suggestion.scope} / {suggestion.type} / {suggestion.reason}
                      </p>
                    </div>
                    <button
                      type="button"
                      onClick={() => void handleAcceptSuggestion(suggestion)}
                      className="rounded-md bg-cyan-500 px-3 py-1 text-xs font-medium text-slate-950 hover:bg-cyan-400"
                    >
                      采用
                    </button>
                  </div>
                  <p className="mt-2 line-clamp-2 text-sm text-slate-300">{suggestion.description}</p>
                </article>
              ))}
            </div>
          </div>
        )}

        <div className="mb-4 flex items-center gap-2">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            disabled={showArchived}
            placeholder={showArchived ? "归档记忆不参与搜索" : "搜索本地记忆"}
            className="w-full rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none focus:border-cyan-500 disabled:opacity-60"
          />
          <button
            type="button"
            onClick={() => void loadMemories().catch((err) => setError(String(err)))}
            className="inline-flex items-center gap-2 rounded-md bg-slate-800 px-4 py-2 text-sm text-slate-100 hover:bg-slate-700"
          >
            <Search className="h-4 w-4" />
            搜索
          </button>
          <button
            type="button"
            onClick={() => setShowArchived((value) => !value)}
            className="inline-flex items-center gap-2 rounded-md border border-slate-700 px-4 py-2 text-sm text-slate-200 hover:bg-slate-900"
          >
            {showArchived ? <ArchiveRestore className="h-4 w-4" /> : <Archive className="h-4 w-4" />}
            {showArchived ? "返回记忆" : "归档箱"}
          </button>
        </div>

        {notice && <p className="mb-3 rounded-md border border-cyan-900 bg-cyan-950/30 p-3 text-sm text-cyan-100">{notice}</p>}
        {error && <p className="mb-3 rounded-md border border-red-900 bg-red-950/40 p-3 text-sm text-red-200">{error}</p>}

        <div className="space-y-3">
          {memories.map((memory) => (
            <article key={memory.id} className="rounded-md border border-slate-800 bg-slate-900 p-4">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <h3 className="font-semibold text-slate-100">{memory.title}</h3>
                  <p className="mt-1 text-xs text-slate-500">
                    {memory.scope} / {memory.type} / {memory.enabled ? "启用" : "停用"}
                    {memory.archived_at ? ` / 归档于 ${new Date(memory.archived_at).toLocaleString()}` : ""}
                  </p>
                </div>
                <div className="flex gap-2">
                  {!showArchived && (
                    <button
                      type="button"
                      onClick={() => setDraft(memory)}
                      className="rounded-md border border-slate-700 px-3 py-1 text-xs text-slate-200 hover:bg-slate-800"
                    >
                      编辑
                    </button>
                  )}
                  {showArchived ? (
                    <button
                      type="button"
                      onClick={() =>
                        memory.id && void restoreMemory(memory.id).then(loadMemories).then(() => setNotice("记忆已恢复。")).catch((err) => setError(String(err)))
                      }
                      className="inline-flex items-center gap-1 rounded-md border border-cyan-800 px-3 py-1 text-xs text-cyan-100 hover:bg-cyan-950"
                    >
                      <ArchiveRestore className="h-3.5 w-3.5" />
                      恢复
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() =>
                        memory.id && void archiveMemory(memory.id).then(loadMemories).then(() => setNotice("记忆已归档。")).catch((err) => setError(String(err)))
                      }
                      className="inline-flex items-center gap-1 rounded-md border border-red-900 px-3 py-1 text-xs text-red-200 hover:bg-red-950"
                    >
                      <Archive className="h-3.5 w-3.5" />
                      归档
                    </button>
                  )}
                </div>
              </div>
              <p className="mt-3 text-sm text-slate-300">{memory.description}</p>
              <p className="mt-2 line-clamp-3 whitespace-pre-wrap text-sm text-slate-400">{memory.body}</p>
            </article>
          ))}
          {memories.length === 0 && (
            <p className="rounded-md border border-slate-800 p-5 text-sm text-slate-500">
              {showArchived ? "暂无归档记忆。" : "暂无记忆。"}
            </p>
          )}
        </div>
      </section>

      <aside className="border-l border-slate-800 bg-slate-950 p-5">
        <h2 className="mb-4 text-lg font-semibold text-slate-100">{draft.id ? "编辑记忆" : "新增记忆"}</h2>
        <form onSubmit={handleSubmit} className="space-y-3">
          <input
            value={draft.title}
            onChange={(event) => setDraft((item) => ({ ...item, title: event.target.value }))}
            placeholder="标题"
            className="w-full rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none focus:border-cyan-500"
          />
          <textarea
            value={draft.description}
            onChange={(event) => setDraft((item) => ({ ...item, description: event.target.value }))}
            placeholder="说明"
            rows={2}
            className="w-full resize-none rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none focus:border-cyan-500"
          />
          <textarea
            value={draft.body}
            onChange={(event) => setDraft((item) => ({ ...item, body: event.target.value }))}
            placeholder="记忆正文"
            rows={8}
            className="w-full resize-none rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none focus:border-cyan-500"
          />
          <input
            value={tagsText}
            onChange={(event) => setTags(event.target.value)}
            placeholder="标签，逗号分隔"
            className="w-full rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none focus:border-cyan-500"
          />
          <div className="grid grid-cols-2 gap-2">
            <select
              value={draft.scope}
              onChange={(event) => setDraft((item) => ({ ...item, scope: event.target.value as DesktopMemory["scope"] }))}
              className="rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100"
            >
              <option value="global">global</option>
              <option value="project">project</option>
              <option value="domain">domain</option>
            </select>
            <select
              value={draft.type}
              onChange={(event) => setDraft((item) => ({ ...item, type: event.target.value as DesktopMemory["type"] }))}
              className="rounded-md border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100"
            >
              <option value="user">user</option>
              <option value="project">project</option>
              <option value="domain">domain</option>
              <option value="feedback">feedback</option>
              <option value="reference">reference</option>
            </select>
          </div>
          <label className="flex items-center gap-2 text-sm text-slate-300">
            <input
              type="checkbox"
              checked={draft.enabled}
              onChange={(event) => setDraft((item) => ({ ...item, enabled: event.target.checked }))}
            />
            启用
          </label>
          <div className="flex gap-2">
            <button type="submit" className="rounded-md bg-cyan-500 px-4 py-2 text-sm font-semibold text-slate-950 hover:bg-cyan-400">
              保存
            </button>
            <button
              type="button"
              onClick={() => setDraft(emptyMemory)}
              className="rounded-md border border-slate-700 px-4 py-2 text-sm text-slate-200 hover:bg-slate-900"
            >
              清空
            </button>
          </div>
        </form>
      </aside>
    </div>
  );
}
