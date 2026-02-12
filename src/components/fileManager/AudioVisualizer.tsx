/**
 * AudioVisualizer Component
 *
 * A clean, lightweight audio player that matches the OxideTerm design system.
 * Uses semantic theme colours (theme-*) throughout. No Web Audio / FFT — pure HTML5 audio.
 *
 * Features:
 *  • Rotating vinyl disk with centre label
 *  • Custom seekbar & volume slider
 *  • Metadata panel: ID3 tags + bitrate / sample-rate / bit-depth
 */

import React, { useRef, useEffect, useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Play, Pause, Volume2, Volume1, VolumeX,
  SkipBack, SkipForward, Disc, Terminal, Music,
} from 'lucide-react';
import { cn } from '../../lib/utils';

// ── Types ────────────────────────────────────────────────────────────────────

interface AudioVisualizerProps {
  src: string;
  name: string;
  /** Absolute file path — used to invoke Rust metadata reader */
  filePath?: string;
  mimeType?: string;
}

interface AudioMeta {
  durationSecs?: number;
  bitrateKbps?: number;
  sampleRate?: number;
  bitDepth?: number;
  channels?: number;
  codec?: string;
  title?: string;
  artist?: string;
  album?: string;
  year?: number;
  genre?: string;
  trackNumber?: number;
  comment?: string;
  lyrics?: string;
  hasCover: boolean;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const pad = (n: number) => String(Math.floor(n)).padStart(2, '0');
const fmtTime = (s: number) => {
  if (!isFinite(s) || isNaN(s)) return '0:00';
  const m = Math.floor(s / 60);
  return `${m}:${pad(s % 60)}`;
};

/** File extension → codec label */
const guessCodec = (fileName: string): string => {
  const ext = fileName.split('.').pop()?.toLowerCase() ?? '';
  const map: Record<string, string> = {
    mp3: 'MP3', flac: 'FLAC', ogg: 'OGG Vorbis', opus: 'Opus',
    wav: 'WAV / PCM', aac: 'AAC', m4a: 'AAC / ALAC', wma: 'WMA',
    aiff: 'AIFF', ape: 'APE', wv: 'WavPack',
  };
  return map[ext] ?? ext.toUpperCase();
};

/** Volume icon based on level */
const VolumeIcon: React.FC<{ volume: number; muted: boolean; className?: string }> = ({ volume, muted, className }) => {
  if (muted || volume === 0) return <VolumeX className={className} />;
  if (volume < 0.5) return <Volume1 className={className} />;
  return <Volume2 className={className} />;
};

// ── Component ────────────────────────────────────────────────────────────────

export const AudioVisualizer: React.FC<AudioVisualizerProps> = ({
  src, name, filePath, mimeType,
}) => {
  const audioRef = useRef<HTMLAudioElement>(null);

  // State
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrent] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(0.8);
  const [muted, setMuted] = useState(false);
  const [meta, setMeta] = useState<AudioMeta | null>(null);
  const [metaLoading, setMetaLoading] = useState(false);
  const [showMeta, setShowMeta] = useState(true);
  /** When lyrics exist, user can toggle between metadata & lyrics view in the panel */
  const [showLyrics, setShowLyrics] = useState(false);

  // ── Metadata fetch ─────────────────────────────────────────────────────────

  useEffect(() => {
    if (!filePath) return;
    let cancelled = false;
    setMetaLoading(true);
    invoke<AudioMeta>('get_audio_metadata', { path: filePath })
      .then((m) => { if (!cancelled) setMeta(m); })
      .catch((e) => console.warn('Audio metadata unavailable:', e))
      .finally(() => { if (!cancelled) setMetaLoading(false); });
    return () => { cancelled = true; };
  }, [filePath]);

  // ── Playback controls ─────────────────────────────────────────────────────

  const togglePlay = useCallback(() => {
    const el = audioRef.current;
    if (!el) return;
    if (el.paused) { el.play(); setPlaying(true); }
    else { el.pause(); setPlaying(false); }
  }, []);

  const seek = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    const el = audioRef.current;
    if (!el || !isFinite(el.duration)) return;
    const rect = e.currentTarget.getBoundingClientRect();
    el.currentTime = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width)) * el.duration;
  }, []);

  const skip = useCallback((d: number) => {
    const el = audioRef.current;
    if (el) el.currentTime = Math.max(0, Math.min(el.duration || 0, el.currentTime + d));
  }, []);

  const toggleMute = useCallback(() => {
    const el = audioRef.current;
    if (el) { el.muted = !el.muted; setMuted(el.muted); }
  }, []);

  const onVolumeChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const el = audioRef.current;
    if (!el) return;
    const v = Number(e.target.value);
    el.volume = v;
    setVolume(v);
    if (v > 0 && el.muted) { el.muted = false; setMuted(false); }
  }, []);

  // ── Audio events ──────────────────────────────────────────────────────────

  useEffect(() => {
    const el = audioRef.current;
    if (!el) return;
    el.volume = volume;
    const onTime = () => setCurrent(el.currentTime);
    const onMeta = () => setDuration(el.duration);
    const onEnd = () => setPlaying(false);
    el.addEventListener('timeupdate', onTime);
    el.addEventListener('loadedmetadata', onMeta);
    el.addEventListener('ended', onEnd);
    return () => {
      el.removeEventListener('timeupdate', onTime);
      el.removeEventListener('loadedmetadata', onMeta);
      el.removeEventListener('ended', onEnd);
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const progress = duration > 0 ? (currentTime / duration) * 100 : 0;
  const displayTitle = meta?.title || name.replace(/\.[^.]+$/, '');
  const displayArtist = meta?.artist ?? '';

  // ── Build metadata lines ──────────────────────────────────────────────────

  type MetaEntry = { label: string; value: string };
  const tagEntries: MetaEntry[] = [];
  const techEntries: MetaEntry[] = [];

  if (meta) {
    if (meta.title) tagEntries.push({ label: 'TITLE', value: meta.title });
    if (meta.artist) tagEntries.push({ label: 'ARTIST', value: meta.artist });
    if (meta.album) tagEntries.push({ label: 'ALBUM', value: meta.album });
    if (meta.year) tagEntries.push({ label: 'YEAR', value: String(meta.year) });
    if (meta.genre) tagEntries.push({ label: 'GENRE', value: meta.genre });
    if (meta.trackNumber) tagEntries.push({ label: 'TRACK', value: String(meta.trackNumber) });
    if (meta.comment) tagEntries.push({ label: 'COMMENT', value: meta.comment });

    if (meta.bitrateKbps) techEntries.push({ label: 'BITRATE', value: `${meta.bitrateKbps} kbps` });
    if (meta.sampleRate) techEntries.push({ label: 'SAMPLE', value: `${(meta.sampleRate / 1000).toFixed(1)} kHz` });
    if (meta.bitDepth) techEntries.push({ label: 'DEPTH', value: `${meta.bitDepth}-bit` });
    if (meta.channels) techEntries.push({ label: 'CHANNELS', value: meta.channels === 1 ? 'Mono' : meta.channels === 2 ? 'Stereo' : `${meta.channels}ch` });
    techEntries.push({ label: 'CODEC', value: meta.codec || guessCodec(name) });
    if (meta.durationSecs) techEntries.push({ label: 'LENGTH', value: fmtTime(meta.durationSecs) });
    if (meta.hasCover) techEntries.push({ label: 'ARTWORK', value: 'embedded' });
  } else if (!metaLoading) {
    techEntries.push({ label: 'FILE', value: name });
    techEntries.push({ label: 'CODEC', value: guessCodec(name) });
  }

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="flex-1 flex min-h-[320px] select-none overflow-hidden relative bg-theme-bg">
      {/* Hidden audio */}
      <audio ref={audioRef} src={src} preload="metadata" {...(mimeType ? { type: mimeType } : {})} />

      {/* ── Left: Main player area ───────────────────────────────────────── */}
      <div className="flex-1 flex flex-col items-center justify-center relative p-6 min-w-0">

        {/* ── Vinyl Disk ──────────────────────────────────────────────────── */}
        <div className="relative mb-5">
          <div
            className={cn(
              "audio-vinyl-disk w-36 h-36 rounded-full border border-theme-border",
              "flex items-center justify-center relative overflow-hidden",
              playing && "audio-vinyl-spin",
            )}
          >
            {/* Disk grooves */}
            <div className="absolute inset-0 rounded-full"
              style={{
                background: `repeating-radial-gradient(
                  circle at center,
                  transparent 0px, transparent 2px,
                  rgba(255,255,255,0.03) 2px, rgba(255,255,255,0.03) 3px
                )`,
              }}
            />
            {/* Disk gradient */}
            <div className="absolute inset-0 rounded-full bg-gradient-to-br from-zinc-800 via-zinc-900 to-black opacity-90" />
            {/* Light reflection */}
            <div className="absolute inset-0 rounded-full"
              style={{
                background: 'linear-gradient(135deg, rgba(255,255,255,0.06) 0%, transparent 50%, rgba(0,0,0,0.2) 100%)',
              }}
            />
            {/* Centre label — uses theme accent */}
            <div className="relative z-10 w-14 h-14 rounded-full bg-theme-accent/80 flex items-center justify-center border border-theme-accent/30">
              <div className="w-2.5 h-2.5 rounded-full bg-theme-bg border border-theme-border" />
            </div>
          </div>
          {/* Subtle glow ring when playing */}
          {playing && (
            <div className="absolute -inset-2 rounded-full pointer-events-none audio-vinyl-glow" />
          )}
        </div>

        {/* Title / Artist */}
        <div className="text-center mb-4 max-w-full px-4">
          <p className="text-sm font-medium text-theme-text truncate">{displayTitle}</p>
          {displayArtist && (
            <p className="text-xs text-theme-text-muted truncate mt-0.5">{displayArtist}</p>
          )}
        </div>

        {/* ── Seekbar ─────────────────────────────────────────────────────── */}
        <div className="w-full max-w-xs px-2">
          <div
            className="group relative h-1 rounded-full cursor-pointer overflow-hidden bg-theme-bg-hover border border-theme-border hover:h-2 transition-all"
            onClick={seek}
          >
            <div
              className="absolute inset-y-0 left-0 rounded-full bg-theme-accent transition-[width] duration-75"
              style={{ width: `${progress}%` }}
            />
            <div
              className="absolute top-1/2 -translate-y-1/2 w-2.5 h-2.5 rounded-full bg-theme-text opacity-0 group-hover:opacity-100 transition-opacity"
              style={{ left: `calc(${progress}% - 5px)` }}
            />
          </div>
          <div className="flex justify-between mt-1 text-[10px] text-theme-text-muted font-mono">
            <span>{fmtTime(currentTime)}</span>
            <span>{fmtTime(duration)}</span>
          </div>
        </div>

        {/* ── Controls row ────────────────────────────────────────────────── */}
        <div className="flex items-center gap-3 mt-3 px-3 py-1.5 rounded-sm bg-theme-bg-panel border border-theme-border">
          {/* Volume — always visible slider */}
          <div className="flex items-center gap-1.5">
            <button
              className="p-1 text-theme-text-muted hover:text-theme-text transition-colors"
              onClick={toggleMute}
              title={muted ? 'Unmute' : 'Mute'}
            >
              <VolumeIcon volume={volume} muted={muted} className="h-3.5 w-3.5" />
            </button>
            <input
              type="range" min={0} max={1} step={0.01}
              value={muted ? 0 : volume}
              onChange={onVolumeChange}
              className="audio-volume-slider w-16 h-1 cursor-pointer"
            />
          </div>

          <div className="w-px h-4 bg-theme-border" />

          <button
            className="p-1.5 text-theme-text-muted hover:text-theme-text transition-colors"
            onClick={() => skip(-10)} title="-10s"
          >
            <SkipBack className="h-3.5 w-3.5" />
          </button>

          {/* Play / Pause */}
          <button
            className={cn(
              "p-2.5 rounded-sm transition-all duration-150",
              "bg-theme-accent text-theme-bg",
              "hover:bg-theme-accent-hover active:scale-95",
            )}
            onClick={togglePlay}
          >
            {playing ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4 ml-0.5" />}
          </button>

          <button
            className="p-1.5 text-theme-text-muted hover:text-theme-text transition-colors"
            onClick={() => skip(10)} title="+10s"
          >
            <SkipForward className="h-3.5 w-3.5" />
          </button>

          <div className="w-px h-4 bg-theme-border" />

          {/* Metadata toggle */}
          <button
            className={cn(
              "p-1 transition-colors",
              showMeta && !showLyrics ? "text-theme-accent" : "text-theme-text-muted hover:text-theme-text",
            )}
            onClick={() => { setShowMeta(s => !s || showLyrics); setShowLyrics(false); }}
            title="Toggle metadata"
          >
            <Terminal className="h-3.5 w-3.5" />
          </button>

          {/* Lyrics toggle — only shown when lyrics are available */}
          {meta?.lyrics && (
            <button
              className={cn(
                "p-1 transition-colors",
                showLyrics ? "text-theme-accent" : "text-theme-text-muted hover:text-theme-text",
              )}
              onClick={() => { setShowMeta(true); setShowLyrics(s => !s); }}
              title="Toggle lyrics"
            >
              <Music className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* ── Right: Metadata panel ─────────────────────────────────────────── */}
      <div
        className={cn(
          "transition-all duration-300 overflow-hidden border-l border-theme-border",
          "bg-zinc-900/50 flex flex-col",
          showMeta ? "w-56" : "w-0 border-l-0",
        )}
      >
        {/* Panel header */}
        <div className="flex items-center gap-1.5 px-3 py-2 border-b border-theme-border text-[10px] text-theme-text-muted font-mono uppercase tracking-wider shrink-0">
          {showLyrics ? (
            <>
              <Music className="h-3 w-3 text-theme-accent" />
              <span>lyrics</span>
            </>
          ) : (
            <>
              <Disc className="h-3 w-3 text-theme-accent" />
              <span>metadata</span>
            </>
          )}
        </div>

        <div className="flex-1 overflow-y-auto p-3 scrollbar-thin scrollbar-thumb-zinc-700">
          {showLyrics && meta?.lyrics ? (
            /* ── Lyrics view ────────────────────────────────────────── */
            <div className="text-[11px] font-mono text-theme-text leading-5 whitespace-pre-wrap break-words">
              {meta.lyrics}
            </div>
          ) : (
            /* ── Metadata view ──────────────────────────────────────── */
            <>
          {/* Tag entries */}
          {tagEntries.length > 0 && (
            <div className="mb-2">
              {tagEntries.map((e, i) => (
                <MetaRow key={i} label={e.label} value={e.value} />
              ))}
            </div>
          )}

          {/* Separator */}
          {tagEntries.length > 0 && techEntries.length > 0 && (
            <div className="border-t border-theme-border my-2" />
          )}

          {/* Tech entries */}
          {techEntries.map((e, i) => (
            <MetaRow key={i} label={e.label} value={e.value} />
          ))}

          {metaLoading && (
            <div className="text-[10px] font-mono text-theme-text-muted animate-pulse">
              scanning metadata…
            </div>
          )}

          {/* Live playback stats */}
          <div className="mt-3 pt-2 border-t border-theme-border">
            <div className="text-[9px] font-mono text-theme-text-muted uppercase tracking-wider mb-1">live</div>
            <div className="text-[10px] font-mono text-theme-accent/80">
              <div>POS  {fmtTime(currentTime)} / {fmtTime(duration)}</div>
              <div>PCT  {progress.toFixed(1)}%</div>
            </div>
          </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
};

// ── Sub-component ────────────────────────────────────────────────────────────

const MetaRow: React.FC<{ label: string; value: string }> = ({ label, value }) => (
  <div className="flex text-[10px] leading-4 font-mono hover:bg-theme-bg-hover/50 px-1 -mx-1 rounded-sm transition-colors">
    <span className="text-theme-accent/50 mr-1">{'>'}</span>
    <span className="text-theme-text-muted w-[72px] shrink-0">{label}</span>
    <span className="text-zinc-300 truncate">{value}</span>
  </div>
);
