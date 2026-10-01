import { Database, FolderOpen, HardDrive, LoaderCircle, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { desktop, type StorageHealth, type StoragePaths } from "../../bridge/desktop";

const emptyHealth: StorageHealth = {
  database_ok: false,
  current_migration_version: 0,
  latest_migration_version: 0,
  database_size_bytes: 0,
  reclaimable_bytes: 0,
  available_disk_bytes: null,
};

function formatBytes(value: number | null) {
  if (value == null) return "未知";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let amount = value;
  let unit = "B";
  for (const candidate of units) {
    amount /= 1024;
    unit = candidate;
    if (amount < 1024) break;
  }
  return `${amount.toFixed(amount >= 10 ? 0 : 1)} ${unit}`;
}

export default function StorageDataPanel() {
  const [paths, setPaths] = useState<StoragePaths | null>(null);
  const [health, setHealth] = useState(emptyHealth);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      if (typeof desktop.getStoragePaths !== "function" || typeof desktop.getStorageHealth !== "function") return;
      const [nextPaths, nextHealth] = await Promise.all([desktop.getStoragePaths(), desktop.getStorageHealth()]);
      setPaths(nextPaths);
      setHealth(nextHealth);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "无法读取本地存储状态");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  return <section className="suna-storage-data-panel" aria-labelledby="storage-data-heading" aria-busy={loading}>
    <header><div><span className="suna-settings-kicker">LOCAL STORAGE</span><h2 id="storage-data-heading">存储与数据</h2><p>查看 Suna 本地数据、知识内容和运行缓存的位置与健康状态。</p></div><button type="button" className="suna-icon-button" onClick={() => void load()} disabled={loading} aria-label="刷新存储状态" title="刷新存储状态">{loading ? <LoaderCircle size={16} className="suna-spin" /> : <RefreshCw size={16} />}</button></header>
    {error && <p className="suna-storage-error" role="alert">{error}</p>}
    <div className="suna-storage-metrics"><div><HardDrive size={16} /><span>SQLite 数据库</span><strong>{health.database_ok ? "正常" : "待检查"}</strong></div><div><Database size={16} /><span>迁移版本</span><strong>{health.current_migration_version} / {health.latest_migration_version}</strong></div><div><HardDrive size={16} /><span>数据库大小</span><strong>{formatBytes(health.database_size_bytes)}</strong></div><div><FolderOpen size={16} /><span>可用空间</span><strong>{formatBytes(health.available_disk_bytes)}</strong></div></div>
    {paths && <div className="suna-storage-paths"><h3>本地目录</h3>{([ ["应用数据", paths.app_data], ["SQLite 数据库", paths.sqlite_database], ["知识内容", paths.knowledge_content], ["缓存", paths.cache], ["日志", paths.logs], ["临时文件", paths.temp] ] as const).map(([label, value]) => <div key={label}><span>{label}</span><code title={value}>{value}</code></div>)}</div>}
  </section>;
}
