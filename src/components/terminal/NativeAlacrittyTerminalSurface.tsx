// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AlertTriangle, Loader2, RotateCcw, SquareTerminal } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useSettingsStore } from '../../store/settingsStore';
import { api, type NativeTerminalAttachResponse, type NativeTerminalBounds, type NativeTerminalFont, type NativeTerminalSnapshot } from '../../lib/api';
import { getTerminalTheme } from '../../lib/themes';

type NativeAlacrittyTerminalSurfaceProps = {
  sessionId: string;
  paneId: string;
  terminalType: 'terminal' | 'local_terminal';
  nodeId?: string | null;
  isActive?: boolean;
};

export const NativeAlacrittyTerminalSurface: React.FC<NativeAlacrittyTerminalSurfaceProps> = ({
  sessionId,
  paneId,
  terminalType,
  nodeId = null,
  isActive = true,
}) => {
  const { t } = useTranslation();
  const updateTerminal = useSettingsStore((state) => state.updateTerminal);
  const terminalSettings = useSettingsStore((state) => state.settings.terminal);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const surfaceIdRef = useRef<string | null>(null);
  const lastBoundsRef = useRef<NativeTerminalBounds | null>(null);
  const lastVisibleRef = useRef<boolean | null>(null);
  const [attachResponse, setAttachResponse] = useState<NativeTerminalAttachResponse | null>(null);
  const [attachError, setAttachError] = useState<string | null>(null);
  const [isAttaching, setIsAttaching] = useState(true);
  const [snapshot, setSnapshot] = useState<NativeTerminalSnapshot | null>(null);

  const font = useMemo<NativeTerminalFont>(() => {
    const nativeFamily = terminalSettings.nativeAlacrittyFontFamily?.trim() || 'Menlo';
    return {
      family: nativeFamily,
      familyCss: nativeFamily,
      primaryFamily: nativeFamily,
      fallbackFamilies: [],
      size: terminalSettings.fontSize,
      lineHeight: terminalSettings.lineHeight,
    };
  }, [
    terminalSettings.fontSize,
    terminalSettings.lineHeight,
    terminalSettings.nativeAlacrittyFontFamily,
  ]);

  const theme = useMemo(() => {
    const terminalTheme = getTerminalTheme(terminalSettings.theme);
    return {
      foreground: terminalTheme.foreground ?? '#d4d4d8',
      background: terminalTheme.background ?? '#020617',
      cursor: terminalTheme.cursor ?? terminalTheme.foreground ?? '#d4d4d8',
      selection: terminalTheme.selectionBackground ?? '#1d4ed8',
    };
  }, [terminalSettings.theme]);

  const fontRef = useRef(font);
  const themeRef = useRef(theme);

  const refreshSnapshot = useCallback(async (surfaceId = surfaceIdRef.current) => {
    if (!surfaceId) return;
    try {
      const next = await api.nativeTerminalGetViewportSnapshot(surfaceId);
      setSnapshot(next);
    } catch {
      setSnapshot(null);
    }
  }, []);

  const readBounds = useCallback((): NativeTerminalBounds | null => {
    const host = hostRef.current;
    if (!host) return null;
    const rect = host.getBoundingClientRect();
    if (rect.width < 8 || rect.height < 8) {
      return null;
    }
    return {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
      dpr: window.devicePixelRatio || 1,
    };
  }, []);

  const isHostVisible = useCallback((): boolean => {
    const host = hostRef.current;
    if (!isActive || document.visibilityState === 'hidden' || !host || !host.isConnected) return false;
    const rect = host.getBoundingClientRect();
    if (rect.width < 8 || rect.height < 8) return false;
    const style = window.getComputedStyle(host);
    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) {
      return false;
    }
    const viewportWidth = window.innerWidth || document.documentElement.clientWidth;
    const viewportHeight = window.innerHeight || document.documentElement.clientHeight;
    const x = Math.min(Math.max(rect.left + rect.width / 2, 0), Math.max(0, viewportWidth - 1));
    const y = Math.min(Math.max(rect.top + rect.height / 2, 0), Math.max(0, viewportHeight - 1));
    const topElement = document.elementFromPoint(x, y);
    return !!topElement && (topElement === host || host.contains(topElement));
  }, [isActive]);

  const boundsEqual = (a: NativeTerminalBounds | null, b: NativeTerminalBounds | null) => {
    if (!a || !b) return a === b;
    return Math.abs(a.x - b.x) < 0.5
      && Math.abs(a.y - b.y) < 0.5
      && Math.abs(a.width - b.width) < 0.5
      && Math.abs(a.height - b.height) < 0.5
      && Math.abs(a.dpr - b.dpr) < 0.01;
  };

  const syncSurfaceGeometry = useCallback(async () => {
    const surfaceId = surfaceIdRef.current;
    if (!surfaceId) return;

    const bounds = readBounds();
    const visible = !!bounds && isHostVisible();

    if (lastVisibleRef.current !== visible) {
      lastVisibleRef.current = visible;
      await api.nativeTerminalUpdateVisibility(surfaceId, visible).catch(() => undefined);
    }

    // React sends the terminal viewport container's getBoundingClientRect().
    // Rust converts those WebView CSS top-left coordinates into AppKit
    // bottom-left coordinates and clamps the native view. This is what keeps
    // the native NSView from covering tabs, settings, sidebars, or Command Bar.
    if (visible && bounds && !boundsEqual(lastBoundsRef.current, bounds)) {
      lastBoundsRef.current = bounds;
      await api.nativeTerminalUpdateBounds(surfaceId, bounds).catch(() => undefined);
    }
  }, [isHostVisible, readBounds]);

  useEffect(() => {
    fontRef.current = font;
  }, [font]);

  useEffect(() => {
    themeRef.current = theme;
  }, [theme]);

  useEffect(() => {
    let cancelled = false;
    let resizeFrame: number | null = null;
    let visibilityTimer: number | null = null;
    let observer: ResizeObserver | null = null;

    const scheduleGeometrySync = () => {
      if (resizeFrame !== null) {
        window.cancelAnimationFrame(resizeFrame);
      }
      resizeFrame = window.requestAnimationFrame(() => {
        resizeFrame = null;
        void syncSurfaceGeometry();
      });
    };

    if (!isActive) {
      setIsAttaching(false);
      setAttachResponse(null);
      setSnapshot(null);
      return () => {
        cancelled = true;
      };
    }

    const attach = async () => {
      let bounds = readBounds();
      if (!bounds) {
        await new Promise((resolve) => window.requestAnimationFrame(resolve));
        bounds = readBounds();
      }
      if (!bounds) return;

      setIsAttaching(true);
      setAttachError(null);
      try {
        const response = await api.nativeTerminalAttach({
          paneId,
          terminalType,
          sessionId,
          nodeId,
          bounds,
          font: fontRef.current,
          theme: themeRef.current,
        });
        if (cancelled) {
          await api.nativeTerminalDetach(response.surfaceId).catch(() => undefined);
          return;
        }
        surfaceIdRef.current = response.surfaceId;
        setAttachResponse(response);
        scheduleGeometrySync();
        requestAnimationFrame(() => hostRef.current?.focus());
      } catch (error) {
        if (!cancelled) {
          setAttachError(error instanceof Error ? error.message : String(error));
        }
      } finally {
        if (!cancelled) {
          setIsAttaching(false);
        }
      }
    };

    attach();

    const host = hostRef.current;
    if (host) {
      observer = new ResizeObserver(() => {
        scheduleGeometrySync();
      });
      observer.observe(host);
    }

    visibilityTimer = window.setInterval(scheduleGeometrySync, 250);
    window.addEventListener('resize', scheduleGeometrySync);
    window.addEventListener('scroll', scheduleGeometrySync, true);
    document.addEventListener('visibilitychange', scheduleGeometrySync);
    document.addEventListener('pointerdown', scheduleGeometrySync, true);

    return () => {
      cancelled = true;
      observer?.disconnect();
      if (visibilityTimer !== null) {
        window.clearInterval(visibilityTimer);
      }
      window.removeEventListener('resize', scheduleGeometrySync);
      window.removeEventListener('scroll', scheduleGeometrySync, true);
      document.removeEventListener('visibilitychange', scheduleGeometrySync);
      document.removeEventListener('pointerdown', scheduleGeometrySync, true);
      if (resizeFrame !== null) {
        window.cancelAnimationFrame(resizeFrame);
      }
      const surfaceId = surfaceIdRef.current;
      surfaceIdRef.current = null;
      lastBoundsRef.current = null;
      lastVisibleRef.current = null;
      if (surfaceId) {
        api.nativeTerminalDetach(surfaceId).catch(() => undefined);
      }
    };
  }, [isActive, nodeId, paneId, readBounds, sessionId, syncSurfaceGeometry, terminalType]);

  useEffect(() => {
    const surfaceId = surfaceIdRef.current;
    if (!surfaceId) return;
    api.nativeTerminalUpdateSettings(surfaceId, font, theme).catch(() => undefined);
  }, [font, theme]);

  useEffect(() => {
    if (!attachResponse?.surfaceId) return;
    let cancelled = false;

    const refresh = () => {
      if (!cancelled) {
        void refreshSnapshot(attachResponse.surfaceId);
      }
    };

    refresh();
    const timer = window.setInterval(refresh, attachResponse.status === 'ready' ? 1000 : 1000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [attachResponse?.surfaceId, attachResponse?.status, refreshSnapshot]);

  const handleFocus = useCallback(() => {
    const surfaceId = surfaceIdRef.current;
    if (surfaceId) {
      api.nativeTerminalFocus(surfaceId).catch(() => undefined);
      hostRef.current?.focus();
    }
  }, []);

  const handleKeyDown = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    const surfaceId = surfaceIdRef.current;
    if (!surfaceId) return;

    if (event.key === 'PageUp') {
      event.preventDefault();
      api.nativeTerminalPageUp(surfaceId)
        .then(() => refreshSnapshot(surfaceId))
        .catch(() => undefined);
    } else if (event.key === 'PageDown') {
      event.preventDefault();
      api.nativeTerminalPageDown(surfaceId)
        .then(() => refreshSnapshot(surfaceId))
        .catch(() => undefined);
    } else if (event.key === 'End') {
      event.preventDefault();
      api.nativeTerminalScrollToBottom(surfaceId)
        .then(() => refreshSnapshot(surfaceId))
        .catch(() => undefined);
    }
  }, [refreshSnapshot]);

  const handleWheel = useCallback((event: React.WheelEvent<HTMLDivElement>) => {
    const surfaceId = surfaceIdRef.current;
    if (!surfaceId) return;
    // Browser wheel delta is positive when the user scrolls down. Alacritty's
    // display scroll delta is positive toward older scrollback, so invert here.
    const rowHeight = Math.max(1, font.size * font.lineHeight);
    const rowDelta = -Math.sign(event.deltaY) * Math.ceil(Math.abs(event.deltaY) / rowHeight);
    if (rowDelta === 0) return;
    event.preventDefault();
    api.nativeTerminalScroll(surfaceId, rowDelta)
      .then(() => refreshSnapshot(surfaceId))
      .catch(() => undefined);
  }, [font.lineHeight, font.size, refreshSnapshot]);

  const message = attachError || attachResponse?.message || t('terminal.native_alacritty.boundary');
  const isReady = attachResponse?.status === 'ready' && !attachError;

  if (isReady) {
    return (
      <div
        ref={hostRef}
        tabIndex={0}
        role="application"
        aria-label={t('terminal.native_alacritty.title')}
        onMouseDown={handleFocus}
        onKeyDown={handleKeyDown}
        onWheel={handleWheel}
        className="relative h-full w-full overflow-hidden bg-theme-bg text-theme-text outline-none"
        style={{
          fontFamily: font.familyCss ?? font.family,
          fontSize: `${font.size}px`,
          lineHeight: font.lineHeight,
          color: theme.foreground,
          backgroundColor: theme.background,
        }}
      >
        <div className="pointer-events-none absolute inset-0" aria-hidden="true" />
        <div className="pointer-events-none absolute bottom-2 right-2 rounded border border-theme-border bg-theme-bg/80 px-2 py-1 text-[10px] text-theme-text-muted">
          {snapshot
            ? `${snapshot.columns}x${snapshot.rows} · ${snapshot.visible ? 'visible' : 'hidden'} · ${Math.round(snapshot.bounds.x)},${Math.round(snapshot.bounds.y)} ${Math.round(snapshot.bounds.width)}x${Math.round(snapshot.bounds.height)} · ${snapshot.paneId} · ${snapshot.activeBuffer} · top ${snapshot.viewportTop}/${snapshot.scrollbackLen} · ${snapshot.followTail ? t('terminal.native_alacritty.follow_tail') : t('terminal.native_alacritty.pinned')}${snapshot.fontMetrics ? ` · ${snapshot.fontMetrics.resolvedFamily}` : ''}`
            : t('terminal.native_alacritty.attaching')}
        </div>
      </div>
    );
  }

  return (
    <div
      ref={hostRef}
      tabIndex={0}
      onMouseDown={handleFocus}
      className="flex h-full w-full items-center justify-center bg-theme-bg p-6 text-theme-text"
    >
      <div className="max-w-2xl rounded-lg border border-theme-border bg-theme-bg-card p-5 shadow-xl">
        <div className="mb-4 flex items-start gap-3">
          <div className="rounded-md border border-theme-accent/30 bg-theme-accent/10 p-2 text-theme-accent">
            {isAttaching ? <Loader2 className="h-5 w-5 animate-spin" /> : <SquareTerminal className="h-5 w-5" />}
          </div>
          <div className="min-w-0">
            <h3 className="text-base font-semibold text-theme-text-heading">
              {t('terminal.native_alacritty.title')}
            </h3>
            <p className="mt-1 text-sm leading-6 text-theme-text-muted">
              {t('terminal.native_alacritty.description')}
            </p>
          </div>
        </div>

        <div className="rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-theme-text">
          <div className="flex gap-2">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-300" />
            <p className="leading-6">{message}</p>
          </div>
        </div>

        <dl className="mt-4 grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-xs text-theme-text-muted">
          <dt>{t('terminal.native_alacritty.status')}</dt>
          <dd className="font-mono text-theme-text">{isAttaching ? t('terminal.native_alacritty.attaching') : (attachResponse?.status ?? 'error')}</dd>
          <dt>{t('terminal.native_alacritty.surface')}</dt>
          <dd className="font-mono text-theme-text">{attachResponse?.surfaceId ?? '-'}</dd>
          <dt>{t('terminal.native_alacritty.grid')}</dt>
          <dd className="font-mono text-theme-text">{snapshot ? `${snapshot.columns}x${snapshot.rows} r${snapshot.revision}` : '-'}</dd>
          <dt>{t('terminal.native_alacritty.parsed')}</dt>
          <dd className="font-mono text-theme-text">{snapshot ? `${snapshot.parsedBytes} bytes` : '-'}</dd>
          <dt>{t('terminal.native_alacritty.session')}</dt>
          <dd className="font-mono text-theme-text">{sessionId}</dd>
          <dt>{t('terminal.native_alacritty.pane')}</dt>
          <dd className="font-mono text-theme-text">{paneId}</dd>
          <dt>{t('terminal.native_alacritty.type')}</dt>
          <dd className="font-mono text-theme-text">{terminalType}</dd>
        </dl>

        <div className="mt-5 flex justify-end">
          <button
            type="button"
            onClick={() => updateTerminal('engine', 'xterm')}
            className="inline-flex items-center gap-2 rounded-md border border-theme-border bg-theme-bg-hover px-3 py-2 text-sm text-theme-text transition-colors hover:border-theme-accent/60 hover:text-theme-accent"
          >
            <RotateCcw className="h-4 w-4" />
            {t('terminal.native_alacritty.use_xterm')}
          </button>
        </div>
      </div>
    </div>
  );
};
