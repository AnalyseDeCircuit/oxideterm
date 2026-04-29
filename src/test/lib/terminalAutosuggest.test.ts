import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@/lib/ai/orchestrator/ledger', () => ({
  getRecentAiCommandRecords: () => [],
}));

import {
  clearTerminalAutosuggestHistory,
  getTerminalAutosuggestCandidates,
  getTerminalAutosuggestion,
  isLikelySecretCommand,
  recordTerminalAutosuggestCommand,
  TerminalAutosuggestInputTracker,
} from '@/lib/terminal/autosuggest';

describe('terminal autosuggest', () => {
  beforeEach(() => {
    clearTerminalAutosuggestHistory();
  });

  it('tracks editable command input and records completed commands', () => {
    const tracker = new TerminalAutosuggestInputTracker();

    expect(tracker.applyData('git st')).toMatchObject({ changed: true });
    expect(tracker.getState()).toMatchObject({
      value: 'git st',
      cursorIndex: 6,
      isCursorAtEnd: true,
    });

    tracker.applyData('atus');
    expect(tracker.getState().value).toBe('git status');

    const result = tracker.applyData('\r');
    expect(result.completedCommand).toBe('git status');
    expect(tracker.getState().value).toBe('');
  });

  it('only offers suffix ghost text for prefix matches', () => {
    recordTerminalAutosuggestCommand('git status');
    recordTerminalAutosuggestCommand('git stash list');

    expect(getTerminalAutosuggestion('git sta')).toBeTruthy();
    expect(getTerminalAutosuggestion('git status')).toBeNull();
  });

  it('deduplicates and ranks recent command history', () => {
    recordTerminalAutosuggestCommand('pnpm test');
    recordTerminalAutosuggestCommand('pnpm test');
    recordTerminalAutosuggestCommand('pnpm exec tsc --noEmit');

    const matches = getTerminalAutosuggestCandidates('pnpm', 10);
    expect(matches.map((match) => match.command)).toEqual([
      'pnpm test',
      'pnpm exec tsc --noEmit',
    ]);
  });

  it('returns recent history when the query is empty', () => {
    recordTerminalAutosuggestCommand('git status');
    recordTerminalAutosuggestCommand('ls -la');

    const matches = getTerminalAutosuggestCandidates('', 10);
    expect(matches.map((match) => match.command)).toEqual([
      'ls -la',
      'git status',
    ]);
  });

  it('filters likely secret commands from suggestions', () => {
    recordTerminalAutosuggestCommand('curl -H "Authorization: Bearer abc" https://example.com');
    recordTerminalAutosuggestCommand('export OPENAI_API_KEY=sk-test');
    recordTerminalAutosuggestCommand('ls -la');

    expect(isLikelySecretCommand('cmd --password hunter2')).toBe(true);
    expect(getTerminalAutosuggestCandidates('curl')).toEqual([]);
    expect(getTerminalAutosuggestCandidates('export')).toEqual([]);
    expect(getTerminalAutosuggestion('ls')).toBe(' -la');
  });
});
