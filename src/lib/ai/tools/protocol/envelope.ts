// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiToolResult } from '../../../../types';
import type { ToolCapability, ToolResultEnvelope } from './types';

const DEFAULT_DURATION_MS = 0;

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
}): ToolResultEnvelope<TData> {
  return {
    ok: input.ok,
    summary: input.summary,
    ...(input.data !== undefined ? { data: input.data } : {}),
    output: input.output ?? input.summary,
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
