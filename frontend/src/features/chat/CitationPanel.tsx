import { BookOpen, LoaderCircle, X } from "lucide-react";
import { useEffect, useState } from "react";
import { desktop, type EvidenceItem, type ResolvedCitation } from "../../bridge/desktop";
import { useLocale } from "../../i18n/locale";

interface CitationPanelProps {
  auditId: string;
  evidence: EvidenceItem[];
}

function sourceStateLabel(
  state: ResolvedCitation["source_state"] | null,
  translate: (key: "citationCurrent" | "citationHistorical" | "citationDeleted" | "citationResolving") => string,
) {
  if (state === "active") return translate("citationCurrent");
  if (state === "inactive") return translate("citationHistorical");
  if (state === "deleted") return translate("citationDeleted");
  return translate("citationResolving");
}

export default function CitationPanel({ auditId, evidence }: CitationPanelProps) {
  const { t } = useLocale();
  const [selectedNumber, setSelectedNumber] = useState<number | null>(null);
  const [resolved, setResolved] = useState<ResolvedCitation | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (selectedNumber === null) {
      setResolved(null);
      return;
    }
    let mounted = true;
    setLoading(true);
    desktop.resolveKnowledgeCitation(auditId, selectedNumber)
      .then((citation) => {
        if (mounted) setResolved(citation);
      })
      .catch(() => {
        if (mounted) setResolved(null);
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });
    return () => {
      mounted = false;
    };
  }, [auditId, selectedNumber]);

  if (evidence.length === 0) return null;
  const fallback = selectedNumber === null
    ? null
    : evidence.find((item) => item.citation_number === selectedNumber) ?? null;
  const sourceName = resolved?.label ?? fallback?.chunk.source_name ?? t("citationResolving");
  const sourceText = resolved?.chunk.text ?? fallback?.chunk.text ?? "";

  return (
    <section className="bloomery-chat-citations" aria-label={t("citationSection")}>
      <div className="bloomery-chat-citations-heading">
        <span><BookOpen size={14} aria-hidden="true" />{t("citationSection")}</span>
        {selectedNumber !== null && (
          <button
            type="button"
            className="bloomery-icon-button"
            onClick={() => setSelectedNumber(null)}
            aria-label={t("closeCitationDetail")}
            title={t("closeCitationDetail")}
          >
            <X size={14} aria-hidden="true" />
          </button>
        )}
      </div>
      <div className="bloomery-chat-citation-list">
        {evidence.map((item) => (
          <button
            type="button"
            key={item.citation_number}
            className={`bloomery-chat-citation ${item.citation_number === selectedNumber ? "is-active" : ""}`}
            onClick={() => setSelectedNumber(item.citation_number)}
            aria-label={t("citationAria", { number: item.citation_number, source: item.chunk.source_name })}
          >
            <strong>[{item.citation_number}]</strong>
            <span>{item.chunk.source_name}</span>
          </button>
        ))}
      </div>
      {selectedNumber !== null && (
        <div className="bloomery-chat-citation-detail" role="region" aria-label={`${t("citationDetail")} ${selectedNumber}`}>
          {loading ? (
            <span className="bloomery-chat-citation-loading"><LoaderCircle size={14} className="bloomery-spin" />{t("resolvingCitation")}</span>
          ) : (
            <>
              <div className="bloomery-chat-citation-detail-heading">
                <strong>{sourceName}</strong>
                <span>{sourceStateLabel(resolved?.source_state ?? null, (key) => t(key))}</span>
              </div>
              <p>{sourceText}</p>
            </>
          )}
        </div>
      )}
    </section>
  );
}
