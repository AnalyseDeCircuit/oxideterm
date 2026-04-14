// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiGuardrailCode } from './types';

export interface GuardrailDetectionInput {
  toolUseEnabled: boolean;
  sawStructuredToolCall: boolean;
  assistantText: string;
  userExplicitlyRequestedJson?: boolean;
}

export interface GuardrailDetectionResult {
  matched: boolean;
  reason?: Extract<AiGuardrailCode, 'pseudo-tool-transcript'>;
  rawText?: string;
}

const TOOL_REQUEST_PAIR = /"name"\s*:\s*"[^"]+"[\s\S]*?"arguments"\s*:/i;
const TOOL_RESULT_PAIR = /("stdout"\s*:|"stderr"\s*:)[\s\S]*?("exit[_-]?code"\s*:|"status"\s*:)/i;
const TOOLISH_FIELD_COUNT = /"(name|arguments|stdout|stderr|exit[_-]?code|status|tool_call_id|toolName|toolCallId)"\s*:/gi;

function stripCodeFence(text: string): string {
  const trimmed = text.trim();
  const match = trimmed.match(/^```(?:json|javascript|js|text)?\s*([\s\S]*?)\s*```$/i);
  return match ? match[1].trim() : trimmed;
}

function looksJsonLike(text: string): boolean {
  return (text.startsWith('{') && text.endsWith('}')) || (text.startsWith('[') && text.endsWith(']'));
}

function countToolishFields(text: string): number {
  return text.match(TOOLISH_FIELD_COUNT)?.length ?? 0;
}

export function detectPseudoToolTranscript(input: GuardrailDetectionInput): GuardrailDetectionResult {
  if (input.sawStructuredToolCall || input.userExplicitlyRequestedJson) {
    return { matched: false };
  }

  const candidate = stripCodeFence(input.assistantText);
  if (!candidate || !looksJsonLike(candidate)) {
    return { matched: false };
  }

  const toolishFieldCount = countToolishFields(candidate);
  const looksLikeToolRequest = TOOL_REQUEST_PAIR.test(candidate);
  const looksLikeToolResult = TOOL_RESULT_PAIR.test(candidate);

  if (!looksLikeToolRequest && !looksLikeToolResult && toolishFieldCount < 3) {
    return { matched: false };
  }

  return {
    matched: true,
    reason: 'pseudo-tool-transcript',
    rawText: input.assistantText,
  };
}

export function shouldTriggerHardDeny(input: GuardrailDetectionInput, detection = detectPseudoToolTranscript(input)): boolean {
  return detection.matched && !input.toolUseEnabled;
}