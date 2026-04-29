// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export { TerminalAutosuggestInputTracker } from './inputTracker';
export {
  clearTerminalAutosuggestHistory,
  getTerminalAutosuggestCandidates,
  getTerminalAutosuggestion,
  importTerminalAutosuggestCommands,
  recordTerminalAutosuggestCommand,
} from './history';
export { isLikelySecretCommand } from './secrets';
export { loadLocalShellHistoryCommands } from './localHistory';
export type {
  TerminalAutosuggestCandidate,
  TerminalAutosuggestInputState,
  TerminalAutosuggestPosition,
  TerminalAutosuggestSettings,
} from './types';
