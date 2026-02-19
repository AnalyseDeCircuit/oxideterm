/**
 * CastPlayer — full-screen asciicast playback component
 *
 * Renders a .cast file using a dedicated xterm.js instance with
 * play/pause, speed control, seekbar, and text search.
 * Styled to match OxideTerm's VideoPlayer / AudioVisualizer controls.
 */

import React, { useRef, useEffect, useState, useCallback } from 'react';
import { Terminal } from '@xterm/xterm';
import { WebglAddon } from '@xterm/addon-webgl';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import '@xterm/xterm/css/xterm.css';
import {
  Play, Pause, SkipBack, SkipForward, X,
  Search, ChevronRight, Gauge,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../lib/utils';
import { themes } from '../../lib/themes';
import { useSettingsStore } from '../../store/settingsStore';
import { getFontFamily } from '../../lib/fontFamily';
import { AsciicastPlayer } from '../../lib/recording/player';
import type { AsciicastHeader, PlaybackSpeed } from '../../lib/recording/types';
import { PLAYBACK_SPEEDS } from '../../lib/recording/types';

// ── Types ────────────────────────────────────────────────────────────────────

type CastPlayerProps = {
  /** Raw .cast file content */
  content: string;
  /** File name for display */
  fileName: string;
  /** Close the player */
  onClose: () => void;
};

// ── Helpers ──────────────────────────────────────────────────────────────────

const pad = (n: number) => String(Math.floor(n)).padStart(2, '0');
function fmtTime(sec: number): string {
  if (!isFinite(sec) || isNaN(sec)) return '0:00';
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.floor(sec % 60);
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/**
 * Size-Lock: measure the exact pixel dimensions of the xterm.js canvas
 * after it has been resized to the recording's cols/rows, then fix the
 * container to those dimensions so the terminal never re-flows.
 *
 * xterm.js renders into a `.xterm-screen` element whose size is a direct
 * product of (cols × charWidth, rows × charHeight). We read that size
 * and apply it as explicit width/height on the container div.
 */
function sizeLockContainer(
  term: Terminal,
  header: AsciicastHeader,
  container: HTMLDivElement | null,
): void {
  if (!container) return;

  // xterm calculates precise char metrics after open(). Use them to
  // derive the pixel size needed for the recording's cols×rows.
  // The internal .xterm-screen element is the most reliable source.
  requestAnimationFrame(() => {
    const screen = container.querySelector('.xterm-screen') as HTMLElement | null;
    if (screen) {
      const w = screen.offsetWidth;
      const h = screen.offsetHeight;
      container.style.width = `${w + 8}px`;   // +padding
      container.style.height = `${h + 8}px`;
    } else {
      // Fallback: estimate from character cell dimensions
      // @ts-expect-error _core is internal xterm API
      const core = term._core;
      const cellW: number = core?._renderService?.dimensions?.css?.cell?.width ?? 8;
      const cellH: number = core?._renderService?.dimensions?.css?.cell?.height ?? 17;
      container.style.width = `${header.width * cellW + 8}px`;
      container.style.height = `${header.height * cellH + 8}px`;
    }
  });
}

// ── Component ────────────────────────────────────────────────────────────────

export const CastPlayer: React.FC<CastPlayerProps> = ({
  content,
  fileName,
  onClose,
}) => {
  const { t } = useTranslation();
  const terminalSettings = useSettingsStore(s => s.settings.terminal);

  const wrapperRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const playerRef = useRef<AsciicastPlayer | null>(null);

  // State
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [speed, setSpeed] = useState<PlaybackSpeed>(1);
  const [header, setHeader] = useState<AsciicastHeader | null>(null);
  const [loading, setLoading] = useState(true);
  const [showSearch, setShowSearch] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<{ time: number; snippet: string }[]>([]);
  const [showSpeedMenu, setShowSpeedMenu] = useState(false);

  // Seekbar drag state
  const seekBarRef = useRef<HTMLDivElement>(null);
  const [isSeeking, setIsSeeking] = useState(false);
  const [seekPreview, setSeekPreview] = useState<number | null>(null);
  const wasPlayingRef = useRef(false);

  // Resolve terminal theme
  const currentTheme = themes[terminalSettings.theme] || themes.default;

  // ── Initialise terminal + player ─────────────────────────────────────────

  useEffect(() => {
    if (!containerRef.current) return;

    const fontSize = terminalSettings.fontSize || 14;
    const resolvedFont = getFontFamily(terminalSettings.fontFamily, terminalSettings.customFontFamily);

    const term = new Terminal({
      fontFamily: resolvedFont,
      fontSize,
      theme: currentTheme,
      allowProposedApi: true,
      disableStdin: true,
      cursorBlink: false,
      cursorStyle: 'bar',
      scrollback: 10000,
    });

    const unicodeAddon = new Unicode11Addon();
    term.loadAddon(unicodeAddon);
    term.unicode.activeVersion = '11';

    term.open(containerRef.current);

    // Try WebGL
    try {
      const webgl = new WebglAddon();
      term.loadAddon(webgl);
    } catch {
      // WebGL unavailable — canvas fallback is fine for playback
    }

    terminalRef.current = term;

    const player = new AsciicastPlayer(term, {
      onProgress: (time, dur) => {
        setCurrentTime(time);
        setDuration(dur);
      },
      onStateChange: (state) => {
        setPlaying(state === 'playing');
      },
      onFinished: () => {
        setPlaying(false);
      },
    });
    playerRef.current = player;

    // Load content — player.load() calls term.resize(header.width, header.height)
    // We intentionally do NOT use FitAddon: the terminal is locked to recording
    // dimensions to prevent TUI layout corruption.
    try {
      const { header: h, duration: d } = player.load(content);
      setHeader(h);
      setDuration(d);

      // After player.load() locks cols/rows, size the container to match
      // the exact pixel dimensions so xterm doesn't stretch or clip.
      sizeLockContainer(term, h, containerRef.current);

      setLoading(false);
    } catch (err) {
      console.error('[CastPlayer] Failed to load cast:', err);
      setLoading(false);
    }

    return () => {
      player.dispose();
      term.dispose();
      terminalRef.current = null;
      playerRef.current = null;
    };
    // Intentionally only run once on mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Keyboard shortcuts ─────────────────────────────────────────────────

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!document.hasFocus()) return;
      if (e.target instanceof HTMLInputElement) return;

      switch (e.key) {
        case ' ':
          e.preventDefault();
          playerRef.current?.togglePlayPause();
          break;
        case 'ArrowLeft':
          e.preventDefault();
          playerRef.current?.seek(Math.max(0, currentTime - 5));
          break;
        case 'ArrowRight':
          e.preventDefault();
          playerRef.current?.seek(currentTime + 5);
          break;
        case 'Escape':
          e.preventDefault();
          if (showSearch) setShowSearch(false);
          else onClose();
          break;
        case 'f':
          if (e.metaKey || e.ctrlKey) {
            e.preventDefault();
            setShowSearch(prev => !prev);
          }
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [currentTime, onClose, showSearch]);

  // ── Playback Controls ──────────────────────────────────────────────────

  const togglePlay = useCallback(() => {
    playerRef.current?.togglePlayPause();
  }, []);

  const skipBack = useCallback(() => {
    playerRef.current?.seek(Math.max(0, currentTime - 10));
  }, [currentTime]);

  const skipForward = useCallback(() => {
    playerRef.current?.seek(currentTime + 10);
  }, [currentTime]);

  const handleSpeedChange = useCallback((s: PlaybackSpeed) => {
    setSpeed(s);
    playerRef.current?.setSpeed(s);
    setShowSpeedMenu(false);
  }, []);

  // ── Seekbar Drag ───────────────────────────────────────────────────────

  const getSeekRatio = useCallback((clientX: number) => {
    const bar = seekBarRef.current;
    if (!bar) return 0;
    const rect = bar.getBoundingClientRect();
    return Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
  }, []);

  const onSeekMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    if (!duration) return;
    wasPlayingRef.current = playing;
    if (playing) playerRef.current?.pause();
    setIsSeeking(true);
    const ratio = getSeekRatio(e.clientX);
    setSeekPreview(ratio * 100);
    playerRef.current?.seek(ratio * duration);
  }, [duration, playing, getSeekRatio]);

  useEffect(() => {
    if (!isSeeking) return;

    const onMove = (e: MouseEvent) => {
      const ratio = getSeekRatio(e.clientX);
      setSeekPreview(ratio * 100);
      playerRef.current?.seek(ratio * duration);
    };

    const onUp = (e: MouseEvent) => {
      const ratio = getSeekRatio(e.clientX);
      playerRef.current?.seek(ratio * duration);
      setIsSeeking(false);
      setSeekPreview(null);
      if (wasPlayingRef.current) playerRef.current?.play();
    };

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [isSeeking, duration, getSeekRatio]);

  // ── Search ─────────────────────────────────────────────────────────────

  const handleSearch = useCallback((query: string) => {
    setSearchQuery(query);
    if (!query.trim() || !playerRef.current) {
      setSearchResults([]);
      return;
    }
    const results = playerRef.current.searchText(query);
    setSearchResults(results);
  }, []);

  const jumpToResult = useCallback((time: number) => {
    playerRef.current?.seek(time);
  }, []);

  // ── Progress ratio ─────────────────────────────────────────────────────

  const progressPct = duration > 0
    ? (seekPreview ?? ((currentTime / duration) * 100))
    : 0;

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <div
      className="fixed inset-0 z-50 flex flex-col bg-zinc-950"
      onClick={(e) => e.stopPropagation()}
    >
      {/* Header bar */}
      <div className="flex items-center justify-between px-4 py-2 bg-zinc-900/80 border-b border-zinc-800">
        <div className="flex items-center gap-3">
          <span className="text-sm font-medium text-zinc-200 truncate max-w-[400px]">
            {fileName}
          </span>
          {header && (
            <span className="text-xs text-zinc-500 font-mono">
              {t('terminal.recording.player.dimensions', {
                cols: header.width,
                rows: header.height,
              })}
            </span>
          )}
          {header?.title && (
            <span className="text-xs text-zinc-400 italic truncate max-w-[300px]">
              {header.title}
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          {/* Search toggle */}
          <button
            onClick={() => setShowSearch(prev => !prev)}
            className={cn(
              'p-1.5 rounded transition-colors',
              showSearch
                ? 'bg-zinc-700 text-zinc-200'
                : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800',
            )}
            title={t('terminal.recording.player.search')}
          >
            <Search className="h-4 w-4" />
          </button>

          {/* Close */}
          <button
            onClick={onClose}
            className="p-1.5 rounded text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition-colors"
            title={t('terminal.recording.player.close')}
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      </div>

      {/* Search panel */}
      {showSearch && (
        <div className="bg-zinc-900/60 border-b border-zinc-800 px-4 py-2">
          <div className="flex items-center gap-2 max-w-lg">
            <Search className="h-3.5 w-3.5 text-zinc-500 shrink-0" />
            <input
              type="text"
              value={searchQuery}
              onChange={e => handleSearch(e.target.value)}
              className="flex-1 bg-zinc-800/60 border border-zinc-700/50 rounded px-2 py-1 text-sm text-zinc-200 placeholder-zinc-500 outline-none focus:border-zinc-600"
              placeholder={t('terminal.recording.player.search')}
              autoFocus
            />
            {searchResults.length > 0 && (
              <span className="text-xs text-zinc-400 shrink-0">
                {t('terminal.recording.player.matches', { count: searchResults.length })}
              </span>
            )}
          </div>
          {/* Search results list */}
          {searchResults.length > 0 && (
            <div className="mt-2 max-h-32 overflow-y-auto space-y-0.5">
              {searchResults.slice(0, 50).map((result, i) => (
                <button
                  key={i}
                  onClick={() => jumpToResult(result.time)}
                  className="flex items-center gap-2 w-full text-left px-2 py-1 rounded text-xs hover:bg-zinc-800/60 transition-colors group"
                >
                  <span className="text-zinc-500 font-mono shrink-0 w-12">
                    {fmtTime(result.time)}
                  </span>
                  <ChevronRight className="h-3 w-3 text-zinc-600 group-hover:text-zinc-400 shrink-0" />
                  <span className="text-zinc-400 truncate font-mono text-[11px]">
                    {result.snippet}
                  </span>
                </button>
              ))}
            </div>
          )}
          {searchQuery && searchResults.length === 0 && (
            <div className="mt-1 text-xs text-zinc-500">
              {t('terminal.recording.player.no_results')}
            </div>
          )}
        </div>
      )}

      {/* Terminal area — centered with black borders (size-locked) */}
      <div
        ref={wrapperRef}
        className="flex-1 relative overflow-hidden flex items-center justify-center bg-zinc-950"
      >
        {loading && (
          <div className="absolute inset-0 flex items-center justify-center bg-zinc-950/80 z-10">
            <div className="text-sm text-zinc-400">
              {t('terminal.recording.player.loading')}
            </div>
          </div>
        )}
        <div
          ref={containerRef}
          className="shrink-0"
          style={{
            padding: '4px',
            overflow: 'hidden',
            isolation: 'isolate',
          }}
        />
      </div>

      {/* Controls bar */}
      <div className="bg-zinc-900/80 border-t border-zinc-800 px-4 py-2.5">
        {/* Seekbar */}
        <div
          ref={seekBarRef}
          className="relative h-1.5 bg-zinc-800 rounded-full cursor-pointer mb-3 group"
          onMouseDown={onSeekMouseDown}
        >
          {/* Progress fill */}
          <div
            className="absolute inset-y-0 left-0 rounded-full bg-orange-500 transition-[width] duration-75"
            style={{ width: `${Math.min(100, progressPct)}%` }}
          />
          {/* Thumb */}
          <div
            className="absolute top-1/2 -translate-y-1/2 -translate-x-1/2 h-3 w-3 rounded-full bg-white shadow-md opacity-0 group-hover:opacity-100 transition-opacity"
            style={{ left: `${Math.min(100, progressPct)}%` }}
          />
        </div>

        {/* Buttons row */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            {/* Skip back 10s */}
            <button
              onClick={skipBack}
              className="text-zinc-400 hover:text-zinc-200 transition-colors"
              title="-10s"
            >
              <SkipBack className="h-4 w-4" />
            </button>

            {/* Play/Pause */}
            <button
              onClick={togglePlay}
              className="h-8 w-8 flex items-center justify-center rounded-full bg-zinc-700 hover:bg-zinc-600 text-zinc-200 transition-colors"
              title={playing
                ? t('terminal.recording.player.pause')
                : t('terminal.recording.player.play')
              }
            >
              {playing
                ? <Pause className="h-4 w-4" />
                : <Play className="h-4 w-4 ml-0.5" />
              }
            </button>

            {/* Skip forward 10s */}
            <button
              onClick={skipForward}
              className="text-zinc-400 hover:text-zinc-200 transition-colors"
              title="+10s"
            >
              <SkipForward className="h-4 w-4" />
            </button>

            {/* Time display */}
            <span className="text-xs font-mono text-zinc-400 ml-2">
              {fmtTime(currentTime)} / {fmtTime(duration)}
            </span>
          </div>

          {/* Speed control */}
          <div className="relative">
            <button
              onClick={() => setShowSpeedMenu(prev => !prev)}
              className={cn(
                'flex items-center gap-1 px-2 py-1 rounded text-xs font-mono transition-colors',
                showSpeedMenu
                  ? 'bg-zinc-700 text-zinc-200'
                  : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800',
              )}
            >
              <Gauge className="h-3.5 w-3.5" />
              <span>{speed}×</span>
            </button>

            {showSpeedMenu && (
              <div className="absolute bottom-full right-0 mb-1 bg-zinc-800 border border-zinc-700 rounded-lg shadow-xl py-1 min-w-[80px]">
                {PLAYBACK_SPEEDS.map(s => (
                  <button
                    key={s}
                    onClick={() => handleSpeedChange(s)}
                    className={cn(
                      'w-full text-left px-3 py-1.5 text-xs font-mono transition-colors',
                      s === speed
                        ? 'bg-zinc-700 text-zinc-200'
                        : 'text-zinc-400 hover:bg-zinc-700/50 hover:text-zinc-200',
                    )}
                  >
                    {s}×
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
