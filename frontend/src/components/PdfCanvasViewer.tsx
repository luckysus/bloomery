import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Minus, Plus } from "lucide-react";
import { useLocale } from "../i18n/locale";
// legacy 构建自带旧内核兼容垫片：现代构建用了 Uint8Array.toHex 等新 API，
// 老一点的手机浏览器会报 "toHex is not a function" 导致预览失败。
import * as pdfjsLib from "pdfjs-dist/legacy/build/pdf.mjs";
import type { PDFDocumentLoadingTask, PDFDocumentProxy, RenderTask } from "pdfjs-dist";
import pdfWorkerUrl from "pdfjs-dist/legacy/build/pdf.worker.mjs?worker&url";

type PdfCanvasViewerProps = {
  url: string;
  title: string;
};

const MIN_ZOOM = 0.5;
const MAX_ZOOM = 3;
const ZOOM_STEP = 0.1;
const OVERSCAN_PAGES = 2;

pdfjsLib.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;

export default function PdfCanvasViewer({ url, title }: PdfCanvasViewerProps) {
  const { t } = useLocale();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const pageCanvasRefs = useRef(new Map<number, HTMLCanvasElement>());
  const renderTasksRef = useRef<RenderTask[]>([]);
  const scrollFrameRef = useRef<number | null>(null);
  const [pdfDocument, setPdfDocument] = useState<PDFDocumentProxy | null>(null);
  const [pageNumber, setPageNumber] = useState(1);
  const [zoom, setZoom] = useState(1);
  const [containerWidth, setContainerWidth] = useState(0);
  const [pageAspectRatio, setPageAspectRatio] = useState(1.414);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const updateWidth = () => setContainerWidth(container.clientWidth);
    updateWidth();

    const resizeObserver = new ResizeObserver(updateWidth);
    resizeObserver.observe(container);
    return () => resizeObserver.disconnect();
  }, []);

  useEffect(() => {
    let cancelled = false;
    let loadingTask: PDFDocumentLoadingTask | null = null;
    let loadedDocument: PDFDocumentProxy | null = null;

    setLoading(true);
    setError("");
    setPdfDocument(null);
    setPageNumber(1);
    setZoom(1);
    pageCanvasRefs.current.clear();
    renderTasksRef.current.forEach(task => task.cancel());
    renderTasksRef.current = [];

    async function loadPdf() {
      try {
        loadingTask = pdfjsLib.getDocument({
          url,
          withCredentials: true,
          disableAutoFetch: false,
          disableStream: false,
        });
        loadedDocument = await loadingTask.promise;
      } catch (firstError: any) {
        if (cancelled || firstError?.name === "PasswordException") return;
        // 部分手机浏览器/代理链路对 Range 分段请求支持不佳，降级为整文件一次性下载重试。
        console.warn("PDF streaming load failed, retrying without range/stream", firstError);
        try {
          void loadingTask?.destroy();
          loadingTask = pdfjsLib.getDocument({
            url,
            withCredentials: true,
            disableAutoFetch: true,
            disableStream: true,
            disableRange: true,
          });
          loadedDocument = await loadingTask.promise;
        } catch (retryError: any) {
          if (cancelled || retryError?.name === "PasswordException") return;
          console.error("Failed to load PDF preview", retryError);
          const detail = String(retryError?.message || retryError?.name || "").slice(0, 120);
          setError(detail ? `${t("pdfPreviewFailed")}: ${detail}` : t("pdfPreviewFailed"));
          return;
        }
      } finally {
        if (!cancelled) setLoading(false);
      }

      try {
        if (cancelled || !loadedDocument) return;
        const firstPage = await loadedDocument.getPage(1);
        const viewport = firstPage.getViewport({ scale: 1 });
        if (!cancelled && viewport.width > 0) {
          setPageAspectRatio(viewport.height / viewport.width);
        }
        if (!cancelled) setPdfDocument(loadedDocument);
      } catch (pageError: any) {
        if (cancelled) return;
        console.error("Failed to read first PDF page", pageError);
        setError(t("pdfPreviewFailed"));
      }
    }

    void loadPdf();

    return () => {
      cancelled = true;
      if (scrollFrameRef.current !== null) {
        cancelAnimationFrame(scrollFrameRef.current);
        scrollFrameRef.current = null;
      }
      renderTasksRef.current.forEach(task => task.cancel());
      renderTasksRef.current = [];
      void loadingTask?.destroy();
      void loadedDocument?.destroy();
    };
  }, [t, url]);

  const totalPages = pdfDocument?.numPages || 0;
  const zoomLabel = useMemo(() => `${Math.round(zoom * 100)}%`, [zoom]);
  const pages = useMemo(
    () => Array.from({ length: totalPages }, (_, index) => index + 1),
    [totalPages],
  );
  const pagesToRender = useMemo(() => {
    if (!totalPages) return [];
    const start = Math.max(1, pageNumber - OVERSCAN_PAGES);
    const end = Math.min(totalPages, pageNumber + OVERSCAN_PAGES);
    return Array.from({ length: end - start + 1 }, (_, index) => start + index);
  }, [pageNumber, totalPages]);

  const availableWidth = Math.max(240, containerWidth - 40);
  const pageWidth = Math.floor(availableWidth * zoom);
  const pageHeight = Math.floor(pageWidth * pageAspectRatio);
  const pagesToRenderKey = pagesToRender.join(",");

  useEffect(() => {
    if (!pdfDocument || containerWidth <= 0 || pagesToRender.length === 0) return;

    let cancelled = false;
    renderTasksRef.current.forEach(task => task.cancel());
    renderTasksRef.current = [];

    async function renderPages() {
      try {
        for (const targetPageNumber of pagesToRender) {
          if (cancelled) return;
          const canvas = pageCanvasRefs.current.get(targetPageNumber);
          if (!canvas) continue;

          const page = await pdfDocument!.getPage(targetPageNumber);
          if (cancelled) return;

          const baseViewport = page.getViewport({ scale: 1 });
          const scale = (availableWidth / baseViewport.width) * zoom;
          const viewport = page.getViewport({ scale });
          const outputScale = window.devicePixelRatio || 1;
          const context = canvas.getContext("2d");
          if (!context) continue;

          canvas.width = Math.floor(viewport.width * outputScale);
          canvas.height = Math.floor(viewport.height * outputScale);
          canvas.style.width = `${Math.floor(viewport.width)}px`;
          canvas.style.height = `${Math.floor(viewport.height)}px`;

          context.setTransform(outputScale, 0, 0, outputScale, 0, 0);
          context.clearRect(0, 0, viewport.width, viewport.height);
          const renderTask = page.render({ canvas, canvasContext: context, viewport });
          renderTasksRef.current.push(renderTask);
          await renderTask.promise;
        }
      } catch (renderError: any) {
        if (!cancelled && renderError?.name !== "RenderingCancelledException") {
          console.error("Failed to render PDF page", renderError);
          setError(t("pdfPageRenderFailed"));
        }
      }
    }

    void renderPages();

    return () => {
      cancelled = true;
      renderTasksRef.current.forEach(task => task.cancel());
      renderTasksRef.current = [];
    };
  }, [availableWidth, containerWidth, pagesToRenderKey, pdfDocument, t, zoom]);

  const setPageCanvas = (targetPage: number, canvas: HTMLCanvasElement | null) => {
    if (canvas) {
      pageCanvasRefs.current.set(targetPage, canvas);
    } else {
      pageCanvasRefs.current.delete(targetPage);
    }
  };

  const scrollToPage = (targetPage: number) => {
    const canvas = pageCanvasRefs.current.get(targetPage);
    canvas?.parentElement?.scrollIntoView({ behavior: "smooth", block: "start" });
    setPageNumber(targetPage);
  };

  const updatePageFromScroll = () => {
    const scroller = scrollRef.current;
    if (!scroller || pages.length === 0) return;

    const scrollerTop = scroller.getBoundingClientRect().top;
    let nearestPage = pageNumber;
    let nearestDistance = Number.POSITIVE_INFINITY;
    for (const targetPage of pages) {
      const pageElement = pageCanvasRefs.current.get(targetPage)?.parentElement;
      if (!pageElement) continue;
      const distance = Math.abs(pageElement.getBoundingClientRect().top - scrollerTop - 44);
      if (distance < nearestDistance) {
        nearestDistance = distance;
        nearestPage = targetPage;
      }
    }
    if (nearestPage !== pageNumber) setPageNumber(nearestPage);
  };

  const handleScroll = () => {
    if (scrollFrameRef.current !== null) return;
    scrollFrameRef.current = requestAnimationFrame(() => {
      scrollFrameRef.current = null;
      updatePageFromScroll();
    });
  };

  const goPrevious = () => {
    scrollToPage(Math.max(1, pageNumber - 1));
  };

  const goNext = () => {
    scrollToPage(totalPages ? Math.min(totalPages, pageNumber + 1) : pageNumber + 1);
  };

  const zoomOut = () => {
    setZoom(current => Math.max(MIN_ZOOM, Number((current - ZOOM_STEP).toFixed(2))));
  };

  const zoomIn = () => {
    setZoom(current => Math.min(MAX_ZOOM, Number((current + ZOOM_STEP).toFixed(2))));
  };

  return (
    <div ref={containerRef} className="relative h-full min-h-[75dvh] overflow-hidden bg-[#f5f0e8]" aria-label={title}>
      <div className="absolute left-1/2 top-2 z-20 flex -translate-x-1/2 items-center gap-1 rounded-lg bg-white/95 px-2 py-1.5 text-xs text-slate-500 shadow-lg ring-1 ring-slate-200 backdrop-blur md:left-auto md:right-3 md:translate-x-0">
        <button
          type="button"
          onClick={goPrevious}
          disabled={pageNumber <= 1 || loading}
          className="rounded p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900 disabled:cursor-not-allowed disabled:opacity-35"
          aria-label={t("previousPage")}
        >
          <ChevronLeft className="h-3.5 w-3.5" />
        </button>
        <span className="min-w-12 text-center tabular-nums text-slate-600">
          {pageNumber} / {totalPages || "?"}
        </span>
        <button
          type="button"
          onClick={goNext}
          disabled={loading || Boolean(totalPages && pageNumber >= totalPages)}
          className="rounded p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900 disabled:cursor-not-allowed disabled:opacity-35"
          aria-label={t("nextPage")}
        >
          <ChevronRight className="h-3.5 w-3.5" />
        </button>
        <span className="mx-1 h-4 w-px bg-slate-200" />
        <button
          type="button"
          onClick={zoomOut}
          disabled={zoom <= MIN_ZOOM || loading}
          className="rounded p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900 disabled:cursor-not-allowed disabled:opacity-35"
          aria-label={t("zoomOut")}
        >
          <Minus className="h-3.5 w-3.5" />
        </button>
        <span className="min-w-12 text-center tabular-nums text-slate-500">{zoomLabel}</span>
        <button
          type="button"
          onClick={zoomIn}
          disabled={zoom >= MAX_ZOOM || loading}
          className="rounded p-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900 disabled:cursor-not-allowed disabled:opacity-35"
          aria-label={t("zoomIn")}
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      </div>

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="h-full overflow-auto px-5 pb-5 pt-12 max-md:px-2"
        style={{ touchAction: "pan-x pan-y pinch-zoom", WebkitOverflowScrolling: "touch" }}
      >
        {error ? (
          <div className="mx-auto mt-20 max-w-sm rounded-lg border border-red-100 bg-white px-4 py-6 text-center text-sm text-red-500">
            {error}
          </div>
        ) : (
          <div className="mx-auto flex min-h-full w-max min-w-full flex-col items-center gap-4">
            {pages.map(targetPage => (
              <div
                key={targetPage}
                className="bg-white shadow-sm"
                style={{ width: pageWidth || undefined, minHeight: pageHeight || 160 }}
              >
                <canvas
                  ref={canvas => setPageCanvas(targetPage, canvas)}
                  className="block bg-white"
                  aria-label={t("pageAria", { title, page: targetPage })}
                />
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
