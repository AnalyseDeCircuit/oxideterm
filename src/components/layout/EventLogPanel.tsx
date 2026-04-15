// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * EventLogPanel — VS Code Problems-style event log (tab view)
 *
 * Aggregates connection lifecycle, reconnect pipeline, and node state events
 * into a filterable, scrollable event log.
 */

import { useRef, useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Info,
  AlertTriangle,
  XCircle,
  Trash2,
  Filter,
  Search,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import { Button } from '../ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '../ui/tooltip';
import { useEventLogStore, type EventLogEntry, type EventSeverity, type EventCategory } from '../../store/eventLogStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';

// ============================================================================
// Severity icon component
// ============================================================================

const SeverityIcon = ({ severity }: { severity: EventSeverity }) => {
  switch (severity) {
    case 'info':
      return <Info className="h-3.5 w-3.5 text-blue-400 shrink-0" />;
    case 'warn':
      return <AlertTriangle className="h-3.5 w-3.5 text-yellow-400 shrink-0" />;
    case 'error':
      return <XCircle className="h-3.5 w-3.5 text-red-400 shrink-0" />;
  }
};

// ============================================================================
// Category badge component
// ============================================================================

const CategoryBadge = ({ category }: { category: EventCategory }) => {
  const { t } = useTranslation();
  const label = t(`event_log.category.${category}`);
  const colorClass =
    category === 'connection' ? 'bg-emerald-500/15 text-emerald-400'
    : category === 'reconnect' ? 'bg-amber-500/15 text-amber-400'
    : 'bg-blue-500/15 text-blue-400';

  return (
    <span className={cn('text-[10px] font-medium px-1.5 py-0.5 rounded-md shrink-0', colorClass)}>
      {label}
    </span>
  );
};

// ============================================================================
// Timestamp formatter
// ============================================================================

function formatTimestamp(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}

// ============================================================================
// Event row
// ============================================================================

const EventRow = ({ entry }: { entry: EventLogEntry }) => {
  const { t } = useTranslation();
  const getNode = useSessionTreeStore((s) => s.getNode);

  // Resolve display name for node
  const nodeName = useMemo(() => {
    if (!entry.nodeId) return undefined;
    const node = getNode(entry.nodeId);
    return node?.displayName || node?.host;
  }, [entry.nodeId, getNode]);

  // Resolve title — use i18n key if it looks like one, otherwise raw string
  const titleText = useMemo(() => {
    if (entry.title.startsWith('event_log.')) {
      return t(entry.title);
    }
    return entry.title;
  }, [entry.title, t]);

  // Resolve detail
  const detailText = useMemo(() => {
    if (!entry.detail) return undefined;
    if (entry.detail.startsWith('event_log.')) {
      // Handle parameterized detail like "event_log.events.affected_children:3"
      const colonIdx = entry.detail.indexOf(':');
      if (colonIdx > -1) {
        const key = entry.detail.substring(0, colonIdx);
        const param = parseInt(entry.detail.substring(colonIdx + 1), 10);
        return t(key, { count: isNaN(param) ? 0 : param });
      }
      return t(entry.detail);
    }
    // For reconnect phases, translate the phase name
    if (entry.source === 'reconnect_orchestrator' && entry.detail) {
      const phaseKey = `event_log.phase.${entry.detail}`;
      const translated = t(phaseKey);
      return translated !== phaseKey ? translated : entry.detail;
    }
    return entry.detail;
  }, [entry.detail, entry.source, t]);

  return (
    <div className="flex items-center gap-2 px-3 py-1 hover:bg-theme-bg-hover text-xs font-mono group min-h-[24px]">
      <span className="text-theme-text-muted w-[60px] shrink-0 tabular-nums">
        {formatTimestamp(entry.timestamp)}
      </span>
      <SeverityIcon severity={entry.severity} />
      <CategoryBadge category={entry.category} />
      {nodeName && (
        <span className="text-theme-accent shrink-0 max-w-[120px] truncate">
          {nodeName}
        </span>
      )}
      <span className="text-theme-text truncate">
        {titleText}
      </span>
      {detailText && (
        <span className="text-theme-text-muted truncate">
          — {detailText}
        </span>
      )}
    </div>
  );
};

// ============================================================================
// Main panel component
// ============================================================================

export const EventLogPanel = () => {
  const { t } = useTranslation();
  const entries = useEventLogStore((s) => s.entries);
  const filter = useEventLogStore((s) => s.filter);
  const setFilter = useEventLogStore((s) => s.setFilter);
  const clear = useEventLogStore((s) => s.clear);
  const markRead = useEventLogStore((s) => s.markRead);

  const scrollRef = useRef<HTMLDivElement>(null);
  const shouldAutoScroll = useRef(true);

  // Mark read on mount
  useEffect(() => {
    markRead();
  }, [markRead]);

  // Filtered entries
  const filteredEntries = useMemo(() => {
    return entries.filter((entry) => {
      if (filter.severity !== 'all' && entry.severity !== filter.severity) return false;
      if (filter.category !== 'all' && entry.category !== filter.category) return false;
      if (filter.search) {
        const q = filter.search.toLowerCase();
        const matches =
          entry.title.toLowerCase().includes(q) ||
          (entry.detail?.toLowerCase().includes(q)) ||
          (entry.nodeId?.toLowerCase().includes(q)) ||
          (entry.connectionId?.toLowerCase().includes(q));
        if (!matches) return false;
      }
      return true;
    });
  }, [entries, filter]);

  // Count by severity (use filteredEntries so counts match visible list)
  const counts = useMemo(() => {
    const c = { info: 0, warn: 0, error: 0 };
    for (const e of filteredEntries) {
      c[e.severity]++;
    }
    return c;
  }, [filteredEntries]);

  // Auto-scroll to bottom on new entries
  useEffect(() => {
    if (shouldAutoScroll.current && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [filteredEntries.length]);

  // Detect if user scrolled away from bottom
  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    shouldAutoScroll.current = scrollHeight - scrollTop - clientHeight < 30;
  }, []);

  // Severity filter cycle
  const cycleSeverity = useCallback(() => {
    const cycle: Array<EventSeverity | 'all'> = ['all', 'error', 'warn', 'info'];
    const idx = cycle.indexOf(filter.severity);
    setFilter({ severity: cycle[(idx + 1) % cycle.length] });
  }, [filter.severity, setFilter]);

  // Category filter cycle
  const cycleCategory = useCallback(() => {
    const cycle: Array<EventCategory | 'all'> = ['all', 'connection', 'reconnect', 'node'];
    const idx = cycle.indexOf(filter.category);
    setFilter({ category: cycle[(idx + 1) % cycle.length] });
  }, [filter.category, setFilter]);

  return (
    <div className="h-full flex flex-col bg-theme-bg select-none">
      {/* Header */}
      <div className="flex items-center gap-1.5 px-3 py-1 bg-theme-bg-panel border-b border-theme-border shrink-0">
        {/* Title */}
        <span className="text-xs font-semibold text-theme-text mr-1">
          {t('event_log.title')}
        </span>

        {/* Severity counts */}
        <div className="flex items-center gap-1.5 text-[10px] mr-2">
          <span className="flex items-center gap-0.5 text-red-400">
            <XCircle className="h-3 w-3" />
            {counts.error}
          </span>
          <span className="flex items-center gap-0.5 text-yellow-400">
            <AlertTriangle className="h-3 w-3" />
            {counts.warn}
          </span>
          <span className="flex items-center gap-0.5 text-blue-400">
            <Info className="h-3 w-3" />
            {counts.info}
          </span>
        </div>

        {/* Spacer */}
        <div className="flex-1" />

        {/* Filter: severity */}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant={filter.severity !== 'all' ? 'secondary' : 'ghost'}
              size="icon"
              className="h-5 w-5"
              onClick={cycleSeverity}
            >
              <Filter className="h-3 w-3" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            {t('event_log.filter_severity')}: {filter.severity === 'all' ? t('event_log.all') : t(`event_log.severity.${filter.severity}`)}
          </TooltipContent>
        </Tooltip>
        {filter.severity !== 'all' && (
          <span className="text-[10px] text-theme-accent">
            {t(`event_log.severity.${filter.severity}`)}
          </span>
        )}

        {/* Filter: category */}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant={filter.category !== 'all' ? 'secondary' : 'ghost'}
              size="icon"
              className="h-5 w-5"
              onClick={cycleCategory}
            >
              <Search className="h-3 w-3" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>
            {t('event_log.filter_category')}: {filter.category === 'all' ? t('event_log.all') : t(`event_log.category.${filter.category}`)}
          </TooltipContent>
        </Tooltip>
        {filter.category !== 'all' && (
          <span className="text-[10px] text-theme-accent">
            {t(`event_log.category.${filter.category}`)}
          </span>
        )}

        {/* Clear */}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-5 w-5"
              onClick={clear}
              disabled={entries.length === 0}
            >
              <Trash2 className="h-3 w-3" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t('event_log.clear')}</TooltipContent>
        </Tooltip>

      </div>

      {/* Event list */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto overflow-x-hidden"
        onScroll={handleScroll}
      >
        {filteredEntries.length === 0 ? (
          <div className="flex items-center justify-center h-full text-xs text-theme-text-muted">
            {t('event_log.empty')}
          </div>
        ) : (
          filteredEntries.map((entry) => (
            <EventRow key={entry.id} entry={entry} />
          ))
        )}
      </div>
    </div>
  );
};
