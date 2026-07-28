import { useEffect, useState } from "react";
import { getIflytekAdminConfig, saveIflytekAdminConfig } from "../../services/user";

interface IflytekAsrConfigInfo {
  app_id: string;
  iat_eos: number;
  app_id_configured: boolean;
  api_key_configured: boolean;
  api_key_preview: string;
  api_secret_configured: boolean;
  api_secret_preview: string;
}

const DEFAULT_CONFIG: IflytekAsrConfigInfo = {
  app_id: "",
  iat_eos: 4000,
  app_id_configured: false,
  api_key_configured: false,
  api_key_preview: "",
  api_secret_configured: false,
  api_secret_preview: "",
};

const INPUT_CLASS =
  "mt-2 h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none placeholder:text-slate-400 focus:border-indigo-300 focus:ring-4 focus:ring-indigo-50";

export default function AsrConfigManager() {
  const [config, setConfig] = useState<IflytekAsrConfigInfo>(DEFAULT_CONFIG);
  const [appId, setAppId] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [apiSecret, setApiSecret] = useState("");
  const [iatEos, setIatEos] = useState(4000);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);

  const applyConfig = (cfg: IflytekAsrConfigInfo) => {
    setConfig(cfg);
    setAppId(cfg.app_id || "");
    setIatEos(cfg.iat_eos || 4000);
    setApiKey("");
    setApiSecret("");
  };

  const load = async () => {
    setLoading(true);
    setError("");
    try {
      const resp = await getIflytekAdminConfig();
      if (!resp.ok) throw new Error(await resp.text());
      const cfg = (await resp.json()) as IflytekAsrConfigInfo;
      applyConfig({ ...DEFAULT_CONFIG, ...cfg });
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const onSave = async () => {
    setSaving(true);
    setError("");
    setSaved(false);
    try {
      const resp = await saveIflytekAdminConfig({
        app_id: appId.trim(),
        api_key: apiKey.trim() ? apiKey.trim() : null,
        api_secret: apiSecret.trim() ? apiSecret.trim() : null,
        iat_eos: iatEos,
      });
      if (!resp.ok) throw new Error(await resp.text());
      const cfg = (await resp.json()) as IflytekAsrConfigInfo;
      applyConfig({ ...DEFAULT_CONFIG, ...cfg });
      setSaved(true);
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setSaving(false);
    }
  };

  const effective = config.app_id_configured && config.api_key_configured && config.api_secret_configured;

  return (
    <div className="space-y-4">
      <div
        className={`rounded-lg border px-3 py-2 text-sm ${
          effective ? "border-emerald-200 bg-emerald-50 text-emerald-700" : "border-amber-200 bg-amber-50 text-amber-700"
        }`}
      >
        {effective ? "语音听写已配置并生效。" : "尚未完整配置 APPID / APIKey / APISecret，麦克风语音输入暂不可用。"}
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <label className="block text-sm font-semibold text-slate-700">
          APPID
          <input
            value={appId}
            onChange={(event) => setAppId(event.target.value)}
            placeholder="讯飞控制台应用的 APPID"
            className={INPUT_CLASS}
          />
        </label>

        <label className="block text-sm font-semibold text-slate-700">
          静默断句时长 iat_eos（毫秒）
          <input
            value={iatEos}
            onChange={(event) => setIatEos(Number(event.target.value) || 0)}
            type="number"
            min={500}
            max={10000}
            placeholder="500 - 10000，默认 4000"
            className={INPUT_CLASS}
          />
          <span className="mt-2 block text-xs leading-relaxed text-slate-400">
            静默多久后自动结束一句识别，范围 500 - 10000 毫秒。
          </span>
        </label>

        <label className="block text-sm font-semibold text-slate-700">
          APIKey
          <input
            value={apiKey}
            onChange={(event) => setApiKey(event.target.value)}
            type="password"
            placeholder={config.api_key_configured ? "留空则保留已保存的 APIKey" : "请输入 APIKey"}
            className={INPUT_CLASS}
          />
          <span className="mt-2 block text-xs leading-relaxed text-slate-400">
            {config.api_key_configured ? `已配置：${config.api_key_preview || "******"}` : "未配置 APIKey。"}
          </span>
        </label>

        <label className="block text-sm font-semibold text-slate-700">
          APISecret
          <input
            value={apiSecret}
            onChange={(event) => setApiSecret(event.target.value)}
            type="password"
            placeholder={config.api_secret_configured ? "留空则保留已保存的 APISecret" : "请输入 APISecret"}
            className={INPUT_CLASS}
          />
          <span className="mt-2 block text-xs leading-relaxed text-slate-400">
            {config.api_secret_configured ? `已配置：${config.api_secret_preview || "******"}` : "未配置 APISecret。"}
          </span>
        </label>
      </div>

      {error && <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-600">{error}</div>}
      {saved && !error && <div className="text-sm text-emerald-600">已保存。</div>}

      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => void onSave()}
          disabled={saving || loading}
          className="inline-flex h-10 items-center rounded-lg bg-indigo-600 px-4 text-sm font-semibold text-white transition-colors hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-60"
        >
          {saving ? "保存中…" : "保存配置"}
        </button>
        <button
          type="button"
          onClick={() => void load()}
          disabled={loading || saving}
          className="inline-flex h-10 items-center rounded-lg border border-slate-200 px-4 text-sm font-semibold text-slate-600 transition-colors hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-60"
        >
          {loading ? "加载中…" : "刷新"}
        </button>
      </div>
    </div>
  );
}
