import { useEffect, useState } from "react";
import { Database } from "lucide-react";
import {
  desktop,
  type BackgroundTask,
  type DatabaseConnectionSummary,
  type DatabaseQueryResult,
  type DatabaseQuerySummary,
} from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

const POLL_INTERVAL_MS = 500;
const ROW_LIMITS = [100, 500, 1000, 5000];
const TERMINAL_STATES = ["completed", "failed", "cancelled", "interrupted"];
const isTerminal = (state: string) => TERMINAL_STATES.includes(state);

export default function DatabasePage({
  onOpenSection,
}: {
  onOpenSection?: (section: "analysis" | "settings") => void;
}) {
  const { t } = useLocale();
  const [connections, setConnections] = useState<DatabaseConnectionSummary[]>([]);
  const [connectionId, setConnectionId] = useState("");
  const [databases, setDatabases] = useState<string[]>([]);
  const [databaseName, setDatabaseName] = useState("");
  const [tables, setTables] = useState<string[]>([]);
  const [sql, setSql] = useState("");
  const [rowLimit, setRowLimit] = useState(500);
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [result, setResult] = useState<DatabaseQueryResult | null>(null);
  const [history, setHistory] = useState<DatabaseQuerySummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const connectionName = (id: string) =>
    connections.find((item) => item.id === id)?.display_name ?? id;

  useEffect(() => {
    let mounted = true;
    desktop
      .listDatabaseConnections()
      .then((items) => {
        if (!mounted) return;
        const enabled = items.filter((item) => item.enabled && item.secret_configured);
        setConnections(enabled);
        setConnectionId(enabled[0]?.id ?? "");
      })
      .catch(() => mounted && setError(t("dbLoadError")))
      .finally(() => mounted && setLoading(false));
    return () => {
      mounted = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!connectionId) return;
    let mounted = true;
    setError(null);
    Promise.all([desktop.listDatabases(connectionId), desktop.listDatabaseTables(connectionId)])
      .then(([names, tableNames]) => {
        if (!mounted) return;
        setDatabases(names);
        setTables(tableNames);
      })
      .catch((cause) => mounted && setError(cause instanceof Error ? cause.message : t("dbLoadError")));
    return () => {
      mounted = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connectionId]);

  useEffect(() => {
    let mounted = true;
    desktop
      .listDatabaseQueryResults()
      .then((items) => mounted && setHistory(items))
      .catch(() => {
        /* 历史加载失败不打断主流程 */
      });
    return () => {
      mounted = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = async () => {
    if (!connectionId || busy || !sql.trim()) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const queued = await desktop.submitDatabaseQuery({
        connection_id: connectionId,
        database: databaseName || null,
        sql,
        row_limit: rowLimit,
      });
      setTask(queued);
      setResult(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("dbQueryFailed"));
      setTask(null);
    } finally {
      setBusy(false);
    }
  };

  const cancel = async () => {
    if (!task) return;
    try {
      await desktop.cancelBackgroundTask(task.id);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("dbQueryFailed"));
    }
  };

  useEffect(() => {
    if (!task || isTerminal(task.state)) return;
    let mounted = true;
    const refresh = async () => {
      try {
        const tasks = await desktop.listBackgroundTasks();
        const current = tasks.find((candidate) => candidate.id === task.id);
        if (!mounted || !current) return;
        if (current.state === "completed") {
          const next = await desktop.getDatabaseQueryResult(current.id);
          if (!mounted) return;
          setTask(current);
          setResult(next);
          void desktop
            .listDatabaseQueryResults()
            .then((items) => mounted && setHistory(items))
            .catch(() => {
              /* 静默 */
            });
        } else {
          setTask(current);
        }
      } catch {
        /* 轮询失败下次重试 */
      }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), POLL_INTERVAL_MS);
    return () => {
      mounted = false;
      window.clearInterval(timer);
    };
  }, [task]);

  const sendToAnalysis = async () => {
    if (!result || sending) return;
    setSending(true);
    setError(null);
    setNotice(null);
    try {
      const saved = await desktop.saveSteelDataset({ sourcePath: result.csv_path });
      await desktop.activateSteelDataset(saved.id);
      setNotice(t("dbSent"));
      onOpenSection?.("analysis");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t("dbSendError"));
    } finally {
      setSending(false);
    }
  };

  const fillFromTable = (name: string) => {
    setSql(`SELECT TOP (${rowLimit}) * FROM [${name.replace(".", "].[")}]`);
  };

  return (
    <div className="bloomery-db bloomery-page-surface">
      <header className="bloomery-db-header">
        <div>
          <h1 id="db-heading">{t("dbTitle")}</h1>
        </div>
      </header>

      {error && (
        <div className="bloomery-settings-alert" role="alert">
          <span>{error}</span>
        </div>
      )}
      {notice && (
        <div className="bloomery-settings-notice" role="status">
          <span>{notice}</span>
        </div>
      )}

      {loading ? null : connections.length === 0 ? (
        <div className="bloomery-extensions-empty">
          <Database size={18} aria-hidden="true" />
          <span>{t("dbEmptyConnections")}</span>
        </div>
      ) : (
        <div className="bloomery-db-body">
          <aside className="bloomery-db-tables" aria-label={t("dbTables")}>
            <h2>{t("dbTables")}</h2>
            {tables.map((name) => (
              <button
                key={name}
                type="button"
                className="bloomery-db-table-button"
                onClick={() => fillFromTable(name)}
                title={name}
              >
                <code>{name}</code>
              </button>
            ))}
          </aside>
          <div className="bloomery-db-main">
            <div className="bloomery-db-toolbar">
              <label>
                <span>{t("dbConnectionLabel")}</span>
                <select
                  aria-label={t("dbConnectionLabel")}
                  value={connectionId}
                  onChange={(event) => setConnectionId(event.target.value)}
                >
                  {connections.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.display_name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>{t("dbDatabaseLabel")}</span>
                <select
                  aria-label={t("dbDatabaseLabel")}
                  value={databaseName}
                  onChange={(event) => setDatabaseName(event.target.value)}
                >
                  <option value="">{t("dbDatabaseLabel")}</option>
                  {databases.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            <div className="bloomery-db-editor">
              <label>
                <span>{t("dbSqlLabel")}</span>
                <textarea
                  aria-label={t("dbSqlLabel")}
                  rows={6}
                  className="bloomery-db-sql-input"
                  value={sql}
                  onChange={(event) => setSql(event.target.value)}
                  spellCheck={false}
                />
              </label>
              <div className="bloomery-db-actions">
                <label>
                  <span>{t("dbRowLimit")}</span>
                  <select
                    aria-label={t("dbRowLimit")}
                    value={rowLimit}
                    onChange={(event) => setRowLimit(Number(event.target.value))}
                  >
                    {ROW_LIMITS.map((limit) => (
                      <option key={limit} value={limit}>
                        {limit}
                      </option>
                    ))}
                  </select>
                </label>
                {task && !isTerminal(task.state) ? (
                  <button
                    type="button"
                    className="bloomery-action-secondary"
                    onClick={() => void cancel()}
                    aria-label={t("dbCancel")}
                  >
                    {t("dbCancel")}
                  </button>
                ) : (
                  <button
                    type="button"
                    className="bloomery-action-primary"
                    onClick={() => void run()}
                    disabled={busy}
                    aria-label={t("dbRun")}
                  >
                    {t("dbRun")}
                  </button>
                )}
                {task && !isTerminal(task.state) && (
                  <span className="bloomery-db-running" aria-live="polite">
                    {t("dbRunning")}
                  </span>
                )}
              </div>
            </div>

            <section className="bloomery-db-result-section" aria-label={t("dbResultsTitle")}>
              {result ? (
                <>
                  <div className="bloomery-db-result-meta">
                    <span>{t("dbDuration", { ms: result.duration_ms })}</span>
                    {result.truncated && (
                      <span className="bloomery-db-truncated" role="status">
                        {t("dbTruncatedNotice", { count: result.row_count })}
                      </span>
                    )}
                    <button
                      type="button"
                      className="bloomery-action-secondary"
                      onClick={() => void sendToAnalysis()}
                      disabled={sending}
                      aria-label={t("dbSendToAnalysis")}
                    >
                      {sending ? t("dbSending") : t("dbSendToAnalysis")}
                    </button>
                  </div>
                  <div className="bloomery-db-result">
                    <table>
                      <thead>
                        <tr>
                          {result.columns.map((column) => (
                            <th key={column} scope="col">
                              {column}
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {result.rows.map((row, index) => (
                          <tr key={index}>
                            {row.map((cell, cellIndex) => (
                              <td key={cellIndex}>{cell ?? ""}</td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </>
              ) : (
                <div className="bloomery-extensions-empty">
                  <span>{t("dbResultEmpty")}</span>
                </div>
              )}
            </section>

            <aside className="bloomery-db-history" aria-label={t("dbHistory")}>
              <h2>{t("dbHistory")}</h2>
              {history.map((item) => (
                <button
                  key={item.task_id}
                  type="button"
                  className="bloomery-db-table-button"
                  title={item.query_text}
                  onClick={() => setSql(item.query_text)}
                >
                  <code>{item.query_text}</code>
                  <span className="bloomery-db-history-meta">
                    {item.database_name || connectionName(connectionId)} · {item.row_count}
                  </span>
                </button>
              ))}
            </aside>
          </div>
        </div>
      )}
    </div>
  );
}
