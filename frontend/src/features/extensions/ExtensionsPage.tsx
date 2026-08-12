import { useEffect, useState } from "react";
import {
  AlertCircle,
  Check,
  FileCode2,
  FolderOpen,
  LoaderCircle,
  PackageCheck,
  PackageOpen,
  Puzzle,
  RefreshCw,
  ShieldCheck,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import {
  desktop,
  type DomainPackageRecord,
  type SkillCatalog,
  type SkillSummary,
} from "../../bridge/desktop";
import { useLocale, type MessageKey } from "../../i18n/locale";
import McpServersPanel from "./McpServersPanel";

const emptyCatalog: SkillCatalog = { skills: [], errors: [] };
const emptyPackages: DomainPackageRecord[] = [];

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function shortHash(value: string) {
  return value.length > 16 ? `${value.slice(0, 16)}...` : value;
}

export default function ExtensionsPage() {
  const { t } = useLocale();
  const [catalog, setCatalog] = useState(emptyCatalog);
  const [packages, setPackages] = useState(emptyPackages);
  const [packagePath, setPackagePath] = useState("");
  const [loading, setLoading] = useState(true);
  const [busySkill, setBusySkill] = useState<string | null>(null);
  const [busyPackage, setBusyPackage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextCatalog, nextPackages] = await Promise.all([
        desktop.listSkills(),
        desktop.listDomainPackages(),
      ]);
      setCatalog(nextCatalog);
      setPackages(nextPackages);
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsLoadError")));
    } finally {
      setLoading(false);
    }
  };

  const browsePackage = async (mode: "directory" | "zip") => {
    setError(null);
    setNotice(null);
    try {
      const selected = await desktop.openFileDialog(
        mode === "directory"
          ? { directory: true, multiple: false, title: t("extensionsDomainBrowseDirectoryTitle") }
          : {
              directory: false,
              multiple: false,
              title: t("extensionsDomainBrowseZipTitle"),
              filters: [{ name: t("extensionsDomainZipFilterName"), extensions: ["zip"] }],
            },
      );
      if (typeof selected === "string") {
        setPackagePath(selected);
      }
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsDomainBrowseError")));
    }
  };

  const installDomainPackage = async () => {
    if (!packagePath.trim()) {
      setError(t("extensionsDomainPathRequired"));
      return;
    }
    setBusyPackage("install");
    setError(null);
    setNotice(null);
    try {
      const result = await desktop.installDomainPackage(packagePath.trim());
      setPackages((current) => [...current, result.package]);
      setPackagePath("");
      setNotice(t("extensionsDomainInstalled"));
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsDomainInstallError")));
    } finally {
      setBusyPackage(null);
    }
  };

  const activateDomainPackage = async (item: DomainPackageRecord) => {
    setBusyPackage(item.id + "@" + item.version);
    setError(null);
    setNotice(null);
    try {
      const activated = await desktop.activateDomainPackage(item.id, item.version);
      setPackages((current) =>
        current.map((candidate) =>
          candidate.id === activated.id
            ? { ...candidate, active: candidate.version === activated.version }
            : candidate,
        ),
      );
      setNotice(t("extensionsDomainActivated"));
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsDomainActivateError")));
    } finally {
      setBusyPackage(null);
    }
  };

  const removeDomainPackage = async (item: DomainPackageRecord) => {
    setBusyPackage(item.id + "@" + item.version);
    setError(null);
    setNotice(null);
    try {
      const preview = await desktop.previewRemoveDomainPackage(item.id, item.version);
      if (preview.active) {
        throw new Error(t("extensionsDomainActiveRemoveError"));
      }
      if (!window.confirm(t("extensionsDomainRemoveConfirm"))) return;
      await desktop.removeDomainPackage(item.id, item.version);
      setPackages((current) =>
        current.filter(
          (candidate) => !(candidate.id === item.id && candidate.version === item.version),
        ),
      );
      setNotice(t("extensionsDomainRemoved"));
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsDomainRemoveError")));
    } finally {
      setBusyPackage(null);
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

      <McpServersPanel />

      <section className="bloomery-extensions-section" aria-labelledby="domains-heading">
        <div className="bloomery-extensions-section-heading">
          <div><p className="bloomery-eyebrow">DECLARATIVE / VERIFIED</p><h2 id="domains-heading">{t("extensionsDomainsTitle")}</h2></div>
          <PackageCheck size={19} aria-hidden="true" />
        </div>
        <p className="bloomery-extensions-copy">{t("extensionsDomainsCopy")}</p>
        <div className="bloomery-domain-install">
          <PackageOpen size={17} aria-hidden="true" />
          <input
            value={packagePath}
            onChange={(event) => setPackagePath(event.target.value)}
            placeholder={t("extensionsDomainPathPlaceholder")}
            aria-label={t("extensionsDomainPath")}
          />
          <button type="button" className="bloomery-secondary-button" onClick={() => void browsePackage("directory")} disabled={busyPackage === "install"}>
            <FolderOpen size={15} aria-hidden="true" />
            {t("extensionsDomainBrowseDirectory")}
          </button>
          <button type="button" className="bloomery-secondary-button" onClick={() => void browsePackage("zip")} disabled={busyPackage === "install"}>
            <FileCode2 size={15} aria-hidden="true" />
            {t("extensionsDomainBrowseZip")}
          </button>
          <button type="button" className="bloomery-secondary-button" onClick={() => void installDomainPackage()} disabled={busyPackage === "install"}>
            {busyPackage === "install" ? t("extensionsDomainInstalling") : t("extensionsDomainInstall")}
          </button>
        </div>
        {loading ? <div className="bloomery-extensions-loading"><LoaderCircle size={18} className="bloomery-spin" />{t("loading")}</div> : (
          <div className="bloomery-extensions-list">
            {packages.length === 0 && <div className="bloomery-extensions-empty"><PackageOpen size={18} aria-hidden="true" /><span>{t("extensionsNoDomains")}</span></div>}
            {packages.map((item) => (
              <article className={"bloomery-extension-item " + (item.active ? "is-enabled" : "")} key={item.id + "@" + item.version}>
                <div className="bloomery-extension-item-main">
                  <div className="bloomery-extension-item-heading">
                    <span className="bloomery-extension-icon"><PackageOpen size={17} aria-hidden="true" /></span>
                    <div><h3>{item.manifest.id}</h3><p>{item.manifest.author} · {item.manifest.license}</p></div>
                  </div>
                  <dl className="bloomery-extension-details">
                    <div><dt>{t("extensionsVersion")}</dt><dd>{item.version}</dd></div>
                    <div><dt>{t("extensionsDomainTrust")}</dt><dd>{item.trust === "official_signed" ? t("extensionsDomainOfficial") : t("extensionsDomainThirdParty")}</dd></div>
                    <div><dt>{t("extensionsHash")}</dt><dd title={item.package_sha256}>{shortHash(item.package_sha256)}</dd></div>
                  </dl>
                  <div className="bloomery-extension-path"><FolderOpen size={14} aria-hidden="true" /><code>{item.path}</code></div>
                </div>
                <div className="bloomery-domain-actions">
                  <span className="bloomery-domain-status">{item.active ? t("extensionsDomainActive") : t("extensionsDomainInstalled")}</span>
                  {!item.active && <button type="button" className="bloomery-icon-button" onClick={() => void activateDomainPackage(item)} disabled={busyPackage === item.id + "@" + item.version} aria-label={t("extensionsDomainActivate")} title={t("extensionsDomainActivate")}><RefreshCw size={16} aria-hidden="true" /></button>}
                  {!item.active && <button type="button" className="bloomery-icon-button" onClick={() => void removeDomainPackage(item)} disabled={busyPackage === item.id + "@" + item.version} aria-label={t("extensionsDomainRemove")} title={t("extensionsDomainRemove")}><Trash2 size={16} aria-hidden="true" /></button>}
                </div>
              </article>
            ))}
          </div>
        )}
      </section>

      <section className="bloomery-extensions-section" aria-labelledby="skills-heading">
        <div className="bloomery-extensions-section-heading">
          <div><p className="bloomery-eyebrow">LOCAL INSTRUCTION PACKS</p><h2 id="skills-heading">{t("extensionsSkillsTitle")}</h2></div>
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
                      <div><dt>{t("extensionsSource")}</dt><dd>{t("extensionsScopeUser")}</dd></div>
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
