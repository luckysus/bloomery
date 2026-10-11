import { useEffect, useState } from "react";
import { CalendarPlus, CheckCircle2, FlaskConical, Trash2 } from "lucide-react";
import { desktop, type ExperimentPlanRecord } from "../../bridge/desktop";
import ExperimentDesignPanel from "./ExperimentDesignPanel";
import "./experiment.css";

/**
 * 第 44–46 章：实验助手。
 *
 * 实验设计与下一实验推荐由 worker 端真实算法承担（DOE/正交/响应面/贝叶斯/
 * 主动学习，见 ExperimentDesignPanel）；本页同时展示第 43 章的实验计划。
 */

function planVariables(plan: ExperimentPlanRecord): Array<{ name: string; value?: number }> {
  try {
    return JSON.parse(plan.variables_json) as Array<{ name: string; value?: number }>;
  } catch {
    return [];
  }
}

export default function ExperimentAssistantPage() {
  const [plans, setPlans] = useState<ExperimentPlanRecord[]>([]);
  const [datasets, setDatasets] = useState<Awaited<ReturnType<typeof desktop.listSteelDatasets>>>([]);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  const loadPlans = async () => {
    try {
      setPlans(await desktop.listExperimentPlans());
    } catch {
      setError("无法加载实验计划");
    }
  };
  useEffect(() => { void loadPlans(); }, []);
  useEffect(() => {
    void (async () => {
      try {
        const items = await desktop.listSteelDatasets();
        setDatasets(items.filter((item) => item.mappingState === "ready"));
      } catch {
        setError("无法加载数据集");
      }
    })();
  }, []);

  const markPlan = async (plan: ExperimentPlanRecord, state: ExperimentPlanRecord["state"]) => {
    try {
      await desktop.setExperimentPlanState(plan.id, state);
      await loadPlans();
    } catch {
      setError("无法更新实验计划状态");
    }
  };
  const removePlan = async (plan: ExperimentPlanRecord) => {
    try {
      await desktop.deleteExperimentPlan(plan.id);
      await loadPlans();
    } catch {
      setError("无法删除实验计划");
    }
  };

  return (
    <section className="suna-experiment-page">
      <header className="suna-experiment-header">
        <div>
          <span className="suna-module-kicker">SUNA EXPERIMENT ASSISTANT</span>
          <h1><FlaskConical size={24} />实验助手</h1>
          <p>用实验设计算法与已有数据，规划下一组高价值实验。</p>
        </div>
      </header>
      {notice && <div className="suna-experiment-notice"><CheckCircle2 size={15} />{notice}</div>}
      {error && <div className="suna-experiment-notice"><CheckCircle2 size={15} />{error}</div>}
      <ExperimentDesignPanel datasets={datasets} onNotice={setNotice} onError={setError} />
      <section className="suna-design-panel">
        <div className="suna-experiment-title"><CalendarPlus size={17} /><div><h2>实验计划</h2><span>来自工艺优化候选或实验设计组合</span></div></div>
        <div className="suna-plan-list">
          {plans.map((plan) => (
            <article className="suna-plan-card" key={plan.id}>
              <div className="suna-plan-card-head">
                <strong>{plan.title}</strong>
                <small>{plan.recommendation_json ? "来自优化" : "手工"} · {new Date(plan.created_at).toLocaleString()}</small>
              </div>
              {plan.objective && <p className="suna-optimization-muted">{plan.objective}</p>}
              <div className="suna-plan-values">
                {planVariables(plan).map((variable, index) => (
                  <span key={index}>{variable.name}{variable.value != null ? ` = ${variable.value.toFixed(2)}` : ""}</span>
                ))}
              </div>
              <div className="suna-plan-actions">
                <span className={`suna-plan-badge ${plan.state === "accepted" ? "" : "is-muted"}`}>
                  {plan.state === "accepted" ? "已采纳" : plan.state === "proposed" ? "候选" : plan.state === "rejected" ? "已否决" : "草稿"}
                </span>
                {plan.state !== "accepted" && (
                  <button className="suna-ghost-button" onClick={() => void markPlan(plan, "accepted")}><CheckCircle2 size={13} />标记完成</button>
                )}
                <button className="suna-ghost-button" onClick={() => void removePlan(plan)}><Trash2 size={13} />删除</button>
              </div>
            </article>
          ))}
          {plans.length === 0 && <p className="suna-optimization-muted">暂无实验计划；在「工艺优化」把候选方案加入计划，或在上方把实验组合加入计划。</p>}
        </div>
      </section>
    </section>
  );
}
