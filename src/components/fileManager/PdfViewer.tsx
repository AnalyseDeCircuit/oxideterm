/**
 * PdfViewer Component
 *
 * Cross-platform PDF viewer using pdf.js — renders PDF pages as canvases,
 * styled to resemble macOS Preview with page shadows, gray background,
 * and smooth scrolling.
 *
 * Optimisations:
 *  - Only visible pages (+ 300px buffer) mount <canvas>; others use a
 *    lightweight placeholder to keep scroll height stable.
 *  - Page-indicator scroll calculation uses pre-computed cumulative offsets
 *    with binary search (O(log N)) instead of DOM traversal.
 *  - rAF-throttled scroll handler prevents main-thread jank.
 */

import React, { useRef, useEffect, useState, useCallback, useMemo } from 'react';
import * as pdfjsLib from 'pdfjs-dist';
import { cn } from '../../lib/utils';

// Configure pdf.js worker via static import (Vite-friendly)
pdfjsLib.GlobalWorkerOptions.workerSrc = new URL(
  'pdfjs-dist/build/pdf.worker.min.mjs',
  import.meta.url,
).toString();

// ── Types ────────────────────────────────────────────────────────────────────

interface PdfViewerProps {
  /** data:application/pdf;base64,... or raw base64 string */
  data?: string;
  /** asset:// URL to stream PDF from disk (preferred over data) */
  url?: string;
  /** Filename for accessibility */
  name?: string;
  /** Zoom level (1 = 100%) */
  zoom?: number;
  /** Called when zoom changes via pinch/ctrl+wheel */
  onZoomChange?: (zoom: number) => void;
  className?: string;
}

interface PageInfo {
  pageNum: number;
  width: number;
  height: number;
}

// ── Constants ────────────────────────────────────────────────────────────────

/** Gap between pages (matches gap-3 = 12px) */
const PAGE_GAP = 12;
/** Vertical padding at top/bottom of scroll area (matches py-4 = 16px) */
const PAGE_PAD = 16;

// ── Single Page ──────────────────────────────────────────────────────────────

const PdfPage: React.FC<{
  doc: pdfjsLib.PDFDocumentProxy;
  pageNum: number;
  width: number;
  height: number;
  zoom: number;
}> = ({ doc, pageNum, width, height, zoom }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const renderTaskRef = useRef<{ cancel: () => void } | null>(null);
  const renderedZoomRef = useRef<number | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    if (renderedZoomRef.current === zoom) return;

    // Cancel previous render
    if (renderTaskRef.current) {
      renderTaskRef.current.cancel();
      renderTaskRef.current = null;
    }

    let cancelled = false;

    (async () => {
      try {
        const page = await doc.getPage(pageNum);
        if (cancelled) return;

        const dpr = window.devicePixelRatio || 1;
        const viewport = page.getViewport({ scale: zoom * dpr });

        canvas.width = viewport.width;
        canvas.height = viewport.height;
        canvas.style.width = `${viewport.width / dpr}px`;
        canvas.style.height = `${viewport.height / dpr}px`;

        const ctx = canvas.getContext('2d');
        if (!ctx || cancelled) return;

        const task = page.render({ canvasContext: ctx, viewport, canvas } as any);
        renderTaskRef.current = { cancel: () => task.cancel() };

        await task.promise;
        renderedZoomRef.current = zoom;
        renderTaskRef.current = null;
      } catch (err: unknown) {
        if (
          err &&
          typeof err === 'object' &&
          'name' in err &&
          (err as { name: string }).name === 'RenderingCancelledException'
        )
          return;
        if (!cancelled) console.warn(`[PdfViewer] Failed to render page ${pageNum}:`, err);
      }
    })();

    return () => {
      cancelled = true;
      if (renderTaskRef.current) {
        renderTaskRef.current.cancel();
        renderTaskRef.current = null;
      }
    };
  }, [doc, pageNum, zoom]);

  // Reset rendered cache when zoom changes so re-render triggers
  useEffect(() => {
    renderedZoomRef.current = null;
  }, [zoom]);

  return (
    <div
      className="relative"
      style={{ width: width * zoom, height: height * zoom }}
    >
      <div
        className="absolute inset-0 rounded-sm"
        style={{ boxShadow: '0 2px 8px rgba(0,0,0,0.3), 0 1px 3px rgba(0,0,0,0.2)' }}
      />
      <canvas
        ref={canvasRef}
        className="block bg-white rounded-sm"
        style={{ width: width * zoom, height: height * zoom }}
      />
    </div>
  );
};

// ── Placeholder for unmounted pages ──────────────────────────────────────────

const PagePlaceholder: React.FC<{ width: number; height: number; zoom: number }> = ({
  width,
  height,
  zoom,
}) => (
  <div
    className="rounded-sm bg-zinc-700/30"
    style={{ width: width * zoom, height: height * zoom }}
  />
);

// ── Component ────────────────────────────────────────────────────────────────

export const PdfViewer: React.FC<PdfViewerProps> = ({
  data,
  url,
  name: _name,
  zoom = 1,
  onZoomChange,
  className,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [pdfDoc, setPdfDoc] = useState<pdfjsLib.PDFDocumentProxy | null>(null);
  const [pages, setPages] = useState<PageInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [currentPage, setCurrentPage] = useState(1);
  const [visiblePages, setVisiblePages] = useState<Set<number>>(new Set());
  const rafRef = useRef(0);

  // Convert data to Uint8Array for pdf.js (legacy base64 path only)
  const pdfData = useMemo(() => {
    if (!data) return null;
    try {
      let base64 = data;
      const commaIdx = data.indexOf(',');
      if (commaIdx !== -1 && data.startsWith('data:')) {
        base64 = data.slice(commaIdx + 1);
      }
      const raw = atob(base64);
      const arr = new Uint8Array(raw.length);
      for (let i = 0; i < raw.length; i++) {
        arr[i] = raw.charCodeAt(i);
      }
      return arr;
    } catch {
      return null;
    }
  }, [data]);

  // Pre-computed cumulative top offsets for binary-search page lookup
  const pageOffsets = useMemo(() => {
    // offsets[i] = top of page i (0-indexed) in the scroll area
    const offsets: number[] = [];
    let y = PAGE_PAD;
    for (const p of pages) {
      offsets.push(y);
      y += p.height * zoom + PAGE_GAP;
    }
    return offsets;
  }, [pages, zoom]);

  // Load PDF document
  useEffect(() => {
    // Determine how to load: url (direct streaming) or data (decoded base64)
    if (!url && !pdfData) {
      setError('Failed to decode PDF data');
      return;
    }

    setError(null);
    let cancelled = false;
    const loadingTask = url
      ? pdfjsLib.getDocument({ url })
      : pdfjsLib.getDocument({ data: pdfData!.slice(0) });

    loadingTask.promise
      .then(async (doc) => {
        if (cancelled) { doc.destroy(); return; }

        // Collect page dimensions (getPage is cheap; no cleanup needed here)
        const infos: PageInfo[] = [];
        for (let i = 1; i <= doc.numPages; i++) {
          if (cancelled) break;
          const page = await doc.getPage(i);
          const vp = page.getViewport({ scale: 1 });
          infos.push({ pageNum: i, width: vp.width, height: vp.height });
        }

        if (!cancelled) {
          setPdfDoc(doc);
          setPages(infos);
          setError(null);
          // Mark first few pages visible so they render immediately
          const initial = new Set<number>();
          for (let i = 1; i <= Math.min(3, doc.numPages); i++) initial.add(i);
          setVisiblePages(initial);
        } else {
          doc.destroy();
        }
      })
      .catch((err) => {
        if (!cancelled) setError(String(err?.message ?? err));
      });

    return () => {
      cancelled = true;
      loadingTask.destroy();
    };
  }, [url, pdfData]);

  // IntersectionObserver — track which pages are in/near the viewport
  useEffect(() => {
    if (pages.length === 0) return;
    const container = containerRef.current;
    if (!container) return;

    const observer = new IntersectionObserver(
      (entries) => {
        setVisiblePages((prev) => {
          const next = new Set(prev);
          let changed = false;
          for (const entry of entries) {
            const pageNum = Number(entry.target.getAttribute('data-page'));
            if (!pageNum) continue;
            if (entry.isIntersecting && !next.has(pageNum)) {
              next.add(pageNum);
              changed = true;
            } else if (!entry.isIntersecting && next.has(pageNum)) {
              next.delete(pageNum);
              changed = true;
            }
          }
          return changed ? next : prev;
        });
      },
      { root: container, rootMargin: '300px' },
    );

    const wrappers = container.querySelectorAll('[data-page]');
    wrappers.forEach((el) => observer.observe(el));

    return () => observer.disconnect();
  }, [pages]);

  // Track current page on scroll — binary search + rAF throttle
  const onScroll = useCallback(() => {
    if (rafRef.current) return; // rAF already scheduled
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = 0;
      const container = containerRef.current;
      if (!container || pageOffsets.length === 0) return;
      const cutoff = container.scrollTop + container.clientHeight / 3;

      // Binary search: find the last page whose top <= cutoff
      let lo = 0;
      let hi = pageOffsets.length - 1;
      let result = 0;
      while (lo <= hi) {
        const mid = (lo + hi) >>> 1;
        if (pageOffsets[mid] <= cutoff) {
          result = mid;
          lo = mid + 1;
        } else {
          hi = mid - 1;
        }
      }
      setCurrentPage(result + 1); // 1-indexed
    });
  }, [pageOffsets]);

  // Clean up rAF on unmount
  useEffect(() => {
    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, []);

  // Pinch-to-zoom (Ctrl+wheel / trackpad pinch gesture)
  const onWheel = useCallback(
    (e: React.WheelEvent) => {
      if (!e.ctrlKey || !onZoomChange) return;
      e.preventDefault();
      const delta = -e.deltaY * 0.01;
      onZoomChange(Math.max(0.25, Math.min(5, zoom + delta)));
    },
    [zoom, onZoomChange],
  );

  // Also attach native listener for { passive: false } to prevent browser zoom
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !onZoomChange) return;
    const handler = (e: WheelEvent) => {
      if (e.ctrlKey) e.preventDefault();
    };
    container.addEventListener('wheel', handler, { passive: false });
    return () => container.removeEventListener('wheel', handler);
  }, [onZoomChange]);

  // Cleanup on unmount
  useEffect(() => {
    return () => { pdfDoc?.destroy(); };
  }, [pdfDoc]);

  // ── Error state ────────────────────────────────────────────────────────────

  if (error) {
    return (
      <div className={cn('flex-1 flex items-center justify-center text-theme-text-muted', className)}>
        <div className="text-center">
          <p className="text-sm mb-1">Failed to load PDF</p>
          <p className="text-xs opacity-60">{error}</p>
        </div>
      </div>
    );
  }

  // ── Loading state ──────────────────────────────────────────────────────────

  if (!pdfDoc || pages.length === 0) {
    return (
      <div className={cn('flex-1 flex items-center justify-center', className)} style={{ backgroundColor: '#525659' }}>
        <div className="text-white/60 text-sm animate-pulse">Loading PDF…</div>
      </div>
    );
  }

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className={cn('relative flex-1 flex flex-col min-h-0', className)}>
      <div
        ref={containerRef}
        className="flex-1 overflow-auto min-h-0"
        style={{ backgroundColor: '#525659' }}
        onScroll={onScroll}
        onWheel={onWheel}
      >
        <div className="flex flex-col items-center py-4 gap-3">
          {pages.map((p) => (
            <div key={p.pageNum} data-page={p.pageNum}>
              {visiblePages.has(p.pageNum) ? (
                <PdfPage
                  doc={pdfDoc}
                  pageNum={p.pageNum}
                  width={p.width}
                  height={p.height}
                  zoom={zoom}
                />
              ) : (
                <PagePlaceholder width={p.width} height={p.height} zoom={zoom} />
              )}
            </div>
          ))}
        </div>
      </div>

      {/* Page indicator — macOS style pill */}
      {pages.length > 1 && (
        <div className="absolute bottom-3 left-1/2 -translate-x-1/2 z-10">
          <div className="px-3 py-1 rounded-full bg-black/60 backdrop-blur-sm text-white/90 text-xs font-medium tabular-nums select-none shadow-lg">
            {currentPage} / {pages.length}
          </div>
        </div>
      )}
    </div>
  );
};
