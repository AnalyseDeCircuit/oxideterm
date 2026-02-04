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
import { themes } from '../../lib/themes';
import { useTerminalViewShortcuts } from '../../hooks/useTerminalKeyboard';
import { SearchBar } from './SearchBar';
import { AiInlinePanel } from './AiInlinePanel';
import { PasteConfirmOverlay, shouldConfirmPaste } from './PasteConfirmOverlay';
import { terminalLinkHandler } from '../../lib/safeUrl';
import { listen } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { 
  registerTerminalBuffer, 
  unregisterTerminalBuffer,
  setActivePaneId as setRegistryActivePaneId,
  touchTerminalEntry 
} from '../../lib/terminalRegistry';

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
  const searchAddonRef = useRef<SearchAddon | null>(null);
  const imageAddonRef = useRef<ImageAddon | null>(null);
  const rendererAddonRef = useRef<{ dispose: () => void } | null>(null);
  
  // Get tab ID for this terminal (used for registry validation)
  // Use prop if provided, otherwise look up from store
  const storeTabId = useAppStore((state) => 
    state.tabs.find(t => t.type === 'local_terminal' && t.sessionId === sessionId)?.id
  );
  const effectiveTabId = propTabId || storeTabId || '';
  
  // Effective pane ID: use provided paneId or fall back to sessionId
  const effectivePaneId = paneId || sessionId;
  
  const isMountedRef = useRef(true);
  const [searchOpen, setSearchOpen] = useState(false);
  const [aiPanelOpen, setAiPanelOpen] = useState(false);
  const [isRunning, setIsRunning] = useState(true);
  
  // Paste protection state
  const [pendingPaste, setPendingPaste] = useState<string | null>(null);
  
  // Search state
  const [searchResults, setSearchResults] = useState<{ resultIndex: number; resultCount: number }>({ 
    resultIndex: -1, 
    resultCount: 0 
  });
  const currentSearchQueryRef = useRef<string>('');
  const currentSearchOptionsRef = useRef<ISearchOptions | undefined>(undefined);

  // RAF buffering for high-frequency PTY data (prevents search index jumping)
  // This batches rapid data events into single writes, reducing buffer churn
  const pendingDataRef = useRef<Uint8Array[]>([]);
  const rafIdRef = useRef<number | null>(null);
  
  // Search pause mechanism: pause search updates during heavy output bursts
  // This prevents the "1->2->3->1" cycling when buffer changes rapidly
  const searchPausedRef = useRef(false);
  const outputThrottleRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const { writeTerminal, resizeTerminal, getTerminal, updateTerminalState } = useLocalTerminalStore();
  const terminalInfo = getTerminal(sessionId);

  // Get terminal settings
  const terminalSettings = useSettingsStore((state) => state.settings.terminal);

  // Import font utility for consistent font stack
  const getFontFamily = (val: string): string => {
    // Use the same font stack as fontUtils.ts for consistency
    switch(val) {
      case 'jetbrains':
        return '"JetBrainsMono Nerd Font", "JetBrainsMono Nerd Font Mono", "JetBrains Mono NF", "JetBrains Mono", monospace';
      case 'meslo':
        return '"MesloLGM Nerd Font", "MesloLGM Nerd Font Mono", "MesloLGM NF", "Meslo LG M", monospace';
      case 'cascadia':
        return '"Cascadia Code NF", "Cascadia Mono NF", "Cascadia Code", "Cascadia Mono", monospace';
      case 'firacode':
        return '"FiraCode Nerd Font", "FiraCode Nerd Font Mono", "Fira Code", monospace';
      case 'menlo':
        return 'Menlo, Monaco, "Courier New", monospace';
      case 'consolas':
        return 'Consolas, "Courier New", monospace';
      case 'courier':
        return '"Courier New", Courier, monospace';
      case 'monospace':
        return 'monospace';
      default:
        return '"JetBrainsMono Nerd Font", "JetBrainsMono Nerd Font Mono", "JetBrains Mono NF", "JetBrains Mono", monospace';
    }
  };

  // Subscribe to terminal settings changes
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
        
        const themeConfig = themes[terminal.theme] || themes.default;
        term.options.theme = themeConfig;
        
        term.refresh(0, term.rows - 1);
        fitAddonRef.current?.fit();
      }
    );
    return unsubscribe;
  }, []);

  // Focus terminal when active
  useEffect(() => {
    if (isActive && terminalRef.current && !searchOpen && !aiPanelOpen) {
      const focusTimeout = setTimeout(() => {
        if (!searchOpen && !aiPanelOpen) {
          terminalRef.current?.focus();
        }
        fitAddonRef.current?.fit();
      }, 50);
      return () => clearTimeout(focusTimeout);
    }
  }, [isActive, searchOpen, aiPanelOpen]);

  // Initialize terminal
  useEffect(() => {
    if (!containerRef.current || terminalRef.current) return;
    
    isMountedRef.current = true;

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
    const imageAddon = new ImageAddon({
      enableSizeReports: true,
      pixelLimit: 16777216,
      storageLimit: 128,
      showPlaceholder: true,
      sixelSupport: true,
      iipSupport: true,
    });
    
    term.loadAddon(fitAddon);
    term.loadAddon(webLinksAddon);
    term.loadAddon(searchAddon);
    term.loadAddon(imageAddon);
    
    // Unicode11Addon for proper Nerd Font icons and CJK wide character rendering
    // Required for Oh My Posh, Starship, and other modern prompts
    const unicode11Addon = new Unicode11Addon();
    term.loadAddon(unicode11Addon);
    term.unicode.activeVersion = '11';
    
    searchAddon.onDidChangeResults((e) => {
      // Only update if there's an active search query AND not paused due to high output
      // searchPausedRef prevents the "1->2->3->1" cycling during rapid buffer changes
      if (currentSearchQueryRef.current && !searchPausedRef.current) {
        setSearchResults({ resultIndex: e.resultIndex, resultCount: e.resultCount });
      }
    });
    
    searchAddonRef.current = searchAddon;
    imageAddonRef.current = imageAddon;
    
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
        try {
          const webglAddon = new WebglAddon();
          webglAddon.onContextLoss(async () => {
            webglAddon.dispose();
            const canvasAddon = await loadCanvasAddon();
            rendererAddonRef.current = canvasAddon;
          });
          term.loadAddon(webglAddon);
          rendererAddonRef.current = webglAddon;
        } catch (e) {
          const canvasAddon = await loadCanvasAddon();
          rendererAddonRef.current = canvasAddon;
        }
      }
    };

    term.open(containerRef.current);
    terminalRef.current = term;
    fitAddonRef.current = fitAddon;

    // Register buffer getter for AI context capture
    // Now uses paneId as key (for split pane support)
    registerTerminalBuffer(
      effectivePaneId,
      effectiveTabId,
      sessionId,
      'local_terminal',
      () => {
        const buffer = term.buffer.active;
        const lines: string[] = [];
        // Get visible lines plus some scrollback
        const startRow = Math.max(0, buffer.baseY);
        const endRow = buffer.baseY + term.rows;
        for (let i = startRow; i < endRow; i++) {
          const line = buffer.getLine(i);
          if (line) {
            lines.push(line.translateToString(true));
          }
        }
        return lines.join('\n');
      }
    );

    // Initial fit
    setTimeout(() => {
      fitAddon.fit();
      
      const dims = fitAddon.proposeDimensions();
      if (dims && Number.isFinite(dims.cols) && Number.isFinite(dims.rows) && dims.cols > 0 && dims.rows > 0) {
        resizeTerminal(sessionId, dims.cols, dims.rows);
      }
    }, 0);

    loadRenderer();

    // Handle terminal data input
    term.onData((data) => {
      if (!isRunning) return;
      const encoder = new TextEncoder();
      const bytes = encoder.encode(data);
      writeTerminal(sessionId, bytes);
    });

    // Handle terminal binary input (for special keys)
    term.onBinary((data) => {
      if (!isRunning) return;
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

    // Welcome message
    term.writeln(`\x1b[32m${t('terminal.local.title')}\x1b[0m`);
    term.writeln(t('terminal.local.shell', { shell: terminalInfo?.shell.label || t('terminal.local.shell_unknown') }));
    term.writeln('');

    return () => {
      isMountedRef.current = false;
      
      // Unregister buffer getter (using paneId, not sessionId)
      unregisterTerminalBuffer(effectivePaneId);
      
      // Remove focus listener
      if (termElement) {
        termElement.removeEventListener('focusin', handleTerminalFocus);
      }
      
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
      
      // Dispose image addon to free memory
      if (imageAddonRef.current) {
        try {
          imageAddonRef.current.dispose();
        } catch (e) {
          // Ignore errors during addon disposal
        }
        imageAddonRef.current = null;
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
  }, [sessionId]);

  // Listen for terminal data events from backend
  useEffect(() => {
    if (!terminalRef.current) return;

    const dataEventName = `local-terminal-data:${sessionId}`;
    const closedEventName = `local-terminal-closed:${sessionId}`;

    // Listen for data - use RAF batching to reduce search index jumping
    // Track mounted state and listener cleanup functions
    let mounted = true;
    let unlistenDataFn: (() => void) | null = null;
    let unlistenClosedFn: (() => void) | null = null;

    // Rust PTY sends high-frequency small packets; batching reduces buffer churn
    listen<{ sessionId: string; data: number[] }>(dataEventName, (event) => {
      if (!mounted || !isMountedRef.current || !terminalRef.current) return;
      const data = new Uint8Array(event.payload.data);
      
      // Queue data for RAF batch write
      pendingDataRef.current.push(data);
      
      if (rafIdRef.current === null) {
        rafIdRef.current = requestAnimationFrame(() => {
          rafIdRef.current = null;
          if (!mounted || !isMountedRef.current || !terminalRef.current) return;
          
          const pending = pendingDataRef.current;
          if (pending.length === 0) return;
          
          // Concatenate all chunks for single write (reduces xterm buffer mutations)
          const totalLength = pending.reduce((sum, chunk) => sum + chunk.length, 0);
          const combined = new Uint8Array(totalLength);
          let offset = 0;
          for (const chunk of pending) {
            combined.set(chunk, offset);
            offset += chunk.length;
          }
          
          pendingDataRef.current = [];
          terminalRef.current.write(combined);
          
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
        });
      }
    }).then((fn) => {
      if (mounted) {
        unlistenDataFn = fn;
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
      // Clean up RAF and throttle timers
      if (rafIdRef.current !== null) {
        cancelAnimationFrame(rafIdRef.current);
        rafIdRef.current = null;
      }
      if (outputThrottleRef.current) {
        clearTimeout(outputThrottleRef.current);
        outputThrottleRef.current = null;
      }
      pendingDataRef.current = [];
      searchPausedRef.current = false;
      
      unlistenDataFn?.();
      unlistenClosedFn?.();
    };
  }, [sessionId, updateTerminalState]);

  // Listen for AI insert command events (only when this terminal is active)
  useEffect(() => {
    if (!isActive || !isRunning) return;

    let mounted = true;
    let unlistenFn: (() => void) | null = null;

    listen<{ command: string }>('ai-insert-command', (event) => {
      if (!mounted || !isMountedRef.current || !isRunning) return;
      
      const { command } = event.payload;
      // Write command to terminal (without executing - user can review and press Enter)
      // For multiline commands, we use bracketed paste mode markers if terminal supports it
      // This ensures the entire command is pasted as one unit
      const encoder = new TextEncoder();
      
      // Check if command is multiline
      if (command.includes('\n')) {
        // Use bracketed paste mode: \x1b[200~ ... \x1b[201~
        // This tells the shell to treat the entire block as pasted text
        const bracketedPaste = `\x1b[200~${command}\x1b[201~`;
        const bytes = encoder.encode(bracketedPaste);
        writeTerminal(sessionId, bytes);
      } else {
        const bytes = encoder.encode(command);
        writeTerminal(sessionId, bytes);
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
  }, [sessionId, isActive, isRunning, writeTerminal]);

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
        if (!fitAddon || !isMountedRef.current) return;
        
        fitAddon.fit();
        
        const dims = fitAddon.proposeDimensions();
        if (dims && Number.isFinite(dims.cols) && Number.isFinite(dims.rows) && dims.cols > 0 && dims.rows > 0) {
          resizeTerminal(sessionId, dims.cols, dims.rows);
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
  }, [sessionId, resizeTerminal]);

  // Search close handler
  const handleSearchClose = useCallback(() => {
    setSearchOpen(false);
    if (searchAddonRef.current) {
      searchAddonRef.current.clearDecorations();
    }
    // Clear search state and pause mechanism
    currentSearchQueryRef.current = '';
    searchPausedRef.current = false;
    if (outputThrottleRef.current) {
      clearTimeout(outputThrottleRef.current);
      outputThrottleRef.current = null;
    }
    setSearchResults({ resultIndex: -1, resultCount: 0 });
    terminalRef.current?.focus();
  }, []);

  // AI Panel close handler
  const handleCloseAiPanel = useCallback(() => {
    setAiPanelOpen(false);
    terminalRef.current?.focus();
  }, []);

  // Use unified terminal keyboard shortcuts
  // Only handles shortcuts when this terminal is active
  useTerminalViewShortcuts(
    isActive,
    searchOpen || aiPanelOpen,
    {
      onOpenSearch: () => setSearchOpen(true),
      onCloseSearch: handleSearchClose,
      onOpenAiPanel: () => setAiPanelOpen(true),
      onCloseAiPanel: handleCloseAiPanel,
      onFocusTerminal: () => terminalRef.current?.focus(),
      searchOpen,
      aiPanelOpen,
    }
  );

  // Search handlers
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

  const handleSearchNext = useCallback(() => {
    if (!searchAddonRef.current || !currentSearchQueryRef.current) return;
    searchAddonRef.current.findNext(currentSearchQueryRef.current, currentSearchOptionsRef.current);
  }, []);

  const handleSearchPrevious = useCallback(() => {
    if (!searchAddonRef.current || !currentSearchQueryRef.current) return;
    searchAddonRef.current.findPrevious(currentSearchQueryRef.current, currentSearchOptionsRef.current);
  }, []);

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
    const encoder = new TextEncoder();
    const bytes = encoder.encode(text);
    writeTerminal(sessionId, bytes);
  }, [sessionId, isRunning, writeTerminal]);

  // Handle AI execute (paste and send enter)
  const handleAiExecute = useCallback((command: string) => {
    if (!terminalRef.current || !isRunning) return;
    const encoder = new TextEncoder();
    // Send command + newline
    const bytes = encoder.encode(command + '\n');
    writeTerminal(sessionId, bytes);
  }, [sessionId, isRunning, writeTerminal]);

  // Paste protection: handle pending paste confirm
  const handlePasteConfirm = useCallback(() => {
    if (pendingPaste && isRunning) {
      const encoder = new TextEncoder();
      const bytes = encoder.encode(pendingPaste);
      writeTerminal(sessionId, bytes);
    }
    setPendingPaste(null);
    terminalRef.current?.focus();
  }, [pendingPaste, sessionId, isRunning, writeTerminal]);

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
      if (!text || !isRunning) return;

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
  }, [terminalSettings.pasteProtection, isRunning]);

  /**
   * Handle container click - focus terminal and update active pane
   */
  const handleContainerClick = useCallback(() => {
    if (!searchOpen && !aiPanelOpen) {
      terminalRef.current?.focus();
      
      // Update active pane in Registry and notify parent
      setRegistryActivePaneId(effectivePaneId);
      touchTerminalEntry(effectivePaneId);
      onFocus?.(effectivePaneId);
    }
  }, [searchOpen, aiPanelOpen, effectivePaneId, onFocus]);

  return (
    <div 
      className="relative flex-1 w-full h-full flex flex-col"
      onClick={handleContainerClick}
    >
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
      
      {/* AI Inline Panel */}
      <AiInlinePanel
        isOpen={aiPanelOpen}
        onClose={handleCloseAiPanel}
        getSelection={getTerminalSelection}
        getVisibleBuffer={getVisibleBuffer}
        onInsert={handleAiInsert}
        onExecute={handleAiExecute}
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
        style={{ minHeight: 0 }}
      />
      
      {/* Status overlay when not running */}
      {!isRunning && (
        <div className="absolute bottom-4 right-4 bg-zinc-800/80 text-zinc-400 text-xs px-2 py-1 rounded">
          {t('terminal.local.session_ended')}
        </div>
      )}
    </div>
  );
};
