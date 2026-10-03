import { useEffect, useState } from "react";
import { Check, Database, ExternalLink, LoaderCircle, RefreshCw, Save, Unplug } from "lucide-react";
import { desktop, isDesktopRuntime, type KnowledgeDatabaseConfig, type KnowledgeDatabaseHealth } from "../../bridge/desktop";
import { settingsErrorMessage } from "./settingsError";

const emptyHealth: KnowledgeDatabaseHealth = {
  configured: false,
  config: null,
  connected: false,
  vector_extension: false,
  migration_version: null,
  message: "尚未配置 PostgreSQL 知识库",
};

const defaultConfig: KnowledgeDatabaseConfig = {
  host: "",
  port: 5432,
  database: "",
  username: "",
  ssl: false,
  vector_store: "postgresql_pgvector",
  embedding_model: "BAAI/bge-m3",
  vector_dimension: 1024,
  index_type: "hnsw",
};

function applyHealth(
  next: KnowledgeDatabaseHealth,
  setHealth: (value: KnowledgeDatabaseHealth) => void,
  setConfig: (value: KnowledgeDatabaseConfig) => void,
) {
  setHealth(next);
  if (next.config) setConfig(next.config);
}

export default function KnowledgeDatabasePanel() {
  const [config, setConfig] = useState(defaultConfig);
  const [password, setPassword] = useState("");
  const [health, setHealth] = useState(emptyHealth);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = () => {
    setLoading(true);
    setError(null);
    setNotice(null);
    if (!isDesktopRuntime()) {
      setLoading(false);
      return;
    }
    const getHealth = desktop.getKnowledgeDatabaseHealth;
    if (typeof getHealth !== "function") {
      setLoading(false);
      setError("当前桌面运行时不支持知识库状态检查");
      return;
    }
    void getHealth()
      .then((next) => applyHealth(next, setHealth, setConfig))
      .catch((cause) => setError(settingsErrorMessage(cause, "无法读取 PostgreSQL 知识库状态")))
      .finally(() => setLoading(false));
  };

  useEffect(refresh, []);

  const update = (key: keyof KnowledgeDatabaseConfig, value: string) => {
    setConfig((current) => ({ ...current, [key]: key === "port" || key === "vector_dimension" ? Number(value) || 0 : key === "ssl" ? value === "true" : value }));
  };

  const test = async () => {
    const validation = validateConfig(config);
    if (validation) { setError(validation); return; }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      applyHealth(await desktop.testKnowledgeDatabase(config, password), setHealth, setConfig);
      setNotice("连接测试完成");
    } catch (cause) {
      setError(settingsErrorMessage(cause, "PostgreSQL 连接测试失败"));
    } finally {
      setBusy(false);
    }
  };

  const configure = async () => {
    const validation = validateConfig(config);
    if (validation) { setError(validation); return; }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      applyHealth(await desktop.configureKnowledgeDatabase(config, password), setHealth, setConfig);
      setPassword("");
      setNotice("知识库连接配置已保存");
    } catch (cause) {
      setError(settingsErrorMessage(cause, "知识库连接配置保存失败"));
    } finally {
      setBusy(false);
    }
  };

  const initialize = async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      applyHealth(await desktop.initializeKnowledgeDatabase(), setHealth, setConfig);
      setNotice("知识库初始化完成");
    } catch (cause) {
      setError(settingsErrorMessage(cause, "知识库初始化失败，请检查连接和 pgvector"));
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    if (typeof desktop.disconnectKnowledgeDatabase !== "function") return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await desktop.disconnectKnowledgeDatabase();
      setHealth({ ...emptyHealth, message: "已断开 PostgreSQL 知识库" });
      setPassword("");
      setNotice("知识库已断开");
    } catch (cause) {
      setError(settingsErrorMessage(cause, "断开知识库失败"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="suna-settings-databases" aria-busy={loading || busy}>
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="flex items-center gap-2"><Database size={18} aria-hidden="true" /> PostgreSQL 知识库</h2>
          <p>知识库使用 PostgreSQL；客户端设置和会话仍保存在本地。</p>
        </div>
        <button type="button" onClick={refresh} disabled={loading || busy} title="刷新状态" aria-label="刷新状态">{loading ? <LoaderCircle size={16} className="suna-spin" /> : <RefreshCw size={16} />}</button>
      </div>
      {loading && <p className="suna-settings-loading" role="status"><LoaderCircle size={15} className="suna-spin" />正在读取知识库状态...</p>}
      <div className="grid gap-3 md:grid-cols-2">
        <label>地址<input value={config.host} placeholder="127.0.0.1" onChange={(event) => update("host", event.target.value)} /></label>
        <label>端口<input type="number" min="1" max="65535" value={config.port} onChange={(event) => update("port", event.target.value)} /></label>
        <label>数据库<input value={config.database} placeholder="suna" onChange={(event) => update("database", event.target.value)} /></label>
        <label>用户名<input value={config.username} placeholder="postgres" onChange={(event) => update("username", event.target.value)} /></label>
        <label className="md:col-span-2">密码<input type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="off" /></label>
        <label className="flex items-center gap-2"><input type="checkbox" checked={config.ssl} onChange={(event) => update("ssl", String(event.target.checked))} />启用 SSL</label>
        <label>Vector Store<select value={config.vector_store} onChange={(event) => update("vector_store", event.target.value)}><option value="postgresql_pgvector">PostgreSQL + pgvector</option></select></label>
        <label>Embedding 模型<select value={config.embedding_model} onChange={(event) => update("embedding_model", event.target.value)}><option value="BAAI/bge-m3">BGE-M3</option><option value="text-embedding-3-small">text-embedding-3-small</option></select></label>
        <label>向量维度<input type="number" min="1" max="65536" value={config.vector_dimension} onChange={(event) => update("vector_dimension", event.target.value)} /></label>
        <label>向量索引<select value={config.index_type} onChange={(event) => update("index_type", event.target.value)}><option value="hnsw">HNSW</option><option value="ivfflat">IVFFlat</option></select></label>
      </div>
      <div className="flex flex-wrap gap-2">
        <button type="button" onClick={() => void test()} disabled={loading || busy}><RefreshCw size={15} />测试连接</button>
        <button type="button" onClick={() => void configure()} disabled={loading || busy}><Save size={15} />保存配置</button>
        <button type="button" onClick={() => void initialize()} disabled={loading || busy || !health.configured}><Database size={15} />初始化知识库</button>
        <button type="button" onClick={() => void disconnect()} disabled={loading || busy || !health.configured}><Unplug size={15} />断开知识库</button>
        <a href="https://www.postgresql.org/download/windows/" target="_blank" rel="noreferrer">安装 PostgreSQL <ExternalLink size={14} /></a>
      </div>
      {notice && <p role="status" className="suna-settings-notice"><Check size={14} />{notice}</p>}
      <p role="status">{health.message}{health.migration_version ? `，迁移版本 ${health.migration_version}` : ""}</p>
      {!health.vector_extension && health.connected && <p>需要安装并启用 pgvector 后才能初始化 RAG。</p>}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}

function validateConfig(config: KnowledgeDatabaseConfig): string | null {
  if (!config.host.trim() || !config.database.trim() || !config.username.trim()) return "地址、数据库和用户名不能为空";
  if (!Number.isInteger(config.port) || config.port < 1 || config.port > 65535) return "端口必须是 1 到 65535 之间的整数";
  if (!Number.isInteger(config.vector_dimension) || config.vector_dimension < 1 || config.vector_dimension > 65536) return "向量维度必须是 1 到 65536 之间的整数";
  if (config.index_type !== "hnsw" && config.index_type !== "ivfflat") return "向量索引类型无效";
  return null;
}
