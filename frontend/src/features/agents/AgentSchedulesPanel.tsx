import { useEffect, useState } from "react";
import { CalendarClock, LoaderCircle, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";
import { desktop, type AgentSchedule, type Conversation, type SaveAgentScheduleRequest } from "../../bridge/desktop";
import "./agentSchedules.css";
import { Checkbox } from "../../components/ui/checkbox";
import { Input } from "../../components/ui/input";
import { Textarea } from "../../components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../../components/ui/select";

type Props = { agentId?: string; conversationId?: string };

function newSchedule(agentId: string, conversationId = ""): SaveAgentScheduleRequest {
  return { agentId, conversationId, expression: "0 9 * * *", timezone: "Asia/Shanghai", prompt: "", enabled: true, recurring: true };
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const runStates: Record<string, string> = {
  completed: "已完成", cancelled: "已取消", failed: "失败", interrupted: "已中断",
  created: "已创建", preparing: "准备中", generating: "思考中", executing_tools: "执行工具",
  awaiting_permission: "等待确认", verifying: "检查结果", completing: "保存结果",
};

export default function AgentSchedulesPanel({ agentId, conversationId }: Props) {
  const [schedules, setSchedules] = useState<AgentSchedule[]>([]);
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [draft, setDraft] = useState<SaveAgentScheduleRequest>(() => newSchedule(agentId ?? "master", conversationId));
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [refresh, setRefresh] = useState(0);

  useEffect(() => {
    let current = true;
    setLoading(true);
    setError("");
    setDraft(newSchedule(agentId ?? "master", conversationId));
    void Promise.all([desktop.listAgentSchedules(), desktop.listConversations()]).then(([jobs, sessions]) => {
      if (!current) return;
      setSchedules(jobs.filter((job) => job.agentId === agentId));
      setConversations(sessions);
      setDraft((value) => ({ ...value, conversationId: value.conversationId || sessions[0]?.id || "" }));
    }).catch((reason) => { if (current) setError(errorText(reason)); })
      .finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [agentId, conversationId, refresh]);

  async function change(operation: () => Promise<unknown>) {
    setBusy(true);
    setError("");
    try { await operation(); setRefresh((value) => value + 1); }
    catch (reason) { setError(errorText(reason)); }
    finally { setBusy(false); }
  }

  function edit(job: AgentSchedule) {
    setDraft({ id: job.id, agentId: job.agentId, conversationId: job.conversationId ?? "",
      expression: job.expression, timezone: job.timezone, prompt: job.prompt, enabled: job.enabled, recurring: job.recurring });
    setError("");
  }

  return <section className="suna-agent-schedules" aria-label="Agent 定时计划">
    <header>
      <div><h3><CalendarClock size={16} />定时计划</h3><p>按指定时区执行，并把结果保存到所选会话。客户端运行期间生效；重新打开会继续处理待执行计划。</p></div>
      <button type="button" className="suna-button is-soft" disabled={busy || loading} onClick={() => setRefresh((value) => value + 1)}><RefreshCw size={14} />刷新</button>
    </header>
    {error && <p className="suna-schedule-error" role="alert">{error}</p>}
    {!agentId ? <p className="suna-schedule-empty">先保存或选择一个 Agent，再创建定时计划。</p> : <>
      {loading && <p className="suna-schedule-empty"><LoaderCircle size={14} />正在加载计划…</p>}
      {!loading && schedules.length === 0 && <p className="suna-schedule-empty">这个 Agent 还没有定时计划。</p>}
      <div className="suna-schedule-list">{schedules.map((job) => <article key={job.id}>
        <div className="suna-schedule-info">
          <strong>{job.prompt}</strong>
          <small>{conversations.find((session) => session.id === job.conversationId)?.title ?? "会话已不可用"} · {job.expression} · {job.timezone}</small>
          <small>{job.enabled ? `下次执行：${new Date(job.nextRunAtUtc).toLocaleString()}` : job.recurring ? "已暂停" : job.lastSlotAtUtc ? "单次计划已触发" : "尚未启用"}
            {job.lastRunState && ` · 最近运行：${runStates[job.lastRunState] ?? job.lastRunState}`}</small>
          {job.lastError && <p className="suna-schedule-error">最近计划状态：{job.lastError}</p>}
        </div>
        <div className="suna-schedule-actions">
          <label><Checkbox aria-label="启用" checked={job.enabled} disabled={busy} onCheckedChange={(checked) => void change(() => desktop.setAgentScheduleEnabled(job.id, checked === true))} />启用</label>
          <button type="button" className="suna-icon-button" aria-label="编辑计划" disabled={busy} onClick={() => edit(job)}><Pencil size={14} /></button>
          <button type="button" className="suna-icon-button" aria-label="删除计划" disabled={busy} onClick={() => void change(() => desktop.deleteAgentSchedule(job.id))}><Trash2 size={14} /></button>
        </div>
      </article>)}</div>
      <form onSubmit={(event) => { event.preventDefault(); void change(() => desktop.saveAgentSchedule(draft)); }}>
        <h4>{draft.id ? "编辑计划" : "新建计划"}</h4>
        <fieldset disabled={busy || loading}>
          <div className="suna-schedule-fields">
            <label>结果会话<Select value={draft.conversationId} required onValueChange={(value) => setDraft({ ...draft, conversationId: value })}>
  <SelectTrigger aria-label="结果会话"><SelectValue /></SelectTrigger>
  <SelectContent>
              <SelectItem value="">选择已有会话</SelectItem>{conversations.map((session) => <SelectItem key={session.id} value={session.id}>{session.title || "未命名会话"}</SelectItem>)}
  </SelectContent>
</Select></label>
            <label>时区<Input required value={draft.timezone} placeholder="Asia/Shanghai" onChange={(event) => setDraft({ ...draft, timezone: event.target.value })} /></label>
            <label>Cron 表达式<Input aria-label="Cron 表达式" required value={draft.expression} placeholder="0 9 * * *" onChange={(event) => setDraft({ ...draft, expression: event.target.value })} /><small>分钟 小时 日期 月份 星期；0 9 * * * 表示每天 09:00。</small></label>
            <label>重复执行<Select value={String(draft.recurring)} onValueChange={(value) => setDraft({ ...draft, recurring: value === "true" })}>
  <SelectTrigger aria-label="重复执行"><SelectValue /></SelectTrigger>
  <SelectContent><SelectItem value="true">按计划重复</SelectItem><SelectItem value="false">只执行下一次</SelectItem>
  </SelectContent>
</Select></label>
          </div>
          <label className="suna-schedule-prompt">执行指令<Textarea required maxLength={16000} rows={3} value={draft.prompt} placeholder="例如：总结本会话已有研究记录，并列出下一步实验建议。" onChange={(event) => setDraft({ ...draft, prompt: event.target.value })} /></label>
          <footer>
            <label><Checkbox aria-label="保存后启用" checked={draft.enabled} onCheckedChange={(checked) => setDraft({ ...draft, enabled: checked === true })} />保存后启用</label>
            <div>{draft.id && <button type="button" className="suna-button is-soft" onClick={() => setDraft(newSchedule(agentId, conversationId || conversations[0]?.id))}>取消编辑</button>}
              <button type="submit" className="suna-button" disabled={!draft.conversationId || !draft.prompt.trim()}>{busy ? <LoaderCircle size={14} /> : <Plus size={14} />}{draft.id ? "保存计划" : "创建计划"}</button></div>
          </footer>
        </fieldset>
      </form>
    </>}
  </section>;
}
