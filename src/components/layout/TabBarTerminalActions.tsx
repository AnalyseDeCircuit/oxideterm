// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * TabBarTerminalActions — terminal-specific actions in the tab bar
 *
 * Renders in the right-fixed area of the TabBar, completely outside
 * the terminal content area. Only shown when the active tab is a
 * terminal or local_terminal.
 *
 * Actions (left to right):
 *   - Split horizontal / vertical (local_terminal only)
 *   - Separator
 *   - Start recording / Open .cast file
 *   - REC indicator when recording is active
 */

import React, { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Circle, FilePlay, Square, Trash2,
  SplitSquareHorizontal, SplitSquareVertical,
  Radio,
} from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { readTextFile } from '@tauri-apps/plugin-fs';
import { cn } from '../../lib/utils';
import { useRecordingStore } from '../../store/recordingStore';
import { useAppStore, findPaneById } from '../../store/appStore';
import { useLocalTerminalStore } from '../../store/localTerminalStore';
import { useSessionTreeStore } from '../../store/sessionTreeStore';
import { useBroadcastStore } from '../../store/broadcastStore';
import { getAllEntries } from '../../lib/terminalRegistry';
import { MAX_PANES_PER_TAB } from '../../types';
import type { Tab, SplitDirection } from '../../types';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
  DropdownMenuSeparator,
  DropdownMenuLabel,
} from '../ui/dropdown-menu';

type TabBarTerminalActionsProps = {
  activeTab: Tab;
};

/* ──────────────────────────────────────────────────────────────────────
 * BroadcastDropdown — target selector for broadcast input
 * ──────────────────────────────────────────────────────────────────── */

type BroadcastDropdownProps = {
  entries: Array<{
    paneId: string;
    tabId: string;
    sessionId: string;
    terminalType: 'terminal' | 'local_terminal';
  }>;
  targets: Set<string>;
  enabled: boolean;
  activePaneId: string | undefined;
  sessions: Map<string, { name: string; host: string }>;
  tabs: Tab[];
  toggleTarget: (paneId: string) => void;
  disableBroadcast: () => void;
  onRefresh: () => void;
  t: (key: string, opts?: Record<string, unknown>) => string;
};

const BroadcastDropdown: React.FC<BroadcastDropdownProps> = ({
  entries,
  targets,
  enabled,
  activePaneId,
  sessions,
  tabs,
  toggleTarget,
  disableBroadcast,
  onRefresh,
  t,
}) => {
  // Separate current terminal from other targets
  const otherEntries = useMemo(
    () => entries.filter(e => e.paneId !== activePaneId),
    [entries, activePaneId],
  );

  const allSelected = otherEntries.length > 0 && otherEntries.every(e => targets.has(e.paneId));

  /** Build human-readable label for an entry */
  const entryLabel = useCallback(
    (e: (typeof entries)[0]) => {
      if (e.terminalType === 'local_terminal') {
        // Find tab title for local terminals
        const tab = tabs.find(tb => tb.id === e.tabId);
        return tab?.title ?? t('terminal.broadcast.local_terminal');
      }
      // SSH terminal — use session name
      const node = useSessionTreeStore.getState().getNodeByTerminalId(e.sessionId);
      if (node?.displayName?.trim()) {
        return node.displayName.trim();
      }
      const session = sessions.get(e.sessionId);
      return session ? `${session.name} (${session.host})` : e.sessionId.slice(0, 8);
    },
    [sessions, tabs, t],
  );

  const handleSelectAll = useCallback(() => {
    const { addTarget } = useBroadcastStore.getState();
    for (const e of otherEntries) addTarget(e.paneId);
  }, [otherEntries]);

  const handleDeselectAll = useCallback(() => {
    disableBroadcast();
  }, [disableBroadcast]);

  return (
    <DropdownMenu onOpenChange={(open) => { if (open) onRefresh(); }}>
      <DropdownMenuTrigger asChild>
        <button
          className={cn(
            'relative p-1.5 mx-1 rounded-md transition-colors',
            enabled
              ? 'text-orange-400 bg-orange-500/15 hover:bg-orange-500/25'
              : 'text-theme-text-muted hover:text-theme-accent hover:bg-theme-bg-hover',
          )}
          title={t('terminal.broadcast.broadcast_input')}
        >
          <Radio className="h-3.5 w-3.5" />
          {enabled && targets.size > 0 && (
            <span className="absolute -top-1 -right-1 min-w-[14px] h-[14px] flex items-center justify-center rounded-full bg-orange-500 text-[9px] font-bold text-white leading-none px-0.5">
              {targets.size}
            </span>
          )}
        </button>
      </DropdownMenuTrigger>

      <DropdownMenuContent align="end" className="min-w-[220px]">
        <DropdownMenuLabel className="text-xs">
          {t('terminal.broadcast.select_targets')}
        </DropdownMenuLabel>
        <DropdownMenuSeparator />

        {/* Current terminal indicator (non-selectable) */}
        {activePaneId && (() => {
          const currentEntry = entries.find(e => e.paneId === activePaneId);
          if (!currentEntry) return null;
          return (
            <div className="flex items-center gap-2 px-2 py-1.5 text-xs opacity-60">
              <span className="h-1.5 w-1.5 rounded-full bg-orange-400 animate-pulse flex-shrink-0" />
              <span className="flex-1 truncate">{entryLabel(currentEntry)}</span>
              <span className="text-[10px] px-1.5 py-0.5 rounded-md font-medium bg-orange-500/15 text-orange-400">
                {t('terminal.broadcast.current')}
              </span>
            </div>
          );
        })()}

        {activePaneId && otherEntries.length > 0 && <DropdownMenuSeparator />}

        {otherEntries.length === 0 ? (
          <div className="px-2 py-3 text-xs text-theme-text-muted text-center">
            {t('terminal.broadcast.no_targets')}
          </div>
        ) : (
          <>
            {otherEntries.map(entry => (
              <DropdownMenuCheckboxItem
                key={entry.paneId}
                checked={targets.has(entry.paneId)}
                onCheckedChange={() => toggleTarget(entry.paneId)}
                onSelect={e => e.preventDefault()}
                className="text-xs gap-2"
              >
                <span className="flex-1 truncate">{entryLabel(entry)}</span>
                <span
                  className={cn(
                    'text-[10px] px-1.5 py-0.5 rounded-md font-medium',
                    entry.terminalType === 'local_terminal'
                      ? 'bg-emerald-500/15 text-emerald-400'
                      : 'bg-blue-500/15 text-blue-400',
                  )}
                >
                  {entry.terminalType === 'local_terminal' ? t('terminal.typeLocal') : t('terminal.typeSsh')}
                </span>
              </DropdownMenuCheckboxItem>
            ))}

            <DropdownMenuSeparator />

            <div className="flex items-center justify-between px-2 py-1.5">
              <button
                onClick={allSelected ? handleDeselectAll : handleSelectAll}
                className="text-[11px] text-theme-text-muted hover:text-theme-accent transition-colors"
              >
                {allSelected
                  ? t('terminal.broadcast.deselect_all')
                  : t('terminal.broadcast.select_all')}
              </button>
              {enabled && (
                <span className="text-[10px] text-orange-400 tabular-nums">
                  {t('terminal.broadcast.target_count', { count: targets.size })}
                </span>
              )}
            </div>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
};

export const TabBarTerminalActions: React.FC<TabBarTerminalActionsProps> = ({
  activeTab,
}) => {
  const { t } = useTranslation();
  const openPlayer = useRecordingStore(s => s.openPlayer);

  // Determine session ID — handle both single pane and split pane modes
  const getActiveSessionId = (): string | undefined => {
    // Split pane mode: find active pane's sessionId from tree
    if (activeTab.rootPane && activeTab.activePaneId) {
      const activePane = findPaneById(activeTab.rootPane, activeTab.activePaneId);
      return activePane?.sessionId;
    }
    // Single pane mode: use legacy sessionId
    return activeTab.sessionId;
  };
  
  const sessionId = getActiveSessionId();

  // Check if this session is recording
  const isRecording = useRecordingStore(s =>
    sessionId ? s.isRecording(sessionId) : false,
  );
  // Select only the primitive elapsed value to avoid creating a new object
  // reference on every getSnapshot() call (which causes infinite re-renders
  // with Zustand v5's raw useSyncExternalStore).
  const recordingElapsed = useRecordingStore(s => {
    if (!sessionId) return null;
    const tick = s.recordingTicks.get(sessionId);
    if (tick) return tick.elapsed;
    const entry = s.recordings.get(sessionId);
    return entry ? entry.meta.elapsed : null;
  });
  const stopRecording = useRecordingStore(s => s.stopRecording);
  const discardRecording = useRecordingStore(s => s.discardRecording);

  /** Dispatch start-recording event to the active terminal */
  const handleStartRecording = useCallback(() => {
    if (!sessionId) return;
    window.dispatchEvent(
      new CustomEvent('oxide:start-recording', {
        detail: { sessionId },
      }),
    );
  }, [sessionId]);

  /** Stop recording — content will be handled by terminal's handleRecordingStop */
  const handleStop = useCallback(() => {
    if (!sessionId) return;
    const content = stopRecording(sessionId);
    if (content) {
      // Dispatch stop event so the terminal view can trigger the save dialog
      window.dispatchEvent(
        new CustomEvent('oxide:recording-stopped', {
          detail: { sessionId, content },
        }),
      );
    }
  }, [sessionId, stopRecording]);

  /** Discard recording */
  const handleDiscard = useCallback(() => {
    if (!sessionId) return;
    discardRecording(sessionId);
  }, [sessionId, discardRecording]);

  /** Open a .cast file from disk and launch the player */
  const handleOpenCast = useCallback(async () => {
    try {
      const filePath = await open({
        filters: [{ name: 'Asciicast', extensions: ['cast'] }],
        multiple: false,
      });

      if (filePath) {
        const content = await readTextFile(filePath as string);
        const fileName = (filePath as string).split(/[/\\]/).pop() || 'recording.cast';
        openPlayer(fileName, content);
      }
    } catch (err) {
      console.error('[TabBarTerminalActions] Failed to open cast file:', err);
    }
  }, [openPlayer]);

  /** Format seconds as MM:SS */
  const fmtElapsed = (sec: number) => {
    const m = Math.floor(sec / 60);
    const s = Math.floor(sec % 60);
    return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  };

  // ── Split pane state (local_terminal only) ───────────────────────────
  const isLocalTerminal = activeTab.type === 'local_terminal';
  const { splitPane, getPaneCount, sessions, tabs } = useAppStore();
  const { createTerminal } = useLocalTerminalStore();
  const paneCount = getPaneCount(activeTab.id);
  const canSplit = paneCount < MAX_PANES_PER_TAB;

  const handleSplit = useCallback(async (direction: SplitDirection) => {
    if (!canSplit || !isLocalTerminal) return;
    try {
      const newSession = await createTerminal();
      splitPane(activeTab.id, direction, newSession.id, 'local_terminal');
    } catch (err) {
      console.error('[TabBarTerminalActions] Failed to split pane:', err);
    }
  }, [canSplit, isLocalTerminal, createTerminal, splitPane, activeTab.id]);

  // ── Broadcast state ─────────────────────────────────────────────────
  const broadcastEnabled = useBroadcastStore(s => s.enabled);
  const broadcastTargets = useBroadcastStore(s => s.targets);
  const toggleTarget = useBroadcastStore(s => s.toggleTarget);
  const disableBroadcast = useBroadcastStore(s => s.disable);

  const [refreshKey, setRefreshKey] = useState(0);

  const terminalEntries = useMemo(() => {
    void broadcastTargets;
    void refreshKey;
    return getAllEntries();
  }, [broadcastTargets, refreshKey]);

  // No session ID (e.g. split pane with cleared sessionId) — hide actions
  if (!sessionId) return null;

  // Build target list from terminal registry
  // For split-pane tabs: use activePaneId from tab state
  // For single-pane (legacy) tabs: fall back to sessionId (matches effectivePaneId in TerminalView)
  const activePaneId = activeTab.activePaneId
    ?? (activeTab.rootPane?.type === 'leaf' ? activeTab.rootPane.id : undefined)
    ?? sessionId;

  // ── Build action groups ────────────────────────────────────────────────
  return (
    <div className="flex-shrink-0 flex items-center h-full border-l border-theme-border">
      {/* ── Split pane actions (local_terminal only) ────────────────── */}
      {isLocalTerminal && (
        <div className="flex items-center gap-0.5 px-2">
          <button
            onClick={() => handleSplit('horizontal')}
            disabled={!canSplit}
            className={cn(
              'p-1.5 rounded-md transition-colors',
              canSplit
                ? 'text-theme-text-muted hover:text-theme-accent hover:bg-theme-bg-hover'
                : 'text-theme-text-muted/40 cursor-not-allowed',
            )}
            title={
              canSplit
                ? t('terminal.pane.split_horizontal')
                : t('terminal.pane.max_panes_reached', { max: MAX_PANES_PER_TAB })
            }
          >
            <SplitSquareHorizontal className="h-3.5 w-3.5" />
          </button>

          <button
            onClick={() => handleSplit('vertical')}
            disabled={!canSplit}
            className={cn(
              'p-1.5 rounded-md transition-colors',
              canSplit
                ? 'text-theme-text-muted hover:text-theme-accent hover:bg-theme-bg-hover'
                : 'text-theme-text-muted/40 cursor-not-allowed',
            )}
            title={
              canSplit
                ? t('terminal.pane.split_vertical')
                : t('terminal.pane.max_panes_reached', { max: MAX_PANES_PER_TAB })
            }
          >
            <SplitSquareVertical className="h-3.5 w-3.5" />
          </button>

          {paneCount > 1 && (
            <span className="text-xs text-theme-text-muted pl-0.5 tabular-nums">
              {paneCount}/{MAX_PANES_PER_TAB}
            </span>
          )}
        </div>
      )}

      {/* ── Separator between split & broadcast/recording groups ─────── */}
      {isLocalTerminal && (
        <div className="w-px h-4 bg-theme-border/50" />
      )}

      {/* ── Broadcast input ─────────────────────────────────────────── */}
      <BroadcastDropdown
        entries={terminalEntries}
        targets={broadcastTargets}
        enabled={broadcastEnabled}
        activePaneId={activePaneId}
        sessions={sessions}
        tabs={tabs}
        toggleTarget={toggleTarget}
        disableBroadcast={disableBroadcast}
        onRefresh={() => setRefreshKey(k => k + 1)}
        t={t}
      />

      {/* ── Separator before recording ──────────────────────────────── */}
      <div className="w-px h-4 bg-theme-border/50" />

      {/* ── Recording actions ───────────────────────────────────────── */}
      {isRecording && recordingElapsed !== null ? (
        <div className="flex items-center gap-1.5 px-2">
          {/* REC badge */}
          <div className="flex items-center gap-1.5">
            <Circle className="h-2.5 w-2.5 fill-red-500 text-red-500 animate-pulse" />
            <span className="text-xs font-mono text-red-400 font-medium">
              {fmtElapsed(recordingElapsed)}
            </span>
          </div>

          {/* Stop */}
          <button
            onClick={handleStop}
            className="p-1 rounded-md text-theme-text-muted hover:text-red-400 hover:bg-theme-bg-hover transition-colors"
            title={t('terminal.recording.stop')}
          >
            <Square className="h-3 w-3 fill-current" />
          </button>

          {/* Discard */}
          <button
            onClick={handleDiscard}
            className="p-1 rounded-md text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover transition-colors"
            title={t('terminal.recording.discard')}
          >
            <Trash2 className="h-3 w-3" />
          </button>
        </div>
      ) : (
        <div className="flex items-center gap-0.5 px-2">
          {/* Start Recording */}
          <button
            onClick={handleStartRecording}
            className={cn(
              'p-1.5 rounded-md transition-colors',
              'text-theme-text-muted hover:text-red-400 hover:bg-theme-bg-hover',
            )}
            title={`${t('terminal.recording.start')}  ⌘⇧R`}
          >
            <Circle className="h-3.5 w-3.5" />
          </button>

          {/* Open Cast File */}
          <button
            onClick={handleOpenCast}
            className={cn(
              'p-1.5 rounded-md transition-colors',
              'text-theme-text-muted hover:text-theme-text hover:bg-theme-bg-hover',
            )}
            title={t('terminal.recording.open_cast')}
          >
            <FilePlay className="h-3.5 w-3.5" />
          </button>
        </div>
      )}
    </div>
  );
};
