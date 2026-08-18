import { useEffect, useState, type FormEvent } from "react";
import {
  Check,
  CircleAlert,
  CircleCheck,
  CircleX,
  Database,
  Pencil,
  Save,
  Table2,
  Trash2,
  PlugZap,
} from "lucide-react";
import {
  desktop,
  type DatabaseConnectionInput,
  type DatabaseConnectionSummary,
} from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

type Draft = {
  id: string | null;
  display_name: string;
  host: string;
  port: string;
  username: string;
  password: string;
};

const emptyDraft = (): Draft => ({
  id: null,
  display_name: "",
  host: "",
  port: "1433",
  username: "",
  password: "",
});

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function updateConnection(connections: DatabaseConnectionSummary[], next: DatabaseConnectionSummary) {
  const index = connections.findIndex((item) => item.id === next.id);
  if (index < 0) return [...connections, next];
  return connections.map((item) => (item.id === next.id ? next : item));
}

export default function DatabaseConnectionsPanel() {
  const { t } = useLocale();
  const [connections, setConnections] = useState<DatabaseConnectionSummary[]>([]);
  const [draft, setDraft] = useState<Draft>(emptyDraft);
  const [busy, setBusy] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, { version: string | null; error: string | null }>>({});
  const [tables, setTables] = useState<Record<string, string[]>>({});

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      setConnections(await desktop.listDatabaseConnections());
    } catch (cause) {
      setError(errorMessage(cause, t("settingsDatabaseLoadError")));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const save = async (event: FormEvent) => {
    event.preventDefault();
    setBusy("save");
    setError(null);
    setNotice(null);
    try {
      const payload: DatabaseConnectionInput = {
        id: draft.id,
        display_name: draft.display_name,
        host: draft.host,
        port: Number(draft.port) || undefined,
        username: draft.username,
        password: draft.password || undefined,
      };
      const saved = await desktop.saveDatabaseConnection(payload);
      setConnections((current) => updateConnection(current, saved));
      setDraft(emptyDraft());
      setNotice(t("settingsDatabaseSaved"));
    } catch (cause) {
      setError(errorMessage(cause, t("settingsDatabaseSaveError")));
    } finally {
      setBusy(null);
    }
  };

  const edit = (connection: DatabaseConnectionSummary) => {
    setDraft({
      id: connection.id,
      display_name: connection.display_name,
      host: connection.host,
      port: String(connection.port),
      username: connection.username,
      password: "",
    });
    setNotice(t("settingsDatabaseEditing"));
  };

  const remove = async (connection: DatabaseConnectionSummary) => {
    if (!window.confirm(t("settingsDatabaseDeleteConfirm"))) return;
    setBusy(`delete:${connection.id}`);
    setError(null);
    try {
      await desktop.deleteDatabaseConnection(connection.id);
      setConnections((current) => current.filter((item) => item.id !== connection.id));
      setNotice(t("settingsDatabaseDeleted"));
    } catch (cause) {
      setError(errorMessage(cause, t("settingsDatabaseDeleteError")));
    } finally {
      setBusy(null);
    }
  };

  const test = async (connection: DatabaseConnectionSummary) => {
    setBusy(`test:${connection.id}`);
    setError(null);
    try {
      const version = await desktop.testDatabaseConnection(connection.id);
      setTestResults((current) => ({
        ...current,
        [connection.id]: { version, error: null },
      }));
      setNotice(t("settingsDatabaseTested"));
    } catch (cause) {
      setTestResults((current) => ({
        ...current,
        [connection.id]: { version: null, error: errorMessage(cause, t("settingsDatabaseTestFailed")) },
      }));
    } finally {
      setBusy(null);
    }
  };

  const listTables = async (connection: DatabaseConnectionSummary) => {
    setBusy(`tables:${connection.id}`);
    setError(null);
    try {
      const names = await desktop.listDatabaseTables(connection.id);
      setTables((current) => ({ ...current, [connection.id]: names }));
    } catch (cause) {
      setError(errorMessage(cause, t("settingsDatabaseTablesError")));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="bloomery-settings-databases" aria-labelledby="settings-databases-heading" aria-busy={loading}>
      <div className="bloomery-settings-permissions-heading">
        <div><h2 id="settings-databases-heading">{t("settingsDatabaseTitle")}</h2></div>
        <Database size={21} aria-hidden="true" />
      </div>
      {error && <div className="bloomery-settings-alert" role="alert"><CircleAlert size={17} aria-hidden="true" /><span>{error}</span></div>}
      {notice && <div className="bloomery-settings-notice" role="status"><Check size={17} aria-hidden="true" /><span>{notice}</span></div>}

      <form className="bloomery-mcp-form" onSubmit={(event) => void save(event)}>
        <div className="bloomery-mcp-form-heading"><strong>{draft.id ? t("settingsDatabaseEdit") : t("settingsDatabaseAdd")}</strong><span>{t("settingsDatabaseSecretNote")}</span></div>
        <div className="bloomery-mcp-fields">
          <label><span>{t("settingsDatabaseDisplayName")}</span><input value={draft.display_name} onChange={(event) => setDraft({ ...draft, display_name: event.target.value })} required /></label>
          <label><span>{t("settingsDatabaseHost")}</span><input value={draft.host} onChange={(event) => setDraft({ ...draft, host: event.target.value })} required /></label>
          <label><span>{t("settingsDatabasePort")}</span><input type="number" min="1" max="65535" value={draft.port} onChange={(event) => setDraft({ ...draft, port: event.target.value })} required /></label>
          <label><span>{t("settingsDatabaseUsername")}</span><input value={draft.username} onChange={(event) => setDraft({ ...draft, username: event.target.value })} required autoComplete="off" /></label>
          <label><span>{t("settingsDatabasePassword")}</span><input type="password" value={draft.password} onChange={(event) => setDraft({ ...draft, password: event.target.value })} placeholder={draft.id ? t("settingsDatabasePasswordPlaceholder") : undefined} autoComplete="new-password" /></label>
        </div>
        <div className="bloomery-mcp-form-actions"><button type="submit" className="bloomery-secondary-button" disabled={busy === "save"}><Save size={15} aria-hidden="true" />{busy === "save" ? t("settingsDatabaseSaving") : t("settingsDatabaseSave")}</button>{draft.id && <button type="button" className="bloomery-icon-button" onClick={() => setDraft(emptyDraft())} aria-label={t("settingsDatabaseCancelEdit")} title={t("settingsDatabaseCancelEdit")}><CircleX size={17} aria-hidden="true" /></button>}</div>
      </form>

      {connections.length === 0 ? <div className="bloomery-extensions-empty"><Database size={18} aria-hidden="true" /><span>{t("settingsDatabaseEmpty")}</span></div> : <div className="bloomery-mcp-list">
        {connections.map((connection) => {
          const result = testResults[connection.id];
          const connectionTables = tables[connection.id] ?? [];
          return <article className="bloomery-mcp-item" key={connection.id}>
            <div className="bloomery-mcp-item-main">
              <div className="bloomery-extension-item-heading"><span className="bloomery-extension-icon"><Database size={17} aria-hidden="true" /></span><div><h3>{connection.display_name}</h3><p>{connection.host}:{connection.port} · {connection.username}</p></div></div>
              {result?.version && <p className="bloomery-mcp-health is-healthy"><CircleCheck size={14} aria-hidden="true" />{result.version.split("\n")[0]}</p>}
              {result?.error && <p className="bloomery-mcp-error">{result.error}</p>}
              {connectionTables.length > 0 && <details className="bloomery-mcp-tools"><summary>{t("settingsDatabaseTables")} ({connectionTables.length})</summary>{connectionTables.map((name) => <div key={name}><code>{name}</code></div>)}</details>}
            </div>
            <div className="bloomery-mcp-actions">
              <button type="button" className="bloomery-icon-button" onClick={() => void test(connection)} disabled={busy !== null} aria-label={t("settingsDatabaseTest")} title={t("settingsDatabaseTest")}><PlugZap size={16} aria-hidden="true" /></button>
              <button type="button" className="bloomery-icon-button" onClick={() => void listTables(connection)} disabled={busy !== null} aria-label={t("settingsDatabaseTables")} title={t("settingsDatabaseTables")}><Table2 size={16} aria-hidden="true" /></button>
              <button type="button" className="bloomery-icon-button" onClick={() => edit(connection)} disabled={busy !== null} aria-label={t("settingsDatabaseEdit")} title={t("settingsDatabaseEdit")}><Pencil size={16} aria-hidden="true" /></button>
              <button type="button" className="bloomery-icon-button" onClick={() => void remove(connection)} disabled={busy !== null} aria-label={t("settingsDatabaseDelete")} title={t("settingsDatabaseDelete")}><Trash2 size={16} aria-hidden="true" /></button>
            </div>
          </article>;
        })}
      </div>}
    </section>
  );
}
