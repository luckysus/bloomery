import { LoaderCircle, ShieldCheck, Trash2 } from "lucide-react";
import type { PermissionRuleRecord } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

interface PermissionRulesPanelProps {
  rules: PermissionRuleRecord[];
  busyId: string | null;
  onRevoke: (rule: PermissionRuleRecord) => void;
}

function sourceLabel(rule: PermissionRuleRecord) {
  switch (rule.source.kind) {
    case "mcp":
      return `MCP / ${rule.source.server_id}`;
    case "domain":
      return `Domain / ${rule.source.package_id}`;
    case "builtin":
      return "Builtin";
  }
}

function scopeLabel(rule: PermissionRuleRecord) {
  try {
    return JSON.stringify(rule.scope);
  } catch {
    return "{}";
  }
}

export default function PermissionRulesPanel({ rules, busyId, onRevoke }: PermissionRulesPanelProps) {
  const { t } = useLocale();

  return (
    <section className="bloomery-settings-permissions" aria-labelledby="settings-permissions-heading">
      <div className="bloomery-settings-permissions-heading">
        <div>
          <p className="bloomery-eyebrow">AGENT / ACCESS CONTROL</p>
          <h2 id="settings-permissions-heading">{t("permissionRulesTitle")}</h2>
          <p>{t("permissionRulesCopy")}</p>
        </div>
        <ShieldCheck size={21} aria-hidden="true" />
      </div>
      {rules.length === 0 ? <p className="bloomery-settings-permissions-empty">{t("permissionRulesEmpty")}</p> : (
        <div className="bloomery-settings-permissions-list">
          {rules.map((rule) => (
            <article className="bloomery-settings-permission" key={rule.id}>
              <div>
                <strong>{rule.tool_id}</strong>
                <span>{t("permissionRuleVersion")}: {rule.tool_version.major}.{rule.tool_version.minor}.{rule.tool_version.patch}</span>
                <span>{t("permissionRuleSource")}: {sourceLabel(rule)}</span>
                <code>{t("permissionRuleScope")}: {scopeLabel(rule)}</code>
              </div>
              <button
                type="button"
                className="bloomery-icon-button bloomery-settings-permission-revoke"
                aria-label={`${t("permissionRevoke")} ${rule.tool_id}`}
                title={t("permissionRevoke")}
                disabled={busyId === rule.id}
                onClick={() => {
                  if (window.confirm(t("permissionRuleRevokeConfirm"))) onRevoke(rule);
                }}
              >
                {busyId === rule.id ? <LoaderCircle size={16} className="bloomery-spin" /> : <Trash2 size={16} aria-hidden="true" />}
              </button>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
