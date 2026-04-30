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
import { useAppStore } from '../../store/appStore';
import { useSettingsStore } from '../../store/settingsStore';
import { triggerGitRefresh } from '../../store/ideStore';
import { api, nodeTerminalUrl } from '../../lib/api';
import { runtimeEventHub } from '../../lib/runtimeEventHub';
import { getTerminalTheme } from '../../lib/themes';
import { getFontFamily } from '../../lib/fontFamily';
import { linuxBackdropBlurClass } from '../../lib/linuxWebviewProfile';
import { useTerminalViewShortcuts } from '../../hooks/useTerminalKeyboard';
import { SearchBar, DeepSearchState } from './SearchBar';
import { AiInlinePanel, type CursorPosition } from './AiInlinePanel';
import { PasteConfirmOverlay } from './PasteConfirmOverlay';
import { getProtectedPasteDecision } from '../../lib/terminalPaste';
import { terminalLinkHandler } from '../../lib/safeUrl';
import { SessionInfo } from '../../types';
import { listen } from '@tauri-apps/api/event';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Lock, Loader2, RefreshCw, AlertTriangle, ScrollText } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import i18n from '../../i18n';
import { cn } from '../../lib/utils';
import { 
  registerTerminalBuffer, 
  unregisterTerminalBuffer, 
  setActivePaneId as setRegistryActivePaneId,
  touchTerminalEntry,
  notifyTerminalOutput,
  updateTerminalReadiness,
  registerTerminalCommandMarkCreator,
} from '../../lib/terminalRegistry';
import {
  cleanupTerminalCommandMarks,
  clearTerminalCommandMarkSelection,
  closeTerminalCommandMarks,
  createTerminalCommandMark,
  getTerminalAbsoluteLineFromClientY,
  selectTerminalCommandMarkAtLine,
} from '../../lib/terminal/commandMarks';
import {
  createShellIntegrationController,
  isShellIntegrationDetected,
} from '../../lib/terminal/shellIntegration';
import { onMapleRegularLoaded, ensureCJKFallback, prepareTerminalFontForOpen } from '../../lib/fontLoader';
import { runInputPipeline, runOutputPipeline } from '../../lib/plugin/pluginTerminalHooks';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useReconnectOrchestratorStore } from '../../store/reconnectOrchestratorStore';
import { usePluginStore } from '../../store/pluginStore';
import {
  hexToRgba,
  getBackgroundFitStyles,
  detectChunkedMarker,
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
import {
  MSG_TYPE_DATA, MSG_TYPE_HEARTBEAT, MSG_TYPE_ERROR,
  HEADER_SIZE, encodeHeartbeatFrame,
} from '../../lib/wireProtocol';
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
import { ScrollbackViewer } from './ScrollbackViewer';
import { useToastStore, type ToastVariant } from '../../hooks/useToast';
import { HighlightEngine } from '../../lib/terminal/highlightEngine';
import {
  applyRuntimeDisabledHighlightRules,
  getHighlightRulesSignature,
  markRuntimeDisabledHighlightRules,
} from '../../lib/terminal/runtimeDisabledHighlightRules';
import { installShiftSelectionGuard, type SelectionGestureController } from '../../lib/terminalSelectionGesture';
import { useBroadcastStore } from '../../store/broadcastStore';
import { broadcastToTargets } from '../../lib/terminalRegistry';
import { HistorySearchMatch, TerminalHistorySearchProgress, type HighlightRule, type UnifiedFlatNode } from '../../types';
import { notifyTrzszTransferEvent } from '../../lib/terminal/trzsz/notifications';
import { TrzszController } from '../../lib/terminal/trzsz/controller';
import { createRemoteTerminalTransport, type RemoteTerminalTransport } from '../../lib/terminal/trzsz/transport';
import { formatTerminalTextInput } from '../../lib/terminalInput';
import {
  encodeTerminalInput,
  formatTerminalEncodingLabel,
  normalizeTerminalEncoding,
  TerminalEncodingMismatchDetector,
  TerminalOutputDecoder,
  type TerminalEncoding,
} from '../../lib/terminalEncoding';
import { createTerminalResizeScheduler, type TerminalResizeScheduler } from '../../lib/terminal/resizeScheduler';

const PREFILL_REPLAY_LINE_COUNT = 50; // Keep aligned with backend replay count
const TRZSZ_MAGIC_PREFIX = '::TRZSZ:TRANSFER:';

function normalizeWebSocketUrl(value: string): string {
  return value.endsWith('/') ? value.slice(0, -1) : value;
}

function historyMatchKey(match: HistorySearchMatch): string {
  return [
    match.source,
    match.chunk_id ?? 'hot',
    match.buffer_line_number ?? match.line_number,
    match.column_start,
    match.column_end,
  ].join(':');
}

function mergeHistorySearchMatches(
  current: HistorySearchMatch[],
  incoming: HistorySearchMatch[],
): HistorySearchMatch[] {
  if (incoming.length === 0) {
    return current;
  }

  const seen = new Set(current.map(historyMatchKey));
  const merged = [...current];
  for (const match of incoming) {
    const key = historyMatchKey(match);
    if (!seen.has(key)) {
      seen.add(key);
      merged.push(match);
    }
  }
  return merged;
}

interface TerminalViewProps {
  sessionId: string;
  isActive?: boolean;
  /** Unique pane ID for split pane support. If not provided, sessionId is used. */
  paneId?: string;
  /** Tab ID for registry security (prevents cross-tab context leakage) */
  tabId?: string;
  /** Callback when this pane receives focus */
  onFocus?: (paneId: string) => void;
}

export const TerminalView: React.FC<TerminalViewProps> = ({ 
  sessionId, 
  isActive = true,
  paneId,
  tabId,
  onFocus,
}) => {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const commandMarkPointerRef = useRef<{ x: number; y: number; selection: string } | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const commandBarResizeSchedulerRef = useRef<TerminalResizeScheduler | null>(null);
  const searchAddonRef = useRef<SearchAddon | null>(null);
  const imageAddonRef = useRef<ImageAddon | null>(null);
  const clipboardAddonRef = useRef<{ dispose: () => void } | null>(null);
  const selectionGestureRef = useRef<SelectionGestureController | null>(null);
  const rendererAddonRef = useRef<{ dispose: () => void } | null>(null);
  const rendererSuspendedRef = useRef(false);
  const rendererTransitionTokenRef = useRef(0);
  const webLinksAddonRef = useRef<WebLinksAddon | null>(null);
  const pasteShortcutSuppressionRef = useRef(createTerminalPasteShortcutSuppressionState());
  // xterm.js event listener disposables - must be explicitly disposed to prevent memory leaks
  const onDataDisposableRef = useRef<{ dispose: () => void } | null>(null);
  const onBinaryDisposableRef = useRef<{ dispose: () => void } | null>(null);
  const onResizeDisposableRef = useRef<{ dispose: () => void } | null>(null);
  const remoteResizeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingRemoteResizeRef = useRef<{ cols: number; rows: number } | null>(null);
  const lastRemoteResizeRef = useRef<{ cols: number; rows: number } | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const smartCopyDisposableRef = useRef<{ dispose: () => void } | null>(null);
  const trzszControllerRef = useRef<TrzszController | null>(null);
  const highlightEngineRef = useRef<HighlightEngine | null>(null);
  const runtimeDisabledHighlightRulesRef = useRef<Map<string, string>>(new Map());
  const runtimeDisabledHighlightRulesSourceRef = useRef<string | null>(null);
  const isMountedRef = useRef(true); // Track mount state for StrictMode
  const isActiveRef = useRef(isActive);
  const reconnectingRef = useRef(false); // Suppress close/error during intentional reconnect
  const manualCloseRef = useRef(false); // Suppress recovery on intentional close
  const wsRecoveryInFlightRef = useRef(false);
  const wsRecoveryAttemptsRef = useRef(0);
  const wsRecoveryTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const wsConnectAbortRef = useRef<AbortController | null>(null); // Cancel WS connect retries on unmount
  const lifecycleToastTimestampsRef = useRef<Map<string, number>>(new Map());
  const gitRefreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [aiPanelOpen, setAiPanelOpen] = useState(false);
  const [scrollbackOpen, setScrollbackOpen] = useState(false);
  const [scrollbackInitialMatch, setScrollbackInitialMatch] = useState<HistorySearchMatch | null>(null);
  const [aiCursorPosition, setAiCursorPosition] = useState<CursorPosition | null>(null);
  const prefillHistoryRef = useRef(false);
  
  // Effective pane ID: use provided paneId or fall back to sessionId
  const effectivePaneId = paneId || sessionId;
  const effectiveTabId = tabId || '';
  
  // Derive stable nodeId from sessionTreeStore (for plugin hooks etc.)
  const nodeId = useSessionTreeStore(s => s.terminalNodeMap.get(sessionId));
  
  // Mouse tracking mode indicator (tmux/vim mouse capture)
  const [mouseMode, setMouseMode] = useState(false);

  // Paste protection state
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  
  // Search state - synced with SearchAddon's onDidChangeResults
  const [searchResults, setSearchResults] = useState<{ resultIndex: number; resultCount: number }>({ 
    resultIndex: -1, 
    resultCount: 0 
  });
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

  // Track current search query for navigation
  const currentSearchQueryRef = useRef<string>('');
  const currentSearchOptionsRef = useRef<ISearchOptions | undefined>(undefined);
  const activeHistorySearchIdRef = useRef<string | null>(null);
  
  // Deep history search state
  const [deepSearchState, setDeepSearchState] = useState<DeepSearchState>({
    loading: false,
    matches: [],
    totalMatches: 0,
    durationMs: 0,
  });

  useEffect(() => {
    let mounted = true;
    let unlistenFn: (() => void) | null = null;

    listen<TerminalHistorySearchProgress>('terminal-history-search-progress', (event) => {
      if (!mounted || !isMountedRef.current) return;

      const progress = event.payload;
      if (progress.session_id !== sessionId) return;
      if (!activeHistorySearchIdRef.current || progress.search_id !== activeHistorySearchIdRef.current) {
        return;
      }

      setDeepSearchState((prev) => ({
        ...prev,
        searchId: progress.search_id,
        loading: !progress.done,
        matches: mergeHistorySearchMatches(prev.matches, progress.matches),
        totalMatches: progress.total_matches,
        durationMs: progress.duration_ms,
        searchedChunks: progress.searched_chunks,
        totalChunks: progress.total_chunks,
        truncated: progress.truncated,
        partialFailure: progress.partial_failure,
        archiveStatus: progress.archive_status,
        error: progress.error,
      }));

      if (progress.done) {
        activeHistorySearchIdRef.current = null;
      }
    }).then((fn) => {
      if (mounted) {
        unlistenFn = fn;
      } else {
        fn();
      }
    });

    return () => {
      mounted = false;
      unlistenFn?.();
      const activeSearchId = activeHistorySearchIdRef.current;
      if (activeSearchId) {
        activeHistorySearchIdRef.current = null;
        void api.cancelTerminalHistorySearch(activeSearchId);
      }
    };
  }, [sessionId]);

  useEffect(() => {
    if (!searchOpen) return;

    let cancelled = false;
    api.getTerminalHistoryStatus(sessionId)
      .then((archiveStatus) => {
        if (cancelled) return;
        setDeepSearchState((prev) => ({ ...prev, archiveStatus }));
      })
      .catch(() => {
        // Deep-search health is best-effort; the actual search path reports errors explicitly.
      });

    return () => {
      cancelled = true;
    };
  }, [searchOpen, sessionId]);
  
  // P3: Backpressure handling — delegated to useAdaptiveRenderer (adaptive FPS)
  // pendingDataRef / rafIdRef are no longer needed here.

  // IME composition state tracking (for Windows input method compatibility)
  const isComposingRef = useRef(false);

  // Get terminal settings from unified store (read early for adaptive renderer)
  const terminalSettings = useSettingsStore((state) => state.settings.terminal);
  const inBandTransferSettings = terminalSettings.inBandTransfer;
  const inBandTransferSettingsRef = useRef(inBandTransferSettings);
  inBandTransferSettingsRef.current = inBandTransferSettings;
  const [fontOpenReady, setFontOpenReady] = useState(false);

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
  const adaptiveRendererRef = useRef(adaptiveRenderer);
  adaptiveRendererRef.current = adaptiveRenderer;
  const outputTextDecoderRef = useRef<TextDecoder | null>(null);
  const terminalEncoding = normalizeTerminalEncoding(terminalSettings.terminalEncoding);
  const terminalEncodingRef = useRef<TerminalEncoding>(terminalEncoding);
  const terminalOutputDecoderRef = useRef(new TerminalOutputDecoder(terminalEncoding));
  const encodingMismatchDetectorRef = useRef(new TerminalEncodingMismatchDetector());
  const encodingHintToastShownRef = useRef(false);
  const encodedInputQueueRef = useRef<Promise<void>>(Promise.resolve());
  const hasOutputProcessors = usePluginStore((state) => state.outputProcessors.length > 0);
  const hasOutputProcessorsRef = useRef(hasOutputProcessors);

  useEffect(() => {
    terminalEncodingRef.current = terminalEncoding;
    terminalOutputDecoderRef.current.reset();
    terminalOutputDecoderRef.current = new TerminalOutputDecoder(terminalEncoding);
    encodingMismatchDetectorRef.current.reset();
    encodingHintToastShownRef.current = false;
    outputTextDecoderRef.current = null;
  }, [terminalEncoding]);

  useEffect(() => {
    hasOutputProcessorsRef.current = hasOutputProcessors;
  }, [hasOutputProcessors]);

  // Track last connected ws_url for reconnection detection
  const lastWsUrlRef = useRef<string | null>(null);

  // ── Session Recording ──────────────────────────────────────────────────
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
    terminalType: 'ssh',
    label: sessionId,
  });

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
  
  // === Standby Mode State (Input Lock during reconnection) ===
  const [inputLocked, setInputLocked] = useState(false);
  const [connectionStatus, setConnectionStatus] = useState<'connected' | 'link_down' | 'reconnecting' | 'disconnected'>('connected');
  const inputLockedRef = useRef(false); // For synchronous check in onData callback
  const connectionStatusRef = useRef<'connected' | 'link_down' | 'reconnecting' | 'disconnected'>('connected');
  const controllerRuntimePendingRef = useRef(false);
  const trzszRuntimeEpochRef = useRef(0);
  const trzszDisabledToastShownRef = useRef(false);
  const trzszDisabledDetectTailRef = useRef('');
  const inBandTransferEnabledRef = useRef(inBandTransferSettings.enabled);
  const blockedRuntimeWebSocketRef = useRef<WebSocket | null>(null);
  const transportRef = useRef<RemoteTerminalTransport | null>(null);
  if (!transportRef.current) {
    transportRef.current = createRemoteTerminalTransport({
      getWebSocket: () => wsRef.current,
      isInputLocked: () => inputLockedRef.current,
    });
  }

  const scheduleRemotePtyResize = useCallback((cols: number, rows: number) => {
    if (!Number.isFinite(cols) || !Number.isFinite(rows) || cols <= 0 || rows <= 0) return;
    pendingRemoteResizeRef.current = { cols, rows };
    if (remoteResizeTimerRef.current) {
      clearTimeout(remoteResizeTimerRef.current);
    }
    remoteResizeTimerRef.current = setTimeout(() => {
      remoteResizeTimerRef.current = null;
      const next = pendingRemoteResizeRef.current;
      if (!next) return;
      pendingRemoteResizeRef.current = null;
      const last = lastRemoteResizeRef.current;
      if (last && last.cols === next.cols && last.rows === next.rows) {
        return;
      }
      const sent = transportRef.current?.sendResize(next.cols, next.rows) ?? false;
      if (!sent) {
        // SSH terminals can be created with a deferred 0x0 PTY. Initial xterm
        // fits often happen before the WebSocket is open; do not mark those
        // dimensions as synced, or the first real WS-open resize gets deduped
        // and the remote shell stays at the wrong size until a manual window
        // resize happens.
        return;
      }
      lastRemoteResizeRef.current = next;
      feedResize(next.cols, next.rows);
      trzszControllerRef.current?.setTerminalColumns(next.cols);
    }, 100);
  }, [feedResize]);

  useEffect(() => () => {
    if (remoteResizeTimerRef.current) {
      clearTimeout(remoteResizeTimerRef.current);
      remoteResizeTimerRef.current = null;
    }
    pendingRemoteResizeRef.current = null;
  }, []);

  const ensureSearchAddon = useCallback(() => {
    const term = terminalRef.current;
    if (!term) return null;
    if (searchAddonRef.current) return searchAddonRef.current;
    const addon = new SearchAddon();
    addon.onDidChangeResults((e) => {
      if (currentSearchQueryRef.current) {
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
            storageLimit: 16,
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
          storageLimit: 16,
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
  
  // Subscribe to session changes (including ws_url updates after reconnect)
  const session = useAppStore((state) => state.sessions.get(sessionId));
  const sessionRef = useRef<SessionInfo | undefined>(session);
  const connectionIdRef = useRef<string | null>(session?.connectionId ?? null);
  const terminalSessionRehydrateRef = useRef<string | null>(null);

  const resolveNodeForTerminalSession = useCallback((targetSessionId: string): UnifiedFlatNode | undefined => {
    const treeState = useSessionTreeStore.getState();
    return treeState.getNodeByTerminalId(targetSessionId)
      ?? treeState.nodes.find((node) => (
        node.terminalSessionId === targetSessionId
        || node.runtime.terminalIds.includes(targetSessionId)
      ));
  }, []);

  const rehydrateTerminalSession = useCallback(async () => {
    if (sessionRef.current?.ws_url) return;
    if (terminalSessionRehydrateRef.current === sessionId) return;
    terminalSessionRehydrateRef.current = sessionId;

    let node = resolveNodeForTerminalSession(sessionId);
    if (!node) {
      try {
        await useSessionTreeStore.getState().fetchTree();
      } catch (error) {
        console.warn(`[TerminalView ${sessionId}] Failed to refresh session tree before terminal rehydrate:`, error);
      }
      node = resolveNodeForTerminalSession(sessionId);
    }

    if (!node) return;

    try {
      const endpoint = await nodeTerminalUrl(node.id);
      if (!isMountedRef.current) return;

      const backendSession = await api.getSession(endpoint.sessionId).catch(() => null);
      if (!isMountedRef.current) return;

      const wsUrl = backendSession?.ws_url ?? `ws://localhost:${endpoint.wsPort}`;
      const connectionId = backendSession?.connectionId ?? node.runtime.connectionId ?? node.sshConnectionId ?? undefined;
      const fallbackSession: SessionInfo = {
        id: endpoint.sessionId,
        name: node.displayName || `${node.username}@${node.host}`,
        host: node.host,
        port: node.port,
        username: node.username,
        state: 'connected',
        ws_url: wsUrl,
        ws_token: endpoint.wsToken,
        color: '#3b82f6',
        uptime_secs: 0,
        order: 0,
        connectionId,
        auth_type: 'password',
      };

      useAppStore.setState((state) => {
        const existing = state.sessions.get(endpoint.sessionId) ?? state.sessions.get(sessionId);
        const newSessions = new Map(state.sessions);
        newSessions.set(endpoint.sessionId, {
          ...fallbackSession,
          ...(existing ?? {}),
          ...(backendSession ?? {}),
          id: endpoint.sessionId,
          ws_url: wsUrl,
          ws_token: endpoint.wsToken,
          connectionId,
        });
        return { sessions: newSessions };
      });

      const treeState = useSessionTreeStore.getState();
      const currentTerminalIds = treeState.nodeTerminalMap.get(node.id) ?? [];
      if (!currentTerminalIds.includes(endpoint.sessionId) || treeState.terminalNodeMap.get(endpoint.sessionId) !== node.id) {
        useSessionTreeStore.setState((state) => {
          const nextNodeMap = new Map(state.nodeTerminalMap);
          const ids = nextNodeMap.get(node.id) ?? [];
          nextNodeMap.set(node.id, ids.includes(endpoint.sessionId) ? ids : [...ids, endpoint.sessionId]);

          const nextTerminalMap = new Map(state.terminalNodeMap);
          nextTerminalMap.set(endpoint.sessionId, node.id);
          return {
            nodeTerminalMap: nextNodeMap,
            terminalNodeMap: nextTerminalMap,
          };
        });
        useSessionTreeStore.getState().rebuildUnifiedNodes();
      }

      if (endpoint.sessionId !== sessionId) {
        useAppStore.getState().updatePaneSessionId(sessionId, endpoint.sessionId);
      }

      if (import.meta.env.DEV) {
        console.info(`[TerminalView ${sessionId}] Rehydrated terminal session from node ${node.id}`);
      }
    } catch (error) {
      if (import.meta.env.DEV) {
        console.warn(`[TerminalView ${sessionId}] Terminal session rehydrate failed:`, error);
      }
    }
  }, [resolveNodeForTerminalSession, sessionId]);

  const cleanupWebSocket = useCallback((ws: WebSocket | null, reason?: string) => {
    if (!ws) return;
    closeTerminalCommandMarks(effectivePaneId, reason === 'Reconnect' ? 'interrupted_mode' : 'session_lost', 'unknown', true);
    ws.onopen = null;
    ws.onmessage = null;
    ws.onerror = null;
    ws.onclose = null;
    try {
      ws.close(1000, reason);
    } catch {
      // Ignore close errors
    }
  }, [effectivePaneId]);

  const writeServerOutputToTerminal = useCallback((payload: Uint8Array) => {
    adaptiveRendererRef.current.scheduleWrite(payload);
  }, []);

  const notifyTerminalLifecycle = useCallback((
    key: string,
    title: string,
    variant: ToastVariant = 'default',
    description?: string,
    cooldownMs = 8_000,
  ) => {
    const now = Date.now();
    const lastShownAt = lifecycleToastTimestampsRef.current.get(key) ?? 0;
    if (now - lastShownAt < cooldownMs) return;
    lifecycleToastTimestampsRef.current.set(key, now);

    useToastStore.getState().addToast({
      title,
      description,
      variant,
      duration: variant === 'error' ? 8000 : variant === 'warning' ? 7000 : 4000,
    });
  }, []);

  const transformTerminalOutput = useCallback((payload: Uint8Array) => {
    return terminalOutputDecoderRef.current.transform(payload);
  }, []);

  const decodeTerminalBytes = useCallback((payload: Uint8Array) => {
    if (terminalEncodingRef.current !== 'utf-8') {
      return terminalOutputDecoderRef.current.decodeText(payload);
    }
    if (!outputTextDecoderRef.current) {
      outputTextDecoderRef.current = new TextDecoder();
    }
    return outputTextDecoderRef.current.decode(payload);
  }, []);

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

  const sendEncodedTerminalInput = useCallback((input: string) => {
    const send = (bytes: Uint8Array) => {
      transportRef.current?.sendEncodedPayload(bytes);
    };
    const encoded = encodeTerminalInput(input, terminalEncodingRef.current);
    if (encoded instanceof Uint8Array) {
      send(encoded);
      return;
    }

    encodedInputQueueRef.current = encodedInputQueueRef.current
      .catch(() => {})
      .then(async () => {
        send(await encoded);
      })
      .catch((error) => {
        console.error('[TerminalView] Failed to encode terminal input:', error);
      });
  }, []);

  const autosuggestRecorder = useTerminalAutosuggestRecorder({
    terminalKind: 'terminal',
    localShellHistory: terminalSettings.autosuggest?.localShellHistory ?? true,
  });
  const autosuggestRecorderRef = useRef(autosuggestRecorder);
  autosuggestRecorderRef.current = autosuggestRecorder;

  const sendCommandBarInput = useCallback((input: string) => {
    if (inputLockedRef.current) return;
    adaptiveRenderer.notifyUserInput();
    const processed = runInputPipeline(input, sessionId, nodeId);
    if (processed === null) return;
    autosuggestRecorderRef.current.observeInput(processed);
    observeCliAgentTerminalInput({
      data: processed,
      targetId: nodeId ? `ssh-node:${nodeId}` : undefined,
      sessionId,
      nodeId,
    });
    feedInput(processed);
    const controller = trzszControllerRef.current;
    if (controller && !controller.isDisposed()) {
      controller.processTerminalInput(processed);
    } else if (!controllerRuntimePendingRef.current) {
      sendEncodedTerminalInput(processed);
    }
  }, [adaptiveRenderer, feedInput, nodeId, sendEncodedTerminalInput, sessionId]);

  const focusTerminal = useCallback((mode: 'soft' | 'strong' = 'soft') => {
    const term = terminalRef.current;
    if (!term || searchOpen || aiPanelOpen || scrollbackOpen || !isTerminalContainerRenderable(containerRef.current)) {
      return false;
    }
    if (mode === 'soft' && !shouldAutoFocusTerminal(containerRef.current)) {
      return false;
    }
    term.focus();
    return true;
  }, [searchOpen, aiPanelOpen, scrollbackOpen]);

  const unlockRuntimeGateIfReady = useCallback(() => {
    if (controllerRuntimePendingRef.current || connectionStatusRef.current !== 'connected' || !inputLockedRef.current) {
      return;
    }

    inputLockedRef.current = false;
    setInputLocked(false);
    lastWsUrlRef.current = null;
    notifyTerminalLifecycle(
      'connection-restored',
      i18n.t('terminal.ssh.reconnected'),
      'success',
      undefined,
      4000,
    );
  }, [notifyTerminalLifecycle]);

  const disposeTrzszController = useCallback((options?: { notifyConnectionLost?: boolean }) => {
    const controller = trzszControllerRef.current;
    if (!controller) return;

    trzszRuntimeEpochRef.current += 1;

    if (options?.notifyConnectionLost && controller.isTransferring()) {
      notifyTrzszTransferEvent({ type: 'connection_lost' });
    }

    controller.stop();
    controller.dispose();
    trzszControllerRef.current = null;
  }, []);

  const syncTrzszController = useCallback(() => {
    const transferSettings = inBandTransferSettingsRef.current;
    const term = terminalRef.current;
    const currentSession = sessionRef.current;
    const connectionId = currentSession?.connectionId ?? null;
    const wsUrl = currentSession?.ws_url ?? null;
    const activeWs = wsRef.current;
    const activeWsUrl = activeWs?.url ? normalizeWebSocketUrl(activeWs.url) : null;

    if (
      !transferSettings.enabled
      || !term
      || !connectionId
      || !wsUrl
      || connectionStatusRef.current !== 'connected'
      || !activeWs
      || activeWs.readyState !== WebSocket.OPEN
      || (controllerRuntimePendingRef.current && blockedRuntimeWebSocketRef.current === activeWs)
      || activeWsUrl !== normalizeWebSocketUrl(wsUrl)
    ) {
      disposeTrzszController();
      return;
    }

    const currentController = trzszControllerRef.current;
    if (
      currentController
      && !currentController.isDisposed()
      && !currentController.isDraining()
      && currentController.matchesRuntime(connectionId, wsUrl)
    ) {
      currentController.updateTransferSettings(transferSettings);
      currentController.setTerminalColumns(term.cols);
      controllerRuntimePendingRef.current = false;
      blockedRuntimeWebSocketRef.current = null;
      unlockRuntimeGateIfReady();
      return;
    }

    disposeTrzszController();

    const ownerId = `trzsz:${sessionId}:${connectionId}:${crypto.randomUUID()}`;
    const runtimeEpoch = ++trzszRuntimeEpochRef.current;
    const controllerTransport = createRemoteTerminalTransport({
      getWebSocket: () => activeWs,
      isInputLocked: () => inputLockedRef.current,
      ignoreInputLock: true,
    });
    const controller = new TrzszController({
      sessionId,
      connectionId,
      wsUrl,
      ownerId,
      isRuntimeCurrent: () => trzszRuntimeEpochRef.current === runtimeEpoch,
      transport: controllerTransport,
      writeServerOutput: writeServerOutputToTerminal,
      loadCapabilities: () => api.getTrzszCapabilities(),
      cleanupOwner: async () => {
        await api.cleanupTrzszOwner(ownerId);
      },
      transferSettings,
    });
    controller.setTerminalColumns(term.cols);
    trzszControllerRef.current = controller;
    controllerRuntimePendingRef.current = false;
    blockedRuntimeWebSocketRef.current = null;
    unlockRuntimeGateIfReady();
  }, [disposeTrzszController, sessionId, unlockRuntimeGateIfReady, writeServerOutputToTerminal]);

  const syncRemotePtySize = useCallback(() => {
    const dims = resolveTerminalDimensions(containerRef.current, terminalRef.current, fitAddonRef.current);
    if (!dims) return;

    scheduleRemotePtyResize(dims.cols, dims.rows);
  }, [scheduleRemotePtyResize]);

  useEffect(() => {
    commandBarResizeSchedulerRef.current?.dispose();
    commandBarResizeSchedulerRef.current = createTerminalResizeScheduler({
      fitAddonRef,
      terminalRef,
      isRenderable: () => isTerminalContainerRenderable(containerRef.current),
      getDimensions: () => resolveTerminalDimensions(containerRef.current, terminalRef.current, fitAddonRef.current),
      onResize: ({ cols, rows }) => scheduleRemotePtyResize(cols, rows),
      resizeDebounceMs: 100,
    });
    return () => {
      commandBarResizeSchedulerRef.current?.dispose();
      commandBarResizeSchedulerRef.current = null;
    };
  }, [scheduleRemotePtyResize]);

  const handleCommandBarLayoutChange = useCallback(() => {
    commandBarResizeSchedulerRef.current?.scheduleFit();
  }, []);

  // ═══════════════════════════════════════════════════════════════════════════
  // Unified WebSocket message handler
  // Shared by both initial connection and reconnection paths.
  // Handles Wire Protocol v1 frames: DATA (0x00), HEARTBEAT (0x02), ERROR (0x03)
  // ═══════════════════════════════════════════════════════════════════════════
  const handleWsMessage = useCallback((event: MessageEvent, ws: WebSocket) => {
    if (!isMountedRef.current || wsRef.current !== ws) return;

    const raw = event.data;
    const data = raw instanceof ArrayBuffer ? new Uint8Array(raw) : new Uint8Array(raw);
    if (data.length < HEADER_SIZE) return;

    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    const msgType = view.getUint8(0);
    const length = view.getUint32(1, false);

    if (data.length < HEADER_SIZE + length) return;

    switch (msgType) {
      case MSG_TYPE_DATA: {
        // CRITICAL: Use slice() to create a copy, not a view!
        // Views keep the entire original ArrayBuffer alive until GC
        let payloadCopy: Uint8Array = data.slice(HEADER_SIZE, HEADER_SIZE + length);
        const controller = trzszControllerRef.current;
        const controllerRuntimePending = controllerRuntimePendingRef.current;
        const activeController = controller && !controller.isDisposed() ? controller : null;
        const shouldCheckTrzszDisabled = !activeController
          && !controllerRuntimePending
          && !inBandTransferEnabledRef.current
          && !trzszDisabledToastShownRef.current;
        const shouldRunOutputPipeline = hasOutputProcessorsRef.current;
        const shouldTryImageAddon = !imageAddonRef.current && Boolean(terminalRef.current);
        const shouldRecordOutput = recorderRef.current !== null;

        if (
          !shouldRunOutputPipeline
          && !shouldTryImageAddon
          && !shouldRecordOutput
          && !shouldCheckTrzszDisabled
          && !controllerRuntimePending
          && !activeController
          && terminalEncodingRef.current === 'utf-8'
        ) {
          maybeSuggestTerminalEncoding(payloadCopy);
          writeServerOutputToTerminal(payloadCopy);
          break;
        }

        if (shouldRunOutputPipeline) {
          payloadCopy = runOutputPipeline(payloadCopy, sessionId, nodeId);
        }

        if (shouldTryImageAddon) {
          maybeLoadImageAddon(payloadCopy);
        }

        if (shouldRecordOutput) {
          feedOutput(payloadCopy);
        }

        if (activeController) {
          activeController.processServerOutput(payloadCopy);
        } else if (!controllerRuntimePending) {
          maybeSuggestTerminalEncoding(payloadCopy);
          const transformed = transformTerminalOutput(payloadCopy);
          if (shouldCheckTrzszDisabled) {
            const text = transformed.text ?? decodeTerminalBytes(payloadCopy);
            const detection = detectChunkedMarker(
              trzszDisabledDetectTailRef.current,
              text,
              TRZSZ_MAGIC_PREFIX,
            );
            if (detection.matched) {
              trzszDisabledToastShownRef.current = true;
              trzszDisabledDetectTailRef.current = detection.tail;
              useToastStore.getState().addToast({
                title: i18n.t('terminal.trzsz.disabled_title'),
                description: i18n.t('terminal.trzsz.disabled_description'),
                variant: 'warning',
              });
            } else {
              trzszDisabledDetectTailRef.current = detection.tail;
            }
          }
          writeServerOutputToTerminal(transformed.bytes);
        }
        break;
      }
      case MSG_TYPE_HEARTBEAT:
        if (length >= 4) {
          const seq = view.getUint32(HEADER_SIZE, false);
          ws.send(encodeHeartbeatFrame(seq));
        }
        break;
      case MSG_TYPE_ERROR: {
        const errorPayload = data.slice(HEADER_SIZE, HEADER_SIZE + length);
        const errorMsg = decodeTerminalBytes(errorPayload);
        terminalRef.current?.writeln(`\r\n\x1b[31m${i18n.t('terminal.ssh.server_error', { error: errorMsg })}\x1b[0m`);
        break;
      }
    }
  }, [decodeTerminalBytes, feedOutput, maybeLoadImageAddon, maybeSuggestTerminalEncoding, nodeId, recorderRef, sessionId, transformTerminalOutput, writeServerOutputToTerminal]);

  // Keep a stable ref to handleWsMessage so WebSocket onmessage handlers
  // (bound once in the init effect) always call the latest version.
  // This ensures settings changes (e.g. adaptive renderer mode) take effect
  // without requiring a WebSocket reconnect.
  const handleWsMessageRef = useRef(handleWsMessage);
  handleWsMessageRef.current = handleWsMessage;

  useEffect(() => {
    sessionRef.current = session;
    if (session?.ws_url) {
      terminalSessionRehydrateRef.current = null;
    }
  }, [session]);

  useEffect(() => {
    if (session?.ws_url) return;
    void rehydrateTerminalSession();
  }, [rehydrateTerminalSession, session?.ws_url]);

  useEffect(() => {
    connectionIdRef.current = session?.connectionId ?? null;
  }, [session?.connectionId]);

  useEffect(() => {
    connectionStatusRef.current = connectionStatus;
  }, [connectionStatus]);

  useEffect(() => {
    inBandTransferEnabledRef.current = inBandTransferSettings.enabled;
    if (!inBandTransferSettings.enabled) {
      disposeTrzszController();
      return;
    }

    trzszDisabledToastShownRef.current = false;
    trzszDisabledDetectTailRef.current = '';
    trzszControllerRef.current?.updateTransferSettings(inBandTransferSettings);
    syncTrzszController();
  }, [
    connectionStatus,
    disposeTrzszController,
    inBandTransferSettings,
    session?.connectionId,
    session?.ws_url,
    syncTrzszController,
  ]);

  const recoverWebSocket = useCallback((reason: string) => {
    if (wsRecoveryInFlightRef.current) return;
    if (wsRecoveryAttemptsRef.current >= 15) {
      // All recovery attempts exhausted. The disconnected overlay renders this
      // state; do not write OxideTerm lifecycle text into the user's PTY buffer.
      return;
    }
    if (connectionStatusRef.current !== 'connected') return;

    wsRecoveryInFlightRef.current = true;
    wsRecoveryAttemptsRef.current += 1;
    const attempt = wsRecoveryAttemptsRef.current;

    // Fast retry for initial connection failures (backend may not be ready yet)
    // Slower retry for mid-session failures (need to recreate PTY)
    const isInitialFailure = reason.startsWith('initial-') && !reason.includes('opened');
    const delayMs = isInitialFailure
      ? Math.min(200 * attempt, 1000)  // 200ms, 400ms, ..., 1000ms
      : Math.min(1000 * attempt, 15000); // 1s, 2s, ..., 15s cap

    if (import.meta.env.DEV) {
      console.warn(`[TerminalView ${sessionId}] WS recover attempt #${attempt} (${reason}) in ${delayMs}ms`);
    }

    if (wsRecoveryTimeoutRef.current) {
      clearTimeout(wsRecoveryTimeoutRef.current);
    }

    wsRecoveryTimeoutRef.current = setTimeout(async () => {
      // 🔴 Early exit if component unmounted
      if (!isMountedRef.current) {
        wsRecoveryInFlightRef.current = false;
        return;
      }
      
      try {
        // For initial failures, first 3 attempts just wait and let the reconnect effect retry
        // (the backend WS bridge should still be accepting connections)
        if (isInitialFailure && attempt <= 3) {
          lastWsUrlRef.current = null; // Force reconnect effect to re-attempt
          wsRecoveryInFlightRef.current = false;
          return;
        }
        
        // 🔴 Check again before making API call
        if (!isMountedRef.current) {
          wsRecoveryInFlightRef.current = false;
          return;
        }
        
        // Full recovery: recreate PTY and get new WS URL
        const result = await api.recreateTerminalPty(sessionId);
        
        // 🔴 Check after API call - component might have unmounted during the await
        if (!isMountedRef.current) {
          wsRecoveryInFlightRef.current = false;
          return;
        }
        
        useAppStore.setState((state) => {
          const newSessions = new Map(state.sessions);
          const existingSession = newSessions.get(sessionId);
          if (existingSession) {
            newSessions.set(sessionId, {
              ...existingSession,
              ws_url: result.wsUrl,
              ws_token: result.wsToken,
            });
          }
          return { sessions: newSessions };
        });
        lastWsUrlRef.current = null; // Allow reconnect even if URL repeats
        wsRecoveryAttemptsRef.current = 0; // Reset on success
      } catch (error) {
        const errorMsg = String(error);

        // Connection-level failure: silently purge
        if (errorMsg.includes('Connection not found') || errorMsg.includes('Session') && errorMsg.includes('not found')) {
          wsRecoveryAttemptsRef.current = 15; // Prevent further retries
          useAppStore.getState().purgeTerminalSession(sessionId);
          return;
        }

        console.error(`[TerminalView ${sessionId}] WS recover failed:`, error);
      } finally {
        wsRecoveryInFlightRef.current = false;
      }
    }, delayMs);
  }, [sessionId]);

  useEffect(() => {
    return () => {
      if (wsRecoveryTimeoutRef.current) {
        clearTimeout(wsRecoveryTimeoutRef.current);
      }
    };
  }, []);

  // Reset WS recovery attempts when network comes back online
  useEffect(() => {
    const handleOnline = () => {
      const ws = wsRef.current;
      const wsBroken = !ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING;
      if (wsBroken && wsRecoveryAttemptsRef.current > 0 && connectionStatusRef.current === 'connected') {
        console.log(`[TerminalView ${sessionId}] Network restored, resetting WS recovery`);
        wsRecoveryAttemptsRef.current = 0;
        recoverWebSocket('network-restored');
      }
    };
    window.addEventListener('online', handleOnline);
    return () => window.removeEventListener('online', handleOnline);
  }, [sessionId, recoverWebSocket]);

  // Recover broken WebSocket when page becomes visible again (App Nap / sleep wake)
  // The local WsBridge may have timed out while JS was paused, even though
  // the SSH connection is still alive (keep-alive). Detect this and reconnect.
  useEffect(() => {
    let lastHiddenAt = 0;

    const handleVisibilityChange = () => {
      if (document.hidden) {
        lastHiddenAt = Date.now();
        return;
      }

      // Page became visible — check if we were hidden long enough for WsBridge to time out
      const hiddenDuration = lastHiddenAt > 0 ? Date.now() - lastHiddenAt : 0;
      if (hiddenDuration < 5_000) return; // Ignore brief tab switches

      const ws = wsRef.current;
      const wsBroken = !ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING;

      if (wsBroken && connectionStatusRef.current === 'connected') {
        console.log(
          `[TerminalView ${sessionId}] Page visible after ${Math.round(hiddenDuration / 1000)}s, WS is broken — recovering`
        );
        wsRecoveryAttemptsRef.current = 0; // Reset attempts for fresh recovery
        recoverWebSocket('wake-ws-broken');
      }
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => document.removeEventListener('visibilitychange', handleVisibilityChange);
  }, [sessionId, recoverWebSocket]);

  // terminalSettings already read above (before adaptive renderer hook)

  // === Listen for connection status changes (Standby mode trigger) ===
  useEffect(() => {
    const unsubscribe = runtimeEventHub.subscribe('connectionStatusChanged', (payload) => {
      const { connection_id, status } = payload;
      
      const currentConnectionId = connectionIdRef.current;
      if (!currentConnectionId) return;
      // Only handle events for our connection
      if (connection_id !== currentConnectionId) return;
      
      console.log(`[TerminalView ${sessionId}] Connection status: ${status}`);
      const previousStatus = connectionStatusRef.current;
      setConnectionStatus(status);
      connectionStatusRef.current = status;

      if (status !== previousStatus) {
        if (status === 'link_down') {
          notifyTerminalLifecycle(
            'connection-lost',
            i18n.t('terminal.standby.connection_lost'),
            'warning',
            i18n.t('terminal.standby.input_locked'),
          );
        } else if (status === 'reconnecting') {
          notifyTerminalLifecycle(
            'connection-reconnecting',
            i18n.t('terminal.standby.reconnecting'),
            'warning',
            i18n.t('terminal.standby.input_locked'),
          );
        } else if (status === 'disconnected') {
          notifyTerminalLifecycle(
            'connection-disconnected',
            i18n.t('terminal.disconnected.message'),
            'error',
          );
        } else if (status === 'connected' && previousStatus !== 'connected') {
          notifyTerminalLifecycle(
            'connection-restored',
            i18n.t('terminal.ssh.reconnected'),
            'success',
            undefined,
            4000,
          );
        }
      }
      
      const shouldLock = status === 'link_down' || status === 'reconnecting';
      const shouldHoldRuntimeGate = shouldLock || status === 'disconnected';

      if (shouldHoldRuntimeGate) {
        controllerRuntimePendingRef.current = true;
        blockedRuntimeWebSocketRef.current = wsRef.current;
      }
      
      if (shouldHoldRuntimeGate && !inputLockedRef.current) {
        // Entering Standby mode
        inputLockedRef.current = true;
        setInputLocked(true);
        closeTerminalCommandMarks(effectivePaneId, 'interrupted_mode', 'unknown', true);
      }

      if (shouldHoldRuntimeGate) {
        disposeTrzszController({ notifyConnectionLost: true });
      }

      if (status === 'disconnected') {
        // Stop any reconnection attempts and close websocket
        wsRecoveryAttemptsRef.current = 3;
        wsRecoveryInFlightRef.current = false;
        const ws = wsRef.current;
        wsRef.current = null;
        manualCloseRef.current = true;
        cleanupWebSocket(ws, 'Disconnected');
        lastWsUrlRef.current = null;
      }
    });

    return () => {
      unsubscribe();
    };
  }, [notifyTerminalLifecycle, sessionId]);

  // Subscribe to terminal settings changes from settingsStore
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
        term.options.scrollback = terminal.scrollback;
        
        // Apply theme update — must use transparent background when bg image is set
        const enabledTabs = terminal.backgroundEnabledTabs ?? ['terminal', 'local_terminal'];
        const hasBg = terminal.backgroundEnabled !== false && !!terminal.backgroundImage && enabledTabs.includes('terminal');
        const themeConfig = getTerminalTheme(terminal.theme);
        term.options.theme = hasBg
          ? { ...themeConfig, background: hexToRgba(themeConfig.background || '#09090b', 0.01), overviewRulerBorder: 'transparent', scrollbarSliderBackground: 'rgba(255,255,255,0.15)', scrollbarSliderHoverBackground: 'rgba(255,255,255,0.3)', scrollbarSliderActiveBackground: 'rgba(255,255,255,0.4)' }
          : { ...themeConfig, overviewRulerBorder: 'transparent', scrollbarSliderBackground: 'rgba(255,255,255,0.15)', scrollbarSliderHoverBackground: 'rgba(255,255,255,0.3)', scrollbarSliderActiveBackground: 'rgba(255,255,255,0.4)' };

        // Sync DOM-level transparency with background image state
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
          syncRemotePtySize();
        });
      }
    );
    return unsubscribe;
  }, [syncRemotePtySize]);

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
        console.log(`[TerminalView] CJK font loaded, synced resize: ${dims.cols}x${dims.rows}`);
      }

      syncRemotePtySize();
    });
    
    return unsubscribe;
  }, [syncRemotePtySize]);

  // Focus terminal when it becomes active (tab switch)
  useEffect(() => {
    if (isActive && terminalRef.current && !searchOpen && !aiPanelOpen && !scrollbackOpen) {
      // Small delay to ensure DOM is ready
      const focusTimeout = setTimeout(() => {
        focusTerminal('soft');
        if (!isTerminalContainerRenderable(containerRef.current)) return;
        fitAddonRef.current?.fit();
        syncRemotePtySize();
      }, 50);
      return () => clearTimeout(focusTimeout);
    }
  }, [focusTerminal, isActive, searchOpen, aiPanelOpen, scrollbackOpen, syncRemotePtySize]);

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
        cleanupTerminalCommandMarks(effectivePaneId);
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
            `[Renderer] WebGL renderer "${rendererInfo.renderer ?? 'unknown'}" appears software-backed, using Canvas in auto mode`,
          );
          rendererAddonRef.current = await loadCanvasAddon();
          if (rendererAddonRef.current) {
            console.log('[Renderer] Canvas addon loaded (auto software fallback)');
          }
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

      // After renderer restore, xterm re-renders viewport from theme —
      // re-force transparent if background image is active.
      if (terminalSettings.backgroundEnabled !== false
        && terminalSettings.backgroundImage
        && (terminalSettings.backgroundEnabledTabs ?? ['terminal', 'local_terminal']).includes('terminal')) {
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
            syncRemotePtySize();
          }
        });
      });
    };

    void restoreRenderer();

    return () => {
      if (fitRaf1 !== null) cancelAnimationFrame(fitRaf1);
      if (fitRaf2 !== null) cancelAnimationFrame(fitRaf2);
    };
  }, [isActive, terminalSettings.cursorBlink, terminalSettings.renderer, syncRemotePtySize]);

  // WebSocket reconnection effect - triggers when ws_url changes (after auto-reconnect)
  useEffect(() => {
    const currentSession = sessionRef.current;
    const wsUrl = currentSession?.ws_url;
    // Skip if terminal not initialized or no ws_url
    if (!terminalRef.current || !wsUrl) return;
    if (connectionStatusRef.current !== 'connected') return;
    const normalizedWsUrl = normalizeWebSocketUrl(wsUrl);
    const existingWs = wsRef.current;
    const existingWsUrl = existingWs?.url
      ? normalizeWebSocketUrl(existingWs.url)
      : (lastWsUrlRef.current ? normalizeWebSocketUrl(lastWsUrlRef.current) : null);
    
    // Skip if this is the same URL we're already connected to
    if (normalizedWsUrl === existingWsUrl) {
      if (existingWs && existingWs.readyState <= WebSocket.OPEN) {
        return;
      }
      // If ws exists but is closed, allow reconnect to same URL
    }
    
    // Skip if WebSocket is already open/connecting to same URL
    if (existingWs && existingWs.readyState <= WebSocket.OPEN) {
      // If old connection exists but URL changed, close it
      if (existingWsUrl !== null && normalizedWsUrl !== existingWsUrl) {
        console.log('[Terminal] Session reconnected, closing old WebSocket and reconnecting...');
        updateTerminalReadiness(sessionId, {
          terminalType: 'terminal',
          frontendOutputListenerReady: false,
        });
        reconnectingRef.current = true;
        controllerRuntimePendingRef.current = true;
        blockedRuntimeWebSocketRef.current = existingWs;
        disposeTrzszController({ notifyConnectionLost: true });
        wsRef.current = null;
        manualCloseRef.current = true;
        cleanupWebSocket(existingWs, 'Reconnect');
      } else {
        return; // Same URL, already connected
      }
    }
    
    const wsToken = currentSession?.ws_token;
    try {
      const ws = new WebSocket(wsUrl);
      let opened = false;
      ws.binaryType = 'arraybuffer';
      wsRef.current = ws;
      lastWsUrlRef.current = wsUrl;

      ws.onopen = () => {
        if (!isMountedRef.current) {
          ws.close();
          return;
        }
        reconnectingRef.current = false;
        opened = true;
        wsRecoveryAttemptsRef.current = 0;
        wsRecoveryInFlightRef.current = false;
        
        // Send authentication token
        if (wsToken) {
          ws.send(wsToken);
        }
        notifyTerminalLifecycle(
          'ws-reconnected',
          i18n.t('terminal.ssh.reconnected'),
          'success',
          undefined,
          4000,
        );
        
        // Re-send dimensions for the newly opened socket even when the
        // frontend size matches the last terminal instance. A reconnect may
        // allocate a fresh PTY that has not seen any resize frame yet.
        lastRemoteResizeRef.current = null;
        syncRemotePtySize();
        syncTrzszController();
        updateTerminalReadiness(sessionId, {
          terminalType: 'terminal',
          frontendOutputListenerReady: true,
          backendBufferReady: true,
        });
      };

      ws.onmessage = (e) => handleWsMessageRef.current(e, ws);

      ws.onerror = (error) => {
        if (!isMountedRef.current || wsRef.current !== ws) return;
        console.error('WebSocket reconnection error:', error);
        notifyTerminalLifecycle(
          'ws-reconnect-error',
          i18n.t('terminal.ssh.ws_reconnect_error'),
          'warning',
        );
        updateTerminalReadiness(sessionId, {
          terminalType: 'terminal',
          frontendOutputListenerReady: false,
        });
        if (!reconnectingRef.current) {
          recoverWebSocket(opened ? 'reconnect-error-opened' : 'reconnect-error');
        }
      };

      ws.onclose = (event) => {
        if (!isMountedRef.current || wsRef.current !== ws) return;
        if (manualCloseRef.current) {
          manualCloseRef.current = false;
          return;
        }
        updateTerminalReadiness(sessionId, {
          terminalType: 'terminal',
          frontendOutputListenerReady: false,
        });
        console.log('WebSocket closed after reconnect:', event.code, event.reason);
        if (event.code !== 1000) {
          notifyTerminalLifecycle(
            `ws-closed-code-${event.code}`,
            i18n.t('terminal.ssh.connection_closed_code', { code: event.code }),
            'warning',
          );
        }
        if (!reconnectingRef.current) {
          recoverWebSocket(opened ? 'reconnect-close-opened' : 'reconnect-close');
        }
      };
    } catch (e) {
      console.error('Failed to reconnect WebSocket:', e);
      notifyTerminalLifecycle(
        'ws-establish-failed',
        i18n.t('terminal.ssh.ws_establish_failed', { error: e }),
        'error',
      );
    }
  }, [session?.ws_url, recoverWebSocket, cleanupWebSocket, connectionStatus, syncRemotePtySize, disposeTrzszController, syncTrzszController, notifyTerminalLifecycle]);

  // Font family resolver — see src/lib/fontFamily.ts

  useEffect(() => {
    if (!fontOpenReady) return;
    if (!containerRef.current || terminalRef.current) return;
    
    isMountedRef.current = true; // Reset mount state
    let clipboardInitCancelled = false;

    // Initialize xterm.js
    // Build theme — if background image is set, make xterm background near-
    // transparent so the GPU-composited background layer shows through.
    // IMPORTANT: xterm.js only parses #hex and rgba() — the CSS keyword
    // 'transparent' is NOT recognised and silently falls back to opaque black,
    // which is why we must use rgba() with an explicit near-zero alpha.
    // Alpha 0.01 (= 3/255 internally) avoids WebGL premultiplied-alpha
    // rendering artefacts that occur with exact-zero alpha on some GPUs.
    const hasBgImage = terminalSettings.backgroundEnabled !== false
      && !!terminalSettings.backgroundImage
      && (terminalSettings.backgroundEnabledTabs ?? ['terminal', 'local_terminal']).includes('terminal');
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
      scrollback: terminalSettings.scrollback,
      allowProposedApi: true,
      fastScrollSensitivity: 5,
      drawBoldTextInBrightColors: true,
      // Controls the custom SmoothScrollableElement scrollbar width in xterm 6.0.
      overviewRuler: { width: 10 },
      // Always enable transparency so we can toggle background images at
      // runtime without remounting (and destroying) the terminal instance.
      // The performance cost of allowTransparency is negligible on modern GPUs.
      allowTransparency: true,
    });

    const fitAddon = new FitAddon();
    // WebLinksAddon with secure URL handler - blocks dangerous protocols (file://, javascript:, etc.)
    const webLinksAddon = new WebLinksAddon(terminalLinkHandler);
    
    term.loadAddon(fitAddon);
    term.loadAddon(webLinksAddon);
    // SearchAddon and ImageAddon are loaded lazily to reduce memory usage
    
    // Unicode11Addon for proper Nerd Font icons and CJK wide character rendering
    // Required for Oh My Posh, Starship, and other modern prompts
    const unicode11Addon = new Unicode11Addon();
    term.loadAddon(unicode11Addon);
    term.unicode.activeVersion = '11';
    
    webLinksAddonRef.current = webLinksAddon;

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
      return false; // Let xterm handle default processing
    });

    const shellIntegrationController = createShellIntegrationController({
      term,
      paneId: effectivePaneId,
      sessionId,
      nodeId,
    });
    const osc133Disposable = term.parser.registerOscHandler(133, shellIntegrationController.handleOsc133);
    const osc633Disposable = term.parser.registerOscHandler(633, shellIntegrationController.handleOsc633);

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
        if (active) {
          closeTerminalCommandMarks(effectivePaneId, 'interrupted_mode', 'unknown', true);
        }
      }
      notifyTerminalOutput(sessionId);
    });

    // Load renderer based on settings
    // renderer: 'auto' | 'webgl' | 'canvas'
    const loadRenderer = async () => {
        const rendererSetting = terminalSettings.renderer || 'auto';
        
        // Helper to load CanvasAddon dynamically (beta version has package.json issues)
        const loadCanvasAddon = async (): Promise<{ dispose: () => void } | null> => {
            try {
                // Dynamic import with explicit path to work around beta package.json bug
                const { CanvasAddon } = await import('@xterm/addon-canvas/lib/xterm-addon-canvas.mjs');
                const canvasAddon = new CanvasAddon();
                term.loadAddon(canvasAddon);
                return canvasAddon;
            } catch (e) {
                console.warn('[Renderer] Canvas addon dynamic import failed', e);
                return null;
            }
        };
        
        if (rendererSetting === 'canvas') {
            // Force Canvas renderer
            const addon = await loadCanvasAddon();
            if (addon) {
                rendererAddonRef.current = addon;
                console.log('[Renderer] Canvas addon loaded (user preference)');
            } else {
                console.warn('[Renderer] Canvas addon failed, using DOM fallback');
            }
        } else if (rendererSetting === 'webgl') {
            // Force WebGL renderer
            try {
                const dpr = Math.ceil(window.devicePixelRatio || 1);
                logWebglRendererInfo('[Renderer:webgl]', getWebglRendererInfo());
                const webglAddon = new WebglAddon();
                webglAddon.onContextLoss(() => {
                    console.warn('[Renderer] WebGL context lost, disposing');
                    webglAddon.dispose();
                    rendererAddonRef.current = null;
                });
                term.loadAddon(webglAddon);
                rendererAddonRef.current = webglAddon;
                console.log(`[Renderer] WebGL addon loaded with DPR: ${dpr}`);
            } catch (e) {
                console.warn('[Renderer] WebGL addon failed, using DOM fallback', e);
            }
        } else {
            // Auto: Try WebGL first, fallback to Canvas
            const rendererInfo = getWebglRendererInfo();
            logWebglRendererInfo('[Renderer:auto]', rendererInfo);
            if (rendererInfo?.isSoftwareRenderer) {
                console.warn(
                    `[Renderer] WebGL renderer "${rendererInfo.renderer ?? 'unknown'}" appears software-backed, using Canvas in auto mode`,
                );
                const canvasAddon = await loadCanvasAddon();
                rendererAddonRef.current = canvasAddon;
                if (canvasAddon) {
                    console.log('[Renderer] Auto mode selected Canvas (software WebGL renderer detected)');
                } else {
                    console.warn('[Renderer] Canvas fallback failed, using DOM');
                }
                return;
            }

            try {
                const dpr = Math.ceil(window.devicePixelRatio || 1);
                const webglAddon = new WebglAddon();
                webglAddon.onContextLoss(async () => {
                    console.warn('[Renderer] WebGL context lost, switching to Canvas');
                    webglAddon.dispose();
                    // Try Canvas fallback on context loss
                    const canvasAddon = await loadCanvasAddon();
                    rendererAddonRef.current = canvasAddon;
                    if (canvasAddon) {
                        console.log('[Renderer] Canvas addon loaded as fallback');
                    }
                });
                term.loadAddon(webglAddon);
                rendererAddonRef.current = webglAddon;
                console.log(`[Renderer] Auto mode selected WebGL with DPR: ${dpr}`);
            } catch (e) {
                console.warn('[Renderer] WebGL addon failed, trying Canvas fallback', e);
                // Fallback to Canvas
                const canvasAddon = await loadCanvasAddon();
                rendererAddonRef.current = canvasAddon;
                if (canvasAddon) {
                    console.log('[Renderer] Auto mode selected Canvas after WebGL load failure');
                } else {
                    console.warn('[Renderer] Canvas fallback failed, using DOM');
                }
            }
        }
    };
    
    loadRenderer();

    term.open(containerRef.current);
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

    // Force xterm internal DOM elements transparent when bg image is set.
    if (hasBgImage) {
      forceViewportTransparent(containerRef.current);
    }

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    if (isTerminalContainerRenderable(containerRef.current)) {
      fitAddon.fit();
    }
    if (isActiveRef.current) {
      focusTerminal('soft');
    }

    syncTrzszController();
    
    const prefillHistory = async (): Promise<boolean> => {
      if (prefillHistoryRef.current) return false;
      prefillHistoryRef.current = true;
      try {
        const stats = await api.getBufferStats(sessionId);
        const desired = Math.min(terminalSettings.scrollback, stats.current_lines);
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
        terminalRef.current.writeln(`\x1b[38;2;234;88;12m${i18n.t('terminal.ssh.initialized')}\x1b[0m`);
      }
    });
    
    // ══════════════════════════════════════════════════════════════════════════
    // Register terminal buffer to unified Terminal Registry
    // This enables AI context retrieval for both SSH and Local terminals
    // ══════════════════════════════════════════════════════════════════════════
    const getBufferContent = (): string => {
      const t = terminalRef.current;
      if (!t) return '';
      
      const buffer = t.buffer.active;
      const lines: string[] = [];
      const lineCount = buffer.length;
      
      // Only read the last 500 lines to avoid copying huge scrollback buffers
      const maxLines = 500;
      const startLine = Math.max(0, lineCount - maxLines);
      for (let i = startLine; i < lineCount; i++) {
        const line = buffer.getLine(i);
        if (line) {
          lines.push(line.translateToString(true));
        }
      }
      
      return lines.join('\n');
    };
    
    // Selection getter for AI sidebar context
    const getSelectionContent = (): string => {
      return terminalRef.current?.getSelection() || '';
    };

    // Screen reader for TUI interaction (experimental)
    const getScreenSnapshot = (): import('@/types').ScreenSnapshot | null => {
      const t = terminalRef.current;
      if (!t) return null;

      const buffer = t.buffer.active;
      const rows = t.rows;
      const cols = t.cols;
      const lines: string[] = [];

      // Read only the visible viewport rows
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
        isAlternateBuffer: t.buffer.active.type === 'alternate',
      };
    };
    
    // Register with paneId as key, not sessionId
    registerTerminalBuffer(
      effectivePaneId,
      effectiveTabId,
      sessionId,
      'terminal', // SSH terminal type
      getBufferContent,
      getSelectionContent,  // Include selection getter
      // Writer function: encode and send via WebSocket
      (data: string) => {
        if (inputLockedRef.current) return; // Respect standby mode
        const controller = trzszControllerRef.current;
        if (controller && !controller.isDisposed()) {
          controller.processTerminalInput(data);
        } else if (!controllerRuntimePendingRef.current) {
          sendEncodedTerminalInput(data);
        }
      },
      getScreenSnapshot,  // Screen reader for TUI interaction
    );
    registerTerminalCommandMarkCreator(effectivePaneId, (request) => {
      const current = terminalRef.current;
      if (!current || inputLockedRef.current || controllerRuntimePendingRef.current) return;
      createTerminalCommandMark(current, effectivePaneId, {
        ...request,
        nodeId,
        cwd: request.cwd ?? undefined,
      });
    });
    
    // Font loading detection - ensure fonts are loaded before connecting
    const ensureFontsLoaded = async () => {
        try {
            const fontsToCheck = ['JetBrains Mono', 'MesloLGM Nerd Font'];
            for (const fontName of fontsToCheck) {
                await document.fonts.load(`16px "${fontName}"`);
                if (import.meta.env.DEV) {
                    console.log(`[Font] ${fontName} loaded`);
                }
            }
            if (import.meta.env.DEV) {
                console.log('[Font] All fonts loaded, ready to connect');
            }
        } catch (error) {
            console.warn('[Font] Failed to load fonts:', error);
            // Continue anyway - fonts may load later
        }
    };

    // Delay WebSocket connection to avoid React StrictMode double-mount issue
    let wsConnectTimeout: ReturnType<typeof setTimeout> | null = null;

    if (session?.ws_url) {
      const wsUrl = session.ws_url; // Capture to avoid undefined in closure
        term.writeln(i18n.t('terminal.ssh.connecting', { user: session.username, host: session.host }));

        // Create AbortController for cancelling WS connect retries
        const abortController = new AbortController();
        wsConnectAbortRef.current = abortController;

        // Helper function to attempt WS connection with retries
        const attemptWsConnect = async (attempt: number, maxAttempts: number): Promise<void> => {
            // Check both mount state and abort signal
            if (!isMountedRef.current || abortController.signal.aborted) return;
            
            // Avoid stale ws_url from reconnect race
            const latestSession = useAppStore.getState().sessions.get(sessionId);
            if (!latestSession?.ws_url || latestSession.ws_url !== wsUrl) {
              return;
            }
            if (wsRef.current && wsRef.current.readyState <= WebSocket.OPEN) {
              return;
            }

            return new Promise<void>((resolve) => {
              // Check abort before creating WebSocket
              if (abortController.signal.aborted) {
                resolve();
                return;
              }

              const ws = new WebSocket(wsUrl);
              let opened = false;
              updateTerminalReadiness(sessionId, {
                terminalType: 'terminal',
                frontendOutputListenerReady: false,
              });
              ws.binaryType = 'arraybuffer';
              wsRef.current = ws;
              lastWsUrlRef.current = wsUrl;

              ws.onopen = () => {
                  if (!isMountedRef.current) {
                      ws.close();
                      resolve();
                      return;
                  }
                  reconnectingRef.current = false;
                  opened = true;
                  wsRecoveryAttemptsRef.current = 0;
                  wsRecoveryInFlightRef.current = false;

                  // SECURITY: Send authentication token as first message
                  const latestToken = latestSession.ws_token;
                  if (latestToken) {
                    ws.send(latestToken);
                  } else {
                      console.warn('No WebSocket token available - authentication may fail');
                  }

                  term.writeln(i18n.t('terminal.ssh.connected') + "\r\n");
                  // New SSH terminals are created with a deferred 0x0 PTY and
                  // wait for this first real resize frame before starting the
                  // remote shell at the correct size.
                  lastRemoteResizeRef.current = null;
                  syncRemotePtySize();
                  syncTrzszController();
                  updateTerminalReadiness(sessionId, {
                    terminalType: 'terminal',
                    frontendOutputListenerReady: true,
                    backendBufferReady: true,
                  });
                  // Focus terminal after connection only when it is appropriate
                  focusTerminal('soft');
                  resolve();
              };

              ws.onerror = async () => {
                  // Check both mount state and abort signal
                  if (!isMountedRef.current || abortController.signal.aborted) {
                    resolve();
                    return;
                  }
                  
                  // Clear current WS ref since it failed
                  if (wsRef.current === ws) {
                    wsRef.current = null;
                  }
                  updateTerminalReadiness(sessionId, {
                    terminalType: 'terminal',
                    frontendOutputListenerReady: false,
                  });
                  
                  if (!opened && attempt < maxAttempts) {
                    // Fast retry with exponential backoff: 250ms, 500ms, 750ms, 1000ms, 1250ms, 1500ms, 1750ms, 2000ms
                    // Total window: ~9 seconds, giving backend plenty of time during SSH handshake contention
                    const delay = Math.min(250 * attempt, 2000);
                    if (import.meta.env.DEV) {
                      console.warn(`[TerminalView ${sessionId}] Initial WS connect failed, retry #${attempt + 1} in ${delay}ms`);
                    }
                    // Abortable delay - check signal after timeout
                    await new Promise<void>(r => {
                      const timeoutId = setTimeout(() => r(), delay);
                      abortController.signal.addEventListener('abort', () => {
                        clearTimeout(timeoutId);
                        r();
                      }, { once: true });
                    });
                    // Don't continue if aborted
                    if (abortController.signal.aborted) {
                      resolve();
                      return;
                    }
                    await attemptWsConnect(attempt + 1, maxAttempts);
                    resolve();
                  } else if (!opened) {
                    // All fast retries failed, check if backend assigned a new port
                    // 🔴 再次检查，避免在组件卸载后调用 recoverWebSocket
                    if (!isMountedRef.current || abortController.signal.aborted) {
                      resolve();
                      return;
                    }
                    
                    // Check if session has a new ws_url (backend may have recreated)
                    const freshSession = useAppStore.getState().sessions.get(sessionId);
                    if (freshSession?.ws_url && freshSession.ws_url !== wsUrl) {
                      // New URL available, the force-remount via key prop will handle reconnection
                      if (import.meta.env.DEV) {
                        console.log(`[TerminalView ${sessionId}] Detected new ws_url, skipping recovery`);
                      }
                      resolve();
                      return;
                    }
                    
                    notifyTerminalLifecycle(
                      'ws-connection-failed',
                      i18n.t('terminal.ssh.ws_error', { error: 'Connection failed' }),
                      'warning',
                    );
                    recoverWebSocket('initial-error');
                    resolve();
                  }
              };

              ws.onclose = () => {
                  if (!isMountedRef.current || wsRef.current !== ws || abortController.signal.aborted) {
                    resolve();
                    return;
                  }
                  if (manualCloseRef.current) {
                    manualCloseRef.current = false;
                    resolve();
                    return;
                  }
                  updateTerminalReadiness(sessionId, {
                    terminalType: 'terminal',
                    frontendOutputListenerReady: false,
                  });
                  if (!opened && attempt < maxAttempts) {
                    // Connection closed before open - retry
                    return; // Let onerror handle retry
                  }
                  if (!reconnectingRef.current && opened) {
                    notifyTerminalLifecycle(
                      'ws-connection-closed',
                      i18n.t('terminal.ssh.connection_closed'),
                      'warning',
                    );
                    recoverWebSocket('initial-close-opened');
                  }
                  resolve();
              };

              ws.onmessage = (e) => handleWsMessageRef.current(e, ws);
            });
        };

        wsConnectTimeout = setTimeout(async () => {
            if (!isMountedRef.current) return;

            // Add random jitter (0-200ms) to prevent thundering herd when multiple terminals reconnect
            const jitter = Math.random() * 200;
            await new Promise(r => setTimeout(r, jitter));
            
            if (!isMountedRef.current || abortController.signal.aborted) return;

            // Start connection immediately, fonts can load in parallel
            // Font loading is non-blocking, xterm.js can re-render when fonts become available
            const connectPromise = attemptWsConnect(1, 5);
            const fontsPromise = ensureFontsLoaded();
            
            // Wait for connection, but don't block on fonts
            await Promise.race([connectPromise, fontsPromise.then(() => connectPromise)]);
        }, 100); // 100ms delay to let StrictMode unmount/remount complete
    } else {
        if (!resolveNodeForTerminalSession(sessionId)) {
          term.writeln(`\x1b[33m${i18n.t('terminal.ssh.no_ws_url')}\x1b[0m`);
        }
    }

    // IME composition event listeners (for Windows input method compatibility)
    const handleCompositionStart = () => {
      isComposingRef.current = true;
      if (import.meta.env.DEV) {
        console.log('[IME] Composition started - using RAF buffering');
      }
    };

    const handleCompositionEnd = () => {
      isComposingRef.current = false;
      if (import.meta.env.DEV) {
        console.log('[IME] Composition ended - using direct write');
      }
    };

    // Listen for composition events on the terminal element
    const terminalElement = term.element;
    terminalElement?.addEventListener('compositionstart', handleCompositionStart);
    terminalElement?.addEventListener('compositionend', handleCompositionEnd);

    // Terminal Input -> WebSocket (registered outside setTimeout to work immediately)
    // === Input Lock: Discard all input when in Standby mode ===
    // IMPORTANT: Save IDisposable for cleanup to prevent memory leaks
    onDataDisposableRef.current = term.onData(data => {
        // Strict input interception: discard input when connection is down/reconnecting
        if (inputLockedRef.current) {
          console.log('[TerminalView] Input discarded - connection in standby mode');
          return; // Discard input silently
        }
        
        // Notify adaptive renderer of user activity (exits idle tier)
        adaptiveRenderer.notifyUserInput();

        // Plugin input pipeline (fail-open, null = suppress)
        const processed = runInputPipeline(data, sessionId, nodeId);
        if (processed === null) return;
        const observedInput = autosuggestRecorderRef.current.observeInput(processed);
        if (observedInput.completedCommand && !isShellIntegrationDetected(effectivePaneId)) {
          createTerminalCommandMark(term, effectivePaneId, {
            command: observedInput.completedCommand,
            source: 'user_input_observed',
            sessionId,
            nodeId,
          });
        }
        observeCliAgentTerminalInput({
          data: processed,
          targetId: nodeId ? `ssh-node:${nodeId}` : undefined,
          sessionId,
          nodeId,
        });

        // Feed recording (user input)
        feedInput(processed);

        const controller = trzszControllerRef.current;
        if (controller && !controller.isDisposed()) {
            controller.processTerminalInput(processed);
        } else if (!controllerRuntimePendingRef.current) {
            sendEncodedTerminalInput(processed);
        }

        // Broadcast input to targets (empty target set = all other terminals)
        const bc = useBroadcastStore.getState();
        if (bc.enabled) {
          broadcastToTargets(effectivePaneId, processed, bc.targets);
        }
        
        // IDE Terminal: 检测回车键触发 Git 刷新
        // 仅当 sessionId 以 'ide-terminal-' 开头时触发（区分普通终端和 IDE 终端）
        if (sessionId.startsWith('ide-terminal-') && (data === '\r' || data === '\n')) {
          // 延迟 500ms 触发，给 git 命令执行时间
          if (gitRefreshTimerRef.current !== null) {
            clearTimeout(gitRefreshTimerRef.current);
          }
          gitRefreshTimerRef.current = setTimeout(() => {
            gitRefreshTimerRef.current = null;
            triggerGitRefresh();
          }, 500);
        }
    });

    // IMPORTANT: Save IDisposable for cleanup to prevent memory leaks
    onBinaryDisposableRef.current = term.onBinary((data) => {
        if (inputLockedRef.current) {
          return;
        }

        adaptiveRendererRef.current.notifyUserInput();

        const controller = trzszControllerRef.current;
        if (controller && !controller.isDisposed()) {
          controller.processBinaryInput(data);
        } else if (!controllerRuntimePendingRef.current) {
          transportRef.current?.sendBinaryInput(data);
        }
    });

    // IMPORTANT: Save IDisposable for cleanup to prevent memory leaks
    onResizeDisposableRef.current = term.onResize((size) => {
        // Don't send resize when in Standby mode or while hidden.
        if (inputLockedRef.current || !isTerminalContainerRenderable(containerRef.current)) return;

      scheduleRemotePtyResize(size.cols, size.rows);
    });

    // Track focus for split pane support
    // Update active pane in Registry when terminal receives focus
    // Note: xterm.js doesn't have onFocus, use DOM event on container
    const handleTerminalFocusIn = () => {
      setRegistryActivePaneId(effectivePaneId);
      touchTerminalEntry(effectivePaneId);
      onFocus?.(effectivePaneId);
    };
    
    // Add focus listener to terminal's element
    const termElement = term.element;
    if (termElement) {
      termElement.addEventListener('focusin', handleTerminalFocusIn);
    }

    // Handle Window Resize - use ResizeObserver for reliable detection
    // especially on Windows fullscreen transitions
    let resizeDebounceTimer: ReturnType<typeof setTimeout> | null = null;
    
    const handleResize = () => {
      // Debounce resize to avoid excessive fits during window transitions
      if (resizeDebounceTimer) {
        clearTimeout(resizeDebounceTimer);
      }
      resizeDebounceTimer = setTimeout(() => {
        if (!isTerminalContainerRenderable(containerRef.current)) {
          resizeDebounceTimer = null;
          return;
        }
        if (fitAddonRef.current && terminalRef.current && isMountedRef.current) {
          fitAddonRef.current.fit();
          syncRemotePtySize();
        }
        resizeDebounceTimer = null;
      }, 50); // 50ms debounce
    };

    // ResizeObserver for container size changes (more reliable than window.resize)
    // Handles: fullscreen toggle, sidebar collapse, multi-monitor DPI changes
    let resizeObserver: ResizeObserver | null = null;
    if (containerRef.current) {
      resizeObserver = new ResizeObserver(() => {
        handleResize();
      });
      resizeObserver.observe(containerRef.current);
    }

    // Also listen for window resize as fallback
    window.addEventListener('resize', handleResize);
    
    // Initial fit with delay for layout stabilization
    const initialFitTimer = setTimeout(() => {
        if (isMountedRef.current && fitAddonRef.current && isTerminalContainerRenderable(containerRef.current)) {
          fitAddonRef.current.fit();
          syncRemotePtySize();
        }
    }, 100);

    return () => {
      isMountedRef.current = false;
      
      // Cancel pending timers
      clearTimeout(initialFitTimer);
      if (gitRefreshTimerRef.current !== null) {
        clearTimeout(gitRefreshTimerRef.current);
        gitRefreshTimerRef.current = null;
      }
      
      // Abort any pending WS connect retries immediately
      if (wsConnectAbortRef.current) {
        wsConnectAbortRef.current.abort();
        wsConnectAbortRef.current = null;
      }
      
      // Unregister from Terminal Registry
      unregisterTerminalBuffer(effectivePaneId);
      clipboardInitCancelled = true;
      
      // Cleanup resize handling
      if (resizeDebounceTimer) {
        clearTimeout(resizeDebounceTimer);
      }
      if (resizeObserver) {
        resizeObserver.disconnect();
      }
      window.removeEventListener('resize', handleResize);

      // Cleanup composition event listeners
      terminalElement?.removeEventListener('compositionstart', handleCompositionStart);
      terminalElement?.removeEventListener('compositionend', handleCompositionEnd);

      // Remove focus listener
      if (termElement) {
        termElement.removeEventListener('focusin', handleTerminalFocusIn);
      }
      osc133Disposable.dispose();
      osc633Disposable.dispose();
      shellIntegrationController.dispose();

      if (wsConnectTimeout) {
          clearTimeout(wsConnectTimeout);
      }
      // Adaptive renderer cleanup is handled by the hook's own useEffect.
      // No manual RAF cancellation needed here.
        if (wsRef.current) {
          manualCloseRef.current = true;
          cleanupWebSocket(wsRef.current, 'Unmount');
          wsRef.current = null;
        }
        lastWsUrlRef.current = null;
      
        // Dispose renderer addon first to avoid "onShowLinkUnderline" error
        // This is a known xterm.js canvas addon bug where dispose order matters
        if (rendererAddonRef.current) {
          cleanupTerminalCommandMarks(effectivePaneId);
          try {
            rendererAddonRef.current.dispose();
          } catch (e) {
            // Ignore errors during addon disposal
          }
          rendererAddonRef.current = null;
        }

        // Dispose plugins (image/search/weblinks/fit) before terminal
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

        if (searchAddonRef.current) {
          try {
            searchAddonRef.current.dispose();
          } catch (e) {
            // Ignore errors during addon disposal
          }
          searchAddonRef.current = null;
        }

        if (webLinksAddonRef.current) {
          try {
            webLinksAddonRef.current.dispose();
          } catch (e) {
            // Ignore errors during addon disposal
          }
          webLinksAddonRef.current = null;
        }

        if (fitAddonRef.current) {
          try {
            fitAddonRef.current.dispose();
          } catch (e) {
            // Ignore errors during addon disposal
          }
          fitAddonRef.current = null;
        }

        // Dispose terminal event listeners (onData, onResize) before terminal
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

        if (onResizeDisposableRef.current) {
          try {
            onResizeDisposableRef.current.dispose();
          } catch (e) {
            // Ignore errors during disposal
          }
          onResizeDisposableRef.current = null;
        }

        disposeTrzszController();

        // Finally dispose terminal
        term.dispose();
        terminalRef.current = null;
    };
  }, [fontOpenReady, sessionId, syncRemotePtySize, scheduleRemotePtyResize, syncTrzszController, disposeTrzszController, sendEncodedTerminalInput, resolveNodeForTerminalSession, notifyTerminalLifecycle]); // Only remount on session change — bg image is handled dynamically

  useEffect(() => {
    selectionGestureRef.current?.refresh();
  }, [terminalSettings.selectionRequiresShift]);

  const sendProgrammaticTerminalInput = useCallback((input: string, mode: 'text' | 'execute' = 'text'): boolean => {
    const controller = trzszControllerRef.current;
    if (controller) {
      return mode === 'execute'
        ? controller.sendExecuteInput(input)
        : controller.sendTextInput(input);
    }

    if (controllerRuntimePendingRef.current) {
      return false;
    }

    if (!transportRef.current) {
      return false;
    }

    const formatted = mode === 'execute'
      ? `${formatTerminalTextInput(input)}\n`
      : formatTerminalTextInput(input);
    sendEncodedTerminalInput(formatted);
    return true;
  }, [sendEncodedTerminalInput]);

  // Listen for AI insert command events (only when this terminal is active and connected)
  useEffect(() => {
    if (!isActive) return;
    const currentSession = sessionRef.current;
    if (!currentSession || currentSession.state !== 'connected') return;

    let mounted = true;
    let unlistenFn: (() => void) | null = null;

    listen<{ command: string }>('ai-insert-command', (event) => {
      if (!mounted || !isMountedRef.current) return;
      if (inputLockedRef.current) return; // Don't insert during standby mode

      const { command } = event.payload;
      sendProgrammaticTerminalInput(command);
    }).then((fn) => {
      if (mounted) {
        unlistenFn = fn;
      } else {
        fn(); // Component unmounted before listener registered, clean up immediately
      }
    });

    return () => {
      mounted = false;
      unlistenFn?.();
    };
  }, [isActive, sendProgrammaticTerminalInput, session?.state]);

  /**
   * Handle container click - focus terminal and update active pane
   */
  const handleContainerClick = (event: React.MouseEvent) => {
    // Update active pane in Registry and notify parent
    setRegistryActivePaneId(effectivePaneId);
    touchTerminalEntry(effectivePaneId);
    onFocus?.(effectivePaneId);

    if (!searchOpen && !aiPanelOpen && shouldFocusTerminalFromClick(event.target)) {
      focusTerminal('strong');
    }
  };

  const cancelCommandMarkDoubleClick = useCallback((event: React.MouseEvent) => {
    if (event.button !== 0 || event.detail <= 1) return false;
    commandMarkPointerRef.current = null;
    clearTerminalCommandMarkSelection(effectivePaneId);
    terminalRef.current?.clearSelection();
    event.preventDefault();
    event.stopPropagation();
    (event.nativeEvent as MouseEvent & { stopImmediatePropagation?: () => void }).stopImmediatePropagation?.();
    return true;
  }, [effectivePaneId]);

  const handleCommandMarkPointerDown = useCallback((event: React.MouseEvent) => {
    if (cancelCommandMarkDoubleClick(event)) return;
    if (event.button !== 0 || !containerRef.current?.contains(event.target as Node)) {
      commandMarkPointerRef.current = null;
      return;
    }
    commandMarkPointerRef.current = {
      x: event.clientX,
      y: event.clientY,
      selection: terminalRef.current?.getSelection() ?? '',
    };
  }, [cancelCommandMarkDoubleClick]);

  const handleCommandMarkPointerUp = useCallback((event: React.MouseEvent) => {
    if (cancelCommandMarkDoubleClick(event)) return;
    const start = commandMarkPointerRef.current;
    commandMarkPointerRef.current = null;
    const term = terminalRef.current;
    const container = containerRef.current;
    if (!start || !term || !container?.contains(event.target as Node)) return;
    if (event.button !== 0) return;
    if (Math.hypot(event.clientX - start.x, event.clientY - start.y) > 4) return;
    const selection = term.getSelection();
    if (term.buffer.active.type === 'alternate' || (selection && selection !== start.selection)) return;
    const line = getTerminalAbsoluteLineFromClientY(term, container, event.clientY);
    if (line === null || !selectTerminalCommandMarkAtLine(term, effectivePaneId, line)) {
      clearTerminalCommandMarkSelection(effectivePaneId);
    }
  }, [cancelCommandMarkDoubleClick, effectivePaneId]);

  const currentTheme = getTerminalTheme(terminalSettings.theme);

  // ── Background Image ──────────────────────────────────────────────────
  // Compute the asset:// URL and effective blur (capped on low-end GPU)
  const bgImageUrl = React.useMemo(
    () => {
      const masterOn = terminalSettings.backgroundEnabled !== false;
      const enabled = (terminalSettings.backgroundEnabledTabs ?? ['terminal', 'local_terminal']).includes('terminal');
      return masterOn && terminalSettings.backgroundImage && enabled ? convertFileSrc(terminalSettings.backgroundImage) : null;
    },
    [terminalSettings.backgroundEnabled, terminalSettings.backgroundImage, terminalSettings.backgroundEnabledTabs]
  );
  const effectiveBlur = React.useMemo(() => {
    if (!bgImageUrl) return 0;
    const raw = terminalSettings.backgroundBlur;
    return isLowEndGPU() ? Math.min(raw, 5) : raw;
  }, [bgImageUrl, terminalSettings.backgroundBlur]);

  // === SearchAddon API for SearchBar ===
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
  
  const handleFindNext = useCallback(() => {
    const query = currentSearchQueryRef.current;
    if (!query) return;
    const searchAddon = ensureSearchAddon();
    if (!searchAddon) return;
    searchAddon.findNext(query, currentSearchOptionsRef.current);
  }, [ensureSearchAddon]);
  
  const handleFindPrevious = useCallback(() => {
    const query = currentSearchQueryRef.current;
    if (!query) return;
    const searchAddon = ensureSearchAddon();
    if (!searchAddon) return;
    searchAddon.findPrevious(query, currentSearchOptionsRef.current);
  }, [ensureSearchAddon]);
  
  const handleCloseSearch = useCallback(() => {
    const activeSearchId = activeHistorySearchIdRef.current;
    if (activeSearchId) {
      activeHistorySearchIdRef.current = null;
      void api.cancelTerminalHistorySearch(activeSearchId);
    }
    setSearchOpen(false);
    searchAddonRef.current?.clearDecorations();
    searchAddonRef.current?.dispose();
    searchAddonRef.current = null;
    setSearchResults({ resultIndex: -1, resultCount: 0 });
    setDeepSearchState({ loading: false, matches: [], totalMatches: 0, durationMs: 0 });
    currentSearchQueryRef.current = '';
    requestAnimationFrame(() => {
      focusTerminal('strong');
    });
  }, [focusTerminal]);
  
  // === Deep History Search ===
  const handleDeepSearch = useCallback(async (query: string, options: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }) => {
    if (!query.trim()) return;

    const previousSearchId = activeHistorySearchIdRef.current;
    if (previousSearchId) {
      activeHistorySearchIdRef.current = null;
      void api.cancelTerminalHistorySearch(previousSearchId);
    }
    
    setDeepSearchState({
      loading: true,
      matches: [],
      totalMatches: 0,
      durationMs: 0,
      searchedChunks: 0,
      totalChunks: 0,
      truncated: false,
      partialFailure: false,
      error: undefined,
      archiveStatus: undefined,
      excerpt: undefined,
    });
    
    try {
      const result = await api.startTerminalHistorySearch(sessionId, {
        query,
        case_sensitive: options.caseSensitive || false,
        regex: options.regex || false,
        whole_word: options.wholeWord || false,
        max_matches: 100,
      });

      activeHistorySearchIdRef.current = result.search_id;
      setDeepSearchState((prev) => ({ ...prev, searchId: result.search_id }));
    } catch (err) {
      setDeepSearchState({
        loading: false,
        matches: [],
        totalMatches: 0,
        durationMs: 0,
        searchedChunks: 0,
        totalChunks: 0,
        truncated: false,
        partialFailure: false,
        error: err instanceof Error ? err.message : 'Search failed',
      });
    }
  }, [sessionId]);
  
  // === Jump to search match from deep history ===
  const handleJumpToMatch = useCallback((match: HistorySearchMatch) => {
    setDeepSearchState((prev) => ({ ...prev, excerpt: undefined }));
    setScrollbackInitialMatch(match);
    setScrollbackOpen(true);
  }, []);
  
  // === AI Panel Helper Functions ===
  
  // Get selected text from terminal
  const getTerminalSelection = useCallback((): string => {
    const term = terminalRef.current;
    if (!term) return '';
    return term.getSelection() || '';
  }, []);
  
  // Get visible buffer content
  const getVisibleBuffer = useCallback((): string => {
    const term = terminalRef.current;
    if (!term) return '';
    
    const { settings } = useSettingsStore.getState();
    const maxLines = settings.ai.contextVisibleLines;
    
    // Get the active buffer
    const buffer = term.buffer.active;
    const totalLines = buffer.length;
    const viewportRows = term.rows;
    
    // Calculate range to read (from bottom, limited by maxLines)
    const endLine = buffer.baseY + viewportRows;
    const startLine = Math.max(0, endLine - maxLines);
    
    const lines: string[] = [];
    for (let i = startLine; i < endLine && i < totalLines; i++) {
      const line = buffer.getLine(i);
      if (line) {
        lines.push(line.translateToString(true));
      }
    }
    
    return lines.join('\n');
  }, []);
  
  // Get cursor position for AI inline panel positioning
  const getCursorPosition = useCallback((): CursorPosition | null => {
    const term = terminalRef.current;
    const container = containerRef.current;
    if (!term || !container) return null;
    
    const buffer = term.buffer.active;
    const cursorX = buffer.cursorX;
    const cursorY = buffer.cursorY;
    const absoluteY = buffer.baseY + cursorY;
    
    // Get cell dimensions from xterm.js (requires DOM access)
    const termElement = term.element;
    if (!termElement) return null;
    
    // Get container rect for boundary calculations
    const containerRect = container.getBoundingClientRect();
    
    // Calculate cell dimensions
    // xterm.js stores dimensions in _core (internal API, but reliable)
    const core = (term as unknown as { _core?: { _renderService?: { dimensions?: { css: { cell: { width: number; height: number } } } } } })._core;
    const dimensions = core?._renderService?.dimensions;
    
    let lineHeight = 20; // Default fallback
    let charWidth = 9;   // Default fallback
    
    if (dimensions?.css?.cell) {
      lineHeight = dimensions.css.cell.height;
      charWidth = dimensions.css.cell.width;
    } else {
      // Fallback: estimate from font size
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

  // Insert text at cursor
  const handleAiInsert = useCallback((text: string) => {
    if (inputLockedRef.current) return; // Respect standby mode
    sendProgrammaticTerminalInput(text);
  }, [sendProgrammaticTerminalInput]);
  
  // Execute command (insert + enter)
  const handleAiExecute = useCallback((command: string) => {
    if (inputLockedRef.current) return; // Respect standby mode
    sendProgrammaticTerminalInput(command, 'execute');
  }, [sendProgrammaticTerminalInput]);
  
  const handleCloseAiPanel = useCallback(() => {
    setAiPanelOpen(false);
    setAiCursorPosition(null);
    requestAnimationFrame(() => {
      focusTerminal('strong');
    });
  }, [focusTerminal]);

  // Paste protection: handle pending paste confirm
  const handlePasteConfirm = useCallback(() => {
    if (inputLockedRef.current) return; // Respect standby mode
    if (pendingPaste) {
      sendProgrammaticTerminalInput(pendingPaste);
    }
    setPendingPaste(null);
    requestAnimationFrame(() => {
      focusTerminal('strong');
    });
  }, [focusTerminal, pendingPaste, sendProgrammaticTerminalInput]);

  const handlePasteCancel = useCallback(() => {
    setPendingPaste(null);
    requestAnimationFrame(() => {
      focusTerminal('strong');
    });
  }, [focusTerminal]);

  const processTerminalPaste = useCallback((text: string | null | undefined, allowNativePassthrough: boolean): boolean => {
    const decision = getProtectedPasteDecision(text, !inputLockedRef.current);

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

    return sendProgrammaticTerminalInput(text);
  }, [sendProgrammaticTerminalInput]);

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
    searchOpen || aiPanelOpen || scrollbackOpen,
    {
      onOpenSearch: () => setSearchOpen(true),
      onCloseSearch: handleCloseSearch,
      onOpenAiPanel: () => {
        // Check if AI is enabled in settings
        const { settings } = useSettingsStore.getState();
        if (settings.ai.enabled) {
          // Calculate cursor position before opening panel
          const position = getCursorPosition();
          setAiCursorPosition(position);
          setAiPanelOpen(true);
        }
      },
      onCloseAiPanel: handleCloseAiPanel,
      onToggleRecording: () => {
        if (!isSessionRecording) {
          const term = terminalRef.current;
          if (term) {
            startRecording(term.cols, term.rows);
          }
        }
        // Stop is handled by RecordingControls overlay
      },
      onFocusTerminal: () => {
        focusTerminal('strong');
      },
      searchOpen,
      aiPanelOpen,
    }
  );
  
  return (
    <div 
      className="terminal-container group h-full w-full overflow-hidden relative flex flex-col"
      style={{ 
        backgroundColor: currentTheme.background 
      }}
      onClick={handleContainerClick}
      onMouseDownCapture={handleCommandMarkPointerDown}
      onMouseUpCapture={handleCommandMarkPointerUp}
    >
       {/* Background Image Layer — GPU-composited, sits below xterm canvas.
           Uses will-change: transform to promote to its own compositor layer,
           so scrolling/typing only repaints the xterm canvas above, not this layer. */}
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

       <div 
         ref={containerRef} 
         className="min-h-0 flex-1 w-full"
         style={{
           contain: 'strict',
           isolation: 'isolate',
           position: 'relative',
           zIndex: 1,
         }}
       />

       {!scrollbackOpen && (
         <button
           type="button"
           onClick={(event) => {
             event.stopPropagation();
             setScrollbackInitialMatch(null);
             setScrollbackOpen(true);
           }}
           className="absolute right-2 top-2 z-20 rounded border border-theme-border/70 bg-theme-bg-panel/85 p-1.5 text-theme-text-muted opacity-0 shadow-lg transition-all hover:text-theme-accent focus:opacity-100 group-hover:opacity-100"
           title={t('terminal.scrollback_viewer.open')}
           aria-label={t('terminal.scrollback_viewer.open')}
         >
           <ScrollText className="h-4 w-4" />
         </button>
       )}

       <ScrollbackViewer
         sessionId={sessionId}
         nodeId={nodeId ?? effectivePaneId}
         isOpen={scrollbackOpen}
         initialMatch={scrollbackInitialMatch}
         onClose={() => setScrollbackOpen(false)}
       />
       {/* Input Lock Overlay - shown during reconnection */}
       {inputLocked && (
         <div className={cn(
           "absolute inset-0 bg-black/40 flex items-center justify-center z-10",
           linuxBackdropBlurClass("backdrop-blur-sm"),
         )}>
           <div className="bg-theme-bg-panel/95 border border-theme-border rounded-lg px-6 py-4 flex flex-col items-center gap-3 shadow-xl">
             <div className="flex items-center gap-2 text-amber-400">
               {connectionStatus === 'reconnecting' ? (
                 <Loader2 className="h-5 w-5 animate-spin" />
               ) : (
                 <Lock className="h-5 w-5" />
               )}
               <span className="font-medium">
                 {connectionStatus === 'link_down' && t('terminal.standby.connection_lost')}
                 {connectionStatus === 'reconnecting' && t('terminal.standby.reconnecting')}
               </span>
             </div>
             <div className="text-xs text-theme-text-muted text-center">
               {t('terminal.standby.input_locked')}
             </div>
           </div>
         </div>
       )}

       {/* Disconnected Overlay - shown when connection is permanently lost */}
       {!inputLocked && connectionStatus === 'disconnected' && (
         <div className="absolute inset-x-0 bottom-0 z-10 flex justify-center pb-6 pointer-events-none">
           <div className="pointer-events-auto bg-theme-bg-panel/95 border border-theme-border rounded-lg px-5 py-3 flex items-center gap-3 shadow-xl">
             <AlertTriangle className="h-4 w-4 text-red-400 shrink-0" />
             <span className="text-sm text-theme-text">{t('terminal.disconnected.message')}</span>
             {nodeId && (
               <button
                 onClick={() => useReconnectOrchestratorStore.getState().scheduleReconnect(nodeId)}
                 className="inline-flex items-center gap-1.5 px-3 py-1 rounded bg-theme-accent/20 hover:bg-theme-accent/30 text-theme-accent text-sm font-medium transition-colors"
               >
                 <RefreshCw className="h-3.5 w-3.5" />
                 {t('terminal.disconnected.retry')}
               </button>
             )}
           </div>
         </div>
       )}
       
       {/* Paste Confirmation Overlay */}
       {pendingPaste && (
         <PasteConfirmOverlay
           content={pendingPaste}
           onConfirm={handlePasteConfirm}
           onCancel={handlePasteCancel}
         />
       )}
       
       {/* Search Bar - using xterm.js SearchAddon */}
       <SearchBar 
         isOpen={searchOpen}
         onClose={handleCloseSearch}
         onSearch={handleSearch}
         onFindNext={handleFindNext}
         onFindPrevious={handleFindPrevious}
         resultIndex={searchResults.resultIndex}
         resultCount={searchResults.resultCount}
         onDeepSearch={handleDeepSearch}
         onJumpToMatch={handleJumpToMatch}
         deepSearchState={deepSearchState}
       />
       
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
         terminalType="terminal"
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
       {mouseMode && (
         <div className="absolute bottom-2 right-2 bg-theme-bg-hover/70 text-theme-text-muted text-[11px] px-2 py-0.5 rounded pointer-events-none select-none">
           {t('terminal.mouse_mode_hint')}
         </div>
       )}

       {/* FPS / Tier overlay (enabled in Settings → Terminal → Show FPS Overlay) */}
       {terminalSettings.showFpsOverlay && (
         <FpsOverlay getStats={adaptiveRenderer.getStats} />
       )}
       {(terminalSettings.commandBar?.enabled ?? false) && (
         <TerminalCommandBar
           paneId={effectivePaneId}
           sessionId={sessionId}
           tabId={effectiveTabId}
           terminalType="terminal"
           nodeId={nodeId}
           isActive={isActive}
           sendInput={sendCommandBarInput}
           focusTerminal={() => { focusTerminal('strong'); }}
           onLayoutChange={handleCommandBarLayoutChange}
         />
       )}
    </div>
  );
};
