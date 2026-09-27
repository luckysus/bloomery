import { useEffect, useState } from "react";
import { FileCode2, LoaderCircle, Puzzle } from "lucide-react";
import { desktop, type SkillCatalog, type SkillSummary } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

export default function SettingsSkillsPanel() {
  const { t } = useLocale();
  const [catalog, setCatalog] = useState<SkillCatalog>({ skills: [], errors: [] });
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void desktop.listSkills().then(setCatalog).catch((cause) => setError(String(cause))).finally(() => setLoading(false));
  }, []);

  const toggle = async (skill: SkillSummary) => {
    setBusy(skill.name);
    setError(null);
    try {
      setCatalog(await desktop.setSkillEnabled(skill.name, !skill.enabled));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  };

  if (loading) return <p className="bloomery-settings-loading"><LoaderCircle size={16} className="bloomery-spin" />{t("loading")}</p>;
  return (
    <section className="bloomery-settings-extension-list" aria-labelledby="settings-skills-heading">
      <h2 id="settings-skills-heading">{t("settingsCategorySkill")}</h2>
      {error && <p role="alert">{error}</p>}
      {catalog.skills.length === 0 ? <p className="bloomery-settings-empty"><Puzzle size={16} />{t("extensionsNoSkills")}</p> : catalog.skills.map((skill) => (
        <article className="bloomery-settings-extension-row" key={`${skill.name}-${skill.source.path}`}>
          <FileCode2 size={17} aria-hidden="true" />
          <div><strong>{skill.name}</strong><span>{skill.description}</span></div>
          <label><input type="checkbox" checked={skill.enabled} disabled={busy === skill.name} onChange={() => void toggle(skill)} />{skill.enabled ? t("extensionsEnabled") : t("extensionsDisabled")}</label>
        </article>
      ))}
    </section>
  );
}
