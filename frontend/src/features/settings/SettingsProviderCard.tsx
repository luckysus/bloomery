import type { FormEvent } from "react";
import { PlugZap, Save, Trash2 } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import {
  type ProviderSlot,
  type SettingsEditor,
  slotTitles,
} from "./settingsModel";
import type { ProviderKind } from "../../bridge/desktop";

interface SettingsProviderCardProps {
  editor: SettingsEditor;
  busy: boolean;
  testing: boolean;
  onChange: (editor: SettingsEditor) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>, editor: SettingsEditor) => void;
  onTest: (editor: SettingsEditor) => void;
  onDelete: (editor: SettingsEditor) => void;
}

export default function SettingsProviderCard({
  editor,
  busy,
  testing,
  onChange,
  onSubmit,
  onTest,
  onDelete,
}: SettingsProviderCardProps) {
  const { t } = useLocale();
  const update = <K extends keyof SettingsEditor>(key: K, value: SettingsEditor[K]) => {
    onChange({ ...editor, [key]: value } as SettingsEditor);
  };
  const updateKind = (kind: ProviderKind) => {
    const preset = kind === "deepseek"
      ? { displayName: "DeepSeek", baseUrl: "https://api.deepseek.com", modelId: "deepseek-v4-flash" }
      : kind === "ollama"
        ? { displayName: "Ollama", baseUrl: "http://127.0.0.1:11434", modelId: "qwen3" }
        : { displayName: "OpenAI Compatible", baseUrl: "https://api.openai.com/v1", modelId: "gpt-4o-mini" };
    onChange({ ...editor, kind, ...preset });
  };

  return (
    <form className="bloomery-settings-card" onSubmit={(event) => void onSubmit(event, editor)}>
      <div className="bloomery-settings-card-heading"><div><span className="bloomery-settings-card-icon"><PlugZap size={17} aria-hidden="true" /></span><h2>{t(slotTitles[editor.slot])}</h2></div><span className={`bloomery-settings-status ${editor.secretConfigured ? "is-configured" : "is-missing"}`}>{editor.secretConfigured ? t("settingsSecretConfigured") : t("settingsSecretMissing")}</span></div>
      <div className="bloomery-settings-fields">
        {editor.slot === "chat" && (
          <>
            <label htmlFor="settings-chat-kind">{t("settingsProviderType")}</label>
            <select id="settings-chat-kind" value={editor.kind} onChange={(event) => updateKind(event.target.value as ProviderKind)}>
              <option value="deepseek">{t("providerDeepSeek")}</option>
              <option value="open_ai_compatible">{t("providerOpenAiCompatible")}</option>
              <option value="ollama">{t("providerOllama")}</option>
            </select>
          </>
        )}
        <label htmlFor={`settings-${editor.slot}-name`}>{t("settingsDisplayName")}</label>
        <input id={`settings-${editor.slot}-name`} value={editor.displayName} onChange={(event) => update("displayName", event.target.value)} required />
        <label htmlFor={`settings-${editor.slot}-url`}>{t("settingsBaseUrl")}</label>
        <input id={`settings-${editor.slot}-url`} value={editor.baseUrl} onChange={(event) => update("baseUrl", event.target.value)} required />
        {editor.kind !== "mineru" && (
          <>
            <label htmlFor={`settings-${editor.slot}-model`}>{t("settingsModelId")}</label>
            <input id={`settings-${editor.slot}-model`} list={editor.kind === "deepseek" ? "deepseek-models" : undefined} value={editor.modelId} onChange={(event) => update("modelId", event.target.value)} required />
            {editor.kind === "deepseek" && (
              <datalist id="deepseek-models">
                <option value="deepseek-v4-flash" />
                <option value="deepseek-v4-pro" />
              </datalist>
            )}
          </>
        )}
        <label htmlFor={`settings-${editor.slot}-key`}>{t("settingsApiKey")}</label>
        <input id={`settings-${editor.slot}-key`} aria-label={`provider.${editor.slot}.apiKey`} type="password" autoComplete="new-password" value={editor.apiKey} onChange={(event) => update("apiKey", event.target.value)} placeholder={t("settingsApiKeyPlaceholder")} />
      </div>
      <label className="bloomery-settings-enabled"><input type="checkbox" checked={editor.enabled} onChange={(event) => update("enabled", event.target.checked)} />{t("settingsEnabled")}</label>
      <div className="bloomery-settings-card-actions">
        <button type="submit" className="bloomery-action-primary" disabled={busy}><Save size={16} aria-hidden="true" />{busy ? t("saving") : t("settingsSave")}</button>
        <button type="button" className="bloomery-action-secondary" onClick={() => void onTest(editor)} disabled={testing || busy}><PlugZap size={16} aria-hidden="true" />{testing ? t("testing") : t("settingsTest")}</button>
        {editor.id && <button type="button" className="bloomery-icon-button bloomery-settings-delete" onClick={() => void onDelete(editor)} disabled={busy} aria-label={`${t("settingsDelete")} ${editor.displayName}`} title={t("settingsDelete")}><Trash2 size={16} aria-hidden="true" /></button>}
      </div>
    </form>
  );
}

export type { ProviderSlot };
