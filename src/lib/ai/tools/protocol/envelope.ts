// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiToolResult } from '../../../../types';
import type { ToolCapability, ToolResultEnvelope } from './types';

const DEFAULT_DURATION_MS = 0;
const MODEL_OUTPUT_MAX_CHARS = 12000;
const MODEL_ERROR_OUTPUT_MAX_CHARS = 2000;
const MODEL_SUMMARY_MAX_CHARS = 1000;
const MODEL_ERROR_MESSAGE_MAX_CHARS = 1000;

function firstNonEmptyLine(value: string): string | undefined {
  return value
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line.length > 0);
}

function fallbackSummary(result: Pick<AiToolResult, 'toolName' | 'success' | 'output' | 'error'>): string {
  if (!result.success) {
    return result.error || firstNonEmptyLine(result.output) || `${result.toolName} failed`;
  }

  return firstNonEmptyLine(result.output) || `${result.toolName} completed`;
}

export function createToolResultEnvelope<TData = unknown>(input: {
  ok: boolean;
  toolName: string;
  summary: string;
  output?: string;
  rawOutput?: string;
  outputPreview?: ToolResultEnvelope['outputPreview'];
  data?: TData;
  warnings?: string[];
  error?: ToolResultEnvelope['error'];
  observations?: ToolResultEnvelope['observations'];
  targets?: ToolResultEnvelope['targets'];
  nextActions?: ToolResultEnvelope['nextActions'];
  disambiguation?: ToolResultEnvelope['disambiguation'];
  recoverable?: boolean;
  waitingForInput?: boolean;
  capability?: ToolCapability;
  targetId?: string;
  durationMs?: number;
  truncated?: boolean;
  verified?: boolean;
  runtimeEpoch?: string;
  stateVersion?: string;
  commandRecordId?: string;
  policyDecision?: ToolResultEnvelope['meta']['policyDecision'];
  profileId?: string;
}): ToolResultEnvelope<TData> {
  return {
    ok: input.ok,
    summary: input.summary,
    ...(input.data !== undefined ? { data: input.data } : {}),
    output: input.output ?? input.summary,
    ...(input.rawOutput !== undefined ? { rawOutput: input.rawOutput } : {}),
    ...(input.outputPreview ? { outputPreview: input.outputPreview } : {}),
    ...(input.warnings && input.warnings.length > 0 ? { warnings: input.warnings } : {}),
    ...(input.error ? { error: input.error } : {}),
    ...(input.observations && input.observations.length > 0 ? { observations: input.observations } : {}),
    ...(input.targets && input.targets.length > 0 ? { targets: input.targets } : {}),
    ...(input.nextActions && input.nextActions.length > 0 ? { nextActions: input.nextActions } : {}),
    ...(input.disambiguation ? { disambiguation: input.disambiguation } : {}),
    ...(input.recoverable !== undefined ? { recoverable: input.recoverable } : {}),
    ...(input.waitingForInput !== undefined ? { waitingForInput: input.waitingForInput } : {}),
    meta: {
      toolName: input.toolName,
      ...(input.capability ? { capability: input.capability } : {}),
      ...(input.targetId ? { targetId: input.targetId } : {}),
      durationMs: input.durationMs ?? DEFAULT_DURATION_MS,
      ...(input.truncated !== undefined ? { truncated: input.truncated } : {}),
      ...(input.verified !== undefined ? { verified: input.verified } : {}),
      ...(input.runtimeEpoch ? { runtimeEpoch: input.runtimeEpoch } : {}),
      ...(input.stateVersion ? { stateVersion: input.stateVersion } : {}),
      ...(input.commandRecordId ? { commandRecordId: input.commandRecordId } : {}),
      ...(input.policyDecision ? { policyDecision: input.policyDecision } : {}),
      ...(input.profileId ? { profileId: input.profileId } : {}),
    },
  };
}

export function toLegacyToolResult(
  envelope: ToolResultEnvelope,
  toolCallId: string,
): AiToolResult {
  return {
    toolCallId,
    toolName: envelope.meta.toolName,
    success: envelope.ok,
    output: envelope.output || envelope.summary,
    ...(envelope.error ? { error: envelope.error.message } : {}),
    ...(envelope.meta.truncated !== undefined ? { truncated: envelope.meta.truncated } : {}),
    durationMs: envelope.meta.durationMs,
    envelope,
  };
}

export function fromLegacyToolResult(result: AiToolResult): ToolResultEnvelope {
  if (result.envelope) {
    return result.envelope;
  }

  return createToolResultEnvelope({
    ok: result.success,
    toolName: result.toolName,
    summary: fallbackSummary(result),
    output: result.output,
    ...(result.truncated ? { warnings: ['Output was truncated.'] } : {}),
    ...(result.success
      ? {}
      : {
          error: {
            code: 'legacy_tool_error',
            message: result.error || fallbackSummary(result),
            recoverable: true,
          },
        }),
    durationMs: result.durationMs,
    truncated: result.truncated,
  });
}

function truncateForModel(value: string, maxChars = MODEL_OUTPUT_MAX_CHARS): { value: string; truncated: boolean } {
  if (value.length <= maxChars) {
    return { value, truncated: false };
  }

  return {
    value: `${value.slice(0, maxChars)}\n\n[truncated: ${value.length - maxChars} chars omitted]`,
    truncated: true,
  };
}

export function formatToolResultForModel(result: AiToolResult): string {
  const envelope = fromLegacyToolResult(result);
  const summary = truncateForModel(envelope.summary, MODEL_SUMMARY_MAX_CHARS);
  const output = truncateForModel(
    envelope.output || result.output || envelope.summary,
    envelope.ok ? MODEL_OUTPUT_MAX_CHARS : MODEL_ERROR_OUTPUT_MAX_CHARS,
  );
  const errorMessage = envelope.error
    ? truncateForModel(envelope.error.message, MODEL_ERROR_MESSAGE_MAX_CHARS)
    : null;
  const payload = {
    ok: envelope.ok,
    summary: summary.value,
    output: output.value,
    ...(envelope.error ? { error: { ...envelope.error, message: errorMessage?.value ?? envelope.error.message } } : {}),
    ...(envelope.recoverable !== undefined ? { recoverable: envelope.recoverable } : {}),
    ...(envelope.waitingForInput !== undefined ? { waitingForInput: envelope.waitingForInput } : {}),
    ...(envelope.warnings && envelope.warnings.length > 0 ? { warnings: envelope.warnings } : {}),
    ...(envelope.observations && envelope.observations.length > 0 ? { observations: envelope.observations } : {}),
    ...(envelope.targets && envelope.targets.length > 0 ? { targets: envelope.targets } : {}),
    ...(envelope.nextActions && envelope.nextActions.length > 0 ? { nextActions: envelope.nextActions } : {}),
    ...(envelope.disambiguation ? { disambiguation: envelope.disambiguation } : {}),
    ...(envelope.outputPreview ? { outputPreview: envelope.outputPreview } : {}),
    meta: {
      ...envelope.meta,
      truncated: envelope.meta.truncated === true
        || summary.truncated
        || output.truncated
        || errorMessage?.truncated === true,
    },
  };

  return JSON.stringify(payload);
}
