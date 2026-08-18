import { Database } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import { ROW_LIMITS, useDatabaseController } from "./useDatabaseController";

const isRunning = (state?: string) =>
  state !== undefined && !["completed", "failed", "cancelled", "interrupted"].includes(state);

export default function DatabasePage({
  onOpenSection,
}: {
  onOpenSection?: (section: "analysis" | "settings") => void;
}) {
  const { t } = useLocale();
  const {
    connections,
    databases,
    tables,
    sql,
    rowLimit,
    task,
    result,
    history,
    loading,
    busy,
    sending,
    error,
    notice,
    connectionId,
    databaseName,
    connectionName,
    onConnectionChange,
    onDatabaseChange,
    onSqlChange,
    onRowLimitChange,
    run,
    cancel,
    sendToAnalysis,
    fillFromTable,
  } = useDatabaseController(onOpenSection);

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
                  onChange={(event) => onConnectionChange(event.target.value)}
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
                  onChange={(event) => onDatabaseChange(event.target.value)}
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
                  onChange={(event) => onSqlChange(event.target.value)}
                  spellCheck={false}
                />
              </label>
              <div className="bloomery-db-actions">
                <label>
                  <span>{t("dbRowLimit")}</span>
                  <select
                    aria-label={t("dbRowLimit")}
                    value={rowLimit}
                    onChange={(event) => onRowLimitChange(Number(event.target.value))}
                  >
                    {ROW_LIMITS.map((limit) => (
                      <option key={limit} value={limit}>
                        {limit}
                      </option>
                    ))}
                  </select>
                </label>
                {task && isRunning(task.state) ? (
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
                {task && isRunning(task.state) && (
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
                  onClick={() => onSqlChange(item.query_text)}
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
