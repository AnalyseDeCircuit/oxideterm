// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React, { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { readTextFile } from '@tauri-apps/plugin-fs';
import {
  ChevronRight,
  FilePlay,
  GitBranch,
  Radio,
  SplitSquareHorizontal,
  SplitSquareVertical,
  Square,
  Trash2,
  Circle,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import { getAllEntries } from '@/lib/terminalRegistry';
import { useTerminalCommandBarState, type TerminalCommandBarTerminalType } from '@/hooks/useTerminalCommandBarState';
import type { TerminalAutosuggestCandidate } from '@/lib/terminal/autosuggest';
import { useAppStore } from '@/store/appStore';
import { useBroadcastStore } from '@/store/broadcastStore';
import { useLocalTerminalStore } from '@/store/localTerminalStore';
import { useRecordingStore } from '@/store/recordingStore';
import { MAX_PANES_PER_TAB, type SplitDirection } from '@/types';
import { BroadcastDropdown } from '@/components/layout/TabBarTerminalActions';

type TerminalCommandBarProps = {
  paneId: string;
  sessionId: string;
  tabId: string;
  terminalType: TerminalCommandBarTerminalType;
  isActive: boolean;
  sendInput: (input: string) => void;
  focusTerminal: () => void;
  onLayoutChange?: () => void;
};

export const TerminalCommandBar: React.FC<TerminalCommandBarProps> = (props) => {
  const { paneId, sessionId, tabId, terminalType, isActive, sendInput, focusTerminal, onLayoutChange } = props;
  const { t } = useTranslation();
  const state = useTerminalCommandBarState({
    paneId,
    sessionId,
    tabId,
    terminalType,
    isActive,
    sendInput,
  });
  const [highlightedSuggestion, setHighlightedSuggestion] = useState(0);
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const composingRef = useRef(false);

  const placeholder = t('terminal.command_bar.command_placeholder');

  const handleKeyDown = useCallback((event: React.KeyboardEvent<HTMLInputElement>) => {
    if (composingRef.current || isComposingKeyEvent(event)) {
      return;
    }
    if (event.key === 'Escape') {
      if (suggestionsOpen) {
        event.preventDefault();
        setSuggestionsOpen(false);
        return;
      }
      state.setFocused(false);
      focusTerminal();
      return;
    }
    if (event.key === 'Tab') {
      if (suggestionsOpen && state.acceptSuggestion(state.suggestions[highlightedSuggestion] ?? state.suggestions[0])) {
        event.preventDefault();
        setSuggestionsOpen(false);
      }
      return;
    }
    if (event.key === 'ArrowDown' && state.suggestions.length > 0) {
      event.preventDefault();
      setSuggestionsOpen(true);
      setHighlightedSuggestion((current) => suggestionsOpen ? Math.min(current + 1, state.suggestions.length - 1) : 0);
      return;
    }
    if (event.key === 'ArrowUp' && state.suggestions.length > 0) {
      event.preventDefault();
      setSuggestionsOpen(true);
      setHighlightedSuggestion((current) => suggestionsOpen ? Math.max(current - 1, 0) : state.suggestions.length - 1);
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      setSuggestionsOpen(false);
      state.submitCommand();
    }
  }, [focusTerminal, highlightedSuggestion, state, suggestionsOpen]);

  useLayoutEffect(() => {
    if (!rootRef.current || !onLayoutChange) return;
    let frame: number | null = null;
    const notify = () => {
      if (frame !== null) cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        frame = null;
        onLayoutChange();
      });
    };
    const observer = new ResizeObserver(notify);
    observer.observe(rootRef.current);
    notify();
    return () => {
      if (frame !== null) cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [onLayoutChange]);

  return (
    <div ref={rootRef} className="relative z-20 flex-shrink-0 border-t border-theme-border/70 bg-theme-bg/95 px-3 py-1 shadow-[0_-6px_18px_rgba(0,0,0,0.16)]">
      {state.focused && suggestionsOpen && state.suggestions.length > 0 && (
        <TerminalCommandSuggestions
          suggestions={state.suggestions}
          highlightedIndex={highlightedSuggestion}
          onPick={(candidate) => {
            state.acceptSuggestion(candidate);
            setHighlightedSuggestion(0);
            setSuggestionsOpen(false);
            inputRef.current?.focus();
          }}
        />
      )}
      <div className="flex min-h-6 min-w-0 items-center justify-between gap-2">
        <TerminalCommandBarChips
          targetLabel={state.targetLabel}
          cwd={state.cwd}
          broadcastEnabled={state.chips.broadcastEnabled}
          broadcastTargetCount={state.chips.broadcastTargetCount}
          isRecording={state.chips.isRecording}
          gitBranch={state.chips.gitBranch}
        />
        <div className="flex flex-shrink-0 items-center gap-1">
          <TerminalCommandBarActions
            paneId={paneId}
            sessionId={sessionId}
            tabId={tabId}
            terminalType={terminalType}
          />
        </div>
      </div>
      <div
        className={cn(
          'mt-0.5 flex min-w-0 cursor-text items-center gap-2 border-t border-theme-border/45 pt-1',
          state.focused && 'border-theme-accent/45',
        )}
        onMouseDown={(event) => {
          if (event.target === event.currentTarget) {
            event.preventDefault();
            inputRef.current?.focus();
          }
        }}
      >
        <span className={cn(
          'flex h-6 w-5 flex-shrink-0 items-center justify-center',
          'text-theme-text-muted',
        )}>
          <ChevronRight className="h-4 w-4" />
        </span>
        <input
          ref={inputRef}
          value={state.value}
          onChange={(event) => {
            state.setValue(event.target.value);
            setHighlightedSuggestion(0);
            setSuggestionsOpen(false);
          }}
          onFocus={() => state.setFocused(true)}
          onBlur={() => window.setTimeout(() => {
            setSuggestionsOpen(false);
            state.setFocused(false);
          }, 120)}
          onCompositionStart={() => {
            composingRef.current = true;
          }}
          onCompositionEnd={() => {
            window.setTimeout(() => {
              composingRef.current = false;
            }, 0);
          }}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          className="min-w-0 max-w-[960px] flex-1 bg-transparent py-0.5 text-sm leading-6 text-theme-text outline-none placeholder:text-theme-text-muted"
          spellCheck={false}
        />
        <button
          type="button"
          onMouseDown={(event) => event.preventDefault()}
          onClick={state.submitCommand}
          className="inline-flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md text-theme-accent hover:bg-theme-accent/10"
          title={t('terminal.command_bar.run_command')}
        >
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
};

type ChipsProps = {
  targetLabel: string;
  cwd: string | null;
  broadcastEnabled: boolean;
  broadcastTargetCount: number;
  isRecording: boolean;
  gitBranch: string | null;
};

const TerminalCommandBarChips: React.FC<ChipsProps> = ({
  targetLabel,
  cwd,
  broadcastEnabled,
  broadcastTargetCount,
  isRecording,
  gitBranch,
}) => {
  const { t } = useTranslation();
  return (
    <div className="hidden min-w-0 flex-1 items-center gap-1.5 md:flex">
      <span className="truncate rounded-md border border-theme-border/60 bg-theme-bg-panel/60 px-1.5 py-0.5 text-[11px] text-theme-text" title={targetLabel}>
        {targetLabel}
      </span>
      {cwd && (
        <span className="truncate rounded-md border border-theme-border/50 bg-theme-bg/45 px-1.5 py-0.5 text-[11px] text-theme-text-muted" title={cwd}>
          {cwd.split('/').filter(Boolean).pop() || cwd}
        </span>
      )}
      {broadcastEnabled && (
        <span className="inline-flex items-center gap-1 rounded-md border border-orange-500/30 bg-orange-500/10 px-1.5 py-0.5 text-[11px] text-orange-300">
          <Radio className="h-3 w-3" />
          {broadcastTargetCount > 0 ? broadcastTargetCount : t('terminal.command_bar.all_targets')}
        </span>
      )}
      {isRecording && (
        <span className="inline-flex items-center gap-1 rounded-md border border-red-500/30 bg-red-500/10 px-1.5 py-0.5 text-[11px] text-red-300">
          <Circle className="h-2.5 w-2.5 fill-current" />
          {t('terminal.command_bar.recording')}
        </span>
      )}
      {gitBranch && (
        <span className="inline-flex min-w-0 items-center gap-1 rounded-md border border-theme-border/50 bg-theme-bg/45 px-1.5 py-0.5 text-[11px] text-theme-text-muted" title={gitBranch}>
          <GitBranch className="h-3 w-3 flex-shrink-0" />
          <span className="truncate">{gitBranch}</span>
        </span>
      )}
    </div>
  );
};

type ActionsProps = {
  paneId: string;
  sessionId: string;
  tabId: string;
  terminalType: TerminalCommandBarTerminalType;
};

const TerminalCommandBarActions: React.FC<ActionsProps> = ({ paneId, sessionId, tabId, terminalType }) => {
  const { t } = useTranslation();
  const openPlayer = useRecordingStore((s) => s.openPlayer);
  const stopRecording = useRecordingStore((s) => s.stopRecording);
  const discardRecording = useRecordingStore((s) => s.discardRecording);
  const isRecording = useRecordingStore((s) => s.isRecording(sessionId));
  const broadcastEnabled = useBroadcastStore((s) => s.enabled);
  const broadcastTargets = useBroadcastStore((s) => s.targets);
  const toggleTarget = useBroadcastStore((s) => s.toggleTarget);
  const disableBroadcast = useBroadcastStore((s) => s.disable);
  const { sessions, tabs, splitPane, getPaneCount } = useAppStore();
  const createTerminal = useLocalTerminalStore((s) => s.createTerminal);
  const [refreshKey, setRefreshKey] = useState(0);
  const activeTab = useMemo(() => tabs.find((tab) => tab.id === tabId), [tabId, tabs]);
  const terminalEntries = useMemo(() => {
    void refreshKey;
    void broadcastTargets;
    return getAllEntries();
  }, [broadcastTargets, refreshKey]);
  const paneCount = activeTab ? getPaneCount(activeTab.id) : 1;
  const canSplit = terminalType === 'local_terminal' && !!activeTab && paneCount < MAX_PANES_PER_TAB;

  const handleSplit = useCallback(async (direction: SplitDirection) => {
    if (!activeTab || !canSplit) return;
    const newSession = await createTerminal();
    splitPane(activeTab.id, direction, newSession.id, 'local_terminal');
  }, [activeTab, canSplit, createTerminal, splitPane]);

  const handleStartRecording = useCallback(() => {
    window.dispatchEvent(new CustomEvent('oxide:start-recording', { detail: { sessionId } }));
  }, [sessionId]);

  const handleStopRecording = useCallback(() => {
    const content = stopRecording(sessionId);
    if (content) {
      window.dispatchEvent(new CustomEvent('oxide:recording-stopped', { detail: { sessionId, content } }));
    }
  }, [sessionId, stopRecording]);

  const handleOpenCast = useCallback(async () => {
    const filePath = await open({
      filters: [{ name: 'Asciicast', extensions: ['cast'] }],
      multiple: false,
    });
    if (!filePath) return;
    const content = await readTextFile(filePath as string);
    const fileName = (filePath as string).split(/[/\\]/).pop() || 'recording.cast';
    openPlayer(fileName, content);
  }, [openPlayer]);

  if (!activeTab) return null;

  return (
    <div className="flex flex-shrink-0 items-center gap-1">
      {terminalType === 'local_terminal' && (
        <>
          <button
            type="button"
            onClick={() => handleSplit('horizontal')}
            disabled={!canSplit}
            className={actionButtonClass(canSplit)}
            title={t('terminal.pane.split_horizontal')}
          >
            <SplitSquareHorizontal className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={() => handleSplit('vertical')}
            disabled={!canSplit}
            className={actionButtonClass(canSplit)}
            title={t('terminal.pane.split_vertical')}
          >
            <SplitSquareVertical className="h-3.5 w-3.5" />
          </button>
        </>
      )}
      <BroadcastDropdown
        entries={terminalEntries}
        targets={broadcastTargets}
        enabled={broadcastEnabled}
        activePaneId={paneId}
        sessions={sessions}
        tabs={tabs}
        toggleTarget={toggleTarget}
        disableBroadcast={disableBroadcast}
        onRefresh={() => setRefreshKey((key) => key + 1)}
        t={t}
      />
      {isRecording ? (
        <>
          <button type="button" onClick={handleStopRecording} className={actionButtonClass(true)} title={t('terminal.recording.stop')}>
            <Square className="h-3.5 w-3.5 text-red-400" />
          </button>
          <button type="button" onClick={() => discardRecording(sessionId)} className={actionButtonClass(true)} title={t('terminal.recording.discard')}>
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </>
      ) : (
        <button type="button" onClick={handleStartRecording} className={actionButtonClass(true)} title={t('terminal.recording.start')}>
          <Circle className="h-3.5 w-3.5" />
        </button>
      )}
      <button type="button" onClick={handleOpenCast} className={actionButtonClass(true)} title={t('terminal.recording.open_cast')}>
        <FilePlay className="h-3.5 w-3.5" />
      </button>
    </div>
  );
};

function actionButtonClass(enabled: boolean): string {
  return cn(
    'inline-flex h-6 w-6 items-center justify-center rounded-md transition-colors',
    enabled
      ? 'text-theme-text-muted hover:text-theme-accent hover:bg-theme-bg-hover'
      : 'text-theme-text-muted/35 cursor-not-allowed',
  );
}

function isComposingKeyEvent(event: React.KeyboardEvent<HTMLInputElement>): boolean {
  const nativeEvent = event.nativeEvent as KeyboardEvent & { isComposing?: boolean };
  return nativeEvent.isComposing === true || nativeEvent.keyCode === 229;
}

type SuggestionsProps = {
  suggestions: TerminalAutosuggestCandidate[];
  highlightedIndex: number;
  onPick: (candidate: TerminalAutosuggestCandidate) => void;
};

const TerminalCommandSuggestions: React.FC<SuggestionsProps> = ({ suggestions, highlightedIndex, onPick }) => {
  const { t } = useTranslation();
  return (
    <div className="absolute bottom-full left-3 mb-2 w-[min(720px,calc(100%-1.5rem))] overflow-hidden rounded-lg border border-theme-border bg-theme-bg-elevated/95 shadow-xl shadow-black/30">
      {suggestions.map((candidate, index) => (
        <button
          key={`${candidate.source}:${candidate.command}`}
          type="button"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => onPick(candidate)}
          className={cn(
            'flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm transition-colors',
            index === highlightedIndex ? 'bg-theme-bg-hover text-theme-text' : 'text-theme-text-muted hover:bg-theme-bg-hover/60 hover:text-theme-text',
          )}
        >
          <span className="min-w-0 flex-1 truncate font-mono">{candidate.command}</span>
          <span className="rounded bg-theme-bg-panel px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-theme-text-muted">
            {t(`terminal.command_bar.${sourceKey(candidate.source)}`, { defaultValue: candidate.source })}
          </span>
        </button>
      ))}
    </div>
  );
};

function sourceKey(source: string): string {
  switch (source) {
    case 'local-history':
      return 'source_local_history';
    case 'ai-ledger':
      return 'source_ai_ledger';
    default:
      return 'source_runtime';
  }
}
