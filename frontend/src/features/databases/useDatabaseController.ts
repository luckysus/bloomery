import { useEffect, useState } from "react";
import {
  desktop,
  type BackgroundTask,
  type DatabaseConnectionSummary,
  type DatabaseQueryResult,
  type DatabaseQuerySummary,
} from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

const POLL_INTERVAL_MS = 500;
const TERMINAL_STATES = ["completed", "failed", "cancelled", "interrupted"];
const isTerminal = (state: string) => TERMINAL_STATES.includes(state);

export const ROW_LIMITS = [100, 500, 1000, 5000];

export interface DatabaseControllerValue {
  connections: DatabaseConnectionSummary[];
  databases: string[];
  tables: string[];
  sql: string;
  rowLimit: number;
  task: BackgroundTask | null;
  result: DatabaseQueryResult | null;
  history: DatabaseQuerySummary[];
  loading: boolean;
  busy: boolean;
  sending: boolean;
  error: string | null;
  notice: string | null;
  connectionId: string;
  databaseName: string;
  connectionName: (id: string) => string;
  onConnectionChange: (id: string) => void;
  onDatabaseChange: (name: string) => void;
  onSqlChange: (value: string) => void;
  onRowLimitChange: (limit: number) => void;
  run: () => Promise<void>;
  cancel: () => Promise<void>;
  sendToAnalysis: () => Promise<void>;
  fillFromTable: (name: string) => void;
}

export function useDatabaseController(
  onOpenSection?: (section: "analysis" | "settings") => void,
): DatabaseControllerValue {
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

  const refreshHistory = () =>
    desktop
      .listDatabaseQueryResults()
      .then(setHistory)
      .catch(() => {
        /* 静默 */
      });

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
          void refreshHistory();
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
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

  return {
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
    onConnectionChange: setConnectionId,
    onDatabaseChange: setDatabaseName,
    onSqlChange: setSql,
    onRowLimitChange: setRowLimit,
    run,
    cancel,
    sendToAnalysis,
    fillFromTable,
  };
}
