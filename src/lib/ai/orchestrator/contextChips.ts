// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { listCliAgentSessions } from './cliAgents';
import { getRecentAiCommandRecords } from './ledger';

export type AiContextChip = {
  id: string;
  kind: 'current_target' | 'current_terminal' | 'recent_command' | 'recent_error' | 'selection' | 'buffer' | 'cli_agent';
  label: string;
  summary: string;
  stale?: boolean;
  metadata?: Record<string, unknown>;
};

export function buildRuntimeContextChips(): AiContextChip[] {
  const chips: AiContextChip[] = [];
  const recentCommands = getRecentAiCommandRecords(5);

  for (const record of recentCommands) {
    chips.push({
      id: `recent-command:${record.commandId}`,
      kind: record.status === 'error' ? 'recent_error' : 'recent_command',
      label: record.status === 'error' ? 'Recent error' : 'Recent command',
      summary: `${record.command}${record.exitCode !== undefined ? ` (exit ${record.exitCode ?? 'unknown'})` : ''}`,
      metadata: {
        commandRecordId: record.commandId,
        targetId: record.targetId,
        sessionId: record.sessionId,
        nodeId: record.nodeId,
        status: record.status,
        runtimeEpoch: record.runtimeEpoch,
      },
    });
  }

  for (const session of listCliAgentSessions().slice(0, 3)) {
    chips.push({
      id: `cli-agent:${session.id}`,
      kind: 'cli_agent',
      label: session.label,
      summary: `${session.kind} is ${session.status}${session.sessionId ? ` in ${session.sessionId}` : ''}`,
      metadata: {
        cliAgentSessionId: session.id,
        kind: session.kind,
        status: session.status,
        targetId: session.targetId,
        sessionId: session.sessionId,
        runtimeEpoch: session.runtimeEpoch,
      },
    });
  }

  return chips;
}

export function formatContextChipsForPrompt(chips: AiContextChip[]): string {
  if (chips.length === 0) return '';
  return [
    '## Runtime Context Chips',
    'These are current-runtime structured hints. Treat chips as stale if their runtimeEpoch differs from current tool results.',
    ...chips.slice(0, 8).map((chip) => `- ${chip.kind}: ${chip.summary}${chip.metadata ? ` ${JSON.stringify(chip.metadata)}` : ''}`),
  ].join('\n');
}
