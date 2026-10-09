import { useEffect, useState } from "react";
import { Check, CircleAlert, Puzzle, Wrench } from "lucide-react";
import { desktop, type SkillCatalog, type ToolCapabilitySummary } from "../../bridge/desktop";
import type { SectionId } from "../../app/navigation";
import AgentManagementPage from "./AgentManagementPage";
import "./management.css";

export default function CapabilityManagementPage({ section }: { section: Extract<SectionId, "agents" | "tools" | "skills"> }) {
  const [catalog, setCatalog] = useState<SkillCatalog>({ skills: [], errors: [] });
  const [tools, setTools] = useState<ToolCapabilitySummary[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    setError("");
    if (section === "skills") void desktop.listSkills().then(setCatalog).catch((reason) => setError(reason instanceof Error ? reason.message : "无法加载 Skill"));
    if (section === "tools") void desktop.listToolCapabilities().then(setTools).catch((reason) => setError(reason instanceof Error ? reason.message : "无法加载工具"));
  }, [section]);
  if (section === "agents") return <AgentManagementPage />;
  const title = section === "tools" ? "工具中心" : "Skill 管理";
  return <section className="suna-management-page"><header className="suna-management-header"><div><span className="suna-module-kicker">SUNA CAPABILITY CENTER</span><h1>{section === "tools" ? <Wrench size={24} /> : <Puzzle size={24} />}{title}</h1><p>{section === "tools" ? "查看 Agent 可以调用的本地能力和外部服务。" : "启用可复用的钢铁材料研究专业能力。"}</p></div></header>{error && <div className="suna-management-alert"><CircleAlert size={15} />{error}</div>}<div className="suna-management-grid">{section === "tools" && tools.map((tool) => <article className="suna-management-card" key={tool.id}><div className="suna-management-card-icon"><Wrench size={17} /></div><div><h2>{tool.name}</h2><p>{tool.description}</p><span className={`suna-management-badge ${tool.enabled ? "" : "is-muted"}`}>{tool.enabled ? <><Check size={12} />可用</> : "未接入"}</span></div></article>)}{section === "skills" && catalog.skills.map((skill) => <article className="suna-management-card" key={skill.name}><div className="suna-management-card-icon"><Puzzle size={17} /></div><div><h2>{skill.name}</h2><p>{skill.description || "可复用的专业 Agent 能力"}</p><span className={`suna-management-badge ${skill.enabled ? "" : "is-muted"}`}>{skill.enabled ? <><Check size={12} />已启用</> : "已停用"}</span></div><label className="suna-management-toggle"><input type="checkbox" checked={skill.enabled} onChange={() => void desktop.setSkillEnabled(skill.name, !skill.enabled).then(setCatalog).catch((reason) => setError(reason instanceof Error ? reason.message : "无法保存 Skill"))} />启用</label></article>)}{section === "skills" && catalog.skills.length === 0 && <div className="suna-management-empty">没有发现可用 Skill。</div>}</div></section>;
}
