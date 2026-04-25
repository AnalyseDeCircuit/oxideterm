// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { api } from '../../../api';
import {
  findPaneBySessionId,
  getTerminalBuffer,
  readScreen,
} from '../../../terminalRegistry';
import type { ScreenSnapshot } from '../../../../types';

export type TerminalBufferSource = 'backend-scroll-buffer' | 'rendered-buffer';

export interface TerminalBufferSnapshot {
  totalLines: number;
  lines: string[];
  source: TerminalBufferSource;
}

export interface TerminalPromptDetection {
  kind: 'shell' | 'password' | 'passphrase' | 'unknown';
  text: string;
}

export interface TerminalObserveRequest {
  sessionId: string;
  includeScreen?: boolean;
  includeRecentOutput?: boolean;
  recentLines?: number;
}

export interface TerminalObserveData {
  sessionId: string;
  screen?: ScreenSnapshot;
  recentOutput?: TerminalBufferSnapshot;
  detectedPrompt?: TerminalPromptDetection;
  waitingForInput?: boolean;
}

export type WaitFormattingResult = {
  success: boolean;
  output: string;
  error?: string;
  truncated?: boolean;
};

const EMPTY_OUTPUT_TAIL_LINES = 20;
const PASSWORD_PROMPT_RE = /(?:^|\n).*(?:\[sudo\]\s*)?(?:password|密码)(?:\s+for\s+[^\n:]+)?\s*:\s*$/i;
const PASSPHRASE_PROMPT_RE = /(?:^|\n).*(?:passphrase|口令)(?:\s+for\s+[^\n:]+)?\s*:\s*$/i;
const SHELL_PROMPT_RE = /(?:^|\n)[\w@.\-~:\/\[\]\(\) ]*[\$#>%]\s*$/;

export function detectTerminalPrompt(text: string): TerminalPromptDetection | undefined {
  const tail = text.split('\n').slice(-3).join('\n');
  const passwordMatch = PASSWORD_PROMPT_RE.exec(tail);
  if (passwordMatch) {
    return { kind: 'password', text: passwordMatch[0].trim() };
  }

  const passphraseMatch = PASSPHRASE_PROMPT_RE.exec(tail);
  if (passphraseMatch) {
    return { kind: 'passphrase', text: passphraseMatch[0].trim() };
  }

  const shellMatch = SHELL_PROMPT_RE.exec(tail);
  if (shellMatch) {
    return { kind: 'shell', text: shellMatch[0].trim() };
  }

  return undefined;
}

export function readRenderedBufferText(sessionId: string): string | null {
  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return null;
  }

  const buffer = getTerminalBuffer(paneId);
  return typeof buffer === 'string' ? buffer : null;
}

export function readRenderedBufferLines(sessionId: string): string[] | null {
  const buffer = readRenderedBufferText(sessionId);
  return typeof buffer === 'string' ? buffer.split('\n') : null;
}

export function getRenderedTextDelta(sessionId: string, initialText: string | null | undefined): string | null {
  if (initialText == null) {
    return null;
  }

  const currentText = readRenderedBufferText(sessionId);
  if (currentText == null || currentText === initialText) {
    return null;
  }

  if (currentText.startsWith(initialText)) {
    return currentText.slice(initialText.length);
  }

  return currentText.split('\n').slice(-EMPTY_OUTPUT_TAIL_LINES).join('\n');
}

export function readRenderedBufferTail(sessionId: string, maxLines: number): TerminalBufferSnapshot | null {
  const lines = readRenderedBufferLines(sessionId);
  if (!lines) {
    return null;
  }

  return {
    totalLines: lines.length,
    lines: maxLines > 0 ? lines.slice(-maxLines) : [],
    source: 'rendered-buffer',
  };
}

export function renderedDeltaFromLineCount(
  sessionId: string,
  initialLineCount: number | null | undefined,
  fallback: WaitFormattingResult,
  options: { completionPromptRe: RegExp; truncateOutput: (output: string) => { text: string; truncated: boolean } },
): WaitFormattingResult | null {
  if (!fallback.success || initialLineCount == null) {
    return null;
  }

  const lines = readRenderedBufferLines(sessionId);
  if (!lines) {
    return null;
  }

  if (lines.length < initialLineCount) {
    const { text, truncated } = options.truncateOutput(lines.join('\n'));
    return { success: true, output: `⚠ Buffer was cleared or reset during command execution. Showing current buffer content:\n${text}`, truncated };
  }

  let newLines = lines.slice(initialLineCount);
  if (newLines.length === 0) {
    return null;
  }
  const lastLine = newLines[newLines.length - 1];
  if (options.completionPromptRe.test(lastLine)) {
    newLines = newLines.slice(0, -1);
  }
  if (newLines.length === 0) {
    return null;
  }

  const { text, truncated } = options.truncateOutput(newLines.join('\n'));
  return { success: true, output: text, truncated };
}

export function renderedDeltaFromTextSnapshot(
  sessionId: string,
  initialText: string | null | undefined,
  fallback: WaitFormattingResult,
  truncateOutput: (output: string) => { text: string; truncated: boolean },
): WaitFormattingResult | null {
  if (!fallback.success) {
    return null;
  }

  const delta = getRenderedTextDelta(sessionId, initialText);
  if (!delta || delta.trim().length === 0) {
    return null;
  }

  const { text, truncated } = truncateOutput(delta);
  return { success: true, output: text, truncated };
}

export function searchRenderedBuffer(
  sessionId: string,
  query: string,
  options: { caseSensitive: boolean; regex: boolean; maxResults: number },
): { lines: string[]; error?: string } | null {
  const bufferLines = readRenderedBufferLines(sessionId);
  if (!bufferLines) {
    return null;
  }

  let matcher: (line: string) => number;
  if (options.regex) {
    let pattern: RegExp;
    try {
      pattern = new RegExp(query, options.caseSensitive ? '' : 'i');
    } catch {
      return { lines: [], error: `Invalid regex pattern: ${query}` };
    }
    matcher = (line) => {
      const match = pattern.exec(line);
      pattern.lastIndex = 0;
      return match?.index ?? -1;
    };
  } else {
    const needle = options.caseSensitive ? query : query.toLowerCase();
    matcher = (line) => {
      const haystack = options.caseSensitive ? line : line.toLowerCase();
      return haystack.indexOf(needle);
    };
  }

  const matches: string[] = [];
  for (let index = 0; index < bufferLines.length && matches.length < options.maxResults; index += 1) {
    const line = bufferLines[index];
    const column = matcher(line);
    if (column >= 0) {
      matches.push(`[rendered] L${index + 1}:${column + 1}: ${line}`);
    }
  }

  return { lines: matches };
}

export async function readBufferStats(sessionId: string, tailLines: number): Promise<TerminalBufferSnapshot | null> {
  try {
    const stats = await api.getBufferStats(sessionId);
    const totalLines = Math.max(0, stats.current_lines);
    const tailCount = Math.min(totalLines, Math.max(0, tailLines));
    const lines = tailCount > 0
      ? (await api.getScrollBuffer(sessionId, Math.max(0, totalLines - tailCount), tailCount)).map(line => line.text)
      : [];
    return { totalLines, lines, source: 'backend-scroll-buffer' };
  } catch {
    // Fall back to the rendered xterm buffer when backend history is unavailable.
  }

  return readRenderedBufferTail(sessionId, tailLines);
}

export async function readBufferTail(sessionId: string, maxLines: number): Promise<TerminalBufferSnapshot | null> {
  return readBufferStats(sessionId, maxLines);
}

export async function readBufferLineCount(sessionId: string): Promise<number | null> {
  const snapshot = await readBufferStats(sessionId, 0);
  return snapshot?.totalLines ?? null;
}

export async function readBufferRange(sessionId: string, startLine: number, count: number): Promise<string[] | null> {
  if (count <= 0) {
    return [];
  }

  try {
    const lines = await api.getScrollBuffer(sessionId, startLine, count);
    return lines.map(line => line.text);
  } catch {
    // Fall back to rendered buffer slicing below.
  }

  const lines = readRenderedBufferLines(sessionId);
  return lines ? lines.slice(startLine, startLine + count) : null;
}

export function readTerminalScreen(sessionId: string): ScreenSnapshot | null {
  const paneId = findPaneBySessionId(sessionId);
  return paneId ? readScreen(paneId) : null;
}

export async function terminalObserve(request: TerminalObserveRequest): Promise<TerminalObserveData | null> {
  const recentLines = Math.max(0, request.recentLines ?? 200);
  const recentOutput = request.includeRecentOutput
    ? readRenderedBufferTail(request.sessionId, recentLines) ?? await readBufferTail(request.sessionId, recentLines)
    : undefined;
  const screen = request.includeScreen ? readTerminalScreen(request.sessionId) ?? undefined : undefined;
  const promptSource = screen?.lines.join('\n') ?? recentOutput?.lines.join('\n') ?? readRenderedBufferText(request.sessionId) ?? '';
  const detectedPrompt = promptSource ? detectTerminalPrompt(promptSource) : undefined;

  if (!recentOutput && !screen) {
    return null;
  }

  return {
    sessionId: request.sessionId,
    ...(screen ? { screen } : {}),
    ...(recentOutput ? { recentOutput } : {}),
    ...(detectedPrompt ? { detectedPrompt } : {}),
    waitingForInput: detectedPrompt?.kind === 'password' || detectedPrompt?.kind === 'passphrase',
  };
}

export function formatScreenSnapshot(snapshot: ScreenSnapshot): string {
  const bufferMode = snapshot.isAlternateBuffer ? 'alternate buffer (TUI mode)' : 'normal buffer';
  const header = `[Screen ${snapshot.cols}×${snapshot.rows} | Cursor: (${snapshot.cursorX},${snapshot.cursorY}) | ${bufferMode}]`;
  const separator = '─'.repeat(Math.min(snapshot.cols, 80));
  const lineWidth = String(snapshot.rows).length;
  const numberedLines = snapshot.lines.map((line: string, i: number) => {
    const num = String(i + 1).padStart(lineWidth);
    return `${num}│${line}`;
  });

  return `${header}\n${separator}\n${numberedLines.join('\n')}`;
}
