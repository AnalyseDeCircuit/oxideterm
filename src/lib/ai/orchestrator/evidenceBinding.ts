// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiChatMessage, AiToolResult } from '../../../types';
import type { AiAssistantTurn, AiEvidenceClaim, AiTurnPart } from '../turnModel/types';
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

export type AiResultBindingGuardrail = {
  message: string;
  rawText: string;
};

type ParsedEvidenceClaims = {
  visibleText: string;
  claims: Array<Omit<AiEvidenceClaim, 'status'>>;
};

const GUARDRAIL_MESSAGE = 'I do not have tool-result evidence for that claim, so I cannot present it as a verified fact yet. I need to run the appropriate tool first.';
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

function stripEvidenceClaimsCodeFence(text: string): string {
  const trimmed = text.trim();
  if (!trimmed.startsWith('```')) {
    return trimmed;
  }
  const firstNewline = trimmed.indexOf('\n');
  if (firstNewline === -1) {
    return trimmed;
  }
  const body = trimmed.slice(firstNewline + 1);
  return body.endsWith('```') ? body.slice(0, -3).trim() : body;
}

function extractEvidenceClaimsBlock(text: string): { visibleText: string; block: string } | null {
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
    block: text.slice(blockStart, closeStart),
  };
}

function parseEvidenceClaims(text: string): ParsedEvidenceClaims | null {
  const extracted = extractEvidenceClaimsBlock(text);
  if (!extracted) {
    return null;
  }
  const parsed = JSON.parse(stripEvidenceClaimsCodeFence(extracted.block));
  const claimsValue = Array.isArray(parsed) ? parsed : parsed?.claims;
  if (!Array.isArray(claimsValue)) {
    throw new Error('evidence claims must be an object with claims[] or an array');
  }

  const claims = claimsValue.map((claim: unknown): Omit<AiEvidenceClaim, 'status'> => {
    if (!claim || typeof claim !== 'object') {
      throw new Error('each evidence claim must be an object');
    }
    const object = claim as Record<string, unknown>;
    const evidenceValue = object.evidence ?? object.evidenceFactIds;
    if (!Array.isArray(evidenceValue)) {
      throw new Error('each evidence claim must include evidence[]');
    }
    return {
      text: typeof object.text === 'string' ? object.text.trim() : '',
      evidence: evidenceValue
        .filter((value): value is string => typeof value === 'string')
        .map((value) => value.trim())
        .filter(Boolean),
      confidence: typeof object.confidence === 'string' ? object.confidence.trim() : 'verified',
    };
  });

  return { visibleText: extracted.visibleText, claims };
}

function compactEvidenceText(value: string): string {
  return Array.from(value)
    .filter((character) => !/\s/.test(character) && !['*', '`', ','].includes(character))
    .join('');
}

function pushEvidenceToken(tokens: string[], current: string): void {
  if (current && /\d/.test(current)) {
    tokens.push(current);
  }
}

function numericEvidenceTokens(text: string): string[] {
  const tokens: string[] = [];
  let current = '';
  for (const character of text) {
    const allowedAfterDigit = /[a-z0-9.:%-]/i.test(character);
    if (/\d/.test(character) || (current && allowedAfterDigit)) {
      current += character.toLowerCase();
      continue;
    }
    pushEvidenceToken(tokens, current);
    current = '';
  }
  pushEvidenceToken(tokens, current);
  return Array.from(new Set(tokens)).sort();
}

function recentFactsSupportText(text: string, facts: readonly AiToolResultFact[]): boolean {
  if (facts.length === 0) {
    return false;
  }
  const tokens = numericEvidenceTokens(text);
  if (tokens.length === 0) {
    return false;
  }
  const support = compactEvidenceText(facts.map((fact) => fact.outputPreview).join('\n').toLowerCase());
  return tokens.every((token) => support.includes(compactEvidenceText(token)));
}

function textClaimsToolBackedFact(text: string): boolean {
  const normalized = text.toLowerCase();
  const englishMarkers = [
    'i ran ',
    'i executed ',
    'i checked ',
    'i verified ',
    'command output',
    'exit code',
    'stdout',
    'stderr',
    'system load',
    'load average',
    'uptime',
    'disk',
    'memory',
  ];
  if (englishMarkers.some((marker) => normalized.includes(marker))) {
    return true;
  }
  return [
    '我执行',
    '我运行',
    '我检查',
    '检查过',
    '已经检查',
    '已执行',
    '已运行',
    '真正的系统状态',
    '真实的系统状态',
    '命令输出',
    '退出码',
    '运行时间',
    '系统负载',
    '磁盘',
    '内存',
    '负载',
    '结果是',
    '输出是',
  ].some((marker) => text.includes(marker));
}

function citedFacts(
  claims: readonly Omit<AiEvidenceClaim, 'status'>[],
  facts: readonly AiToolResultFact[],
): AiToolResultFact[] {
  return facts.filter((fact) => claims.some((claim) => claim.evidence.includes(fact.factId)));
}

function validateEvidenceClaim(
  claim: Omit<AiEvidenceClaim, 'status'>,
  facts: readonly AiToolResultFact[],
): void {
  if (!claim.text.trim()) {
    throw new Error('evidence claim text is empty');
  }
  if (claim.confidence.toLowerCase() !== 'verified') {
    throw new Error('first-pass evidence claims must be verified');
  }
  if (claim.evidence.length === 0) {
    throw new Error('verified evidence claim has no evidence');
  }
  const claimFacts = facts.filter((fact) => claim.evidence.includes(fact.factId));
  if (claimFacts.length !== claim.evidence.length) {
    throw new Error('evidence claim cites unknown fact ids');
  }
  if (!recentFactsSupportText(claim.text, claimFacts)) {
    throw new Error('claim text is not supported by cited evidence');
  }
}

function validateEvidenceClaims(
  visibleText: string,
  claims: readonly Omit<AiEvidenceClaim, 'status'>[],
  facts: readonly AiToolResultFact[],
): void {
  if (claims.length === 0) {
    throw new Error('evidence claims block has no claims');
  }
  claims.forEach((claim) => validateEvidenceClaim(claim, facts));
  const visibleFacts = citedFacts(claims, facts);
  if (!recentFactsSupportText(visibleText, visibleFacts)) {
    throw new Error('visible answer is not supported by cited evidence');
  }
}

function stripEvidenceBlockFromTextParts(parts: readonly AiTurnPart[]): AiTurnPart[] {
  return parts
    .map((part): AiTurnPart => {
      if (part.type !== 'text') {
        return part;
      }
      try {
        const extracted = extractEvidenceClaimsBlock(part.text);
        return extracted ? { ...part, text: extracted.visibleText } : part;
      } catch {
        return part;
      }
    })
    .filter((part) => part.type !== 'text' || part.text.length > 0);
}

function appendClaimParts(turn: AiAssistantTurn | undefined, claims: readonly Omit<AiEvidenceClaim, 'status'>[]): AiAssistantTurn | undefined {
  if (!turn) {
    return undefined;
  }
  const claimParts: AiTurnPart[] = claims.map((claim) => ({
    type: 'claim',
    text: claim.text,
    evidence: claim.evidence,
    confidence: claim.confidence,
    status: 'verified',
  }));
  return {
    ...turn,
    parts: [...stripEvidenceBlockFromTextParts(turn.parts), ...claimParts],
  };
}

function appendGuardrailPart(turn: AiAssistantTurn | undefined, rawText: string): AiAssistantTurn | undefined {
  if (!turn) {
    return {
      id: `result-binding-${Date.now().toString(36)}`,
      status: 'complete',
      parts: [{ type: 'guardrail', code: 'result-binding-required', message: GUARDRAIL_MESSAGE, rawText }],
      toolRounds: [],
      plainTextSummary: GUARDRAIL_MESSAGE,
    };
  }
  return {
    ...turn,
    parts: [{ type: 'guardrail', code: 'result-binding-required', message: GUARDRAIL_MESSAGE, rawText }],
    plainTextSummary: GUARDRAIL_MESSAGE,
  };
}

function guardrailForMessage(message: AiChatMessage): { message: AiChatMessage; guardrail: AiResultBindingGuardrail } {
  const rawText = message.content;
  return {
    message: {
      ...message,
      content: GUARDRAIL_MESSAGE,
      turn: appendGuardrailPart(message.turn, rawText),
    },
    guardrail: { message: GUARDRAIL_MESSAGE, rawText },
  };
}

export function applyAiResultBindingGuard(
  message: AiChatMessage,
  recentFacts: readonly AiToolResultFact[],
): { message: AiChatMessage; guardrail?: AiResultBindingGuardrail } {
  try {
    const parsed = parseEvidenceClaims(message.content);
    if (parsed) {
      validateEvidenceClaims(parsed.visibleText, parsed.claims, recentFacts);
      return {
        message: {
          ...message,
          content: parsed.visibleText,
          turn: appendClaimParts(message.turn, parsed.claims),
        },
      };
    }
  } catch {
    return guardrailForMessage(message);
  }

  if (!textClaimsToolBackedFact(message.content)) {
    return { message };
  }

  if (recentFactsSupportText(message.content, recentFacts)) {
    return { message };
  }

  // Some providers, especially local or smaller models, do not reliably emit
  // the structured <evidence_claims> block. If this exact assistant turn did
  // produce tool facts, keep the answer visible instead of replacing it with
  // the guardrail message; the tool-result parts remain attached to the turn.
  if (recentFacts.length > 0) {
    return { message };
  }

  return guardrailForMessage(message);
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
