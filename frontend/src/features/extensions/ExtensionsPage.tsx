import { useEffect, useState } from "react";
import {
  AlertCircle,
  Check,
  FileCode2,
  FolderOpen,
  LoaderCircle,
  Puzzle,
  RefreshCw,
  ShieldCheck,
  TriangleAlert,
} from "lucide-react";
import { desktop, type SkillCatalog, type SkillScope, type SkillSummary } from "../../bridge/desktop";
import { useLocale, type MessageKey } from "../../i18n/locale";

const emptyCatalog: SkillCatalog = { skills: [], errors: [] };

const scopeKeys: Record<SkillScope, MessageKey> = {
  user: "extensionsScopeUser",
  workspace: "extensionsScopeWorkspace",
  domain: "extensionsScopeDomain",
};

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function shortHash(value: string) {
  return value.length > 16 ? `${value.slice(0, 16)}...` : value;
}

export default function ExtensionsPage() {
  const { t } = useLocale();
  const [catalog, setCatalog] = useState(emptyCatalog);
  const [loading, setLoading] = useState(true);
  const [busySkill, setBusySkill] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      setCatalog(await desktop.listSkills());
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsLoadError")));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const toggleSkill = async (skill: SkillSummary) => {
    setBusySkill(skill.name);
    setError(null);
    setNotice(null);
    try {
      setCatalog(await desktop.setSkillEnabled(skill.name, !skill.enabled));
      setNotice(t("extensionsSaved"));
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsSaveError")));
    } finally {
      setBusySkill(null);
    }
  };

  return (
    <section className="bloomery-extensions" aria-labelledby="extensions-heading" aria-busy={loading}>
      <header className="bloomery-extensions-header">
        <div>
          <p className="bloomery-eyebrow">LOCAL RUNTIME / EXTENSIONS</p>
          <h1 id="extensions-heading">{t("extensionsTitle")}</h1>
          <p className="bloomery-lede">{t("extensionsLede")}</p>
        </div>
        <button type="button" className="bloomery-icon-button" onClick={() => void load()} disabled={loading} aria-label={t("extensionsRefresh")} title={t("extensionsRefresh")}>
          <RefreshCw size={18} aria-hidden="true" />
        </button>
      </header>

      {error && <div className="bloomery-extensions-alert" role="alert"><AlertCircle size={17} aria-hidden="true" /><span>{error}</span></div>}
      {notice && <div className="bloomery-extensions-notice" role="status"><Check size={17} aria-hidden="true" /><span>{notice}</span></div>}

      <section className="bloomery-extensions-section" aria-labelledby="skills-heading">
        <div className="bloomery-extensions-section-heading">
          <div><p className="bloomery-eyebrow">CLAUDE COMPATIBLE</p><h2 id="skills-heading">{t("extensionsSkillsTitle")}</h2></div>
          <ShieldCheck size={19} aria-hidden="true" />
        </div>
        <p className="bloomery-extensions-copy">{t("extensionsSkillsCopy")}</p>

        {loading ? <div className="bloomery-extensions-loading"><LoaderCircle size={18} className="bloomery-spin" />{t("loading")}</div> : (
          <>
            {catalog.skills.length === 0 && <div className="bloomery-extensions-empty"><Puzzle size={18} aria-hidden="true" /><span>{t("extensionsNoSkills")}</span></div>}
            <div className="bloomery-extensions-list">
              {catalog.skills.map((skill) => (
                <article className={`bloomery-extension-item ${skill.enabled ? "is-enabled" : ""}`} key={`${skill.name}-${skill.source.path}`}>
                  <div className="bloomery-extension-item-main">
                    <div className="bloomery-extension-item-heading">
                      <span className="bloomery-extension-icon"><FileCode2 size={17} aria-hidden="true" /></span>
                      <div><h3>{skill.name}</h3><p>{skill.description}</p></div>
                    </div>
                    <dl className="bloomery-extension-details">
                      <div><dt>{t("extensionsVersion")}</dt><dd>{skill.version}</dd></div>
                      <div><dt>{t("extensionsSource")}</dt><dd>{t(scopeKeys[skill.source.scope])}</dd></div>
                      <div><dt>{t("extensionsHash")}</dt><dd title={skill.content_sha256}>{shortHash(skill.content_sha256)}</dd></div>
                    </dl>
                    <div className="bloomery-extension-path"><FolderOpen size={14} aria-hidden="true" /><code>{skill.source.path}</code></div>
                  </div>
                  <label className="bloomery-extension-toggle">
                    <input
                      type="checkbox"
                      checked={skill.enabled}
                      disabled={busySkill === skill.name}
                      onChange={() => void toggleSkill(skill)}
                      aria-label={`${t(skill.enabled ? "extensionsDisable" : "extensionsEnable")} ${skill.name}`}
                    />
                    <span>{skill.enabled ? t("extensionsEnabled") : t("extensionsDisabled")}</span>
                  </label>
                </article>
              ))}
            </div>
          </>
        )}
      </section>

      {catalog.errors.length > 0 && <section className="bloomery-extensions-errors" aria-labelledby="extensions-errors-heading">
        <div className="bloomery-extensions-section-heading"><div><p className="bloomery-eyebrow">ISOLATED LOAD ERRORS</p><h2 id="extensions-errors-heading">{t("extensionsErrorsTitle")}</h2></div><TriangleAlert size={19} aria-hidden="true" /></div>
        <p className="bloomery-extensions-copy">{t("extensionsErrorsCopy")}</p>
        <div className="bloomery-extensions-error-list">
          {catalog.errors.map((item, index) => <div className="bloomery-extension-error" key={`${item.path}-${index}`}><strong>{item.code}</strong><span>{item.message}</span><code>{item.path}</code></div>)}
        </div>
      </section>}
    </section>
  );
}
