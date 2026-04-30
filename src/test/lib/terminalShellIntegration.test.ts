// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Terminal } from '@xterm/xterm';
import { cleanupTerminalCommandMarks, createTerminalCommandMark, listTerminalCommandMarks } from '@/lib/terminal/commandMarks';
import {
  cleanupShellIntegration,
  createShellIntegrationController,
  getShellIntegrationTrace,
  getShellIntegrationStatus,
  parseOsc133,
  parseOsc633,
  sanitizeShellIntegrationCommandText,
} from '@/lib/terminal/shellIntegration';
import { addAiCommandRecord } from '@/lib/ai/orchestrator/ledger';

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

vi.mock('@/lib/api', () => ({
  api: {
    createCommandFact: vi.fn(() => Promise.resolve({
      factId: 'fact-1',
      fact: {
        factId: 'fact-1',
        sessionId: 'session-1',
        source: 'shell_integration',
        startGlobalLine: 0,
        commandGlobalLine: 0,
        bufferGeneration: 0,
        runtimeEpoch: 'test-runtime',
        status: 'open',
        confidence: 'high',
        createdAt: Date.now(),
      },
    })),
    closeCommandFact: vi.fn(() => Promise.resolve({})),
    getCommandFacts: vi.fn(() => Promise.resolve([])),
    getCommandFactOutput: vi.fn(() => Promise.resolve({ text: '', truncated: false, lineCount: 0, stale: false })),
  },
}));

vi.mock('@/lib/clipboardSupport', () => ({
  writeSystemClipboardText: vi.fn(() => Promise.resolve()),
}));

function createMockTerminal(lines: Record<number, string> = {}): Terminal & {
  setPosition: (line: number, col?: number) => void;
} {
  const activeBuffer = {
    type: 'normal',
    baseY: 0,
    cursorY: 0,
    cursorX: 0,
    viewportY: 0,
    length: 200,
    getLine: vi.fn((line: number) => ({ translateToString: () => lines[line] ?? '' })),
  };
  const element = document.createElement('div');
  const event = vi.fn(() => ({ dispose: vi.fn() }));

  const term = {
    cols: 120,
    rows: 24,
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
      const marker = {
        line: activeBuffer.baseY + activeBuffer.cursorY + offset,
        isDisposed: false,
        dispose: vi.fn(() => {
          marker.isDisposed = true;
          marker.line = -1;
        }),
        onDispose: vi.fn(),
      };
      return marker;
    }),
    registerDecoration: vi.fn(() => ({
      dispose: vi.fn(),
      onRender: vi.fn(),
    })),
    setPosition: (line: number, col = 0) => {
      activeBuffer.baseY = 0;
      activeBuffer.cursorY = line;
      activeBuffer.cursorX = col;
    },
  };
  return term as unknown as Terminal & { setPosition: (line: number, col?: number) => void };
}

describe('terminal shell integration', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    cleanupShellIntegration('pane-1');
    cleanupTerminalCommandMarks('pane-1');
  });

  it('maps OSC 133 and OSC 633 sequences to internal events', () => {
    expect(parseOsc133('A', { line: 10, col: 0 })).toMatchObject({ kind: 'prompt_start', source: 'osc133' });
    expect(parseOsc133('B', { line: 10, col: 2 })).toMatchObject({ kind: 'command_start', source: 'osc133' });
    expect(parseOsc133('C', { line: 11, col: 0 })).toMatchObject({ kind: 'output_start', source: 'osc133' });
    expect(parseOsc133('D;7', { line: 12, col: 0 })).toMatchObject({ kind: 'command_end', exitCode: 7 });

    expect(parseOsc633('A', { line: 20, col: 0 })).toMatchObject({ kind: 'prompt_start', source: 'osc633' });
    expect(parseOsc633('E;git%20status', { line: 20, col: 10 })).toMatchObject({
      kind: 'output_start',
      command: 'git status',
    });
  });

  it('drops malformed or oversized explicit command text', () => {
    expect(sanitizeShellIntegrationCommandText('%E0%A4%A')).toBeNull();
    expect(sanitizeShellIntegrationCommandText('x'.repeat(5000))).toBeNull();
    expect(sanitizeShellIntegrationCommandText('pwd\u0000\u001b')).toBe('pwd');
  });

  it('creates and closes a high confidence mark from VS Code OSC 633 commandline', () => {
    const term = createMockTerminal();
    const controller = createShellIntegrationController({ term, paneId: 'pane-1', sessionId: 'session-1' });

    term.setPosition(10);
    controller.handleOsc633('A');
    term.setPosition(10, 5);
    controller.handleOsc633('B');
    controller.handleOsc633('E;pwd');
    term.setPosition(12);
    controller.handleOsc633('D;0');

    expect(listTerminalCommandMarks('pane-1')).toMatchObject([
      {
        command: 'pwd',
        startLine: 10,
        commandLine: 10,
        endLine: 11,
        isClosed: true,
        exitCode: 0,
        detectionSource: 'shell_integration',
        confidence: 'high',
      },
    ]);
    expect(addAiCommandRecord).toHaveBeenCalledWith(expect.objectContaining({
      command: 'pwd',
      source: 'shell_integration',
      startLine: 10,
      endLine: 11,
    }));
  });

  it('uses the next prompt to close commands that never emitted command_end', () => {
    const term = createMockTerminal({ 31: 'echo done' });
    const controller = createShellIntegrationController({ term, paneId: 'pane-1', sessionId: 'session-1' });

    term.setPosition(30);
    controller.handleOsc133('A');
    term.setPosition(31);
    controller.handleOsc133('B');
    term.setPosition(32);
    controller.handleOsc133('C');
    term.setPosition(35);
    controller.handleOsc133('A');

    expect(listTerminalCommandMarks('pane-1')).toMatchObject([
      {
        startLine: 30,
        endLine: 34,
        closedBy: 'next_command',
        isClosed: true,
      },
    ]);
  });

  it('creates standalone range marks for empty shell integration commands without writing the ledger', () => {
    const term = createMockTerminal();
    const controller = createShellIntegrationController({ term, paneId: 'pane-1', sessionId: 'session-1' });

    term.setPosition(40);
    controller.handleOsc133('A');
    term.setPosition(41);
    controller.handleOsc133('B');
    term.setPosition(42);
    controller.handleOsc133('C');
    term.setPosition(43);
    controller.handleOsc133('D;0');

    expect(listTerminalCommandMarks('pane-1')).toMatchObject([
      {
        command: null,
        startLine: 40,
        commandLine: 41,
        endLine: 42,
        detectionSource: 'shell_integration',
        isClosed: true,
      },
    ]);
    expect(addAiCommandRecord).not.toHaveBeenCalled();
  });

  it('does not fall back to visible text when explicit OSC 633 commandline is malformed', () => {
    const term = createMockTerminal({ 50: 'rm -rf /should-not-be-read' });
    const controller = createShellIntegrationController({ term, paneId: 'pane-1', sessionId: 'session-1' });

    term.setPosition(50);
    controller.handleOsc633('A');
    controller.handleOsc633('B');
    controller.handleOsc633('E;%E0%A4%A');
    term.setPosition(52);
    controller.handleOsc633('D;0');

    expect(listTerminalCommandMarks('pane-1')).toMatchObject([
      {
        command: null,
        detectionSource: 'shell_integration',
      },
    ]);
    expect(addAiCommandRecord).not.toHaveBeenCalled();
  });

  it('does not reuse a previous prompt start when an empty command is missing prompt_start', () => {
    const term = createMockTerminal({ 11: 'ls' });
    const controller = createShellIntegrationController({ term, paneId: 'pane-1', sessionId: 'session-1' });

    term.setPosition(10);
    controller.handleOsc133('A');
    term.setPosition(11);
    controller.handleOsc133('B');
    term.setPosition(12);
    controller.handleOsc133('C');
    term.setPosition(20);
    controller.handleOsc133('D;0');

    term.setPosition(22);
    controller.handleOsc133('B');
    term.setPosition(23);
    controller.handleOsc133('C');
    term.setPosition(24);
    controller.handleOsc133('D;130');

    expect(listTerminalCommandMarks('pane-1')).toMatchObject([
      {
        command: 'ls',
        startLine: 10,
        endLine: 19,
        isClosed: true,
      },
      {
        command: null,
        startLine: 22,
        commandLine: 22,
        endLine: 23,
        isClosed: true,
      },
    ]);
    expect(addAiCommandRecord).toHaveBeenCalledTimes(1);
  });

  it('uses the full prompt block start when empty commands follow a multiline p10k prompt', () => {
    const term = createMockTerminal({
      10: '   ~ ······················ lipsc@host  01:57:29 ',
      11: '❯ ls',
      30: '   ~ ······················ INT ✘  lipsc@host  01:57:32 ',
      31: '❯ ',
    });
    const controller = createShellIntegrationController({ term, paneId: 'pane-1', sessionId: 'session-1' });

    term.setPosition(10);
    controller.handleOsc133('A');
    term.setPosition(11);
    controller.handleOsc133('B');
    term.setPosition(12);
    controller.handleOsc133('C');

    term.setPosition(31);
    controller.handleOsc133('B');
    term.setPosition(32);
    controller.handleOsc133('C');
    controller.handleOsc133('D;130');

    expect(listTerminalCommandMarks('pane-1')).toMatchObject([
      {
        command: 'ls',
        startLine: 10,
        endLine: 29,
        closedBy: 'next_command',
        isClosed: true,
      },
      {
        command: null,
        startLine: 30,
        commandLine: 31,
        endLine: 31,
        closedBy: 'shell_integration',
        isClosed: true,
        exitCode: 130,
      },
    ]);
    expect(addAiCommandRecord).toHaveBeenCalledTimes(1);
  });

  it('keeps a capped shell integration trace for diagnostics', () => {
    const term = createMockTerminal();
    const controller = createShellIntegrationController({ term, paneId: 'pane-1', sessionId: 'session-1' });

    for (let index = 0; index < 205; index += 1) {
      term.setPosition(index);
      controller.handleOsc633('A');
    }

    const trace = getShellIntegrationTrace('pane-1');
    expect(trace).toHaveLength(200);
    expect(trace[0]?.line).toBe(5);
    expect(trace.at(-1)).toMatchObject({
      kind: 'prompt_start',
      source: 'osc633',
      line: 204,
      promptBlockStartLine: 204,
    });
  });

  it('records integration source and last seen time for diagnostics', () => {
    const term = createMockTerminal();
    const controller = createShellIntegrationController({ term, paneId: 'pane-1', sessionId: 'session-1' });

    term.setPosition(5);
    controller.handleOsc633('A');

    expect(getShellIntegrationStatus('pane-1')).toMatchObject({
      detected: true,
      state: 'prompt',
      integrationSource: 'osc633',
    });
  });

  it('merges a recent Command Bar mark with matching shell integration boundaries', () => {
    const term = createMockTerminal();
    term.setPosition(10);
    const commandBarMark = createTerminalCommandMark(term, 'pane-1', {
      command: 'pwd',
      source: 'command_bar',
      sessionId: 'session-1',
    });
    const controller = createShellIntegrationController({ term, paneId: 'pane-1', sessionId: 'session-1' });

    term.setPosition(11);
    controller.handleOsc633('A');
    controller.handleOsc633('B');
    controller.handleOsc633('E;pwd');
    term.setPosition(13);
    controller.handleOsc633('D;0');

    expect(listTerminalCommandMarks('pane-1')).toMatchObject([
      {
        commandId: commandBarMark?.commandId,
        command: 'pwd',
        startLine: 11,
        endLine: 12,
        detectionSource: 'shell_integration',
        submittedBy: 'command_bar',
      },
    ]);
    expect(addAiCommandRecord).toHaveBeenCalledTimes(1);
  });
});
