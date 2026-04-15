// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { IDisposable, IDecoration, IMarker, Terminal } from '@xterm/xterm';

import {
  buildRuntimeHighlightRules,
  type RuntimeHighlightRule,
  type SafeCompiledPattern,
  type SafeMatchResult,
} from './highlightPattern';
import {
  collectViewportLogicalLines,
  mapMatchToLogicalLineSlices,
  type CachedLogicalLine,
} from './highlightTextMap';
import type { HighlightRule } from '@/types';

const MAX_HIGHLIGHT_DECORATIONS = 10_000;
const RULE_TIMEOUT_MS = 10;

type BufferSnapshot = {
  type: 'normal' | 'alternate';
  length: number;
  baseY: number;
  viewportY: number;
  cols: number;
  rows: number;
};

type DecorationRecord = {
  marker: IMarker;
  decoration: IDecoration;
  windowKey: string;
};

type ScannedWindowMeta = {
  key: string;
  startRow: number;
  endRow: number;
  centerRow: number;
  lastAccessAt: number;
  logicalLineIds: Set<string>;
};

export type HighlightEngineOptions = {
  onRulesAutoDisabled?: (ruleIds: string[], reason: 'timeout' | 'error') => void;
};

type MatchCandidate = {
  rule: RuntimeHighlightRule;
  index: number;
  length: number;
};

class RegexWorkerClient {
  private worker: Worker | null = null;
  private nextId = 1;
  private coldStart = true;
  private pending = new Map<number, {
    resolve: (result: SafeMatchResult) => void;
    timeoutId: ReturnType<typeof setTimeout>;
  }>();

  constructor() {
    this.ensureWorker();
  }

  private ensureWorker(): Worker {
    if (this.worker) {
      return this.worker;
    }

    const worker = new Worker(new URL('./highlightWorker.ts', import.meta.url), { type: 'module' });
    worker.onmessage = (event: MessageEvent<{ id: number; result: SafeMatchResult }>) => {
      const pending = this.pending.get(event.data.id);
      if (!pending) {
        return;
      }

      this.coldStart = false;
      clearTimeout(pending.timeoutId);
      this.pending.delete(event.data.id);
      pending.resolve(event.data.result);
    };
    this.worker = worker;
    return worker;
  }

  request(pattern: SafeCompiledPattern, line: string, timeoutMs = RULE_TIMEOUT_MS): Promise<SafeMatchResult> {
    const id = this.nextId;
    this.nextId += 1;
    const effectiveTimeoutMs = this.coldStart ? Math.max(timeoutMs, 100) : timeoutMs;

    return new Promise((resolve) => {
      const worker = this.ensureWorker();
      const timeoutId = setTimeout(() => {
        this.pending.delete(id);
        this.restartWorker('error');
        resolve({ ok: false, reason: 'timeout' });
      }, effectiveTimeoutMs);

      this.pending.set(id, { resolve, timeoutId });
      worker.postMessage({ id, pattern, line });
    });
  }

  private restartWorker(reason: 'timeout' | 'error'): void {
    for (const [pendingId, pending] of this.pending.entries()) {
      clearTimeout(pending.timeoutId);
      this.pending.delete(pendingId);
      pending.resolve({ ok: false, reason });
    }
    this.worker?.terminate();
    this.worker = null;
    this.coldStart = true;
  }

  dispose(): void {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timeoutId);
      pending.resolve({ ok: false, reason: 'error' });
    }
    this.pending.clear();
    this.worker?.terminate();
    this.worker = null;
  }
}

function isHexColor(value: string | undefined): value is string {
  return typeof value === 'string' && /^#[0-9a-f]{6}(?:[0-9a-f]{2})?$/i.test(value);
}

function overlap(start: number, end: number, otherStart: number, otherEnd: number): boolean {
  return start < otherEnd && end > otherStart;
}

function applyDecorationClasses(element: HTMLElement, rule: RuntimeHighlightRule): void {
  const renderMode = rule.renderMode ?? 'background';
  const usesOverlayStyles = renderMode !== 'background';

  element.classList.add('xterm-highlight-decoration');
  element.classList.remove('xterm-highlight-background', 'xterm-highlight-underline', 'xterm-highlight-outline');
  if (usesOverlayStyles) {
    element.classList.add(`xterm-highlight-${renderMode}`);
  }
  element.dataset.highlightRuleId = rule.id;

  if (usesOverlayStyles && rule.background) {
    element.style.setProperty('--xterm-highlight-bg', rule.background);
  } else {
    element.style.removeProperty('--xterm-highlight-bg');
  }

  if (usesOverlayStyles && rule.foreground) {
    element.style.setProperty('--xterm-highlight-fg', rule.foreground);
  } else {
    element.style.removeProperty('--xterm-highlight-fg');
  }

  // Background mode uses xterm's native decoration colors instead of a DOM overlay.
  element.style.backgroundColor = '';
}

export const __testOnly = {
  applyDecorationClasses,
};

export class HighlightEngine {
  private term: Terminal;
  private options: HighlightEngineOptions;
  private matcher = new RegexWorkerClient();
  private disposables: IDisposable[] = [];
  private compiledRules: RuntimeHighlightRule[] = [];
  private lineCache = new Map<string, CachedLogicalLine>();
  private decorationIndex = new Map<string, DecorationRecord[]>();
  private logicalLineIndex = new Map<string, Set<string>>();
  private scannedWindows = new Map<string, ScannedWindowMeta>();
  private timeoutCounts = new Map<string, number>();
  private bufferGeneration = 0;
  private viewportSignature = '';
  private lastSnapshot: BufferSnapshot;
  private scheduledScanHandle: number | null = null;
  private activeScanToken = 0;
  private scanInFlight = false;
  private rescanRequested = false;

  constructor(term: Terminal, rules: HighlightRule[], options: HighlightEngineOptions = {}) {
    this.term = term;
    this.options = options;
    this.lastSnapshot = this.captureSnapshot();
    this.updateRules(rules);

    this.disposables.push(
      term.onWriteParsed(() => {
        this.handleBufferMutation();
        this.scheduleViewportScan();
      }),
      term.onResize(() => {
        this.invalidateAll();
        this.scheduleViewportScan();
      }),
      term.onScroll(() => {
        this.scheduleViewportScan();
      }),
      term.buffer.onBufferChange(() => {
        this.invalidateAll();
        this.scheduleViewportScan();
      }),
    );
  }

  updateRules(rules: HighlightRule[]): void {
    this.compiledRules = buildRuntimeHighlightRules(rules)
      .sort((left, right) => right.normalizedPriority - left.normalizedPriority);
    this.timeoutCounts.clear();
    this.invalidateAll();
    this.scheduleViewportScan();
  }

  dispose(): void {
    if (this.scheduledScanHandle !== null) {
      cancelAnimationFrame(this.scheduledScanHandle);
      this.scheduledScanHandle = null;
    }
    this.disposables.forEach((disposable) => disposable.dispose());
    this.disposables = [];
    this.disposeAllDecorations();
    this.matcher.dispose();
  }

  private captureSnapshot(): BufferSnapshot {
    const buffer = this.term.buffer.active;
    return {
      type: buffer.type,
      length: buffer.length,
      baseY: buffer.baseY,
      viewportY: buffer.viewportY,
      cols: this.term.cols,
      rows: this.term.rows,
    };
  }

  private handleBufferMutation(): void {
    const next = this.captureSnapshot();
    const trimmed = next.length === this.lastSnapshot.length && next.baseY > this.lastSnapshot.baseY;
    const reset = next.length < this.lastSnapshot.length || next.baseY < this.lastSnapshot.baseY;
    this.viewportSignature = '';
    if (
      trimmed
      || reset
      || next.type !== this.lastSnapshot.type
      || next.cols !== this.lastSnapshot.cols
      || next.rows !== this.lastSnapshot.rows
    ) {
      this.invalidateAll();
    }
    this.lastSnapshot = next;
  }

  private invalidateAll(): void {
    this.bufferGeneration += 1;
    this.viewportSignature = '';
    this.lineCache.clear();
    this.scannedWindows.clear();
    this.logicalLineIndex.clear();
    this.disposeAllDecorations();
  }

  private disposeAllDecorations(): void {
    for (const records of this.decorationIndex.values()) {
      records.forEach((record) => {
        record.decoration.dispose();
        record.marker.dispose();
      });
    }
    this.decorationIndex.clear();
  }

  private scheduleViewportScan(): void {
    if (this.scanInFlight) {
      this.rescanRequested = true;
      return;
    }

    if (this.scheduledScanHandle !== null) {
      return;
    }

    this.scheduledScanHandle = requestAnimationFrame(() => {
      this.scheduledScanHandle = null;
      void this.scanViewport();
    });
  }

  private async scanViewport(): Promise<void> {
    this.scanInFlight = true;

    const buffer = this.term.buffer.active;
    const viewportStart = buffer.viewportY;
    const viewportEnd = Math.min(buffer.length - 1, viewportStart + this.term.rows - 1);
    const signature = `${this.bufferGeneration}:${buffer.type}:${viewportStart}:${viewportEnd}:${this.term.cols}:${this.term.rows}:${this.compiledRules.map((rule) => `${rule.id}:${rule.enabled}:${rule.priority}:${rule.pattern}:${rule.renderMode ?? 'background'}`).join('|')}`;
    try {
      if (signature === this.viewportSignature) {
        return;
      }
      this.viewportSignature = signature;

      const windowKey = `${buffer.type}:${this.bufferGeneration}:${viewportStart}:${viewportEnd}`;
      const scanToken = this.activeScanToken + 1;
      this.activeScanToken = scanToken;

      const lines = collectViewportLogicalLines(this.term, this.bufferGeneration, viewportStart, viewportEnd);
      const windowMeta: ScannedWindowMeta = {
        key: windowKey,
        startRow: viewportStart,
        endRow: viewportEnd,
        centerRow: Math.floor((viewportStart + viewportEnd) / 2),
        lastAccessAt: Date.now(),
        logicalLineIds: new Set(lines.map((line) => line.id)),
      };
      this.scannedWindows.set(windowKey, windowMeta);

      for (const line of lines) {
        if (scanToken !== this.activeScanToken) {
          return;
        }
        this.lineCache.set(line.id, line);
        this.clearLogicalLineDecorations(line.id);
        const acceptedMatches = await this.resolveAcceptedMatches(line);
        if (scanToken !== this.activeScanToken) {
          return;
        }
        this.applyMatches(line, acceptedMatches, windowKey);
      }

      this.purgeFarWindows(Math.floor((viewportStart + viewportEnd) / 2));
      this.lastSnapshot = this.captureSnapshot();
    } finally {
      this.scanInFlight = false;
      if (this.rescanRequested) {
        this.rescanRequested = false;
        this.scheduleViewportScan();
      }
    }
  }

  private clearLogicalLineDecorations(logicalLineId: string): void {
    const keys = this.logicalLineIndex.get(logicalLineId);
    if (!keys) {
      return;
    }

    for (const key of keys) {
      const records = this.decorationIndex.get(key);
      if (!records) {
        continue;
      }

      records.forEach((record) => {
        record.decoration.dispose();
        record.marker.dispose();
      });
      this.decorationIndex.delete(key);
    }

    this.logicalLineIndex.delete(logicalLineId);
  }

  private async resolveAcceptedMatches(line: CachedLogicalLine): Promise<MatchCandidate[]> {
    const matches: MatchCandidate[] = [];
    const rulesSnapshot = [...this.compiledRules];
    for (const rule of rulesSnapshot) {
      if (!rule.enabled || !rule.compiled) {
        continue;
      }

      const result = await this.matchRule(rule, line.text);
      if (!result.ok) {
        this.handleRuleFailure(rule.id, result.reason);
        continue;
      }

      this.timeoutCounts.delete(rule.id);

      for (const match of result.matches) {
        matches.push({
          rule,
          index: match.index,
          length: match.length,
        });
      }
    }

    const accepted: MatchCandidate[] = [];
    const sorted = [...matches].sort((left, right) => {
      if (right.rule.normalizedPriority !== left.rule.normalizedPriority) {
        return right.rule.normalizedPriority - left.rule.normalizedPriority;
      }
      if (left.index !== right.index) {
        return left.index - right.index;
      }
      return right.length - left.length;
    });

    for (const candidate of sorted) {
      const candidateEnd = candidate.index + candidate.length;
      if (accepted.some((existing) => overlap(candidate.index, candidateEnd, existing.index, existing.index + existing.length))) {
        continue;
      }
      accepted.push(candidate);
    }

    return accepted.sort((left, right) => left.index - right.index);
  }

  private async matchRule(rule: RuntimeHighlightRule, line: string): Promise<SafeMatchResult> {
    if (!rule.compiled) {
      return { ok: false, reason: 'error' };
    }

    return this.matcher.request(rule.compiled, line, RULE_TIMEOUT_MS);
  }

  private handleRuleFailure(ruleId: string, reason: 'timeout' | 'error'): void {
    const count = (this.timeoutCounts.get(ruleId) ?? 0) + 1;
    this.timeoutCounts.set(ruleId, count);
    if (count < 3) {
      return;
    }

    const nextRules = this.compiledRules.map((rule) => (
      rule.id === ruleId
        ? { ...rule, enabled: false }
        : rule
    ));
    this.compiledRules = nextRules;
    this.options.onRulesAutoDisabled?.([ruleId], reason);
    this.invalidateAll();
    this.activeScanToken += 1;
    this.rescanRequested = true;
  }

  private applyMatches(line: CachedLogicalLine, matches: MatchCandidate[], windowKey: string): void {
    for (const match of matches) {
      const slices = mapMatchToLogicalLineSlices(line, match.index, match.length);
      const decorationKey = `${windowKey}:${line.id}:${match.rule.id}:${match.index}:${match.length}`;
      const records: DecorationRecord[] = [];

      for (const slice of slices) {
        const marker = this.createMarkerForRow(slice.row);
        if (!marker) {
          continue;
        }

        const decoration = this.term.registerDecoration({
          marker,
          x: slice.colStart,
          width: Math.max(1, slice.colEnd - slice.colStart),
          backgroundColor: match.rule.renderMode === 'background' && isHexColor(match.rule.background)
            ? match.rule.background
            : undefined,
          foregroundColor: isHexColor(match.rule.foreground) ? match.rule.foreground : undefined,
          layer: match.rule.renderMode === 'background' ? 'bottom' : 'top',
        });
        if (!decoration) {
          marker.dispose();
          continue;
        }

        decoration.onRender((element) => applyDecorationClasses(element, match.rule));
        records.push({ marker, decoration, windowKey });
      }

      if (!records.length) {
        continue;
      }

      this.decorationIndex.set(decorationKey, records);
      const keys = this.logicalLineIndex.get(line.id) ?? new Set<string>();
      keys.add(decorationKey);
      this.logicalLineIndex.set(line.id, keys);
    }
  }

  private createMarkerForRow(row: number): IMarker | undefined {
    const buffer = this.term.buffer.active;
    const absoluteCursorRow = buffer.baseY + buffer.cursorY;
    const offset = row - absoluteCursorRow;
    return this.term.registerMarker(offset);
  }

  private purgeFarWindows(viewportCenter: number): void {
    const decorationCount = Array.from(this.decorationIndex.values()).reduce((total, records) => total + records.length, 0);
    if (decorationCount <= MAX_HIGHLIGHT_DECORATIONS) {
      return;
    }

    const windows = Array.from(this.scannedWindows.values())
      .sort((left, right) => {
        const leftDistance = Math.abs(left.centerRow - viewportCenter);
        const rightDistance = Math.abs(right.centerRow - viewportCenter);
        if (rightDistance !== leftDistance) {
          return rightDistance - leftDistance;
        }
        return left.lastAccessAt - right.lastAccessAt;
      });

    for (const windowMeta of windows) {
      if (Array.from(this.decorationIndex.values()).reduce((total, records) => total + records.length, 0) <= MAX_HIGHLIGHT_DECORATIONS) {
        break;
      }

      for (const logicalLineId of windowMeta.logicalLineIds) {
        this.clearLogicalLineDecorations(logicalLineId);
      }
      this.scannedWindows.delete(windowMeta.key);
    }
  }
}