/**
 * ImageViewer — zoomable, pannable & rotatable image preview.
 *
 * Supports:
 *  - Pinch-to-zoom (trackpad) & Ctrl+Wheel zoom
 *  - Click-and-drag panning when zoomed in
 *  - Double-click to reset to fit
 *  - Optional controlled zoom & rotation from parent
 *  - Zoom clamped between 0.1× and 10×
 *  - Cursor changes: grab/grabbing when zoomed, default at 1×
 */

import React, { useRef, useState, useCallback, useEffect } from 'react';
import { cn } from '../../lib/utils';

interface ImageViewerProps {
  src: string;
  alt?: string;
  className?: string;
  /** Controlled zoom (overrides internal state) */
  zoom?: number;
  /** Called when internal zoom changes (e.g. pinch/wheel) */
  onZoomChange?: (zoom: number) => void;
  /** Rotation in degrees (0, 90, 180, 270) */
  rotation?: number;
  /** Whether to show the zoom indicator badge */
  showZoomBadge?: boolean;
}

const MIN_ZOOM = 0.1;
const MAX_ZOOM = 10;

export const ImageViewer: React.FC<ImageViewerProps> = ({
  src,
  alt,
  className,
  zoom: controlledZoom,
  onZoomChange,
  rotation = 0,
  showZoomBadge = true,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);

  // Internal zoom state (used when uncontrolled)
  const [internalZoom, setInternalZoom] = useState(1);
  const zoom = controlledZoom ?? internalZoom;
  const updateZoom = useCallback(
    (z: number) => {
      const clamped = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));
      if (onZoomChange) onZoomChange(clamped);
      else setInternalZoom(clamped);
      return clamped;
    },
    [onZoomChange],
  );

  const [offset, setOffset] = useState({ x: 0, y: 0 });

  // Drag state (refs to avoid re-renders during drag)
  const dragging = useRef(false);
  const dragStart = useRef({ x: 0, y: 0 });
  const offsetStart = useRef({ x: 0, y: 0 });

  // ── Reset when image source changes ────────────────────────────────────────
  useEffect(() => {
    setInternalZoom(1);
    setOffset({ x: 0, y: 0 });
  }, [src]);

  // ── Wheel zoom (Ctrl+wheel / trackpad pinch) ──────────────────────────────
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();

      const rect = container.getBoundingClientRect();
      const cx = e.clientX - rect.left - rect.width / 2;
      const cy = e.clientY - rect.top - rect.height / 2;

      const factor = 1 - e.deltaY * 0.01;
      const newZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom * factor));
      const scale = newZoom / zoom;

      setOffset(prev => ({
        x: cx - scale * (cx - prev.x),
        y: cy - scale * (cy - prev.y),
      }));
      updateZoom(newZoom);
    };

    container.addEventListener('wheel', onWheel, { passive: false });
    return () => container.removeEventListener('wheel', onWheel);
  }, [zoom, updateZoom]);

  // ── Double-click to reset ──────────────────────────────────────────────────
  const onDoubleClick = useCallback(() => {
    updateZoom(1);
    setOffset({ x: 0, y: 0 });
  }, [updateZoom]);

  // ── Mouse drag (pan) ──────────────────────────────────────────────────────
  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (zoom <= 1) return;
      e.preventDefault();
      dragging.current = true;
      dragStart.current = { x: e.clientX, y: e.clientY };
      offsetStart.current = { ...offset };
    },
    [zoom, offset],
  );

  useEffect(() => {
    const onMouseMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      setOffset({
        x: offsetStart.current.x + (e.clientX - dragStart.current.x),
        y: offsetStart.current.y + (e.clientY - dragStart.current.y),
      });
    };
    const onMouseUp = () => {
      dragging.current = false;
    };
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
    return () => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };
  }, []);

  const cursor = zoom <= 1 ? 'default' : dragging.current ? 'grabbing' : 'grab';

  return (
    <div
      ref={containerRef}
      className={cn('relative flex items-center justify-center h-full overflow-hidden select-none', className)}
      onMouseDown={onMouseDown}
      onDoubleClick={onDoubleClick}
      style={{ cursor }}
    >
      <img
        ref={imgRef}
        src={src}
        alt={alt}
        draggable={false}
        className="max-w-full max-h-full object-contain"
        style={{
          transform: `translate(${offset.x}px, ${offset.y}px) scale(${zoom}) rotate(${rotation}deg)`,
          transformOrigin: 'center center',
          transition: dragging.current ? 'none' : 'transform 0.15s ease-out',
        }}
      />

      {showZoomBadge && zoom !== 1 && (
        <div className="absolute bottom-3 right-3 px-2 py-1 rounded-full bg-black/60 backdrop-blur-sm text-white/90 text-xs font-medium tabular-nums select-none shadow-lg">
          {Math.round(zoom * 100)}%
        </div>
      )}
    </div>
  );
};

export default ImageViewer;
