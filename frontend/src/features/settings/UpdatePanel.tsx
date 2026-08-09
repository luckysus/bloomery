import { useState } from "react";
import { AlertCircle, Check, Download, RefreshCw } from "lucide-react";
import { desktop, type UpdateInfo } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

export default function UpdatePanel() {
  const { t } = useLocale();
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const checkForUpdate = async () => {
    setChecking(true);
    setError(false);
    setNotice(null);
    try {
      const next = await desktop.checkForUpdate();
      setUpdate(next);
      if (!next) setNotice(t("updateUpToDate"));
    } catch {
      setUpdate(null);
      setError(true);
    } finally {
      setChecking(false);
    }
  };

  const installUpdate = async () => {
    setInstalling(true);
    setError(false);
    try {
      await desktop.installUpdate();
    } catch {
      setError(true);
      setInstalling(false);
    }
  };

  return (
    <section className="bloomery-settings-update" aria-labelledby="settings-update-heading">
      <div className="bloomery-settings-update-heading">
        <div>
          <p className="bloomery-eyebrow">RELEASE CHANNEL / STABLE</p>
          <h2 id="settings-update-heading">{t("updateTitle")}</h2>
          <p>{t("updateCopy")}</p>
        </div>
        <button type="button" className="bloomery-icon-button" onClick={() => void checkForUpdate()} disabled={checking || installing} aria-label={t("updateCheck")} title={t("updateCheck")}>
          <RefreshCw size={17} className={checking ? "bloomery-spin" : undefined} aria-hidden="true" />
        </button>
      </div>
      {error && <div className="bloomery-settings-update-message is-error" role="alert"><AlertCircle size={16} aria-hidden="true" />{t("updateCheckError")}</div>}
      {notice && <div className="bloomery-settings-update-message is-success" role="status"><Check size={16} aria-hidden="true" />{notice}</div>}
      {update && (
        <div className="bloomery-settings-update-available">
          <div><strong>{t("updateAvailable")}</strong><span>{update.version}</span></div>
          {update.body && <p>{update.body}</p>}
          <button type="button" className="bloomery-action-primary" onClick={() => void installUpdate()} disabled={installing}>
            <Download size={16} aria-hidden="true" />{installing ? t("updateInstalling") : t("updateInstall")}
          </button>
        </div>
      )}
    </section>
  );
}
