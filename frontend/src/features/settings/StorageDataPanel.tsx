import { Database, Download, ExternalLink, FolderOpen, HardDrive, LoaderCircle, RefreshCw, Trash2, Archive } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { desktop, type StorageHealth, type StoragePaths } from "../../bridge/desktop";

const emptyHealth: StorageHealth = { database_ok: false, current_migration_version: 0, latest_migration_version: 0, database_size_bytes: 0, reclaimable_bytes: 0, available_disk_bytes: null };
function formatBytes(value: number | null) { if (value == null) return "未知"; if (value < 1024) return String(value) + " B"; const units = ["KB", "MB", "GB", "TB"]; let amount = value; let unit = "B"; for (const candidate of units) { amount /= 1024; unit = candidate; if (amount < 1024) break; } return amount.toFixed(amount >= 10 ? 0 : 1) + " " + unit; }

export default function StorageDataPanel() {
  const [paths, setPaths] = useState<StoragePaths | null>(null);
  const [health, setHealth] = useState(emptyHealth);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true); setError(null);
    try {
      if (typeof desktop.getStoragePaths !== "function" || typeof desktop.getStorageHealth !== "function") return;
      const [nextPaths, nextHealth] = await Promise.all([desktop.getStoragePaths(), desktop.getStorageHealth()]);
      setPaths(nextPaths); setHealth(nextHealth);
    } catch (cause) { setError(cause instanceof Error ? cause.message : "无法读取本地存储状态"); } finally { setLoading(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);

  const openPath = async (kind: keyof StoragePaths) => { setBusy("open:" + kind); setError(null); try { await desktop.openStoragePath(kind); } catch (cause) { setError(cause instanceof Error ? cause.message : "无法打开目录"); } finally { setBusy(null); } };
  const clear = async (kind: "cache" | "temp", label: string) => { if (!window.confirm("确认清理" + label + "吗？这不会删除知识内容或数据库。")) return; setBusy("clear:" + kind); setError(null); try { const removed = await desktop.clearStorageCache(kind); setNotice("已清理 " + removed + " 个" + label + "项目"); await load(); } catch (cause) { setError(cause instanceof Error ? cause.message : "清理" + label + "失败"); } finally { setBusy(null); } };
  const exportDiagnostics = async () => { const outputPath = await desktop.saveFileDialog({ defaultPath: "suna-diagnostics.json", filters: [{ name: "JSON", extensions: ["json"] }] }); if (!outputPath) return; setBusy("diagnostics"); setError(null); try { await desktop.writeDiagnosticsExport(outputPath); setNotice("诊断信息已导出"); } catch (cause) { setError(cause instanceof Error ? cause.message : "诊断信息导出失败"); } finally { setBusy(null); } };
  const createBackup = async () => { const outputPath = await desktop.saveFileDialog({ defaultPath: "suna-backup.suna", filters: [{ name: "Suna 备份", extensions: ["suna-backup"] }] }); if (!outputPath) return; setBusy("backup"); setError(null); try { const result = await desktop.createBackup(outputPath); setNotice("备份已创建：" + formatBytes(result.database_bytes) + " 数据库，" + result.content_file_count + " 个内容文件"); } catch (cause) { setError(cause instanceof Error ? cause.message : "备份创建失败"); } finally { setBusy(null); } };

  return <section className="suna-storage-data-panel" aria-labelledby="storage-data-heading" aria-busy={loading}><header><div><span className="suna-settings-kicker">LOCAL STORAGE</span><h2 id="storage-data-heading">存储与数据</h2><p>查看 Suna 本地数据、知识内容和运行缓存的位置与健康状态。</p></div><button type="button" className="suna-icon-button" onClick={() => void load()} disabled={loading} aria-label="刷新存储状态" title="刷新存储状态">{loading ? <LoaderCircle size={16} className="suna-spin" /> : <RefreshCw size={16} />}</button></header>{error && <p className="suna-storage-error" role="alert">{error}</p>}{notice && <p className="suna-storage-notice" role="status">{notice}</p>}<div className="suna-storage-metrics"><div><HardDrive size={16} /><span>SQLite 数据库</span><strong>{health.database_ok ? "正常" : "待检查"}</strong></div><div><Database size={16} /><span>迁移版本</span><strong>{health.current_migration_version} / {health.latest_migration_version}</strong></div><div><HardDrive size={16} /><span>数据库大小</span><strong>{formatBytes(health.database_size_bytes)}</strong></div><div><FolderOpen size={16} /><span>可回收空间</span><strong>{formatBytes(health.reclaimable_bytes)}</strong></div><div><HardDrive size={16} /><span>可用磁盘</span><strong>{formatBytes(health.available_disk_bytes)}</strong></div></div>{paths && <div className="suna-storage-paths"><h3>本地目录</h3>{([ ["应用数据", "app_data", paths.app_data], ["SQLite 数据库", "sqlite_database", paths.sqlite_database], ["知识内容", "knowledge_content", paths.knowledge_content], ["缓存", "cache", paths.cache], ["日志", "logs", paths.logs], ["临时文件", "temp", paths.temp] ] as const).map(([label, kind, value]) => <div key={kind}><span>{label}</span><code title={value}>{value}</code>{kind !== "sqlite_database" && kind !== "knowledge_content" && <button type="button" className="suna-icon-button" onClick={() => void openPath(kind)} disabled={busy === "open:" + kind} aria-label={"打开" + label} title={"打开" + label}>{busy === "open:" + kind ? <LoaderCircle size={14} className="suna-spin" /> : <ExternalLink size={14} />}</button>}</div>)}</div>}<div className="suna-storage-actions"><button type="button" className="suna-secondary-button" onClick={() => void clear("cache", "缓存")} disabled={busy !== null}><Trash2 size={15} />清理缓存</button><button type="button" className="suna-secondary-button" onClick={() => void clear("temp", "临时文件")} disabled={busy !== null}><Trash2 size={15} />清理临时文件</button><button type="button" className="suna-secondary-button" onClick={() => void exportDiagnostics()} disabled={busy !== null}><Download size={15} />导出诊断</button><button type="button" className="suna-primary-button" onClick={() => void createBackup()} disabled={busy !== null}><Archive size={15} />创建备份</button></div></section>;
}
