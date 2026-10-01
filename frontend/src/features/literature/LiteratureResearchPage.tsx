import { useEffect, useMemo, useState, type FormEvent } from "react";
import { BookOpen, Bookmark, BookmarkCheck, Check, Clipboard, FilePlus2, Library, Loader2, Quote, Search, Sparkles, Table2, X, ChevronDown, Plus } from "lucide-react";
import { desktop, type LiteratureSearchHit, type LiteratureSectionResponse } from "../../bridge/desktop";
import "./literature.css";

type Status = "idle" | "loading" | "error";
const settingKey = "suna.literature.favorites";

function titleOf(hit: LiteratureSearchHit) {
  return hit.title_path?.trim() || hit.source_name || "未命名文献";
}
function excerptOf(hit: LiteratureSearchHit) {
  return hit.snippet?.trim() || hit.text?.trim() || "暂无摘要片段";
}
function sourceLocation(hit: LiteratureSearchHit) {
  const location = hit.source_location;
  if (!location) return "来源位置未记录";
  const page = location.page ?? location.page_number;
  const heading = location.heading ?? location.title_path;
  return [page ? `第 ${page} 页` : "", typeof heading === "string" ? heading : ""].filter(Boolean).join(" · ") || "来源位置已记录";
}

export default function LiteratureResearchPage() {
  const [query, setQuery] = useState("");
  const [submittedQuery, setSubmittedQuery] = useState("");
  const [results, setResults] = useState<LiteratureSearchHit[]>([]);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [compareIds, setCompareIds] = useState<string[]>([]);
  const [favorites, setFavorites] = useState<string[]>([]);
  const [reading, setReading] = useState<LiteratureSectionResponse | null>(null);
  const [readingStatus, setReadingStatus] = useState<Status>("idle");
  const [summary, setSummary] = useState("");
  const [summaryStatus, setSummaryStatus] = useState<Status>("idle");
  const [review, setReview] = useState("");
  const [reviewStatus, setReviewStatus] = useState<Status>("idle");
  const [compareSummary, setCompareSummary] = useState("");
  const [compareStatus, setCompareStatus] = useState<Status>("idle");
  const [notice, setNotice] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [sourceFilter, setSourceFilter] = useState("all");
  const [activeTab, setActiveTab] = useState<"search" | "reading" | "library" | "review">("search");
  const [yearFilter, setYearFilter] = useState("all");
  const [typeFilter, setTypeFilter] = useState("all");
  const [sortOrder, setSortOrder] = useState("relevance");

  useEffect(() => { desktop.getSetting(settingKey).then((value) => { if (value) { try { setFavorites(JSON.parse(value)); } catch { /* ignore malformed local preference */ } } }).catch(() => undefined); }, []);
  const selected = results.find((item) => item.document_id === selectedId) ?? results[0];
  const compareItems = results.filter((item) => compareIds.includes(item.document_id ?? item.source_name));
  const sources = useMemo(() => Array.from(new Set(results.map((item) => item.source_name).filter(Boolean))), [results]);
  const years = useMemo(() => Array.from(new Set(results.map((item) => {
    const value = sourceLocation(item).match(/20\d{2}/)?.[0];
    return value;
  }).filter(Boolean) as string[])).sort((a, b) => Number(b) - Number(a)), [results]);
  const types = useMemo(() => Array.from(new Set(results.map((item) => {
    const source = item.source_name || "";
    return source.includes("学位") ? "学位论文" : source.includes("会议") ? "会议论文" : "中文期刊";
  }))), [results]);
  const visibleResults = useMemo(() => {
    const filtered = results.filter((item) => {
      const sourceYear = sourceLocation(item).match(/20\d{2}/)?.[0];
      const sourceType = (item.source_name || "").includes("学位") ? "学位论文" : (item.source_name || "").includes("会议") ? "会议论文" : "中文期刊";
      return (sourceFilter === "all" || item.source_name === sourceFilter) && (yearFilter === "all" || sourceYear === yearFilter) && (typeFilter === "all" || sourceType === typeFilter);
    });
    return [...filtered].sort((a, b) => sortOrder === "latest" ? Number((sourceLocation(b).match(/20\d{2}/)?.[0] || 0)) - Number((sourceLocation(a).match(/20\d{2}/)?.[0] || 0)) : (b.rrf_score ?? b.score ?? 0) - (a.rrf_score ?? a.score ?? 0));
  }, [results, sourceFilter, yearFilter, typeFilter, sortOrder]);
  const favoriteSet = useMemo(() => new Set(favorites), [favorites]);

  const saveFavorites = (next: string[]) => { setFavorites(next); void desktop.setSetting(settingKey, JSON.stringify(next)); };
  const toggleFavorite = (hit: LiteratureSearchHit) => {
    const id = hit.document_id ?? hit.source_name;
    saveFavorites(favoriteSet.has(id) ? favorites.filter((item) => item !== id) : [...favorites, id]);
  };
  const search = async (event?: FormEvent) => {
    event?.preventDefault();
    const value = query.trim();
    if (!value) return;
    setSubmittedQuery(value); setStatus("loading"); setError(""); setReading(null); setSummary(""); setSourceFilter("all"); setYearFilter("all"); setTypeFilter("all");
    try {
      const response = await desktop.searchLiterature({ query: value, limit: 20 });
      const next = response.literature_results ?? response.results ?? [];
      setResults(next); setSelectedId(next[0]?.document_id ?? next[0]?.source_name ?? null); setStatus("idle");
    } catch (reason) { setStatus("error"); setError(reason instanceof Error ? reason.message : "文献检索失败，请检查知识库连接后重试"); }
  };
  const read = async (target = selected, part?: number) => {
    if (!target) return;
    setReadingStatus("loading"); setReading(null);
    try { setReading(await desktop.readLiteratureSection({ query: submittedQuery || titleOf(target), documentHint: target.source_name, chapterNumber: part })); setReadingStatus("idle"); }
    catch (reason) { setReadingStatus("error"); setError(reason instanceof Error ? reason.message : "无法读取文献内容"); }
  };
  const generateReview = async () => {
    if (compareItems.length < 2) { setNotice("生成综述需要先选择至少 2 篇文献"); return; }
    setReviewStatus("loading"); setReview("");
    try {
      const evidence = compareItems.map((item, index) => `文献${index + 1}：${titleOf(item)}\n来源：${item.source_name}\n证据：${excerptOf(item)}`).join("\n\n");
      const response = await desktop.desktopAgentChat({ message: `请基于以下 ${compareItems.length} 篇钢铁材料文献生成结构化综述。输出研究背景、研究现状、常用材料与工艺、性能结论、争议与研究空白、未来方向，并在段落末使用“文献1”等标记对应来源。不要展示思考过程。\n\n${evidence}`, smartSearchEnabled: false });
      setReview(response.answer); setReviewStatus("idle");
    } catch (reason) { setReviewStatus("error"); setError(reason instanceof Error ? reason.message : "综述生成失败"); }
  };
  const generateComparison = async () => {
    if (compareItems.length < 2) { setNotice("对比分析需要先选择至少 2 篇文献"); return; }
    setCompareStatus("loading"); setCompareSummary("");
    try {
      const evidence = compareItems.map((item, index) => `文献${index + 1}：${titleOf(item)}\n${excerptOf(item)}`).join("\n\n");
      const response = await desktop.desktopAgentChat({ message: `请对比以下文献的研究材料、成分、工艺、组织、性能和结论，输出一张清晰的差异分析，并指出证据不足的字段。使用文献1、文献2标记来源。\n\n${evidence}`, smartSearchEnabled: false });
      setCompareSummary(response.answer); setCompareStatus("idle");
    } catch (reason) { setCompareStatus("error"); setError(reason instanceof Error ? reason.message : "文献对比失败"); }
  };
  const copyCitation = async (target: LiteratureSearchHit) => {
    const citation = `${titleOf(target)}. ${target.source_name}. ${sourceLocation(target)}`;
    try { await navigator.clipboard.writeText(citation); setNotice("引用已复制"); } catch { setNotice(citation); }
  };
  const summarize = async (target = selected) => {
    if (!target) return;
    setSummaryStatus("loading"); setSummary("");
    try {
      const response = await desktop.desktopAgentChat({ message: `请基于文献“${titleOf(target)}”的证据片段，输出研究目的、材料与工艺、组织与性能、结论、局限和可复现实验线索。只给出研究摘要，不展示思考过程。\n\n${excerptOf(target)}`, smartSearchEnabled: false });
      setSummary(response.answer); setSummaryStatus("idle");
    } catch (reason) { setSummaryStatus("error"); setError(reason instanceof Error ? reason.message : "总结生成失败"); }
  };
  const importDocument = async () => {
    const path = await desktop.openFileDialog({ multiple: false, directory: false, filters: [{ name: "文献", extensions: ["pdf", "md", "txt", "docx"] }] });
    if (typeof path !== "string") return;
    try { await desktop.processLiterature({ file_path: path }); setNotice("文献已提交知识库导入任务"); } catch (reason) { setError(reason instanceof Error ? reason.message : "文献入库失败"); }
  };
  const toggleCompare = (hit: LiteratureSearchHit) => {
    const id = hit.document_id ?? hit.source_name;
    setCompareIds((current) => current.includes(id) ? current.filter((item) => item !== id) : current.length < 4 ? [...current, id] : current);
  };
  return <section className="suna-literature-page">
    <header className="suna-literature-header"><div><span className="suna-module-kicker">SUNA LITERATURE RESEARCH</span><h1><BookOpen size={25} />文献研究</h1><p>检索、阅读和比较钢铁材料领域文献</p></div><div className="suna-literature-header-actions"><button className="suna-ghost-button" onClick={importDocument}><FilePlus2 size={16} />导入文献</button><button className="suna-primary-button" onClick={() => void generateReview()} disabled={reviewStatus === "loading"}><Sparkles size={16} />{reviewStatus === "loading" ? "生成中..." : "生成综述"}</button></div></header>
    <nav className="suna-literature-tabs" aria-label="文献研究视图">{([["search", "文献检索"], ["reading", "文献阅读"], ["library", "文献管理"], ["review", "研究综述"]] as const).map(([value, label]) => <button key={value} className={activeTab === value ? "is-active" : ""} onClick={() => setActiveTab(value)}>{label}</button>)}</nav>
    <form className="suna-literature-searchbar" onSubmit={search}><Search size={18} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="输入材料、工艺、性能或研究问题，例如：高强钢 热处理 显微组织" /><button className="suna-primary-button" disabled={status === "loading"}>{status === "loading" ? <Loader2 className="suna-spin" size={16} /> : <Search size={16} />}检索</button></form>
    <div className="suna-literature-toolbar"><label>年份<select value={yearFilter} onChange={(event) => setYearFilter(event.target.value)}><option value="all">近5年</option>{years.map((year) => <option value={year} key={year}>{year}</option>)}</select><ChevronDown size={14} /></label><label>文献类型<select value={typeFilter} onChange={(event) => setTypeFilter(event.target.value)}><option value="all">全部类型</option>{types.map((type) => <option value={type} key={type}>{type}</option>)}</select><ChevronDown size={14} /></label><label>排序<select value={sortOrder} onChange={(event) => setSortOrder(event.target.value)}><option value="relevance">相关性优先</option><option value="latest">最新发布</option></select><ChevronDown size={14} /></label><button className="suna-toolbar-filter" onClick={() => setFilterOpen((value) => !value)}><Library size={15} />{filterOpen ? "收起筛选" : "更多筛选"}</button></div>
    {filterOpen && <div className="suna-literature-filter"><label>来源<select value={sourceFilter} onChange={(event) => setSourceFilter(event.target.value)}><option value="all">全部来源</option>{sources.map((source) => <option value={source} key={source}>{source}</option>)}</select></label></div>}
    <div className="suna-literature-layout"><div className="suna-literature-results"><div className="suna-literature-result-head"><div><span>{status === "loading" ? "正在检索知识库" : `共找到 ${visibleResults.length} 篇相关文献`}</span></div><span className="suna-literature-chip"><Library size={14} />本地知识库</span></div>
      {status === "error" && <div className="suna-literature-error"><X size={16} /><span>{error}</span><button onClick={() => void search()}>重试</button></div>}
      {status === "idle" && results.length === 0 && <div className="suna-literature-empty"><BookOpen size={28} /><strong>从研究问题开始</strong><span>检索结果会显示来源、相关度和可追溯位置。</span></div>}
      <div className="suna-literature-list">{visibleResults.map((hit, index) => { const id = hit.document_id ?? hit.source_name; const active = selectedId === id; return <article className={`suna-literature-result ${active ? "is-active" : ""}`} key={`${id}-${index}`} onClick={() => setSelectedId(id)}><div className="suna-literature-result-top"><span className="suna-literature-result-index">{index + 1}</span><div><h3>{titleOf(hit)}</h3><p>{hit.source_name} &nbsp;|&nbsp; {sourceLocation(hit)} &nbsp;|&nbsp; {hit.source_name.includes("学位") ? "学位论文" : hit.source_name.includes("会议") ? "会议论文" : "中文期刊"}</p></div></div><p className="suna-literature-excerpt">{excerptOf(hit)}</p><div className="suna-literature-tags"><span>{hit.source_name.includes("钢") ? "钢铁材料" : "材料研究"}</span><span>检索证据</span><span>{hit.score != null ? `${Math.round(Math.max(0, Math.min(1, hit.score)) * 100)}% 相关` : "相关文献"}</span></div><div className="suna-literature-result-actions"><button onClick={(event) => { event.stopPropagation(); void read(hit); }}><BookOpen size={15} />阅读</button><button onClick={(event) => { event.stopPropagation(); toggleFavorite(hit); }}>{favoriteSet.has(id) ? <BookmarkCheck size={15} /> : <Bookmark size={15} />}{favoriteSet.has(id) ? "已收藏" : "收藏"}</button><button onClick={(event) => { event.stopPropagation(); toggleCompare(hit); }}><Plus size={15} />{compareIds.includes(id) ? "已加入对比" : "加入对比"}</button></div></article>; })}</div></div>
      <aside className="suna-literature-detail"><div className="suna-literature-detail-head"><div><span>文献详情</span><h2>{selected ? titleOf(selected) : "选择一篇文献"}</h2></div>{selected && <button className="suna-icon-button" onClick={() => toggleFavorite(selected)} aria-label="收藏文献">{favoriteSet.has(selected.document_id ?? selected.source_name) ? <BookmarkCheck size={18} /> : <Bookmark size={18} />}</button>}</div>{selected ? <><div className="suna-literature-meta"><span><Quote size={14} />{selected.source_name}</span><span>{sourceLocation(selected)}</span><span>相关度 {selected.score != null ? selected.score.toFixed(3) : "-"}</span></div><p className="suna-literature-detail-text">{excerptOf(selected)}</p><div className="suna-literature-detail-actions"><button className="suna-primary-button" onClick={() => void read()} disabled={readingStatus === "loading"}>{readingStatus === "loading" ? <Loader2 className="suna-spin" size={15} /> : <BookOpen size={15} />}阅读原文</button><button className="suna-ghost-button" onClick={() => void summarize()} disabled={summaryStatus === "loading"}>{summaryStatus === "loading" ? <Loader2 className="suna-spin" size={15} /> : <Sparkles size={15} />}生成总结</button><button className="suna-icon-button" onClick={() => void copyCitation(selected)} aria-label="复制引用"><Clipboard size={15} /></button></div>{reading && <div className="suna-literature-reading"><div><strong>{reading.section_title || reading.document || "文献正文"}</strong><span>{reading.part && reading.total_parts ? `第 ${reading.part}/${reading.total_parts} 部分` : ""}</span></div><p>{reading.content || reading.error || "没有可显示的正文"}</p>{reading.has_more && <button className="suna-ghost-button" onClick={() => void read(selected, (reading.part ?? 1) + 1)}>继续阅读</button>}</div>}{summary && <div className="suna-literature-summary"><strong>AI 摘要</strong><p>{summary}</p></div>}</> : <div className="suna-literature-detail-empty"><BookOpen size={24} /><span>选择检索结果查看证据和操作</span></div>}</aside></div>
    {compareItems.length > 0 && <div className="suna-literature-compare"><div className="suna-literature-compare-head"><div><strong>文献对比</strong><span>已选择 {compareItems.length}/4 篇</span></div><div className="suna-literature-compare-actions"><button className="suna-ghost-button" onClick={() => void generateComparison()} disabled={compareStatus === "loading"}>{compareStatus === "loading" ? "分析中..." : "生成差异分析"}</button><button onClick={() => setCompareIds([])} aria-label="清空对比"><X size={16} /></button></div></div><div className="suna-literature-compare-table"><div className="suna-literature-compare-row"><strong>研究对象</strong>{compareItems.map((item) => <span key={item.document_id ?? item.source_name}>{titleOf(item)}</span>)}</div><div className="suna-literature-compare-row"><strong>来源</strong>{compareItems.map((item) => <span key={item.document_id ?? item.source_name}>{item.source_name}</span>)}</div><div className="suna-literature-compare-row"><strong>证据位置</strong>{compareItems.map((item) => <span key={item.document_id ?? item.source_name}>{sourceLocation(item)}</span>)}</div><div className="suna-literature-compare-row"><strong>相关片段</strong>{compareItems.map((item) => <span key={item.document_id ?? item.source_name}>{excerptOf(item)}</span>)}</div></div>{compareSummary && <div className="suna-literature-ai-result"><strong>AI 差异分析</strong><p>{compareSummary}</p></div>}</div>}
    {review && <div className="suna-literature-review"><div><strong>文献综述</strong><span>基于当前选择的 {compareItems.length} 篇文献生成</span></div><p>{review}</p></div>}
    {notice && <button className="suna-literature-notice" onClick={() => setNotice("")}><Check size={15} />{notice}</button>}
  </section>;
}
