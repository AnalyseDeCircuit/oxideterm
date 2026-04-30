// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { Terminal } from '@xterm/xterm';
import {
  closeTerminalCommandMarkById,
  createShellIntegratedCommandMark,
  getTerminalPromptBlockStartLine,
  type TerminalCommandMarkClosedBy,
} from './commandMarks';

export type ShellIntegrationSource = 'osc133' | 'osc633';

export type ShellIntegrationEvent =
  | { kind: 'prompt_start'; source: ShellIntegrationSource; line: number; col: number; raw?: string; sequence?: string }
  | { kind: 'command_start'; source: ShellIntegrationSource; line: number; col: number; raw?: string; sequence?: string }
  | { kind: 'output_start'; source: ShellIntegrationSource; line: number; col: number; command?: string | null; raw?: string; sequence?: string }
  | { kind: 'command_end'; source: ShellIntegrationSource; line: number; col: number; exitCode?: number; raw?: string; sequence?: string };

export type ShellIntegrationLifecycleState = 'idle' | 'prompt' | 'command' | 'output' | 'closed';

export type ShellIntegrationStatus = {
  detected: boolean;
  state: ShellIntegrationLifecycleState;
  integrationSource?: ShellIntegrationSource;
  lastSeenAt?: number;
};

export interface ShellIntegrationTraceEntry {
  id: number;
  at: number;
  source: ShellIntegrationSource;
  kind: ShellIntegrationEvent['kind'];
  line: number;
  col: number;
  raw?: string;
  sequence?: string;
  promptBlockStartLine?: number;
  activeCommandId?: string;
  createdMarkId?: string;
  closedMarkId?: string;
  closeLine?: number;
  closedBy?: TerminalCommandMarkClosedBy;
}

type TerminalPosition = {
  line: number;
  col: number;
};

type ControllerOptions = {
  term: Terminal;
  paneId: string;
  sessionId: string;
  nodeId?: string;
  getCwd?: () => string | null | undefined;
};

type PromptPosition = TerminalPosition & { at: number };

type ControllerState = {
  lifecycle: ShellIntegrationLifecycleState;
  integrationSource?: ShellIntegrationSource;
  lastSeenAt?: number;
  promptStart?: PromptPosition;
  commandStart?: PromptPosition;
  pendingCommandText?: string | null;
  pendingCommandTextFromProtocol?: boolean;
  activeCommandId?: string;
  activeStartLine?: number;
  startedAt?: number;
};

const MAX_COMMAND_TEXT_LENGTH = 4096;
const MAX_SHELL_INTEGRATION_TRACE_PER_PANE = 200;

const stateByPane = new Map<string, ControllerState>();
const traceByPane = new Map<string, ShellIntegrationTraceEntry[]>();
const statusListeners = new Set<() => void>();
let nextTraceId = 1;

function getAbsoluteCursorPosition(term: Terminal): TerminalPosition {
  return {
    line: term.buffer.active.baseY + term.buffer.active.cursorY,
    col: term.buffer.active.cursorX ?? 0,
  };
}

function splitSequence(data: string): { sequence: string; args: string[] } {
  const [sequence = '', ...args] = data.split(';');
  return { sequence, args };
}

function parseExitCode(args: string[]): number | undefined {
  const raw = args.find((part) => /^-?\d+$/.test(part.trim()));
  if (raw === undefined) return undefined;
  const value = Number(raw);
  return Number.isFinite(value) ? value : undefined;
}

export function sanitizeShellIntegrationCommandText(raw: string): string | null {
  if (!raw || raw.length > MAX_COMMAND_TEXT_LENGTH * 4) return null;

  let value = raw;
  if (/%[0-9A-Fa-f]{2}/.test(value)) {
    try {
      value = decodeURIComponent(value);
    } catch {
      return null;
    }
  }

  value = value
    .replace(/[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]/g, '')
    .trim();

  if (!value || value.length > MAX_COMMAND_TEXT_LENGTH) return null;
  return value;
}

export function parseOsc133(data: string, position: TerminalPosition): ShellIntegrationEvent | null {
  const { sequence, args } = splitSequence(data);
  const base = {
    source: 'osc133' as const,
    line: position.line,
    col: position.col,
    raw: data,
    sequence,
  };

  switch (sequence) {
    case 'A':
      return { ...base, kind: 'prompt_start' };
    case 'B':
      return { ...base, kind: 'command_start' };
    case 'C':
      return { ...base, kind: 'output_start' };
    case 'D':
      return { ...base, kind: 'command_end', exitCode: parseExitCode(args) };
    default:
      return null;
  }
}

export function parseOsc633(data: string, position: TerminalPosition): ShellIntegrationEvent | null {
  const { sequence, args } = splitSequence(data);
  const base = {
    source: 'osc633' as const,
    line: position.line,
    col: position.col,
    raw: data,
    sequence,
  };

  switch (sequence) {
    case 'A':
      return { ...base, kind: 'prompt_start' };
    case 'B':
      return { ...base, kind: 'command_start' };
    case 'C':
      return { ...base, kind: 'output_start' };
    case 'D':
      return { ...base, kind: 'command_end', exitCode: parseExitCode(args) };
    case 'E':
      return {
        ...base,
        kind: 'output_start',
        command: sanitizeShellIntegrationCommandText(args.join(';')),
      };
    default:
      return null;
  }
}

function getLineText(term: Terminal, absoluteLine: number): string {
  const line = term.buffer.active.getLine(absoluteLine);
  return line?.translateToString(true) ?? '';
}

function extractCommandFromVisibleBuffer(
  term: Terminal,
  commandStart: TerminalPosition | undefined,
  outputStart: TerminalPosition,
): string | null {
  const startLine = commandStart?.line ?? outputStart.line;
  const endLine = Math.max(startLine, outputStart.line);
  const lines: string[] = [];
  for (let line = startLine; line <= endLine; line += 1) {
    lines.push(getLineText(term, line));
  }

  const text = lines
    .join('\n')
    .replace(/^[\s❯➜λ>$#%❮›»]+/u, '')
    .trim();
  return sanitizeShellIntegrationCommandText(text);
}

function notifyStatusListeners(): void {
  for (const listener of statusListeners) {
    try {
      listener();
    } catch (error) {
      console.error('[ShellIntegration] listener failed:', error);
    }
  }
}

function updateStatus(paneId: string, source: ShellIntegrationSource, lifecycle: ShellIntegrationLifecycleState): ControllerState {
  const current = stateByPane.get(paneId) ?? { lifecycle: 'idle' as const };
  current.lifecycle = lifecycle;
  current.integrationSource = source;
  current.lastSeenAt = Date.now();
  stateByPane.set(paneId, current);
  notifyStatusListeners();
  return current;
}

function pushShellIntegrationTrace(paneId: string, entry: Omit<ShellIntegrationTraceEntry, 'id' | 'at'>): ShellIntegrationTraceEntry {
  const trace: ShellIntegrationTraceEntry = {
    ...entry,
    id: nextTraceId++,
    at: Date.now(),
  };
  const traces = traceByPane.get(paneId) ?? [];
  traces.push(trace);
  if (traces.length > MAX_SHELL_INTEGRATION_TRACE_PER_PANE) {
    traces.splice(0, traces.length - MAX_SHELL_INTEGRATION_TRACE_PER_PANE);
  }
  traceByPane.set(paneId, traces);
  return trace;
}

type ShellIntegrationCloseTrace = {
  closedMarkId: string;
  closeLine: number;
  closedBy: TerminalCommandMarkClosedBy;
};

function closeActiveMarkBefore(
  paneId: string,
  state: ControllerState,
  nextBlockStartLine: number,
  closedBy: Extract<TerminalCommandMarkClosedBy, 'next_command' | 'unknown'>,
): ShellIntegrationCloseTrace | null {
  if (!state.activeCommandId) return null;
  const fallbackStart = state.activeStartLine ?? state.promptStart?.line ?? nextBlockStartLine;
  const closedMarkId = state.activeCommandId;
  const closeLine = Math.max(fallbackStart, nextBlockStartLine - 1);
  closeTerminalCommandMarkById(paneId, state.activeCommandId, closedBy, 'high', {
    endLine: closeLine,
  });
  state.activeCommandId = undefined;
  state.activeStartLine = undefined;
  return { closedMarkId, closeLine, closedBy };
}

export function getShellIntegrationTrace(paneId: string): ShellIntegrationTraceEntry[] {
  return [...(traceByPane.get(paneId) ?? [])];
}

export function getShellIntegrationStatus(paneId: string): ShellIntegrationStatus {
  const state = stateByPane.get(paneId);
  if (!state?.integrationSource || !state.lastSeenAt) {
    return { detected: false, state: 'idle' };
  }
  return {
    detected: true,
    state: state.lifecycle,
    integrationSource: state.integrationSource,
    lastSeenAt: state.lastSeenAt,
  };
}

export function isShellIntegrationDetected(paneId: string): boolean {
  return getShellIntegrationStatus(paneId).detected;
}

export function subscribeShellIntegrationStatus(listener: () => void): () => void {
  statusListeners.add(listener);
  return () => {
    statusListeners.delete(listener);
  };
}

export function cleanupShellIntegration(paneId: string): void {
  traceByPane.delete(paneId);
  if (stateByPane.delete(paneId)) {
    notifyStatusListeners();
  }
}

export function createShellIntegrationController(options: ControllerOptions): {
  handleOsc133: (data: string) => boolean;
  handleOsc633: (data: string) => boolean;
  dispose: () => void;
} {
  const { term, paneId, sessionId, nodeId, getCwd } = options;

  const handleEvent = (event: ShellIntegrationEvent): void => {
    const previousLifecycle = stateByPane.get(paneId)?.lifecycle ?? 'idle';
    const state = updateStatus(paneId, event.source, event.kind === 'command_end' ? 'closed' : event.kind === 'output_start' ? 'output' : event.kind === 'command_start' ? 'command' : 'prompt');
    const trace: Omit<ShellIntegrationTraceEntry, 'id' | 'at'> = {
      source: event.source,
      kind: event.kind,
      line: event.line,
      col: event.col,
      raw: event.raw,
      sequence: event.sequence,
      activeCommandId: state.activeCommandId,
    };

    switch (event.kind) {
      case 'prompt_start': {
        const promptBlockStartLine = getTerminalPromptBlockStartLine(term, event.line);
        trace.promptBlockStartLine = promptBlockStartLine;
        const closed = closeActiveMarkBefore(paneId, state, promptBlockStartLine, 'next_command');
        if (closed) {
          trace.closedMarkId = closed.closedMarkId;
          trace.closeLine = closed.closeLine;
          trace.closedBy = closed.closedBy;
        }
        state.promptStart = { line: promptBlockStartLine, col: event.col, at: Date.now() };
        state.commandStart = undefined;
        state.pendingCommandText = null;
        state.pendingCommandTextFromProtocol = false;
        state.startedAt = undefined;
        state.lifecycle = 'prompt';
        break;
      }
      case 'command_start': {
        const promptBlockStartLine = getTerminalPromptBlockStartLine(term, event.line);
        trace.promptBlockStartLine = promptBlockStartLine;
        if (state.activeCommandId) {
          const closed = closeActiveMarkBefore(paneId, state, promptBlockStartLine, 'next_command');
          if (closed) {
            trace.closedMarkId = closed.closedMarkId;
            trace.closeLine = closed.closeLine;
            trace.closedBy = closed.closedBy;
          }
        }
        if (previousLifecycle !== 'prompt') {
          state.promptStart = { line: promptBlockStartLine, col: event.col, at: Date.now() };
        }
        state.commandStart = { line: event.line, col: event.col, at: Date.now() };
        state.pendingCommandText = null;
        state.pendingCommandTextFromProtocol = false;
        state.startedAt = Date.now();
        state.lifecycle = 'command';
        break;
      }
      case 'output_start': {
        if (event.command !== undefined) {
          state.pendingCommandText = event.command;
          state.pendingCommandTextFromProtocol = true;
        }
        if (state.activeCommandId) {
          state.lifecycle = 'output';
          break;
        }

        const startLine = state.promptStart?.line ?? state.commandStart?.line ?? event.line;
        const commandLine = state.commandStart?.line ?? state.promptStart?.line ?? startLine;
        const command = state.pendingCommandTextFromProtocol
          ? state.pendingCommandText ?? null
          : extractCommandFromVisibleBuffer(term, state.commandStart, { line: event.line, col: event.col });
        const mark = createShellIntegratedCommandMark(term, paneId, {
          command,
          sessionId,
          nodeId,
          cwd: getCwd?.() ?? undefined,
          startLine,
          commandLine,
          startedAt: state.startedAt ?? state.promptStart?.at ?? Date.now(),
        });
        if (mark) {
          state.activeCommandId = mark.commandId;
          state.activeStartLine = startLine;
          trace.createdMarkId = mark.commandId;
        }
        state.promptStart = undefined;
        state.lifecycle = 'output';
        break;
      }
      case 'command_end': {
        if (!state.activeCommandId) {
          state.lifecycle = 'closed';
          break;
        }
        const endBoundaryLine = getTerminalPromptBlockStartLine(term, event.line);
        trace.promptBlockStartLine = endBoundaryLine;
        const fallbackStart = state.activeStartLine ?? state.promptStart?.line ?? endBoundaryLine;
        const closeLine = Math.max(fallbackStart, endBoundaryLine - 1);
        const closedMarkId = state.activeCommandId;
        closeTerminalCommandMarkById(paneId, state.activeCommandId, 'shell_integration', 'high', {
          endLine: closeLine,
          exitCode: event.exitCode,
        });
        trace.closedMarkId = closedMarkId;
        trace.closeLine = closeLine;
        trace.closedBy = 'shell_integration';
        state.activeCommandId = undefined;
        state.activeStartLine = undefined;
        state.lifecycle = 'closed';
        break;
      }
      default:
        break;
    }
    pushShellIntegrationTrace(paneId, trace);
  };

  return {
    handleOsc133: (data: string) => {
      const event = parseOsc133(data, getAbsoluteCursorPosition(term));
      if (event) handleEvent(event);
      return false;
    },
    handleOsc633: (data: string) => {
      const event = parseOsc633(data, getAbsoluteCursorPosition(term));
      if (event) handleEvent(event);
      return false;
    },
    dispose: () => {
      cleanupShellIntegration(paneId);
    },
  };
}
