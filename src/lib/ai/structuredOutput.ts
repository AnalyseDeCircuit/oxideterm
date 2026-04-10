// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AgentPlanStep } from '../../types';

type ParsedReview = {
  assessment: 'on_track' | 'needs_correction' | 'critical_issue';
  findings: string;
  suggestions: string[];
  shouldContinue: boolean;
};

type ParsedCompletion = {
  status: 'completed' | 'failed';
  summary: string;
  details: string;
};

function stripTrailingCommas(input: string): string {
  let result = '';
  let inString = false;
  let stringQuote = '"';
  let escaped = false;

  for (let i = 0; i < input.length; i++) {
    const char = input[i];

    if (escaped) {
      result += char;
      escaped = false;
      continue;
    }

    if (char === '\\') {
      result += char;
      escaped = true;
      continue;
    }

    if (inString) {
      result += char;
      if (char === stringQuote) {
        inString = false;
      }
      continue;
    }

    if (char === '"' || char === "'") {
      inString = true;
      stringQuote = char;
      result += char;
      continue;
    }

    if (char === ',') {
      let nextIndex = i + 1;
      while (nextIndex < input.length && /\s/.test(input[nextIndex])) {
        nextIndex++;
      }
      if (nextIndex < input.length && (input[nextIndex] === '}' || input[nextIndex] === ']')) {
        continue;
      }
    }

    result += char;
  }

  return result;
}

function extractBalancedJson(text: string): string | null {
  const start = text.search(/[\[{]/);
  if (start === -1) return null;

  let depth = 0;
  let inString = false;
  let stringQuote = '"';
  let escaped = false;

  for (let i = start; i < text.length; i++) {
    const char = text[i];

    if (escaped) {
      escaped = false;
      continue;
    }

    if (char === '\\') {
      escaped = true;
      continue;
    }

    if (inString) {
      if (char === stringQuote) {
        inString = false;
      }
      continue;
    }

    if (char === '"' || char === "'") {
      inString = true;
      stringQuote = char;
      continue;
    }

    if (char === '{' || char === '[') {
      depth++;
    } else if (char === '}' || char === ']') {
      depth--;
      if (depth === 0) {
        return text.slice(start, i + 1);
      }
    }
  }

  return null;
}

function getJsonCandidates(text: string): string[] {
  const candidates: string[] = [];
  const trimmed = text.trim();
  const fenceRegex = /```(?:json|jsonc|javascript|js)?\s*([\s\S]*?)```/gi;

  for (const match of trimmed.matchAll(fenceRegex)) {
    const candidate = match[1]?.trim();
    if (candidate) candidates.push(candidate);
  }

  if (trimmed) candidates.push(trimmed);

  const balanced = extractBalancedJson(trimmed);
  if (balanced) candidates.push(balanced.trim());

  return [...new Set(candidates)];
}

function tryParseCandidate(candidate: string): unknown | null {
  const attempts = [candidate.trim(), stripTrailingCommas(candidate.trim())];
  for (const attempt of attempts) {
    try {
      return JSON.parse(attempt);
    } catch {
      // try next variant
    }
  }
  return null;
}

function parseStructuredPayload(text: string): unknown | null {
  for (const candidate of getJsonCandidates(text)) {
    const parsed = tryParseCandidate(candidate);
    if (parsed !== null) return parsed;
  }
  return null;
}

function coerceString(value: unknown): string {
  if (typeof value === 'string') return value.trim();
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return '';
}

function splitStepString(value: string): string[] {
  return value
    .split(/\r?\n/)
    .map((line) => line.replace(/^\s*(?:[-*]|\d+[.)])\s*/, '').trim())
    .filter(Boolean);
}

function normalizePlanStep(value: unknown): AgentPlanStep | null {
  if (typeof value === 'string') {
    const description = value.trim();
    return description ? { description, status: 'pending' } : null;
  }

  if (value && typeof value === 'object') {
    const record = value as Record<string, unknown>;
    const description = coerceString(
      record.description
      ?? record.step
      ?? record.title
      ?? record.task
      ?? record.content
      ?? record.action
      ?? record.name,
    );
    if (description) {
      return { description, status: 'pending' };
    }
  }

  const fallback = coerceString(value);
  return fallback ? { description: fallback, status: 'pending' } : null;
}

export function parsePlanResponse(text: string): { description: string; steps: AgentPlanStep[] } | null {
  const parsed = parseStructuredPayload(text);
  if (!parsed || typeof parsed !== 'object') return null;

  const root = parsed as Record<string, unknown>;
  const plan = ((root.plan ?? root.result ?? root.output ?? root) as Record<string, unknown> | undefined) ?? root;
  const nestedPlan = (plan.plan && typeof plan.plan === 'object') ? plan.plan as Record<string, unknown> : plan;

  let rawSteps = nestedPlan.steps;
  if (typeof rawSteps === 'string') {
    rawSteps = splitStepString(rawSteps);
  }
  if (!Array.isArray(rawSteps)) return null;

  const steps = rawSteps
    .map((step) => normalizePlanStep(step))
    .filter((step): step is AgentPlanStep => step !== null);

  if (steps.length === 0) return null;

  const description = coerceString(nestedPlan.description ?? root.description) || steps[0].description;
  return { description, steps };
}

function normalizeAssessment(value: unknown): ParsedReview['assessment'] {
  const normalized = coerceString(value).toLowerCase().replace(/[\s-]+/g, '_');
  if (normalized === 'critical' || normalized === 'critical_blocker' || normalized === 'blocker') {
    return 'critical_issue';
  }
  if (normalized === 'needs_revision' || normalized === 'needs_fix' || normalized === 'needs_changes') {
    return 'needs_correction';
  }
  if (normalized === 'critical_issue' || normalized === 'needs_correction' || normalized === 'on_track') {
    return normalized;
  }
  return 'on_track';
}

function normalizeSuggestions(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value.map((item) => coerceString(item)).filter(Boolean);
  }

  const single = coerceString(value);
  if (!single) return [];

  return single
    .split(/\r?\n|;\s*/)
    .map((item) => item.replace(/^\s*(?:[-*]|\d+[.)])\s*/, '').trim())
    .filter(Boolean);
}

function coerceBoolean(value: unknown): boolean | null {
  if (typeof value === 'boolean') return value;
  if (typeof value === 'string') {
    const normalized = value.trim().toLowerCase();
    if (normalized === 'true' || normalized === 'yes') return true;
    if (normalized === 'false' || normalized === 'no') return false;
  }
  return null;
}

export function parseReviewResponse(text: string): ParsedReview | null {
  const parsed = parseStructuredPayload(text);
  if (!parsed || typeof parsed !== 'object') return null;

  const root = parsed as Record<string, unknown>;
  const review = ((root.review ?? root.result ?? root.output ?? root) as Record<string, unknown> | undefined) ?? root;
  const nestedReview = (review.review && typeof review.review === 'object') ? review.review as Record<string, unknown> : review;

  const findings = coerceString(
    nestedReview.findings
    ?? nestedReview.summary
    ?? nestedReview.finding
    ?? nestedReview.reason,
  );
  const suggestions = normalizeSuggestions(
    nestedReview.suggestions
    ?? nestedReview.recommendations
    ?? nestedReview.recommendation
    ?? nestedReview.actions,
  );
  const assessment = normalizeAssessment(
    nestedReview.assessment
    ?? nestedReview.status
    ?? nestedReview.severity,
  );

  const explicitContinue = coerceBoolean(
    nestedReview.should_continue
    ?? nestedReview.shouldContinue
    ?? nestedReview.continue,
  );

  if (!findings && suggestions.length === 0 && !('assessment' in nestedReview) && explicitContinue === null) {
    return null;
  }

  return {
    assessment,
    findings,
    suggestions,
    shouldContinue: explicitContinue ?? (assessment !== 'critical_issue'),
  };
}

function normalizeCompletionStatus(value: unknown): ParsedCompletion['status'] | null {
  const normalized = coerceString(value).toLowerCase().replace(/[\s-]+/g, '_');
  if (!normalized) return null;
  if (['completed', 'complete', 'success', 'succeeded', 'done'].includes(normalized)) {
    return 'completed';
  }
  if (['failed', 'fail', 'error', 'errored', 'blocked', 'incomplete', 'cancelled', 'canceled'].includes(normalized)) {
    return 'failed';
  }
  return null;
}

function normalizeDetails(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object') return JSON.stringify(value, null, 2);
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  return '';
}

export function parseCompletionResponse(text: string): ParsedCompletion | null {
  const parsed = parseStructuredPayload(text);
  if (!parsed || typeof parsed !== 'object') return null;

  const root = parsed as Record<string, unknown>;
  const completion = ((root.result ?? root.output ?? root.completion ?? root) as Record<string, unknown> | undefined) ?? root;
  const nestedCompletion = (completion.completion && typeof completion.completion === 'object')
    ? completion.completion as Record<string, unknown>
    : completion;

  const status = normalizeCompletionStatus(
    nestedCompletion.status
    ?? nestedCompletion.result
    ?? nestedCompletion.outcome,
  );
  const summary = coerceString(
    nestedCompletion.summary
    ?? nestedCompletion.message
    ?? nestedCompletion.findings,
  );

  if (!status || !summary) return null;

  return {
    status,
    summary,
    details: normalizeDetails(nestedCompletion.details ?? nestedCompletion.detail ?? nestedCompletion.metadata),
  };
}