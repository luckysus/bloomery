import { useState } from "react";
import { Download, FileChartColumn, Loader2, Sparkles } from "lucide-react";
import { desktop } from "../../bridge/desktop";
import "./reports.css";
import { Textarea } from "../../components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

export default function ResearchReportPage() {
  const [type, setType] = useState("研究报告");
  const [topic, setTopic] = useState("");
  const [content, setContent] = useState("");
  const [busy, setBusy] = useState(false);
  const generate = async () => { if (!topic.trim()) return; setBusy(true); try { const response = await desktop.desktopAgentChat({ message: `请生成一份${type}，主题为“${topic}”。包含研究背景、数据与方法、结果、讨论、结论、局限和参考来源占位。使用 Markdown，明确区分证据和推断，不展示思考过程。`, smartSearchEnabled: true }); setContent(response.answer); } finally { setBusy(false); } };
  const download = async () => { if (!content) return; const path = await desktop.saveFileDialog({ defaultPath: `${topic.replace(/[\\/:*?"<>|]/g, "_")}.md`, filters: [{ name: "Markdown", extensions: ["md"] }] }); if (!path) return; const blob = new Blob([content], { type: "text/markdown;charset=utf-8" }); const url = URL.createObjectURL(blob); const anchor = document.createElement("a"); anchor.href = url; anchor.download = path.split(/[\\/]/).pop() || "suna-report.md"; anchor.click(); URL.revokeObjectURL(url); };
  return <section className="suna-reports-page"><header className="suna-reports-header"><div><span className="suna-module-kicker">SUNA RESEARCH REPORTS</span><h1><FileChartColumn size={24} />科研报告</h1><p>把数据、文献和 Agent 分析整理成可追溯的研究文档。</p></div><button className="suna-primary-button" onClick={() => void generate()} disabled={busy}>{busy ? <Loader2 className="suna-spin" size={16} /> : <Sparkles size={16} />}{busy ? "生成中..." : "生成报告"}</button></header><div className="suna-reports-layout"><section className="suna-reports-config"><label>报告类型<Select value={type} onValueChange={(value) => setType(value)}>
  <SelectTrigger aria-label="报告类型"><SelectValue /></SelectTrigger>
  <SelectContent><SelectItem value="研究报告">研究报告</SelectItem><SelectItem value="实验报告">实验报告</SelectItem><SelectItem value="文献综述">文献综述</SelectItem><SelectItem value="技术报告">技术报告</SelectItem>
  </SelectContent>
</Select></label><label>研究主题<Textarea value={topic} onChange={(event) => setTopic(event.target.value)} rows={5} /></label><p>生成时会调用 Agent 检索当前知识中心和文献证据。</p></section><section className="suna-reports-preview"><div className="suna-reports-preview-head"><strong>报告预览</strong>{content && <button className="suna-ghost-button" onClick={() => void download()}><Download size={15} />下载 Markdown</button>}</div>{content ? <article>{content}</article> : <div className="suna-reports-empty"><FileChartColumn size={30} /><strong>尚未生成报告</strong><span>设置主题后开始生成。</span></div>}</section></div></section>;
}
