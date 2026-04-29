// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { ToolOutputPreview } from '../tools/protocol';
import { getAiRuntimeEpoch } from './runtimeEpoch';
import type { AiActionRisk, AiTarget } from './types';

export type AiCommandStatus = 'running' | 'completed' | 'error' | 'waiting_for_input' | 'stale';

export type AiCommandRecord = {
  commandId: string;
  targetId?: string;
  sessionId?: string;
  nodeId?: string;
  command: string;
  cwd?: string;
  source: 'ai.run_command' | 'ai.terminal_input' | 'user.terminal_input';
  status: AiCommandStatus;
  exitCode?: number | null;
  startedAt: number;
  finishedAt?: number;
  runtimeEpoch: string;
  approvalMode?: 'default' | 'bypass';
  risk: AiActionRisk;
  outputPreview?: ToolOutputPreview;
  rawOutputRef?: string;
};

const MAX_GLOBAL_RECORDS = 200;
const MAX_SESSION_RECORDS = 50;

let sequence = 0;
const records: AiCommandRecord[] = [];

function nextCommandId(): string {
  sequence += 1;
  return `cmd-${Date.now().toString(36)}-${sequence.toString(36)}`;
}

function trimRecords(): void {
  if (records.length > MAX_GLOBAL_RECORDS) {
    records.splice(0, records.length - MAX_GLOBAL_RECORDS);
  }

  const perSession = new Map<string, number>();
  for (let index = records.length - 1; index >= 0; index -= 1) {
    const record = records[index];
    const key = record.sessionId ?? record.nodeId ?? record.targetId ?? 'global';
    const count = (perSession.get(key) ?? 0) + 1;
    perSession.set(key, count);
    if (count > MAX_SESSION_RECORDS) {
      records.splice(index, 1);
    }
  }
}

export function addAiCommandRecord(input: Omit<AiCommandRecord, 'commandId' | 'startedAt' | 'runtimeEpoch' | 'status'> & {
  commandId?: string;
  startedAt?: number;
  runtimeEpoch?: string;
  status?: AiCommandStatus;
}): AiCommandRecord {
  const record: AiCommandRecord = {
    commandId: input.commandId ?? nextCommandId(),
    startedAt: input.startedAt ?? Date.now(),
    runtimeEpoch: input.runtimeEpoch ?? getAiRuntimeEpoch(),
    status: input.status ?? 'completed',
    ...input,
  };
  records.push(record);
  trimRecords();
  return record;
}

export function listAiCommandRecords(): AiCommandRecord[] {
  return records.slice();
}

export function getRecentAiCommandRecords(limit = 8): AiCommandRecord[] {
  return records.slice(-Math.max(0, limit));
}

export function getAiCommandRecord(commandId: string): AiCommandRecord | undefined {
  return records.find((record) => record.commandId === commandId);
}

export function clearAiCommandLedger(): void {
  records.length = 0;
}

export function commandRecordFromToolResult(input: {
  toolName: string;
  args: Record<string, unknown>;
  target?: AiTarget;
  ok: boolean;
  waitingForInput?: boolean;
  risk: AiActionRisk;
  approvalMode?: 'default' | 'bypass';
  outputPreview?: ToolOutputPreview;
  rawOutputStored?: boolean;
  exitCode?: number | null;
}): AiCommandRecord | null {
  if (input.toolName !== 'run_command' && input.toolName !== 'send_terminal_input') {
    return null;
  }

  const commandValue = input.toolName === 'run_command'
    ? input.args.command
    : input.args.text ?? input.args.keys ?? input.args.sequence;
  const command = typeof commandValue === 'string'
    ? commandValue
    : Array.isArray(commandValue)
      ? commandValue.join(' ')
      : '';
  if (!command.trim()) return null;

  const target = input.target;
  return addAiCommandRecord({
    targetId: target?.id,
    sessionId: target?.refs.sessionId,
    nodeId: target?.refs.nodeId,
    cwd: typeof input.args.cwd === 'string' ? input.args.cwd : undefined,
    command,
    source: input.toolName === 'run_command' ? 'ai.run_command' : 'ai.terminal_input',
    status: input.waitingForInput ? 'waiting_for_input' : input.ok ? 'completed' : 'error',
    finishedAt: Date.now(),
    approvalMode: input.approvalMode,
    risk: input.risk,
    outputPreview: input.outputPreview,
    rawOutputRef: input.rawOutputStored ? 'tool-result.rawOutput' : undefined,
    exitCode: input.exitCode,
  });
}
