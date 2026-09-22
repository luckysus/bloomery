import { useMemo, useState } from "react";
import { Atom, ChevronDown, ChevronRight, Globe } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import AIAnswerRenderer, { type AnswerReferenceResult } from "../../../components/answer/AnswerRenderer";
import type { WebMessage, WebSource } from "./webTypes";

function stripInternalAgentBlocks(text: string) {
  return text
    .replace(/<internal(?:_|-)?(?:thought|reasoning)>[\s\S]*?<\/internal(?:_|-)?(?:thought|reasoning)>/gi, "")
    .replace(/证据(\d+)/g, "文献$1")
    .trim();
}

function ReasoningBlock({
  reasoning,
  reasoningMs,
}: {
  reasoning: string;
  reasoningMs?: number;
}) {
  const [open, setOpen] = useState(true);
  const label = reasoningMs === undefined
    ? "正在思考"
    : `已思考（用时 ${Math.max(1, Math.round(reasoningMs / 1000))} 秒）`;

  return (
    <div className="bloomery-reasoning-block mb-2">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="bloomery-reasoning-toggle inline-flex items-center gap-2 bg-transparent py-0.5 text-sm font-medium"
        aria-expanded={open}
      >
        <Atom size={16} className="bloomery-reasoning-icon" />
        <span>{label}</span>
        {open
          ? <ChevronDown size={16} className="bloomery-reasoning-chevron" />
          : <ChevronRight size={16} className="bloomery-reasoning-chevron" />}
      </button>
      {open && (
        <div className="agent-reasoning-md bloomery-reasoning-content mt-1 border-l-2 pl-3 text-[15px] leading-relaxed">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{reasoning}</ReactMarkdown>
        </div>
      )}
    </div>
  );
}

function WebSourcesBar({ sources }: { sources: WebSource[] }) {
  const [open, setOpen] = useState(false);
  if (sources.length === 0) return null;
  return (
    <div className="mb-2">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="inline-flex items-center gap-1.5 rounded-full border border-[#e3d7ca] bg-[#fffaf3] px-3 py-1 text-[13px] font-medium text-[#6f6258] hover:bg-[#f7efe5]"
      >
        <Globe size={14} className="text-[#cc785c]" />
        <span>已搜索 {sources.length} 个网页</span>
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
      </button>
      {open && (
        <div className="mt-2 flex flex-wrap gap-2">
          {sources.map((source) => (
            <a
              key={`${source.index}-${source.url}`}
              href={source.url}
              target="_blank"
              rel="noreferrer"
              className="flex max-w-[260px] items-center gap-1.5 rounded-lg border border-[#eadfd2] bg-[#fffaf3] px-2.5 py-1.5 text-[13px] text-[#4a3f35] hover:border-[#cc785c]/45 hover:bg-[#f7efe5]"
              title={source.title}
            >
              <span className="flex h-4 w-4 shrink-0 items-center justify-center rounded-[4px] bg-[#f0e5da] text-[10px] font-semibold text-[#cc785c]">
                {source.index}
              </span>
              <span className="truncate">{source.title}</span>
            </a>
          ))}
        </div>
      )}
    </div>
  );
}

export default function WebAnswerRenderer({ message }: { message: WebMessage }) {
  const response = message.response;
  const reasoning = (message.reasoning ?? response?.reasoning ?? "").trim();
  const reasoningMs = message.reasoningMs ?? response?.reasoning_ms;
  const webSources = response?.web_sources ?? [];
  const evidence = response?.evidence ?? message.streamEvidence;
  const literatureResults = useMemo<AnswerReferenceResult[]>(
    () => evidence.map((item, index) => ({
      content: item.chunk.text || item.chunk.source_name || `文献${index + 1}`,
      paper_name: item.chunk.source_name || `文献${index + 1}`,
      header_path: item.chunk.source_location.kind === "heading"
        ? item.chunk.source_location.path.join(" / ")
        : item.chunk.source_location.kind,
      similarity_score: item.chunk.rerank_score ?? item.chunk.rrf_score,
    })),
    [evidence],
  );

  return (
    <>
      {reasoning ? <ReasoningBlock reasoning={reasoning} reasoningMs={reasoningMs} /> : null}
      {!reasoning && webSources.length > 0 ? <WebSourcesBar sources={webSources} /> : null}
      <AIAnswerRenderer
        answer={stripInternalAgentBlocks(message.content)}
        literatureResults={literatureResults}
        webSources={webSources}
      />
    </>
  );
}
