// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiToolResult } from '../../../types';
import type { ToolCapability, ToolResultEnvelope } from '../tools/protocol';
import { createToolResultEnvelope } from '../tools/protocol';
import type { AiActionResult, AiActionRisk, AiTarget } from './types';
import { getAiRuntimeEpoch } from './runtimeEpoch';

const FULL_OUTPUT_MAX_CHARS = 24 * 1024;
const RAW_OUTPUT_PERSIST_MAX_CHARS = 256 * 1024;
const MODEL_OUTPUT_PREVIEW_MAX_CHARS = 12000;

function riskToCapability(risk: AiActionRisk): ToolCapability | undefined {
  switch (risk) {
    case 'read':
      return 'state.list';
    case 'write':
      return 'filesystem.write';
    case 'execute':
      return 'command.run';
    case 'interactive':
      return 'terminal.send';
    default:
      return undefined;
  }
}

function lineCount(value: string): number {
  if (!value) return 0;
  return value.split('\n').length;
}

function buildHeadTailPreview(value: string, maxChars = MODEL_OUTPUT_PREVIEW_MAX_CHARS): string {
  if (value.length <= maxChars) return value;
  const marker = `\n\n[output truncated: ${value.length - maxChars} chars omitted; showing head and tail]\n\n`;
  const available = Math.max(0, maxChars - marker.length);
  const headChars = Math.ceil(available * 0.55);
  const tailChars = Math.max(0, available - headChars);
  return `${value.slice(0, headChars)}${marker}${value.slice(-tailChars)}`;
}

function prepareToolOutput(value: string) {
  const charCount = value.length;
  const lines = lineCount(value);
  if (charCount <= FULL_OUTPUT_MAX_CHARS) {
    return {
      output: value,
      truncated: false,
      rawOutput: undefined,
      outputPreview: {
        strategy: 'full' as const,
        charCount,
        lineCount: lines,
        rawOutputStored: false,
      },
    };
  }

  const output = buildHeadTailPreview(value);
  const rawOutputStored = charCount <= RAW_OUTPUT_PERSIST_MAX_CHARS;
  return {
    output,
    truncated: true,
    rawOutput: rawOutputStored ? value : undefined,
    outputPreview: {
      strategy: 'head_tail' as const,
      charCount,
      lineCount: lines,
      omittedChars: Math.max(0, charCount - output.length),
      rawOutputStored,
    },
  };
}

function mapTarget(target: AiTarget): NonNullable<ToolResultEnvelope['targets']>[number] {
  return {
    id: target.id,
    kind: target.kind,
    label: target.label,
    metadata: {
      state: target.state,
      capabilities: target.capabilities,
      refs: target.refs,
      ...(target.metadata ?? {}),
    },
  };
}

export function actionResultToToolResult(
  toolCallId: string,
  toolName: string,
  result: AiActionResult,
  durationMs: number,
  meta?: {
    commandRecordId?: string;
    policyDecision?: ToolResultEnvelope['meta']['policyDecision'];
    profileId?: string;
  },
): AiToolResult {
  const rawOutput = result.output ?? result.summary;
  const preparedOutput = prepareToolOutput(rawOutput);
  const runtimeEpoch = result.runtimeEpoch ?? getAiRuntimeEpoch();
  const verified = result.verified ?? (result.ok && !result.error);
  const targets = [
    ...(result.target ? [result.target] : []),
    ...(result.targets ?? []),
  ];
  const envelope = createToolResultEnvelope({
    ok: result.ok,
    toolName,
    summary: result.summary,
    output: preparedOutput.output,
    rawOutput: preparedOutput.rawOutput,
    outputPreview: preparedOutput.outputPreview,
    data: result.data,
    warnings: [
      ...(result.output && preparedOutput.truncated && !preparedOutput.rawOutput
        ? ['Full output exceeded the UI retention limit; showing a head/tail preview. Use a narrower command such as grep, tail -n, or find ... | head for exact data.']
        : []),
    ],
    error: result.error,
    observations: result.observations,
    recoverable: result.error?.recoverable,
    waitingForInput: result.waitingForInput,
    capability: riskToCapability(result.risk),
    targetId: result.target?.id,
    targets: targets.map(mapTarget),
    nextActions: result.nextActions?.map((next) => ({
      tool: next.action,
      args: next.args,
      reason: next.reason,
      priority: 'recommended' as const,
    })),
    durationMs,
    truncated: preparedOutput.truncated,
    verified,
    runtimeEpoch,
    stateVersion: result.stateVersion,
    commandRecordId: meta?.commandRecordId,
    policyDecision: meta?.policyDecision,
    profileId: meta?.profileId,
  });

  return {
    toolCallId,
    toolName,
    success: result.ok,
    output: preparedOutput.output,
    ...(result.error ? { error: result.error.message } : {}),
    durationMs,
    truncated: preparedOutput.truncated,
    envelope,
  };
}

export function failAction(
  summary: string,
  code: string,
  message: string,
  risk: AiActionRisk = 'read',
  options: Partial<AiActionResult> = {},
): AiActionResult {
  return {
    ok: false,
    summary,
    output: message,
    risk,
    ...options,
    error: {
      code,
      message,
      recoverable: true,
      ...(options.error ?? {}),
    },
  };
}
