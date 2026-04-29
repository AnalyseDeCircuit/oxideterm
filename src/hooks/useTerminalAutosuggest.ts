// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useCallback, useEffect, useRef, useState } from 'react';
import type { Terminal } from '@xterm/xterm';
import type { RefObject } from 'react';
import {
  getTerminalAutosuggestion,
  importTerminalAutosuggestCommands,
  loadLocalShellHistoryCommands,
  recordTerminalAutosuggestCommand,
  TerminalAutosuggestInputTracker,
  type TerminalAutosuggestPosition,
  type TerminalAutosuggestSettings,
} from '@/lib/terminal/autosuggest';

type TerminalKind = 'terminal' | 'local_terminal';

type UseTerminalAutosuggestOptions = {
  terminalRef: RefObject<Terminal | null>;
  containerRef: RefObject<HTMLElement | null>;
  settings: TerminalAutosuggestSettings;
  isActive: boolean;
  terminalKind: TerminalKind;
  disabled: () => boolean;
  sendInput: (text: string) => void;
};

type TerminalAutosuggestState = {
  ghostText: string | null;
  ghostPosition: TerminalAutosuggestPosition | null;
};

export function useTerminalAutosuggest(options: UseTerminalAutosuggestOptions) {
  const {
    terminalRef,
    containerRef,
    settings,
    isActive,
    terminalKind,
    disabled,
    sendInput,
  } = options;
  const trackerRef = useRef(new TerminalAutosuggestInputTracker());
  const [state, setState] = useState<TerminalAutosuggestState>({
    ghostText: null,
    ghostPosition: null,
  });

  const computeGhostPosition = useCallback((): TerminalAutosuggestPosition | null => {
    const term = terminalRef.current;
    const container = containerRef.current;
    const screen = term?.element?.querySelector('.xterm-screen') as HTMLElement | null;
    if (!term || !container || !screen || term.cols <= 0 || term.rows <= 0) return null;

    const overlayRoot = container.parentElement ?? container;
    const rootRect = overlayRoot.getBoundingClientRect();
    const screenRect = screen.getBoundingClientRect();
    const cellWidth = screenRect.width / term.cols;
    const lineHeight = screenRect.height / term.rows;
    const buffer = term.buffer.active;
    return {
      left: screenRect.left - rootRect.left + buffer.cursorX * cellWidth,
      top: screenRect.top - rootRect.top + buffer.cursorY * lineHeight,
      lineHeight,
    };
  }, [containerRef, terminalRef]);

  const refreshGhost = useCallback(() => {
    const term = terminalRef.current;
    if (!settings.enabled || !isActive || !term || disabled() || term.buffer.active.type === 'alternate') {
      setState((current) => current.ghostText || current.ghostPosition
        ? { ...current, ghostText: null, ghostPosition: null }
        : current);
      return;
    }

    const input = trackerRef.current.getState();
    if (!input.value || !input.isCursorAtEnd) {
      setState((current) => current.ghostText || current.ghostPosition
        ? { ...current, ghostText: null, ghostPosition: null }
        : current);
      return;
    }

    const ghostText = getTerminalAutosuggestion(input.value);
    const ghostPosition = ghostText ? computeGhostPosition() : null;
    setState((current) => (
      current.ghostText === ghostText
      && positionsEqual(current.ghostPosition, ghostPosition)
        ? current
        : { ...current, ghostText, ghostPosition }
    ));
  }, [computeGhostPosition, disabled, isActive, settings.enabled, terminalRef]);

  const observeInput = useCallback((data: string) => {
    const result = trackerRef.current.applyData(data);
    if (result.completedCommand) {
      recordTerminalAutosuggestCommand(result.completedCommand, 'runtime');
    }
    if (result.changed) {
      window.setTimeout(refreshGhost, 0);
    }
  }, [refreshGhost]);

  const clearSuggestion = useCallback(() => {
    setState((current) => current.ghostText || current.ghostPosition
      ? { ...current, ghostText: null, ghostPosition: null }
      : current);
  }, []);

  const resetInput = useCallback(() => {
    trackerRef.current.reset();
    clearSuggestion();
  }, [clearSuggestion]);

  const acceptGhost = useCallback((): boolean => {
    const ghostText = state.ghostText;
    if (!ghostText || disabled()) return false;
    const input = trackerRef.current.getState();
    if (!input.isCursorAtEnd) return false;
    trackerRef.current.accept(ghostText);
    sendInput(ghostText);
    clearSuggestion();
    return true;
  }, [clearSuggestion, disabled, sendInput, state.ghostText]);

  const handleKeyEvent = useCallback((event: KeyboardEvent): boolean => {
    if (!settings.enabled || !isActive) return true;
    if (event.type !== 'keydown') return true;

    const isPlainAcceptKey = !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey
      && (event.key === 'ArrowRight' || event.key === 'End');
    if (isPlainAcceptKey && acceptGhost()) {
      event.preventDefault();
      event.stopPropagation();
      return false;
    }

    return true;
  }, [acceptGhost, isActive, settings.enabled]);

  useEffect(() => {
    if (terminalKind !== 'local_terminal' || !settings.localShellHistory) return;
    let cancelled = false;
    void loadLocalShellHistoryCommands().then((commands) => {
      if (!cancelled) {
        importTerminalAutosuggestCommands(commands, 'local-history');
      }
    });
    return () => {
      cancelled = true;
    };
  }, [settings.localShellHistory, terminalKind]);

  return {
    ghostText: state.ghostText,
    ghostPosition: state.ghostPosition,
    observeInput,
    handleKeyEvent,
    refreshGhost,
    clearSuggestion,
    resetInput,
  };
}

function positionsEqual(a: TerminalAutosuggestPosition | null, b: TerminalAutosuggestPosition | null): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return Math.abs(a.left - b.left) < 0.5
    && Math.abs(a.top - b.top) < 0.5
    && Math.abs(a.lineHeight - b.lineHeight) < 0.5;
}
