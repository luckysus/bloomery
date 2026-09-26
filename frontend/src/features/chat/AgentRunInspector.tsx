import {
  AlertTriangle,
  Check,
  CircleDashed,
  Clock3,
  FileText,
  History,
  LoaderCircle,
  ShieldAlert,
  Sparkles,
  Wrench,
  X,
} from "lucide-react";
import { useLocale } from "../../i18n/locale";
import type { PermissionDecision } from "../../bridge/generated/protocol";
import type { RecoveredRun } from "../../bridge/desktop";
import type { AgentPermissionView, AgentRunView, AgentToolView } from "./agentEvents";

interface AgentRunInspectorProps {
  run: AgentRunView | null;
  recovery: RecoveredRun | null;
  onResolvePermission: (permissionId: string, decision: PermissionDecision) => void;
  onRetry: () => void;
  onResume: () => void;
}

const terminalStates: AgentRunView["state"][] = ["completed", "failed", "cancelled", "interrupted"];

function stateTone(state: AgentRunView["state"]) {
  if (state === "completed") return "is-good";
  if (["failed", "cancelled", "interrupted"].includes(state)) return "is-danger";
  if (state === "awaiting_permission") return "is-pending";
  return "is-neutral";
}

function stateLabel(state: AgentRunView["state"], t: ReturnType<typeof useLocale>["t"]) {
  if (state === "completed") return t("runtimeReady");
  if (["failed"].includes(state)) return t("chatError");
  if (["cancelled"].includes(state)) return t("stopGenerating");
  if (state === "awaiting_permission") return t("permissionRequired");
  if (terminalStates.includes(state)) return t("contextPreparing");
  return t("generating");
}

function toolTone(tool: AgentToolView) {
  if (tool.status === "succeeded") return "is-good";
  if (["failed", "cancelled"].includes(tool.status)) return "is-danger";
  return "is-pending";
}

function toolStatus(tool: AgentToolView) {
  if (tool.status === "succeeded") return "已完成";
  if (tool.status === "failed") return "失败";
  if (tool.status === "cancelled") return "已取消";
  if (tool.status === "running") return `${tool.progress}%`;
  return "等待执行";
}

function formatTokens(value: number | undefined) {
  return typeof value === "number" ? value.toLocaleString() : "—";
}

function PermissionActions({
  permission,
  onResolve,
}: {
  permission: AgentPermissionView;
  onResolve: (permissionId: string, decision: PermissionDecision) => void;
}) {
  const actions: Array<{ decision: PermissionDecision; label: string; danger?: boolean }> = [
    { decision: "allow_once", label: "允许一次" },
    { decision: "allow_session", label: "本次会话允许" },
    { decision: "allow_always", label: "始终允许" },
    { decision: "deny", label: "拒绝", danger: true },
  ];

  return (
    <div className="bloomery-chat-permission-actions">
      {actions.map(({ decision, label, danger }) => (
        <button
          type="button"
          className={danger ? "bloomery-action-secondary is-danger" : "bloomery-action-secondary"}
          aria-label={`Agent Workspace permission ${decision}`}
          key={decision}
          onClick={() => onResolve(permission.permissionId, decision)}
        >
          {danger ? <X size={12} aria-hidden="true" /> : <Check size={12} aria-hidden="true" />}
          {label}
        </button>
      ))}
    </div>
  );
}

function ToolRow({ tool }: { tool: AgentToolView }) {
  return (
    <div className="bloomery-chat-inspector-tool">
      <span className={`bloomery-chat-inspector-tool-icon ${toolTone(tool)}`}>
        {tool.status === "running" ? <LoaderCircle size={14} className="bloomery-spin" aria-hidden="true" /> : <Wrench size={14} aria-hidden="true" />}
      </span>
      <div>
        <strong title={tool.name}>工具 · {tool.name}</strong>
        <span>{toolStatus(tool)}</span>
        {tool.message && <small>{tool.message}</small>}
        {tool.error && <small>{tool.error.code}: {tool.error.message}</small>}
      </div>
    </div>
  );
}

export default function AgentRunInspector({
  run,
  recovery,
  onResolvePermission,
  onRetry,
  onResume,
}: AgentRunInspectorProps) {
  const { t } = useLocale();

  if (!run) {
    return (
      <aside className="bloomery-chat-inspector" aria-label="Agent Workspace">
        <div className="bloomery-chat-inspector-empty">
          <span className="bloomery-chat-inspector-empty-icon"><Sparkles size={18} aria-hidden="true" /></span>
          <strong>Agent Workspace</strong>
          <p>创建任务后，这里会显示运行状态、工具调用、权限和来源。</p>
        </div>
      </aside>
    );
  }

  const pendingPermissions = run.permissions.filter((permission) => permission.decision === null);
  const settled = terminalStates.includes(run.state);
  const progress = run.taskProgress?.progress ?? (run.state === "completed" ? 100 : 0);
  const canRetry = settled && run.state !== "completed";
  const canResume = recovery?.action.kind === "resume_from_checkpoint";

  return (
    <aside className="bloomery-chat-inspector" aria-label="Agent Workspace">
      <header className="bloomery-chat-inspector-header">
        <div>
          <p className="bloomery-eyebrow">AGENT WORKSPACE</p>
          <h3>运行检查器</h3>
        </div>
        <span className={`bloomery-chat-inspector-state ${stateTone(run.state)}`}>
          <CircleDashed size={12} aria-hidden="true" />{stateLabel(run.state, t)}
        </span>
      </header>

      <div className="bloomery-chat-inspector-metrics">
        <div><span>事件序号</span><strong>{run.sequence}</strong></div>
        <div><span>工具调用</span><strong>{run.toolCalls.length}</strong></div>
        <div><span>Token</span><strong>{formatTokens(run.usage?.total_tokens)}</strong></div>
        <div><span>引用</span><strong>{run.citationNumbers.length}</strong></div>
      </div>

      <section className="bloomery-chat-inspector-section" aria-labelledby="agent-progress-heading">
        <div className="bloomery-chat-inspector-section-heading">
          <h4 id="agent-progress-heading"><Clock3 size={13} aria-hidden="true" />当前进度</h4>
          <span>{run.taskProgress?.kind ?? stateLabel(run.state, t)}</span>
        </div>
        <div className="bloomery-chat-inspector-progress" aria-label={`任务进度 ${progress}%`}>
          <span style={{ width: `${Math.max(0, Math.min(100, progress))}%` }} />
        </div>
      </section>

      {run.toolCalls.length > 0 && (
        <section className="bloomery-chat-inspector-section" aria-labelledby="agent-tools-heading">
          <div className="bloomery-chat-inspector-section-heading">
            <h4 id="agent-tools-heading"><Wrench size={13} aria-hidden="true" />工具调用</h4>
            <span>{run.toolCalls.length}</span>
          </div>
          <div className="bloomery-chat-inspector-tool-list">
            {run.toolCalls.map((tool) => <ToolRow key={tool.toolCallId} tool={tool} />)}
          </div>
        </section>
      )}

      {pendingPermissions.length > 0 && (
        <section className="bloomery-chat-inspector-section bloomery-chat-inspector-permissions" aria-labelledby="agent-permissions-heading">
          <div className="bloomery-chat-inspector-section-heading">
            <h4 id="agent-permissions-heading"><ShieldAlert size={13} aria-hidden="true" />需要授权</h4>
            <span>{pendingPermissions.length}</span>
          </div>
          <div className="bloomery-chat-permission-list">
            {pendingPermissions.map((permission) => (
              <div className="bloomery-chat-permission" key={permission.permissionId}>
                <div className="bloomery-chat-permission-heading">
                  <ShieldAlert size={14} aria-hidden="true" />
                  <div><strong>授权 · {permission.summary}</strong><span>{permission.risk}</span></div>
                </div>
                <p>{permission.reason}</p>
                <PermissionActions permission={permission} onResolve={onResolvePermission} />
              </div>
            ))}
          </div>
        </section>
      )}

      {(run.checkpoint || run.recovery || recovery) && (
        <section className="bloomery-chat-inspector-section" aria-labelledby="agent-recovery-heading">
          <div className="bloomery-chat-inspector-section-heading">
            <h4 id="agent-recovery-heading"><History size={13} aria-hidden="true" />恢复</h4>
            <span>{run.recovery?.action ?? recovery?.action.kind ?? "checkpoint"}</span>
          </div>
          {run.checkpoint && <small>checkpoint · 第 {run.checkpoint.modelCallIndex} 次模型调用 · 工具轮次 {run.checkpoint.toolRound}</small>}
          {run.recovery && <small>{run.recovery.phase === "started" ? "正在恢复" : "恢复已完成"} · 尝试 {run.recovery.recoveryAttempt}</small>}
          {(canResume || canRetry) && (
            <div className="bloomery-chat-permission-actions">
              {canResume && <button type="button" className="bloomery-action-secondary" aria-label="Agent Workspace resume" onClick={onResume}>恢复运行</button>}
              {canRetry && <button type="button" className="bloomery-action-secondary" aria-label="Agent Workspace retry" onClick={onRetry}>重试</button>}
            </div>
          )}
        </section>
      )}

      {run.citationNumbers.length > 0 && (
        <section className="bloomery-chat-inspector-section" aria-labelledby="agent-sources-heading">
          <div className="bloomery-chat-inspector-section-heading">
            <h4 id="agent-sources-heading"><FileText size={13} aria-hidden="true" />来源</h4>
            <span>{run.citationNumbers.length}</span>
          </div>
          <div className="bloomery-chat-inspector-citations">
            {run.citationNumbers.map((number) => <span key={number}>[{number}]</span>)}
          </div>
        </section>
      )}

      {run.error && (
        <section className="bloomery-chat-inspector-section" aria-labelledby="agent-error-heading">
          <div className="bloomery-chat-inspector-section-heading">
            <h4 id="agent-error-heading"><AlertTriangle size={13} aria-hidden="true" />错误</h4>
            <span>{run.error.category}</span>
          </div>
          <small>{run.error.code}: {run.error.message}</small>
        </section>
      )}
    </aside>
  );
}
