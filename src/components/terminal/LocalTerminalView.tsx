// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React, { useEffect, useRef, useState, useCallback } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { SearchAddon, ISearchOptions } from '@xterm/addon-search';
import { ImageAddon } from '@xterm/addon-image';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import '@xterm/xterm/css/xterm.css';
import { useSettingsStore } from '../../store/settingsStore';
import { useAppStore } from '../../store/appStore';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { getTerminalTheme } from '../../lib/themes';
import { getFontFamily } from '../../lib/fontFamily';
import { useTerminalViewShortcuts } from '../../hooks/useTerminalKeyboard';
import { SearchBar } from './SearchBar';
import { AiInlinePanel, type CursorPosition } from './AiInlinePanel';
import { PasteConfirmOverlay } from './PasteConfirmOverlay';
import { getProtectedPasteDecision } from '../../lib/terminalPaste';
import { terminalLinkHandler } from '../../lib/safeUrl';
import { listen } from '@tauri-apps/api/event';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import {
  hexToRgba,
  getBackgroundFitStyles,
  getWebglRendererInfo,
  logWebglRendererInfo,
  isLowEndGPU,
  forceViewportTransparent,
  clearViewportTransparent,
  isTerminalContainerRenderable,
  resolveTerminalDimensions,
  shouldAutoFocusTerminal,
  shouldFocusTerminalFromClick,
} from '../../lib/terminalHelpers';
import { formatTerminalTextInput } from '../../lib/terminalInput';
import { 
  registerTerminalBuffer, 
  unregisterTerminalBuffer,
  setActivePaneId as setRegistryActivePaneId,
  touchTerminalEntry,
  notifyTerminalOutput,
  updateTerminalReadiness,
} from '../../lib/terminalRegistry';
import { onMapleRegularLoaded, ensureCJKFallback, prepareTerminalFontForOpen } from '../../lib/fontLoader';
import { api } from '../../lib/api';
import { installTerminalClipboardSupport, readSystemClipboardText } from '../../lib/clipboardSupport';
import {
  armTerminalPasteShortcutSuppression,
  createTerminalPasteShortcutSuppressionState,
  markTerminalPasteShortcutHandled,
  shouldSuppressTerminalPasteEvent,
  takeTerminalPasteShortcutFallback,
} from '../../lib/terminalPasteShortcutGuard';
import { attachTerminalSmartCopy } from '../../hooks/useTerminalSmartCopy';
import { useTerminalRecording } from '../../hooks/useTerminalRecording';
import { useAdaptiveRenderer } from '../../hooks/useAdaptiveRenderer';
import { useTerminalAutosuggestRecorder } from '../../hooks/useTerminalAutosuggestRecorder';
import { observeCliAgentTerminalInput } from '../../lib/ai/orchestrator/cliAgents';
import { RecordingControls } from './RecordingControls';
import { FpsOverlay } from './FpsOverlay';
import { TerminalCommandBar } from './TerminalCommandBar';
import { useToastStore } from '../../hooks/useToast';
import { HighlightEngine } from '../../lib/terminal/highlightEngine';
import {
  applyRuntimeDisabledHighlightRules,
  getHighlightRulesSignature,
  markRuntimeDisabledHighlightRules,
} from '../../lib/terminal/runtimeDisabledHighlightRules';
import { installShiftSelectionGuard, type SelectionGestureController } from '../../lib/terminalSelectionGesture';
import { useBroadcastStore } from '../../store/broadcastStore';
import { broadcastToTargets } from '../../lib/terminalRegistry';
import type { HighlightRule } from '../../types';
import {
  encodeTerminalInput,
  formatTerminalEncodingLabel,
  normalizeTerminalEncoding,
  TerminalEncodingMismatchDetector,
  TerminalOutputDecoder,
  type TerminalEncoding,
} from '../../lib/terminalEncoding';

interface LocalTerminalViewProps {
  sessionId: string;
  isActive?: boolean;
  /** Unique pane ID for split pane support. If not provided, sessionId is used. */
  paneId?: string;
  /** Tab ID for registry security (prevents cross-tab context leakage) */
  tabId?: string;
  /** Callback when this pane receives focus */
  onFocus?: (paneId: string) => void;
}

const PREFILL_REPLAY_LINE_COUNT = 50; // Keep aligned with backend replay count

export const LocalTerminalView: React.FC<LocalTerminalViewProps> = ({ 
  sessionId, 
  isActive = true,
  paneId,
  tabId: propTabId,
  onFocus,
}) => {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const webLinksAddonRef = useRef<WebLinksAddon | null>(null);
  const searchAddonRef = useRef<SearchAddon | null>(null);
  const imageAddonRef = useRef<ImageAddon | null>(null);
  const clipboardAddonRef = useRef<{ dispose: () => void } | null>(null);
  const selectionGestureRef = useRef<SelectionGestureController | null>(null);
  const smartCopyDisposableRef = useRef<{ dispose: () => void } | null>(null);
  const highlightEngineRef = useRef<HighlightEngine | null>(null);
  const runtimeDisabledHighlightRulesRef = useRef<Map<string, string>>(new Map());
  const runtimeDisabledHighlightRulesSourceRef = useRef<string | null>(null);
  const rendererAddonRef = useRef<{ dispose: () => void } | null>(null);
  const rendererSuspendedRef = useRef(false);
  const rendererTransitionTokenRef = useRef(0);
  const pasteShortcutSuppressionRef = useRef(createTerminalPasteShortcutSuppressionState());
  const isComposingRef = useRef(false);
  // xterm.js event listener disposables - must be explicitly disposed to prevent memory leaks
  const onDataDisposableRef = useRef<{ dispose: () => void } | null>(null);
  const onBinaryDisposableRef = useRef<{ dispose: () => void } | null>(null);
  
  // Get tab ID for this terminal (used for registry validation)
  // Use prop if provided, otherwise look up from store
  const storeTabId = useAppStore((state) => 
    state.tabs.find(t => t.type === 'local_terminal' && t.sessionId === sessionId)?.id
  );
  const effectiveTabId = propTabId || storeTabId || '';
  
  // Effective pane ID: use provided paneId or fall back to sessionId
  const effectivePaneId = paneId || sessionId;
  
  const isMountedRef = useRef(true);
  const isActiveRef = useRef(isActive);
  const [searchOpen, setSearchOpen] = useState(false);
  const [aiPanelOpen, setAiPanelOpen] = useState(false);
  const [aiCursorPosition, setAiCursorPosition] = useState<CursorPosition | null>(null);
  const [isRunning, setIsRunning] = useState(true);
  const isRunningRef = useRef(true);

  // Mouse tracking mode indicator (tmux/vim mouse capture)
  const [mouseMode, setMouseMode] = useState(false);

  // Paste protection state
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  
  // Search state
  const [searchResults, setSearchResults] = useState<{ resultIndex: number; resultCount: number }>({ 
    resultIndex: -1, 
    resultCount: 0 
  });
  const currentSearchQueryRef = useRef<string>('');
  const currentSearchOptionsRef = useRef<ISearchOptions | undefined>(undefined);

  // RAF buffering delegated to useAdaptiveRenderer (adaptive FPS)
  // pendingDataRef / rafIdRef are no longer needed here.
  
  // Search pause mechanism: pause search updates during heavy output bursts
  // This prevents the "1->2->3->1" cycling when buffer changes rapidly
  const searchPausedRef = useRef(false);
  const outputThrottleRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const prefillHistoryRef = useRef(false);

  // Get terminal settings (read early for adaptive renderer)
  const terminalSettings = useSettingsStore((state) => state.settings.terminal);
  const terminalEncoding = normalizeTerminalEncoding(terminalSettings.terminalEncoding);
  const terminalEncodingRef = useRef<TerminalEncoding>(terminalEncoding);
  const terminalOutputDecoderRef = useRef(new TerminalOutputDecoder(terminalEncoding));
  const encodingMismatchDetectorRef = useRef(new TerminalEncodingMismatchDetector());
  const encodingHintToastShownRef = useRef(false);
  const encodedInputQueueRef = useRef<Promise<void>>(Promise.resolve());
  const [fontOpenReady, setFontOpenReady] = useState(false);

  useEffect(() => {
    terminalEncodingRef.current = terminalEncoding;
    terminalOutputDecoderRef.current.reset();
    terminalOutputDecoderRef.current = new TerminalOutputDecoder(terminalEncoding);
    encodingMismatchDetectorRef.current.reset();
    encodingHintToastShownRef.current = false;
  }, [terminalEncoding]);

  useEffect(() => {
    let cancelled = false;
    if (terminalRef.current) {
      void prepareTerminalFontForOpen(terminalSettings.fontFamily);
      return () => {
        cancelled = true;
      };
    }
    setFontOpenReady(false);
    void prepareTerminalFontForOpen(terminalSettings.fontFamily).finally(() => {
      if (!cancelled) {
        setFontOpenReady(true);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [terminalSettings.fontFamily]);

  useEffect(() => {
    isActiveRef.current = isActive;
  }, [isActive]);

  // ── Adaptive Renderer (Dynamic Refresh Rate) ──────────────────────────
  const adaptiveRenderer = useAdaptiveRenderer({
    terminalRef,
    mode: terminalSettings.adaptiveRenderer ?? 'auto',
  });

  // Stable ref so event listener effects never depend on the handle identity.
  // Callbacks read .current at call-time, making the effect deps truly stable.
  const adaptiveRendererRef = useRef(adaptiveRenderer);
  adaptiveRendererRef.current = adaptiveRenderer;

  const focusTerminal = useCallback((mode: 'soft' | 'strong' = 'soft') => {
    const term = terminalRef.current;
    if (!term || searchOpen || aiPanelOpen || !isTerminalContainerRenderable(containerRef.current)) {
      return false;
    }
    if (mode === 'soft' && !shouldAutoFocusTerminal(containerRef.current)) {
      return false;
    }
    term.focus();
    return true;
  }, [searchOpen, aiPanelOpen]);

  const { writeTerminal, resizeTerminal, getTerminal, updateTerminalState } = useLocalTerminalStore();
  const terminalInfo = getTerminal(sessionId);

  const writeEncodedTerminalInput = useCallback((input: string) => {
    const write = (bytes: Uint8Array) => {
      void writeTerminal(sessionId, bytes);
    };
    const encoded = encodeTerminalInput(input, terminalEncodingRef.current);
    if (encoded instanceof Uint8Array) {
      write(encoded);
      return;
    }

    encodedInputQueueRef.current = encodedInputQueueRef.current
      .catch(() => {})
      .then(async () => {
        write(await encoded);
      })
      .catch((error) => {
        console.error('[LocalTerminalView] Failed to encode terminal input:', error);
      });
  }, [sessionId, writeTerminal]);

  const autosuggestRecorder = useTerminalAutosuggestRecorder({
    terminalKind: 'local_terminal',
    localShellHistory: terminalSettings.autosuggest.localShellHistory,
  });
  const autosuggestRecorderRef = useRef(autosuggestRecorder);
  autosuggestRecorderRef.current = autosuggestRecorder;

  const maybeSuggestTerminalEncoding = useCallback((payload: Uint8Array) => {
    if (terminalEncodingRef.current !== 'utf-8' || encodingHintToastShownRef.current) {
      return;
    }

    const detection = encodingMismatchDetectorRef.current.observe(payload);
    if (!detection || detection.suggestions.length === 0) {
      return;
    }

    encodingHintToastShownRef.current = true;
    const suggestions = detection.suggestions.slice(0, 3);
    const labels = suggestions.map(formatTerminalEncodingLabel);
    useToastStore.getState().addToast({
      title: t('terminal.encoding_hint.title'),
      description: t('terminal.encoding_hint.description', { encodings: labels.join(' / ') }),
      variant: 'warning',
      duration: 15000,
      actions: suggestions.map((encoding) => ({
        label: t('terminal.encoding_hint.use_encoding', { encoding: formatTerminalEncodingLabel(encoding) }),
        onClick: () => {
          useSettingsStore.getState().updateTerminal('terminalEncoding', encoding);
        },
      })),
    });
  }, [t]);

  const syncLocalPtySize = useCallback(() => {
    const dims = resolveTerminalDimensions(containerRef.current, terminalRef.current, fitAddonRef.current);
    if (!dims) return;

    resizeTerminal(sessionId, dims.cols, dims.rows);
  }, [resizeTerminal, sessionId]);

  const handleCommandBarLayoutChange = useCallback(() => {
    if (!fitAddonRef.current || !terminalRef.current || !isTerminalContainerRenderable(containerRef.current)) return;
    fitAddonRef.current.fit();
    syncLocalPtySize();
  }, [syncLocalPtySize]);

  const getEffectiveHighlightRules = useCallback((rules: HighlightRule[]) => {
    const rulesSignature = getHighlightRulesSignature(rules);
    if (
      runtimeDisabledHighlightRulesSourceRef.current
      && runtimeDisabledHighlightRulesSourceRef.current !== rulesSignature
    ) {
      runtimeDisabledHighlightRulesRef.current.clear();
    }

    runtimeDisabledHighlightRulesSourceRef.current = rulesSignature;
    return applyRuntimeDisabledHighlightRules(runtimeDisabledHighlightRulesRef.current, rules);
  }, []);

  const ensureSearchAddon = useCallback(() => {
    const term = terminalRef.current;
    if (!term) return null;
    if (searchAddonRef.current) return searchAddonRef.current;
    const addon = new SearchAddon();
    addon.onDidChangeResults((e) => {
      if (currentSearchQueryRef.current && !searchPausedRef.current) {
        setSearchResults({ resultIndex: e.resultIndex, resultCount: e.resultCount });
      }
    });
    term.loadAddon(addon);
    searchAddonRef.current = addon;
    return addon;
  }, []);

  const maybeLoadImageAddon = useCallback((payload: Uint8Array) => {
    if (imageAddonRef.current || !terminalRef.current) return;
    for (let i = 0; i < payload.length - 2; i++) {
      if (payload[i] !== 0x1b) continue;
      const next = payload[i + 1];
      if (next === 0x5d) {
        // ESC ] 1337 ;
        if (
          i + 6 < payload.length &&
          payload[i + 2] === 0x31 &&
          payload[i + 3] === 0x33 &&
          payload[i + 4] === 0x33 &&
          payload[i + 5] === 0x37 &&
          payload[i + 6] === 0x3b
        ) {
          const addon = new ImageAddon({
            enableSizeReports: true,
            pixelLimit: 16777216,
            storageLimit: 32,
            showPlaceholder: true,
            sixelSupport: true,
            iipSupport: true,
          });
          terminalRef.current.loadAddon(addon);
          imageAddonRef.current = addon;
          return;
        }
      } else if (next === 0x50 && payload[i + 2] === 0x71) {
        // ESC P q (SIXEL)
        const addon = new ImageAddon({
          enableSizeReports: true,
          pixelLimit: 16777216,
          storageLimit: 32,
          showPlaceholder: true,
          sixelSupport: true,
          iipSupport: true,
        });
        terminalRef.current.loadAddon(addon);
        imageAddonRef.current = addon;
        return;
      }
    }
  }, []);

  // ── Session Recording ──────────────────────────────────────────────────────
  const {
    startRecording,
    feedOutput,
    feedInput,
    feedResize,
    handleRecordingStop,
    handleRecordingDiscard,
    isRecording: isSessionRecording,
    recorderRef,
  } = useTerminalRecording({
    sessionId,
    terminalType: 'local',
    label: terminalInfo?.shell?.path || sessionId,
  });

  const sendCommandBarInput = useCallback((input: string) => {
    if (!isRunningRef.current) return;
    adaptiveRendererRef.current.notifyUserInput();
    autosuggestRecorderRef.current.observeInput(input);
    observeCliAgentTerminalInput({
      data: input,
      targetId: 'local-shell:default',
      sessionId,
    });
    feedInput(input);
    writeEncodedTerminalInput(input);
  }, [feedInput, sessionId, writeEncodedTerminalInput]);

  // ── Listen for TabBar recording events ──────────────────────────────────
  useEffect(() => {
    const onStartRec = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail?.sessionId !== sessionId) return;
      const term = terminalRef.current;
      if (term && !isSessionRecording) startRecording(term.cols, term.rows);
    };
    const onRecStopped = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail?.sessionId !== sessionId) return;
      if (detail?.content) handleRecordingStop(detail.content);
    };
    window.addEventListener('oxide:start-recording', onStartRec);
    window.addEventListener('oxide:recording-stopped', onRecStopped);
    return () => {
      window.removeEventListener('oxide:start-recording', onStartRec);
      window.removeEventListener('oxide:recording-stopped', onRecStopped);
    };
  }, [sessionId, isSessionRecording, startRecording, handleRecordingStop]);

  // terminalSettings already read above (before adaptive renderer hook)

  // Font family resolver — see src/lib/fontFamily.ts

  // Subscribe to terminal settings changes
  useEffect(() => {
    const unsubscribe = useSettingsStore.subscribe(
      (state) => state.settings.terminal,
      (terminal) => {
        const term = terminalRef.current;
        if (!term) return;
        
        term.options.fontFamily = getFontFamily(terminal.fontFamily, terminal.customFontFamily);
        term.options.fontSize = terminal.fontSize;
        term.options.cursorStyle = terminal.cursorStyle;
        term.options.cursorBlink = terminal.cursorBlink;
        term.options.lineHeight = terminal.lineHeight;
        
        // Apply theme — use transparent background when bg image is set
        const enabledTabs = terminal.backgroundEnabledTabs ?? ['terminal', 'local_terminal'];
        const hasBg = terminal.backgroundEnabled !== false && !!terminal.backgroundImage && enabledTabs.includes('local_terminal');
        const themeConfig = getTerminalTheme(terminal.theme);
        term.options.theme = hasBg
          ? { ...themeConfig, background: hexToRgba(themeConfig.background || '#09090b', 0.01), overviewRulerBorder: 'transparent', scrollbarSliderBackground: 'rgba(255,255,255,0.15)', scrollbarSliderHoverBackground: 'rgba(255,255,255,0.3)', scrollbarSliderActiveBackground: 'rgba(255,255,255,0.4)' }
          : { ...themeConfig, overviewRulerBorder: 'transparent', scrollbarSliderBackground: 'rgba(255,255,255,0.15)', scrollbarSliderHoverBackground: 'rgba(255,255,255,0.3)', scrollbarSliderActiveBackground: 'rgba(255,255,255,0.4)' };

        if (hasBg) {
          forceViewportTransparent(containerRef.current);
        } else {
          clearViewportTransparent(containerRef.current);
        }

        highlightEngineRef.current?.updateRules(getEffectiveHighlightRules(terminal.highlightRules));
        
        term.refresh(0, term.rows - 1);
        // Delay fit to next frame so xterm recalculates glyph metrics with new fontSize
        requestAnimationFrame(() => {
          const fitAddon = fitAddonRef.current;
          if (!fitAddon || !isTerminalContainerRenderable(containerRef.current)) return;
          fitAddon.fit();
          syncLocalPtySize();
        });
      }
    );
    return unsubscribe;
  }, [sessionId, resizeTerminal, syncLocalPtySize]);

  // CJK Font lazy loading: refresh terminal ONCE when Maple Mono Regular loads
  // Only Regular triggers refresh, secondary weights (Bold/Italic) load silently
  useEffect(() => {
    // Trigger CJK font preload in background (non-blocking)
    ensureCJKFallback();
    
    // Listen for Regular weight load completion only (prevents 4x refresh)
    const unsubscribe = onMapleRegularLoaded(() => {
      const term = terminalRef.current;
      const fitAddon = fitAddonRef.current;
      if (!term || !fitAddon || !isTerminalContainerRenderable(containerRef.current)) return;
      
      // Refresh terminal to apply newly loaded CJK font
      term.refresh(0, term.rows - 1);
      fitAddon.fit();

      const dims = resolveTerminalDimensions(containerRef.current, term, fitAddon);
      if (dims && import.meta.env.DEV) {
        console.log(`[LocalTerminalView] CJK font loaded, synced resize: ${dims.cols}x${dims.rows}`);
      }

      syncLocalPtySize();
    });
    
    return unsubscribe;
  }, [sessionId, resizeTerminal, syncLocalPtySize]);

  // Focus terminal when active
  useEffect(() => {
    if (isActive && terminalRef.current && !searchOpen && !aiPanelOpen) {
      const focusTimeout = setTimeout(() => {
        focusTerminal('soft');
        if (!isTerminalContainerRenderable(containerRef.current)) return;
        fitAddonRef.current?.fit();
        syncLocalPtySize();
      }, 50);
      return () => clearTimeout(focusTimeout);
    }
  }, [focusTerminal, isActive, searchOpen, aiPanelOpen, syncLocalPtySize]);

  // Suspend heavy renderer while tab is inactive, and restore on activation.
  useEffect(() => {
    const term = terminalRef.current;
    if (!term) return;
    const transitionToken = ++rendererTransitionTokenRef.current;
    let fitRaf1: number | null = null;
    let fitRaf2: number | null = null;
    const isStale = () =>
      transitionToken !== rendererTransitionTokenRef.current || !terminalRef.current;

    term.options.cursorBlink = isActive ? terminalSettings.cursorBlink : false;

    if (!isActive) {
      if (rendererAddonRef.current) {
        try {
          rendererAddonRef.current.dispose();
        } catch {
          // Ignore renderer disposal errors during suspend.
        }
        rendererAddonRef.current = null;
        rendererSuspendedRef.current = true;
      }
      return () => {
        if (fitRaf1 !== null) cancelAnimationFrame(fitRaf1);
        if (fitRaf2 !== null) cancelAnimationFrame(fitRaf2);
      };
    }

    if (!rendererSuspendedRef.current || rendererAddonRef.current) {
      return () => {
        if (fitRaf1 !== null) cancelAnimationFrame(fitRaf1);
        if (fitRaf2 !== null) cancelAnimationFrame(fitRaf2);
      };
    }

    const restoreRenderer = async () => {
      const currentTerm = terminalRef.current;
      if (!currentTerm || isStale()) return;
      const rendererSetting = terminalSettings.renderer || 'auto';

      const loadCanvasAddon = async (): Promise<{ dispose: () => void } | null> => {
        try {
          const { CanvasAddon } = await import('@xterm/addon-canvas/lib/xterm-addon-canvas.mjs');
          if (isStale()) return null;
          const canvasAddon = new CanvasAddon();
          currentTerm.loadAddon(canvasAddon);
          if (isStale()) {
            canvasAddon.dispose();
            return null;
          }
          return canvasAddon;
        } catch {
          return null;
        }
      };

      if (rendererSetting === 'canvas') {
        rendererAddonRef.current = await loadCanvasAddon();
      } else if (rendererSetting === 'webgl') {
        try {
          if (isStale()) return;
          const webglAddon = new WebglAddon();
          webglAddon.onContextLoss(() => {
            webglAddon.dispose();
            if (!isStale()) {
              rendererAddonRef.current = null;
            }
          });
          currentTerm.loadAddon(webglAddon);
          if (isStale()) {
            webglAddon.dispose();
            return;
          }
          rendererAddonRef.current = webglAddon;
        } catch {
          rendererAddonRef.current = await loadCanvasAddon();
        }
      } else {
        const rendererInfo = getWebglRendererInfo();
        if (rendererInfo?.isSoftwareRenderer) {
          console.warn(
            `[LocalTerminal] WebGL renderer "${rendererInfo.renderer ?? 'unknown'}" appears software-backed, using Canvas in auto mode`,
          );
          rendererAddonRef.current = await loadCanvasAddon();
        } else {
          try {
            if (isStale()) return;
            const webglAddon = new WebglAddon();
            webglAddon.onContextLoss(async () => {
              webglAddon.dispose();
              if (!isStale()) {
                rendererAddonRef.current = await loadCanvasAddon();
              }
            });
            currentTerm.loadAddon(webglAddon);
            if (isStale()) {
              webglAddon.dispose();
              return;
            }
            rendererAddonRef.current = webglAddon;
          } catch {
            rendererAddonRef.current = await loadCanvasAddon();
          }
        }
      }

      if (isStale()) return;
      rendererSuspendedRef.current = false;

      // Re-force transparent after renderer restore
      if (terminalSettings.backgroundEnabled !== false
        && terminalSettings.backgroundImage
        && (terminalSettings.backgroundEnabledTabs ?? ['terminal', 'local_terminal']).includes('local_terminal')) {
        forceViewportTransparent(containerRef.current);
      }

      fitRaf1 = requestAnimationFrame(() => {
        fitRaf2 = requestAnimationFrame(() => {
          if (!isStale()) {
            // Flush any pending option changes (fontSize, theme, etc.) that
            // were applied while the renderer was disposed.
            const t = terminalRef.current;
            if (t) t.refresh(0, t.rows - 1);
            if (!isTerminalContainerRenderable(containerRef.current)) return;
            fitAddonRef.current?.fit();
            syncLocalPtySize();
          }
        });
      });
    };

    void restoreRenderer();

    return () => {
      if (fitRaf1 !== null) cancelAnimationFrame(fitRaf1);
      if (fitRaf2 !== null) cancelAnimationFrame(fitRaf2);
    };
  }, [isActive, terminalSettings.cursorBlink, terminalSettings.renderer, syncLocalPtySize]);

  // Initialize terminal
  useEffect(() => {
    if (!fontOpenReady) return;
    if (!containerRef.current || terminalRef.current) return;
    
    isMountedRef.current = true;
    let clipboardInitCancelled = false;

    const hasBgImage = terminalSettings.backgroundEnabled !== false
      && !!terminalSettings.backgroundImage
      && (terminalSettings.backgroundEnabledTabs ?? ['terminal', 'local_terminal']).includes('local_terminal');
    const baseTheme = getTerminalTheme(terminalSettings.theme);
    const xtermTheme = hasBgImage
      ? { ...baseTheme, background: hexToRgba(baseTheme.background || '#09090b', 0.01), overviewRulerBorder: 'transparent', scrollbarSliderBackground: 'rgba(255,255,255,0.15)', scrollbarSliderHoverBackground: 'rgba(255,255,255,0.3)', scrollbarSliderActiveBackground: 'rgba(255,255,255,0.4)' }
      : { ...baseTheme, overviewRulerBorder: 'transparent', scrollbarSliderBackground: 'rgba(255,255,255,0.15)', scrollbarSliderHoverBackground: 'rgba(255,255,255,0.3)', scrollbarSliderActiveBackground: 'rgba(255,255,255,0.4)' };

    const term = new Terminal({
      cursorBlink: terminalSettings.cursorBlink,
      cursorStyle: terminalSettings.cursorStyle,
      fontFamily: getFontFamily(terminalSettings.fontFamily, terminalSettings.customFontFamily),
      fontSize: terminalSettings.fontSize,
      lineHeight: terminalSettings.lineHeight,
      theme: xtermTheme,
      scrollback: terminalSettings.scrollback || 5000,
      allowProposedApi: true,
      fastScrollSensitivity: 5,
      drawBoldTextInBrightColors: true,
      // Controls the custom SmoothScrollableElement scrollbar width in xterm 6.0.
      overviewRuler: { width: 10 },
      // Always enable so bg image can be toggled without remounting
      allowTransparency: true,
    });

    const fitAddon = new FitAddon();
    // WebLinksAddon with secure URL handler - blocks dangerous protocols (file://, javascript:, etc.)
    const webLinksAddon = new WebLinksAddon(terminalLinkHandler);
    
    term.loadAddon(fitAddon);
    term.loadAddon(webLinksAddon);
    webLinksAddonRef.current = webLinksAddon;
    // SearchAddon and ImageAddon are loaded lazily to reduce memory usage
    
    // Unicode11Addon for proper Nerd Font icons and CJK wide character rendering
    // Required for Oh My Posh, Starship, and other modern prompts
    const unicode11Addon = new Unicode11Addon();
    term.loadAddon(unicode11Addon);
    term.unicode.activeVersion = '11';

    // OSC 7 shell integration: capture current working directory
    // Shells emit \x1b]7;file://hostname/path\x07 on directory change
    term.parser.registerOscHandler(7, (data: string) => {
      try {
        const url = data.startsWith('file://') ? new URL(data) : null;
        const cwd = url ? decodeURIComponent(url.pathname) : data;
        const host = url?.hostname;
        if (cwd) {
          import('../../lib/terminalRegistry').then(({ updateCwd }) => {
            updateCwd(effectivePaneId, cwd, host);
          });
        }
      } catch {
        // Malformed URL — ignore silently
      }
      return false;
    });

    void installTerminalClipboardSupport(term).then((addon) => {
      if (clipboardInitCancelled) {
        addon.dispose();
        return;
      }
      clipboardAddonRef.current = addon;
    });

    highlightEngineRef.current = new HighlightEngine(term, getEffectiveHighlightRules(terminalSettings.highlightRules), {
      onRulesAutoDisabled: handleHighlightRulesAutoDisabled,
    });

    // Detect mouse tracking mode changes (tmux, vim, etc.)
    let prevMouseTracking = false;
    term.onWriteParsed(() => {
      const active = term.modes.mouseTrackingMode !== 'none';
      if (active !== prevMouseTracking) {
        prevMouseTracking = active;
        setMouseMode(active);
      }
      notifyTerminalOutput(sessionId);
    });


    // Load renderer (WebGL or Canvas)
    const loadRenderer = async () => {
      const rendererSetting = terminalSettings.renderer || 'auto';
      
      const loadCanvasAddon = async (): Promise<{ dispose: () => void } | null> => {
        try {
          const { CanvasAddon } = await import('@xterm/addon-canvas/lib/xterm-addon-canvas.mjs');
          const canvasAddon = new CanvasAddon();
          term.loadAddon(canvasAddon);
          return canvasAddon;
        } catch (e) {
          console.warn('[LocalTerminal] Canvas addon failed', e);
          return null;
        }
      };
      
      if (rendererSetting === 'canvas') {
        const addon = await loadCanvasAddon();
        if (addon) {
          rendererAddonRef.current = addon;
        }
      } else if (rendererSetting === 'webgl') {
        try {
          logWebglRendererInfo('[LocalTerminal:webgl]', getWebglRendererInfo());
          const webglAddon = new WebglAddon();
          webglAddon.onContextLoss(() => {
            webglAddon.dispose();
            rendererAddonRef.current = null;
          });
          term.loadAddon(webglAddon);
          rendererAddonRef.current = webglAddon;
        } catch (e) {
          console.warn('[LocalTerminal] WebGL failed', e);
        }
      } else {
        // Auto: try WebGL first, fallback to Canvas
        const rendererInfo = getWebglRendererInfo();
        logWebglRendererInfo('[LocalTerminal:auto]', rendererInfo);
        if (rendererInfo?.isSoftwareRenderer) {
          console.warn(
            `[LocalTerminal] WebGL renderer "${rendererInfo.renderer ?? 'unknown'}" appears software-backed, using Canvas in auto mode`,
          );
          const canvasAddon = await loadCanvasAddon();
          rendererAddonRef.current = canvasAddon;
          if (canvasAddon) {
            console.log('[LocalTerminal] Auto mode selected Canvas (software WebGL renderer detected)');
          }
        } else {
          try {
            const webglAddon = new WebglAddon();
            webglAddon.onContextLoss(async () => {
              webglAddon.dispose();
              const canvasAddon = await loadCanvasAddon();
              rendererAddonRef.current = canvasAddon;
            });
            term.loadAddon(webglAddon);
            rendererAddonRef.current = webglAddon;
            console.log('[LocalTerminal] Auto mode selected WebGL');
          } catch (e) {
            const canvasAddon = await loadCanvasAddon();
            rendererAddonRef.current = canvasAddon;
            if (canvasAddon) {
              console.log('[LocalTerminal] Auto mode selected Canvas after WebGL load failure');
            }
          }
        }
      }
    };

    term.open(containerRef.current);
    const handleCompositionStart = () => {
      isComposingRef.current = true;
    };
    const handleCompositionEnd = () => {
      isComposingRef.current = false;
    };
    containerRef.current.addEventListener('compositionstart', handleCompositionStart);
    containerRef.current.addEventListener('compositionend', handleCompositionEnd);
    smartCopyDisposableRef.current = attachTerminalSmartCopy(term, {
      isActive: () => isActiveRef.current,
      isEnabled: () => useSettingsStore.getState().settings.terminal.smartCopy,
      isCopyOnSelectEnabled: () => useSettingsStore.getState().settings.terminal.copyOnSelect,
      isMiddleClickPasteEnabled: () => useSettingsStore.getState().settings.terminal.middleClickPaste,
      onPasteShortcut: handlePasteShortcut,
      container: containerRef.current,
    });
    selectionGestureRef.current = installShiftSelectionGuard(
      term,
      () => useSettingsStore.getState().settings.terminal.selectionRequiresShift,
    );

    // Force xterm DOM elements transparent for bg image
    if (hasBgImage) {
      forceViewportTransparent(containerRef.current);
    }

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;
    if (isActiveRef.current) {
      focusTerminal('soft');
    }

    const writeWelcomeMessage = () => {
      term.writeln(`\x1b[32m${t('terminal.local.title')}\x1b[0m`);
      term.writeln(t('terminal.local.shell', { shell: terminalInfo?.shell.label || t('terminal.local.shell_unknown') }));
      term.writeln('');
    };

    // Check for pending replay data (from reattached background session)
    const replayData = useLocalTerminalStore.getState().consumeReplay(sessionId);
    if (replayData && replayData.length > 0) {
      const bytes = new Uint8Array(replayData);
      term.write(bytes);
    }

    const prefillHistory = async (): Promise<boolean> => {
      // Skip prefill if we already replayed background data
      if (replayData && replayData.length > 0) return true;
      if (prefillHistoryRef.current) return false;
      prefillHistoryRef.current = true;
      try {
        const stats = await api.getBufferStats(sessionId);
        const desired = Math.min(terminalSettings.scrollback || 5000, stats.current_lines);
        const prefillCount = Math.max(desired - PREFILL_REPLAY_LINE_COUNT, 0);
        if (prefillCount <= 0) {
          return stats.current_lines > 0;
        }
        const startLine = Math.max(
          stats.current_lines - PREFILL_REPLAY_LINE_COUNT - prefillCount,
          0,
        );
        const lines = await api.getScrollBuffer(sessionId, startLine, prefillCount);
        if (!isMountedRef.current || !terminalRef.current) return stats.current_lines > 0;
        if (lines.length > 0) {
          const text = lines.map((line) => line.ansi_text ?? line.text).join('\r\n') + '\r\n';
          terminalRef.current.write(text);
        }
        return stats.current_lines > 0;
      } catch {
        return false;
      }
    };

    void prefillHistory().then((hasHistory) => {
      if (!hasHistory && isMountedRef.current && terminalRef.current) {
        writeWelcomeMessage();
      }
    });

    // Buffer getter for AI context capture
    const getBufferContent = (): string => {
      const buffer = term.buffer.active;
      const lines: string[] = [];
      const lineCount = buffer.length;
      // Read last 500 lines (scrollback + visible) for AI context
      const maxLines = 500;
      const startRow = Math.max(0, lineCount - maxLines);
      for (let i = startRow; i < lineCount; i++) {
        const line = buffer.getLine(i);
        if (line) {
          lines.push(line.translateToString(true));
        }
      }
      return lines.join('\n');
    };
    
    // Selection getter for AI sidebar context
    const getSelectionContent = (): string => {
      return term.getSelection() || '';
    };

    // Screen reader for TUI interaction (experimental)
    const getScreenSnapshot = (): import('@/types').ScreenSnapshot | null => {
      const buffer = term.buffer.active;
      const rows = term.rows;
      const cols = term.cols;
      const lines: string[] = [];

      for (let i = 0; i < rows; i++) {
        const line = buffer.getLine(buffer.baseY + i);
        lines.push(line ? line.translateToString(false) : '');
      }

      return {
        lines,
        cursorX: buffer.cursorX + 1,  // Convert from 0-based to 1-based
        cursorY: buffer.cursorY + 1,  // Convert from 0-based to 1-based
        rows,
        cols,
        isAlternateBuffer: term.buffer.active.type === 'alternate',
      };
    };

    // Register buffer getter for AI context capture
    // Now uses paneId as key (for split pane support)
    registerTerminalBuffer(
      effectivePaneId,
      effectiveTabId,
      sessionId,
      'local_terminal',
      getBufferContent,
      getSelectionContent,  // Include selection getter
      // Writer function: send data to local PTY via Tauri command
      (data: string) => {
        writeEncodedTerminalInput(data);
      },
      getScreenSnapshot,  // Screen reader for TUI interaction
    );

    // Initial fit
    setTimeout(() => {
      if (!isTerminalContainerRenderable(containerRef.current)) return;
      fitAddon.fit();
      syncLocalPtySize();
    }, 0);

    loadRenderer();

    // Handle terminal data input
    // IMPORTANT: Save IDisposable for cleanup to prevent memory leaks
    onDataDisposableRef.current = term.onData((data) => {
      if (!isRunningRef.current) return;
      // Notify adaptive renderer of user activity (exits idle tier)
      adaptiveRendererRef.current.notifyUserInput();
      autosuggestRecorderRef.current.observeInput(data);
      observeCliAgentTerminalInput({
        data,
        targetId: 'local-shell:default',
        sessionId,
      });
      // Feed recording (user input)
      feedInput(data);
      writeEncodedTerminalInput(data);

      // Broadcast input to targets (empty target set = all other terminals)
      const bc = useBroadcastStore.getState();
      if (bc.enabled) {
        broadcastToTargets(effectivePaneId, data, bc.targets);
      }
    });

    // Handle terminal binary input (for special keys)
    // IMPORTANT: Save IDisposable for cleanup to prevent memory leaks
    onBinaryDisposableRef.current = term.onBinary((data) => {
      if (!isRunningRef.current) return;
      const bytes = new Uint8Array(data.length);
      for (let i = 0; i < data.length; i++) {
        bytes[i] = data.charCodeAt(i);
      }
      writeTerminal(sessionId, bytes);
    });

    // Track focus for split pane support
    // Update active pane in Registry when terminal receives focus
    // Note: xterm.js doesn't have onFocus, use DOM event on container
    const handleTerminalFocus = () => {
      setRegistryActivePaneId(effectivePaneId);
      touchTerminalEntry(effectivePaneId);
      onFocus?.(effectivePaneId);
    };
    
    // Add focus listener to terminal's element
    const termElement = term.element;
    if (termElement) {
      termElement.addEventListener('focusin', handleTerminalFocus);
    }

    return () => {
      isMountedRef.current = false;
      clipboardInitCancelled = true;
      
      // Unregister buffer getter (using paneId, not sessionId)
      unregisterTerminalBuffer(effectivePaneId);
      
      // Remove focus listener
      if (termElement) {
        termElement.removeEventListener('focusin', handleTerminalFocus);
      }
      containerRef.current?.removeEventListener('compositionstart', handleCompositionStart);
      containerRef.current?.removeEventListener('compositionend', handleCompositionEnd);
      
      // Dispose renderer addon first to avoid "onShowLinkUnderline" error
      // This is a known xterm.js canvas addon bug where dispose order matters
      if (rendererAddonRef.current) {
        try {
          rendererAddonRef.current.dispose();
        } catch (e) {
          // Ignore errors during addon disposal
        }
        rendererAddonRef.current = null;
      }
      
      // Dispose clipboard addon
      if (clipboardAddonRef.current) {
        try {
          clipboardAddonRef.current.dispose();
        } catch (e) {
          // Ignore errors during addon disposal
        }
        clipboardAddonRef.current = null;
      }

      if (selectionGestureRef.current) {
        selectionGestureRef.current.dispose();
        selectionGestureRef.current = null;
      }

      if (smartCopyDisposableRef.current) {
        try {
          smartCopyDisposableRef.current.dispose();
        } catch (e) {
          // Ignore errors during addon disposal
        }
        smartCopyDisposableRef.current = null;
      }

      if (highlightEngineRef.current) {
        try {
          highlightEngineRef.current.dispose();
        } catch (e) {
          // Ignore errors during cleanup
        }
        highlightEngineRef.current = null;
      }

      runtimeDisabledHighlightRulesRef.current.clear();
      runtimeDisabledHighlightRulesSourceRef.current = null;

      if (imageAddonRef.current) {
        try {
          imageAddonRef.current.dispose();
        } catch (e) {
          // Ignore errors during addon disposal
        }
        imageAddonRef.current = null;
      }

      // Dispose search addon to free index memory
      if (searchAddonRef.current) {
        try {
          searchAddonRef.current.dispose();
        } catch (e) {
          // Ignore errors during addon disposal
        }
        searchAddonRef.current = null;
      }

      // Dispose web links addon
      if (webLinksAddonRef.current) {
        try {
          webLinksAddonRef.current.dispose();
        } catch (e) {
          // Ignore errors during addon disposal
        }
        webLinksAddonRef.current = null;
      }

      // Dispose fit addon
      if (fitAddonRef.current) {
        try {
          fitAddonRef.current.dispose();
        } catch (e) {
          // Ignore errors during addon disposal
        }
        fitAddonRef.current = null;
      }

      // Dispose terminal event listeners (onData, onBinary) before terminal
      // This prevents "ghost references" from closures holding terminal buffer
      if (onDataDisposableRef.current) {
        try {
          onDataDisposableRef.current.dispose();
        } catch (e) {
          // Ignore errors during disposal
        }
        onDataDisposableRef.current = null;
      }

      if (onBinaryDisposableRef.current) {
        try {
          onBinaryDisposableRef.current.dispose();
        } catch (e) {
          // Ignore errors during disposal
        }
        onBinaryDisposableRef.current = null;
      }
      
      // Finally dispose terminal
      if (terminalRef.current) {
        terminalRef.current.dispose();
        terminalRef.current = null;
      }
      
      // NOTE: DO NOT close PTY here!
      // React StrictMode double-mounts components (mount -> unmount -> mount)
      // If we close PTY on unmount, it will kill the running shell on remount.
      // PTY cleanup is handled ONLY by appStore.closeTab() when the tab is closed.
      console.debug(`[LocalTerminalView] Unmount cleanup for ${sessionId} (PTY kept alive)`);
    };
  }, [fontOpenReady, sessionId, writeEncodedTerminalInput]); // Only remount on session change — bg image is handled dynamically

  useEffect(() => {
    selectionGestureRef.current?.refresh();
  }, [terminalSettings.selectionRequiresShift]);

  // Listen for terminal data events from backend
  useEffect(() => {
    if (!fontOpenReady) return;
    if (!terminalRef.current) return;

    const dataEventName = `local-terminal-data:${sessionId}`;
    const closedEventName = `local-terminal-closed:${sessionId}`;

    // Listen for data - use RAF batching to reduce search index jumping
    // Track mounted state and listener cleanup functions
    let mounted = true;
    let unlistenDataFn: (() => void) | null = null;
    let unlistenClosedFn: (() => void) | null = null;

    // Rust PTY sends high-frequency small packets; adaptive renderer handles batching
    listen<{ sessionId: string; data: number[] }>(dataEventName, (event) => {
      if (!mounted || !isMountedRef.current || !terminalRef.current) return;
      const data = new Uint8Array(event.payload.data);
      
      maybeLoadImageAddon(data);
      maybeSuggestTerminalEncoding(data);
      const displayData = terminalOutputDecoderRef.current.transform(data).bytes;
      if (recorderRef.current) {
        // Feed recording (terminal output)
        feedOutput(displayData);
      }
      notifyTerminalOutput(sessionId);
      // Delegate batching + tier management to adaptive renderer
      adaptiveRendererRef.current.scheduleWrite(displayData);

      // Pause search updates during high-frequency output
      // Resume after 150ms of quiet, then re-run search to get accurate results
      if (currentSearchQueryRef.current) {
        searchPausedRef.current = true;
        if (outputThrottleRef.current) {
          clearTimeout(outputThrottleRef.current);
        }
        outputThrottleRef.current = setTimeout(() => {
          searchPausedRef.current = false;
          outputThrottleRef.current = null;
          // Re-trigger search to get accurate results after output settles
          if (currentSearchQueryRef.current && searchAddonRef.current) {
            searchAddonRef.current.findNext(
              currentSearchQueryRef.current,
              currentSearchOptionsRef.current
            );
          }
        }, 150);
      }
    }).then((fn) => {
      if (mounted) {
        unlistenDataFn = fn;
        updateTerminalReadiness(sessionId, {
          terminalType: 'local_terminal',
          frontendOutputListenerReady: true,
          backendBufferReady: true,
        });
      } else {
        fn(); // Component unmounted, clean up immediately
      }
    });

    // Listen for close
    listen<{ sessionId: string; exitCode: number | null }>(closedEventName, (event) => {
      if (!mounted || !isMountedRef.current || !terminalRef.current) return;
      const { exitCode } = event.payload;
      
      // Enhanced logging for debugging "秒退" issues
      console.warn(`[LocalTerminalView] Session ${sessionId} closed, exitCode: ${exitCode}`);
      
      setIsRunning(false);
      isRunningRef.current = false;
      updateTerminalState(sessionId, false);
      
      terminalRef.current.writeln('');
      if (exitCode !== null) {
        terminalRef.current.writeln(`\x1b[33m${t('terminal.local.exit_code', { code: exitCode })}\x1b[0m`);
      } else {
        terminalRef.current.writeln(`\x1b[33m${t('terminal.local.process_terminated')}\x1b[0m`);
      }
    }).then((fn) => {
      if (mounted) {
        unlistenClosedFn = fn;
      } else {
        fn(); // Component unmounted, clean up immediately
      }
    });

    return () => {
      mounted = false;
      // Adaptive renderer cleanup is handled by the hook's own useEffect.
      if (outputThrottleRef.current) {
        clearTimeout(outputThrottleRef.current);
        outputThrottleRef.current = null;
      }
      searchPausedRef.current = false;
      
      unlistenDataFn?.();
      unlistenClosedFn?.();
      updateTerminalReadiness(sessionId, {
        frontendOutputListenerReady: false,
      });
    };
  }, [feedOutput, fontOpenReady, maybeLoadImageAddon, maybeSuggestTerminalEncoding, recorderRef, sessionId, updateTerminalState]);

  // Listen for AI insert command events (only when this terminal is active)
  useEffect(() => {
    if (!isActive || !isRunning) return;

    let mounted = true;
    let unlistenFn: (() => void) | null = null;

    listen<{ command: string }>('ai-insert-command', (event) => {
      if (!mounted || !isMountedRef.current || !isRunning) return;
      
      const { command } = event.payload;
      writeEncodedTerminalInput(formatTerminalTextInput(command));
    }).then((fn) => {
      if (mounted) {
        unlistenFn = fn;
      } else {
        fn(); // Component unmounted, clean up immediately
      }
    });

    return () => {
      mounted = false;
      unlistenFn?.();
    };
  }, [isActive, isRunning, writeEncodedTerminalInput]);

  // Resize handling with 50ms debounce to reduce PTY backend pressure
  useEffect(() => {
    if (!containerRef.current || !fitAddonRef.current) return;

    let resizeDebounceTimer: ReturnType<typeof setTimeout> | null = null;

    const handleResize = () => {
      // Debounce resize to avoid excessive fits during window drag
      if (resizeDebounceTimer) {
        clearTimeout(resizeDebounceTimer);
      }
      resizeDebounceTimer = setTimeout(() => {
        const fitAddon = fitAddonRef.current;
        if (!fitAddon || !isMountedRef.current || !isTerminalContainerRenderable(containerRef.current)) {
          resizeDebounceTimer = null;
          return;
        }
        
        fitAddon.fit();

        const dims = resolveTerminalDimensions(containerRef.current, terminalRef.current, fitAddon);
        if (dims) {
          resizeTerminal(sessionId, dims.cols, dims.rows);
          // Feed recording (resize)
          feedResize(dims.cols, dims.rows);
        }
        resizeDebounceTimer = null;
      }, 50); // 50ms debounce - balances responsiveness with backend pressure
    };

    const resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(containerRef.current);

    return () => {
      if (resizeDebounceTimer) {
        clearTimeout(resizeDebounceTimer);
      }
      resizeObserver.disconnect();
    };
  }, [feedResize, fontOpenReady, resizeTerminal, sessionId]);

  // Search close handler
  const handleSearchClose = useCallback(() => {
    setSearchOpen(false);
    if (searchAddonRef.current) {
      searchAddonRef.current.clearDecorations();
      searchAddonRef.current.dispose();
      searchAddonRef.current = null;
    }
    // Clear search state and pause mechanism
    currentSearchQueryRef.current = '';
    searchPausedRef.current = false;
    if (outputThrottleRef.current) {
      clearTimeout(outputThrottleRef.current);
      outputThrottleRef.current = null;
    }
    setSearchResults({ resultIndex: -1, resultCount: 0 });
    requestAnimationFrame(() => {
      focusTerminal('strong');
    });
  }, [focusTerminal]);

  // Get cursor position for AI inline panel positioning
  const getCursorPosition = useCallback((): CursorPosition | null => {
    const term = terminalRef.current;
    const container = containerRef.current;
    if (!term || !container) return null;
    
    const buffer = term.buffer.active;
    const cursorX = buffer.cursorX;
    const cursorY = buffer.cursorY;
    const absoluteY = buffer.baseY + cursorY;
    
    const termElement = term.element;
    if (!termElement) return null;
    
    const containerRect = container.getBoundingClientRect();
    
    // Get cell dimensions from xterm.js internal API
    const core = (term as unknown as { _core?: { _renderService?: { dimensions?: { css: { cell: { width: number; height: number } } } } } })._core;
    const dimensions = core?._renderService?.dimensions;
    
    let lineHeight = 20;
    let charWidth = 9;
    
    if (dimensions?.css?.cell) {
      lineHeight = dimensions.css.cell.height;
      charWidth = dimensions.css.cell.width;
    } else {
      const fontSize = useSettingsStore.getState().settings.terminal.fontSize;
      lineHeight = Math.ceil(fontSize * 1.2);
      charWidth = Math.ceil(fontSize * 0.6);
    }
    
    return {
      x: cursorX,
      y: cursorY,
      absoluteY,
      lineHeight,
      charWidth,
      containerRect,
    };
  }, []);

  // AI Panel close handler
  const handleCloseAiPanel = useCallback(() => {
    setAiPanelOpen(false);
    setAiCursorPosition(null);
    requestAnimationFrame(() => {
      focusTerminal('strong');
    });
  }, [focusTerminal]);

  // Search handlers
  const handleSearch = useCallback((query: string, options: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }) => {
    if (!query) {
      searchAddonRef.current?.clearDecorations();
      setSearchResults({ resultIndex: -1, resultCount: 0 });
      currentSearchQueryRef.current = '';
      return;
    }
    const searchAddon = ensureSearchAddon();
    if (!searchAddon) {
      setSearchResults({ resultIndex: -1, resultCount: 0 });
      currentSearchQueryRef.current = '';
      return;
    }
    
    const searchOptions: ISearchOptions = {
      caseSensitive: options.caseSensitive,
      regex: options.regex,
      wholeWord: options.wholeWord,
      decorations: {
        matchBackground: '#5a4a00',
        matchBorder: '#997700',
        matchOverviewRuler: '#997700',
        activeMatchBackground: '#997700',
        activeMatchBorder: '#ffcc00',
        activeMatchColorOverviewRuler: '#ffcc00',
      }
    };
    
    // Store for navigation
    currentSearchQueryRef.current = query;
    currentSearchOptionsRef.current = searchOptions;
    
    searchAddon.findNext(query, searchOptions);
  }, [ensureSearchAddon]);

  const handleSearchNext = useCallback(() => {
    if (!currentSearchQueryRef.current) return;
    const searchAddon = ensureSearchAddon();
    if (!searchAddon) return;
    searchAddon.findNext(currentSearchQueryRef.current, currentSearchOptionsRef.current);
  }, [ensureSearchAddon]);

  const handleSearchPrevious = useCallback(() => {
    if (!currentSearchQueryRef.current) return;
    const searchAddon = ensureSearchAddon();
    if (!searchAddon) return;
    searchAddon.findPrevious(currentSearchQueryRef.current, currentSearchOptionsRef.current);
  }, [ensureSearchAddon]);

  // Get terminal selection for AI context
  const getTerminalSelection = useCallback((): string => {
    return terminalRef.current?.getSelection() || '';
  }, []);

  // Get visible buffer for AI context
  const getVisibleBuffer = useCallback((): string => {
    const term = terminalRef.current;
    if (!term) return '';
    
    const buffer = term.buffer.active;
    const lines: string[] = [];
    
    // Get all visible lines
    for (let i = 0; i < term.rows; i++) {
      const line = buffer.getLine(buffer.viewportY + i);
      if (line) {
        lines.push(line.translateToString(true));
      }
    }
    
    return lines.join('\n');
  }, []);

  // Handle AI insert (paste text into terminal)
  const handleAiInsert = useCallback((text: string) => {
    if (!terminalRef.current || !isRunning) return;
    writeEncodedTerminalInput(formatTerminalTextInput(text));
  }, [isRunning, writeEncodedTerminalInput]);

  // Handle AI execute (paste and send enter)
  const handleAiExecute = useCallback((command: string) => {
    if (!terminalRef.current || !isRunning) return;
    writeEncodedTerminalInput(`${formatTerminalTextInput(command)}\n`);
  }, [isRunning, writeEncodedTerminalInput]);

  // Paste protection: handle pending paste confirm
  const handlePasteConfirm = useCallback(() => {
    if (pendingPaste && isRunning) {
      writeEncodedTerminalInput(formatTerminalTextInput(pendingPaste));
    }
    setPendingPaste(null);
    requestAnimationFrame(() => {
      focusTerminal('strong');
    });
  }, [focusTerminal, pendingPaste, isRunning, writeEncodedTerminalInput]);

  const handlePasteCancel = useCallback(() => {
    setPendingPaste(null);
    requestAnimationFrame(() => {
      focusTerminal('strong');
    });
  }, [focusTerminal]);

  const processTerminalPaste = useCallback((text: string | null | undefined, allowNativePassthrough: boolean): boolean => {
    const decision = getProtectedPasteDecision(text, isRunningRef.current);

    if (decision === 'block') {
      return true;
    }

    if (!text) {
      return !allowNativePassthrough;
    }

    if (decision === 'confirm') {
      setPendingPaste(text);
      return true;
    }

    if (allowNativePassthrough) {
      return false;
    }

    if (!terminalRef.current || !isRunningRef.current) {
      return true;
    }

    writeEncodedTerminalInput(formatTerminalTextInput(text));
    return true;
  }, [writeEncodedTerminalInput]);

  const handlePasteShortcut = useCallback(() => {
    armTerminalPasteShortcutSuppression(pasteShortcutSuppressionRef);
    void readSystemClipboardText().then((text) => {
      if (text === null) {
        const fallbackText = takeTerminalPasteShortcutFallback(pasteShortcutSuppressionRef);
        if (fallbackText !== null) {
          processTerminalPaste(fallbackText, false);
        }
        return;
      }
      markTerminalPasteShortcutHandled(pasteShortcutSuppressionRef);
      processTerminalPaste(text, false);
    });
  }, [processTerminalPaste]);

  const handleHighlightRulesAutoDisabled = useCallback((ruleIds: string[], reason: 'timeout' | 'error') => {
    markRuntimeDisabledHighlightRules(
      runtimeDisabledHighlightRulesRef.current,
      useSettingsStore.getState().settings.terminal.highlightRules,
      ruleIds,
    );
    useToastStore.getState().addToast({
      title: t('settings_view.terminal.highlight_rules.toast_auto_disabled_title'),
      description: t('settings_view.terminal.highlight_rules.toast_auto_disabled_description', {
        count: ruleIds.length,
        reason: t(`settings_view.terminal.highlight_rules.runtime_reason.${reason}`),
      }),
      variant: 'warning',
    });
  }, [t]);

  // Paste protection: intercept paste events
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handlePaste = (e: ClipboardEvent) => {
      const text = e.clipboardData?.getData('text');

      if (shouldSuppressTerminalPasteEvent(pasteShortcutSuppressionRef, text)) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }

      if (!terminalSettings.pasteProtection) {
        return;
      }

      if (processTerminalPaste(text, true)) {
        e.preventDefault();
        e.stopPropagation();
      }
    };

    container.addEventListener('paste', handlePaste, { capture: true });
    return () => container.removeEventListener('paste', handlePaste, { capture: true });
  }, [processTerminalPaste, terminalSettings.pasteProtection]);

  // Use unified terminal keyboard shortcuts
  // Only handles shortcuts when this terminal is active
  useTerminalViewShortcuts(
    isActive,
    searchOpen || aiPanelOpen,
    {
      onOpenSearch: () => setSearchOpen(true),
      onCloseSearch: handleSearchClose,
      onOpenAiPanel: () => {
        const position = getCursorPosition();
        setAiCursorPosition(position);
        setAiPanelOpen(true);
      },
      onCloseAiPanel: handleCloseAiPanel,
      onToggleRecording: () => {
        if (!isSessionRecording) {
          const term = terminalRef.current;
          if (term) {
            startRecording(term.cols, term.rows);
          }
        }
      },
      onFocusTerminal: () => {
        focusTerminal('strong');
      },
      searchOpen,
      aiPanelOpen,
    }
  );

  /**
   * Handle container click - focus terminal and update active pane
   */
  const handleContainerClick = useCallback((event: React.MouseEvent) => {
    // Update active pane in Registry and notify parent
    setRegistryActivePaneId(effectivePaneId);
    touchTerminalEntry(effectivePaneId);
    onFocus?.(effectivePaneId);

    if (!searchOpen && !aiPanelOpen && shouldFocusTerminalFromClick(event.target)) {
      focusTerminal('strong');
    }
  }, [searchOpen, aiPanelOpen, effectivePaneId, focusTerminal, onFocus]);

  // ── Background Image ──────────────────────────────────────────────────────────
  const currentTheme = getTerminalTheme(terminalSettings.theme);
  const bgImageUrl = React.useMemo(
    () => {
      const masterOn = terminalSettings.backgroundEnabled !== false;
      const enabled = (terminalSettings.backgroundEnabledTabs ?? ['terminal', 'local_terminal']).includes('local_terminal');
      return masterOn && terminalSettings.backgroundImage && enabled ? convertFileSrc(terminalSettings.backgroundImage) : null;
    },
    [terminalSettings.backgroundEnabled, terminalSettings.backgroundImage, terminalSettings.backgroundEnabledTabs]
  );
  const effectiveBlur = React.useMemo(() => {
    if (!bgImageUrl) return 0;
    const raw = terminalSettings.backgroundBlur;
    return isLowEndGPU() ? Math.min(raw, 5) : raw;
  }, [bgImageUrl, terminalSettings.backgroundBlur]);

  return (
    <div 
      className="relative flex-1 w-full h-full flex flex-col overflow-hidden"
      style={{ backgroundColor: currentTheme.background }}
      onClick={handleContainerClick}
    >
      {/* Background Image Layer */}
      {bgImageUrl && (
        terminalSettings.backgroundFit === 'tile' ? (
          <div
            className="absolute inset-0 pointer-events-none"
            style={{
              zIndex: 0,
              backgroundImage: `url(${bgImageUrl})`,
              backgroundRepeat: 'repeat',
              backgroundSize: 'auto',
              opacity: terminalSettings.backgroundOpacity,
              filter: effectiveBlur > 0 ? `blur(${effectiveBlur}px)` : undefined,
              willChange: 'transform',
            }}
          />
        ) : (
          <img
            src={bgImageUrl}
            alt=""
            draggable={false}
            className="absolute inset-0 pointer-events-none select-none"
            style={{
              zIndex: 0,
              opacity: terminalSettings.backgroundOpacity,
              filter: effectiveBlur > 0 ? `blur(${effectiveBlur}px)` : undefined,
              willChange: 'transform',
              ...getBackgroundFitStyles(terminalSettings.backgroundFit),
            }}
          />
        )
      )}

      {/* Search Bar - no deep search for local terminal */}
      {searchOpen && (
        <SearchBar
          isOpen={searchOpen}
          onSearch={handleSearch}
          onFindNext={handleSearchNext}
          onFindPrevious={handleSearchPrevious}
          onClose={handleSearchClose}
          resultIndex={searchResults.resultIndex}
          resultCount={searchResults.resultCount}
          showDeepSearch={false}
        />
      )}
      
      {/* AI Inline Panel - VS Code style inline chat */}
      <AiInlinePanel
        isOpen={aiPanelOpen}
        onClose={handleCloseAiPanel}
        getSelection={getTerminalSelection}
        getVisibleBuffer={getVisibleBuffer}
        onInsert={handleAiInsert}
        onExecute={handleAiExecute}
        cursorPosition={aiCursorPosition}
        sessionId={sessionId}
        terminalType="local_terminal"
      />
      
      {/* Paste Confirmation Overlay */}
      {pendingPaste && (
        <PasteConfirmOverlay
          content={pendingPaste}
          onConfirm={handlePasteConfirm}
          onCancel={handlePasteCancel}
        />
      )}
      
      {/* Terminal Container */}
      <div 
        ref={containerRef}
        className="flex-1 w-full"
        style={{
          minHeight: 0,
          position: 'relative',
          zIndex: 1,
          contain: 'strict',
          isolation: 'isolate',
        }}
      />
      {/* Recording status overlay (shown only during active recording) */}
      {isSessionRecording && (
        <RecordingControls
          sessionId={sessionId}
          onStop={handleRecordingStop}
          onDiscard={handleRecordingDiscard}
        />
      )}

      {/* Mouse mode indicator */}
      {mouseMode && isRunning && (
        <div className="absolute bottom-2 right-2 bg-theme-bg-hover/70 text-theme-text-muted text-[11px] px-2 py-0.5 rounded pointer-events-none select-none">
          {t('terminal.mouse_mode_hint')}
        </div>
      )}

      {/* FPS / Tier overlay (enabled in Settings → Terminal → Show FPS Overlay) */}
      {terminalSettings.showFpsOverlay && (
        <FpsOverlay getStats={adaptiveRenderer.getStats} />
      )}

      {/* Status overlay when not running */}
      {!isRunning && (
        <div className="absolute bottom-4 right-4 bg-theme-bg-hover/80 text-theme-text-muted text-xs px-2 py-1 rounded">
          {t('terminal.local.session_ended')}
        </div>
      )}
      {terminalSettings.commandBar.enabled && (
        <TerminalCommandBar
          paneId={effectivePaneId}
          sessionId={sessionId}
          tabId={effectiveTabId}
          terminalType="local_terminal"
          isActive={isActive}
          sendInput={sendCommandBarInput}
          focusTerminal={() => { focusTerminal('strong'); }}
          onLayoutChange={handleCommandBarLayoutChange}
        />
      )}
    </div>
  );
};
