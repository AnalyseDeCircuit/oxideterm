/**
 * VideoPlayer Component
 *
 * A clean video player that matches the OxideTerm design system.
 * Uses semantic theme colours (theme-*) throughout with custom controls.
 *
 * Features:
 *  • Custom seekbar & volume slider matching AudioVisualizer
 *  • Play / pause / skip / fullscreen controls
 *  • Duration & current time display
 *  • Picture-in-picture support
 *  • Toggleable metadata info panel (resolution, codec, file size…)
 */

import React, { useRef, useEffect, useState, useCallback } from 'react';
import {
  Play, Pause, Volume2, Volume1, VolumeX,
  SkipBack, SkipForward, Maximize, Minimize,
  PictureInPicture2, Info, Film,
} from 'lucide-react';
import { cn } from '../../lib/utils';

// ── Types ────────────────────────────────────────────────────────────────────

interface VideoPlayerProps {
  src: string;
  name: string;
  mimeType?: string;
  /** Original file path — for display */
  filePath?: string;
  /** File size in bytes */
  fileSize?: number;
}

interface VideoInfo {
  width: number;
  height: number;
  duration: number;
  /** Decoded frame rate — estimated from played frames */
  fps?: number;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const pad = (n: number) => String(Math.floor(n)).padStart(2, '0');
const fmtTime = (s: number) => {
  if (!isFinite(s) || isNaN(s)) return '0:00';
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = Math.floor(s % 60);
  if (h > 0) return `${h}:${pad(m)}:${pad(sec)}`;
  return `${m}:${pad(sec)}`;
};

/** Volume icon based on level */
const VolumeIcon: React.FC<{ volume: number; muted: boolean; className?: string }> = ({ volume, muted, className }) => {
  if (muted || volume === 0) return <VolumeX className={className} />;
  if (volume < 0.5) return <Volume1 className={className} />;
  return <Volume2 className={className} />;
};

/** Format bytes to human-readable size */
const fmtSize = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
};

/** File extension → container label */
const guessContainer = (fileName: string): string => {
  const ext = fileName.split('.').pop()?.toLowerCase() ?? '';
  const map: Record<string, string> = {
    mp4: 'MP4 / H.264', m4v: 'M4V / H.264', webm: 'WebM / VP8/VP9',
    ogv: 'OGG / Theora', mov: 'QuickTime / MOV', mkv: 'Matroska / MKV',
    avi: 'AVI', wmv: 'WMV', flv: 'FLV',
  };
  return map[ext] ?? ext.toUpperCase();
};

// ── Component ────────────────────────────────────────────────────────────────

export const VideoPlayer: React.FC<VideoPlayerProps> = ({ src, name, mimeType, filePath, fileSize }) => {
  const videoRef = useRef<HTMLVideoElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const wrapperRef = useRef<HTMLDivElement>(null);

  // State
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrent] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(0.8);
  const [muted, setMuted] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [showControls, setShowControls] = useState(true);
  const [buffered, setBuffered] = useState(0);
  const [showInfo, setShowInfo] = useState(false);
  const [videoInfo, setVideoInfo] = useState<VideoInfo | null>(null);
  /** Actual container size — measured via ResizeObserver */
  const [containerSize, setContainerSize] = useState<{ w: number; h: number } | null>(null);
  const videoWrapRef = useRef<HTMLDivElement>(null);
  const hideTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  // ── Drag-seek state ───────────────────────────────────────────────────────
  const seekBarRef = useRef<HTMLDivElement>(null);
  const [isSeeking, setIsSeeking] = useState(false);
  /** Visual-only progress shown during drag (0–100) */
  const [seekPreview, setSeekPreview] = useState<number | null>(null);
  /** Whether video was playing before drag started */
  const wasPlayingRef = useRef(false);

  // ── Measure container ─────────────────────────────────────────────────────

  useEffect(() => {
    const el = videoWrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      if (entry) {
        setContainerSize({ w: entry.contentRect.width, h: entry.contentRect.height });
      }
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ── Playback controls ─────────────────────────────────────────────────────

  const togglePlay = useCallback(() => {
    const el = videoRef.current;
    if (!el) return;
    if (el.paused) { el.play(); setPlaying(true); }
    else { el.pause(); setPlaying(false); }
  }, []);

  // ── Drag-seek handlers ──────────────────────────────────────────────────

  /** Compute ratio (0-1) from mouse position relative to seekbar */
  const getSeekRatio = useCallback((clientX: number) => {
    const bar = seekBarRef.current;
    if (!bar) return 0;
    const rect = bar.getBoundingClientRect();
    return Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
  }, []);

  /** Start drag — pause video, show preview position */
  const onSeekMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    const el = videoRef.current;
    if (!el || !isFinite(el.duration)) return;
    wasPlayingRef.current = !el.paused;
    if (!el.paused) el.pause();
    setIsSeeking(true);
    const ratio = getSeekRatio(e.clientX);
    setSeekPreview(ratio * 100);
    // Immediately seek so user sees the frame
    el.currentTime = ratio * el.duration;
  }, [getSeekRatio]);

  useEffect(() => {
    if (!isSeeking) return;
    const el = videoRef.current;

    let rafId: number | null = null;
    let latestRatio = 0;

    const onMove = (e: MouseEvent) => {
      latestRatio = getSeekRatio(e.clientX);
      setSeekPreview(latestRatio * 100);
      // Throttle actual seeks to rAF to avoid hammering the decoder
      if (rafId === null) {
        rafId = requestAnimationFrame(() => {
          rafId = null;
          if (el && isFinite(el.duration)) {
            el.currentTime = latestRatio * el.duration;
          }
        });
      }
    };

    const onUp = (e: MouseEvent) => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      const ratio = getSeekRatio(e.clientX);
      if (el && isFinite(el.duration)) {
        el.currentTime = ratio * el.duration;
      }
      setIsSeeking(false);
      setSeekPreview(null);
      if (wasPlayingRef.current && el) {
        el.play().catch(() => {});
      }
    };

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [isSeeking, getSeekRatio]);

  /** Click-to-seek (non-drag) — kept for accessibility */
  const seek = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    // If we just finished a drag, don't double-seek
    if (isSeeking) return;
    const el = videoRef.current;
    if (!el || !isFinite(el.duration)) return;
    const ratio = getSeekRatio(e.clientX);
    el.currentTime = ratio * el.duration;
  }, [isSeeking, getSeekRatio]);

  const skip = useCallback((d: number) => {
    const el = videoRef.current;
    if (el) el.currentTime = Math.max(0, Math.min(el.duration || 0, el.currentTime + d));
  }, []);

  const toggleMute = useCallback(() => {
    const el = videoRef.current;
    if (el) { el.muted = !el.muted; setMuted(el.muted); }
  }, []);

  const onVolumeChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const el = videoRef.current;
    if (!el) return;
    const v = Number(e.target.value);
    el.volume = v;
    setVolume(v);
    if (v > 0 && el.muted) { el.muted = false; setMuted(false); }
  }, []);

  const toggleFullscreen = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;
    if (!document.fullscreenElement) {
      container.requestFullscreen().catch(() => {});
    } else {
      document.exitFullscreen().catch(() => {});
    }
  }, []);

  const togglePiP = useCallback(async () => {
    const el = videoRef.current;
    if (!el) return;
    try {
      if (document.pictureInPictureElement) {
        await document.exitPictureInPicture();
      } else {
        await el.requestPictureInPicture();
      }
    } catch { /* PiP not supported */ }
  }, []);

  // ── Auto-hide controls ───────────────────────────────────────────────────

  const resetHideTimer = useCallback(() => {
    setShowControls(true);
    if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    hideTimerRef.current = setTimeout(() => {
      if (videoRef.current && !videoRef.current.paused) {
        setShowControls(false);
      }
    }, 3000);
  }, []);

  // ── Video events ──────────────────────────────────────────────────────────

  useEffect(() => {
    const el = videoRef.current;
    if (!el) return;
    el.volume = volume;

    const onTime = () => setCurrent(el.currentTime);
    const onMeta = () => {
      setDuration(el.duration);
      setVideoInfo({
        width: el.videoWidth,
        height: el.videoHeight,
        duration: el.duration,
      });
    };
    const onEnd = () => { setPlaying(false); setShowControls(true); };
    const onPlay = () => setPlaying(true);
    const onPause = () => { setPlaying(false); setShowControls(true); };
    const onProgress = () => {
      if (el.buffered.length > 0) {
        setBuffered(el.buffered.end(el.buffered.length - 1));
      }
    };

    el.addEventListener('timeupdate', onTime);
    el.addEventListener('loadedmetadata', onMeta);
    el.addEventListener('ended', onEnd);
    el.addEventListener('play', onPlay);
    el.addEventListener('pause', onPause);
    el.addEventListener('progress', onProgress);

    return () => {
      el.removeEventListener('timeupdate', onTime);
      el.removeEventListener('loadedmetadata', onMeta);
      el.removeEventListener('ended', onEnd);
      el.removeEventListener('play', onPlay);
      el.removeEventListener('pause', onPause);
      el.removeEventListener('progress', onProgress);
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Auto-focus container on mount so keyboard shortcuts work immediately
  useEffect(() => {
    // Use rAF to ensure the DOM is settled before focusing
    const id = requestAnimationFrame(() => wrapperRef.current?.focus());
    return () => cancelAnimationFrame(id);
  }, []);

  // Fullscreen change listener
  useEffect(() => {
    const onChange = () => setIsFullscreen(!!document.fullscreenElement);
    document.addEventListener('fullscreenchange', onChange);
    return () => document.removeEventListener('fullscreenchange', onChange);
  }, []);

  // Keyboard shortcuts — scoped to container via onKeyDown (not window)
  // to avoid clashing with QuickLook's own window-level shortcuts.
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.target instanceof HTMLInputElement) return;
    switch (e.key) {
      case ' ':
      case 'k':
        e.preventDefault();
        e.stopPropagation();
        togglePlay();
        break;
      case 'ArrowLeft':
        e.preventDefault();
        e.stopPropagation();
        skip(-5);
        break;
      case 'ArrowRight':
        e.preventDefault();
        e.stopPropagation();
        skip(5);
        break;
      case 'f':
        e.preventDefault();
        e.stopPropagation();
        toggleFullscreen();
        break;
      case 'm':
        e.preventDefault();
        e.stopPropagation();
        toggleMute();
        break;
    }
  }, [togglePlay, skip, toggleFullscreen, toggleMute]);

  // During drag, show the preview position; otherwise show actual playback position
  const progress = seekPreview !== null ? seekPreview : (duration > 0 ? (currentTime / duration) * 100 : 0);
  const bufferProgress = duration > 0 ? (buffered / duration) * 100 : 0;

  // ── Smart video sizing ────────────────────────────────────────────────────
  // If native res is smaller than the container, display at native size (no upscale).
  // Otherwise scale down with object-contain to fit.

  const videoStyle = React.useMemo<React.CSSProperties>(() => {
    if (isFullscreen) return { width: '100%', height: '100%', objectFit: 'contain' as const };
    if (!videoInfo?.width || !videoInfo?.height || !containerSize) {
      // Before metadata loads, fill available space
      return { maxWidth: '100%', maxHeight: '100%', objectFit: 'contain' as const };
    }
    const { width: nw, height: nh } = videoInfo;
    const { w: cw, h: ch } = containerSize;

    // If native size fits inside container — display at native pixels (crisp, no upscale)
    if (nw <= cw && nh <= ch) {
      return { width: nw, height: nh, objectFit: 'contain' as const };
    }
    // Otherwise let the browser scale down while maintaining aspect ratio
    return { maxWidth: '100%', maxHeight: '100%', objectFit: 'contain' as const };
  }, [isFullscreen, videoInfo, containerSize]);

  // ── Build info entries ────────────────────────────────────────────────────

  type InfoEntry = { label: string; value: string };
  const fileEntries: InfoEntry[] = [];
  const techEntries: InfoEntry[] = [];

  fileEntries.push({ label: 'FILE', value: name });
  if (filePath) fileEntries.push({ label: 'PATH', value: filePath });
  if (fileSize) fileEntries.push({ label: 'SIZE', value: fmtSize(fileSize) });
  fileEntries.push({ label: 'FORMAT', value: guessContainer(name) });
  if (mimeType) fileEntries.push({ label: 'MIME', value: mimeType });

  if (videoInfo) {
    if (videoInfo.width && videoInfo.height) {
      techEntries.push({ label: 'RESOLUTION', value: `${videoInfo.width}×${videoInfo.height}` });
      // Show actual display size if different from native
      if (containerSize && (videoInfo.width > containerSize.w || videoInfo.height > containerSize.h)) {
        const scale = Math.min(containerSize.w / videoInfo.width, containerSize.h / videoInfo.height);
        const dw = Math.round(videoInfo.width * scale);
        const dh = Math.round(videoInfo.height * scale);
        techEntries.push({ label: 'DISPLAY', value: `${dw}×${dh} (${Math.round(scale * 100)}%)` });
      }
      const ratio = videoInfo.width / videoInfo.height;
      const label = Math.abs(ratio - 16/9) < 0.05 ? '16:9' :
                    Math.abs(ratio - 4/3) < 0.05 ? '4:3' :
                    Math.abs(ratio - 21/9) < 0.05 ? '21:9' :
                    Math.abs(ratio - 1) < 0.05 ? '1:1' :
                    ratio.toFixed(2);
      techEntries.push({ label: 'ASPECT', value: label });
    }
    if (videoInfo.duration && videoInfo.duration > 0) {
      techEntries.push({ label: 'DURATION', value: fmtTime(videoInfo.duration) });
    }
    if (fileSize && videoInfo.duration > 0) {
      const bitrateKbps = Math.round((fileSize * 8) / videoInfo.duration / 1000);
      techEntries.push({ label: 'BITRATE', value: `~${bitrateKbps} kbps` });
    }
  }

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div
      ref={wrapperRef}
      className="flex-1 flex min-h-[320px] select-none overflow-hidden bg-theme-bg outline-none"
      tabIndex={0}
      onKeyDown={handleKeyDown}
    >
      {/* ── Left: Video area ─────────────────────────────────────────────── */}
      <div
        ref={containerRef}
        className={cn(
          "flex-1 flex flex-col overflow-hidden relative",
          isFullscreen && "fixed inset-0 z-50",
        )}
        onMouseMove={resetHideTimer}
        onMouseLeave={() => playing && setShowControls(false)}
      >
      {/* ── Video element ───────────────────────────────────────────────── */}
      <div
        ref={videoWrapRef}
        className="flex-1 flex items-center justify-center min-h-0 cursor-pointer bg-black/40"
        onClick={togglePlay}
        onDoubleClick={toggleFullscreen}
      >
        <video
          ref={videoRef}
          src={src}
          preload="metadata"
          className="block"
          style={videoStyle}
          {...(mimeType ? { type: mimeType } : {})}
        />

        {/* Big play overlay when paused */}
        {!playing && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <div className="w-16 h-16 rounded-full bg-theme-accent/80 flex items-center justify-center backdrop-blur-sm">
              <Play className="h-7 w-7 text-theme-bg ml-1" />
            </div>
          </div>
        )}
      </div>

      {/* ── Controls overlay ────────────────────────────────────────────── */}
      <div
        className={cn(
          "absolute bottom-0 left-0 right-0 transition-all duration-300",
          showControls ? "opacity-100 translate-y-0" : "opacity-0 translate-y-2 pointer-events-none",
        )}
      >
        {/* Gradient backdrop */}
        <div className="absolute inset-0 bg-gradient-to-t from-black/70 via-black/30 to-transparent pointer-events-none" />

        <div className="relative px-3 pb-2 pt-6">
          {/* ── Seekbar ─────────────────────────────────────────────────── */}
          <div
            ref={seekBarRef}
            className={cn(
              "group relative h-1 rounded-full cursor-pointer overflow-hidden bg-theme-bg-hover/50 hover:h-1.5 transition-all mb-2",
              isSeeking && "h-1.5",
            )}
            onMouseDown={onSeekMouseDown}
            onClick={seek}
          >
            {/* Buffer bar */}
            <div
              className="absolute inset-y-0 left-0 rounded-full bg-theme-text/10"
              style={{ width: `${bufferProgress}%` }}
            />
            {/* Progress bar */}
            <div
              className="absolute inset-y-0 left-0 rounded-full bg-theme-accent transition-[width] duration-75"
              style={{ width: `${progress}%` }}
            />
            {/* Hover thumb */}
            <div
              className={cn(
                "absolute top-1/2 -translate-y-1/2 w-2.5 h-2.5 rounded-full bg-theme-text transition-opacity",
                isSeeking ? "opacity-100" : "opacity-0 group-hover:opacity-100",
              )}
              style={{ left: `calc(${progress}% - 5px)` }}
            />
          </div>

          {/* ── Controls row ────────────────────────────────────────────── */}
          <div className="flex items-center gap-2">
            {/* Play / Pause */}
            <button
              className="p-1.5 text-white/90 hover:text-white transition-colors"
              onClick={(e) => { e.stopPropagation(); togglePlay(); }}
            >
              {playing ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4 ml-0.5" />}
            </button>

            {/* Skip buttons */}
            <button
              className="p-1 text-white/60 hover:text-white transition-colors"
              onClick={(e) => { e.stopPropagation(); skip(-10); }}
              title="-10s"
            >
              <SkipBack className="h-3.5 w-3.5" />
            </button>
            <button
              className="p-1 text-white/60 hover:text-white transition-colors"
              onClick={(e) => { e.stopPropagation(); skip(10); }}
              title="+10s"
            >
              <SkipForward className="h-3.5 w-3.5" />
            </button>

            {/* Volume */}
            <div className="flex items-center gap-1.5 ml-1">
              <button
                className="p-1 text-white/60 hover:text-white transition-colors"
                onClick={(e) => { e.stopPropagation(); toggleMute(); }}
                title={muted ? 'Unmute' : 'Mute'}
              >
                <VolumeIcon volume={volume} muted={muted} className="h-3.5 w-3.5" />
              </button>
              <input
                type="range" min={0} max={1} step={0.01}
                value={muted ? 0 : volume}
                onChange={onVolumeChange}
                onClick={(e) => e.stopPropagation()}
                className="audio-volume-slider w-16 h-1 cursor-pointer"
              />
            </div>

            {/* Time display */}
            <div className="text-[11px] font-mono text-white/70 ml-2 select-none">
              {fmtTime(currentTime)} / {fmtTime(duration)}
            </div>

            {/* Spacer */}
            <div className="flex-1" />

            {/* File name */}
            <span className="text-[10px] font-mono text-white/40 truncate max-w-[200px] hidden sm:block">
              {name}
            </span>

            {/* PiP */}
            <button
              className="p-1 text-white/60 hover:text-white transition-colors"
              onClick={(e) => { e.stopPropagation(); togglePiP(); }}
              title="Picture-in-Picture"
            >
              <PictureInPicture2 className="h-3.5 w-3.5" />
            </button>

            {/* Info toggle */}
            <button
              className={cn(
                "p-1 transition-colors",
                showInfo ? "text-theme-accent" : "text-white/60 hover:text-white",
              )}
              onClick={(e) => { e.stopPropagation(); setShowInfo(s => !s); }}
              title="Toggle info"
            >
              <Info className="h-3.5 w-3.5" />
            </button>

            {/* Fullscreen */}
            <button
              className="p-1 text-white/60 hover:text-white transition-colors"
              onClick={(e) => { e.stopPropagation(); toggleFullscreen(); }}
              title={isFullscreen ? 'Exit fullscreen' : 'Fullscreen'}
            >
              {isFullscreen ? <Minimize className="h-3.5 w-3.5" /> : <Maximize className="h-3.5 w-3.5" />}
            </button>
          </div>
        </div>
      </div>
      </div>

      {/* ── Right: Info panel ─────────────────────────────────────────────── */}
      <div
        className={cn(
          "transition-all duration-300 overflow-hidden border-l border-theme-border",
          "bg-zinc-900/50 flex flex-col shrink-0",
          showInfo ? "w-56" : "w-0 border-l-0",
        )}
      >
        {/* Panel header */}
        <div className="flex items-center gap-1.5 px-3 py-2 border-b border-theme-border text-[10px] text-theme-text-muted font-mono uppercase tracking-wider shrink-0">
          <Film className="h-3 w-3 text-theme-accent" />
          <span>video info</span>
        </div>

        <div className="flex-1 overflow-y-auto p-3 scrollbar-thin scrollbar-thumb-zinc-700">
          {/* File entries */}
          {fileEntries.length > 0 && (
            <div className="mb-2">
              {fileEntries.map((e, i) => (
                <VideoInfoRow key={i} label={e.label} value={e.value} />
              ))}
            </div>
          )}

          {/* Separator */}
          {fileEntries.length > 0 && techEntries.length > 0 && (
            <div className="border-t border-theme-border my-2" />
          )}

          {/* Tech entries */}
          {techEntries.map((e, i) => (
            <VideoInfoRow key={i} label={e.label} value={e.value} />
          ))}

          {!videoInfo && (
            <div className="text-[10px] font-mono text-theme-text-muted animate-pulse">
              loading video info…
            </div>
          )}

          {/* Live playback stats */}
          <div className="mt-3 pt-2 border-t border-theme-border">
            <div className="text-[9px] font-mono text-theme-text-muted uppercase tracking-wider mb-1">live</div>
            <div className="text-[10px] font-mono text-theme-accent/80">
              <div>POS  {fmtTime(currentTime)} / {fmtTime(duration)}</div>
              <div>PCT  {progress.toFixed(1)}%</div>
              {buffered > 0 && <div>BUF  {fmtTime(buffered)}</div>}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

// ── Sub-component ────────────────────────────────────────────────────────────

const VideoInfoRow: React.FC<{ label: string; value: string }> = ({ label, value }) => (
  <div className="flex text-[10px] leading-4 font-mono hover:bg-theme-bg-hover/50 px-1 -mx-1 rounded-sm transition-colors">
    <span className="text-theme-accent/50 mr-1">{'>'}</span>
    <span className="text-theme-text-muted w-[72px] shrink-0">{label}</span>
    <span className="text-zinc-300 truncate" title={value}>{value}</span>
  </div>
);
