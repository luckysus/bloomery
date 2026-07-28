import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { listCloudJobs, syncCloudJobs, type DesktopCloudJob } from "./services/cloudJobs";

export default function DesktopTasksPage() {
  const [jobs, setJobs] = useState<DesktopCloudJob[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const refresh = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await syncCloudJobs();
      setJobs(result.jobs);
      if (result.failed > 0) {
        setError(`有 ${result.failed} 个云任务状态暂时同步失败，已保留本地镜像。`);
      }
    } catch (err) {
      setJobs(await listCloudJobs().catch(() => []));
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  return (
    <section className="min-h-0 flex-1 overflow-auto bg-slate-950 p-5">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-slate-100">本地任务镜像</h2>
          <p className="mt-1 text-sm text-slate-500">云端训练、优化、文献或智能体任务返回 ID 后，会记录在这里。</p>
        </div>
        <button
          type="button"
          onClick={() => void refresh()}
          className="inline-flex items-center gap-2 rounded-md bg-slate-800 px-4 py-2 text-sm text-slate-100 hover:bg-slate-700"
        >
          <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
          刷新
        </button>
      </div>
      {error && <p className="mb-3 rounded-md border border-red-900 bg-red-950/40 p-3 text-sm text-red-200">{error}</p>}
      <div className="overflow-hidden rounded-md border border-slate-800">
        <table className="w-full text-left text-sm">
          <thead className="bg-slate-900 text-slate-400">
            <tr>
              <th className="px-3 py-2">类型</th>
              <th className="px-3 py-2">状态</th>
              <th className="px-3 py-2">云端任务 ID</th>
              <th className="px-3 py-2">来源</th>
              <th className="px-3 py-2">更新时间</th>
            </tr>
          </thead>
          <tbody>
            {jobs.map((job) => (
              <tr key={job.id} className="border-t border-slate-800 text-slate-300">
                <td className="px-3 py-2">{job.type}</td>
                <td className="px-3 py-2">{job.status}</td>
                <td className="px-3 py-2 font-mono text-xs">{job.cloud_job_id}</td>
                <td className="px-3 py-2 text-xs text-slate-500">{payloadSource(job.payload_json)}</td>
                <td className="px-3 py-2">{new Date(job.updated_at).toLocaleString()}</td>
              </tr>
            ))}
            {jobs.length === 0 && (
              <tr>
                <td className="px-3 py-6 text-center text-slate-500" colSpan={5}>
                  暂无任务记录。
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function payloadSource(payloadJson: string) {
  try {
    const payload = JSON.parse(payloadJson) as Record<string, unknown>;
    return String(payload.source || payload.event_type || "desktop");
  } catch {
    return "desktop";
  }
}
