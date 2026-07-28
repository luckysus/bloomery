import type { AgentEvidence, AgentMessage, AgentResponse } from "../agent/types";
import type { AnswerReferenceResult, ImageReference } from "../components/answer/AnswerRenderer";

export interface AgentRetrievalFlowTargets {
  yieldValue?: number;
  tensileValue?: number;
  elongationValue?: number;
}

export function parseAgentRetrievalFlowTargets(text: string): AgentRetrievalFlowTargets {
  const targets: AgentRetrievalFlowTargets = {};
  const pickNumber = (patterns: RegExp[]) => {
    for (const pattern of patterns) {
      const match = text.match(pattern);
      if (match?.[1]) {
        const value = Number(match[1]);
        if (Number.isFinite(value) && value > 0) return value;
      }
    }
    return undefined;
  };
  targets.yieldValue = pickNumber([
    new RegExp("(?:\\u5c48\\u670d(?:\\u5f3a\\u5ea6)?|RP0\\.?2|Rp0\\.?2|yield|ys)[^\\d]{0,12}(\\d+(?:\\.\\d+)?)", "i"),
    new RegExp("(\\d+(?:\\.\\d+)?)\\s*(?:MPa|mpa)?[^\\n,，。；;]{0,8}(?:\\u5c48\\u670d(?:\\u5f3a\\u5ea6)?|RP0\\.?2|Rp0\\.?2)", "i"),
  ]);
  targets.tensileValue = pickNumber([
    new RegExp("(?:\\u6297\\u62c9(?:\\u5f3a\\u5ea6)?|Rm|tensile|ts)[^\\d]{0,12}(\\d+(?:\\.\\d+)?)", "i"),
    new RegExp("(\\d+(?:\\.\\d+)?)\\s*(?:MPa|mpa)?[^\\n,，。；;]{0,8}(?:\\u6297\\u62c9(?:\\u5f3a\\u5ea6)?|Rm)", "i"),
  ]);
  targets.elongationValue = pickNumber([
    new RegExp("(?:\\u65ad\\u540e\\u4f38\\u957f\\u7387|\\u4f38\\u957f\\u7387|\\u5ef6\\u4f38\\u7387|A\\s*\\(?%?\\)?|elongation|el)[^\\d]{0,12}(\\d+(?:\\.\\d+)?)", "i"),
    new RegExp("(\\d+(?:\\.\\d+)?)\\s*%?[^\\n,，。；;]{0,8}(?:\\u65ad\\u540e\\u4f38\\u957f\\u7387|\\u4f38\\u957f\\u7387|\\u5ef6\\u4f38\\u7387)", "i"),
  ]);
  return targets;
}

export function shouldRunAgentRetrievalOptimizationFlow(text: string, targets: AgentRetrievalFlowTargets) {
  const normalized = text.toLowerCase();
  const hasTarget = Boolean(targets.yieldValue || targets.tensileValue || targets.elongationValue);
  const asksOptimization = /优化|寻优|工艺|方案|推荐|调整|目标|达到|满足/.test(normalized);
  return hasTarget && asksOptimization;
}

export function buildAgentChatHistory(messages: AgentMessage[], maxMessages = 30) {
  return messages
    .filter(message => message.content.trim())
    .slice(-maxMessages)
    .map(message => ({
      role: message.role === "user" ? "user" : "assistant",
      content: message.content.slice(0, 2000),
    }));
}

export function agentEvidenceToLiteratureResults(response?: AgentResponse | null): AnswerReferenceResult[] {
  const evidence = response?.evidence ?? [];
  return evidence.map((item, index) => {
    const metadata = item.metadata ?? {};
    return {
      content: item.content || item.title || item.source_label || `文献${index + 1}`,
      paper_name: String(metadata.paper_name ?? metadata.paperName ?? item.source_label ?? item.title ?? `文献${index + 1}`),
      header_path: String(metadata.header_path ?? metadata.headerPath ?? item.evidence_level ?? item.type ?? ""),
      similarity_score: typeof item.score === "number" ? item.score : 0,
    };
  });
}

function evidenceToImageReference(item: AgentEvidence): ImageReference {
  const metadata = item.metadata ?? {};
  const imagePath = String(metadata.image_path ?? metadata.imagePath ?? item.source_id ?? "");
  const caption = String(metadata.caption ?? item.content ?? item.title ?? "");
  const paperName = metadata.paper_name ?? metadata.paperName;
  const headerPath = metadata.header_path ?? metadata.headerPath;
  return {
    imagePath,
    caption,
    paperName: paperName != null ? String(paperName) : undefined,
    headerPath: headerPath != null ? String(headerPath) : undefined,
  };
}

/**
 * 把证据中的图片（type==="image"）按 source_label 前缀拆成“图片N”与“金相照片N”两组，
 * 按标签里的编号回填到对应下标，供 AnswerRenderer 的 imageResults / experimentalImageResults 使用。
 */
export function agentEvidenceToImageReferences(
  response?: AgentResponse | null,
  streamEvidence?: AgentEvidence[],
): { imageResults: ImageReference[]; experimentalImageResults: ImageReference[] } {
  const evidence = response?.evidence ?? streamEvidence ?? [];
  const imageResults: ImageReference[] = [];
  const experimentalImageResults: ImageReference[] = [];
  for (const item of evidence) {
    const label = item.source_label ?? "";
    const metalMatch = label.match(/^金相照片(\d+)/);
    const imgMatch = label.match(/^图片(\d+)/);
    if (!metalMatch && !imgMatch) continue;
    if (metalMatch) {
      experimentalImageResults[parseInt(metalMatch[1], 10) - 1] = evidenceToImageReference(item);
    } else if (imgMatch) {
      imageResults[parseInt(imgMatch[1], 10) - 1] = evidenceToImageReference(item);
    }
  }
  return { imageResults, experimentalImageResults };
}
