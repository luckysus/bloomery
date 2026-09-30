import { useMemo, useState } from "react";
import { ArrowRight, BarChart3, Beaker, CheckCircle2, ChevronDown, FileText, Filter, Gauge, Network, Play, Plus, Search, Settings2, Sparkles, Table2, Upload, Wrench, X } from "lucide-react";
import { getNavigationSection, type SectionId } from "../../app/navigation";

type Row = { title: string; meta: string; status: string; tone?: "good" | "pending" };
const copy: Record<string, { action: string; hint: string; fields: string[] }> = {
  data: { action: "导入数据", hint: "支持 Excel、CSV、JSON 和数据库连接", fields: ["数据集", "数据清洗", "统计分析", "可视化"] },
  prediction: { action: "开始预测", hint: "选择材料体系、模型和预测目标", fields: ["材料体系", "数据集", "预测模型", "预测目标"] },
  optimization: { action: "开始优化", hint: "设置目标、变量和约束条件", fields: ["优化目标", "优化变量", "约束条件", "算法"] },
  experiment: { action: "设计实验", hint: "基于现有数据推荐下一组高价值实验", fields: ["实验目标", "变量选择", "设计方法", "评估标准"] },
};

export default function ResearchModulePage({ section }: { section: SectionId }) {
  const active = getNavigationSection(section);
  const [query, setQuery] = useState("");
  const [running, setRunning] = useState(false);
  const [selected, setSelected] = useState(0);
  const config = copy[section];
  const list: Row[] = [];
  const Icon = active.icon;
  const summary = ["项目数量", "0", "当前状态", "等待数据"];
  return <section className="suna-research-page"><header className="suna-research-header"><div><span className="suna-module-kicker">SUNA RESEARCH PLATFORM</span><h1><Icon size={23} />{active.label}</h1><p>{active.description}</p></div><div className="suna-research-actions"><button className="suna-ghost-button"><Filter size={15} />筛选</button><button className="suna-primary-button" onClick={() => setRunning(true)}><Plus size={15} />{config?.action ?? (section === "literature" ? "检索文献" : "添加")}</button></div></header>
    <div className="suna-research-summary">{[0, 1].map((i) => <div className="suna-research-summary-card" key={i}><span>{summary[i * 2]}</span><strong>{summary[i * 2 + 1]}</strong><small>{i === 0 ? "工作区统计" : "最近 30 天"}</small></div>)}</div>
    {config ? <ConfigWorkspace config={config} running={running} onRun={() => setRunning(true)} /> : <ListWorkspace section={section} list={list} query={query} onQuery={setQuery} selected={selected} onSelect={setSelected} />}
    {running && <div className="suna-run-banner"><div><CheckCircle2 size={17} /><strong>{config?.action ?? "任务"}已创建</strong><span>Agent Runtime 已接收任务，完成后会在运行记录中显示结果。</span></div><button onClick={() => setRunning(false)} aria-label="关闭"><X size={16} /></button></div>}
  </section>;
}
function ListWorkspace({ section, list, query, onQuery, selected, onSelect }: { section: string; list: Row[]; query: string; onQuery: (v: string) => void; selected: number; onSelect: (v: number) => void }) { const title = section === "literature" ? "文献检索结果" : section === "agents" ? "Agent 列表" : section === "tools" ? "工具列表" : section === "mcp" ? "MCP Server" : "Skill 列表"; return <div className="suna-research-panel"><div className="suna-research-panel-head"><div><h2>{title}</h2><span>{list.length} 个项目</span></div><label className="suna-research-search"><Search size={15} /><input value={query} onChange={(e) => onQuery(e.target.value)} placeholder="搜索..." /></label></div><div className="suna-research-list">{list.map((item, index) => <button key={item.title} className={`suna-research-row ${selected === index ? "is-selected" : ""}`} onClick={() => onSelect(index)}><span className="suna-research-row-icon"><FileText size={17} /></span><span><strong>{item.title}</strong><small>{item.meta}</small></span><em className={item.tone === "good" ? "is-good" : ""}>{item.status}</em><ArrowRight size={15} /></button>)}</div>{list.length === 0 && <div className="suna-research-empty"><Search size={21} /><p>没有匹配结果</p></div>}</div>; }
function ConfigWorkspace({ config, running, onRun }: { config: { action: string; hint: string; fields: string[] }; running: boolean; onRun: () => void }) { return <div className="suna-config-workspace"><div className="suna-config-form"><div className="suna-research-panel-head"><div><h2>任务配置</h2><span>{config.hint}</span></div><Settings2 size={17} /></div>{config.fields.map((field) => <label className="suna-config-field" key={field}><span>{field}</span><div><input placeholder={`选择${field}`} /><ChevronDown size={14} /></div></label>)}<button className="suna-primary-button suna-config-run" onClick={onRun} disabled={running}><Play size={15} />{running ? "运行中..." : config.action}</button></div><div className="suna-config-preview"><div className="suna-research-panel-head"><div><h2>结果预览</h2><span>完成任务后将在这里显示结果</span></div><BarChart3 size={18} /></div><div className="suna-chart-placeholder"><div><BarChart3 size={28} /><p>尚未生成结果</p><small>运行 Agent 后查看分析图表</small></div></div></div></div>; }
