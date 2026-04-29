// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { getRecentAiCommandRecords } from '@/lib/ai/orchestrator/ledger';
import { isLikelySecretCommand } from './secrets';
import type { TerminalAutosuggestCandidate } from './types';

type HistoryEntry = {
  command: string;
  source: TerminalAutosuggestCandidate['source'];
  lastUsedAt: number;
  uses: number;
  sequence: number;
};

const MAX_HISTORY = 1000;
const entries = new Map<string, HistoryEntry>();
let sequenceCounter = 0;

function normalizeCommand(command: string): string {
  return command.replace(/\s+/g, ' ').trim();
}

function putCommand(
  command: string,
  source: TerminalAutosuggestCandidate['source'],
  lastUsedAt = Date.now(),
  countUse = true,
): void {
  const normalized = normalizeCommand(command);
  if (!normalized || normalized.length > 2000 || isLikelySecretCommand(normalized)) return;

  const existing = entries.get(normalized);
  if (existing) {
    existing.lastUsedAt = Math.max(existing.lastUsedAt, lastUsedAt);
    existing.sequence = ++sequenceCounter;
    if (countUse) {
      existing.uses += 1;
    }
    return;
  }

  entries.set(normalized, { command: normalized, source, lastUsedAt, uses: 1, sequence: ++sequenceCounter });
  if (entries.size > MAX_HISTORY) {
    const oldest = [...entries.values()].sort((a, b) => a.lastUsedAt - b.lastUsedAt)[0];
    if (oldest) entries.delete(oldest.command);
  }
}

export function recordTerminalAutosuggestCommand(command: string, source: TerminalAutosuggestCandidate['source'] = 'runtime'): void {
  putCommand(command, source);
}

export function importTerminalAutosuggestCommands(
  commands: Iterable<string>,
  source: TerminalAutosuggestCandidate['source'],
): void {
  let offset = 0;
  for (const command of commands) {
    putCommand(command, source, Date.now() - offset, false);
    offset += 1;
  }
}

function seedFromAiLedger(): void {
  for (const record of getRecentAiCommandRecords(80)) {
    putCommand(record.command, 'ai-ledger', record.finishedAt ?? record.startedAt, false);
  }
}

function fuzzyScore(command: string, query: string): number {
  if (!query) return 0;
  if (command.startsWith(query)) return 1000 + query.length * 8;
  const lowerCommand = command.toLowerCase();
  const lowerQuery = query.toLowerCase();
  if (lowerCommand.startsWith(lowerQuery)) return 850 + query.length * 6;
  if (lowerCommand.includes(lowerQuery)) return 450 + query.length * 4;

  let qi = 0;
  let score = 0;
  for (let ci = 0; ci < lowerCommand.length && qi < lowerQuery.length; ci += 1) {
    if (lowerCommand[ci] === lowerQuery[qi]) {
      score += 20;
      qi += 1;
    }
  }
  return qi === lowerQuery.length ? score : 0;
}

export function getTerminalAutosuggestCandidates(query: string, limit = 8): TerminalAutosuggestCandidate[] {
  const trimmed = query.trimStart();

  seedFromAiLedger();
  const now = Date.now();
  const candidates: TerminalAutosuggestCandidate[] = [];
  for (const entry of entries.values()) {
    const fuzzy = fuzzyScore(entry.command, trimmed);
    if (trimmed && (fuzzy <= 0 || entry.command === trimmed)) continue;
    const recency = Math.max(0, 200 - Math.floor((now - entry.lastUsedAt) / 60_000));
    const score = (trimmed ? fuzzy + recency + entry.uses * 5 : recency + entry.uses * 5) + entry.sequence / 1_000_000;
    candidates.push({ command: entry.command, source: entry.source, lastUsedAt: entry.lastUsedAt, score });
  }

  return candidates
    .sort((a, b) => b.score - a.score || b.lastUsedAt - a.lastUsedAt || a.command.localeCompare(b.command))
    .slice(0, limit);
}

export function getTerminalAutosuggestion(input: string): string | null {
  const query = input.trimStart();
  if (!query) return null;
  const leading = input.slice(0, input.length - query.length);
  const candidate = getTerminalAutosuggestCandidates(query, 1)[0];
  if (!candidate || !candidate.command.startsWith(query) || candidate.command === query) return null;
  return `${leading}${candidate.command}`.slice(input.length);
}

export function clearTerminalAutosuggestHistory(): void {
  entries.clear();
  sequenceCounter = 0;
}
