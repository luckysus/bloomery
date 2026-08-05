import React, { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { FileText, Globe, ImageIcon } from "lucide-react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import type { PluggableList } from "unified";
import { proxyImg } from "../../utils/searchRender";
import { useLocale } from "../../i18n/locale";

export interface AnswerReferenceResult {
  paper_name: string;
  header_path: string;
  content: string;
  similarity_score: number;
}

export interface ImageReference {
  imagePath: string;
  caption: string;
  paperName?: string;
  headerPath?: string;
}

export interface WebSourceRef {
  index: number;
  title: string;
  url: string;
  site?: string;
  date?: string;
  snippet?: string;
}

const remarkGfmNoSingleTilde: PluggableList = [[remarkGfm, { singleTilde: false }]];

let globalUnlock: (() => void) | null = null;

function HoverCardTag({
  tagLabel,
  children,
}: {
  tagLabel: ReactNode;
  children: ReactNode;
}) {
  const [isHovered, setIsHovered] = useState(false);
  const [isTooltipHovered, setIsTooltipHovered] = useState(false);
  const [isLocked, setIsLocked] = useState(false);
  const tagRef = useRef<HTMLSpanElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const showTooltip = isHovered || isTooltipHovered || isLocked;

  const unlock = useCallback(() => {
    setIsLocked(false);
    if (globalUnlock === unlock) globalUnlock = null;
  }, []);

  useLayoutEffect(() => {
    const tooltip = tooltipRef.current;
    const tag = tagRef.current;
    if (!tooltip || !tag) return;

    const tagRect = tag.getBoundingClientRect();
    const tooltipW = tooltip.offsetWidth || 340;
    const tooltipH = tooltip.offsetHeight || 0;

    let left = tagRect.left;
    if (left + tooltipW > window.innerWidth - 16) left = window.innerWidth - tooltipW - 16;
    if (left < 16) left = 16;

    if (tagRect.top > tooltipH + 16) {
      tooltip.style.top = `${tagRect.top - tooltipH - 8}px`;
    } else {
      tooltip.style.top = `${tagRect.bottom + 8}px`;
    }
    tooltip.style.left = `${left}px`;
    tooltip.style.visibility = "visible";
  });

  useEffect(() => {
    if (!isLocked) return;
    const handleOutside = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (!tagRef.current?.contains(target) && !tooltipRef.current?.contains(target)) {
        unlock();
      }
    };
    const timer = setTimeout(() => document.addEventListener("click", handleOutside), 0);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("click", handleOutside);
    };
  }, [isLocked, unlock]);

  useEffect(() => {
    return () => {
      if (globalUnlock === unlock) globalUnlock = null;
    };
  }, [unlock]);

  const handleTagClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (isLocked) {
      unlock();
    } else {
      if (globalUnlock && globalUnlock !== unlock) globalUnlock();
      setIsLocked(true);
      globalUnlock = unlock;
    }
  };

  return (
    <span className="relative inline-flex items-baseline">
      <span
        ref={tagRef}
        className="ref-tag cursor-pointer text-indigo-600 font-medium hover:text-indigo-800 transition-colors"
        onMouseEnter={() => setIsHovered(true)}
        onMouseLeave={() => setIsHovered(false)}
        onClick={handleTagClick}
      >
        {tagLabel}
      </span>
      {showTooltip && createPortal(
        <div
          ref={tooltipRef}
          className="ref-tooltip w-[340px] max-w-[calc(100vw-2rem)] rounded-xl border border-slate-200 bg-white shadow-2xl"
          style={{ position: "fixed", zIndex: 9999, visibility: "hidden", left: 0, top: 0 }}
          onMouseEnter={() => setIsTooltipHovered(true)}
          onMouseLeave={() => setIsTooltipHovered(false)}
          onClick={(e) => e.stopPropagation()}
        >
          {children}
        </div>,
        document.body
      )}
    </span>
  );
}

function ReferenceTag({
  index,
  label,
  content,
  paperName,
  headerPath,
}: {
  index: number;
  label: string;
  content: string;
  paperName: string;
  headerPath: string;
}) {
  return (
    <HoverCardTag tagLabel={`${label}${index}`}>
      <div className="flex items-start gap-2.5 px-4 pt-4 pb-2.5 border-b border-slate-100">
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-indigo-50 mt-0.5">
          <FileText size={14} className="text-indigo-500" />
        </div>
        <div className="min-w-0 flex-1">
          <h4 className="text-sm font-semibold text-slate-900 leading-snug break-words">{paperName}</h4>
          <p className="mt-0.5 text-sm text-indigo-500 font-medium break-words">{headerPath}</p>
        </div>
      </div>
      <div className="px-4 py-3">
        <p className="text-[15px] text-slate-600 leading-relaxed line-clamp-6">
          {content}
        </p>
      </div>
    </HoverCardTag>
  );
}

function ImageReferenceTag({
  label,
  index,
  image,
}: {
  label: string;
  index: number;
  image: ImageReference;
}) {
  return (
    <HoverCardTag tagLabel={`${label}${index}`}>
      <div className="flex items-start gap-2.5 px-4 pt-4 pb-2.5 border-b border-slate-100">
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-indigo-50 mt-0.5">
          <ImageIcon size={14} className="text-indigo-500" />
        </div>
        <div className="min-w-0 flex-1">
          <h4 className="text-sm font-semibold text-slate-900 leading-snug break-words">{image.paperName || label}</h4>
          {image.headerPath ? (
            <p className="mt-0.5 text-sm text-indigo-500 font-medium break-words">{image.headerPath}</p>
          ) : null}
        </div>
      </div>
      <div className="px-4 py-3">
        <div className="overflow-hidden rounded-lg border border-slate-100 bg-slate-50">
          <img
            src={proxyImg(image.imagePath)}
            alt={image.caption || label}
            className="max-h-56 w-full object-contain"
            loading="lazy"
          />
        </div>
        {image.caption ? (
          <p className="mt-2 text-[15px] text-slate-600 leading-relaxed line-clamp-3">
            {image.caption}
          </p>
        ) : null}
      </div>
    </HoverCardTag>
  );
}

function InlineReferenceText({ index, prefix }: { index: number; prefix: string }) {
  return (
    <span className="ref-tag cursor-default text-indigo-600 font-medium">
      {prefix}{index}
    </span>
  );
}

function SiteIcon({ site }: { site?: string }) {
  const [failed, setFailed] = useState(false);
  if (!site || failed) return <Globe size={14} className="text-indigo-500" />;
  return (
    <img
      src={`https://${site}/favicon.ico`}
      alt=""
      className="h-4 w-4 rounded-sm object-contain"
      onError={() => setFailed(true)}
    />
  );
}

function WebReferenceTag({ index, source }: { index: number; source: WebSourceRef }) {
  return (
    <HoverCardTag
      tagLabel={
        <sup className="mx-0.5 inline-flex min-w-[16px] items-center justify-center rounded-[5px] bg-indigo-50 px-1 text-[11px] font-semibold text-indigo-600">
          {index}
        </sup>
      }
    >
      <div className="flex items-start gap-2.5 px-4 pt-4 pb-2.5 border-b border-slate-100">
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-indigo-50 mt-0.5 overflow-hidden">
          <SiteIcon site={source.site} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-xs text-slate-400">
            {source.site ? <span className="truncate">{source.site}</span> : null}
            {source.date ? <span className="shrink-0">{source.date}</span> : null}
          </div>
          <a
            href={source.url}
            target="_blank"
            rel="noreferrer"
            className="mt-0.5 block text-sm font-semibold text-slate-900 leading-snug break-words hover:text-indigo-600"
          >
            {source.title}
          </a>
        </div>
      </div>
      {source.snippet ? (
        <div className="px-4 py-3">
          <p className="text-[15px] text-slate-600 leading-relaxed line-clamp-5">{source.snippet}</p>
        </div>
      ) : null}
    </HoverCardTag>
  );
}

export default function AIAnswerRenderer({
  answer,
  literatureResults,
  imageResults = [],
  experimentalImageResults = [],
  fallbackPrefix = "文献",
  webSources = [],
}: {
  answer: string;
  literatureResults: AnswerReferenceResult[];
  imageResults?: ImageReference[];
  experimentalImageResults?: ImageReference[];
  fallbackPrefix?: string;
  webSources?: WebSourceRef[];
}) {
  const { locale, t } = useLocale();
  const referenceLabel = fallbackPrefix === "文献" ? t("literatureReference") : fallbackPrefix;

  const preprocessAnswer = (text: string): string => {
    if (webSources.length) {
      text = text.replace(/\[(\d+)\]/g, (m, n) => {
        const i = parseInt(n, 10);
        return i >= 1 && i <= webSources.length ? `AIWEBTOKEN${i}AIWEBEND` : m;
      });
    }
    text = text.replace(/(?:文献|参考资料|成分标准|references?|standards?)\s*(\d+)(?:[、,，]\s*\d+)+/gi, (match) => {
      const nums = match.match(/\d+/g) || [];
      return nums.map(n => `AIREFTOKEN${n}AIREFEND`).join(locale === "en-US" ? ", " : "、");
    });
    text = text.replace(/(?:文献|参考资料|成分标准|references?|standards?)\s*(\d+)/gi, "AIREFTOKEN$1AIREFEND");
    text = text.replace(/(?:金相照片|metallography\s*photos?|metallography)(\d+)(?:[、,，]\s*\d+)+/gi, (match) => {
      const nums = match.match(/\d+/g) || [];
      return nums.map(n => `AIMETALTOKEN${n}AIMETALEND`).join(locale === "en-US" ? ", " : "、");
    });
    text = text.replace(/(?:金相照片|metallography\s*photos?|metallography)\s*(\d+)/gi, "AIMETALTOKEN$1AIMETALEND");
    text = text.replace(/(?:图片|images?)\s*(\d+)(?:[、,，]\s*\d+)+/gi, (match) => {
      const nums = match.match(/\d+/g) || [];
      return nums.map(n => `AIIMGTOKEN${n}AIIMGEND`).join(locale === "en-US" ? ", " : "、");
    });
    text = text.replace(/(?:图片|images?)\s*(\d+)/gi, "AIIMGTOKEN$1AIIMGEND");
    return text;
  };

  const postprocessContent = (content: ReactNode): ReactNode => {
    if (typeof content === "string") {
      const parts = content.split(/(AIREFTOKEN\d+AIREFEND|AIIMGTOKEN\d+AIIMGEND|AIMETALTOKEN\d+AIMETALEND|AIWEBTOKEN\d+AIWEBEND)/g);
      return parts.map((part, idx) => {
        const webMatch = part.match(/^AIWEBTOKEN(\d+)AIWEBEND$/);
        if (webMatch) {
          const webIndex = parseInt(webMatch[1], 10);
          const src = webSources[webIndex - 1];
          if (src) {
            return <WebReferenceTag key={idx} index={webIndex} source={src} />;
          }
          return null;
        }
        const refMatch = part.match(/^AIREFTOKEN(\d+)AIREFEND$/);
        if (refMatch) {
          const refIndex = parseInt(refMatch[1], 10);
          const lit = literatureResults[refIndex - 1];
          if (lit) {
            return (
              <ReferenceTag
                key={idx}
                index={refIndex}
                label={referenceLabel}
                content={lit.content}
                paperName={lit.paper_name}
                headerPath={lit.header_path}
              />
            );
          }
          return <InlineReferenceText key={idx} index={refIndex} prefix={referenceLabel} />;
        }
        const imgMatch = part.match(/^AIIMGTOKEN(\d+)AIIMGEND$/);
        if (imgMatch) {
          const imgIndex = parseInt(imgMatch[1], 10);
          const img = imageResults[imgIndex - 1];
          if (img) {
            return <ImageReferenceTag key={idx} label={t("imageLabel")} index={imgIndex} image={img} />;
          }
          return <InlineReferenceText key={idx} index={imgIndex} prefix={t("imageLabel")} />;
        }
        const metalMatch = part.match(/^AIMETALTOKEN(\d+)AIMETALEND$/);
        if (metalMatch) {
          const metalIndex = parseInt(metalMatch[1], 10);
          const img = experimentalImageResults[metalIndex - 1];
          if (img) {
            return <ImageReferenceTag key={idx} label={t("metallographyLabel")} index={metalIndex} image={img} />;
          }
          return <InlineReferenceText key={idx} index={metalIndex} prefix={t("metallographyLabel")} />;
        }
        return part;
      });
    }
    if (Array.isArray(content)) {
      return content.map((child, idx) => (
        <span key={idx}>{postprocessContent(child)}</span>
      ));
    }
    return content;
  };

  const components: Components = {
    p: ({ children }) => {
      return <p>{postprocessContent(children)}</p>;
    },
    li: ({ children }) => {
      return <li>{postprocessContent(children)}</li>;
    },
    strong: ({ children }) => {
      return <strong>{postprocessContent(children)}</strong>;
    },
    em: ({ children }) => {
      return <em>{postprocessContent(children)}</em>;
    },
    del: ({ children }) => {
      return <span>{postprocessContent(children)}</span>;
    },
    a: ({ children, href }) => {
      return (
        <a href={href} target="_blank" rel="noreferrer">
          {postprocessContent(children)}
        </a>
      );
    },
    td: ({ children }) => {
      return <td>{postprocessContent(children)}</td>;
    },
    th: ({ children }) => {
      return <th>{postprocessContent(children)}</th>;
    },
  };

  const processedAnswer = preprocessAnswer(answer);

  return <ReactMarkdown remarkPlugins={remarkGfmNoSingleTilde} components={components}>{processedAnswer}</ReactMarkdown>;
}
