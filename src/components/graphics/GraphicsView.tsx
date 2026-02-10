/**
 * WSL Graphics View — Built-in component for displaying WSL GUI apps via VNC/noVNC.
 *
 * Backend: feature-gated Rust module (wsl-graphics + Windows only).
 * When the feature is unavailable, Tauri commands return descriptive errors.
 */

import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import RFB from '@novnc/novnc/lib/rfb.js';

// ─── Types ──────────────────────────────────────────────────────────

interface WslDistro {
  name: string;
  isDefault: boolean;
  isRunning: boolean;
}

interface WslGraphicsSession {
  id: string;
  wsPort: number;
  wsToken: string;
  distro: string;
  desktopName: string;
}

const STATUS = {
  IDLE: 'idle',
  STARTING: 'starting',
  ACTIVE: 'active',
  DISCONNECTED: 'disconnected',
  ERROR: 'error',
} as const;

type Status = typeof STATUS[keyof typeof STATUS];

// ─── Distro Selector ────────────────────────────────────────────────

function DistroSelector({
  distros,
  onSelect,
  error,
  loading,
}: {
  distros: WslDistro[];
  onSelect: (name: string) => void;
  error: string | null;
  loading: boolean;
}) {
  const { t } = useTranslation();
  const displayError = error === '__NOT_AVAILABLE__' ? t('graphics.not_available') : error;

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        <div className="flex flex-col items-center gap-3">
          <div className="animate-spin w-6 h-6 border-2 border-primary border-t-transparent rounded-full" />
          <span>{t('graphics.loading_distros')}</span>
        </div>
      </div>
    );
  }

  if (distros.length === 0 && !error) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        <div className="flex flex-col items-center gap-3 max-w-md text-center">
          <svg
            className="w-12 h-12 text-muted-foreground/50"
            viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"
          >
            <rect x={2} y={3} width={20} height={14} rx={2} />
            <line x1={8} y1={21} x2={16} y2={21} />
            <line x1={12} y1={17} x2={12} y2={21} />
          </svg>
          <p className="text-sm">{t('graphics.no_distros')}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-center h-full">
      <div className="flex flex-col gap-4 max-w-sm w-full px-6">
        <h2 className="text-lg font-semibold text-foreground text-center">
          {t('graphics.select_distro')}
        </h2>

        {displayError && (
          <div className="px-3 py-2 rounded bg-destructive/10 text-destructive text-sm">
            {displayError}
          </div>
        )}

        {distros.map((distro) => (
          <button
            key={distro.name}
            onClick={() => onSelect(distro.name)}
            className="flex items-center gap-3 px-4 py-3 rounded-lg border transition-colors border-border hover:border-primary hover:bg-accent text-left"
          >
            <div className="flex-1">
              <div className="font-medium text-foreground">
                {distro.name}
                {distro.isDefault && (
                  <span className="ml-2 text-xs px-1.5 py-0.5 rounded bg-primary/10 text-primary">
                    Default
                  </span>
                )}
              </div>
              <div className="text-xs text-muted-foreground mt-0.5">
                {distro.isRunning ? t('graphics.distro_running') : t('graphics.distro_stopped')}
              </div>
            </div>
            <svg
              className="w-4 h-4 text-muted-foreground"
              viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
            >
              <polyline points="9 18 15 12 9 6" />
            </svg>
          </button>
        ))}
      </div>
    </div>
  );
}

// ─── Toolbar ────────────────────────────────────────────────────────

function Toolbar({
  onStop,
  onReconnect,
  onFullscreen,
  status,
  sessionInfo,
}: {
  onStop: () => void;
  onReconnect: () => void;
  onFullscreen: () => void;
  status: Status;
  sessionInfo: WslGraphicsSession | null;
}) {
  const { t } = useTranslation();

  const isExperimental = true; // WSL Graphics is globally experimental

  const buttonClass =
    'px-3 py-1.5 text-xs font-medium rounded transition-colors border border-border hover:bg-accent text-foreground';

  return (
    <div className="absolute top-0 right-0 left-0 z-10 flex justify-end opacity-0 hover:opacity-100 transition-opacity duration-200">
      <div className="flex items-center gap-2 px-3 py-1.5 mt-2 mr-2 rounded-lg bg-background/90 backdrop-blur-sm border border-border shadow-sm">
        {sessionInfo && (
          <span className="text-xs text-muted-foreground mr-2">
            {sessionInfo.distro}
            {sessionInfo.desktopName && (
              <span className="ml-1.5 text-muted-foreground/70">
                · {sessionInfo.desktopName}
              </span>
            )}
            {isExperimental && (
              <span className="ml-1.5 px-1.5 py-0.5 rounded text-[10px] font-medium bg-warning/15 text-warning border border-warning/20">
                {t('graphics.desktop_experimental')}
              </span>
            )}
          </span>
        )}

        {status === STATUS.DISCONNECTED && (
          <button onClick={onReconnect} className={buttonClass}>
            {t('graphics.reconnect')}
          </button>
        )}

        <button onClick={onFullscreen} className={buttonClass}>
          {t('graphics.fullscreen')}
        </button>

        <button
          onClick={onStop}
          className={`${buttonClass} hover:bg-destructive/10 hover:text-destructive hover:border-destructive/30`}
        >
          {t('graphics.stop')}
        </button>
      </div>
    </div>
  );
}

// ─── Status Overlay ─────────────────────────────────────────────────

function StatusOverlay({ status, error }: { status: Status; error: string | null }) {
  const { t } = useTranslation();
  const displayError = error === '__NOT_AVAILABLE__' ? t('graphics.not_available') : error;

  if (status === STATUS.ACTIVE) return null;

  const overlays: Partial<Record<Status, { icon: React.ReactNode; text: string }>> = {
    [STATUS.STARTING]: {
      icon: <div className="animate-spin w-8 h-8 border-2 border-primary border-t-transparent rounded-full" />,
      text: t('graphics.starting'),
    },
    [STATUS.DISCONNECTED]: {
      icon: (
        <svg className="w-8 h-8 text-warning" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
          <line x1={12} y1={9} x2={12} y2={13} />
          <line x1={12} y1={17} x2={12.01} y2={17} />
        </svg>
      ),
      text: t('graphics.disconnected'),
    },
    [STATUS.ERROR]: {
      icon: (
        <svg className="w-8 h-8 text-destructive" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx={12} cy={12} r={10} />
          <line x1={15} y1={9} x2={9} y2={15} />
          <line x1={9} y1={9} x2={15} y2={15} />
        </svg>
      ),
      text: displayError || t('graphics.error'),
    },
  };

  const content = overlays[status];
  if (!content) return null;

  return (
    <div className="absolute inset-0 flex items-center justify-center bg-background/70 backdrop-blur-sm z-20">
      <div className="flex flex-col items-center gap-3">
        {content.icon}
        <span className="text-sm text-muted-foreground">{content.text}</span>
      </div>
    </div>
  );
}

// ─── Main GraphicsView Component ────────────────────────────────────

export function GraphicsView() {
  const canvasContainerRef = useRef<HTMLDivElement>(null);
  const rfbRef = useRef<RFB | null>(null);
  const sessionRef = useRef<WslGraphicsSession | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [session, setSession] = useState<WslGraphicsSession | null>(null);
  const [status, setStatus] = useState<Status>(STATUS.IDLE);
  const [distros, setDistros] = useState<WslDistro[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  // ── Load WSL distros on mount ───────────────────────────────────
  useEffect(() => {
    let cancelled = false;
    setLoading(true);

    invoke<WslDistro[]>('wsl_graphics_list_distros')
      .then((list) => {
        if (!cancelled) {
          setDistros(list);
          setError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          const msg = String(e);
          // Mark stub error for i18n-aware rendering (don't bake translated string into state)
          if (msg.includes('only available on Windows')) {
            setError('__NOT_AVAILABLE__');
          } else {
            setError(msg);
          }
          setDistros([]);
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => { cancelled = true; };
  }, []);

  // ── Start session ───────────────────────────────────────────────
  const startSession = useCallback(async (distro: string) => {
    setStatus(STATUS.STARTING);
    setError(null);
    try {
      const sess = await invoke<WslGraphicsSession>('wsl_graphics_start', { distro });
      sessionRef.current = sess;
      setSession(sess);
      setStatus(STATUS.ACTIVE);
    } catch (e) {
      setError(String(e));
      setStatus(STATUS.ERROR);
    }
  }, []);

  // ── Connect noVNC when session starts ───────────────────────────
  useEffect(() => {
    if (!session || !canvasContainerRef.current) return;

    const timer = setTimeout(() => {
      try {
        const url = `ws://127.0.0.1:${session.wsPort}?token=${session.wsToken}`;
        const rfb = new RFB(canvasContainerRef.current!, url, {
          wsProtocols: ['binary'],
        });

        rfb.scaleViewport = true;
        rfb.resizeSession = true;
        rfb.clipViewport = false;
        rfb.background = '#000000';
        rfbRef.current = rfb;

        rfb.addEventListener('connect', () => {
          setStatus(STATUS.ACTIVE);
        });

        rfb.addEventListener('disconnect', ((e: CustomEvent) => {
          if (!e.detail.clean) {
            setStatus(STATUS.DISCONNECTED);
          }
        }) as EventListener);

        rfb.addEventListener('securityfailure', ((e: CustomEvent) => {
          setError(`Security failure: ${e.detail.reason}`);
          setStatus(STATUS.ERROR);
        }) as EventListener);
      } catch (e) {
        setError(`noVNC init failed: ${String(e)}`);
        setStatus(STATUS.ERROR);
      }
    }, 100);

    return () => {
      clearTimeout(timer);
      if (rfbRef.current) {
        try { rfbRef.current.disconnect(); } catch { /* already disconnected */ }
        rfbRef.current = null;
      }
    };
  }, [session]);

  // ── Stop session ────────────────────────────────────────────────
  const stopSession = useCallback(async () => {
    // Cancel any pending reconnect timer first
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }

    // Disconnect noVNC
    if (rfbRef.current) {
      try { rfbRef.current.disconnect(); } catch { /* ignore */ }
      rfbRef.current = null;
    }

    // Tell backend to stop
    if (session) {
      try {
        await invoke('wsl_graphics_stop', { sessionId: session.id });
      } catch (e) {
        console.warn('[WSL Graphics] Stop error:', e);
      }
    }

    sessionRef.current = null;
    setSession(null);
    setStatus(STATUS.IDLE);
    setError(null);
  }, [session]);

  // ── Reconnect (bridge-only, VNC/desktop stay alive) ─────────────
  const reconnect = useCallback(async () => {
    if (!session) return;

    // Disconnect noVNC before rebuilding bridge
    if (rfbRef.current) {
      try { rfbRef.current.disconnect(); } catch { /* ignore */ }
      rfbRef.current = null;
    }

    setStatus(STATUS.STARTING);
    setError(null);

    try {
      const newSess = await invoke<WslGraphicsSession>('wsl_graphics_reconnect', {
        sessionId: session.id,
      });
      sessionRef.current = newSess;
      setSession(newSess);
      // noVNC will auto-connect via the session useEffect
    } catch (e) {
      setError(String(e));
      setStatus(STATUS.ERROR);
    }
  }, [session]);

  // ── Fullscreen toggle ───────────────────────────────────────────
  const toggleFullscreen = useCallback(() => {
    const container = canvasContainerRef.current?.parentElement;
    if (!container) return;

    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {});
    } else {
      container.requestFullscreen().catch(() => {});
    }
  }, []);

  // ── Cleanup on unmount ──────────────────────────────────────────
  useEffect(() => {
    return () => {
      // Cancel any pending reconnect timer
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }

      // Disconnect noVNC
      if (rfbRef.current) {
        try { rfbRef.current.disconnect(); } catch { /* ignore */ }
        rfbRef.current = null;
      }

      // Stop backend session (VNC process + bridge proxy)
      if (sessionRef.current) {
        const sid = sessionRef.current.id;
        sessionRef.current = null;
        invoke('wsl_graphics_stop', { sessionId: sid }).catch((e) => {
          console.warn('[WSL Graphics] unmount stop error:', e);
        });
      }
    };
  }, []);

  // ── Render: idle/error → distro selector ────────────────────────
  if (status === STATUS.IDLE || (status === STATUS.ERROR && !session)) {
    return (
      <DistroSelector
        distros={distros}
        onSelect={startSession}
        error={error}
        loading={loading}
      />
    );
  }

  // ── Render: active/starting/disconnected → VNC canvas ───────────
  return (
    <div className="relative w-full h-full bg-black">
      <Toolbar
        onStop={stopSession}
        onReconnect={reconnect}
        onFullscreen={toggleFullscreen}
        status={status}
        sessionInfo={session}
      />
      <StatusOverlay status={status} error={error} />
      <div
        ref={canvasContainerRef}
        className="w-full h-full"
        style={{ minHeight: '300px' }}
      />
    </div>
  );
}
