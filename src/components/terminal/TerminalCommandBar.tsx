// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import React, { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { readTextFile } from '@tauri-apps/plugin-fs';
import {
  ChevronRight,
  FilePlay,
  Folder,
  GitBranch,
  Pencil,
  Radio,
  Search,
  Server,
  SplitSquareHorizontal,
  SplitSquareVertical,
  Square,
  Trash2,
  Circle,
  Container,
  Monitor,
  Play,
  Plus,
  Save,
  X,
  Zap,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '@/lib/utils';
import { getAllEntries } from '@/lib/terminalRegistry';
import { useTerminalCommandBarState, type TerminalCommandBarTerminalType } from '@/hooks/useTerminalCommandBarState';
import type { CommandBarCompletion } from '@/lib/terminal/completion';
import { classifyCommandRisk } from '@/lib/terminal/completion/risk';
import { shouldHandOffFocusToTerminal } from '@/lib/terminal/focusHandoff';
import { useConfirm } from '@/hooks/useConfirm';
import { useToastStore } from '@/hooks/useToast';
import { useAppStore } from '@/store/appStore';
import { useBroadcastStore } from '@/store/broadcastStore';
import { useLocalTerminalStore } from '@/store/localTerminalStore';
import {
  DEFAULT_QUICK_COMMAND_CATEGORIES,
  useQuickCommandsStore,
  matchQuickCommandHostPattern,
  type QuickCommand,
  type QuickCommandCategory,
  type QuickCommandCategoryDraft,
  type QuickCommandDraft,
  type QuickCommandIcon,
} from '@/store/quickCommandsStore';
import { useRecordingStore } from '@/store/recordingStore';
import { useSettingsStore } from '@/store/settingsStore';
import { MAX_PANES_PER_TAB, type SplitDirection } from '@/types';
import { BroadcastDropdown } from '@/components/layout/TabBarTerminalActions';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

type TerminalCommandBarProps = {
  paneId: string;
  sessionId: string;
  tabId: string;
  terminalType: TerminalCommandBarTerminalType;
  nodeId?: string | null;
  isActive: boolean;
  sendInput: (input: string) => void;
  focusTerminal: () => void;
  onLayoutChange?: () => void;
};

export const TerminalCommandBar: React.FC<TerminalCommandBarProps> = (props) => {
  const { paneId, sessionId, tabId, terminalType, nodeId, isActive, sendInput, focusTerminal, onLayoutChange } = props;
  const { t } = useTranslation();
  const state = useTerminalCommandBarState({
    paneId,
    sessionId,
    tabId,
    terminalType,
    nodeId,
    isActive,
    sendInput,
  });
  const [highlightedSuggestion, setHighlightedSuggestion] = useState(-1);
  const [suggestionsOpen, setSuggestionsOpen] = useState(false);
  const [quickCommandsOpen, setQuickCommandsOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const composingRef = useRef(false);
  const quickCommandSettings = useSettingsStore((s) => s.settings.terminal.commandBar);
  const hydrateQuickCommands = useQuickCommandsStore((s) => s.hydrate);
  const { confirm, ConfirmDialog } = useConfirm();

  const placeholder = t('terminal.command_bar.command_placeholder');

  const submitCommand = useCallback((commandOverride?: string) => {
    const command = commandOverride ?? state.value;
    const didSubmit = state.submitCommand(commandOverride);
    if (didSubmit && shouldHandOffFocusToTerminal(command, quickCommandSettings.focusHandoffCommands)) {
      focusTerminal();
    }
    return didSubmit;
  }, [focusTerminal, quickCommandSettings.focusHandoffCommands, state]);

  useEffect(() => {
    if (quickCommandSettings.quickCommandsEnabled) {
      void hydrateQuickCommands();
    }
  }, [hydrateQuickCommands, quickCommandSettings.quickCommandsEnabled]);

  const runCommand = useCallback(async (command: string) => {
    const risk = classifyCommandRisk(command);
    if (quickCommandSettings.quickCommandsConfirmBeforeRun || risk === 'high' || risk === 'medium') {
      const confirmed = await confirm({
        title: t('terminal.quick_commands.confirm_title'),
        description: risk
          ? t('terminal.quick_commands.confirm_risky_description', { command })
          : t('terminal.quick_commands.confirm_description', { command }),
        confirmLabel: t('terminal.quick_commands.run'),
        variant: risk === 'high' ? 'danger' : 'default',
      });
      if (!confirmed) return;
    }
    const didSubmit = submitCommand(command);
    if (didSubmit && quickCommandSettings.quickCommandsShowToast) {
      useToastStore.getState().addToast({
        title: t('terminal.quick_commands.toast_executed'),
        description: command,
        variant: 'success',
      });
    }
    setQuickCommandsOpen(false);
    if (!didSubmit || !shouldHandOffFocusToTerminal(command, quickCommandSettings.focusHandoffCommands)) {
      inputRef.current?.focus();
    }
  }, [confirm, quickCommandSettings.focusHandoffCommands, quickCommandSettings.quickCommandsConfirmBeforeRun, quickCommandSettings.quickCommandsShowToast, submitCommand, t]);

  const handleKeyDown = useCallback((event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (composingRef.current || isComposingKeyEvent(event)) {
      return;
    }
    if (event.key === 'Escape') {
      if (quickCommandsOpen) {
        event.preventDefault();
        setQuickCommandsOpen(false);
        return;
      }
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
      if (suggestionsOpen && state.acceptSuggestion(highlightedSuggestion >= 0 ? state.suggestions[highlightedSuggestion] : state.suggestions[0])) {
        event.preventDefault();
        setHighlightedSuggestion(-1);
        setSuggestionsOpen(false);
      }
      return;
    }
    if (event.key === 'ArrowRight' && state.suggestions.length > 0 && state.ghostText) {
      const inlineSuggestion = state.suggestions.find((candidate) => candidate.inlineSafe);
      if (state.acceptSuggestion(inlineSuggestion)) {
        event.preventDefault();
        setHighlightedSuggestion(-1);
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
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      void state.revealHistorySuggestions().then((count) => {
        if (count > 0) {
          setSuggestionsOpen(true);
          setHighlightedSuggestion(0);
        }
      });
      return;
    }
    if (event.key === 'ArrowUp' && state.suggestions.length > 0) {
      event.preventDefault();
      setSuggestionsOpen(true);
      setHighlightedSuggestion((current) => suggestionsOpen && current >= 0 ? Math.max(current - 1, 0) : state.suggestions.length - 1);
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      void state.revealHistorySuggestions().then((count) => {
        if (count > 0) {
          setSuggestionsOpen(true);
          setHighlightedSuggestion(count - 1);
        }
      });
      return;
    }
    if (event.key === 'Enter' && (event.shiftKey || event.altKey)) {
      // Command Bar keeps plain Enter as "run", so explicit modified Enter is
      // the manual newline path for multi-line shell snippets and heredocs.
      setSuggestionsOpen(false);
      setHighlightedSuggestion(-1);
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      setSuggestionsOpen(false);
      // Auto-opened completions are only suggestions, not a selection. Enter
      // must run the typed command unless ArrowUp/ArrowDown picked an item.
      const selectedSuggestion = suggestionsOpen && highlightedSuggestion >= 0
        ? state.suggestions[highlightedSuggestion] ?? state.suggestions[0]
        : undefined;
      if (selectedSuggestion && !selectedSuggestion.executable) {
        state.acceptSuggestion(selectedSuggestion);
        setHighlightedSuggestion(-1);
        return;
      }
      setHighlightedSuggestion(-1);
      submitCommand(selectedSuggestion?.insertText);
    }
  }, [focusTerminal, highlightedSuggestion, quickCommandsOpen, state, submitCommand, suggestionsOpen]);

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

  useLayoutEffect(() => {
    const input = inputRef.current;
    if (composingRef.current || state.inputComposing) return;
    if (!input || document.activeElement !== input) return;
    const cursor = Math.max(0, Math.min(state.value.length, state.cursorIndex));
    input.setSelectionRange(cursor, cursor);
  }, [state.cursorIndex, state.inputComposing, state.value]);

  useEffect(() => {
    if (state.suggestions.length > 0 || suggestionsOpen) return;
    setHighlightedSuggestion(-1);
    setSuggestionsOpen(false);
  }, [state.suggestions.length, suggestionsOpen]);

  useEffect(() => {
    if (!quickCommandsOpen && !suggestionsOpen) return;

    const handlePointerDown = (event: PointerEvent) => {
      const root = rootRef.current;
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (root?.contains(target)) return;
      if (isRadixPortalInteraction(event)) return;

      setQuickCommandsOpen(false);
      setSuggestionsOpen(false);
      setHighlightedSuggestion(-1);
    };

    document.addEventListener('pointerdown', handlePointerDown, true);
    return () => document.removeEventListener('pointerdown', handlePointerDown, true);
  }, [quickCommandsOpen, suggestionsOpen]);

  return (
    <div ref={rootRef} className="relative z-20 flex-shrink-0 border-t border-theme-border/70 bg-theme-bg/95 px-3 py-1 shadow-[0_-6px_18px_rgba(0,0,0,0.16)]">
      {state.focused && suggestionsOpen && state.suggestions.length > 0 && (
        <TerminalCommandSuggestions
          suggestions={state.suggestions}
          highlightedIndex={highlightedSuggestion}
          onPick={(candidate) => {
            state.acceptSuggestion(candidate);
            setHighlightedSuggestion(-1);
            setSuggestionsOpen(false);
            inputRef.current?.focus();
          }}
        />
      )}
      {quickCommandSettings.quickCommandsEnabled && quickCommandsOpen && (
        <QuickCommandsPopover
          targetLabel={state.targetLabel}
          cwdHost={null}
          onInsert={(command) => {
            state.setValue(command);
            state.setCursorIndex(command.length);
            setQuickCommandsOpen(false);
            inputRef.current?.focus();
          }}
          onRun={(command) => void runCommand(command)}
          onClose={() => {
            setQuickCommandsOpen(false);
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
        <textarea
          ref={inputRef}
          value={state.value}
          onChange={(event) => {
            state.setValue(event.target.value);
            state.setCursorIndex(event.target.selectionStart ?? event.target.value.length);
            setHighlightedSuggestion(-1);
            setSuggestionsOpen(false);
          }}
          onSelect={(event) => {
            if (!composingRef.current) {
              state.setCursorIndex(event.currentTarget.selectionStart ?? state.value.length);
            }
          }}
          onKeyUp={(event) => {
            if (!composingRef.current) {
              state.setCursorIndex(event.currentTarget.selectionStart ?? state.value.length);
            }
          }}
          onClick={(event) => state.setCursorIndex(event.currentTarget.selectionStart ?? state.value.length)}
          onFocus={() => state.setFocused(true)}
          onBlur={() => window.setTimeout(() => {
            setSuggestionsOpen(false);
            state.setFocused(false);
          }, 120)}
          onCompositionStart={() => {
            composingRef.current = true;
            state.setInputComposing(true);
            setHighlightedSuggestion(-1);
            setSuggestionsOpen(false);
          }}
          onCompositionEnd={(event) => {
            const nextValue = event.currentTarget.value;
            state.setValue(nextValue);
            state.setCursorIndex(event.currentTarget.selectionStart ?? nextValue.length);
            window.setTimeout(() => {
              composingRef.current = false;
              state.setInputComposing(false);
            }, 0);
          }}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          rows={Math.min(6, Math.max(1, state.value.split(/\r?\n/).length))}
          className="max-h-36 min-h-6 min-w-0 flex-1 resize-none bg-transparent py-0.5 text-sm leading-6 text-theme-text outline-none placeholder:text-theme-text-muted"
          style={terminalFontStyle}
          spellCheck={false}
        />
        {state.focused && state.ghostText && !state.value.includes('\n') && (
          <span className="pointer-events-none max-w-[24rem] flex-shrink truncate text-sm leading-6 text-theme-text-muted/35" style={terminalFontStyle}>
            {state.ghostText}
          </span>
        )}
        {quickCommandSettings.quickCommandsEnabled && (
          <div className="ml-auto flex flex-shrink-0 items-center">
            <button
              type="button"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => {
                setSuggestionsOpen(false);
                setHighlightedSuggestion(-1);
                setQuickCommandsOpen((open) => !open);
                inputRef.current?.focus();
              }}
              className={cn(
                'inline-flex h-6 w-6 items-center justify-center rounded-md hover:bg-theme-accent/10',
                quickCommandsOpen ? 'bg-theme-accent/10 text-theme-accent' : 'text-theme-text-muted hover:text-theme-accent',
              )}
              title={t('terminal.quick_commands.open')}
            >
              <Zap className="h-4 w-4" />
            </button>
          </div>
        )}
      </div>
      {ConfirmDialog}
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

type QuickCommandsPopoverProps = {
  targetLabel: string;
  cwdHost?: string | null;
  onInsert: (command: string) => void;
  onRun: (command: string) => void;
  onClose: () => void;
};

const QuickCommandsPopover: React.FC<QuickCommandsPopoverProps> = ({ targetLabel, cwdHost, onInsert, onRun, onClose }) => {
  const { t } = useTranslation();
  const categories = useQuickCommandsStore((s) => s.categories);
  const commands = useQuickCommandsStore((s) => s.commands);
  const upsertCommand = useQuickCommandsStore((s) => s.upsertCommand);
  const deleteCommand = useQuickCommandsStore((s) => s.deleteCommand);
  const upsertCategory = useQuickCommandsStore((s) => s.upsertCategory);
  const deleteCategory = useQuickCommandsStore((s) => s.deleteCategory);
  const [query, setQuery] = useState('');
  const [activeCategory, setActiveCategory] = useState(categories[0]?.id ?? 'system');
  const [editingCommand, setEditingCommand] = useState<QuickCommand | null>(null);
  const [editingCategory, setEditingCategory] = useState<QuickCommandCategory | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [categoryEditorOpen, setCategoryEditorOpen] = useState(false);

  const targetFields = useMemo(() => [targetLabel, cwdHost], [cwdHost, targetLabel]);
  const filteredCommands = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return commands.filter((command) => (
      command.category === activeCategory
      && matchQuickCommandHostPattern(command.hostPattern, targetFields)
      && (
        !normalizedQuery
        || command.name.toLowerCase().includes(normalizedQuery)
        || command.command.toLowerCase().includes(normalizedQuery)
        || command.description?.toLowerCase().includes(normalizedQuery)
      )
    ));
  }, [activeCategory, commands, query, targetFields]);

  const startCreate = useCallback(() => {
    setEditingCommand(null);
    setEditorOpen(true);
  }, []);

  const startEdit = useCallback((command: QuickCommand) => {
    setEditingCommand(command);
    setEditorOpen(true);
  }, []);

  const handleSave = useCallback((draft: QuickCommandDraft) => {
    upsertCommand(draft);
    setEditorOpen(false);
    setEditingCommand(null);
  }, [upsertCommand]);

  const handleSaveCategory = useCallback((draft: QuickCommandCategoryDraft) => {
    const saved = upsertCategory(draft);
    setActiveCategory(saved.id);
    setCategoryEditorOpen(false);
    setEditingCategory(null);
  }, [upsertCategory]);

  const handleDeleteCategory = useCallback((categoryId: string) => {
    if (deleteCategory(categoryId) && activeCategory === categoryId) {
      setActiveCategory(categories.find((category) => category.id !== categoryId)?.id ?? 'custom');
    }
  }, [activeCategory, categories, deleteCategory]);

  return (
    <div className="absolute bottom-full right-3 z-30 mb-2 flex max-h-[min(520px,70vh)] w-[min(860px,calc(100%-1.5rem))] overflow-hidden rounded-lg border border-theme-border bg-theme-bg-elevated/95 shadow-xl shadow-black/30" style={terminalFontStyle}>
      <div className="w-40 flex-shrink-0 border-r border-theme-border/60 bg-theme-bg/45 p-2">
        <div className="mb-2 flex items-center justify-between">
          <span className="text-[11px] font-medium uppercase tracking-wide text-theme-text-muted">
            {t('terminal.quick_commands.title')}
          </span>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => {
                setEditingCategory(null);
                setCategoryEditorOpen(true);
                setEditorOpen(false);
              }}
              className="rounded p-1 text-theme-text-muted hover:bg-theme-bg-hover hover:text-theme-text"
              title={t('terminal.quick_commands.add_group')}
            >
              <Plus className="h-3.5 w-3.5" />
            </button>
            <button type="button" onClick={onClose} className="rounded p-1 text-theme-text-muted hover:bg-theme-bg-hover hover:text-theme-text" title={t('terminal.quick_commands.close')}>
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
        <div className="space-y-1">
          {categories.map((category) => {
            const Icon = quickCommandIcon(category.icon);
            const count = commands.filter((command) => command.category === category.id).length;
            const isDefaultCategory = DEFAULT_QUICK_COMMAND_CATEGORIES.some((defaultCategory) => defaultCategory.id === category.id);
            const canDeleteCategory = !isDefaultCategory && count === 0;
            return (
              <div
                key={category.id}
                className={cn(
                  'group flex w-full items-center gap-1 rounded-md px-2 py-1.5 text-xs transition-colors',
                  activeCategory === category.id
                    ? 'bg-theme-accent/12 text-theme-accent'
                    : 'text-theme-text-muted hover:bg-theme-bg-hover hover:text-theme-text',
                )}
              >
                <button type="button" onClick={() => setActiveCategory(category.id)} className="flex min-w-0 flex-1 items-center gap-2 text-left">
                  <Icon className="h-3.5 w-3.5 flex-shrink-0" />
                  <span className="min-w-0 flex-1 truncate">{category.name}</span>
                  <span className="rounded bg-theme-bg-panel px-1.5 py-0.5 text-[10px] text-theme-text-muted">{count}</span>
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setEditingCategory(category);
                    setCategoryEditorOpen(true);
                    setEditorOpen(false);
                  }}
                  className="rounded p-0.5 text-theme-text-muted opacity-100 hover:bg-theme-bg-hover hover:text-theme-accent sm:opacity-0 sm:group-hover:opacity-100"
                  title={t('terminal.quick_commands.edit_group')}
                >
                  <Pencil className="h-3 w-3" />
                </button>
                {canDeleteCategory && (
                  <button
                    type="button"
                    onClick={() => handleDeleteCategory(category.id)}
                    className="rounded p-0.5 text-theme-text-muted opacity-100 hover:bg-theme-bg-hover hover:text-red-300 sm:opacity-0 sm:group-hover:opacity-100"
                    title={t('terminal.quick_commands.delete_group')}
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-theme-border/60 p-2">
          <div className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-theme-text-muted" />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t('terminal.quick_commands.search_placeholder')}
              className="h-8 w-full rounded-md border border-theme-border/50 bg-theme-bg/70 pl-7 pr-2 text-sm text-theme-text outline-none placeholder:text-theme-text-muted focus:border-theme-accent/60"
              style={terminalFontStyle}
            />
          </div>
          <button type="button" onClick={startCreate} className="inline-flex h-8 items-center gap-1 rounded-md border border-theme-border/60 px-2 text-xs text-theme-text-muted hover:bg-theme-bg-hover hover:text-theme-text">
            <Plus className="h-3.5 w-3.5" />
            {t('terminal.quick_commands.add')}
          </button>
        </div>
        {categoryEditorOpen && (
          <QuickCommandCategoryEditor
            category={editingCategory}
            onSave={handleSaveCategory}
            onCancel={() => {
              setCategoryEditorOpen(false);
              setEditingCategory(null);
            }}
          />
        )}
        {editorOpen && (
          <QuickCommandEditor
            command={editingCommand}
            category={activeCategory}
            categories={categories}
            onSave={handleSave}
            onCancel={() => {
              setEditorOpen(false);
              setEditingCommand(null);
            }}
          />
        )}
        <div className="min-h-0 flex-1 overflow-auto p-2">
          {filteredCommands.length === 0 ? (
            <div className="flex h-32 flex-col items-center justify-center text-center text-sm text-theme-text-muted">
              <Zap className="mb-2 h-5 w-5" />
              <div>{query ? t('terminal.quick_commands.empty_search') : t('terminal.quick_commands.empty_category')}</div>
            </div>
          ) : (
            <div className="space-y-1">
              {filteredCommands.map((command) => (
                <QuickCommandRow
                  key={command.id}
                  command={command}
                  onInsert={() => onInsert(command.command)}
                  onRun={() => onRun(command.command)}
                  onEdit={() => startEdit(command)}
                  onDelete={() => deleteCommand(command.id)}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

type QuickCommandRowProps = {
  command: QuickCommand;
  onInsert: () => void;
  onRun: () => void;
  onEdit: () => void;
  onDelete: () => void;
};

const QuickCommandRow: React.FC<QuickCommandRowProps> = ({ command, onInsert, onRun, onEdit, onDelete }) => {
  const { t } = useTranslation();
  const risk = classifyCommandRisk(command.command);
  return (
    <div className="group flex items-center gap-2 rounded-md px-2 py-2 text-sm text-theme-text-muted hover:bg-theme-bg-hover/70 hover:text-theme-text">
      <button type="button" onClick={onInsert} className="min-w-0 flex-1 text-left">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate font-medium text-theme-text">{command.name}</span>
          {risk && (
            <span className={cn(
              'rounded px-1.5 py-0.5 text-[10px] uppercase tracking-wide',
              risk === 'high' ? 'bg-red-500/15 text-red-300' : 'bg-amber-500/15 text-amber-300',
            )}>
              {risk}
            </span>
          )}
          {command.hostPattern && (
            <span className="rounded bg-theme-bg-panel px-1.5 py-0.5 text-[10px] text-theme-text-muted">
              {command.hostPattern}
            </span>
          )}
        </div>
        <div className="truncate text-xs text-theme-accent/85" style={terminalFontStyle}>{command.command}</div>
        {command.description && <div className="truncate text-xs text-theme-text-muted/70">{command.description}</div>}
      </button>
      <div className="flex flex-shrink-0 items-center gap-1 opacity-100 sm:opacity-0 sm:transition-opacity sm:group-hover:opacity-100">
        <button type="button" onClick={onRun} className={actionButtonClass(true)} title={t('terminal.quick_commands.run')}>
          <Play className="h-3.5 w-3.5" />
        </button>
        <button type="button" onClick={onEdit} className={actionButtonClass(true)} title={t('terminal.quick_commands.edit')}>
          <Pencil className="h-3.5 w-3.5" />
        </button>
        <button type="button" onClick={onDelete} className={actionButtonClass(true)} title={t('terminal.quick_commands.delete')}>
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
};

type QuickCommandEditorProps = {
  command: QuickCommand | null;
  category: string;
  categories: Array<{ id: string; name: string }>;
  onSave: (draft: QuickCommandDraft) => void;
  onCancel: () => void;
};

type QuickCommandCategoryEditorProps = {
  category: QuickCommandCategory | null;
  onSave: (draft: QuickCommandCategoryDraft) => void;
  onCancel: () => void;
};

const QUICK_COMMAND_ICON_OPTIONS: QuickCommandIcon[] = ['terminal', 'server', 'folder', 'docker', 'zap'];

const QuickCommandCategoryEditor: React.FC<QuickCommandCategoryEditorProps> = ({ category, onSave, onCancel }) => {
  const { t } = useTranslation();
  const [name, setName] = useState(category?.name ?? '');
  const [icon, setIcon] = useState<QuickCommandIcon>(category?.icon ?? 'zap');
  const canSave = name.trim().length > 0;

  useEffect(() => {
    setName(category?.name ?? '');
    setIcon(category?.icon ?? 'zap');
  }, [category]);

  return (
    <div className="border-b border-theme-border/60 bg-theme-bg/35 p-2">
      <div className="grid gap-2 md:grid-cols-[1fr_220px]">
        <input value={name} onChange={(event) => setName(event.target.value)} placeholder={t('terminal.quick_commands.group_name_placeholder')} className={quickCommandInputClass} style={terminalFontStyle} />
        <Select value={icon} onValueChange={(value) => setIcon(value as QuickCommandIcon)}>
          <SelectTrigger className="h-8 border-theme-border/50 bg-theme-bg/70 px-2 text-sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {QUICK_COMMAND_ICON_OPTIONS.map((candidate) => {
              const Icon = quickCommandIcon(candidate);
              return (
                <SelectItem key={candidate} value={candidate}>
                  <span className="inline-flex items-center gap-2">
                    <Icon className="h-3.5 w-3.5" />
                    {t(`terminal.quick_commands.icon_${candidate}`)}
                  </span>
                </SelectItem>
              );
            })}
          </SelectContent>
        </Select>
      </div>
      <div className="mt-2 flex justify-end gap-2">
        <button type="button" onClick={onCancel} className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs text-theme-text-muted hover:bg-theme-bg-hover hover:text-theme-text">
          <X className="h-3.5 w-3.5" />
          {t('terminal.quick_commands.cancel')}
        </button>
        <button
          type="button"
          disabled={!canSave}
          onClick={() => onSave({ id: category?.id, name, icon })}
          className={cn(
            'inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs',
            canSave ? 'bg-theme-accent/15 text-theme-accent hover:bg-theme-accent/25' : 'cursor-not-allowed text-theme-text-muted/40',
          )}
        >
          <Save className="h-3.5 w-3.5" />
          {t('terminal.quick_commands.save_group')}
        </button>
      </div>
    </div>
  );
};

const QuickCommandEditor: React.FC<QuickCommandEditorProps> = ({ command, category, categories, onSave, onCancel }) => {
  const { t } = useTranslation();
  const [name, setName] = useState(command?.name ?? '');
  const [commandText, setCommandText] = useState(command?.command ?? '');
  const [description, setDescription] = useState(command?.description ?? '');
  const [selectedCategory, setSelectedCategory] = useState(command?.category ?? category);
  const [hostPattern, setHostPattern] = useState(command?.hostPattern ?? '');
  const canSave = name.trim().length > 0 && commandText.trim().length > 0;

  useEffect(() => {
    setName(command?.name ?? '');
    setCommandText(command?.command ?? '');
    setDescription(command?.description ?? '');
    setSelectedCategory(command?.category ?? category);
    setHostPattern(command?.hostPattern ?? '');
  }, [category, command]);

  return (
    <div className="border-b border-theme-border/60 bg-theme-bg/35 p-2">
      <div className="grid gap-2 md:grid-cols-[1fr_1.2fr]">
        <input value={name} onChange={(event) => setName(event.target.value)} placeholder={t('terminal.quick_commands.name_placeholder')} className={quickCommandInputClass} style={terminalFontStyle} />
        <input value={commandText} onChange={(event) => setCommandText(event.target.value)} placeholder={t('terminal.quick_commands.command_placeholder')} className={quickCommandInputClass} style={terminalFontStyle} />
        <input value={description} onChange={(event) => setDescription(event.target.value)} placeholder={t('terminal.quick_commands.description_placeholder')} className={quickCommandInputClass} style={terminalFontStyle} />
        <div className="grid grid-cols-2 gap-2">
          <Select value={selectedCategory} onValueChange={setSelectedCategory}>
            <SelectTrigger className="h-8 border-theme-border/50 bg-theme-bg/70 px-2 text-sm">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {categories.map((candidate) => (
                <SelectItem key={candidate.id} value={candidate.id}>
                  {candidate.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <input value={hostPattern} onChange={(event) => setHostPattern(event.target.value)} placeholder={t('terminal.quick_commands.host_pattern_placeholder')} className={quickCommandInputClass} style={terminalFontStyle} />
        </div>
      </div>
      <div className="mt-2 flex justify-end gap-2">
        <button type="button" onClick={onCancel} className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs text-theme-text-muted hover:bg-theme-bg-hover hover:text-theme-text">
          <X className="h-3.5 w-3.5" />
          {t('terminal.quick_commands.cancel')}
        </button>
        <button
          type="button"
          disabled={!canSave}
          onClick={() => onSave({
            id: command?.id,
            name,
            command: commandText,
            description,
            category: selectedCategory,
            hostPattern,
          })}
          className={cn(
            'inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs',
            canSave ? 'bg-theme-accent/15 text-theme-accent hover:bg-theme-accent/25' : 'cursor-not-allowed text-theme-text-muted/40',
          )}
        >
          <Save className="h-3.5 w-3.5" />
          {t('terminal.quick_commands.save')}
        </button>
      </div>
    </div>
  );
};

const terminalFontStyle: React.CSSProperties = { fontFamily: 'var(--terminal-font-family)' };
const quickCommandInputClass = 'h-8 rounded-md border border-theme-border/50 bg-theme-bg/70 px-2 text-sm text-theme-text outline-none placeholder:text-theme-text-muted focus:border-theme-accent/60';

function quickCommandIcon(icon: QuickCommandIcon): React.ComponentType<{ className?: string }> {
  switch (icon) {
    case 'server':
      return Server;
    case 'folder':
      return Folder;
    case 'docker':
      return Container;
    case 'zap':
      return Zap;
    case 'terminal':
    default:
      return Monitor;
  }
}

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

function isComposingKeyEvent(event: React.KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>): boolean {
  const nativeEvent = event.nativeEvent as KeyboardEvent & { isComposing?: boolean };
  return nativeEvent.isComposing === true || nativeEvent.keyCode === 229;
}

function isRadixPortalInteraction(event: PointerEvent): boolean {
  return event.composedPath().some((element) => (
    element instanceof HTMLElement
    && (
      element.hasAttribute('data-radix-popper-content-wrapper')
      || element.getAttribute('role') === 'listbox'
      || element.getAttribute('role') === 'option'
    )
  ));
}

type SuggestionsProps = {
  suggestions: CommandBarCompletion[];
  highlightedIndex: number;
  onPick: (candidate: CommandBarCompletion) => void;
};

const TerminalCommandSuggestions: React.FC<SuggestionsProps> = ({ suggestions, highlightedIndex, onPick }) => {
  const { t } = useTranslation();
  const groupedSuggestions = useMemo(() => {
    const groups = new Map<string, Array<{ candidate: CommandBarCompletion; index: number }>>();
    suggestions.forEach((candidate, index) => {
      const key = groupKey(candidate);
      groups.set(key, [...(groups.get(key) ?? []), { candidate, index }]);
    });
    return [...groups.entries()];
  }, [suggestions]);

  return (
    <div className="absolute bottom-full left-3 mb-2 w-[min(720px,calc(100%-1.5rem))] overflow-hidden rounded-lg border border-theme-border bg-theme-bg-elevated/95 shadow-xl shadow-black/30" style={terminalFontStyle}>
      {groupedSuggestions.map(([group, entries]) => (
        <div key={group}>
          <div className="border-b border-theme-border/50 bg-theme-bg/60 px-3 py-1 text-[10px] font-medium uppercase tracking-wide text-theme-text-muted">
            {t(`terminal.command_bar.${group}`)}
          </div>
          {entries.map(({ candidate, index }) => (
            <button
              key={`${candidate.source}:${candidate.kind}:${candidate.insertText}`}
              type="button"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => onPick(candidate)}
              className={cn(
                'flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm transition-colors',
                index === highlightedIndex ? 'bg-theme-bg-hover text-theme-text' : 'text-theme-text-muted hover:bg-theme-bg-hover/60 hover:text-theme-text',
              )}
            >
              <span className="min-w-0 flex-1 truncate" style={terminalFontStyle}>{candidate.label}</span>
              {candidate.description && (
                <span className="hidden min-w-0 flex-1 truncate text-xs text-theme-text-muted/70 sm:inline">
                  {candidate.description}
                </span>
              )}
              {candidate.risk && (
                <span className={cn(
                  'rounded px-1.5 py-0.5 text-[10px] uppercase tracking-wide',
                  candidate.risk === 'high' ? 'bg-red-500/15 text-red-300' : 'bg-amber-500/15 text-amber-300',
                )}>
                  {candidate.risk}
                </span>
              )}
              <span className="rounded bg-theme-bg-panel px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-theme-text-muted">
                {t(`terminal.command_bar.${sourceKey(candidate)}`, { defaultValue: candidate.source })}
              </span>
            </button>
          ))}
        </div>
      ))}
    </div>
  );
};

function groupKey(candidate: CommandBarCompletion): string {
  if (candidate.source === 'history') return 'group_history';
  if (candidate.source === 'quick_command') return 'group_quick_commands';
  if (candidate.source === 'path') return 'group_path';
  if (candidate.kind === 'option') return 'group_option';
  return 'group_command';
}

function sourceKey(candidate: CommandBarCompletion): string {
  switch (candidate.source) {
    case 'history':
      return 'source_history';
    case 'quick_command':
      return 'source_quick_command';
    case 'fig':
      return candidate.kind === 'option' ? 'source_option' : 'source_command';
    case 'path':
      return 'source_path';
    case 'ai':
      return 'source_ai';
    default:
      return 'source_runtime';
  }
}
