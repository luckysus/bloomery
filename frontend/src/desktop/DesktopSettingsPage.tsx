import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Download, Save } from "lucide-react";
import { exportDiagnostics, downloadDiagnostics } from "./services/diagnostics";
import { getCloudApiBaseSetting, saveCloudApiBaseSetting } from "./services/settings";

export default function DesktopSettingsPage() {
  const [apiBase, setApiBase] = useState("");
  const [status, setStatus] = useState("");
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    void getCloudApiBaseSetting().then(setApiBase).catch((err) => setStatus(String(err)));
  }, []);

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    await saveCloudApiBaseSetting(apiBase.trim());
    setStatus("已保存。");
  };

  const handleExportDiagnostics = async () => {
    setExporting(true);
    setStatus("");
    try {
      const report = await exportDiagnostics();
      downloadDiagnostics(report);
      setStatus("诊断报告已生成。");
    } catch (err) {
      setStatus(err instanceof Error ? err.message : String(err));
    } finally {
      setExporting(false);
    }
  };

  return (
    <section className="min-h-0 flex-1 overflow-auto bg-slate-950 p-5">
      <div className="max-w-2xl space-y-6">
        <div>
          <h2 className="text-lg font-semibold text-slate-100">设置</h2>
          <p className="mt-1 text-sm text-slate-500">配置桌面端调用的云端 API 地址。留空时使用当前同源 `/api`。</p>
        </div>

        <form onSubmit={handleSubmit} className="space-y-3 rounded-md border border-slate-800 bg-slate-900 p-4">
          <label className="block text-sm text-slate-300">
            云端 API Base
            <input
              value={apiBase}
              onChange={(event) => setApiBase(event.target.value)}
              placeholder="例如 https://agent.mystl.top"
              className="mt-2 w-full rounded-md border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-slate-100 outline-none focus:border-cyan-500"
            />
          </label>
          <button type="submit" className="inline-flex items-center gap-2 rounded-md bg-cyan-500 px-4 py-2 text-sm font-semibold text-slate-950 hover:bg-cyan-400">
            <Save className="h-4 w-4" />
            保存设置
          </button>
        </form>

        <section className="rounded-md border border-slate-800 bg-slate-900 p-4">
          <h3 className="text-sm font-semibold text-slate-100">本地诊断</h3>
          <p className="mt-1 text-sm text-slate-500">
            导出应用版本、系统、SQLite 是否存在、表数量和最后错误类型；不会包含聊天内容、记忆正文、API 地址或登录令牌。
          </p>
          <button
            type="button"
            onClick={() => void handleExportDiagnostics()}
            disabled={exporting}
            className="mt-3 inline-flex items-center gap-2 rounded-md border border-slate-700 px-4 py-2 text-sm text-slate-100 hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-60"
          >
            <Download className="h-4 w-4" />
            {exporting ? "生成中" : "导出诊断"}
          </button>
        </section>

        <section className="rounded-md border border-slate-800 bg-slate-900 p-4">
          <h3 className="text-sm font-semibold text-slate-100">软件更新</h3>
          <p className="mt-1 text-sm text-slate-500">
            当前版本可通过诊断报告查看。自动更新需要发布清单和签名密钥，配置更新源后再启用。
          </p>
        </section>

        {status && <p className="text-sm text-slate-400">{status}</p>}
      </div>
    </section>
  );
}
