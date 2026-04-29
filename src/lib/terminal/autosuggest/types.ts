// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export interface TerminalAutosuggestSettings {
  enabled: boolean;
  localShellHistory: boolean;
}

export interface TerminalAutosuggestCandidate {
  command: string;
  source: 'runtime' | 'local-history' | 'ai-ledger';
  lastUsedAt: number;
  score: number;
}

export interface TerminalAutosuggestInputState {
  value: string;
  cursorIndex: number;
  isCursorAtEnd: boolean;
}

export interface TerminalAutosuggestPosition {
  left: number;
  top: number;
  lineHeight: number;
}
