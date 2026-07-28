import { useEffect, useRef, useState } from "react";
import {
  getMineruStatus,
  getMineruReleases,
  startMineruUpdate,
  getMineruUpdateJob,
  activateMineruVersion,
  rollbackMineruVersion,
  deleteMineruVersion,
  type MinerUStatus,
  type MinerUReleaseItem,
  type MinerUUpdateJob,
} from "../../services/mineruVersions";

export default function MinerUVersionManager() {
  const [status, setStatus] = useState<MinerUStatus | null>(null);
  const [releases, setReleases] = useState<MinerUReleaseItem[]>([]);
  const [selectedVersion, setSelectedVersion] = useState("");
  const [job, setJob] = useState<MinerUUpdateJob | null>(null);
  const [error, setError] = useState("");
  const pollRef = useRef<number | null>(null);

  const refresh = () => getMineruStatus().then(setStatus).catch((e) => setError(String(e)));
  useEffect(() => {
    refresh();
    getMineruReleases()
      .then((r) => {
        setReleases(r.releases);
        if (r.releases.length) setSelectedVersion(r.releases[0].version);
      })
      .catch(() => {});
  }, []);
  useEffect(() => () => {
    if (pollRef.current) window.clearInterval(pollRef.current);
  }, []);

  const pollJob = (jobId: string) => {
    if (pollRef.current) window.clearInterval(pollRef.current);
    pollRef.current = window.setInterval(async () => {
      const j = await getMineruUpdateJob(jobId);
      setJob(j);
      if (["ready", "failed"].includes(j.status)) {
        if (pollRef.current) window.clearInterval(pollRef.current);
        refresh();
      }
    }, 3000);
  };

  const onUpdate = async (version: string) => {
    setError("");
    try {
      const res = await startMineruUpdate(version);
      pollJob(res.job_id);
    } catch (e) {
      setError(String(e));
    }
  };
  const onActivate = async (version: string) => {
    setError("");
    try {
      await activateMineruVersion(version);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };
  const onRollback = async (version: string) => {
    setError("");
    try {
      await rollbackMineruVersion(version);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };
  const onDelete = async (version: string) => {
    setError("");
    try {
      await deleteMineruVersion(version);
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  if (!status) return <div className="text-sm text-slate-500">加载中...</div>;
  const installed = Object.entries(status.versions);
  const btn =
    "rounded-md border border-slate-200 bg-white px-2.5 py-1 text-xs font-semibold text-slate-700 transition-colors hover:border-indigo-300 hover:text-indigo-600 disabled:cursor-not-allowed disabled:opacity-40";

  return (
    <div className="space-y-4 text-sm text-slate-700">
      {error && <div className="rounded-lg bg-rose-50 px-3 py-2 text-xs text-rose-600">{error}</div>}

      <div>
        <div className="text-sm font-semibold text-slate-800">当前版本</div>
        <p className="mt-1 text-xs text-slate-500">
          激活：{status.active ?? "无"}
          {status.has_running_jobs ? "（有解析任务运行中，暂不可切换）" : ""}
        </p>
      </div>

      <div>
        <div className="text-sm font-semibold text-slate-800">可用版本</div>
        <div className="mt-2 flex items-center gap-2">
          <select
            className="h-8 flex-1 rounded-md border border-slate-200 bg-white px-2 text-xs text-slate-700 focus:border-indigo-300 focus:outline-none"
            value={selectedVersion}
            onChange={(e) => setSelectedVersion(e.target.value)}
          >
            {releases.map((r) => (
              <option key={r.version} value={r.version}>
                {r.version}
                {r.prerelease ? "（预发布）" : ""}
                {status.versions[r.version] ? "（已安装）" : ""}
              </option>
            ))}
          </select>
          <button
            className={btn}
            disabled={!selectedVersion || Boolean(status.versions[selectedVersion])}
            onClick={() => onUpdate(selectedVersion)}
          >
            {selectedVersion && status.versions[selectedVersion] ? "已安装" : "下载并安装"}
          </button>
        </div>
      </div>

      {job && (
        <div>
          <div className="text-sm font-semibold text-slate-800">
            更新进度：{job.version}（{job.status}）
          </div>
          <pre className="mt-2 max-h-60 overflow-auto rounded-lg bg-slate-900 p-3 text-xs text-slate-100">
            {(job.logs || []).join("\n")}
          </pre>
          {job.error && <div className="mt-1 text-xs text-rose-600">{job.error}</div>}
        </div>
      )}

      <div>
        <div className="text-sm font-semibold text-slate-800">已安装版本</div>
        <ul className="mt-2 space-y-1.5">
          {installed.map(([ver, rec]) => (
            <li key={ver} className="flex flex-wrap items-center gap-2">
              <span>
                {ver} — {rec.status}
                {rec.verified ? " ✓" : ""}
              </span>
              {ver !== status.active && rec.verified && (
                <button className={btn} disabled={status.has_running_jobs} onClick={() => onActivate(ver)}>
                  启用
                </button>
              )}
              {ver !== status.active && rec.verified && (
                <button className={btn} disabled={status.has_running_jobs} onClick={() => onRollback(ver)}>
                  回退到此
                </button>
              )}
              {ver !== status.active && (
                <button className={btn} onClick={() => onDelete(ver)}>
                  删除
                </button>
              )}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
