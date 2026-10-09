import { useEffect, useRef, useState } from "react";
import { Bot, CircleAlert, GitBranch, LoaderCircle, RefreshCw, Square, Wrench } from "lucide-react";
import { desktop, type AgentChildTurnRecord } from "../../bridge/desktop";
import type { AgentEventEnvelope, AgentRunState, PermissionDecision } from "../../bridge/generated/protocol";
import { createAgentRunView, reduceAgentEvents, type AgentRunView } from "./agentEvents";
import "./childAgents.css";

const settledStates: AgentRunState[] = ["completed", "cancelled", "failed", "interrupted"];
const stateLabels: Record<AgentRunState, string> = {
  created: "等待启动", preparing: "准备中", generating: "生成中", awaiting_permission: "等待授权",
  executing_tools: "执行工具", verifying: "核对结果", completing: "整合结果", completed: "已完成",
  cancelled: "已取消", failed: "失败", interrupted: "已中断",
};

interface ChildAgentPanelProps {
  parentRunId: string;
  parentConversationId: string;
  showToolDetails?: boolean;
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
}

function errorMessage(reason: unknown) {
  return reason instanceof Error ? reason.message : typeof reason === "string" ? reason : "无法读取子任务";
}

export default function ChildAgentPanel({ parentRunId, parentConversationId, showToolDetails = true, onResolvePermission }: ChildAgentPanelProps) {
  const [children, setChildren] = useState<AgentChildTurnRecord[]>([]);
  const [views, setViews] = useState<Record<string, AgentRunView>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [cancelling, setCancelling] = useState<string | null>(null);
  const [cancelRequested, setCancelRequested] = useState<string[]>([]);
  const [refreshNonce, setRefreshNonce] = useState(0);
  const viewsRef = useRef<Record<string, AgentRunView>>({});
  const generation = useRef(0);

  useEffect(() => {
    const currentGeneration = ++generation.current;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let unlisten: (() => void) | undefined;
    let fetching = false;
    let started = false;
    let pendingChildEvents: AgentEventEnvelope[] = [];
    viewsRef.current = {};
    setChildren([]); setViews({}); setSelectedId(null); setCancelling(null); setCancelRequested([]); setError(""); setLoading(true);
    if (typeof desktop.listAgentChildTurns !== "function") { setLoading(false); return; }

    const accept = (child: AgentChildTurnRecord, events: AgentEventEnvelope[]) => {
      if (!active) return;
      // Child events use the parent conversation id; the child session id remains metadata.
      const current = viewsRef.current[child.child_turn_id] ?? { ...createAgentRunView(child.child_turn_id, parentConversationId), state: child.state };
      const queued = pendingChildEvents.filter((event) => event.run_id === child.child_turn_id);
      pendingChildEvents = pendingChildEvents.filter((event) => event.run_id !== child.child_turn_id);
      viewsRef.current = { ...viewsRef.current, [child.child_turn_id]: reduceAgentEvents(current, [...events, ...queued]) };
      setViews(viewsRef.current);
    };
    const refresh = async () => {
      if (!active || fetching) return;
      fetching = true;
      try {
        const records = await desktop.listAgentChildTurns(parentRunId);
        if (!active) return;
        const scoped = records.filter((child) => child.parent_turn_id === parentRunId && child.parent_conversation_id === parentConversationId);
        setChildren(scoped);
        setSelectedId((current) => scoped.some((child) => child.child_turn_id === current) ? current : scoped[0]?.child_turn_id ?? null);
        scoped.forEach((child) => accept(child, []));
        const results = await Promise.allSettled(scoped.map(async (child) => {
          const events = await desktop.replayAgentChildTurn(child.child_turn_id, viewsRef.current[child.child_turn_id]?.sequence ?? 0);
          accept(child, events);
        }));
        const failed = results.find((result) => result.status === "rejected");
        if (active) setError(failed?.status === "rejected" ? errorMessage(failed.reason) : "");
      } catch (reason) { if (active) setError(errorMessage(reason)); }
      finally {
        fetching = false;
        if (active) { setLoading(false); timer = setTimeout(() => void refresh(), 3000); }
      }
    };
    const listenChildEvents = desktop.listenAgentChildEvents ?? desktop.listenAgentEvents;
    const receive = (event: AgentEventEnvelope) => {
        if (!active) return;
        const current = viewsRef.current[event.run_id];
        if (current && current.conversationId === event.conversation_id) {
          viewsRef.current = { ...viewsRef.current, [event.run_id]: reduceAgentEvents(current, [event]) };
          setViews(viewsRef.current);
        }
        if (!current && event.run_id !== parentRunId && event.conversation_id === parentConversationId) {
          pendingChildEvents = [...pendingChildEvents, event].slice(-512);
          if (timer) clearTimeout(timer);
          if (started) void refresh();
        }
    };
    void (async () => {
      try {
        if (typeof listenChildEvents === "function") {
          const stop = await listenChildEvents(receive);
          if (!active || currentGeneration !== generation.current) { stop(); return; }
          unlisten = stop;
        }
      } catch (reason) { if (active) setError(errorMessage(reason)); }
      started = true;
      if (active) await refresh();
    })();
    return () => { active = false; generation.current += 1; if (timer) clearTimeout(timer); unlisten?.(); };
  }, [parentRunId, parentConversationId, refreshNonce]);

  const cancel = async (child: AgentChildTurnRecord) => {
    const requestGeneration = generation.current;
    setCancelling(child.child_turn_id); setError("");
    try {
      const result = await desktop.cancelAgentChildTurn(child.child_turn_id);
      if (generation.current !== requestGeneration) return;
      setChildren((current) => current.map((item) => item.child_turn_id === child.child_turn_id ? result.child : item));
      const current = viewsRef.current[child.child_turn_id] ?? createAgentRunView(child.child_turn_id, parentConversationId);
      const next = reduceAgentEvents(current, result.events);
      viewsRef.current = { ...viewsRef.current, [child.child_turn_id]: next };
      if (!settledStates.includes(result.child.state) && !settledStates.includes(next.state)) {
        setCancelRequested((current) => current.includes(child.child_turn_id) ? current : [...current, child.child_turn_id]);
      }
      setViews(viewsRef.current);
    } catch (reason) { if (generation.current === requestGeneration) setError(errorMessage(reason)); }
    finally { if (generation.current === requestGeneration) setCancelling(null); }
  };

  const selected = children.find((child) => child.child_turn_id === selectedId);
  const view = selectedId ? views[selectedId] : undefined;
  const state = selected ? (settledStates.includes(selected.state) ? selected.state : view?.state ?? selected.state) : null;
  const progress = state === "completed" ? 100 : view?.taskProgress?.progress;

  return <section className="suna-chat-inspector-section suna-child-agents" aria-label="子 Agent 任务">
    <div className="suna-chat-inspector-section-heading"><h4><GitBranch size={13} />子 Agent 任务</h4><button type="button" className="suna-child-refresh" aria-label="刷新子任务" onClick={() => setRefreshNonce((value) => value + 1)}><RefreshCw size={12} /></button></div>
    {error && <p className="suna-child-error" role="alert"><CircleAlert size={12} />{error}</p>}
    {loading && children.length === 0 ? <small role="status"><LoaderCircle size={12} className="suna-spin" />读取子任务...</small> : children.length === 0 ? <small>此运行尚未委派子任务。</small> : <>
      <div className="suna-child-list">{children.map((child, index) => {
        const childView = views[child.child_turn_id];
        const childState = settledStates.includes(child.state) ? child.state : childView?.state ?? child.state;
        return <button type="button" className={`suna-child-select ${selectedId === child.child_turn_id ? "is-active" : ""}`} key={child.child_turn_id} onClick={() => setSelectedId(child.child_turn_id)} aria-pressed={selectedId === child.child_turn_id}><span><Bot size={12} />{child.agent_id || `子任务 ${index + 1}`}<code>{child.child_turn_id.slice(0, 8)}</code></span>{child.task_summary && <small className="suna-child-task-summary">{child.task_summary}</small>}<small>{stateLabels[childState]}{childView?.taskProgress ? ` · ${childView.taskProgress.progress}%` : ""}</small></button>;
      })}</div>
      {selected && <div className="suna-child-detail">
        <div className="suna-child-detail-header"><strong>{state ? stateLabels[state] : ""}</strong>{state && !settledStates.includes(state) && <button type="button" className="suna-action-secondary is-danger" disabled={cancelling === selected.child_turn_id || cancelRequested.includes(selected.child_turn_id)} onClick={() => void cancel(selected)}><Square size={10} />{cancelling === selected.child_turn_id ? "取消中" : cancelRequested.includes(selected.child_turn_id) ? "已请求取消" : "取消子任务"}</button>}</div>
        {typeof progress === "number" && <div className="suna-chat-inspector-progress" aria-label={`子任务进度 ${progress}%`}><span style={{ width: `${Math.max(0, Math.min(100, progress))}%` }} /></div>}
        <small>事件 {view?.sequence ?? 0} · Token {view?.usage?.total_tokens.toLocaleString() ?? "—"}</small>
        {(selected.provider || selected.model) && <small>模型 {selected.provider} · {selected.model || "默认"}</small>}
        {selected.task_summary && <p className="suna-child-task">{selected.task_summary}</p>}
        <small>开始于 {new Date(selected.created_at).toLocaleString()}</small>
        {showToolDetails && !!view?.toolCalls.length && <div className="suna-child-tools">{view.toolCalls.map((tool) => <div key={tool.toolCallId}><Wrench size={11} /><span>{tool.name}<small>{tool.status === "running" ? `${tool.progress}%` : tool.status}{tool.message ? ` · ${tool.message}` : ""}</small>{tool.error && <small className="suna-child-error">{tool.error.message}</small>}</span></div>)}</div>}
        {!!view?.permissions.filter((permission) => permission.decision === null).length && <div className="suna-child-permissions">{view.permissions.filter((permission) => permission.decision === null).map((permission) => <div key={permission.permissionId}><strong>{permission.summary}</strong><small>{permission.reason}</small><div><button type="button" className="suna-action-secondary" onClick={() => onResolvePermission(permission.permissionId, "allow_once")}>允许一次</button><button type="button" className="suna-action-secondary is-danger" onClick={() => onResolvePermission(permission.permissionId, "deny")}>拒绝</button></div></div>)}</div>}
        {view?.assistantText && <details open={state === "completed"}><summary>子任务结果</summary><pre className="suna-child-result">{view.assistantText}</pre></details>}
        {view?.error && <p className="suna-child-error">{view.error.code}: {view.error.message}</p>}
        {view?.checkpoint && <small>检查点 · 模型调用 {view.checkpoint.modelCalls} · 工具轮次 {view.checkpoint.toolRound}</small>}
      </div>}
    </>}
  </section>;
}
