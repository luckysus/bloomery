import { useEffect, useMemo, useState } from "react";
import { FileText, Pencil, Trash2, X } from "lucide-react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeRaw from "rehype-raw";
import rehypeKatex from "rehype-katex";
import "katex/dist/katex.min.css";
import { getLiteratureRawUrl, getLiteratureImageUrl, type LiteratureFileInfo, type LiteratureFilePreview } from "../services/literature";
import RawDocumentViewer from "./RawDocumentViewer";

type KnowledgeBaseDetailProps = {
  folderName: string;
  files: LiteratureFileInfo[];
  loading: boolean;
  selectedFile: string;
  setSelectedFile: (filename: string) => void;
  preview: LiteratureFilePreview | null;
  previewLoading: boolean;
  onRenameFile: (filename: string, newFilename: string) => void | Promise<void>;
  onDeleteFile: (filename: string) => void | Promise<void>;
};

export default function KnowledgeBaseDetail({
  folderName,
  files,
  loading,
  selectedFile,
  setSelectedFile,
  preview,
  previewLoading,
  onRenameFile,
  onDeleteFile,
}: KnowledgeBaseDetailProps) {
  const [showRawPdf, setShowRawPdf] = useState(false);
  const [searchText, setSearchText] = useState("");
  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<LiteratureFileInfo | null>(null);

  const filteredFiles = useMemo(() => {
    const keyword = searchText.trim().toLowerCase();
    if (!keyword) return files;
    return files.filter(file => file.name.toLowerCase().includes(keyword));
  }, [files, searchText]);

  const currentFile = files.find(file => file.name === selectedFile) || files[0];
  const blocks = preview?.blocks || [];
  const pdfUrl = currentFile ? getLiteratureRawUrl(folderName, currentFile.name) : "";

  const markdownComponents = useMemo<Components>(() => {
    const resolveImageSrc = (src?: string) => {
      if (!src) return src || "";
      if (/^(https?:|data:)/i.test(src)) return src;
      const rel = src.replace(/^\.\//, "").replace(/^\//, "");
      if (!currentFile) return src;
      return getLiteratureImageUrl(folderName, currentFile.name, rel);
    };
    return {
      img: ({ src, alt }) => {
        const rawSrc = typeof src === "string" ? src : "";
        // 论文 PDF 常混入 CrossMark（Check for updates）等装饰徽标，MinerU 会当作图片抽出，过滤掉
        if (isJunkImage(rawSrc, alt)) return null;
        return (
          <img
            src={resolveImageSrc(rawSrc)}
            alt={alt || ""}
            loading="lazy"
            className="mx-auto my-3 max-w-full rounded-lg border border-[#eadccf] bg-white shadow-sm"
          />
        );
      },
    };
  }, [folderName, currentFile?.name]);

  // MinerU 从 PDF 抽取的文本常混入 C0/C1 控制字符（如 U+0082），浏览器会渲染成黑色方块，渲染前剔除
  const previewMarkdown = useMemo(
    () => (preview?.content || "").replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F]/g, ""),
    [preview?.content],
  );

  useEffect(() => {
    setShowRawPdf(false);
    setRenaming(false);
    setRenameValue("");
    setDeleteTarget(null);
  }, [currentFile?.name]);

  // 窄屏（<768px）没有并排空间：原始文档与解析结果改为上下堆叠，
  // 否则 minmax(420px,1fr)×2 至少需要 840px，会把手机屏撑爆导致无法滚动。
  const [isNarrow, setIsNarrow] = useState(
    () => typeof window !== "undefined" && window.matchMedia("(max-width: 767px)").matches,
  );

  useEffect(() => {
    const mql = window.matchMedia("(max-width: 767px)");
    const sync = () => setIsNarrow(mql.matches);
    mql.addEventListener("change", sync);
    return () => mql.removeEventListener("change", sync);
  }, []);

  const pdfGridTemplate = useMemo(() => {
    if (!showRawPdf) return "1fr";
    if (isNarrow) return "1fr";
    return "minmax(420px, 1fr) minmax(420px, 1fr)";
  }, [showRawPdf, isNarrow]);

  const openRename = () => {
    if (!currentFile) return;
    setRenameValue(stripPdfExtension(currentFile.name));
    setRenaming(true);
  };

  const confirmRename = async () => {
    if (!currentFile) return;
    const nextName = ensurePdfExtension(renameValue.trim());
    if (!nextName || nextName === currentFile.name) {
      setRenaming(false);
      return;
    }
    await onRenameFile(currentFile.name, nextName);
    setRenaming(false);
  };

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    await onDeleteFile(deleteTarget.name);
    setDeleteTarget(null);
  };

  if (!folderName) {
    return (
      <div className="flex h-full items-center justify-center rounded-2xl border border-dashed border-[#dfcfc0] bg-[#fffaf3] text-sm text-[#9a8b7d]">
        请选择一个知识库
      </div>
    );
  }

  return (
    <div className="relative h-full overflow-hidden rounded-2xl border border-[#e5d8cc] bg-[#fffdf8] shadow-sm max-md:min-h-[70dvh]">
      <div className="grid h-full grid-cols-[280px_minmax(0,1fr)] max-md:grid-cols-1 max-md:grid-rows-[auto_minmax(0,1fr)]">
        <aside className="min-h-0 border-r border-[#eadccf] bg-[#fffaf3] max-md:max-h-[40dvh] max-md:min-h-[26dvh] max-md:overflow-hidden max-md:border-r-0 max-md:border-b">
          <div className="p-3 max-md:p-2">
            <input
              value={searchText}
              onChange={event => setSearchText(event.target.value)}
              placeholder="搜索"
              className="h-10 w-full rounded-xl border border-[#e2d2c4] bg-[#fffdf8] px-3 text-base text-[#241b15] outline-none placeholder:text-[#a79a8d] focus:border-[#c96f52] max-md:h-9 max-md:text-sm"
            />
          </div>
          <div className="px-4 pb-2 text-xs font-semibold text-[#8b7b6e] max-md:px-3 max-md:pb-1">文档列表</div>
          <div className="h-[calc(100%-5.5rem)] overflow-auto px-3 pb-3 max-md:h-auto max-md:max-h-[calc(40dvh-4.5rem)] max-md:px-2 max-md:pb-2">
            {loading ? (
              <div className="px-2 py-6 text-sm text-[#9a8b7d]">正在加载文档...</div>
            ) : files.length === 0 ? (
              <div className="px-2 py-6 text-sm text-[#9a8b7d]">暂无文档</div>
            ) : filteredFiles.length === 0 ? (
              <div className="px-2 py-6 text-sm text-[#9a8b7d]">没有匹配的文档</div>
            ) : (
              <div className="space-y-1">
                {filteredFiles.map(file => {
                  const active = file.name === currentFile?.name;
                  return (
                    <button
                      key={file.name}
                      type="button"
                      onClick={() => setSelectedFile(file.name)}
                      className={`flex w-full items-center gap-2 rounded-xl px-2.5 py-2 text-left text-base transition-colors ${
                        active ? "bg-[#fbede3] text-[#241b15]" : "text-[#5d5046] hover:bg-[#fff7ef]"
                      }`}
                    >
                      <FileText className="h-4 w-4 shrink-0 text-[#8b6b58]" />
                      <span className="truncate">{file.name}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </aside>

        <section className="min-w-0 min-h-0 flex flex-col bg-[#f7f3ed]">
          <div className="relative flex h-14 shrink-0 items-center justify-between gap-4 border-b border-[#eadccf] bg-[#fffdf8] px-4">
            <div className="flex min-w-0 items-center gap-2 text-base text-[#241b15]">
              <FileText className="h-4 w-4 shrink-0 text-[#7b604f]" />
              <span className="truncate font-medium">{currentFile?.name || folderName}</span>
              {currentFile && (
                <button
                  type="button"
                  onClick={openRename}
                  className="rounded p-1 text-[#8a7665] hover:bg-[#f1e6dc] hover:text-[#2f261f]"
                  title="重命名"
                >
                  <Pencil className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
            {currentFile && (
              <div className="flex shrink-0 items-center gap-3 text-xs text-[#8b7b6e]">
                <button
                  type="button"
                  onClick={() => setShowRawPdf(prev => !prev)}
                  className="flex items-center gap-2 rounded px-1 py-1 text-sm text-[#7f7064] hover:text-[#2f261f]"
                >
                  <span>预览原始文档</span>
                  <span className={`relative inline-flex h-4 w-7 items-center rounded-full transition-colors ${showRawPdf ? "bg-[#c96f52]" : "bg-[#e5d8cc]"}`}>
                    <span className={`h-3 w-3 rounded-full bg-white shadow transition-transform ${showRawPdf ? "translate-x-3.5" : "translate-x-0.5"}`} />
                  </span>
                </button>
                <button
                  type="button"
                  onClick={() => setDeleteTarget(currentFile)}
                  className="rounded p-1.5 text-[#8a7665] hover:bg-red-50 hover:text-red-600"
                  title="删除文档"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            )}

            {renaming && currentFile && (
              <div className="absolute left-4 top-12 z-30 w-80 rounded-lg bg-white p-4 shadow-xl ring-1 ring-slate-200">
                <div className="mb-3 text-sm font-semibold text-slate-950">重命名</div>
                <textarea
                  value={renameValue}
                  maxLength={100}
                  onChange={event => setRenameValue(event.target.value)}
                  className="h-28 w-full resize-none rounded-lg border border-slate-200 bg-white px-3 py-2 pr-12 text-sm text-slate-900 outline-none focus:border-[#cc785c]"
                />
                <div className="mt-2 flex items-center justify-between">
                  <span className="text-xs text-slate-400">{renameValue.length}/100</span>
                  <button
                    type="button"
                    onClick={() => void confirmRename()}
                    className="rounded-lg bg-[#cc785c] px-3 py-1.5 text-sm font-semibold text-white hover:bg-[#a9583e]"
                  >
                    保存
                  </button>
                </div>
              </div>
            )}
          </div>

          {!currentFile ? (
            <div className="flex flex-1 items-center justify-center text-sm text-slate-400">暂无文档</div>
          ) : (
            <div className="min-h-0 flex-1 overflow-hidden">
              <div className="grid h-full max-md:h-auto" style={{ gridTemplateColumns: pdfGridTemplate }}>
                {showRawPdf && (
                  <div className="min-h-0 border-r border-slate-200 bg-white max-md:h-[70dvh] max-md:border-r-0 max-md:border-b">
                    <RawDocumentViewer url={pdfUrl} title={currentFile.name} />
                  </div>
                )}
                <div
                  className="min-h-0 overflow-auto bg-[#f7f3ed] p-3 max-md:h-[70dvh] max-md:p-2"
                  style={{ touchAction: "pan-x pan-y pinch-zoom", WebkitOverflowScrolling: "touch" }}
                >
                  {previewLoading ? (
                    <div className="h-full min-h-[240px]" />
                  ) : !preview?.processed ? (
                    <div className="flex h-full min-h-[240px] items-center justify-center px-8 text-center text-sm leading-6 text-[#8f8174]">
                      正在处理，完成后这里会显示 Markdown 全文。
                    </div>
                  ) : preview.content ? (
                    <div className="markdown-preview min-h-full rounded-xl border border-[#eadccf] bg-[#fffdf8] px-5 py-5 text-base leading-7 text-[#241b15] shadow-sm">
                      <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeRaw, rehypeKatex]} components={markdownComponents}>
                        {previewMarkdown}
                      </ReactMarkdown>
                    </div>
                  ) : (
                    <div className="space-y-2">
                      {blocks.map((block, index) => (
                        <div
                          key={`${currentFile.name}-${index}`}
                          className="whitespace-pre-wrap rounded-xl border border-[#eadccf] bg-[#fffdf8] px-4 py-4 text-base leading-7 text-[#241b15] shadow-sm"
                        >
                          {block.content}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}
        </section>
      </div>

      {deleteTarget && (
        <div className="absolute inset-0 z-40 flex items-center justify-center bg-slate-950/40">
          <div className="w-80 rounded-lg bg-white p-6 shadow-2xl">
            <div className="mb-5 flex items-start justify-between gap-4">
              <div>
                <div className="text-base font-semibold text-slate-950">是否确认删除？</div>
                <div className="mt-4 text-sm leading-6 text-slate-500">删除后关联智能体中的引用将失效</div>
              </div>
              <button
                type="button"
                onClick={() => setDeleteTarget(null)}
                className="rounded p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900"
                aria-label="关闭"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setDeleteTarget(null)}
                className="rounded-lg bg-slate-100 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-200"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => void confirmDelete()}
                className="rounded-lg bg-red-500 px-4 py-2 text-sm font-semibold text-white hover:bg-red-600"
              >
                删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function isJunkImage(src: string, alt?: string) {
  const hay = `${src} ${alt || ""}`.toLowerCase();
  return /crossmark|check[\s_-]*for[\s_-]*updates/.test(hay);
}

function stripPdfExtension(name: string) {
  return name.replace(/\.pdf$/i, "");
}

function ensurePdfExtension(name: string) {
  if (!name) return "";
  return name.toLowerCase().endsWith(".pdf") ? name : `${name}.pdf`;
}
