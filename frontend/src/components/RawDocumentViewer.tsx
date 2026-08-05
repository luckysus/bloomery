import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Download, FileWarning, Minus, Plus } from "lucide-react";
import * as XLSX from "xlsx";
import { renderAsync } from "docx-preview";
import { useLocale } from "../i18n/locale";
import PdfCanvasViewer from "./PdfCanvasViewer";

type RawDocumentViewerProps = {
  url: string;
  title: string;
};

const IMAGE_EXTS = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];
const MIN_ZOOM = 0.5;
const MAX_ZOOM = 3;
const ZOOM_STEP = 0.1;

function getExtension(name: string) {
  const idx = name.lastIndexOf(".");
  return idx >= 0 ? name.slice(idx + 1).toLowerCase() : "";
}

function guessImageMime(ext: string) {
  if (ext === "jpg" || ext === "jpeg") return "image/jpeg";
  if (ext === "svg") return "image/svg+xml";
  return `image/${ext}`;
}

export default function RawDocumentViewer({ url, title }: RawDocumentViewerProps) {
  const { t } = useLocale();
  const ext = useMemo(() => getExtension(title), [title]);
  const isImage = IMAGE_EXTS.includes(ext);
  const isDocx = ext === "docx";
  const isXlsx = ext === "xlsx";
  const isPdf = ext === "pdf";
  const isPptx = ext === "pptx" || ext === "ppt";
  const needsFetch = isImage || isDocx || isXlsx;

  const [status, setStatus] = useState<"idle" | "loading" | "ready" | "error">(needsFetch ? "loading" : "ready");
  const [imageUrl, setImageUrl] = useState("");
  const [sheets, setSheets] = useState<{ name: string; html: string }[]>([]);
  const [zoom, setZoom] = useState(1);
  const blobRef = useRef<Blob | null>(null);
  const docxRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    blobRef.current = null;
    setImageUrl("");
    setSheets([]);
    setZoom(1);

    if (!needsFetch) {
      setStatus("ready");
      return;
    }

    let cancelled = false;
    let objectUrl = "";
    setStatus("loading");

    (async () => {
      try {
        const response = await fetch(url, { credentials: "include" });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const blob = await response.blob();
        if (cancelled) return;
        blobRef.current = blob;

        if (isImage) {
          objectUrl = URL.createObjectURL(new Blob([blob], { type: guessImageMime(ext) }));
          setImageUrl(objectUrl);
          setStatus("ready");
          return;
        }

        if (isXlsx) {
          const buffer = await blob.arrayBuffer();
          if (cancelled) return;
          const workbook = XLSX.read(buffer, { type: "array" });
          const parsed = workbook.SheetNames.map(name => ({
            name,
            html: XLSX.utils.sheet_to_html(workbook.Sheets[name], { editable: false }),
          }));
          setSheets(parsed);
          setStatus("ready");
          return;
        }

        if (isDocx) {
          // docx 内容在下方 effect 里等容器挂载后渲染
          setStatus("ready");
          return;
        }
      } catch (error) {
        if (!cancelled) {
          console.error("加载原始文档失败", error);
          setStatus("error");
        }
      }
    })();

    return () => {
      cancelled = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [url, ext, isImage, isDocx, isXlsx, needsFetch]);

  useEffect(() => {
    if (!isDocx || status !== "ready") return;
    const blob = blobRef.current;
    const container = docxRef.current;
    if (!blob || !container) return;
    let cancelled = false;
    container.innerHTML = "";
    renderAsync(blob, container, undefined, {
      className: "docx-preview",
      inWrapper: true,
      ignoreWidth: false,
      ignoreHeight: false,
    }).catch(error => {
      if (cancelled) return;
      console.error("DOCX 渲染失败", error);
      setStatus("error");
    });
    return () => {
      cancelled = true;
    };
  }, [isDocx, status]);

  const download = async () => {
    try {
      const blob = blobRef.current || (await (await fetch(url, { credentials: "include" })).blob());
      const objectUrl = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = objectUrl;
      anchor.download = title;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(objectUrl);
    } catch (error) {
      console.error("下载原文失败", error);
    }
  };

  // PDF 走既有的 pdf.js 渲染器（自带缩放/翻页工具条）
  if (isPdf) {
    return <PdfCanvasViewer url={url} title={title} />;
  }

  const zoomLabel = `${Math.round(zoom * 100)}%`;
  const zoomOut = () => setZoom(current => Math.max(MIN_ZOOM, Number((current - ZOOM_STEP).toFixed(2))));
  const zoomIn = () => setZoom(current => Math.min(MAX_ZOOM, Number((current + ZOOM_STEP).toFixed(2))));
  const showZoomToolbar = status === "ready" && (isImage || isDocx || isXlsx);
  const zoomStyle = { zoom } as CSSProperties;

  return (
    <div className="relative h-full overflow-hidden bg-[#f5f0e8]" aria-label={title}>
      {showZoomToolbar && (
        <div className="absolute right-3 top-2 z-20 flex items-center gap-1 rounded-lg bg-white/95 px-2 py-1.5 text-xs text-slate-500 shadow-lg ring-1 ring-slate-200 backdrop-blur">
          <button
            type="button"
            onClick={zoomOut}
            disabled={zoom <= MIN_ZOOM}
            className="rounded p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900 disabled:cursor-not-allowed disabled:opacity-35"
            aria-label={t("zoomOut")}
          >
            <Minus className="h-3.5 w-3.5" />
          </button>
          <span className="min-w-12 text-center tabular-nums text-slate-500">{zoomLabel}</span>
          <button
            type="button"
            onClick={zoomIn}
            disabled={zoom >= MAX_ZOOM}
            className="rounded p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900 disabled:cursor-not-allowed disabled:opacity-35"
            aria-label={t("zoomIn")}
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        </div>
      )}

      <div className="h-full overflow-auto px-5 pb-5 pt-12">
        {status === "loading" ? (
          <div className="mx-auto mt-20 max-w-sm text-center text-sm text-[#8f8174]">{t("documentLoading")}</div>
        ) : status === "error" ? (
          <div className="mx-auto mt-20 max-w-sm rounded-lg border border-red-100 bg-white px-4 py-6 text-center text-sm text-red-500">
            {t("documentLoadFailed")}
          </div>
        ) : isImage ? (
          <div style={zoomStyle} className="flex min-h-full items-start justify-center">
            <img src={imageUrl} alt={title} className="max-w-full rounded-lg bg-white shadow-sm" />
          </div>
        ) : isXlsx ? (
          <div style={zoomStyle} className="space-y-6">
            {sheets.map(sheet => (
              <div key={sheet.name} className="rounded-lg border border-[#eadccf] bg-white p-3 shadow-sm">
                <div className="mb-2 text-xs font-semibold text-[#8b7b6e]">{sheet.name}</div>
                <div className="xlsx-preview overflow-auto text-sm text-[#241b15]" dangerouslySetInnerHTML={{ __html: sheet.html }} />
              </div>
            ))}
          </div>
        ) : isDocx ? (
          <div style={zoomStyle}>
            <div ref={docxRef} className="mx-auto max-w-3xl rounded-lg bg-white p-2 shadow-sm" />
          </div>
        ) : isPptx ? (
          <div className="mx-auto mt-16 max-w-sm rounded-xl border border-[#eadccf] bg-white px-6 py-8 text-center shadow-sm">
            <FileWarning className="mx-auto mb-3 h-8 w-8 text-[#c99a52]" />
            <div className="text-sm font-medium text-[#241b15]">{t("pptxNoPreview")}</div>
            <div className="mt-2 text-xs leading-6 text-[#8f8174]">
              {t("documentDownloadHint")}
            </div>
            <button
              type="button"
              onClick={() => void download()}
              className="mt-4 inline-flex items-center gap-1.5 rounded-lg bg-[#cc785c] px-3 py-1.5 text-sm font-semibold text-white hover:bg-[#a9583e]"
            >
              <Download className="h-3.5 w-3.5" />
              {t("downloadOriginal")}
            </button>
          </div>
        ) : (
          <div className="mx-auto mt-16 max-w-sm text-center text-sm text-[#8f8174]">{t("unsupportedPreview")}</div>
        )}
      </div>
    </div>
  );
}
