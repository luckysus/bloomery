import { useEffect, useState, type FormEvent } from "react";
import {
  Check,
  CircleAlert,
  CircleCheck,
  CircleX,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Server,
  Trash2,
  Wrench,
} from "lucide-react";
import {
  desktop,
  type McpHealth,
  type McpServerInput,
  type McpServerSummary,
  type McpToolSummary,
  type McpTransportKind,
} from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

type Draft = Omit<McpServerInput, "id"> & { id: string | null; bearer_token: string };

const emptyDraft = (): Draft => ({
  id: null,
  display_name: "",
  server_id: "",
  transport: "stdio",
  url: null,
  executable: "",
  args: [],
  working_directory: null,
  inherited_env: [],
  env_values: {},
  bearer_token: "",
  clear_bearer_token: false,
  timeout_ms: 30000,
  enabled: true,
});

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

function parseLines(value: string) {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function parseEnvironment(value: string): Record<string, string> {
  const entries: Record<string, string> = {};
  for (const line of parseLines(value)) {
    const separator = line.indexOf("=");
    if (separator <= 0) throw new Error("MCP environment entries must use NAME=value");
    const name = line.slice(0, separator).trim();
    const secret = line.slice(separator + 1);
    if (!/^[A-Za-z0-9_]+$/.test(name) || !secret) {
      throw new Error("MCP environment names and values are required");
    }
    entries[name] = secret;
  }
  return entries;
}

function updateServer(servers: McpServerSummary[], next: McpServerSummary) {
  const index = servers.findIndex((item) => item.id === next.id);
  if (index < 0) return [...servers, next];
  return servers.map((item) => (item.id === next.id ? next : item));
}

function serverTransportLabel(transport: McpTransportKind, t: (key: any) => string) {
  if (transport === "stdio") return t("extensionsMcpStdio");
  if (transport === "sse") return t("extensionsMcpSse");
  return t("extensionsMcpHttp");
}

export default function McpServersPanel() {
  const { t } = useLocale();
  const [servers, setServers] = useState<McpServerSummary[]>([]);
  const [draft, setDraft] = useState<Draft>(emptyDraft);
  const [argsText, setArgsText] = useState("");
  const [inheritedText, setInheritedText] = useState("");
  const [environmentText, setEnvironmentText] = useState("");
  const [health, setHealth] = useState<Record<string, McpHealth>>({});
  const [tools, setTools] = useState<Record<string, McpToolSummary[]>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      setServers(await desktop.listMcpServers());
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsMcpLoadError")));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const save = async (event: FormEvent) => {
    event.preventDefault();
    setBusy("save");
    setError(null);
    setNotice(null);
    try {
      const environment = parseEnvironment(environmentText);
      const payload: McpServerInput = {
        ...draft,
        url: draft.transport === "stdio" ? null : draft.url,
        executable: draft.transport === "stdio" ? draft.executable : null,
        args: parseLines(argsText),
        inherited_env: parseLines(inheritedText),
        env_values: environment,
        bearer_token: draft.bearer_token || undefined,
      };
      const saved = await desktop.saveMcpServer(payload);
      setServers((current) => updateServer(current, saved));
      setDraft(emptyDraft());
      setArgsText("");
      setInheritedText("");
      setEnvironmentText("");
      setNotice(t("extensionsMcpSaved"));
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsMcpSaveError")));
    } finally {
      setBusy(null);
    }
  };

  const inspect = async (server: McpServerSummary, restart = false) => {
    setBusy(`${restart ? "restart" : "check"}:${server.id}`);
    setError(null);
    setNotice(null);
    try {
      const result = restart
        ? await desktop.restartMcpServer(server.id)
        : await desktop.checkMcpServer(server.id);
      setHealth((current) => ({ ...current, [server.id]: result }));
      setTools((current) => ({ ...current, [server.id]: result.tools }));
      if (result.status === "healthy") setNotice(t("extensionsMcpChecked"));
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsMcpCheckError")));
    } finally {
      setBusy(null);
    }
  };

  const listTools = async (server: McpServerSummary) => {
    setBusy(`tools:${server.id}`);
    setError(null);
    try {
      const nextTools = await desktop.listMcpTools(server.id);
      setTools((current) => ({ ...current, [server.id]: nextTools }));
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsMcpToolsError")));
    } finally {
      setBusy(null);
    }
  };

  const edit = (server: McpServerSummary) => {
    setDraft({
      id: server.id,
      display_name: server.display_name,
      server_id: server.server_id,
      transport: server.transport,
      url: server.url,
      executable: server.executable,
      args: server.args,
      working_directory: server.working_directory,
      inherited_env: [],
      env_values: {},
      bearer_token: "",
      clear_bearer_token: false,
      timeout_ms: server.timeout_ms,
      enabled: server.enabled,
    });
    setArgsText(server.args.join("\n"));
    setInheritedText("");
    setEnvironmentText("");
    setNotice(t("extensionsMcpEditing"));
  };

  const remove = async (server: McpServerSummary) => {
    if (!window.confirm(t("extensionsMcpDeleteConfirm"))) return;
    setBusy(`delete:${server.id}`);
    setError(null);
    try {
      await desktop.deleteMcpServer(server.id);
      setServers((current) => current.filter((item) => item.id !== server.id));
      setNotice(t("extensionsMcpDeleted"));
    } catch (cause) {
      setError(errorMessage(cause, t("extensionsMcpDeleteError")));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="bloomery-extensions-section bloomery-mcp-panel" aria-labelledby="mcp-heading" aria-busy={loading}>
      <div className="bloomery-extensions-section-heading">
        <div><p className="bloomery-eyebrow">MODEL CONTEXT PROTOCOL / CONTROLLED</p><h2 id="mcp-heading">{t("extensionsMcpTitle")}</h2></div>
        <Server size={19} aria-hidden="true" />
      </div>
      <p className="bloomery-extensions-copy">{t("extensionsMcpCopy")}</p>
      {error && <div className="bloomery-extensions-alert" role="alert"><CircleAlert size={17} aria-hidden="true" /><span>{error}</span></div>}
      {notice && <div className="bloomery-extensions-notice" role="status"><Check size={17} aria-hidden="true" /><span>{notice}</span></div>}

      <form className="bloomery-mcp-form" onSubmit={(event) => void save(event)}>
        <div className="bloomery-mcp-form-heading"><strong>{draft.id ? t("extensionsMcpEdit") : t("extensionsMcpAdd")}</strong><span>{t("extensionsMcpSecretNote")}</span></div>
        <div className="bloomery-mcp-fields">
          <label><span>{t("extensionsMcpDisplayName")}</span><input value={draft.display_name} onChange={(event) => setDraft({ ...draft, display_name: event.target.value })} required /></label>
          <label><span>{t("extensionsMcpServerId")}</span><input value={draft.server_id} onChange={(event) => setDraft({ ...draft, server_id: event.target.value })} required /></label>
          <label><span>{t("extensionsMcpTransport")}</span><select value={draft.transport} onChange={(event) => setDraft({ ...draft, transport: event.target.value as McpTransportKind })}><option value="stdio">{t("extensionsMcpStdio")}</option><option value="streamable_http">{t("extensionsMcpHttp")}</option><option value="sse">{t("extensionsMcpSse")}</option></select></label>
          {draft.transport === "stdio" ? <label><span>{t("extensionsMcpExecutable")}</span><input value={draft.executable ?? ""} onChange={(event) => setDraft({ ...draft, executable: event.target.value })} required /></label> : <label><span>{t("extensionsMcpUrl")}</span><input type="url" value={draft.url ?? ""} onChange={(event) => setDraft({ ...draft, url: event.target.value })} required /></label>}
          <label><span>{t("extensionsMcpTimeout")}</span><input type="number" min="100" max="600000" step="100" value={draft.timeout_ms} onChange={(event) => setDraft({ ...draft, timeout_ms: Number(event.target.value) })} required /></label>
          <label><span>{t("extensionsMcpBearer")}</span><input type="password" value={draft.bearer_token} onChange={(event) => setDraft({ ...draft, bearer_token: event.target.value })} placeholder={t("extensionsMcpBearerPlaceholder")} autoComplete="new-password" /></label>
          <label className="bloomery-mcp-wide"><span>{t("extensionsMcpArguments")}</span><textarea value={argsText} onChange={(event) => setArgsText(event.target.value)} placeholder={t("extensionsMcpArgumentsPlaceholder")} /></label>
          <label className="bloomery-mcp-wide"><span>{t("extensionsMcpEnvironment")}</span><textarea value={environmentText} onChange={(event) => setEnvironmentText(event.target.value)} placeholder={t("extensionsMcpEnvironmentPlaceholder")} autoComplete="off" /></label>
        </div>
        <div className="bloomery-mcp-form-actions"><button type="submit" className="bloomery-secondary-button" disabled={busy === "save"}><Save size={15} aria-hidden="true" />{busy === "save" ? t("extensionsMcpSaving") : t("extensionsMcpSave")}</button>{draft.id && <button type="button" className="bloomery-icon-button" onClick={() => setDraft(emptyDraft())} aria-label={t("extensionsMcpCancelEdit")} title={t("extensionsMcpCancelEdit")}><CircleX size={17} aria-hidden="true" /></button>}</div>
      </form>

      {servers.length === 0 ? <div className="bloomery-extensions-empty"><Server size={18} aria-hidden="true" /><span>{t("extensionsMcpEmpty")}</span></div> : <div className="bloomery-mcp-list">
        {servers.map((server) => {
          const result = health[server.id];
          const serverTools = tools[server.id] ?? [];
          return <article className="bloomery-mcp-item" key={server.id}>
            <div className="bloomery-mcp-item-main"><div className="bloomery-extension-item-heading"><span className="bloomery-extension-icon"><Server size={17} aria-hidden="true" /></span><div><h3>{server.display_name}</h3><p>{server.server_id} · {serverTransportLabel(server.transport, t)}</p></div></div><dl className="bloomery-extension-details"><div><dt>{t("extensionsMcpStatus")}</dt><dd>{result?.status === "healthy" ? <span className="bloomery-mcp-health is-healthy"><CircleCheck size={14} aria-hidden="true" />{t("extensionsMcpHealthy")}</span> : result?.status === "failed" ? <span className="bloomery-mcp-health is-failed"><CircleX size={14} aria-hidden="true" />{t("extensionsMcpFailed")}</span> : t("extensionsMcpUnchecked")}</dd></div><div><dt>{t("extensionsMcpTools")}</dt><dd>{result?.tool_count ?? server.tool_count}</dd></div><div><dt>{t("extensionsMcpCredentials")}</dt><dd>{server.secret_configured ? t("extensionsMcpConfigured") : t("extensionsMcpNotConfigured")}</dd></div></dl>{result?.error && <p className="bloomery-mcp-error">{result.error}</p>}{serverTools.length > 0 && <div className="bloomery-mcp-tools">{serverTools.map((tool) => <div key={tool.id}><strong>{tool.name}</strong><span>{tool.description}</span><code>{tool.id}</code></div>)}</div>}</div>
            <div className="bloomery-mcp-actions"><button type="button" className="bloomery-icon-button" onClick={() => void inspect(server)} disabled={busy !== null} aria-label={t("extensionsMcpCheck")} title={t("extensionsMcpCheck")}><CircleCheck size={16} aria-hidden="true" /></button><button type="button" className="bloomery-icon-button" onClick={() => void inspect(server, true)} disabled={busy !== null} aria-label={t("extensionsMcpRestart")} title={t("extensionsMcpRestart")}><RefreshCw size={16} aria-hidden="true" /></button><button type="button" className="bloomery-icon-button" onClick={() => void listTools(server)} disabled={busy !== null} aria-label={t("extensionsMcpTools")} title={t("extensionsMcpTools")}><Wrench size={16} aria-hidden="true" /></button><button type="button" className="bloomery-icon-button" onClick={() => edit(server)} disabled={busy !== null} aria-label={t("extensionsMcpEdit")} title={t("extensionsMcpEdit")}><Pencil size={16} aria-hidden="true" /></button><button type="button" className="bloomery-icon-button" onClick={() => void remove(server)} disabled={busy !== null} aria-label={t("extensionsMcpDelete")} title={t("extensionsMcpDelete")}><Trash2 size={16} aria-hidden="true" /></button></div>
          </article>;
        })}
      </div>}
      <div className="bloomery-mcp-footnote"><Plus size={14} aria-hidden="true" />{t("extensionsMcpFooter")}</div>
    </section>
  );
}
