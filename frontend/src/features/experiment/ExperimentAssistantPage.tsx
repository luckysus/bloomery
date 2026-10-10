import { useEffect, useState } from "react";
import { Beaker, CalendarPlus, CheckCircle2, FlaskConical, Loader2, Plus, Sparkles, Trash2 } from "lucide-react";
import { desktop, type ExperimentPlanRecord } from "../../bridge/desktop";
import "./experiment.css";
import { Input } from "../../components/ui/input";
import { Textarea } from "../../components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

type Variable = { name: string; low: string; high: string };
export default function ExperimentAssistantPage() {
  const [goal, setGoal] = useState("");
  const [method, setMethod] = useState("");
  const [variables, setVariables] = useState<Variable[]>([{ name: "", low: "", high: "" }]);
  const [recommendation, setRecommendation] = useState("");
  const [plans, setPlans] = useState<ExperimentPlanRecord[]>([]);
  useEffect(() => { void loadPlans(); }, []);
  const loadPlans = async () => {
    try { setPlans(await desktop.listExperimentPlans()); } catch { /* 计划列表加载失败不阻断实验设计 */ }
  };
  const planVariables = (plan: ExperimentPlanRecord) => {
    try { return JSON.parse(plan.variables_json) as Array<{ name: string; low?: number; high?: number; value?: number }>; } catch { return []; }
  };
  const markPlan = async (plan: ExperimentPlanRecord, state: ExperimentPlanRecord["state"]) => {
    try { await desktop.setExperimentPlanState(plan.id, state); await loadPlans(); } catch { setNotice("无法更新实验计划状态"); }
  };
  const removePlan = async (plan: ExperimentPlanRecord) => {
    try { await desktop.deleteExperimentPlan(plan.id); await loadPlans(); } catch { setNotice("无法删除实验计划"); }
  };
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
  return <section className="suna-experiment-page"><header className="suna-experiment-header"><div><span className="suna-module-kicker">SUNA EXPERIMENT ASSISTANT</span><h1><FlaskConical size={24} />实验助手</h1><p>结合已有数据和 Agent 分析，设计下一组高价值实验。</p></div><button className="suna-primary-button" onClick={() => void run()} disabled={busy}>{busy ? <Loader2 className="suna-spin" size={16} /> : <Sparkles size={16} />}{busy ? "分析中..." : "生成实验方案"}</button></header>{notice && <div className="suna-experiment-notice"><CheckCircle2 size={15} />{notice}</div>}<div className="suna-experiment-layout"><section className="suna-experiment-config"><div className="suna-experiment-title"><Beaker size={17} /><div><h2>实验设计</h2><span>DOE、响应面或主动学习</span></div></div><label>实验目标<Textarea value={goal} onChange={(event) => setGoal(event.target.value)} rows={3} /></label><label>设计方法<Select value={method} onValueChange={(value) => setMethod(value)}>
  <SelectTrigger aria-label="设计方法"><SelectValue /></SelectTrigger>
  <SelectContent><SelectItem value="2-level DOE">2-level DOE</SelectItem><SelectItem value="正交实验">正交实验</SelectItem><SelectItem value="响应面">响应面</SelectItem><SelectItem value="Bayesian Optimization">Bayesian Optimization</SelectItem><SelectItem value="Active Learning">Active Learning</SelectItem>
  </SelectContent>
</Select></label><div className="suna-experiment-variable-head"><strong>变量范围</strong><button className="suna-icon-button" onClick={add} aria-label="添加变量"><Plus size={15} /></button></div>{variables.map((item, index) => <div className="suna-experiment-variable" key={index}><Input value={item.name} placeholder="变量名" onChange={(event) => update(index, "name", event.target.value)} /><Input value={item.low} placeholder="下限" onChange={(event) => update(index, "low", event.target.value)} /><Input value={item.high} placeholder="上限" onChange={(event) => update(index, "high", event.target.value)} />{variables.length > 1 && <button className="suna-icon-button" onClick={() => setVariables((current) => current.filter((_, itemIndex) => itemIndex !== index))} aria-label="删除变量"><Trash2 size={14} /></button>}</div>)}</section><section className="suna-experiment-result"><div className="suna-experiment-title"><Sparkles size={17} /><div><h2>Agent 推荐</h2><span>解释推荐原因和不确定性</span></div></div>{recommendation ? <div className="suna-experiment-answer">{recommendation}</div> : <div className="suna-experiment-empty"><FlaskConical size={30} /><strong>尚未生成实验方案</strong><span>填写变量范围后运行 Agent。</span></div>}
  <div className="suna-experiment-title"><CalendarPlus size={17} /><div><h2>实验计划</h2><span>来自工艺优化候选或手工添加；完成一项标记即可</span></div></div>
  <div className="suna-plan-list">{plans.map((plan) => <article className="suna-plan-card" key={plan.id}><div className="suna-plan-card-head"><strong>{plan.title}</strong><small>{plan.recommendation_json ? "来自优化" : "手工"} · {new Date(plan.created_at).toLocaleString()}</small></div>{plan.objective && <p className="suna-optimization-muted">{plan.objective}</p>}<div className="suna-plan-values">{planVariables(plan).map((variable, index) => <span key={index}>{variable.name}{variable.value != null ? ` = ${variable.value.toFixed(2)}` : ` ${variable.low ?? "?"}–${variable.high ?? "?"}`}</span>)}</div><div className="suna-plan-actions"><span className={`suna-plan-badge ${plan.state === "accepted" ? "" : "is-muted"}`}>{plan.state === "accepted" ? "已采纳" : plan.state === "proposed" ? "候选" : plan.state === "rejected" ? "已否决" : "草稿"}</span>{plan.state !== "accepted" && <button className="suna-ghost-button" onClick={() => void markPlan(plan, "accepted")}><CheckCircle2 size={13} />标记完成</button>}<button className="suna-ghost-button" onClick={() => void removePlan(plan)}><Trash2 size={13} />删除</button></div></article>)}{plans.length === 0 && <p className="suna-optimization-muted">暂无实验计划；在「工艺优化」把候选方案加入计划后会出现在这里。</p>}</div>
</section></div></section>;
}
