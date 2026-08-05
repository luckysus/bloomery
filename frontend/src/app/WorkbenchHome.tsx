import {
  ArrowUpRight,
  BookOpen,
  CircleCheck,
  Clock3,
  Database,
  FileUp,
  MessageSquarePlus,
  Server,
} from "lucide-react";

interface WorkbenchHomeProps {
  initializationState: "loading" | "ready" | "failed";
  onOpenSection: (section: "chat" | "knowledge" | "analysis") => void;
}

const statusRows = [
  { label: "本地运行时", value: "已启动", icon: Server, tone: "good" },
  { label: "知识库", value: "尚未建立", icon: BookOpen, tone: "neutral" },
  { label: "后台任务", value: "无活动任务", icon: Clock3, tone: "neutral" },
] as const;

export default function WorkbenchHome({ initializationState, onOpenSection }: WorkbenchHomeProps) {
  const ready = initializationState === "ready";

  return (
    <div className="bloomery-workbench" data-testid="workbench-home">
      <section className="bloomery-hero" aria-labelledby="workbench-heading">
        <div>
          <p className="bloomery-eyebrow">LOCAL WORKSPACE / STEEL DOMAIN</p>
          <h1 id="workbench-heading">工作台</h1>
          <p className="bloomery-lede">从本地知识、对话和生产数据开始一次可追溯的工作。</p>
        </div>
        <div className="bloomery-hero-mark" aria-hidden="true">
          <span className="bloomery-hero-mark-line" />
          <span className="bloomery-hero-mark-line bloomery-hero-mark-line-short" />
          <span className="bloomery-hero-mark-dot" />
        </div>
      </section>

      <section className="bloomery-action-strip" aria-label="常用操作">
        <button type="button" className="bloomery-action-primary" onClick={() => onOpenSection("chat")}>
          <MessageSquarePlus size={18} aria-hidden="true" />
          <span>新建对话</span>
          <ArrowUpRight size={16} aria-hidden="true" />
        </button>
        <button type="button" className="bloomery-action-secondary" onClick={() => onOpenSection("knowledge")}>
          <FileUp size={17} aria-hidden="true" />
          <span>导入文档</span>
        </button>
        <button type="button" className="bloomery-action-secondary" onClick={() => onOpenSection("analysis")}>
          <Database size={17} aria-hidden="true" />
          <span>导入数据</span>
        </button>
      </section>

      <section className="bloomery-status-section" aria-labelledby="status-heading">
        <div className="bloomery-section-heading">
          <div>
            <p className="bloomery-eyebrow">RUNTIME STATUS</p>
            <h2 id="status-heading">运行状态</h2>
          </div>
          <span className={`bloomery-state-badge ${ready ? "is-good" : "is-pending"}`}>
            <span className="bloomery-state-dot" aria-hidden="true" />
            {ready ? "本地就绪" : initializationState === "failed" ? "需要检查" : "初始化中"}
          </span>
        </div>
        <div className="bloomery-status-list">
          {statusRows.map(({ label, value, icon: Icon, tone }) => (
            <div className="bloomery-status-row" key={label}>
              <span className={`bloomery-status-icon is-${tone}`} aria-hidden="true">
                <Icon size={16} />
              </span>
              <span className="bloomery-status-label">{label}</span>
              <span className="bloomery-status-value">{value}</span>
              <CircleCheck className="bloomery-status-check" size={16} aria-hidden="true" />
            </div>
          ))}
        </div>
      </section>

      <section className="bloomery-empty-section" aria-labelledby="recent-heading">
        <div className="bloomery-section-heading">
          <div>
            <p className="bloomery-eyebrow">RECENT WORK</p>
            <h2 id="recent-heading">最近工作</h2>
          </div>
          <span className="bloomery-muted-label">0 项记录</span>
        </div>
        <div className="bloomery-empty-state">
          <div className="bloomery-empty-icon" aria-hidden="true">
            <BookOpen size={22} />
          </div>
          <div>
            <strong>工作区还没有记录</strong>
            <p>新建一次对话或导入一份文档后，内容会出现在这里。</p>
          </div>
        </div>
      </section>
    </div>
  );
}
