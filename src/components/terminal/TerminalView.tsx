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
import { api } from '../../lib/api';
import { themes } from '../../lib/themes';
import { platform } from '../../lib/platform';
import { useTerminalViewShortcuts } from '../../hooks/useTerminalKeyboard';
import { SearchBar, DeepSearchState } from './SearchBar';
import { AiInlinePanel } from './AiInlinePanel';
import { PasteConfirmOverlay, shouldConfirmPaste } from './PasteConfirmOverlay';
import { terminalLinkHandler } from '../../lib/safeUrl';
import { SearchMatch, SessionInfo } from '../../types';
import { listen } from '@tauri-apps/api/event';
import { Lock, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import i18n from '../../i18n';
import { 
  registerTerminalBuffer, 
  unregisterTerminalBuffer, 
  setActivePaneId as setRegistryActivePaneId,
  touchTerminalEntry 
} from '../../lib/terminalRegistry';

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

// Protocol Constants - Wire Protocol v1
// Frame Format: [Type: 1 byte][Length: 4 bytes big-endian][Payload: n bytes]
const MSG_TYPE_DATA = 0x00;
const MSG_TYPE_RESIZE = 0x01;
const MSG_TYPE_HEARTBEAT = 0x02;
const MSG_TYPE_ERROR = 0x03;
const HEADER_SIZE = 5; // 1 byte type + 4 bytes length

// Helper function to encode a heartbeat response frame
const encodeHeartbeatFrame = (seq: number): Uint8Array => {
  const frame = new Uint8Array(HEADER_SIZE + 4); // 4 bytes for sequence number
  const view = new DataView(frame.buffer);
  view.setUint8(0, MSG_TYPE_HEARTBEAT);  // Type
  view.setUint32(1, 4, false);           // Length (4 bytes payload)
  view.setUint32(5, seq, false);         // Sequence number (big-endian)
  return frame;
};

// Helper function to encode a data frame
const encodeDataFrame = (payload: Uint8Array): Uint8Array => {
  const frame = new Uint8Array(HEADER_SIZE + payload.length);
  const view = new DataView(frame.buffer);
  view.setUint8(0, MSG_TYPE_DATA);           // Type
  view.setUint32(1, payload.length, false);  // Length (big-endian)
  frame.set(payload, HEADER_SIZE);           // Payload
  return frame;
};

// Helper function to encode a resize frame
const encodeResizeFrame = (cols: number, rows: number): Uint8Array => {
  const frame = new Uint8Array(HEADER_SIZE + 4); // 4 bytes for cols + rows
  const view = new DataView(frame.buffer);
  view.setUint8(0, MSG_TYPE_RESIZE);  // Type
  view.setUint32(1, 4, false);        // Length (4 bytes payload)
  view.setUint16(5, cols, false);     // Cols (big-endian)
  view.setUint16(7, rows, false);     // Rows (big-endian)
  return frame;
};

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
  const fitAddonRef = useRef<FitAddon | null>(null);
  const searchAddonRef = useRef<SearchAddon | null>(null);
  const imageAddonRef = useRef<ImageAddon | null>(null);
  const rendererAddonRef = useRef<{ dispose: () => void } | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const isMountedRef = useRef(true); // Track mount state for StrictMode
  const reconnectingRef = useRef(false); // Suppress close/error during intentional reconnect
  const [searchOpen, setSearchOpen] = useState(false);
  const [aiPanelOpen, setAiPanelOpen] = useState(false);
  
  // Effective pane ID: use provided paneId or fall back to sessionId
  const effectivePaneId = paneId || sessionId;
  const effectiveTabId = tabId || '';
  
  // Paste protection state
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  
  // Search state - synced with SearchAddon's onDidChangeResults
  const [searchResults, setSearchResults] = useState<{ resultIndex: number; resultCount: number }>({ 
    resultIndex: -1, 
    resultCount: 0 
  });
  // Track current search query for navigation
  const currentSearchQueryRef = useRef<string>('');
  const currentSearchOptionsRef = useRef<ISearchOptions | undefined>(undefined);
  
  // Deep history search state
  const [deepSearchState, setDeepSearchState] = useState<DeepSearchState>({
    loading: false,
    matches: [],
    totalMatches: 0,
    durationMs: 0,
  });
  
  // P3: Backpressure handling - batch terminal writes with RAF
  const pendingDataRef = useRef<Uint8Array[]>([]);
  const rafIdRef = useRef<number | null>(null);

  // IME composition state tracking (for Windows input method compatibility)
  const isComposingRef = useRef(false);

  // Track last connected ws_url for reconnection detection
  const lastWsUrlRef = useRef<string | null>(null);
  
  // === Standby Mode State (Input Lock during reconnection) ===
  const [inputLocked, setInputLocked] = useState(false);
  const [connectionStatus, setConnectionStatus] = useState<'connected' | 'link_down' | 'reconnecting' | 'disconnected'>('connected');
  const inputLockedRef = useRef(false); // For synchronous check in onData callback
  
  const { getSession } = useAppStore();
  const session = getSession(sessionId);
  const sessionRef = useRef<SessionInfo | undefined>(session);
  const connectionIdRef = useRef<string | null>(session?.connectionId ?? null);

  useEffect(() => {
    sessionRef.current = session;
  }, [session]);

  useEffect(() => {
    connectionIdRef.current = session?.connectionId ?? null;
  }, [session?.connectionId]);

  // Get terminal settings from unified store
  const terminalSettings = useSettingsStore((state) => state.settings.terminal);

  // === Listen for connection status changes (Standby mode trigger) ===
  useEffect(() => {
    let mounted = true;
    let unlistenFn: (() => void) | null = null;
    
    interface ConnectionStatusEvent {
      connection_id: string;
      status: 'connected' | 'link_down' | 'reconnecting' | 'disconnected';
    }

    listen<ConnectionStatusEvent>('connection_status_changed', (event) => {
      if (!mounted) return;
      
      const { connection_id, status } = event.payload;
      
      const currentConnectionId = connectionIdRef.current;
      if (!currentConnectionId) return;
      // Only handle events for our connection
      if (connection_id !== currentConnectionId) return;
      
      console.log(`[TerminalView ${sessionId}] Connection status: ${status}`);
      setConnectionStatus(status);
      
      const term = terminalRef.current;
      const shouldLock = status === 'link_down' || status === 'reconnecting';
      
      if (shouldLock && !inputLockedRef.current) {
        // Entering Standby mode
        inputLockedRef.current = true;
        setInputLocked(true);
        
        // Write status message (NO clear!)
        if (term) {
          if (status === 'link_down') {
            term.write(`\r\n\x1b[33m${i18n.t('terminal.ssh.connection_lost')}\x1b[0m\r\n`);
          } else if (status === 'reconnecting') {
            term.write(`\r\n\x1b[33m${i18n.t('terminal.ssh.attempting_reconnect')}\x1b[0m\r\n`);
          }
        }
      } else if (!shouldLock && inputLockedRef.current) {
        // Exiting Standby mode
        inputLockedRef.current = false;
        setInputLocked(false);
        
        if (term && status === 'connected') {
          term.write(`\r\n\x1b[32m${i18n.t('terminal.ssh.link_restored')}\x1b[0m\r\n`);
        } else if (term && status === 'disconnected') {
          term.write(`\r\n\x1b[31m${i18n.t('terminal.ssh.connection_failed')}\x1b[0m\r\n`);
        }
      }
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
  }, [sessionId]);

  // Subscribe to terminal settings changes from settingsStore
  useEffect(() => {
    const unsubscribe = useSettingsStore.subscribe(
      (state) => state.settings.terminal,
      (terminal) => {
        const term = terminalRef.current;
        if (!term) return;
        
        term.options.fontFamily = getFontFamily(terminal.fontFamily);
        term.options.fontSize = terminal.fontSize;
        term.options.cursorStyle = terminal.cursorStyle;
        term.options.cursorBlink = terminal.cursorBlink;
        term.options.lineHeight = terminal.lineHeight;
        
        // Apply theme update
        const themeConfig = themes[terminal.theme] || themes.default;
        term.options.theme = themeConfig;
        
        term.refresh(0, term.rows - 1);
        fitAddonRef.current?.fit();
      }
    );
    return unsubscribe;
  }, []);

  // Focus terminal when it becomes active (tab switch)
  useEffect(() => {
    if (isActive && terminalRef.current && !searchOpen && !aiPanelOpen) {
      // Small delay to ensure DOM is ready
      const focusTimeout = setTimeout(() => {
        if (!searchOpen && !aiPanelOpen) { // Double-check before focusing
          terminalRef.current?.focus();
        }
        fitAddonRef.current?.fit();
      }, 50);
      return () => clearTimeout(focusTimeout);
    }
  }, [isActive, searchOpen, aiPanelOpen]);

  // WebSocket reconnection effect - triggers when ws_url changes (after auto-reconnect)
  useEffect(() => {
    const currentSession = sessionRef.current;
    const wsUrl = currentSession?.ws_url;
    // Skip if terminal not initialized or no ws_url
    if (!terminalRef.current || !wsUrl) return;
    
    // Skip if this is the same URL we're already connected to
    if (wsUrl === lastWsUrlRef.current) {
      const existingWs = wsRef.current;
      if (existingWs && existingWs.readyState <= WebSocket.OPEN) {
        return;
      }
      // If ws exists but is closed, allow reconnect to same URL
    }
    
    // Skip if WebSocket is already open/connecting to same URL
    if (wsRef.current && wsRef.current.readyState <= WebSocket.OPEN) {
      // If old connection exists but URL changed, close it
      if (lastWsUrlRef.current !== null && wsUrl !== lastWsUrlRef.current) {
        console.log('[Terminal] Session reconnected, closing old WebSocket and reconnecting...');
        reconnectingRef.current = true;
        const oldWs = wsRef.current;
        wsRef.current = null;
        oldWs.close(1000, 'Reconnect');
      } else {
        return; // Same URL, already connected
      }
    }
    
    const term = terminalRef.current;
    const wsToken = currentSession?.ws_token;
    const displayUser = currentSession?.username ?? 'unknown';
    const displayHost = currentSession?.host ?? 'unknown';
    
    term.writeln(`\r\n\x1b[33m${i18n.t('terminal.ssh.reconnecting', { user: displayUser, host: displayHost })}\x1b[0m`);
    
    try {
      const ws = new WebSocket(wsUrl);
      ws.binaryType = 'arraybuffer';
      wsRef.current = ws;
      lastWsUrlRef.current = wsUrl;

      ws.onopen = () => {
        if (!isMountedRef.current) {
          ws.close();
          return;
        }
        reconnectingRef.current = false;
        
        // Send authentication token
        if (wsToken) {
          ws.send(wsToken);
        }
        
        term.writeln(`\x1b[32m${i18n.t('terminal.ssh.reconnected')}\x1b[0m\r\n`);
        
        // Re-send current terminal size
        if (fitAddonRef.current) {
          const dims = fitAddonRef.current.proposeDimensions();
          if (dims) {
            const frame = encodeResizeFrame(dims.cols, dims.rows);
            ws.send(frame);
          }
        }
      };

      ws.onmessage = (event) => {
        if (!isMountedRef.current || wsRef.current !== ws) return;
        
        const data = new Uint8Array(event.data);
        if (data.length < HEADER_SIZE) return;
        
        const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
        const msgType = view.getUint8(0);
        const length = view.getUint32(1, false);
        
        if (data.length < HEADER_SIZE + length) return;
        
        const payload = data.slice(HEADER_SIZE, HEADER_SIZE + length);
        
        switch (msgType) {
          case MSG_TYPE_DATA:
            if (platform.isWindows) {
              // Windows: 根据是否在 IME 合成中决定策略
              if (isComposingRef.current) {
                // IME 合成期间：使用 RAF 缓冲，避免候选框抖动
                pendingDataRef.current.push(payload);
                if (rafIdRef.current === null) {
                  rafIdRef.current = requestAnimationFrame(() => {
                    if (pendingDataRef.current.length > 0 && terminalRef.current) {
                      const combined = new Uint8Array(
                        pendingDataRef.current.reduce((acc, arr) => acc + arr.length, 0)
                      );
                      let offset = 0;
                      for (const chunk of pendingDataRef.current) {
                        combined.set(chunk, offset);
                        offset += chunk.length;
                      }
                      pendingDataRef.current = [];
                      terminalRef.current.write(combined);
                    }
                    rafIdRef.current = null;
                  });
                }
              } else {
                // 非合成期间：直接写入，最小化延迟
                if (terminalRef.current) {
                  terminalRef.current.write(payload);
                }
              }
            } else {
              // macOS/Linux: 继续使用 RAF 批处理以提升性能
              pendingDataRef.current.push(payload);
              if (rafIdRef.current === null) {
                rafIdRef.current = requestAnimationFrame(() => {
                  if (pendingDataRef.current.length > 0 && terminalRef.current) {
                    const combined = new Uint8Array(
                      pendingDataRef.current.reduce((acc, arr) => acc + arr.length, 0)
                    );
                    let offset = 0;
                    for (const chunk of pendingDataRef.current) {
                      combined.set(chunk, offset);
                      offset += chunk.length;
                    }
                    pendingDataRef.current = [];
                    terminalRef.current.write(combined);
                  }
                  rafIdRef.current = null;
                });
              }
            }
            break;
          case MSG_TYPE_HEARTBEAT:
            if (payload.length >= 4) {
              const seq = view.getUint32(HEADER_SIZE, false);
              ws.send(encodeHeartbeatFrame(seq));
            }
            break;
          case MSG_TYPE_ERROR:
            const errorMsg = new TextDecoder().decode(payload);
            term.writeln(`\r\n\x1b[31m${i18n.t('terminal.ssh.server_error', { error: errorMsg })}\x1b[0m`);
            break;
        }
      };

      ws.onerror = (error) => {
        if (!isMountedRef.current || wsRef.current !== ws) return;
        console.error('WebSocket reconnection error:', error);
        term.writeln(`\r\n\x1b[31m${i18n.t('terminal.ssh.ws_reconnect_error')}\x1b[0m`);
      };

      ws.onclose = (event) => {
        if (!isMountedRef.current || wsRef.current !== ws) return;
        console.log('WebSocket closed after reconnect:', event.code, event.reason);
        if (event.code !== 1000) {
          term.writeln(`\r\n\x1b[33m${i18n.t('terminal.ssh.connection_closed_code', { code: event.code })}\x1b[0m`);
        }
      };
    } catch (e) {
      console.error('Failed to reconnect WebSocket:', e);
      term.writeln(`\r\n\x1b[31m${i18n.t('terminal.ssh.ws_establish_failed', { error: e })}\x1b[0m`);
    }
  }, [session?.ws_url]);

  const getFontFamily = (val: string) => {
      switch(val) {
          case 'jetbrains': return '"JetBrains Mono", monospace';
          case 'meslo': return '"MesloLGM Nerd Font", monospace';
          case 'tinos': return '"Tinos Nerd Font", monospace';
          default: return '"JetBrains Mono", monospace';
      }
  };

  useEffect(() => {
    if (!containerRef.current || terminalRef.current) return;
    
    isMountedRef.current = true; // Reset mount state

    // Initialize xterm.js
    const term = new Terminal({
      cursorBlink: terminalSettings.cursorBlink,
      cursorStyle: terminalSettings.cursorStyle,
      fontFamily: getFontFamily(terminalSettings.fontFamily),
      fontSize: terminalSettings.fontSize,
      lineHeight: terminalSettings.lineHeight,
      theme: themes[terminalSettings.theme] || themes.default,
      scrollback: terminalSettings.scrollback || 5000,
      allowProposedApi: true,
    });

    const fitAddon = new FitAddon();
    // WebLinksAddon with secure URL handler - blocks dangerous protocols (file://, javascript:, etc.)
    const webLinksAddon = new WebLinksAddon(terminalLinkHandler);
    const searchAddon = new SearchAddon();
    // ImageAddon for iTerm2 inline images protocol (OSC 1337) and SIXEL support
    // Enables image preview in tools like Yazi, imgcat, lsix
    const imageAddon = new ImageAddon({
      enableSizeReports: true,    // Enable CSI t reports for terminal metrics
      pixelLimit: 16777216,       // 4096x4096 pixels max per image
      storageLimit: 128,          // 128MB FIFO cache for images
      showPlaceholder: true,      // Show placeholder for evicted images
      sixelSupport: true,         // Enable SIXEL protocol
      iipSupport: true,           // Enable iTerm2 Inline Images Protocol
    });
    
    term.loadAddon(fitAddon);
    term.loadAddon(webLinksAddon);
    term.loadAddon(searchAddon);
    term.loadAddon(imageAddon);   // Load before term.open()
    
    // Unicode11Addon for proper Nerd Font icons and CJK wide character rendering
    // Required for Oh My Posh, Starship, and other modern prompts
    const unicode11Addon = new Unicode11Addon();
    term.loadAddon(unicode11Addon);
    term.unicode.activeVersion = '11';
    
    // Listen for search result changes
    searchAddon.onDidChangeResults((e) => {
      // Only update if there's an active search query to prevent spurious index updates
      if (currentSearchQueryRef.current) {
        setSearchResults({ resultIndex: e.resultIndex, resultCount: e.resultCount });
      }
    });
    
    searchAddonRef.current = searchAddon;
    imageAddonRef.current = imageAddon;

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
                console.log(`[Renderer] WebGL addon loaded (auto) with DPR: ${dpr}`);
            } catch (e) {
                console.warn('[Renderer] WebGL addon failed, trying Canvas fallback', e);
                // Fallback to Canvas
                const canvasAddon = await loadCanvasAddon();
                rendererAddonRef.current = canvasAddon;
                if (canvasAddon) {
                    console.log('[Renderer] Canvas addon loaded as fallback');
                } else {
                    console.warn('[Renderer] Canvas fallback failed, using DOM');
                }
            }
        }
    };
    
    loadRenderer();

    term.open(containerRef.current);
    fitAddon.fit();
    term.focus(); // Focus immediately after opening

    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    term.writeln(`\x1b[38;2;234;88;12m${i18n.t('terminal.ssh.initialized')}\x1b[0m`);
    
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
      
      // Read all lines from the buffer
      for (let i = 0; i < lineCount; i++) {
        const line = buffer.getLine(i);
        if (line) {
          lines.push(line.translateToString(true));
        }
      }
      
      return lines.join('\n');
    };
    
    // Register with paneId as key, not sessionId
    registerTerminalBuffer(
      effectivePaneId,
      effectiveTabId,
      sessionId,
      'terminal', // SSH terminal type
      getBufferContent
    );
    
    // Font loading detection - ensure fonts are loaded before connecting
    const ensureFontsLoaded = async () => {
        try {
            const fontsToCheck = ['JetBrains Mono', 'MesloLGM Nerd Font', 'Tinos Nerd Font'];
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

        wsConnectTimeout = setTimeout(async () => {
            if (!isMountedRef.current) return; // Check if still mounted after delay

            // Avoid stale ws_url from reconnect race
            const latestSession = useAppStore.getState().sessions.get(sessionId);
            if (!latestSession?.ws_url || latestSession.ws_url !== wsUrl) {
              return;
            }
            if (wsRef.current && wsRef.current.readyState <= WebSocket.OPEN) {
              return;
            }

            // Wait for fonts to load before connecting
            await ensureFontsLoaded();

            try {
                const ws = new WebSocket(wsUrl);
                ws.binaryType = 'arraybuffer';
                wsRef.current = ws;
                lastWsUrlRef.current = wsUrl; // Track initial ws_url

                ws.onopen = () => {
                    if (!isMountedRef.current) {
                        ws.close();
                        return;
                    }
                  reconnectingRef.current = false;

                    // SECURITY: Send authentication token as first message
                    const latestToken = latestSession.ws_token;
                    if (latestToken) {
                      ws.send(latestToken);
                    } else {
                        console.warn('No WebSocket token available - authentication may fail');
                    }

                    term.writeln(i18n.t('terminal.ssh.connected') + "\r\n");
                    // Initial resize using Wire Protocol v1
                    const frame = encodeResizeFrame(term.cols, term.rows);
                    ws.send(frame);
                    // Focus terminal after connection
                    term.focus();
                };

                ws.onmessage = (event) => {
                  if (!isMountedRef.current || wsRef.current !== ws) return;
                    const data = event.data;
                    if (data instanceof ArrayBuffer) {
                        // Parse Wire Protocol v1 frame: [Type: 1][Length: 4][Payload: n]
                        const view = new DataView(data);
                        if (data.byteLength < HEADER_SIZE) return;

                        const type = view.getUint8(0);
                        const length = view.getUint32(1, false); // big-endian

                        if (data.byteLength < HEADER_SIZE + length) return;

                        if (type === MSG_TYPE_DATA) {
                            const payload = new Uint8Array(data, HEADER_SIZE, length);
                            // P3: Queue data and batch writes with RAF for backpressure handling
                            pendingDataRef.current.push(payload);

                            // Schedule RAF flush if not already scheduled
                            if (rafIdRef.current === null) {
                                rafIdRef.current = requestAnimationFrame(() => {
                                    rafIdRef.current = null;
                                    if (!isMountedRef.current || !terminalRef.current) return;

                                    // Flush all pending data in one batch
                                    const pending = pendingDataRef.current;
                                    if (pending.length === 0) return;

                                    // Concatenate all chunks for single write
                                    const totalLength = pending.reduce((sum, chunk) => sum + chunk.length, 0);
                                    const combined = new Uint8Array(totalLength);
                                    let offset = 0;
                                    for (const chunk of pending) {
                                        combined.set(chunk, offset);
                                        offset += chunk.length;
                                    }

                                    pendingDataRef.current = [];
                                    terminalRef.current.write(combined);
                                });
                            }
                        } else if (type === MSG_TYPE_HEARTBEAT) {
                            // Heartbeat ping from server - respond with pong
                            if (length === 4) {
                                const seq = view.getUint32(HEADER_SIZE, false); // big-endian
                                const response = encodeHeartbeatFrame(seq);
                                ws.send(response);
                            }
                        } else if (type === MSG_TYPE_ERROR) {
                            // Error message from backend - display in terminal
                            const payload = new Uint8Array(data, HEADER_SIZE, length);
                            const decoder = new TextDecoder('utf-8');
                            const errorMsg = decoder.decode(payload);
                            term.writeln(`\r\n\x1b[31m${i18n.t('terminal.ssh.server_error', { error: errorMsg })}\x1b[0m`);
                        }
                    }
                };

                ws.onclose = () => {
                  if (!isMountedRef.current || wsRef.current !== ws) return;
                  if (!reconnectingRef.current) {
                    term.writeln(`\r\n\x1b[31m${i18n.t('terminal.ssh.connection_closed')}\x1b[0m`);
                  }
                };

                ws.onerror = (e) => {
                  if (!isMountedRef.current || wsRef.current !== ws) return;
                  term.writeln(`\r\n\x1b[31m${i18n.t('terminal.ssh.ws_error', { error: e })}\x1b[0m`);
                };



            } catch (e) {
                term.writeln(`\r\n\x1b[31m${i18n.t('terminal.ssh.ws_establish_failed', { error: e })}\x1b[0m`);
            }
        }, 100); // 100ms delay to let StrictMode unmount/remount complete
    } else {
         term.writeln(`\x1b[33m${i18n.t('terminal.ssh.no_ws_url')}\x1b[0m`);
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
    term.onData(data => {
        // Strict input interception: discard input when connection is down/reconnecting
        if (inputLockedRef.current) {
          console.log('[TerminalView] Input discarded - connection in standby mode');
          return; // Discard input silently
        }
        
        const ws = wsRef.current;
        if (ws && ws.readyState === WebSocket.OPEN) {
            // Encode as Wire Protocol v1 Data frame
            const encoder = new TextEncoder();
            const payload = encoder.encode(data);
            const frame = encodeDataFrame(payload);
            ws.send(frame);
        }
    });

    term.onResize((size) => {
        // Don't send resize when in Standby mode
        if (inputLockedRef.current) return;
        
        const ws = wsRef.current;
        if (ws && ws.readyState === WebSocket.OPEN) {
            // Send resize frame using Wire Protocol v1
            const frame = encodeResizeFrame(size.cols, size.rows);
            ws.send(frame);
            api.resizeSession(sessionId, size.cols, size.rows);
        }
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
        if (fitAddonRef.current && terminalRef.current && isMountedRef.current) {
          fitAddonRef.current.fit();
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
    setTimeout(() => {
        fitAddon.fit();
    }, 100);

    return () => {
      isMountedRef.current = false;
      
      // Unregister from Terminal Registry
      unregisterTerminalBuffer(effectivePaneId);
      
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

      if (wsConnectTimeout) {
          clearTimeout(wsConnectTimeout);
      }
      // Cancel pending RAF
      if (rafIdRef.current !== null) {
          cancelAnimationFrame(rafIdRef.current);
          rafIdRef.current = null;
      }
      pendingDataRef.current = [];
      if (wsRef.current) {
          wsRef.current.close();
          wsRef.current = null;
      }
        lastWsUrlRef.current = null;
      
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
      
      // Dispose ImageAddon to free memory (canvas + image storage)
      if (imageAddonRef.current) {
          try {
              imageAddonRef.current.dispose();
          } catch (e) {
              // Ignore errors during addon disposal
          }
          imageAddonRef.current = null;
      }
      
      // Finally dispose terminal
      term.dispose();
      terminalRef.current = null;
    };
  }, [sessionId]); // Only re-mount if sessionId changes absolutely

  // Listen for AI insert command events (only when this terminal is active and connected)
  useEffect(() => {
    if (!isActive) return;
    const currentSession = sessionRef.current;
    if (!currentSession || currentSession.state !== 'connected') return;

    const unlisten = listen<{ command: string }>('ai-insert-command', (event) => {
      if (!isMountedRef.current) return;
      if (inputLockedRef.current) return; // Don't insert during standby mode
      
      const ws = wsRef.current;
      if (!ws || ws.readyState !== WebSocket.OPEN) return;
      
      const { command } = event.payload;
      // Encode and send command to SSH terminal (without executing - user can review and press Enter)
      // For multiline commands, use bracketed paste mode to insert as one unit
      const encoder = new TextEncoder();
      
      // Check if command is multiline
      if (command.includes('\n')) {
        // Use bracketed paste mode: \x1b[200~ ... \x1b[201~
        // This tells the shell to treat the entire block as pasted text
        const bracketedPaste = `\x1b[200~${command}\x1b[201~`;
        const payload = encoder.encode(bracketedPaste);
        const frame = encodeDataFrame(payload);
        ws.send(frame);
      } else {
        const payload = encoder.encode(command);
        const frame = encodeDataFrame(payload);
        ws.send(frame);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [isActive, session?.state]);

  /**
   * Handle container click - focus terminal and update active pane
   */
  const handleContainerClick = () => {
    if (!searchOpen && !aiPanelOpen) {
      terminalRef.current?.focus();
      
      // Update active pane in Registry and notify parent
      setRegistryActivePaneId(effectivePaneId);
      touchTerminalEntry(effectivePaneId);
      onFocus?.(effectivePaneId);
    }
  };

  const currentTheme = themes[terminalSettings.theme] || themes.default;

  // === SearchAddon API for SearchBar ===
  const handleSearch = useCallback((query: string, options: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }) => {
    const searchAddon = searchAddonRef.current;
    if (!searchAddon || !query) {
      searchAddon?.clearDecorations();
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
  }, []);
  
  const handleFindNext = useCallback(() => {
    const query = currentSearchQueryRef.current;
    if (!query || !searchAddonRef.current) return;
    searchAddonRef.current.findNext(query, currentSearchOptionsRef.current);
  }, []);
  
  const handleFindPrevious = useCallback(() => {
    const query = currentSearchQueryRef.current;
    if (!query || !searchAddonRef.current) return;
    searchAddonRef.current.findPrevious(query, currentSearchOptionsRef.current);
  }, []);
  
  const handleCloseSearch = useCallback(() => {
    setSearchOpen(false);
    searchAddonRef.current?.clearDecorations();
    setSearchResults({ resultIndex: -1, resultCount: 0 });
    setDeepSearchState({ loading: false, matches: [], totalMatches: 0, durationMs: 0 });
    currentSearchQueryRef.current = '';
    terminalRef.current?.focus();
  }, []);
  
  // === Deep History Search ===
  const handleDeepSearch = useCallback(async (query: string, options: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }) => {
    if (!query.trim()) return;
    
    setDeepSearchState(prev => ({ ...prev, loading: true, error: undefined }));
    
    try {
      const result = await api.searchTerminal(sessionId, {
        query,
        case_sensitive: options.caseSensitive || false,
        regex: options.regex || false,
        whole_word: options.wholeWord || false,
      });
      
      setDeepSearchState({
        loading: false,
        matches: result.matches,
        totalMatches: result.total_matches,
        durationMs: result.duration_ms,
        error: result.error,
      });
    } catch (err) {
      setDeepSearchState({
        loading: false,
        matches: [],
        totalMatches: 0,
        durationMs: 0,
        error: err instanceof Error ? err.message : 'Search failed',
      });
    }
  }, [sessionId]);
  
  // === Jump to search match from deep history ===
  const handleJumpToMatch = useCallback(async (match: SearchMatch) => {
    const term = terminalRef.current;
    if (!term) return;
    
    const CONTEXT_LINES = 5;
    const ORANGE = '\x1b[38;2;234;88;12m';
    const YELLOW_BG = '\x1b[48;2;90;74;0m';
    const RED = '\x1b[31m';
    const RESET = '\x1b[0m';
    
    // Helper: highlight matched text within a line
    const highlightMatch = (text: string, matchedText: string): string => {
      const idx = text.indexOf(matchedText);
      if (idx === -1) return YELLOW_BG + text + RESET;
      return (
        text.slice(0, idx) +
        YELLOW_BG + matchedText + RESET +
        text.slice(idx + matchedText.length)
      );
    };
    
    try {
      // Fetch context around the match line from backend
      const lines = await api.scrollToLine(sessionId, match.line_number, CONTEXT_LINES);
      
      if (lines.length === 0) {
        // Buffer might have been completely cleared
        term.writeln(`\r\n${ORANGE}━━━ ${i18n.t('terminal.ssh.history_match', { line: match.line_number + 1 })} ━━━${RESET}`);
        term.writeln(`${RED}${i18n.t('terminal.ssh.buffer_empty')}${RESET}`);
        term.writeln(highlightMatch(match.line_content, match.matched_text));
        term.writeln(`${ORANGE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}\r\n`);
        term.scrollToBottom();
        return;
      }
      
      // Calculate which line in the returned array should be the match
      // scrollToLine returns: [line_number - context ... line_number ... line_number + context]
      const startLineInBuffer = match.line_number - CONTEXT_LINES;
      const targetIndexInResult = match.line_number - Math.max(0, startLineInBuffer);
      
      // Validate: check if the target line still contains the matched text
      const targetLine = lines[Math.min(targetIndexInResult, lines.length - 1)];
      const isStillValid = targetLine && targetLine.text.includes(match.matched_text);
      
      // Write header
      term.writeln(`\r\n${ORANGE}━━━ ${i18n.t('terminal.ssh.history_match', { line: match.line_number + 1 })} ━━━${RESET}`);
      
      if (!isStillValid) {
        // Buffer has rotated - the line at this index is no longer the same
        term.writeln(`${RED}${i18n.t('terminal.ssh.buffer_rotated')}${RESET}`);
        term.writeln(`${RED}${i18n.t('terminal.ssh.cached_match')}${RESET} ${highlightMatch(match.line_content, match.matched_text)}`);
        term.writeln(`${RED}${i18n.t('terminal.ssh.current_line', { index: match.line_number })}${RESET} ${targetLine?.text || '(empty)'}`);
        term.writeln(`${ORANGE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}\r\n`);
        term.scrollToBottom();
        return;
      }
      
      // Valid match - show context with highlighting
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        const actualLineNum = Math.max(0, startLineInBuffer) + i;
        const isMatchLine = actualLineNum === match.line_number;
        
        if (isMatchLine) {
          // Highlight the matched text within the line
          term.writeln(highlightMatch(line.text, match.matched_text));
        } else {
          term.writeln(line.text);
        }
      }
      
      term.writeln(`${ORANGE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}\r\n`);
      term.scrollToBottom();
      
    } catch (err) {
      console.error('Failed to fetch line context:', err);
      // Fallback: show the cached match from search results
      term.writeln(`\r\n${ORANGE}━━━ ${i18n.t('terminal.ssh.history_match', { line: match.line_number + 1 })} ━━━${RESET}`);
      term.writeln(`${RED}${i18n.t('terminal.ssh.fetch_context_failed')}${RESET}`);
      term.writeln(highlightMatch(match.line_content, match.matched_text));
      term.writeln(`${ORANGE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}\r\n`);
      term.scrollToBottom();
    }
  }, [sessionId]);
  
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
  
  // Insert text at cursor
  const handleAiInsert = useCallback((text: string) => {
    const ws = wsRef.current;
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    
    // Send text as if user typed it (but don't execute - let user review)
    const encoder = new TextEncoder();
    const payload = encoder.encode(text);
    const frame = encodeDataFrame(payload);
    ws.send(frame);
  }, []);
  
  // Execute command (insert + enter)
  const handleAiExecute = useCallback((command: string) => {
    const ws = wsRef.current;
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    
    // Send command followed by newline
    const encoder = new TextEncoder();
    const payload = encoder.encode(command + '\n');
    const frame = encodeDataFrame(payload);
    ws.send(frame);
  }, []);
  
  const handleCloseAiPanel = useCallback(() => {
    setAiPanelOpen(false);
    terminalRef.current?.focus();
  }, []);

  // Paste protection: handle pending paste confirm
  const handlePasteConfirm = useCallback(() => {
    if (pendingPaste) {
      const ws = wsRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) {
        const encoder = new TextEncoder();
        const payload = encoder.encode(pendingPaste);
        const frame = encodeDataFrame(payload);
        ws.send(frame);
      }
    }
    setPendingPaste(null);
    terminalRef.current?.focus();
  }, [pendingPaste]);

  const handlePasteCancel = useCallback(() => {
    setPendingPaste(null);
    terminalRef.current?.focus();
  }, []);

  // Paste protection: intercept paste events
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !terminalSettings.pasteProtection) return;

    const handlePaste = (e: ClipboardEvent) => {
      const text = e.clipboardData?.getData('text');
      if (!text) return;

      // Skip if input is locked (reconnecting)
      if (inputLockedRef.current) return;

      // Check if paste needs confirmation
      if (shouldConfirmPaste(text)) {
        e.preventDefault();
        e.stopPropagation();
        setPendingPaste(text);
      }
      // If not multi-line, let xterm.js handle normally
    };

    container.addEventListener('paste', handlePaste, { capture: true });
    return () => container.removeEventListener('paste', handlePaste, { capture: true });
  }, [terminalSettings.pasteProtection]);
  
  // Use unified terminal keyboard shortcuts
  // Only handles shortcuts when this terminal is active
  useTerminalViewShortcuts(
    isActive,
    searchOpen || aiPanelOpen,
    {
      onOpenSearch: () => setSearchOpen(true),
      onCloseSearch: handleCloseSearch,
      onOpenAiPanel: () => {
        // Check if AI is enabled in settings
        const { settings } = useSettingsStore.getState();
        if (settings.ai.enabled) {
          setAiPanelOpen(true);
        }
      },
      onCloseAiPanel: handleCloseAiPanel,
      onFocusTerminal: () => terminalRef.current?.focus(),
      searchOpen,
      aiPanelOpen,
    }
  );
  
  return (
    <div 
      className="terminal-container h-full w-full overflow-hidden relative" 
      style={{ 
        padding: '4px',
        backgroundColor: currentTheme.background 
      }}
      onClick={handleContainerClick}
    >
       <div 
         ref={containerRef} 
         className="h-full w-full"
         style={{
           contain: 'strict',
           isolation: 'isolate'
         }}
       />
       
       {/* Input Lock Overlay - shown during reconnection */}
       {inputLocked && (
         <div className="absolute inset-0 bg-black/40 backdrop-blur-sm flex items-center justify-center z-10">
           <div className="bg-zinc-900/95 border border-zinc-700 rounded-lg px-6 py-4 flex flex-col items-center gap-3 shadow-xl">
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
             <div className="text-xs text-zinc-400 text-center">
               {t('terminal.standby.input_locked')}
             </div>
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
       
       {/* AI Inline Panel */}
       <AiInlinePanel
         isOpen={aiPanelOpen}
         onClose={handleCloseAiPanel}
         getSelection={getTerminalSelection}
         getVisibleBuffer={getVisibleBuffer}
         onInsert={handleAiInsert}
         onExecute={handleAiExecute}
       />
    </div>
  );
};
