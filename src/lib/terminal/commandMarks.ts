// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { IDecoration, IMarker, Terminal } from '@xterm/xterm';
import { useSettingsStore } from '@/store/settingsStore';
import { addAiCommandRecord } from '@/lib/ai/orchestrator/ledger';
import { getAiRuntimeEpoch } from '@/lib/ai/orchestrator/runtimeEpoch';
import { writeSystemClipboardText } from '@/lib/clipboardSupport';

export type TerminalCommandMarkDetectionSource =
  | 'command_bar'
  | 'ai'
  | 'broadcast'
  | 'user_input_observed'
  | 'heuristic'
  | 'shell_integration';

export type TerminalCommandMarkClosedBy =
  | 'next_command'
  | 'shell_integration'
  | 'terminal_reset'
  | 'session_lost'
  | 'interrupted_mode'
  | 'timeout'
  | 'manual'
  | 'unknown';

export type TerminalCommandMarkConfidence = 'high' | 'medium' | 'low';
export type TerminalCommandMarkOutputConfidence = TerminalCommandMarkConfidence | 'unknown';

export type TerminalCommandMark = {
  commandId: string;
  paneId: string;
  sessionId: string;
  nodeId?: string;
  command: string;
  cwd?: string;
  startLine: number;
  commandLine: number;
  endLine?: number;
  isClosed: boolean;
  closedBy?: TerminalCommandMarkClosedBy;
  exitCode?: number | null;
  durationMs?: number;
  runtimeEpoch: string;
  detectionSource: TerminalCommandMarkDetectionSource;
  confidence: TerminalCommandMarkConfidence;
  outputConfidence: TerminalCommandMarkOutputConfidence;
  collapsed: boolean;
  stale: boolean;
  startedAt: number;
  finishedAt?: number;
};

export type TerminalCommandMarkRequest = {
  command: string;
  source: TerminalCommandMarkDetectionSource;
  sessionId: string;
  nodeId?: string;
  cwd?: string | null;
};

type DecorationRecord = {
  marker: IMarker;
  decoration: IDecoration;
  primary: boolean;
  role: CommandMarkDecorationRole;
  element?: HTMLElement;
};

type SelectionOverlayRecord = {
  paneId: string;
  mark: TerminalCommandMark;
  term: Terminal;
  element: HTMLElement;
  actionsElement?: HTMLElement;
  disposables: Array<{ dispose: () => void }>;
};

const MAX_MARKS_PER_PANE = 200;
const MAX_OUTPUT_CHARS = 24000;
const MAX_OUTPUT_LINES = 400;

const marksByPane = new Map<string, TerminalCommandMark[]>();
const decorationsByCommandId = new Map<string, DecorationRecord[]>();
const selectionOverlaysByPane = new Map<string, SelectionOverlayRecord>();
const selectedMarkByPane = new Map<string, string>();
const listeners = new Set<() => void>();
let sequence = 0;

const commandSelectionFallbacks: Record<string, string> = {
  'terminal.command_selection.actions': 'Command selection actions',
  'terminal.command_selection.copy': 'Copy',
  'terminal.command_selection.copy_title': 'Copy command output',
};

function setLocalizedText(element: HTMLElement, key: string, attribute?: 'title' | 'aria-label'): void {
  const fallback = commandSelectionFallbacks[key] ?? key;
  if (attribute) {
    element.setAttribute(attribute, fallback);
  } else {
    element.textContent = fallback;
  }

  void import('@/i18n').then(({ default: i18n }) => {
    const translated = i18n.t(key);
    if (attribute) {
      element.setAttribute(attribute, translated);
    } else {
      element.textContent = translated;
    }
  }).catch(() => {
    // Keep the fallback text when i18n is unavailable in isolated tests.
  });
}

function nextCommandId(): string {
  sequence += 1;
  return `term-cmd-${Date.now().toString(36)}-${sequence.toString(36)}`;
}

function notify(): void {
  for (const listener of listeners) {
    try {
      listener();
    } catch (error) {
      console.error('[TerminalCommandMarks] listener failed:', error);
    }
  }
}

function getAbsoluteCursorLine(term: Terminal): number {
  return term.buffer.active.baseY + term.buffer.active.cursorY;
}

export function getTerminalAbsoluteLineFromClientY(term: Terminal, container: HTMLElement, clientY: number): number | null {
  const rowsElement = container.querySelector<HTMLElement>('.xterm-rows');
  const rect = (rowsElement ?? container).getBoundingClientRect();
  if (rect.height <= 0 || term.rows <= 0) return null;
  const y = clientY - rect.top;
  if (y < 0 || y > rect.height) return null;
  const row = Math.min(term.rows - 1, Math.max(0, Math.floor(y / (rect.height / term.rows))));
  return term.buffer.active.viewportY + row;
}

function getLineText(term: Terminal, absoluteLine: number): string {
  const line = term.buffer.active.getLine(absoluteLine);
  return line?.translateToString(true) ?? '';
}

function isLikelyPromptInputLine(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed) return true;
  return /^[❯➜λ>$#%❮›»]/u.test(trimmed);
}

function isLikelyPromptPreambleLine(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed) return false;

  const hasPrivateUseGlyph = /[\uE000-\uF8FF]/u.test(trimmed);
  const hasPowerlineGlyph = /[]/u.test(trimmed);
  const hasRuler = /[·•∙.]{6,}/u.test(trimmed);
  const hasClock = /\b\d{1,2}:\d{2}(?::\d{2})?\b/.test(trimmed);
  const hasPromptContext = /[@~/$]|[A-Za-z0-9._-]+@[A-Za-z0-9._-]+/.test(trimmed);

  return hasPowerlineGlyph
    || (hasPrivateUseGlyph && (hasClock || hasRuler || hasPromptContext))
    || (hasRuler && (hasClock || hasPromptContext));
}

function getPromptBlockStartLine(term: Terminal, commandLine: number): number {
  if (!isLikelyPromptInputLine(getLineText(term, commandLine))) {
    return commandLine;
  }

  let startLine = commandLine;
  const minLine = Math.max(0, commandLine - 3);
  for (let line = commandLine - 1; line >= minLine; line -= 1) {
    if (!isLikelyPromptPreambleLine(getLineText(term, line))) break;
    startLine = line;
  }
  return startLine;
}

function getPrimaryDecorationRecord(commandId: string): DecorationRecord | null {
  return decorationsByCommandId.get(commandId)?.find((record) => record.primary) ?? null;
}

function getLiveMarkRange(mark: TerminalCommandMark): { startLine: number; endLine?: number } | null {
  const primary = getPrimaryDecorationRecord(mark.commandId);
  if (!primary) {
    return {
      startLine: mark.startLine,
      endLine: mark.endLine,
    };
  }
  if (primary.marker.line < 0 || primary.marker.isDisposed) return null;

  const startLine = primary.marker.line;
  if (typeof mark.endLine !== 'number') {
    return { startLine };
  }

  return {
    startLine,
    endLine: startLine + Math.max(0, mark.endLine - mark.startLine),
  };
}

function getSelectableMarkRange(term: Terminal, mark: TerminalCommandMark): { startLine: number; endLine: number } | null {
  const liveRange = getLiveMarkRange(mark);
  if (!liveRange) return null;
  if (typeof liveRange.endLine === 'number') {
    return {
      startLine: liveRange.startLine,
      endLine: liveRange.endLine,
    };
  }
  const transientEndLine = Math.max(liveRange.startLine, getPromptBlockStartLine(term, getAbsoluteCursorLine(term)) - 1);
  return {
    startLine: liveRange.startLine,
    endLine: transientEndLine,
  };
}

function getLiveCommandLine(mark: TerminalCommandMark): number | null {
  const liveRange = getLiveMarkRange(mark);
  if (!liveRange) return null;
  return liveRange.startLine + Math.max(0, mark.commandLine - mark.startLine);
}

function getOutputRangeText(term: Terminal, mark: TerminalCommandMark): { text: string; truncated: boolean; lineCount: number } {
  const liveRange = getSelectableMarkRange(term, mark);
  const liveStartLine = liveRange?.startLine ?? mark.startLine;
  const liveCommandLine = getLiveCommandLine(mark) ?? liveStartLine;
  const liveEndLine = liveRange?.endLine ?? mark.endLine;
  const start = liveCommandLine + 1;
  const end = typeof liveEndLine === 'number'
    ? liveEndLine
    : Math.min(term.buffer.active.length - 1, getAbsoluteCursorLine(term));
  const lines: string[] = [];
  let charCount = 0;
  let truncated = false;

  for (let line = start; line <= end; line += 1) {
    if (lines.length >= MAX_OUTPUT_LINES || charCount >= MAX_OUTPUT_CHARS) {
      truncated = true;
      break;
    }
    const text = getLineText(term, line);
    charCount += text.length + 1;
    if (charCount > MAX_OUTPUT_CHARS) {
      lines.push(text.slice(0, Math.max(0, MAX_OUTPUT_CHARS - (charCount - text.length - 1))));
      truncated = true;
      break;
    }
    lines.push(text);
  }

  return { text: lines.join('\n'), truncated, lineCount: Math.max(0, end - start + 1) };
}

function isLedgerSource(source: TerminalCommandMarkDetectionSource): boolean {
  return source === 'command_bar' || source === 'ai' || source === 'broadcast' || source === 'shell_integration';
}

function closeOpenMarks(
  paneId: string,
  nextStartLine: number,
  closedBy: TerminalCommandMarkClosedBy,
  outputConfidence: TerminalCommandMarkOutputConfidence,
): void {
  const marks = marksByPane.get(paneId) ?? [];
  for (const mark of marks) {
    if (mark.isClosed) continue;
    mark.isClosed = true;
    mark.closedBy = closedBy;
    mark.outputConfidence = outputConfidence;
    mark.endLine = Math.max(mark.startLine, nextStartLine - 1);
    mark.finishedAt = Date.now();
    mark.durationMs = mark.finishedAt - mark.startedAt;
    if (isLedgerSource(mark.detectionSource) && mark.confidence === 'high') {
      addAiCommandRecord({
        commandId: mark.commandId,
        targetId: mark.nodeId ? `ssh-node:${mark.nodeId}` : undefined,
        sessionId: mark.sessionId,
        nodeId: mark.nodeId,
        command: mark.command,
        cwd: mark.cwd,
        source: mark.detectionSource === 'ai'
          ? 'ai.terminal_input'
          : mark.detectionSource === 'broadcast'
            ? 'broadcast'
            : mark.detectionSource === 'shell_integration'
              ? 'shell_integration'
              : 'command_bar',
        status: mark.stale ? 'stale' : 'completed',
        startedAt: mark.startedAt,
        finishedAt: mark.finishedAt,
        runtimeEpoch: mark.runtimeEpoch,
        risk: 'execute',
        exitCode: mark.exitCode,
        startLine: mark.startLine,
        endLine: mark.endLine,
        detectionSource: mark.detectionSource,
        outputConfidence: mark.outputConfidence,
        stale: mark.stale,
      });
    }
  }
}

function disposeRecords(records: DecorationRecord[]): void {
  for (const record of records) {
    try {
      record.decoration.dispose();
    } catch {
      // Ignore stale xterm decoration teardown.
    }
    try {
      record.marker.dispose();
    } catch {
      // Ignore stale marker teardown.
    }
  }
}

function disposeDecoration(commandId: string): void {
  const records = decorationsByCommandId.get(commandId);
  if (!records) return;
  decorationsByCommandId.delete(commandId);
  disposeRecords(records);
}

function disposeSelectionOverlay(record: SelectionOverlayRecord): void {
  for (const disposable of record.disposables) {
    try {
      disposable.dispose();
    } catch {
      // Ignore stale xterm listener teardown.
    }
  }
  record.element.remove();
  record.actionsElement?.remove();
}

function clearSelectionDecorations(paneId: string): void {
  const record = selectionOverlaysByPane.get(paneId);
  if (record) {
    selectionOverlaysByPane.delete(paneId);
    disposeSelectionOverlay(record);
  }
}

function trimPaneMarks(paneId: string): void {
  const marks = marksByPane.get(paneId);
  if (!marks || marks.length <= MAX_MARKS_PER_PANE) return;
  const removed = marks.splice(0, marks.length - MAX_MARKS_PER_PANE);
  for (const mark of removed) {
    disposeDecoration(mark.commandId);
  }
}

function pushDecorationRecord(commandId: string, record: DecorationRecord): void {
  const records = decorationsByCommandId.get(commandId) ?? [];
  records.push(record);
  decorationsByCommandId.set(commandId, records);
}

function removeDecorationRecord(commandId: string, record: DecorationRecord): void {
  const records = decorationsByCommandId.get(commandId);
  if (!records) return;
  const next = records.filter((candidate) => candidate !== record);
  if (next.length === 0) {
    decorationsByCommandId.delete(commandId);
  } else {
    decorationsByCommandId.set(commandId, next);
  }
}

type CommandMarkDecorationRole = 'start' | 'body' | 'end' | 'single';

function renderMarkDecoration(element: HTMLElement, mark: TerminalCommandMark, role: CommandMarkDecorationRole): void {
  element.className = [
    'xterm-command-mark-decoration',
    `xterm-command-mark-decoration-${role}`,
    mark.stale ? 'xterm-command-mark-decoration-stale' : '',
  ].filter(Boolean).join(' ');
  element.replaceChildren();
}

function createSelectionActions(term: Terminal, mark: TerminalCommandMark): HTMLElement | null {
  const showHoverActions = useSettingsStore.getState().settings.terminal.commandMarks?.showHoverActions ?? true;
  if (!showHoverActions) return null;

  const actions = document.createElement('div');
  actions.className = 'xterm-command-selection-actions';
  actions.setAttribute('role', 'toolbar');
  setLocalizedText(actions, 'terminal.command_selection.actions', 'aria-label');

  const copy = document.createElement('button');
  copy.type = 'button';
  setLocalizedText(copy, 'terminal.command_selection.copy');
  setLocalizedText(copy, 'terminal.command_selection.copy_title', 'title');
  copy.addEventListener('mousedown', (event) => event.preventDefault());
  copy.addEventListener('click', (event) => {
    event.preventDefault();
    event.stopPropagation();
    const output = getOutputRangeText(term, mark);
    void writeSystemClipboardText(output.text);
  });

  actions.append(copy);
  return actions;
}

function getSelectionOverlayHost(term: Terminal): HTMLElement | null {
  return term.element?.querySelector<HTMLElement>('.xterm-screen') ?? null;
}

function getSelectionOverlayMetrics(term: Terminal, host: HTMLElement): {
  rowTop: number;
  rowHeight: number;
  width: number;
} | null {
  const rowsElement = term.element?.querySelector<HTMLElement>('.xterm-rows') ?? null;
  const hostRect = host.getBoundingClientRect();
  const rowsRect = rowsElement?.getBoundingClientRect() ?? hostRect;
  const rowHeight = rowsRect.height > 0 && term.rows > 0
    ? rowsRect.height / term.rows
    : hostRect.height > 0 && term.rows > 0
      ? hostRect.height / term.rows
      : 0;

  if (!Number.isFinite(rowHeight) || rowHeight <= 0) return null;
  return {
    rowTop: rowsRect.top - hostRect.top,
    rowHeight,
    width: host.clientWidth || hostRect.width,
  };
}

function updateSelectionOverlay(record: SelectionOverlayRecord): boolean {
  const host = record.element.parentElement;
  if (!host) return false;
  const range = getSelectableMarkRange(record.term, record.mark);
  if (!range) return false;
  const metrics = getSelectionOverlayMetrics(record.term, host);
  if (!metrics) return false;

  const viewportStart = record.term.buffer.active.viewportY;
  const viewportEnd = viewportStart + record.term.rows - 1;
  if (range.endLine < viewportStart || range.startLine > viewportEnd) {
    record.element.style.display = 'none';
    if (record.actionsElement) record.actionsElement.style.display = 'none';
    return true;
  }

  const visibleStart = Math.max(range.startLine, viewportStart);
  const visibleEnd = Math.min(range.endLine, viewportEnd);
  const hasTop = range.startLine >= viewportStart;
  const hasBottom = range.endLine <= viewportEnd;
  const top = metrics.rowTop + (visibleStart - viewportStart) * metrics.rowHeight;
  const height = (visibleEnd - visibleStart + 1) * metrics.rowHeight;

  record.element.className = [
    'xterm-command-selection-overlay',
    hasTop ? 'xterm-command-selection-overlay-top' : '',
    hasBottom ? 'xterm-command-selection-overlay-bottom' : '',
    record.mark.stale ? 'xterm-command-selection-overlay-stale' : '',
  ].filter(Boolean).join(' ');
  record.element.style.display = 'block';
  record.element.style.left = '0px';
  record.element.style.top = `${top}px`;
  record.element.style.width = `${metrics.width}px`;
  record.element.style.height = `${height}px`;
  if (record.actionsElement) {
    const actionsHeight = record.actionsElement.offsetHeight || 22;
    const gap = 5;
    const viewportTop = metrics.rowTop;
    const viewportBottom = metrics.rowTop + metrics.rowHeight * record.term.rows;
    const spaceAbove = top - viewportTop;
    const spaceBelow = viewportBottom - (top + height);
    let actionTop = spaceAbove >= actionsHeight + gap || spaceBelow < actionsHeight + gap
      ? top - actionsHeight - gap
      : top + height + gap;

    actionTop = Math.max(viewportTop, Math.min(actionTop, viewportBottom - actionsHeight));
    record.actionsElement.style.display = 'flex';
    record.actionsElement.style.top = `${actionTop}px`;
    record.actionsElement.style.right = '10px';
  }
  return true;
}

function addTerminalOverlayListener(
  record: SelectionOverlayRecord,
  eventName: 'onScroll' | 'onRender' | 'onResize' | 'onWriteParsed' | 'onCursorMove',
): void {
  const subscribe = record.term[eventName] as unknown;
  if (typeof subscribe !== 'function') return;
  const disposable = subscribe(() => {
    if (!updateSelectionOverlay(record)) {
      clearTerminalCommandMarkSelection(record.paneId);
    }
  }) as { dispose: () => void };
  record.disposables.push(disposable);
}

function createSelectionOverlay(term: Terminal, paneId: string, mark: TerminalCommandMark): SelectionOverlayRecord | null {
  const host = getSelectionOverlayHost(term);
  if (!host) return null;
  const element = document.createElement('div');
  const actions = createSelectionActions(term, mark);
  host.append(element);
  if (actions) host.append(actions);

  const record: SelectionOverlayRecord = {
    paneId,
    mark,
    term,
    element,
    actionsElement: actions ?? undefined,
    disposables: [],
  };

  addTerminalOverlayListener(record, 'onScroll');
  addTerminalOverlayListener(record, 'onRender');
  addTerminalOverlayListener(record, 'onResize');
  addTerminalOverlayListener(record, 'onWriteParsed');
  addTerminalOverlayListener(record, 'onCursorMove');

  if (!updateSelectionOverlay(record)) {
    disposeSelectionOverlay(record);
    return null;
  }
  return record;
}

function registerDecorationAtLine(
  term: Terminal,
  paneId: string,
  mark: TerminalCommandMark,
  absoluteLine: number,
  role: CommandMarkDecorationRole,
  primary: boolean,
): DecorationRecord | null {
  const offset = absoluteLine - getAbsoluteCursorLine(term);
  const marker = term.registerMarker(offset);
  if (!marker) return null;
  const decoration = term.registerDecoration({
    marker,
    x: 0,
    width: Math.max(1, term.cols),
    layer: 'top',
  });
  if (!decoration) {
    marker.dispose();
    return null;
  }
  const record: DecorationRecord = { marker, decoration, primary, role };
  decoration.onRender((element) => {
    record.element = element;
    renderMarkDecoration(element, mark, role);
  });
  marker.onDispose(() => {
    removeDecorationRecord(mark.commandId, record);
    if (!primary) return;
    if (selectedMarkByPane.get(paneId) === mark.commandId) {
      clearSelectionDecorations(paneId);
      selectedMarkByPane.delete(paneId);
    }
    const marks = marksByPane.get(paneId);
    if (!marks) return;
    const index = marks.findIndex((candidate) => candidate.commandId === mark.commandId);
    if (index >= 0) {
      marks.splice(index, 1);
      notify();
    }
  });
  return record;
}

export function createTerminalCommandMark(
  term: Terminal,
  paneId: string,
  request: TerminalCommandMarkRequest,
): TerminalCommandMark | null {
  const command = request.command.trim();
  if (!command) return null;
  const settings = useSettingsStore.getState().settings.terminal.commandMarks;
  if (!settings?.enabled) return null;
  if (request.source === 'heuristic' && !settings.heuristicDetection) return null;
  if (term.buffer.active.type === 'alternate' || term.modes.mouseTrackingMode !== 'none') return null;

  const commandLine = getAbsoluteCursorLine(term);
  const startLine = getPromptBlockStartLine(term, commandLine);
  const mark: TerminalCommandMark = {
    commandId: nextCommandId(),
    paneId,
    sessionId: request.sessionId,
    nodeId: request.nodeId,
    command,
    cwd: request.cwd ?? undefined,
    startLine,
    commandLine,
    isClosed: false,
    runtimeEpoch: getAiRuntimeEpoch(),
    detectionSource: request.source,
    confidence: request.source === 'heuristic' || request.source === 'user_input_observed' ? 'low' : 'high',
    outputConfidence: 'unknown',
    collapsed: false,
    stale: false,
    startedAt: Date.now(),
  };

  const record = registerDecorationAtLine(term, paneId, mark, startLine, 'start', true);
  if (!record) return null;

  // The xterm marker/decoration is registered first. Only now mutate the store.
  closeOpenMarks(paneId, startLine, 'next_command', 'high');
  pushDecorationRecord(mark.commandId, record);
  const marks = marksByPane.get(paneId) ?? [];
  marks.push(mark);
  marksByPane.set(paneId, marks);
  trimPaneMarks(paneId);
  notify();
  return mark;
}

export function closeTerminalCommandMarks(
  paneId: string,
  closedBy: TerminalCommandMarkClosedBy,
  outputConfidence: TerminalCommandMarkOutputConfidence = 'unknown',
  stale = false,
): void {
  clearTerminalCommandMarkSelection(paneId);
  const marks = marksByPane.get(paneId) ?? [];
  const now = Date.now();
  for (const mark of marks) {
    if (mark.isClosed) continue;
    mark.isClosed = true;
    mark.closedBy = closedBy;
    mark.outputConfidence = outputConfidence;
    mark.endLine = mark.endLine ?? mark.startLine;
    mark.finishedAt = now;
    mark.durationMs = mark.finishedAt - mark.startedAt;
    mark.stale = stale || mark.stale;
  }
  notify();
}

export function selectTerminalCommandMark(term: Terminal, paneId: string, commandId: string): boolean {
  const mark = (marksByPane.get(paneId) ?? []).find((candidate) => candidate.commandId === commandId);
  if (!mark) return false;
  clearSelectionDecorations(paneId);
  selectedMarkByPane.delete(paneId);
  const overlay = createSelectionOverlay(term, paneId, mark);
  if (!overlay) return false;
  selectedMarkByPane.set(paneId, commandId);
  selectionOverlaysByPane.set(paneId, overlay);
  notify();
  return true;
}

export function selectTerminalCommandMarkAtLine(term: Terminal, paneId: string, absoluteLine: number): boolean {
  const marks = marksByPane.get(paneId) ?? [];
  const mark = [...marks].reverse().find((candidate) => {
    const liveRange = getSelectableMarkRange(term, candidate);
    if (!liveRange) return false;
    return absoluteLine >= liveRange.startLine && absoluteLine <= liveRange.endLine;
  });
  if (!mark) return false;
  return selectTerminalCommandMark(term, paneId, mark.commandId);
}

export function clearTerminalCommandMarkSelection(paneId: string): void {
  const hadSelection = selectedMarkByPane.delete(paneId);
  clearSelectionDecorations(paneId);
  if (hadSelection) notify();
}

export function cleanupTerminalCommandMarks(paneId: string): void {
  clearTerminalCommandMarkSelection(paneId);
  const marks = [...(marksByPane.get(paneId) ?? [])];
  for (const mark of marks) {
    disposeDecoration(mark.commandId);
  }
  marksByPane.delete(paneId);
  notify();
}

export function listTerminalCommandMarks(paneId: string): TerminalCommandMark[] {
  return (marksByPane.get(paneId) ?? []).map((mark) => ({ ...mark }));
}

export function subscribeTerminalCommandMarks(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
