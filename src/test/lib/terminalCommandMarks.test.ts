// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { Terminal } from '@xterm/xterm';
import {
  clearTerminalCommandMarkSelection,
  cleanupTerminalCommandMarks,
  createTerminalCommandMark,
  listTerminalCommandMarks,
  selectTerminalCommandMarkAtLine,
} from '@/lib/terminal/commandMarks';

const mocks = vi.hoisted(() => ({
  writeText: vi.fn(() => Promise.resolve()),
  t: vi.fn((key: string, params?: Record<string, unknown>) => {
    const translations: Record<string, string> = {
      'terminal.command_selection.actions': 'Command selection actions',
      'terminal.command_selection.copy': 'Copy',
      'terminal.command_selection.copy_title': 'Copy command output',
    };
    return (translations[key] ?? key).replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(params?.[name] ?? ''));
  }),
}));

vi.mock('@/i18n', () => ({
  default: {
    t: mocks.t,
  },
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: {
    getState: () => ({
      settings: {
        terminal: {
          commandMarks: {
            enabled: true,
            userInputObserved: false,
            heuristicDetection: false,
            showHoverActions: true,
          },
        },
      },
    }),
  },
}));

vi.mock('@/lib/ai/orchestrator/ledger', () => ({
  addAiCommandRecord: vi.fn(),
}));

vi.mock('@/lib/ai/orchestrator/runtimeEpoch', () => ({
  getAiRuntimeEpoch: () => 'test-runtime',
}));

vi.mock('@/lib/clipboardSupport', () => ({
  writeSystemClipboardText: mocks.writeText,
}));

type MockMarker = {
  dispose: ReturnType<typeof vi.fn>;
  onDispose: ReturnType<typeof vi.fn>;
  line?: number;
  isDisposed?: boolean;
};

type MockDecoration = {
  dispose: ReturnType<typeof vi.fn>;
  onRender: ReturnType<typeof vi.fn>;
};

function createMockTerminal(options: {
  marker?: MockMarker | null;
  decoration?: MockDecoration | null;
  baseY?: number;
  cursorY?: number;
  lines?: Record<number, string>;
} = {}): Terminal {
  const decoration = options.decoration === undefined
    ? { dispose: vi.fn(), onRender: vi.fn() }
    : options.decoration;
  const activeBuffer = {
    type: 'normal',
    baseY: options.baseY ?? 10,
    cursorY: options.cursorY ?? 2,
    viewportY: options.baseY ?? 10,
    length: 40,
    getLine: vi.fn((line: number) => ({ translateToString: () => options.lines?.[line] ?? 'output' })),
  };
  const createMarker = (line: number): MockMarker => {
    const marker = {
      line,
      isDisposed: false,
      dispose: vi.fn(() => {
        marker.isDisposed = true;
        marker.line = -1;
      }),
      onDispose: vi.fn(),
    };
    return marker;
  };
  const element = document.createElement('div');
  const screen = document.createElement('div');
  const rows = document.createElement('div');
  screen.className = 'xterm-screen';
  rows.className = 'xterm-rows';
  const rect = {
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: 1200,
    bottom: 400,
    width: 1200,
    height: 400,
    toJSON: () => ({}),
  };
  screen.getBoundingClientRect = vi.fn(() => rect);
  rows.getBoundingClientRect = vi.fn(() => rect);
  Object.defineProperty(screen, 'clientWidth', { value: 1200 });
  screen.append(rows);
  element.append(screen);
  const event = vi.fn(() => ({ dispose: vi.fn() }));

  return {
    cols: 120,
    rows: 20,
    element,
    buffer: {
      active: activeBuffer,
    },
    modes: { mouseTrackingMode: 'none' },
    onScroll: event,
    onRender: event,
    onResize: event,
    onWriteParsed: event,
    onCursorMove: event,
    registerMarker: vi.fn((offset = 0) => {
      if (options.marker === null) return null;
      const line = activeBuffer.baseY + activeBuffer.cursorY + offset;
      if (options.marker) {
        options.marker.line ??= line;
        options.marker.isDisposed ??= false;
        return options.marker;
      }
      return createMarker(line);
    }),
    registerDecoration: vi.fn(() => decoration),
  } as unknown as Terminal;
}

describe('terminal command marks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    cleanupTerminalCommandMarks('pane-1');
  });

  it('does not mutate the store when decoration registration fails', () => {
    const marker = { dispose: vi.fn(), onDispose: vi.fn() };
    const term = createMockTerminal({ marker, decoration: null });

    const mark = createTerminalCommandMark(term, 'pane-1', {
      command: 'ls -la',
      source: 'command_bar',
      sessionId: 'session-1',
    });

    expect(mark).toBeNull();
    expect(marker.dispose).toHaveBeenCalledTimes(1);
    expect(listTerminalCommandMarks('pane-1')).toEqual([]);
  });

  it('commits a mark only after marker and decoration are registered', () => {
    const decoration = { dispose: vi.fn(), onRender: vi.fn() };
    const term = createMockTerminal({ decoration });

    const mark = createTerminalCommandMark(term, 'pane-1', {
      command: 'pwd',
      source: 'command_bar',
      sessionId: 'session-1',
      cwd: '/tmp',
    });

    expect(mark?.command).toBe('pwd');
    expect(decoration.onRender).toHaveBeenCalledTimes(1);
    expect(term.registerDecoration).toHaveBeenCalledTimes(1);
    expect(listTerminalCommandMarks('pane-1')).toMatchObject([
      {
        command: 'pwd',
        cwd: '/tmp',
        startLine: 12,
        isClosed: false,
        detectionSource: 'command_bar',
        confidence: 'high',
      },
    ]);
  });

  it('keeps experimental heuristic marks disabled by default', () => {
    const term = createMockTerminal();

    const mark = createTerminalCommandMark(term, 'pane-1', {
      command: 'echo maybe',
      source: 'heuristic',
      sessionId: 'session-1',
    });

    expect(mark).toBeNull();
    expect(listTerminalCommandMarks('pane-1')).toEqual([]);
  });

  it('records observed user input marks and allows selecting the current open command range', () => {
    const term = createMockTerminal();

    const mark = createTerminalCommandMark(term, 'pane-1', {
      command: 'pwd',
      source: 'user_input_observed',
      sessionId: 'session-1',
    });
    (term.buffer.active as unknown as { baseY: number }).baseY = 15;
    (term.buffer.active as unknown as { cursorY: number }).cursorY = 0;

    expect(mark).toMatchObject({
      command: 'pwd',
      isClosed: false,
      confidence: 'low',
    });
    expect(selectTerminalCommandMarkAtLine(term, 'pane-1', 13)).toBe(true);
    expect(term.registerDecoration).toHaveBeenCalledTimes(1);
  });

  it('renders a localized real copy action without exposing fake actions', () => {
    const term = createMockTerminal();

    const mark = createTerminalCommandMark(term, 'pane-1', {
      command: 'pwd',
      source: 'user_input_observed',
      sessionId: 'session-1',
    });
    (term.buffer.active as unknown as { baseY: number }).baseY = 15;
    (term.buffer.active as unknown as { cursorY: number }).cursorY = 0;

    expect(mark).toBeTruthy();
    expect(selectTerminalCommandMarkAtLine(term, 'pane-1', 13)).toBe(true);

    const buttons = Array.from((term.element as HTMLElement).querySelectorAll('button'));
    expect(buttons.map((button) => button.textContent)).toEqual(['Copy']);
    const overlay = (term.element as HTMLElement).querySelector('.xterm-command-selection-overlay');
    const actions = (term.element as HTMLElement).querySelector('.xterm-command-selection-actions');
    expect(actions?.parentElement).toBe(overlay?.parentElement);
    expect(overlay?.contains(actions)).toBe(false);

    buttons[0].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    expect(mocks.writeText).toHaveBeenCalledWith('output\noutput');
  });

  it('starts a mark at the prompt preamble so the previous mark excludes the next prompt', () => {
    const term = createMockTerminal({
      lines: {
        12: '❯ pwd',
        13: '/home/lipsc',
        14: '   ~ ··························· lips@host 02:18:19',
        15: '❯ ls',
      },
    });

    const first = createTerminalCommandMark(term, 'pane-1', {
      command: 'pwd',
      source: 'user_input_observed',
      sessionId: 'session-1',
    });
    (term.buffer.active as unknown as { baseY: number }).baseY = 15;
    (term.buffer.active as unknown as { cursorY: number }).cursorY = 0;
    const second = createTerminalCommandMark(term, 'pane-1', {
      command: 'ls',
      source: 'user_input_observed',
      sessionId: 'session-1',
    });

    expect(first).toMatchObject({
      command: 'pwd',
      startLine: 12,
      commandLine: 12,
      endLine: 13,
    });
    expect(second).toMatchObject({
      command: 'ls',
      startLine: 14,
      commandLine: 15,
    });
    expect(selectTerminalCommandMarkAtLine(term, 'pane-1', 14)).toBe(true);
    expect((term.element as HTMLElement).querySelector('.xterm-command-selection-overlay')).toBeTruthy();
  });

  it('excludes the returned prompt preamble from an open mark selection and copied output', () => {
    const term = createMockTerminal({
      lines: {
        12: '❯ pwd',
        13: '/home/lipsc',
        14: '   ~ ··························· lips@host 02:18:19',
        15: '❯',
      },
    });

    createTerminalCommandMark(term, 'pane-1', {
      command: 'pwd',
      source: 'user_input_observed',
      sessionId: 'session-1',
    });
    (term.buffer.active as unknown as { baseY: number }).baseY = 15;
    (term.buffer.active as unknown as { cursorY: number }).cursorY = 0;

    expect(selectTerminalCommandMarkAtLine(term, 'pane-1', 14)).toBe(false);
    expect(selectTerminalCommandMarkAtLine(term, 'pane-1', 13)).toBe(true);

    const copy = (term.element as HTMLElement).querySelector('button');
    copy?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    expect(mocks.writeText).toHaveBeenCalledWith('/home/lipsc');
  });

  it('closes the previous mark without drawing a range until selected', () => {
    const term = createMockTerminal();

    const first = createTerminalCommandMark(term, 'pane-1', {
      command: 'pwd',
      source: 'command_bar',
      sessionId: 'session-1',
    });
    (term.buffer.active as unknown as { baseY: number }).baseY = 13;
    (term.buffer.active as unknown as { cursorY: number }).cursorY = 0;
    const second = createTerminalCommandMark(term, 'pane-1', {
      command: 'ls',
      source: 'command_bar',
      sessionId: 'session-1',
    });

    expect(first).toBeTruthy();
    expect(second).toBeTruthy();
    expect(term.registerDecoration).toHaveBeenCalledTimes(2);
    expect(listTerminalCommandMarks('pane-1')[0]).toMatchObject({
      command: 'pwd',
      isClosed: true,
      endLine: 12,
    });

    expect(selectTerminalCommandMarkAtLine(term, 'pane-1', 12)).toBe(true);
    expect(term.registerDecoration).toHaveBeenCalledTimes(2);
    clearTerminalCommandMarkSelection('pane-1');
  });

  it('hit-tests against the live marker line after scrollback shifts', () => {
    const term = createMockTerminal();

    createTerminalCommandMark(term, 'pane-1', {
      command: 'pwd',
      source: 'command_bar',
      sessionId: 'session-1',
    });
    (term.buffer.active as unknown as { baseY: number }).baseY = 13;
    (term.buffer.active as unknown as { cursorY: number }).cursorY = 0;
    createTerminalCommandMark(term, 'pane-1', {
      command: 'ls',
      source: 'command_bar',
      sessionId: 'session-1',
    });

    const primaryMarker = vi.mocked(term.registerMarker).mock.results[0].value as MockMarker;
    primaryMarker.line = 9;

    expect(selectTerminalCommandMarkAtLine(term, 'pane-1', 12)).toBe(false);
    expect(selectTerminalCommandMarkAtLine(term, 'pane-1', 9)).toBe(true);
  });
});
