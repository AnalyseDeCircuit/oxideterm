// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import {
  AlertTriangle,
  Archive,
  ChevronDown,
  ChevronUp,
  Database,
  Loader2,
  RefreshCw,
  Search,
  Trash2,
  X,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../../lib/api';
import { cn } from '../../lib/utils';
import { useConfirm } from '../../hooks/useConfirm';
import { getFontFamily } from '../../lib/fontFamily';
import { useSettingsStore } from '../../store/settingsStore';
import { parseTerminalLineText, type ParsedAnsiLine, type ParsedAnsiSpan } from '../../lib/terminal/ansiSgr';
import {
  buildScrollbackMinimapBins,
  type ScrollbackMinimapVisibleRange,
} from '../../lib/gpu';
import { GpuChartCanvas } from '../gpu/GpuChartCanvas';
import type {
  ArchivedHistoryExcerpt,
  BufferStats,
  CommandFact,
  HistorySearchMatch,
  SearchOptions,
  TerminalSearchModelSnapshot,
  TerminalLine,
} from '../../types';

const PAGE_SIZE = 800;
const MAX_CACHED_PAGES = 10;
const PROTECTED_VIEWPORT_PAGES = 1;
const STATS_REFRESH_MS = 2000;
const SEARCH_POLL_MS = 250;
const EXCERPT_CONTEXT_LINES = 6;
const DEFAULT_ROW_HEIGHT = 22;
const MINIMAP_BIN_COUNT = 128;

interface ScrollbackViewerProps {
  sessionId: string;
  nodeId: string;
  isOpen: boolean;
  searchId?: string | null;
  initialMatch?: HistorySearchMatch | null;
  onClose: () => void;
}

interface CachedLine {
  globalLine: number;
  line: TerminalLine;
  parsed: ParsedAnsiLine;
}

interface CachedPage {
  key: number;
  startGlobalLine: number;
  endGlobalLine: number;
  lines: CachedLine[];
  lastAccessedAt: number;
}

interface HighlightRange {
  start: number;
  end: number;
  active?: boolean;
}

type CommandFactRowRole = 'single' | 'start' | 'body' | 'end';

interface CommandFactRowMarker {
  fact: CommandFact;
  role: CommandFactRowRole;
  selected: boolean;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function getBaseGlobalLine(stats: BufferStats): number {
  return Math.max(0, Number(stats.total_lines) - stats.current_lines);
}

function getPageKey(globalLine: number): number {
  return Math.floor(globalLine / PAGE_SIZE) * PAGE_SIZE;
}

function getProtectedPageKeys(visiblePageKeys: Set<number>): Set<number> {
  const protectedKeys = new Set<number>();
  for (const key of visiblePageKeys) {
    for (let offset = -PROTECTED_VIEWPORT_PAGES; offset <= PROTECTED_VIEWPORT_PAGES; offset += 1) {
      protectedKeys.add(key + offset * PAGE_SIZE);
    }
  }
  return protectedKeys;
}

export function enforceScrollbackPageCacheLimit<T extends { lastAccessedAt: number }>(
  pages: Map<number, T>,
  protectedPageKeys: Set<number>,
  maxCachedPages = MAX_CACHED_PAGES,
): Map<number, T> {
  if (pages.size <= maxCachedPages) return pages;

  const next = new Map(pages);
  const candidates = Array.from(next.entries())
    .filter(([key]) => !protectedPageKeys.has(key))
    .sort((left, right) => left[1].lastAccessedAt - right[1].lastAccessedAt);

  for (const [key] of candidates) {
    if (next.size <= maxCachedPages) break;
    next.delete(key);
  }

  return next;
}

function historyMatchKey(match: HistorySearchMatch): string {
  return [
    match.source,
    match.chunk_id ?? 'live',
    match.line_number,
    match.column_start,
    match.column_end,
  ].join(':');
}

function isHotMatchInWindow(match: HistorySearchMatch, stats: BufferStats | null): boolean {
  if (!stats || match.source !== 'hot') return false;
  const baseGlobalLine = getBaseGlobalLine(stats);
  return match.line_number >= baseGlobalLine && match.line_number < baseGlobalLine + stats.current_lines;
}

function splitSpanByHighlights(span: ParsedAnsiSpan, ranges: HighlightRange[]): Array<{
  text: string;
  style: React.CSSProperties;
  highlighted: boolean;
  active: boolean;
}> {
  const boundaries = new Set<number>([span.start, span.end]);
  for (const range of ranges) {
    const start = clamp(range.start, span.start, span.end);
    const end = clamp(range.end, span.start, span.end);
    if (start < end) {
      boundaries.add(start);
      boundaries.add(end);
    }
  }

  const sorted = Array.from(boundaries).sort((left, right) => left - right);
  const segments = [];
  for (let index = 0; index < sorted.length - 1; index += 1) {
    const start = sorted[index];
    const end = sorted[index + 1];
    if (start >= end) continue;
    const matchingRange = ranges.find((range) => start < range.end && end > range.start);
    segments.push({
      text: span.text.slice(start - span.start, end - span.start),
      style: span.style,
      highlighted: Boolean(matchingRange),
      active: Boolean(matchingRange?.active),
    });
  }
  return segments;
}

function renderParsedLine(parsed: ParsedAnsiLine, ranges: HighlightRange[]) {
  if (parsed.spans.length === 0) {
    return <span>{parsed.plainText || '\u00a0'}</span>;
  }

  return parsed.spans.flatMap((span, spanIndex) => {
    const segments = splitSpanByHighlights(span, ranges);
    return segments.map((segment, segmentIndex) => (
      <span
        key={`${spanIndex}-${segmentIndex}`}
        style={{
          ...segment.style,
          ...(segment.highlighted
            ? {
                backgroundColor: segment.active ? 'var(--theme-accent)' : 'rgba(234, 179, 8, 0.32)',
                color: segment.active ? 'var(--theme-bg)' : undefined,
              }
            : null),
        }}
      >
        {segment.text}
      </span>
    ));
  });
}

interface ScrollbackMinimapProps {
  enabled: boolean;
  stats: BufferStats | null;
  visibleRange: ScrollbackMinimapVisibleRange | null;
  matches: HistorySearchMatch[];
  activeMatchIndex: number;
  onJumpToRow: (rowIndex: number) => void;
  title: string;
}

const ScrollbackMinimap: React.FC<ScrollbackMinimapProps> = ({
  enabled,
  stats,
  visibleRange,
  matches,
  activeMatchIndex,
  onJumpToRow,
  title,
}) => {
  const bins = useMemo(() => buildScrollbackMinimapBins({
    stats,
    visibleRange,
    searchMatches: matches,
    activeMatchIndex,
    binCount: MINIMAP_BIN_COUNT,
  }), [activeMatchIndex, matches, stats, visibleRange]);

  return (
    <div className="relative w-3 shrink-0 border-l border-theme-border/40 bg-theme-bg-panel/30">
      <GpuChartCanvas
        kind="vertical"
        enabled={enabled}
        bins={bins}
        className="cursor-pointer"
        title={title}
        onClickBin={(binIndex) => {
          if (!stats || stats.current_lines <= 0) return;
          const ratio = binIndex / Math.max(1, bins.length);
          onJumpToRow(clamp(Math.floor(ratio * stats.current_lines), 0, stats.current_lines - 1));
        }}
      />
    </div>
  );
};

export const ScrollbackViewer: React.FC<ScrollbackViewerProps> = ({
  sessionId,
  nodeId,
  isOpen,
  searchId = null,
  initialMatch = null,
  onClose,
}) => {
  const { t } = useTranslation();
  const terminalSettings = useSettingsStore((state) => state.settings.terminal);
  const gpuCanvasEnabled = useSettingsStore((state) => state.settings.experimental?.gpuCanvas ?? false);
  const { confirm, ConfirmDialog } = useConfirm();
  const scrollRef = useRef<HTMLDivElement>(null);
  const generationRef = useRef(0);
  const searchIdRef = useRef<string | null>(null);
  const initialMatchKeyRef = useRef<string | null>(null);
  const [stats, setStats] = useState<BufferStats | null>(null);
  const statsRef = useRef<BufferStats | null>(null);
  const [pages, setPages] = useState<Map<number, CachedPage>>(() => new Map());
  const pagesRef = useRef<Map<number, CachedPage>>(new Map());
  const [loadingPages, setLoadingPages] = useState<Set<number>>(() => new Set());
  const loadingPagesRef = useRef<Set<number>>(new Set());
  const [initialLoading, setInitialLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [regex, setRegex] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [searchLoading, setSearchLoading] = useState(false);
  const [matches, setMatches] = useState<HistorySearchMatch[]>([]);
  const matchesRef = useRef<HistorySearchMatch[]>([]);
  const [commandFacts, setCommandFacts] = useState<CommandFact[]>([]);
  const commandFactsRef = useRef<CommandFact[]>([]);
  const [selectedCommandFactId, setSelectedCommandFactId] = useState<string | null>(null);
  const [activeMatchIndex, setActiveMatchIndex] = useState(-1);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [excerpt, setExcerpt] = useState<ArchivedHistoryExcerpt | null>(null);
  const [excerptLoading, setExcerptLoading] = useState(false);
  const visiblePageKeysRef = useRef<Set<number>>(new Set());
  const terminalFontFamily = useMemo(
    () => getFontFamily(terminalSettings.fontFamily, terminalSettings.customFontFamily),
    [terminalSettings.customFontFamily, terminalSettings.fontFamily],
  );
  const rowHeight = useMemo(
    () => Math.max(16, Math.ceil((terminalSettings.fontSize || 13) * (terminalSettings.lineHeight || 1.2))),
    [terminalSettings.fontSize, terminalSettings.lineHeight],
  );

  const rowVirtualizer = useVirtualizer({
    count: stats?.current_lines ?? 0,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight || DEFAULT_ROW_HEIGHT,
    overscan: 40,
  });
  const rowVirtualizerRef = useRef(rowVirtualizer);
  rowVirtualizerRef.current = rowVirtualizer;

  const baseGlobalLine = stats ? getBaseGlobalLine(stats) : 0;

  const setPageState = useCallback((updater: (current: Map<number, CachedPage>) => Map<number, CachedPage>) => {
    setPages((current) => {
      const next = updater(current);
      pagesRef.current = next;
      return next;
    });
  }, []);

  const setLoadingPageState = useCallback((updater: (current: Set<number>) => Set<number>) => {
    setLoadingPages((current) => {
      const next = updater(current);
      loadingPagesRef.current = next;
      return next;
    });
  }, []);

  const clearSearchState = useCallback(() => {
    setMatches([]);
    matchesRef.current = [];
    setActiveMatchIndex(-1);
    setSearchError(null);
    setExcerpt(null);
    setExcerptLoading(false);
  }, []);

  const loadCommandFactsForStats = useCallback(async (nextStats: BufferStats) => {
    if (nextStats.current_lines <= 0) {
      commandFactsRef.current = [];
      setCommandFacts([]);
      setSelectedCommandFactId(null);
      return;
    }
    const base = getBaseGlobalLine(nextStats);
    const end = base + nextStats.current_lines - 1;
    try {
      const facts = await api.getCommandFacts(sessionId, base, end);
      commandFactsRef.current = facts;
      setCommandFacts(facts);
      setSelectedCommandFactId((current) => {
        if (!current || facts.some((fact) => fact.factId === current)) return current;
        return null;
      });
    } catch (caught) {
      console.debug('[ScrollbackViewer] failed to load command facts:', caught);
    }
  }, [sessionId]);

  const refreshStats = useCallback(async (options: { initial?: boolean; resetErrors?: boolean } = {}) => {
    if (!isOpen) return;
    const wasNearBottom = (() => {
      const element = scrollRef.current;
      if (!element) return true;
      return element.scrollHeight - element.scrollTop - element.clientHeight < rowHeight * 4;
    })();

    try {
      if (options.initial) setInitialLoading(true);
      if (options.resetErrors) setError(null);
      const nextStats = await api.getBufferStats(sessionId);
      void loadCommandFactsForStats(nextStats);

      setStats((previous) => {
        const previousBase = previous ? getBaseGlobalLine(previous) : 0;
        const nextBase = getBaseGlobalLine(nextStats);
        const changedWindow = !previous
          || previousBase !== nextBase
          || previous.current_lines !== nextStats.current_lines
          || previous.total_lines !== nextStats.total_lines;

        if (changedWindow) {
          generationRef.current += 1;
          const resetWindow = !previous
            || nextBase < previousBase
            || nextStats.total_lines < previous.total_lines
            || nextStats.current_lines === 0;
          if (resetWindow) {
            setLoadingPageState(() => new Set());
          }
          const nextEnd = nextBase + nextStats.current_lines;
          setPageState((current) => {
            if (resetWindow) return new Map();
            const pruned = new Map<number, CachedPage>();
            for (const [key, page] of current) {
              if (page.endGlobalLine > nextBase && page.startGlobalLine < nextEnd) {
                pruned.set(key, page);
              }
            }
            return pruned;
          });

          if (nextStats.current_lines === 0) {
            clearSearchState();
          }
        }

        statsRef.current = nextStats;
        return nextStats;
      });

      if (wasNearBottom && nextStats.current_lines > 0) {
        requestAnimationFrame(() => rowVirtualizerRef.current.scrollToIndex(nextStats.current_lines - 1, { align: 'end' }));
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      if (options.initial) setInitialLoading(false);
    }
  }, [clearSearchState, isOpen, loadCommandFactsForStats, rowHeight, sessionId, setLoadingPageState, setPageState]);

  const loadPageForGlobalLine = useCallback(async (globalLine: number) => {
    const currentStats = statsRef.current;
    if (!currentStats || currentStats.current_lines <= 0) return;
    const base = getBaseGlobalLine(currentStats);
    const end = base + currentStats.current_lines;
    if (globalLine < base || globalLine >= end) return;

    const pageKey = getPageKey(globalLine);
    if (pagesRef.current.has(pageKey) || loadingPagesRef.current.has(pageKey)) return;

    const requestStartGlobal = Math.max(pageKey, base);
    const hotStart = requestStartGlobal - base;
    if (hotStart < 0 || hotStart >= currentStats.current_lines) return;

    const requestEndGlobal = Math.min(pageKey + PAGE_SIZE, end);
    const count = Math.min(PAGE_SIZE, currentStats.current_lines - hotStart, requestEndGlobal - requestStartGlobal);
    if (count <= 0) return;

    const generation = generationRef.current;
    setLoadingPageState((current) => new Set(current).add(pageKey));

    try {
      const lines = await api.getScrollBuffer(sessionId, hotStart, count);
      if (generation !== generationRef.current) return;

      const latestStats = statsRef.current;
      if (!latestStats) return;
      const latestBase = getBaseGlobalLine(latestStats);
      if (requestStartGlobal < latestBase || requestStartGlobal >= latestBase + latestStats.current_lines) return;

      const cachedLines = lines.map((line, index) => ({
        globalLine: requestStartGlobal + index,
        line,
        parsed: parseTerminalLineText(line.text, line.ansi_text),
      }));

      setPageState((current) => {
        const next = new Map(current);
        next.set(pageKey, {
          key: pageKey,
          startGlobalLine: requestStartGlobal,
          endGlobalLine: requestStartGlobal + cachedLines.length,
          lines: cachedLines,
          lastAccessedAt: Date.now(),
        });
        return enforceScrollbackPageCacheLimit(next, getProtectedPageKeys(visiblePageKeysRef.current));
      });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      void refreshStats();
    } finally {
      setLoadingPageState((current) => {
        const next = new Set(current);
        next.delete(pageKey);
        return next;
      });
    }
  }, [refreshStats, sessionId, setLoadingPageState, setPageState]);

  const getCachedLine = useCallback((globalLine: number): CachedLine | null => {
    const pageKey = getPageKey(globalLine);
    const page = pages.get(pageKey);
    if (!page || globalLine < page.startGlobalLine || globalLine >= page.endGlobalLine) return null;
    return page.lines[globalLine - page.startGlobalLine] ?? null;
  }, [pages]);

  const cancelActiveSearch = useCallback(() => {
    const activeSearchId = searchIdRef.current;
    if (activeSearchId) {
      searchIdRef.current = null;
      void api.closeTerminalSearchModel(activeSearchId);
    }
  }, []);

  const applySearchModelSnapshot = useCallback((snapshot: TerminalSearchModelSnapshot) => {
    if (snapshot.session_id !== sessionId) return;

    matchesRef.current = snapshot.matches;
    setMatches(snapshot.matches);
    setActiveMatchIndex(snapshot.active_match_index ?? (snapshot.matches.length > 0 ? 0 : -1));
    setSearchQuery(snapshot.query);
    setSearchError(snapshot.error ?? snapshot.excerpt_error ?? null);
    setSearchLoading(snapshot.loading);
    setExcerpt(snapshot.excerpt ?? null);

    const activeMatch = snapshot.active_match
      ?? (typeof snapshot.active_match_index === 'number' ? snapshot.matches[snapshot.active_match_index] : null);
    if (!activeMatch || activeMatch.source !== 'hot') return;
    const currentStats = statsRef.current;
    if (!currentStats || !isHotMatchInWindow(activeMatch, currentStats)) return;
    const rowIndex = activeMatch.line_number - getBaseGlobalLine(currentStats);
    void loadPageForGlobalLine(activeMatch.line_number);
    requestAnimationFrame(() => rowVirtualizerRef.current.scrollToIndex(rowIndex, { align: 'center' }));
  }, [loadPageForGlobalLine, sessionId]);

  const loadExcerptForMatch = useCallback(async (match: HistorySearchMatch) => {
    if (match.source !== 'cold' || !match.chunk_id) {
      setExcerpt(null);
      return;
    }

    setExcerptLoading(true);
    try {
      const nextExcerpt = await api.getArchivedHistoryExcerpt(
        sessionId,
        match.chunk_id,
        match.line_number,
        EXCERPT_CONTEXT_LINES,
      );
      setExcerpt(nextExcerpt);
    } catch (caught) {
      setSearchError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setExcerptLoading(false);
    }
  }, [sessionId]);

  const activateMatch = useCallback((index: number) => {
    const list = matchesRef.current;
    if (list.length === 0) return;
    const nextIndex = ((index % list.length) + list.length) % list.length;
    const match = list[nextIndex];
    setActiveMatchIndex(nextIndex);

    if (match.source === 'cold') {
      void loadExcerptForMatch(match);
      return;
    }

    setExcerpt(null);
    const currentStats = statsRef.current;
    if (!currentStats || !isHotMatchInWindow(match, currentStats)) return;
    const rowIndex = match.line_number - getBaseGlobalLine(currentStats);
    void loadPageForGlobalLine(match.line_number);
    requestAnimationFrame(() => rowVirtualizerRef.current.scrollToIndex(rowIndex, { align: 'center' }));
  }, [loadExcerptForMatch, loadPageForGlobalLine]);

  useEffect(() => {
    if (!isOpen) {
      initialMatchKeyRef.current = null;
      return;
    }
    if (!initialMatch) return;
    if (initialMatch.source !== 'cold' && !statsRef.current) return;
    const key = historyMatchKey(initialMatch);
    if (initialMatchKeyRef.current === key) return;
    initialMatchKeyRef.current = key;

    cancelActiveSearch();
    const nextMatches = [initialMatch];
    matchesRef.current = nextMatches;
    setMatches(nextMatches);
    setActiveMatchIndex(0);
    setSearchQuery(initialMatch.matched_text ?? '');
    setSearchError(null);

    if (initialMatch.source === 'cold') {
      void loadExcerptForMatch(initialMatch);
      return;
    }

    setExcerpt(null);
    const currentStats = statsRef.current;
    if (!currentStats || !isHotMatchInWindow(initialMatch, currentStats)) return;
    const rowIndex = initialMatch.line_number - getBaseGlobalLine(currentStats);
    void loadPageForGlobalLine(initialMatch.line_number);
    requestAnimationFrame(() => rowVirtualizerRef.current.scrollToIndex(rowIndex, { align: 'center' }));
  }, [cancelActiveSearch, initialMatch, isOpen, loadExcerptForMatch, loadPageForGlobalLine, stats]);

  useEffect(() => {
    if (!isOpen || !searchId) return;
    let cancelled = false;

    api.getTerminalSearchModelSnapshot(searchId)
      .then((snapshot) => {
        if (cancelled) return;
        applySearchModelSnapshot(snapshot);
      })
      .catch((caught) => {
        if (cancelled) return;
        setSearchError(caught instanceof Error ? caught.message : String(caught));
      });

    return () => {
      cancelled = true;
    };
  }, [applySearchModelSnapshot, isOpen, searchId]);

  const runSearch = useCallback(async () => {
    const query = searchQuery.trim();
    cancelActiveSearch();
    clearSearchState();
    if (!query) return;

    const options: SearchOptions = {
      query,
      case_sensitive: caseSensitive,
      regex,
      whole_word: wholeWord,
      max_matches: 1000,
    };

    setSearchLoading(true);
    try {
      const { search_id } = await api.startTerminalSearchModel(sessionId, options);
      searchIdRef.current = search_id;
      let latestSnapshot: TerminalSearchModelSnapshot | null = null;

      while (searchIdRef.current === search_id) {
        const snapshot = await api.getTerminalSearchModelSnapshot(search_id);
        if (searchIdRef.current !== search_id) return;
        latestSnapshot = snapshot;
        applySearchModelSnapshot(snapshot);
        if (snapshot.done) break;
        await new Promise((resolve) => setTimeout(resolve, SEARCH_POLL_MS));
      }

      const finalSnapshot = latestSnapshot ?? await api.getTerminalSearchModelSnapshot(search_id);
      if (searchIdRef.current === search_id) {
        if (finalSnapshot.matches.length > 0 && typeof finalSnapshot.active_match_index !== 'number') {
          const selected = await api.selectTerminalSearchMatch(search_id, 0);
          applySearchModelSnapshot(selected);
        } else {
          applySearchModelSnapshot(finalSnapshot);
        }
      }
    } catch (caught) {
      setSearchError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setSearchLoading(false);
      const activeSearchId = searchIdRef.current;
      if (activeSearchId) {
        searchIdRef.current = null;
        void api.closeTerminalSearchModel(activeSearchId);
      }
    }
  }, [applySearchModelSnapshot, cancelActiveSearch, caseSensitive, clearSearchState, regex, searchQuery, sessionId, wholeWord]);

  const handleClear = useCallback(async () => {
    const confirmed = await confirm({
      title: t('terminal.scrollback_viewer.clear_confirm_title'),
      description: t('terminal.scrollback_viewer.clear_confirm_description'),
      confirmLabel: t('terminal.scrollback_viewer.clear_confirm_action'),
      variant: 'danger',
    });
    if (!confirmed) return;

    try {
      await api.clearBuffer(sessionId);
      generationRef.current += 1;
      setPageState(() => new Map());
      commandFactsRef.current = [];
      setCommandFacts([]);
      setSelectedCommandFactId(null);
      clearSearchState();
      await refreshStats({ resetErrors: true });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [clearSearchState, confirm, refreshStats, sessionId, setPageState, t]);

  useEffect(() => {
    if (!isOpen) {
      cancelActiveSearch();
      return;
    }

    generationRef.current += 1;
    statsRef.current = null;
    pagesRef.current = new Map();
    loadingPagesRef.current = new Set();
    commandFactsRef.current = [];
    setStats(null);
    setPageState(() => new Map());
    setLoadingPageState(() => new Set());
    setCommandFacts([]);
    setSelectedCommandFactId(null);
    setError(null);
    clearSearchState();
    void refreshStats({ initial: true, resetErrors: true });
    const interval = window.setInterval(() => {
      void refreshStats();
    }, STATS_REFRESH_MS);

    return () => {
      window.clearInterval(interval);
      cancelActiveSearch();
    };
  }, [cancelActiveSearch, clearSearchState, isOpen, refreshStats, setLoadingPageState, setPageState]);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [isOpen, onClose]);

  useEffect(() => {
    if (!isOpen || !stats || stats.current_lines === 0) return;
    void loadPageForGlobalLine(baseGlobalLine + stats.current_lines - 1);
  }, [baseGlobalLine, isOpen, loadPageForGlobalLine, stats]);

  const virtualItems = rowVirtualizer.getVirtualItems();
  const visibleRangeSignature = virtualItems.map((item) => item.index).join(',');
  const visibleRange = useMemo<ScrollbackMinimapVisibleRange | null>(() => {
    if (virtualItems.length === 0) return null;
    return {
      startIndex: virtualItems[0].index,
      endIndex: virtualItems[virtualItems.length - 1].index,
    };
  }, [visibleRangeSignature]);

  useEffect(() => {
    if (!isOpen || !stats || stats.current_lines === 0) return;
    const visiblePageKeys = new Set<number>();
    for (const item of virtualItems) {
      const globalLine = baseGlobalLine + item.index;
      visiblePageKeys.add(getPageKey(globalLine));
      void loadPageForGlobalLine(globalLine);
    }
    visiblePageKeysRef.current = visiblePageKeys;
    const protectedKeys = getProtectedPageKeys(visiblePageKeys);
    const now = Date.now();
    setPageState((current) => {
      let changed = false;
      const touched = new Map(current);
      for (const key of visiblePageKeys) {
        const page = touched.get(key);
        if (page) {
          touched.set(key, { ...page, lastAccessedAt: now });
          changed = true;
        }
      }
      const limited = enforceScrollbackPageCacheLimit(touched, protectedKeys);
      return changed || limited.size !== current.size ? limited : current;
    });
  }, [baseGlobalLine, isOpen, loadPageForGlobalLine, setPageState, stats, visibleRangeSignature]);

  const hotMatchesByLine = useMemo(() => {
    const map = new Map<number, HighlightRange[]>();
    matches.forEach((match, index) => {
      if (match.source !== 'hot') return;
      const range = {
        start: match.column_start,
        end: match.column_end,
        active: index === activeMatchIndex,
      };
      map.set(match.line_number, [...(map.get(match.line_number) ?? []), range]);
    });
    return map;
  }, [activeMatchIndex, matches]);

  const commandFactsByLine = useMemo(() => {
    const map = new Map<number, CommandFactRowMarker>();
    if (!stats || stats.current_lines <= 0) return map;
    const hotStart = getBaseGlobalLine(stats);
    const hotEnd = hotStart + stats.current_lines - 1;

    for (const fact of commandFacts) {
      if (fact.status === 'open' || typeof fact.endGlobalLine !== 'number') continue;
      const start = clamp(fact.startGlobalLine, hotStart, hotEnd);
      const end = clamp(fact.endGlobalLine, hotStart, hotEnd);
      if (start > end) continue;
      for (let line = start; line <= end; line += 1) {
        const single = start === end;
        map.set(line, {
          fact,
          role: single ? 'single' : line === start ? 'start' : line === end ? 'end' : 'body',
          selected: fact.factId === selectedCommandFactId,
        });
      }
    }

    return map;
  }, [commandFacts, selectedCommandFactId, stats]);

  const liveMatchCount = matches.filter((match) => match.source === 'hot').length;
  const archiveMatchCount = matches.filter((match) => match.source === 'cold').length;

  if (!isOpen) return null;

  return (
    <div
      className="absolute inset-0 z-[70] flex flex-col bg-theme-bg text-theme-text border border-theme-border/70 shadow-2xl"
      data-node-id={nodeId}
      role="dialog"
      aria-label={t('terminal.scrollback_viewer.title')}
    >
      <div className="flex items-center gap-2 border-b border-theme-border bg-theme-bg-panel/95 px-3 py-2">
        <Database className="h-4 w-4 text-theme-accent shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold text-theme-text">{t('terminal.scrollback_viewer.title')}</h2>
            {stats && (
              <span className="text-[11px] text-theme-text-muted tabular-nums">
                {t('terminal.scrollback_viewer.stats', {
                  current: stats.current_lines,
                  total: stats.total_lines,
                  max: stats.max_lines,
                  memory: stats.memory_usage_mb.toFixed(1),
                })}
              </span>
            )}
          </div>
          <p className="text-[11px] text-theme-text-muted leading-tight">
            {t('terminal.scrollback_viewer.live_hint')}
          </p>
        </div>

        <form
          className="flex min-w-[320px] items-center gap-1"
          onSubmit={(event) => {
            event.preventDefault();
            void runSearch();
          }}
        >
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-theme-text-muted" />
            <input
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder={t('terminal.scrollback_viewer.search_placeholder')}
              className="h-8 w-full rounded border border-theme-border bg-theme-bg px-7 text-xs text-theme-text outline-none focus:border-theme-accent"
            />
            {searchLoading && (
              <Loader2 className="absolute right-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 animate-spin text-theme-accent" />
            )}
          </div>
          <button
            type="button"
            onClick={() => setCaseSensitive((value) => !value)}
            className={cn(
              'h-8 rounded border px-2 text-[11px] transition-colors',
              caseSensitive ? 'border-theme-accent text-theme-accent' : 'border-theme-border text-theme-text-muted hover:text-theme-text',
            )}
            title={t('terminal.scrollback_viewer.case_sensitive')}
          >
            Aa
          </button>
          <button
            type="button"
            onClick={() => setRegex((value) => !value)}
            className={cn(
              'h-8 rounded border px-2 text-[11px] transition-colors',
              regex ? 'border-theme-accent text-theme-accent' : 'border-theme-border text-theme-text-muted hover:text-theme-text',
            )}
            title={t('terminal.scrollback_viewer.regex')}
          >
            .*
          </button>
          <button
            type="button"
            onClick={() => setWholeWord((value) => !value)}
            className={cn(
              'h-8 rounded border px-2 text-[11px] transition-colors',
              wholeWord ? 'border-theme-accent text-theme-accent' : 'border-theme-border text-theme-text-muted hover:text-theme-text',
            )}
            title={t('terminal.scrollback_viewer.whole_word')}
          >
            W
          </button>
          <button type="submit" className="h-8 rounded bg-theme-accent px-3 text-xs font-medium text-theme-bg hover:opacity-90">
            {t('terminal.scrollback_viewer.search')}
          </button>
        </form>

        <button
          type="button"
          onClick={() => activateMatch(activeMatchIndex - 1)}
          disabled={matches.length === 0}
          className="rounded p-1.5 text-theme-text-muted transition-colors hover:bg-theme-bg-hover hover:text-theme-text disabled:opacity-40"
          title={t('terminal.scrollback_viewer.previous_match')}
        >
          <ChevronUp className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={() => activateMatch(activeMatchIndex + 1)}
          disabled={matches.length === 0}
          className="rounded p-1.5 text-theme-text-muted transition-colors hover:bg-theme-bg-hover hover:text-theme-text disabled:opacity-40"
          title={t('terminal.scrollback_viewer.next_match')}
        >
          <ChevronDown className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={() => void refreshStats({ resetErrors: true })}
          className="rounded p-1.5 text-theme-text-muted transition-colors hover:bg-theme-bg-hover hover:text-theme-text"
          title={t('terminal.scrollback_viewer.refresh')}
        >
          <RefreshCw className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={() => void handleClear()}
          className="rounded p-1.5 text-theme-text-muted transition-colors hover:bg-red-500/10 hover:text-red-400"
          title={t('terminal.scrollback_viewer.clear')}
        >
          <Trash2 className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={onClose}
          className="rounded p-1.5 text-theme-text-muted transition-colors hover:bg-theme-bg-hover hover:text-theme-text"
          title={t('terminal.scrollback_viewer.close')}
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      {(error || searchError) && (
        <div className="flex items-center gap-2 border-b border-theme-border/60 bg-red-500/10 px-3 py-1.5 text-xs text-red-300">
          <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">{error ?? searchError}</span>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <div
          ref={scrollRef}
          className="min-w-0 flex-1 overflow-auto"
          style={{
            fontFamily: terminalFontFamily,
            fontSize: terminalSettings.fontSize,
            lineHeight: `${terminalSettings.lineHeight}`,
          }}
        >
          {initialLoading ? (
            <div className="space-y-2 p-4">
              {Array.from({ length: 10 }).map((_, index) => (
                <div key={index} className="h-4 animate-pulse rounded bg-theme-bg-hover" style={{ width: `${45 + (index % 5) * 9}%` }} />
              ))}
            </div>
          ) : stats?.current_lines === 0 ? (
            <div className="flex h-full flex-col items-center justify-center gap-2 text-theme-text-muted">
              <Database className="h-8 w-8 opacity-50" />
              <p className="text-sm">{t('terminal.scrollback_viewer.empty_title')}</p>
              <p className="text-xs">{t('terminal.scrollback_viewer.empty_description')}</p>
            </div>
          ) : (
            <div
              className="relative w-full"
              style={{ height: rowVirtualizer.getTotalSize() }}
            >
              {virtualItems.map((virtualItem) => {
                const globalLine = baseGlobalLine + virtualItem.index;
                const cachedLine = getCachedLine(globalLine);
                const pageLoading = loadingPages.has(getPageKey(globalLine));
                const ranges = hotMatchesByLine.get(globalLine) ?? [];
                const commandFactMarker = commandFactsByLine.get(globalLine);

                return (
                  <div
                    key={virtualItem.key}
                    data-testid="scrollback-live-row"
                    className={cn(
                      'absolute left-0 top-0 grid w-full grid-cols-[3.25rem_minmax(0,1fr)] gap-2 whitespace-pre px-2 text-theme-text',
                      commandFactMarker && 'cursor-pointer border-l-2 border-theme-accent/50 bg-theme-accent/5',
                      commandFactMarker?.selected && 'bg-theme-accent/12',
                      (commandFactMarker?.role === 'start' || commandFactMarker?.role === 'single') && 'border-t border-theme-accent/40',
                      (commandFactMarker?.role === 'end' || commandFactMarker?.role === 'single') && 'border-b border-theme-accent/40',
                      commandFactMarker?.fact.status === 'stale' && 'opacity-70',
                    )}
                    onClick={() => {
                      if (commandFactMarker) setSelectedCommandFactId(commandFactMarker.fact.factId);
                    }}
                    title={commandFactMarker?.fact.command ?? undefined}
                    style={{
                      height: virtualItem.size,
                      transform: `translateY(${virtualItem.start}px)`,
                    }}
                  >
                    <span
                      className="select-none text-right text-theme-text-muted/60 tabular-nums"
                      style={{ fontFamily: 'inherit', fontSize: '0.92em' }}
                    >
                      {globalLine + 1}
                    </span>
                    <pre
                      data-testid="scrollback-live-line-text"
                      className="m-0 min-w-0 overflow-visible"
                      style={{
                        fontFamily: 'inherit',
                        fontSize: 'inherit',
                        lineHeight: 'inherit',
                      }}
                    >
                      {cachedLine
                        ? renderParsedLine(cachedLine.parsed, ranges)
                        : pageLoading
                          ? <span className="text-theme-text-muted">{t('terminal.scrollback_viewer.loading_line')}</span>
                          : '\u00a0'}
                    </pre>
                  </div>
                );
              })}
            </div>
          )}
        </div>
        <ScrollbackMinimap
          enabled={gpuCanvasEnabled}
          stats={stats}
          visibleRange={visibleRange}
          matches={matches}
          activeMatchIndex={activeMatchIndex}
          title={t('terminal.scrollback_viewer.minimap')}
          onJumpToRow={(rowIndex) => rowVirtualizerRef.current.scrollToIndex(rowIndex, { align: 'center' })}
        />

        {(matches.length > 0 || excerpt || excerptLoading) && (
          <aside className="flex w-80 shrink-0 flex-col border-l border-theme-border bg-theme-bg-panel/70">
            <div className="border-b border-theme-border px-3 py-2">
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium text-theme-text">{t('terminal.scrollback_viewer.matches')}</span>
                <span className="text-[11px] text-theme-text-muted">
                  {activeMatchIndex >= 0 ? `${activeMatchIndex + 1}/${matches.length}` : matches.length}
                </span>
              </div>
              <div className="mt-1 flex gap-1.5 text-[11px]">
                <span className="rounded border border-theme-border px-1.5 py-0.5 text-theme-text-muted">
                  {t('terminal.scrollback_viewer.live_buffer_badge')}: {liveMatchCount}
                </span>
                <span className="rounded border border-theme-border px-1.5 py-0.5 text-theme-text-muted">
                  {t('terminal.scrollback_viewer.archive_badge')}: {archiveMatchCount}
                </span>
              </div>
            </div>

            <div className="min-h-0 flex-1 overflow-auto">
              {matches.map((match, index) => (
                <button
                  key={historyMatchKey(match)}
                  type="button"
                  onClick={() => activateMatch(index)}
                  className={cn(
                    'block w-full border-b border-theme-border/50 px-3 py-2 text-left text-xs transition-colors',
                    index === activeMatchIndex ? 'bg-theme-accent/15' : 'hover:bg-theme-bg-hover',
                  )}
                >
                  <div className="mb-1 flex items-center gap-1.5">
                    {match.source === 'cold' ? <Archive className="h-3 w-3 text-amber-400" /> : <Database className="h-3 w-3 text-theme-accent" />}
                    <span className="font-medium text-theme-text">
                      {match.source === 'cold'
                        ? t('terminal.scrollback_viewer.archive_badge')
                        : t('terminal.scrollback_viewer.live_buffer_badge')}
                    </span>
                    <span className="ml-auto text-theme-text-muted tabular-nums">
                      {t('terminal.scrollback_viewer.line_number', { line: match.line_number + 1 })}
                    </span>
                  </div>
                  <p
                    className="line-clamp-2 break-all text-theme-text-muted"
                    style={{ fontFamily: terminalFontFamily }}
                  >
                    {match.line_content}
                  </p>
                </button>
              ))}

              {(excerptLoading || excerpt) && (
                <div className="border-t border-theme-border p-3">
                  <div className="mb-2 flex items-center gap-1.5 text-xs font-medium text-theme-text">
                    <Archive className="h-3.5 w-3.5 text-amber-400" />
                    {t('terminal.scrollback_viewer.archive_excerpt')}
                    {excerptLoading && <Loader2 className="h-3.5 w-3.5 animate-spin text-theme-accent" />}
                  </div>
                  {excerpt?.lines.map((line) => {
                    const parsed = parseTerminalLineText(line.text, line.ansi_text);
                    return (
                      <div
                        key={line.line_number}
                        className={cn('grid grid-cols-[4rem_1fr] gap-2 py-0.5 text-[12px] leading-5', line.is_match && 'bg-theme-accent/15')}
                        style={{ fontFamily: terminalFontFamily }}
                      >
                        <span className="text-right text-theme-text-muted/70 tabular-nums">{line.line_number + 1}</span>
                        <pre
                          className="m-0 overflow-hidden text-theme-text"
                          style={{ fontFamily: 'inherit', lineHeight: 'inherit' }}
                        >
                          {renderParsedLine(parsed, line.is_match ? [{ start: 0, end: parsed.plainText.length, active: true }] : [])}
                        </pre>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </aside>
        )}
      </div>

      {ConfirmDialog}
    </div>
  );
};

export default ScrollbackViewer;
