// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { useCallback, useEffect, useRef } from 'react';
import {
  importTerminalAutosuggestCommands,
  loadLocalShellHistoryCommands,
  recordTerminalAutosuggestCommand,
  TerminalAutosuggestInputTracker,
} from '@/lib/terminal/autosuggest';

type TerminalKind = 'terminal' | 'local_terminal';

export function useTerminalAutosuggestRecorder(options: {
  terminalKind: TerminalKind;
  localShellHistory: boolean;
}) {
  const { terminalKind, localShellHistory } = options;
  const trackerRef = useRef(new TerminalAutosuggestInputTracker());

  const observeInput = useCallback((data: string) => {
    const result = trackerRef.current.applyData(data);
    if (result.completedCommand) {
      recordTerminalAutosuggestCommand(result.completedCommand, 'runtime');
    }
    return result;
  }, []);

  const resetInput = useCallback(() => {
    trackerRef.current.reset();
  }, []);

  useEffect(() => {
    if (terminalKind !== 'local_terminal' || !localShellHistory) return;
    let cancelled = false;
    void loadLocalShellHistoryCommands().then((commands) => {
      if (!cancelled) {
        importTerminalAutosuggestCommands(commands, 'local-history');
      }
    });
    return () => {
      cancelled = true;
    };
  }, [localShellHistory, terminalKind]);

  return { observeInput, resetInput };
}
