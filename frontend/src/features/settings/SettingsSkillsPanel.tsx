import { useEffect, useState } from "react";
import { Check, CircleAlert, FileCode2, Hash, LoaderCircle, Puzzle, RefreshCw, Tag } from "lucide-react";
import { desktop, type SkillCatalog, type SkillSummary } from "../../bridge/desktop";
import { useSkillStore } from "../../stores/skillStore";
import { useLocale } from "../../i18n/locale";
import { settingsErrorMessage } from "./settingsError";
import { Checkbox } from "../../components/ui/checkbox";

export default function SettingsSkillsPanel() {
  const { t } = useLocale();
  // 第 77 章：Skill 目录由 skillStore 承载。
  const catalog = useSkillStore((state) => state.catalog);
  const setCatalog = useSkillStore((state) => state.setCatalog);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const load = async () => { setLoading(true); setError(null); setNotice(null); try { setCatalog(await desktop.listSkills()); } catch (cause) { setError(settingsErrorMessage(cause, "无法读取 Skill 列表")); } finally { setLoading(false); } };
  useEffect(() => { void load(); }, []);
  const toggle = async (skill: SkillSummary) => { setBusy(skill.name); setError(null); setNotice(null); try { setCatalog(await desktop.setSkillEnabled(skill.name, !skill.enabled)); setNotice(`${skill.name} 已${skill.enabled ? "停用" : "启用"}`); } catch (cause) { setError(settingsErrorMessage(cause, "Skill 状态保存失败")); } finally { setBusy(null); } };
  const enabledCount = catalog.skills.filter((skill) => skill.enabled).length;
  if (loading) return <section className="suna-settings-form-panel" aria-busy="true"><p className="suna-settings-loading" role="status" aria-live="polite"><LoaderCircle size={16} className="suna-spin" />{t("loading")}</p></section>;
  return <section className="suna-settings-form-panel suna-settings-skill-panel" aria-labelledby="settings-skills-heading" aria-busy={busy !== null}><header className="suna-settings-form-heading"><div><span className="suna-settings-kicker">REUSABLE CAPABILITIES</span><h2 id="settings-skills-heading">{t("settingsCategorySkill")}</h2><p>管理可复用的专业研究能力、来源和加载状态。</p></div><div className="suna-settings-form-heading-actions"><button type="button" className="suna-icon-button" onClick={() => void load()} disabled={loading || busy !== null} aria-label="刷新 Skill"><RefreshCw size={17} /></button><Puzzle size={22} aria-hidden="true" /></div></header><p className="suna-skill-limit" role="status">已启用 {enabledCount}/12 个 Skill；Agent 每次运行最多加载 12 个 Skill。</p>{error && <p className="suna-settings-inline-error" role="alert">{error}</p>}{notice && <p className="suna-settings-notice" role="status"><Check size={14} />{notice}</p>}{catalog.errors.length > 0 && <div className="suna-skill-errors" role="alert">{catalog.errors.map((item) => <p key={item.path}><CircleAlert size={14} />{item.path} · {item.message}</p>)}</div>}{catalog.skills.length === 0 ? <p className="suna-settings-empty"><Puzzle size={16} />{catalog.errors.length > 0 ? "未能加载可用 Skill，请修复上方错误后重试。" : t("extensionsNoSkills")}</p> : <div className="suna-settings-skill-list">{catalog.skills.map((skill) => <article className="suna-settings-skill-card" key={skill.name + "-" + skill.source.path}><div className="suna-settings-skill-card-head"><span className="suna-settings-skill-icon"><FileCode2 size={18} /></span><div><strong>{skill.name}</strong><p>{skill.description || "可复用的专业 Agent 能力"}</p></div><label className="suna-skill-switch"><Checkbox aria-label={`启用 ${skill.name}`} checked={skill.enabled} disabled={busy === skill.name} onCheckedChange={() => void toggle(skill)} /></label></div><div className="suna-settings-skill-meta"><span><Tag size={13} />{skill.tags.length ? skill.tags.join(" · ") : "未分类"}</span><span>v{skill.version}</span><span>{skill.enabled ? <><Check size={13} />已启用</> : "已停用"}</span></div><div className="suna-settings-skill-details"><span>来源：{skill.source.scope === "user" ? "用户 Skill" : skill.source.scope}</span><span>兼容：{skill.compatibility.length ? skill.compatibility.join(", ") : "通用"}</span><span title={skill.content_sha256}><Hash size={12} />{skill.content_sha256.slice(0, 16)}…</span></div></article>)}</div>}</section>;
}
