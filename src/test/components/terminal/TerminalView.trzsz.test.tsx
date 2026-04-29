import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TerminalView } from '@/components/terminal/TerminalView';
import { TrzszController } from '@/lib/terminal/trzsz/controller';
import { encodeDataFrame } from '@/lib/wireProtocol';

const eventListeners = vi.hoisted(() => new Map<string, Set<(event: { payload: unknown }) => void>>());

const appStoreState = vi.hoisted(() => ({
  sessions: new Map<string, any>(),
  purgeTerminalSession: vi.fn(),
}));

const settingsState = vi.hoisted(() => ({
  settings: {
    terminal: {
      adaptiveRenderer: 'off',
      theme: 'default',
      fontSize: 14,
      fontFamily: 'monospace',
      letterSpacing: 0,
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: 'block',
      allowTransparency: true,
      scrollback: 1000,
      rightClickSelectsWord: false,
      macOptionIsMeta: false,
      backgroundEnabled: false,
      backgroundEnabledTabs: ['terminal'],
      backgroundImage: '',
      backgroundBlur: 0,
      copyOnSelect: false,
      middleClickPaste: false,
      pasteProtection: 'off',
      selectionRequiresShift: false,
      autoScrollOnOutput: true,
      highlightRules: [],
      inBandTransfer: {
        enabled: true,
        provider: 'trzsz',
        allowDirectory: true,
        maxChunkBytes: 1024 * 1024,
        maxFileCount: 1024,
        maxTotalBytes: 10 * 1024 * 1024 * 1024,
      },
    },
    ai: {
      enabled: true,
      contextVisibleLines: 50,
    },
  },
}));

const terminalRegistryMocks = vi.hoisted(() => ({
  registerTerminalBuffer: vi.fn(),
  unregisterTerminalBuffer: vi.fn(),
  setActivePaneId: vi.fn(),
  touchTerminalEntry: vi.fn(),
  notifyTerminalOutput: vi.fn(),
  broadcastToTargets: vi.fn(),
}));

const shortcutState = vi.hoisted(() => ({
  handlers: null as null | {
    onOpenAiPanel: () => void;
    onCloseAiPanel: () => void;
  },
}));

const apiMocks = vi.hoisted(() => ({
  getTrzszCapabilities: vi.fn().mockResolvedValue({
    status: 'unavailable',
    reason: 'command-missing',
  }),
  trzszPrepareDownloadRoot: vi.fn(async (_ownerId: string, rootPath: string) => ({ rootPath })),
  getTerminalHistoryStatus: vi.fn().mockResolvedValue({ available: false }),
  cancelTerminalHistorySearch: vi.fn().mockResolvedValue(undefined),
  recreateTerminalPty: vi.fn(),
  getBufferStats: vi.fn(),
  getScrollBuffer: vi.fn(),
  startTerminalHistorySearch: vi.fn(),
  getArchivedHistoryExcerpt: vi.fn(),
  scrollToLine: vi.fn(),
}));

const dialogMocks = vi.hoisted(() => ({
  chooseSaveRoot: vi.fn(),
}));

const recordingMocks = vi.hoisted(() => ({
  startRecording: vi.fn(),
  feedOutput: vi.fn(),
  feedInput: vi.fn(),
  feedResize: vi.fn(),
  handleRecordingStop: vi.fn(),
  handleRecordingDiscard: vi.fn(),
  recorderRef: { current: null as null | { recordOutput: (data: Uint8Array) => void } },
}));

const adaptiveRendererMock = vi.hoisted(() => ({
  scheduleWrite: vi.fn(),
  notifyUserInput: vi.fn(),
  getStats: vi.fn(() => ({ fps: 60 })),
}));

type MockDisposable = { dispose: ReturnType<typeof vi.fn> };

const terminalInstances = vi.hoisted(() => [] as any[]);

class MockWebSocket {
  static OPEN = 1;

  readyState = MockWebSocket.OPEN;
  binaryType = 'arraybuffer';
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  send = vi.fn();
  close = vi.fn();

  constructor(public readonly url: string) {
    mockSockets.push(this);
    queueMicrotask(() => {
      this.onopen?.(new Event('open'));
    });
  }

  emitMessage(data: ArrayBuffer | Uint8Array) {
    const payload = data instanceof Uint8Array ? data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength) : data;
    this.onmessage?.(new MessageEvent('message', { data: payload }));
  }
}

const mockSockets = vi.hoisted(() => [] as MockWebSocket[]);

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
  initReactI18next: {
    type: '3rdParty',
    init: () => undefined,
  },
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((eventName: string, callback: (event: { payload: unknown }) => void) => {
    const listeners = eventListeners.get(eventName) ?? new Set();
    listeners.add(callback);
    eventListeners.set(eventName, listeners);
    return Promise.resolve(() => {
      listeners.delete(callback);
      if (listeners.size === 0) {
        eventListeners.delete(eventName);
      }
    });
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => path,
}));

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    element = document.createElement('div');
    parser = {
      registerOscHandler: vi.fn(() => ({ dispose: vi.fn() })),
    };
    unicode = { activeVersion: '11' };
    buffer = {
      active: {
        length: 0,
        baseY: 0,
        cursorX: 0,
        cursorY: 0,
        getLine: vi.fn(() => null),
      },
    };
    modes = { mouseTrackingMode: 'none' };
    onDataHandler: ((data: string) => void) | null = null;
    onBinaryHandler: ((data: string) => void) | null = null;
    onResizeHandler: ((size: { cols: number; rows: number }) => void) | null = null;
    onDataDispose = vi.fn(() => {
      this.onDataHandler = null;
    });
    onBinaryDispose = vi.fn(() => {
      this.onBinaryHandler = null;
    });
    onResizeDispose = vi.fn(() => {
      this.onResizeHandler = null;
    });
    loadAddon = vi.fn();
    open = vi.fn();
    focus = vi.fn();
    write = vi.fn();
    writeln = vi.fn();
    dispose = vi.fn();
    clear = vi.fn();
    refresh = vi.fn();
    scrollToBottom = vi.fn();
    getSelection = vi.fn(() => '');
    hasSelection = vi.fn(() => false);
    attachCustomKeyEventHandler = vi.fn();
    onWriteParsed = vi.fn(() => ({ dispose: vi.fn() }));

    constructor() {
      terminalInstances.push(this);
    }

    onData(handler: (data: string) => void): MockDisposable {
      this.onDataHandler = handler;
      return { dispose: this.onDataDispose };
    }

    onBinary(handler: (data: string) => void): MockDisposable {
      this.onBinaryHandler = handler;
      return { dispose: this.onBinaryDispose };
    }

    onResize(handler: (size: { cols: number; rows: number }) => void): MockDisposable {
      this.onResizeHandler = handler;
      return { dispose: this.onResizeDispose };
    }
  },
}));

vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class {
    fit = vi.fn();
    dispose = vi.fn();
  },
}));

vi.mock('@xterm/addon-webgl', () => ({
  WebglAddon: class {
    dispose = vi.fn();
    onContextLoss = vi.fn(() => ({ dispose: vi.fn() }));
  },
}));

vi.mock('@xterm/addon-web-links', () => ({
  WebLinksAddon: class {
    dispose = vi.fn();
  },
}));

vi.mock('@xterm/addon-search', () => ({
  SearchAddon: class {
    clearDecorations = vi.fn();
    dispose = vi.fn();
    findNext = vi.fn();
    findPrevious = vi.fn();
    onDidChangeResults = vi.fn(() => ({ dispose: vi.fn() }));
  },
}));

vi.mock('@xterm/addon-image', () => ({
  ImageAddon: class {
    dispose = vi.fn();
  },
}));

vi.mock('@xterm/addon-unicode11', () => ({
  Unicode11Addon: class {
    dispose = vi.fn();
  },
}));

vi.mock('lucide-react', () => ({
  Lock: () => null,
  Loader2: () => null,
  RefreshCw: () => null,
  AlertTriangle: () => null,
}));

vi.mock('@/store/appStore', () => ({
  useAppStore: Object.assign(
    (selector: (state: typeof appStoreState) => unknown) => selector(appStoreState),
    {
      getState: () => appStoreState,
      setState: (updater: (state: typeof appStoreState) => Partial<typeof appStoreState> | void) => {
        const partial = updater(appStoreState);
        if (partial) {
          Object.assign(appStoreState, partial);
        }
      },
    },
  ),
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: Object.assign(
    (selector?: (state: typeof settingsState) => unknown) => selector ? selector(settingsState) : settingsState,
    {
      getState: () => settingsState,
      subscribe: vi.fn(() => () => undefined),
    },
  ),
}));

vi.mock('@/store/sessionTreeStore', () => ({
  useSessionTreeStore: (selector: (state: { terminalNodeMap: Map<string, string> }) => unknown) => selector({
    terminalNodeMap: new Map([['session-1', 'node-1']]),
  }),
}));

vi.mock('@/store/reconnectOrchestratorStore', () => ({
  useReconnectOrchestratorStore: {
    getState: () => ({
      scheduleReconnect: vi.fn(),
    }),
  },
}));

vi.mock('@/store/broadcastStore', () => ({
  useBroadcastStore: {
    getState: () => ({
      enabled: false,
      selectedTargetIds: [],
    }),
  },
}));

vi.mock('@/store/ideStore', () => ({
  triggerGitRefresh: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  api: apiMocks,
}));

vi.mock('@/lib/terminal/trzsz/dialogs', () => ({
  chooseSendEntries: vi.fn(),
  chooseSaveRoot: dialogMocks.chooseSaveRoot,
}));

vi.mock('@/lib/themes', () => ({
  getTerminalTheme: vi.fn(() => ({ background: '#000000', foreground: '#ffffff' })),
}));

vi.mock('@/lib/fontFamily', () => ({
  getFontFamily: vi.fn(() => 'monospace'),
}));

vi.mock('@/hooks/useTerminalKeyboard', () => ({
  useTerminalViewShortcuts: vi.fn((_: boolean, __: boolean, handlers: typeof shortcutState.handlers) => {
    shortcutState.handlers = handlers;
  }),
}));

vi.mock('@/components/terminal/SearchBar', () => ({
  SearchBar: () => null,
}));

vi.mock('@/components/terminal/AiInlinePanel', () => ({
  AiInlinePanel: ({ isOpen, onInsert, onExecute }: { isOpen: boolean; onInsert: (text: string) => void; onExecute: (text: string) => void }) => {
    if (!isOpen) {
      return null;
    }

    return (
      <div>
        <button type="button" onClick={() => onInsert('ai-insert-inline')}>insert-ai</button>
        <button type="button" onClick={() => onExecute('ai-execute-inline')}>execute-ai</button>
      </div>
    );
  },
}));

vi.mock('@/components/terminal/PasteConfirmOverlay', () => ({
  PasteConfirmOverlay: () => null,
}));

vi.mock('@/lib/terminalPaste', () => ({
  getProtectedPasteDecision: vi.fn(() => 'allow'),
}));

vi.mock('@/lib/safeUrl', () => ({
  terminalLinkHandler: vi.fn(),
}));

vi.mock('@/lib/terminalRegistry', () => ({
  registerTerminalBuffer: terminalRegistryMocks.registerTerminalBuffer,
  unregisterTerminalBuffer: terminalRegistryMocks.unregisterTerminalBuffer,
  setActivePaneId: terminalRegistryMocks.setActivePaneId,
  touchTerminalEntry: terminalRegistryMocks.touchTerminalEntry,
  notifyTerminalOutput: terminalRegistryMocks.notifyTerminalOutput,
  broadcastToTargets: terminalRegistryMocks.broadcastToTargets,
}));

vi.mock('@/lib/fontLoader', () => ({
  onMapleRegularLoaded: vi.fn(() => () => undefined),
  ensureCJKFallback: vi.fn(),
  prepareTerminalFontForOpen: vi.fn(() => Promise.resolve()),
}));

vi.mock('@/lib/plugin/pluginTerminalHooks', () => ({
  runInputPipeline: vi.fn((input: string) => input),
  runOutputPipeline: vi.fn((payload: Uint8Array) => payload),
}));

vi.mock('@/lib/terminalHelpers', () => ({
  hexToRgba: vi.fn(() => 'rgba(0, 0, 0, 1)'),
  getBackgroundFitStyles: vi.fn(() => ({ backgroundSize: 'cover' })),
  getWebglRendererInfo: vi.fn(() => null),
  logWebglRendererInfo: vi.fn(),
  isLowEndGPU: vi.fn(() => false),
  forceViewportTransparent: vi.fn(),
  clearViewportTransparent: vi.fn(),
  isTerminalContainerRenderable: vi.fn(() => true),
  resolveTerminalDimensions: vi.fn(() => ({ cols: 80, rows: 24 })),
  shouldAutoFocusTerminal: vi.fn(() => true),
  shouldFocusTerminalFromClick: vi.fn(() => true),
}));

vi.mock('@/lib/clipboardSupport', () => ({
  installTerminalClipboardSupport: vi.fn().mockResolvedValue({ dispose: vi.fn() }),
  readSystemClipboardText: vi.fn().mockResolvedValue(null),
}));

vi.mock('@/lib/terminalPasteShortcutGuard', () => ({
  armTerminalPasteShortcutSuppression: vi.fn(),
  createTerminalPasteShortcutSuppressionState: vi.fn(() => ({ pending: null })),
  markTerminalPasteShortcutHandled: vi.fn(),
  shouldSuppressTerminalPasteEvent: vi.fn(() => false),
  takeTerminalPasteShortcutFallback: vi.fn(() => null),
}));

vi.mock('@/hooks/useTerminalSmartCopy', () => ({
  attachTerminalSmartCopy: vi.fn(() => ({ dispose: vi.fn() })),
}));

vi.mock('@/hooks/useTerminalRecording', () => ({
  useTerminalRecording: vi.fn(() => ({
    ...recordingMocks,
    isRecording: false,
  })),
}));

vi.mock('@/hooks/useAdaptiveRenderer', () => ({
  useAdaptiveRenderer: vi.fn(() => adaptiveRendererMock),
}));

vi.mock('@/components/terminal/RecordingControls', () => ({
  RecordingControls: () => null,
}));

vi.mock('@/components/terminal/FpsOverlay', () => ({
  FpsOverlay: () => null,
}));

vi.mock('@/hooks/useToast', () => ({
  useToastStore: {
    getState: () => ({
      addToast: vi.fn(),
    }),
  },
}));

vi.mock('@/lib/terminal/highlightEngine', () => ({
  HighlightEngine: class {
    updateRules = vi.fn();
    dispose = vi.fn();
  },
}));

vi.mock('@/lib/terminal/runtimeDisabledHighlightRules', () => ({
  applyRuntimeDisabledHighlightRules: vi.fn((_: Map<string, string>, rules: unknown[]) => rules),
  getHighlightRulesSignature: vi.fn(() => 'sig'),
  markRuntimeDisabledHighlightRules: vi.fn(),
}));

vi.mock('@/lib/terminalSelectionGesture', () => ({
  installShiftSelectionGuard: vi.fn(() => ({
    refresh: vi.fn(),
    dispose: vi.fn(),
  })),
}));

function emitEvent<T>(name: string, payload: T) {
  const listeners = eventListeners.get(name);
  listeners?.forEach((listener) => listener({ payload }));
}

function hasDataFrameWithTextPrefix(socket: MockWebSocket | undefined, prefix: string) {
  if (!socket) return false;
  return vi.mocked(socket.send).mock.calls.some(([payload]) => {
    if (!(payload instanceof Uint8Array) || payload[0] !== 0x00 || payload.length < 5) {
      return false;
    }

    const text = new TextDecoder().decode(payload.slice(5));
    return text.startsWith(prefix);
  });
}

function setConnectedSession(overrides: Record<string, unknown> = {}) {
  appStoreState.sessions = new Map([
    ['session-1', {
      id: 'session-1',
      state: 'connected',
      connectionId: 'connection-1',
      ws_url: 'ws://terminal-1',
      ws_token: 'token-1',
      title: 'Primary terminal',
      ...overrides,
    }],
  ]);
}

function setInBandTransferEnabled(enabled: boolean) {
  settingsState.settings.terminal.inBandTransfer = {
    ...settingsState.settings.terminal.inBandTransfer,
    enabled,
  };
}

describe('TerminalView trzsz Phase 1 wiring', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventListeners.clear();
    terminalInstances.length = 0;
    mockSockets.length = 0;
    shortcutState.handlers = null;
    setConnectedSession();
    setInBandTransferEnabled(true);
    dialogMocks.chooseSaveRoot.mockReset();

    Object.defineProperty(document, 'fonts', {
      configurable: true,
      value: {
        load: vi.fn().mockResolvedValue([]),
      },
    });

    vi.stubGlobal('WebSocket', MockWebSocket);
    vi.stubGlobal('ResizeObserver', class {
      observe() {}
      disconnect() {}
    });
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    });
    vi.stubGlobal('cancelAnimationFrame', vi.fn());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('routes binary input, server output and AI insert events through the controller', async () => {
    const processBinaryInputSpy = vi.spyOn(TrzszController.prototype, 'processBinaryInput');
    const processServerOutputSpy = vi.spyOn(TrzszController.prototype, 'processServerOutput');
    const sendTextInputSpy = vi.spyOn(TrzszController.prototype, 'sendTextInput');
    const sendExecuteInputSpy = vi.spyOn(TrzszController.prototype, 'sendExecuteInput');

    render(<TerminalView sessionId="session-1" />);

    await waitFor(() => {
      expect(apiMocks.getTrzszCapabilities).toHaveBeenCalledTimes(1);
      expect(terminalInstances).toHaveLength(1);
      expect(mockSockets).toHaveLength(1);
      expect(typeof mockSockets[0]?.onmessage).toBe('function');
    });

    act(() => {
      terminalInstances[0]?.onBinaryHandler?.('\u001bOA');
    });
    expect(processBinaryInputSpy).toHaveBeenCalledWith('\u001bOA');

    const frame = encodeDataFrame(new TextEncoder().encode('server-output'));
    act(() => {
      mockSockets[0]?.emitMessage(frame);
    });
    await waitFor(() => {
      expect(processServerOutputSpy).toHaveBeenCalled();
    });

    act(() => {
      emitEvent('ai-insert-command', { command: 'printf test' });
    });
    expect(sendTextInputSpy).toHaveBeenCalledWith('printf test');

    act(() => {
      shortcutState.handlers?.onOpenAiPanel();
    });
    fireEvent.click(screen.getByRole('button', { name: 'execute-ai' }));
    expect(sendExecuteInputSpy).toHaveBeenCalledWith('ai-execute-inline');
  });

  it('disposes the controller on disconnect, recreates it on ws_url change, and releases onBinary on unmount', async () => {
    const disposeSpy = vi.spyOn(TrzszController.prototype, 'dispose');
    const processTerminalInputSpy = vi.spyOn(TrzszController.prototype, 'processTerminalInput');

    const { rerender, unmount } = render(<TerminalView sessionId="session-1" />);

    await waitFor(() => {
      expect(apiMocks.getTrzszCapabilities).toHaveBeenCalledTimes(1);
      expect(terminalInstances).toHaveLength(1);
    });

    const registryWriter = terminalRegistryMocks.registerTerminalBuffer.mock.calls[0]?.[6] as ((data: string) => void) | undefined;
    expect(registryWriter).toBeTypeOf('function');

    act(() => {
      emitEvent('connection_status_changed', {
        connection_id: 'connection-1',
        status: 'reconnecting',
      });
    });

    await waitFor(() => {
      expect(disposeSpy).toHaveBeenCalledTimes(1);
    });

    act(() => {
      emitEvent('connection_status_changed', {
        connection_id: 'connection-1',
        status: 'connected',
      });
    });

    act(() => {
      registryWriter?.('blocked-before-refresh');
    });
    expect(processTerminalInputSpy).not.toHaveBeenCalledWith('blocked-before-refresh');

    const staleFrame = encodeDataFrame(new TextEncoder().encode('stale-output'));
    act(() => {
      mockSockets[0]?.emitMessage(staleFrame);
    });
    expect(adaptiveRendererMock.scheduleWrite).not.toHaveBeenCalledWith(expect.any(Uint8Array));

    setConnectedSession({
      connectionId: 'connection-2',
      ws_url: 'ws://terminal-2',
      ws_token: 'token-2',
    });
    rerender(<TerminalView sessionId="session-1" />);

    await waitFor(() => {
      expect(mockSockets).toHaveLength(2);
      expect(apiMocks.getTrzszCapabilities).toHaveBeenCalledTimes(2);
    });

    act(() => {
      registryWriter?.('printf registry');
    });
    expect(processTerminalInputSpy).toHaveBeenCalledWith('printf registry');

    unmount();
    expect(terminalInstances[0]?.onBinaryDispose).toHaveBeenCalledTimes(1);
  });

  it('disposes the controller when the in-band transfer setting is disabled at runtime', async () => {
    const processTerminalInputSpy = vi.spyOn(TrzszController.prototype, 'processTerminalInput');
    const disposeSpy = vi.spyOn(TrzszController.prototype, 'dispose');

    const { rerender } = render(<TerminalView sessionId="session-1" />);

    await waitFor(() => {
      expect(terminalInstances).toHaveLength(1);
      expect(mockSockets).toHaveLength(1);
    });

    setInBandTransferEnabled(false);
    rerender(<TerminalView sessionId="session-1" />);

    await waitFor(() => {
      expect(disposeSpy).toHaveBeenCalledTimes(1);
    });

    act(() => {
      terminalInstances[0]?.onDataHandler?.('echo fallback');
    });

    expect(processTerminalInputSpy).not.toHaveBeenCalledWith('echo fallback');
  });

  it('sends cleanup on the original websocket when reconnecting interrupts a pending download prompt', async () => {
    let resolveSaveRoot: ((value: { rootPath: string; displayName: string; maps: Map<number, string> }) => void) | null = null;
    dialogMocks.chooseSaveRoot.mockImplementation(
      () => new Promise((resolve) => {
        resolveSaveRoot = resolve;
      }),
    );

    render(<TerminalView sessionId="session-1" />);

    await waitFor(() => {
      expect(mockSockets).toHaveLength(1);
      expect(typeof mockSockets[0]?.onmessage).toBe('function');
    });

    act(() => {
      mockSockets[0]?.emitMessage(
        encodeDataFrame(new TextEncoder().encode('::TRZSZ:TRANSFER:S:1.1.6:12345678\r\n')),
      );
    });

    await waitFor(() => {
      expect(dialogMocks.chooseSaveRoot).toHaveBeenCalledTimes(1);
    });

    act(() => {
      emitEvent('connection_status_changed', {
        connection_id: 'connection-1',
        status: 'reconnecting',
      });
    });

    act(() => {
      resolveSaveRoot?.({
        rootPath: '/tmp/trzsz-downloads',
        displayName: 'downloads',
        maps: new Map(),
      });
    });

    await waitFor(() => {
      expect(hasDataFrameWithTextPrefix(mockSockets[0], '#fail:')).toBe(true);
    });
  });
});
