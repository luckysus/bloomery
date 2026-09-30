import { useState } from "react";
import { Beaker, CheckCircle2, FlaskConical, Loader2, Plus, Sparkles, Trash2 } from "lucide-react";
import { desktop } from "../../bridge/desktop";
import "./experiment.css";

type Variable = { name: string; low: string; high: string };
export default function ExperimentAssistantPage() {
  const [goal, setGoal] = useState("");
  const [method, setMethod] = useState("");
  const [variables, setVariables] = useState<Variable[]>([{ name: "", low: "", high: "" }]);
  const [recommendation, setRecommendation] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const add = () => setVariables((current) => [...current, { name: "", low: "", high: "" }]);
  const update = (index: number, key: keyof Variable, value: string) => setVariables((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, [key]: value } : item));
  const run = async () => {
    const valid = variables.filter((item) => item.name.trim() && item.low.trim() && item.high.trim());
    if (!goal.trim() || !method.trim() || valid.length === 0) { setNotice("请填写实验目标、设计方法和至少一个变量"); return; }
    setBusy(true); setRecommendation("");
    try { const response = await desktop.desktopAgentChat({ message: `请为钢铁材料实验设计下一组高价值实验。目标：${goal}\n方法：${method}\n变量范围：${valid.map((item) => `${item.name} ${item.low}-${item.high}`).join("；")}\n输出推荐实验组合、选择理由、预期观测指标和风险。不要展示思考过程。`, smartSearchEnabled: true }); setRecommendation(response.answer); } catch (reason) { setNotice(reason instanceof Error ? reason.message : "实验设计失败"); } finally { setBusy(false); }
  };
  return <section className="suna-experiment-page"><header className="suna-experiment-header"><div><span className="suna-module-kicker">SUNA EXPERIMENT ASSISTANT</span><h1><FlaskConical size={24} />实验助手</h1><p>结合已有数据和 Agent 分析，设计下一组高价值实验。</p></div><button className="suna-primary-button" onClick={() => void run()} disabled={busy}>{busy ? <Loader2 className="suna-spin" size={16} /> : <Sparkles size={16} />}{busy ? "分析中..." : "生成实验方案"}</button></header>{notice && <div className="suna-experiment-notice"><CheckCircle2 size={15} />{notice}</div>}<div className="suna-experiment-layout"><section className="suna-experiment-config"><div className="suna-experiment-title"><Beaker size={17} /><div><h2>实验设计</h2><span>DOE、响应面或主动学习</span></div></div><label>实验目标<textarea value={goal} onChange={(event) => setGoal(event.target.value)} rows={3} /></label><label>设计方法<select value={method} onChange={(event) => setMethod(event.target.value)}><option>2-level DOE</option><option>正交实验</option><option>响应面</option><option>Bayesian Optimization</option><option>Active Learning</option></select></label><div className="suna-experiment-variable-head"><strong>变量范围</strong><button className="suna-icon-button" onClick={add} aria-label="添加变量"><Plus size={15} /></button></div>{variables.map((item, index) => <div className="suna-experiment-variable" key={index}><input value={item.name} placeholder="变量名" onChange={(event) => update(index, "name", event.target.value)} /><input value={item.low} placeholder="下限" onChange={(event) => update(index, "low", event.target.value)} /><input value={item.high} placeholder="上限" onChange={(event) => update(index, "high", event.target.value)} />{variables.length > 1 && <button className="suna-icon-button" onClick={() => setVariables((current) => current.filter((_, itemIndex) => itemIndex !== index))} aria-label="删除变量"><Trash2 size={14} /></button>}</div>)}</section><section className="suna-experiment-result"><div className="suna-experiment-title"><Sparkles size={17} /><div><h2>Agent 推荐</h2><span>解释推荐原因和不确定性</span></div></div>{recommendation ? <div className="suna-experiment-answer">{recommendation}</div> : <div className="suna-experiment-empty"><FlaskConical size={30} /><strong>尚未生成实验方案</strong><span>填写变量范围后运行 Agent。</span></div>}</section></div></section>;
}
