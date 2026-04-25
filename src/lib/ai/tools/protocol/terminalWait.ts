// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { subscribeTerminalOutput } from '../../../terminalRegistry';
import {
  getRenderedTextDelta,
  readBufferLineCount,
  readBufferRange,
  readBufferStats,
  renderedDeltaFromTextSnapshot,
} from './terminalObserve';

export type TerminalOutputSubscription = {
  getCount: () => number;
  unsubscribe: () => void;
};

export type TerminalWaitReason = 'pattern' | 'prompt' | 'stable' | 'timeout' | 'lost' | 'aborted';

export type TerminalWaitResult = {
  success: boolean;
  output: string;
  reason?: TerminalWaitReason;
  error?: string;
  truncated?: boolean;
};

export interface TerminalWaitOptions {
  sessionId: string;
  timeoutSecs: number;
  stableSecs: number;
  patternRe: RegExp | null;
  startTime: number;
  preSnapshotLineCount?: number | null;
  abortSignal?: AbortSignal;
  existingSubscription?: TerminalOutputSubscription;
  initialRenderedText?: string | null;
  completionPromptRe: RegExp;
  interactiveInputPromptRe: RegExp;
  truncateOutput: (output: string) => { text: string; truncated: boolean };
  emptyOutputTailLines: number;
  fallbackTailLines: number;
  promptGraceMs: number;
  maxAdaptiveStableSecs: number;
}

export function createTerminalOutputSubscription(sessionId: string): TerminalOutputSubscription {
  let outputCounter = 0;
  let unsubscribed = false;
  const rawUnsubscribe = subscribeTerminalOutput(sessionId, () => {
    outputCounter += 1;
  });

  return {
    getCount: () => outputCounter,
    unsubscribe: () => {
      if (unsubscribed) return;
      unsubscribed = true;
      rawUnsubscribe();
    },
  };
}

export async function waitForTerminalOutput(options: TerminalWaitOptions): Promise<TerminalWaitResult> {
  let initialLineCount: number;
  if (options.preSnapshotLineCount != null) {
    initialLineCount = options.preSnapshotLineCount;
  } else {
    const initialSnapshot = await readBufferStats(options.sessionId, 0);
    if (initialSnapshot === null) {
      return { success: false, output: '', error: 'Session not found or buffer unavailable.' };
    }
    initialLineCount = initialSnapshot.totalLines;
  }

  const timeoutMs = options.timeoutSecs * 1000;
  const baseStableMs = options.stableSecs * 1000;
  const maxStableMs = options.maxAdaptiveStableSecs * 1000;
  const pollIntervalMs = 200;

  console.debug(`[AI:ToolExec] waitForTerminalOutput: initial=${initialLineCount}, timeout=${options.timeoutSecs}s, stable=${options.stableSecs}s`);

  const outputSubscription = options.existingSubscription ?? createTerminalOutputSubscription(options.sessionId);

  const result = await new Promise<TerminalWaitReason>((resolve) => {
    let stableTimer: ReturnType<typeof setTimeout> | null = null;
    let promptGraceTimer: ReturnType<typeof setTimeout> | null = null;
    let pollTimer: ReturnType<typeof setInterval> | null = null;
    let settled = false;
    let lastCheckedCounter = 0;
    let lastRenderedDelta = '';
    let checking = false;
    let outputBursts = 0;

    const abortHandler = options.abortSignal ? () => done('aborted') : null;

    const done = (reason: TerminalWaitReason) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeoutTimer);
      if (stableTimer) clearTimeout(stableTimer);
      if (promptGraceTimer) clearTimeout(promptGraceTimer);
      if (pollTimer) clearInterval(pollTimer);
      if (options.abortSignal && abortHandler) options.abortSignal.removeEventListener('abort', abortHandler);
      outputSubscription.unsubscribe();
      console.debug(`[AI:ToolExec] done: reason=${reason}`);
      resolve(reason);
    };

    const timeoutTimer = setTimeout(() => done('timeout'), Math.max(0, timeoutMs - (Date.now() - options.startTime)));

    if (options.abortSignal) {
      if (options.abortSignal.aborted) {
        done('aborted');
        return;
      }
      options.abortSignal.addEventListener('abort', abortHandler!, { once: true });
    }

    pollTimer = setInterval(async () => {
      if (settled || checking) return;
      const outputCounter = outputSubscription.getCount();
      if (outputCounter === lastCheckedCounter) return;

      checking = true;
      lastCheckedCounter = outputCounter;

      let currentLineCount: number | null;
      let currentLines: string[] | null;
      try {
        currentLineCount = await readBufferLineCount(options.sessionId);
        if (currentLineCount === null) {
          currentLines = null;
        } else if (currentLineCount <= initialLineCount) {
          currentLines = [];
        } else {
          currentLines = await readBufferRange(options.sessionId, initialLineCount, currentLineCount - initialLineCount);
        }
      } catch {
        if (!settled) done('lost');
        checking = false;
        return;
      }

      if (settled) { checking = false; return; }

      if (currentLines === null) {
        done('lost');
        checking = false;
        return;
      }

      const delta = (currentLineCount ?? initialLineCount) - initialLineCount;
      const renderedDelta = getRenderedTextDelta(options.sessionId, options.initialRenderedText);
      const hasRenderedDelta = Boolean(renderedDelta && renderedDelta.trim().length > 0 && renderedDelta !== lastRenderedDelta);

      if (options.patternRe && delta > 0) {
        if (currentLines.some(line => options.patternRe!.test(line))) {
          done('pattern');
          checking = false;
          return;
        }
      }

      if (options.patternRe && hasRenderedDelta && renderedDelta) {
        options.patternRe.lastIndex = 0;
        if (options.patternRe.test(renderedDelta)) {
          done('pattern');
          checking = false;
          return;
        }
      }

      if (delta > 0 || hasRenderedDelta) {
        if (hasRenderedDelta && renderedDelta) {
          lastRenderedDelta = renderedDelta;
        }
        outputBursts++;
        if (stableTimer) clearTimeout(stableTimer);
        if (promptGraceTimer) {
          clearTimeout(promptGraceTimer);
          promptGraceTimer = null;
        }
        const adaptiveMs = Math.min(baseStableMs + outputBursts * 200, maxStableMs);
        stableTimer = setTimeout(() => done('stable'), adaptiveMs);

        if (!promptGraceTimer) {
          const tail = delta > 0 ? currentLines.slice(-3).join('\n') : renderedDelta ?? '';
          if (options.completionPromptRe.test(tail) || options.interactiveInputPromptRe.test(tail)) {
            promptGraceTimer = setTimeout(() => {
              if (!settled) done('prompt');
            }, options.promptGraceMs);
          }
        }
      }

      checking = false;
    }, pollIntervalMs);
  });

  if (result === 'aborted') {
    return { success: false, output: '', reason: result, error: 'Generation was stopped.' };
  }

  const finalSnapshot = await readBufferStats(options.sessionId, options.fallbackTailLines);
  if (finalSnapshot === null || result === 'lost') {
    return { success: false, output: '', reason: result, error: 'Session became unavailable during wait.' };
  }

  const finalLineCount = finalSnapshot.totalLines;

  if (finalLineCount < initialLineCount) {
    const { text, truncated } = options.truncateOutput(finalSnapshot.lines.join('\n'));
    return { success: true, output: `⚠ Buffer was cleared or reset during command execution. Showing current buffer content:\n${text}`, reason: result, truncated };
  }

  let newLines = finalLineCount > initialLineCount
    ? await readBufferRange(options.sessionId, initialLineCount, finalLineCount - initialLineCount)
    : [];

  if (newLines === null) {
    return { success: false, output: '', reason: result, error: 'Session became unavailable during wait.' };
  }

  if (result === 'prompt' && newLines.length > 0) {
    const lastLine = newLines[newLines.length - 1];
    if (options.completionPromptRe.test(lastLine)) {
      newLines = newLines.slice(0, -1);
    }
  }

  if (newLines.length === 0) {
    const renderedDelta = renderedDeltaFromTextSnapshot(options.sessionId, options.initialRenderedText, { success: true, output: '' }, options.truncateOutput);
    if (renderedDelta) {
      return { ...renderedDelta, reason: result };
    }

    if (result !== 'timeout') {
      const tail = finalSnapshot.lines.slice(-options.emptyOutputTailLines);
      if (tail.length > 0) {
        const { text, truncated } = options.truncateOutput(tail.join('\n'));
        return { success: true, output: `No new terminal output detected. Here are the last ${tail.length} lines of the terminal:\n${text}`, reason: result, truncated };
      }
    }
    const msg = result === 'timeout'
      ? `No new output after ${options.timeoutSecs}s. The command may be waiting for input or still running.`
      : 'No new output detected.';
    return { success: true, output: msg, reason: result };
  }

  console.debug(`[AI:ToolExec] captured ${newLines.length} new lines (reason=${result})`);
  const { text, truncated } = options.truncateOutput(newLines.join('\n'));
  return { success: true, output: text, reason: result, truncated };
}
