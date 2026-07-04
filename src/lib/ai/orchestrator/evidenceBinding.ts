// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiChatMessage, AiToolResult } from '../../../types';
import type { AiAssistantTurn, AiTurnPart } from '../turnModel/types';
import { fromLegacyToolResult } from '../tools/protocol';
import { getAiRuntimeEpoch } from './runtimeEpoch';

export type AiToolExecutionRecord = {
  recordId: string;
  conversationId: string;
  assistantMessageId: string;
  toolCallId: string;
  toolName: string;
  argumentSummary: string;
  targetId?: string;
  targetKind?: string;
  risk: string;
  approvalSource?: string;
  executionSurface: string;
  visibleInTerminal?: boolean;
  status: string;
  success?: boolean;
  errorCode?: string;
  resultSummary?: string;
  durationMs?: number;
  startedAt: number;
  finishedAt?: number;
  runtimeEpoch: string;
};

export type AiToolResultFact = {
  factId: string;
  conversationId: string;
  assistantMessageId: string;
  toolCallId: string;
  toolName: string;
  sourceKind: string;
  textHash: string;
  summary: string;
  outputPreview: string;
  createdAt: number;
  runtimeEpoch: string;
};

const MAX_TOOL_EXECUTION_RECORDS = 1000;
const MAX_TOOL_RESULT_FACTS = 1000;

const toolExecutionRecords: AiToolExecutionRecord[] = [];
const toolResultFacts: AiToolResultFact[] = [];

function trimLedger<T>(items: T[], maxItems: number): void {
  if (items.length > maxItems) {
    items.splice(0, items.length - maxItems);
  }
}

function truncateRecordText(value: string, maxChars: number): string {
  const chars = Array.from(value);
  if (chars.length <= maxChars) {
    return value;
  }
  return `${chars.slice(0, maxChars).join('')}...`;
}

function fnv1a64(value: string): string {
  // Browser builds need a synchronous audit hash; this is for change tracking,
  // not for cryptographic integrity.
  let hash = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  for (const byte of new TextEncoder().encode(value)) {
    hash ^= BigInt(byte);
    hash = (hash * prime) & mask;
  }
  return `fnv1a64:${hash.toString(16).padStart(16, '0')}`;
}

function factValueText(value: unknown): string {
  return typeof value === 'string' ? value : JSON.stringify(value);
}

function firstLine(value: string): string {
  return value.split('\n')[0] ?? '';
}

function factFromText(
  record: AiToolExecutionRecord,
  sourceKind: string,
  text: string,
  now: number,
): AiToolResultFact {
  const outputPreview = truncateRecordText(text, 4000);
  return {
    factId: `${record.toolCallId}.${sourceKind}`,
    conversationId: record.conversationId,
    assistantMessageId: record.assistantMessageId,
    toolCallId: record.toolCallId,
    toolName: record.toolName,
    sourceKind,
    textHash: fnv1a64(text),
    summary: truncateRecordText(firstLine(text), 240),
    outputPreview,
    createdAt: now,
    runtimeEpoch: record.runtimeEpoch,
  };
}

export function evidenceFactsForModel(result: AiToolResult): Array<{
  factId: string;
  toolCallId: string;
  toolName: string;
  sourceKind: string;
}> {
  const envelope = fromLegacyToolResult(result);
  const candidates: Array<[string, unknown]> = [
    ['summary', envelope.summary],
    ['output', envelope.output],
    ['execution.exit_code', envelope.execution?.exitCode],
    ['execution.visible_in_terminal', (envelope.execution as { visibleInTerminal?: unknown } | undefined)?.visibleInTerminal],
    ['execution.state', (envelope.execution as { state?: unknown } | undefined)?.state],
  ];

  return candidates
    .filter(([, value]) => value !== undefined && !(typeof value === 'string' && value.trim() === ''))
    .map(([sourceKind]) => ({
      factId: `${result.toolCallId}.${sourceKind}`,
      toolCallId: result.toolCallId,
      toolName: result.toolName,
      sourceKind,
    }));
}

export function recordAiToolExecution(record: AiToolExecutionRecord): void {
  const index = toolExecutionRecords.findIndex((entry) => entry.recordId === record.recordId);
  if (index === -1) {
    toolExecutionRecords.push(record);
  } else {
    toolExecutionRecords[index] = record;
  }
  trimLedger(toolExecutionRecords, MAX_TOOL_EXECUTION_RECORDS);
}

export function extractAiToolResultFacts(
  record: AiToolExecutionRecord,
  result: AiToolResult | undefined,
  now = Date.now(),
): AiToolResultFact[] {
  if (!result) {
    return [];
  }

  const envelope = fromLegacyToolResult(result);
  const facts: AiToolResultFact[] = [];
  if (envelope.summary.trim()) {
    facts.push(factFromText(record, 'summary', envelope.summary, now));
  }
  if (envelope.output.trim()) {
    facts.push(factFromText(record, 'output', envelope.output, now));
  }
  if (envelope.execution && Object.prototype.hasOwnProperty.call(envelope.execution, 'exitCode')) {
    facts.push(factFromText(record, 'execution.exit_code', `exit_code: ${factValueText(envelope.execution.exitCode ?? null)}`, now));
  } else if (envelope.data && typeof envelope.data === 'object' && Object.prototype.hasOwnProperty.call(envelope.data, 'exitCode')) {
    facts.push(factFromText(record, 'execution.exit_code', `exit_code: ${factValueText((envelope.data as { exitCode?: unknown }).exitCode ?? null)}`, now));
  }

  const executionExtras = envelope.execution as { visibleInTerminal?: unknown; state?: unknown } | undefined;
  const dataExtras = envelope.data as { visibleInTerminal?: unknown; executionState?: unknown } | undefined;
  const visibleInTerminal = executionExtras?.visibleInTerminal ?? dataExtras?.visibleInTerminal;
  if (visibleInTerminal !== undefined) {
    facts.push(factFromText(record, 'execution.visible_in_terminal', `visible_in_terminal: ${factValueText(visibleInTerminal)}`, now));
  }
  const executionState = executionExtras?.state ?? dataExtras?.executionState;
  if (executionState !== undefined) {
    facts.push(factFromText(record, 'execution.state', `execution_state: ${factValueText(executionState)}`, now));
  }

  return facts;
}

export function recordAiToolResultFacts(
  record: AiToolExecutionRecord,
  result: AiToolResult | undefined,
  now = Date.now(),
): AiToolResultFact[] {
  const facts = extractAiToolResultFacts(record, result, now);
  for (const fact of facts) {
    const existingIndex = toolResultFacts.findIndex((entry) => (
      entry.conversationId === fact.conversationId
      && entry.assistantMessageId === fact.assistantMessageId
      && entry.factId === fact.factId
    ));
    if (existingIndex !== -1) {
      toolResultFacts.splice(existingIndex, 1);
    }
  }
  toolResultFacts.push(...facts);
  trimLedger(toolResultFacts, MAX_TOOL_RESULT_FACTS);
  return facts;
}

export function aiToolResultFactsForMessage(
  conversationId: string,
  assistantMessageId: string,
): AiToolResultFact[] {
  // Result binding is intentionally local to the assistant turn that produced
  // the tool output; old transcript facts cannot prove a new "I checked" claim.
  return toolResultFacts.filter((fact) => (
    fact.conversationId === conversationId && fact.assistantMessageId === assistantMessageId
  ));
}

export function clearAiToolEvidenceLedger(): void {
  toolExecutionRecords.length = 0;
  toolResultFacts.length = 0;
}

function extractEvidenceClaimsBlock(text: string): { visibleText: string } | null {
  const open = '<evidence_claims>';
  const close = '</evidence_claims>';
  const start = text.indexOf(open);
  if (start === -1) {
    return null;
  }
  const blockStart = start + open.length;
  const closeStart = text.indexOf(close, blockStart);
  if (closeStart === -1) {
    throw new Error('evidence claims block missing closing tag');
  }
  const closeEnd = closeStart + close.length;
  if (text.slice(closeEnd).includes(open)) {
    throw new Error('multiple evidence claims blocks are not supported');
  }
  return {
    visibleText: `${text.slice(0, start)}${text.slice(closeEnd)}`.trim(),
  };
}

export function stripEvidenceClaimsFromText(text: string): string {
  let nextText = text;
  while (true) {
    try {
      const extracted = extractEvidenceClaimsBlock(nextText);
      if (!extracted) {
        return nextText.trim();
      }
      nextText = extracted.visibleText;
    } catch {
      const start = nextText.indexOf('<evidence_claims>');
      return (start === -1 ? nextText : nextText.slice(0, start)).trim();
    }
  }
}

function stripEvidenceBlockFromTextParts(parts: readonly AiTurnPart[]): AiTurnPart[] {
  return parts
    .map((part): AiTurnPart => {
      if (part.type !== 'text') {
        return part;
      }
      return { ...part, text: stripEvidenceClaimsFromText(part.text) };
    })
    .filter((part) => part.type !== 'text' || part.text.length > 0);
}

function stripEvidenceBlockFromTurn(turn: AiAssistantTurn | undefined): AiAssistantTurn | undefined {
  if (!turn) {
    return undefined;
  }
  const parts = stripEvidenceBlockFromTextParts(turn.parts);
  return {
    ...turn,
    parts,
    plainTextSummary: parts
      .filter((part): part is Extract<AiTurnPart, { type: 'text' }> => part.type === 'text')
      .map((part) => part.text)
      .join(''),
  };
}

export function stripAiEvidenceClaims(message: AiChatMessage): AiChatMessage {
  const turn = stripEvidenceBlockFromTurn(message.turn);
  return {
    ...message,
    content: stripEvidenceClaimsFromText(message.content),
    turn,
  };
}

export function buildAiToolExecutionRecord(input: {
  conversationId: string;
  assistantMessageId: string;
  toolCallId: string;
  toolName: string;
  args: Record<string, unknown>;
  status: string;
  result?: AiToolResult;
  risk: string;
  startedAt?: number;
  finishedAt?: number;
  runtimeEpoch?: string;
}): AiToolExecutionRecord {
  const result = input.result;
  const envelope = result ? fromLegacyToolResult(result) : undefined;
  const targetId = envelope?.meta.targetId
    ?? envelope?.execution?.target?.id
    ?? envelope?.targets?.[0]?.id
    ?? (typeof input.args.target_id === 'string' ? input.args.target_id : undefined);
  const targetKind = envelope?.execution?.target?.kind ?? envelope?.targets?.[0]?.kind;
  const executionExtras = envelope?.execution as { visibleInTerminal?: boolean } | undefined;
  const dataExtras = envelope?.data as { visibleInTerminal?: boolean } | undefined;
  const visibleInTerminal = executionExtras?.visibleInTerminal ?? dataExtras?.visibleInTerminal;
  const approvalSource = envelope?.meta.approvalMode ?? envelope?.meta.policyDecision?.approvalMode;

  return {
    recordId: `${input.assistantMessageId}:${input.toolCallId}`,
    conversationId: input.conversationId,
    assistantMessageId: input.assistantMessageId,
    toolCallId: input.toolCallId,
    toolName: input.toolName,
    argumentSummary: summarizeToolArguments(input.toolName, input.args),
    targetId,
    targetKind,
    risk: input.risk,
    approvalSource,
    executionSurface: resolveExecutionSurface(input.toolName, input.args, visibleInTerminal),
    visibleInTerminal,
    status: input.status,
    success: result?.success,
    errorCode: envelope?.error?.code,
    resultSummary: envelope?.summary,
    durationMs: result?.durationMs ?? envelope?.meta.durationMs,
    startedAt: input.startedAt ?? Date.now(),
    finishedAt: input.finishedAt,
    runtimeEpoch: envelope?.meta.runtimeEpoch ?? input.runtimeEpoch ?? getAiRuntimeEpoch(),
  };
}

function summarizeToolArguments(toolName: string, args: Record<string, unknown>): string {
  // Summaries intentionally avoid large write payloads and secret-like content.
  switch (toolName) {
    case 'run_command': {
      const target = typeof args.target_id === 'string' ? args.target_id : '<missing target>';
      const command = typeof args.command === 'string' ? truncateRecordText(args.command, 200) : '<missing command>';
      const cwd = typeof args.cwd === 'string' && args.cwd.trim() ? ` cwd=${truncateRecordText(args.cwd, 120)}` : '';
      return `target=${target}${cwd} command=${command}`;
    }
    case 'send_terminal_input': {
      const textChars = typeof args.text === 'string' ? Array.from(args.text).length : 0;
      return `text_chars=${textChars} append_enter=${args.append_enter === true}`;
    }
    case 'read_resource':
    case 'write_resource':
    case 'transfer_resource': {
      const target = typeof args.target_id === 'string' ? args.target_id : '<missing target>';
      const resource = typeof args.resource === 'string' ? args.resource : '<missing resource>';
      const path = typeof args.path === 'string' ? ` path=${truncateRecordText(args.path, 160)}` : '';
      return `target=${target} resource=${resource}${path}`;
    }
    case 'connect_target':
      return `target=${typeof args.target_id === 'string' ? args.target_id : '<missing target>'}`;
    case 'open_app_surface':
      return `surface=${typeof args.surface === 'string' ? args.surface : '<missing surface>'}`;
    default:
      return `keys=${Object.keys(args).sort().join(',')}`;
  }
}

function resolveExecutionSurface(
  toolName: string,
  args: Record<string, unknown>,
  visibleInTerminal: boolean | undefined,
): string {
  if (visibleInTerminal === true) {
    return 'visible_terminal';
  }
  if (toolName === 'run_command') {
    return args.target_id === 'local-shell:default' ? 'local_process' : 'background_capture';
  }
  if (toolName === 'send_terminal_input') {
    return 'visible_terminal';
  }
  if (toolName === 'connect_target' || toolName === 'open_app_surface' || toolName === 'remember_preference') {
    return 'ui_action';
  }
  if (toolName === 'read_resource' || toolName === 'write_resource' || toolName === 'transfer_resource') {
    return args.resource === 'settings' ? 'settings' : 'filesystem';
  }
  return 'app_state';
}
