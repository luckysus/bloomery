import { useMemo, useState } from "react";
import { Atom, ChevronDown, ChevronRight, Globe } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { stripInternalAgentBlocks } from "../../agent/agentRunDisplay";
import type { AgentMessage, AgentWebSource } from "../../agent/types";
import { agentEvidenceToImageReferences, agentEvidenceToLiteratureResults } from "../../utils/agentFlow";
import AIAnswerRenderer, { type AnswerReferenceResult } from "../answer/AnswerRenderer";

function ReasoningBlock({ reasoning, reasoningMs, webSources = [] }: { reasoning: string; reasoningMs?: number; webSources?: AgentWebSource[] }) {
  const done = reasoningMs !== undefined;
  // 完成后保持展开，思考过程一直可见（用户可手动点标题收起）。
  const [open, setOpen] = useState(true);

  const seconds = done ? Math.max(1, Math.round((reasoningMs as number) / 1000)) : 0;
  const label = done ? `已思考（用时 ${seconds} 秒）` : "正在思考";

  return (
    <div style={{ marginBottom: 8, background: "transparent" }}>
      <button
        type="button"
        onClick={() => setOpen(v => !v)}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 8,
          background: "transparent",
          border: "none",
          cursor: "pointer",
          padding: "2px 0",
          color: "#6f6258",
          fontSize: 14,
          fontWeight: 500,
          userSelect: "none",
        }}
      >
        <Atom size={16} style={{ color: "#cc785c" }} />
        <span>{label}</span>
        {open ? <ChevronDown size={16} style={{ color: "#a89684" }} /> : <ChevronRight size={16} style={{ color: "#a89684" }} />}
      </button>
      {open ? (
        <div
          className="agent-reasoning-md"
          style={{
            marginTop: 4,
            paddingLeft: 14,
            borderLeft: "2px solid #d9d9d9",
            color: "#8b8b8b",
            fontSize: 15,
            lineHeight: 1.7,
          }}
        >
          {webSources.length ? <WebSourcesBar sources={webSources} /> : null}
          {reasoning ? <ReactMarkdown remarkPlugins={[remarkGfm]}>{reasoning}</ReactMarkdown> : null}
        </div>
      ) : null}
    </div>
  );
}

function WebSourcesBar({ sources }: { sources: AgentWebSource[] }) {
  const [open, setOpen] = useState(false);
  if (!sources.length) return null;
  return (
    <div style={{ marginBottom: 10 }}>
      <button
        type="button"
        onClick={() => setOpen(v => !v)}
        className="flex items-center gap-1.5 rounded-full border border-[#e3d7ca] bg-[#fffaf3] px-3 py-1 text-[13px] font-medium text-[#6f6258] transition-colors hover:bg-[#f7efe5] hover:text-[#2b2118]"
      >
        <Globe size={14} className="text-[#cc785c]" />
        <span>已搜索 {sources.length} 个网页</span>
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
      </button>
      {open ? (
        <div className="mt-2 flex flex-wrap gap-2">
          {sources.map(src => (
            <a
              key={src.index}
              href={src.url}
              target="_blank"
              rel="noreferrer"
              title={src.title}
              className="flex max-w-[260px] items-center gap-1.5 rounded-lg border border-[#eadfd2] bg-[#fffaf3] px-2.5 py-1.5 text-[13px] text-[#4a3f35] transition-colors hover:border-[#cc785c]/45 hover:bg-[#f7efe5]"
            >
              <span className="flex h-4 w-4 shrink-0 items-center justify-center rounded-[4px] bg-[#f0e5da] text-[10px] font-semibold text-[#cc785c]">{src.index}</span>
              <span className="truncate">{src.title}</span>
            </a>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export default function AgentAnswerRenderer({ message }: { message: AgentMessage }) {
  const response = message.response ?? null;
  const reasoning = (message.reasoning ?? "").trim();
  const webSources = message.webSources ?? [];
  const evidenceResults = useMemo(() => {
    const sourceResponse = response;
    if (sourceResponse) return agentEvidenceToLiteratureResults(sourceResponse);
    const evidence = message.streamEvidence ?? [];
    return evidence.map((item, index) => {
      const metadata = item.metadata ?? {};
      return {
        content: item.content || item.title || item.source_label || `文献${index + 1}`,
        paper_name: String(metadata.paper_name ?? metadata.paperName ?? item.source_label ?? item.title ?? `文献${index + 1}`),
        header_path: String(metadata.header_path ?? metadata.headerPath ?? item.evidence_level ?? item.type ?? ""),
        similarity_score: typeof item.score === "number" ? item.score : 0,
      } as AnswerReferenceResult;
    });
  }, [message.streamEvidence, response]);
  const { imageResults, experimentalImageResults } = useMemo(
    () => agentEvidenceToImageReferences(response, message.streamEvidence),
    [message.streamEvidence, response],
  );
  const answer = useMemo(() => {
    return stripInternalAgentBlocks(message.content).replace(/证据(\d+)/g, "文献$1");
  }, [message.content]);

  return (
    <>
      {reasoning ? (
        <ReasoningBlock reasoning={reasoning} reasoningMs={message.reasoningMs} webSources={webSources} />
      ) : (
        webSources.length ? <WebSourcesBar sources={webSources} /> : null
      )}
      <AIAnswerRenderer
        answer={answer}
        literatureResults={evidenceResults}
        imageResults={imageResults}
        experimentalImageResults={experimentalImageResults}
        fallbackPrefix="文献"
        webSources={webSources}
      />
    </>
  );
}
