import { useEffect, useState } from "react";
import { Database, ExternalLink, RefreshCw, Save } from "lucide-react";
import { desktop, type KnowledgeDatabaseConfig, type KnowledgeDatabaseHealth } from "../../bridge/desktop";

const emptyHealth: KnowledgeDatabaseHealth = {
  configured: false,
  connected: false,
  vector_extension: false,
  migration_version: null,
  message: "尚未配置 PostgreSQL 知识库",
};

const defaultConfig: KnowledgeDatabaseConfig = {
  host: "127.0.0.1",
  port: 5432,
  database: "bloomery",
  username: "postgres",
};

export default function KnowledgeDatabasePanel() {
  const [config, setConfig] = useState(defaultConfig);
  const [password, setPassword] = useState("");
  const [health, setHealth] = useState(emptyHealth);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    const getHealth = desktop.getKnowledgeDatabaseHealth;
    if (typeof getHealth !== "function") return;
    void getHealth().then(setHealth).catch((cause) => setError(String(cause)));
  };

  useEffect(refresh, []);

  const update = (key: keyof KnowledgeDatabaseConfig, value: string) => {
    setConfig((current) => ({ ...current, [key]: key === "port" ? Number(value) || 0 : value }));
  };

  const test = async () => {
    setBusy(true);
    setError(null);
    try {
      setHealth(await desktop.testKnowledgeDatabase(config, password));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const configure = async () => {
    setBusy(true);
    setError(null);
    try {
      setHealth(await desktop.configureKnowledgeDatabase(config, password));
      setPassword("");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const initialize = async () => {
    setBusy(true);
    setError(null);
    try {
      setHealth(await desktop.initializeKnowledgeDatabase());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="bloomery-settings-databases" aria-busy={busy}>
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="flex items-center gap-2"><Database size={18} aria-hidden="true" /> PostgreSQL 知识库</h2>
          <p>知识库使用 PostgreSQL；客户端设置和会话仍保存在本地。</p>
        </div>
        <button type="button" onClick={refresh} disabled={busy} title="刷新状态" aria-label="刷新状态"><RefreshCw size={16} /></button>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <label>地址<input value={config.host} onChange={(event) => update("host", event.target.value)} /></label>
        <label>端口<input type="number" min="1" max="65535" value={config.port} onChange={(event) => update("port", event.target.value)} /></label>
        <label>数据库<input value={config.database} onChange={(event) => update("database", event.target.value)} /></label>
        <label>用户名<input value={config.username} onChange={(event) => update("username", event.target.value)} /></label>
        <label className="md:col-span-2">密码<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="off" /></label>
      </div>
      <div className="flex flex-wrap gap-2">
        <button type="button" onClick={() => void test()} disabled={busy}><RefreshCw size={15} />测试连接</button>
        <button type="button" onClick={() => void configure()} disabled={busy}><Save size={15} />保存配置</button>
        <button type="button" onClick={() => void initialize()} disabled={busy || !health.configured}><Database size={15} />初始化知识库</button>
        <a href="https://www.postgresql.org/download/windows/" target="_blank" rel="noreferrer">安装 PostgreSQL <ExternalLink size={14} /></a>
      </div>
      <p role="status">{health.message}{health.migration_version ? `，迁移版本 ${health.migration_version}` : ""}</p>
      {!health.vector_extension && health.connected && <p>需要安装并启用 pgvector 后才能初始化 RAG。</p>}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
