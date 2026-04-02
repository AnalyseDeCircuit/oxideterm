// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Agent Reviewer — Periodic self-review during agent execution.
 *
 * When enabled (settings.ai.agentRoles.reviewer), the reviewer is invoked
 * every N rounds to audit recent actions. It can use a different (potentially
 * stronger) model from the executor to catch errors, security issues, or
 * suggest course corrections.
 *
 * The review output is injected back into the executor's message history
 * so it can self-correct in subsequent rounds.
 */

import type { AgentStep } from '../../types';

// DEFAULT_REVIEW_INTERVAL is now in agentConfig.ts — kept as re-export for compatibility
export { DEFAULT_REVIEW_INTERVAL } from './agentConfig';

/** Build the reviewer system prompt */
export function buildReviewerSystemPrompt(): string {
  return `You are a quality assurance reviewer for an autonomous terminal operations agent. Your job is to audit the agent's recent actions and provide actionable feedback.

## Your Responsibilities
1. **Correctness**: Did the agent's actions achieve the intended step? Are there errors it missed?
2. **Security**: Any dangerous commands executed? Files modified that shouldn't be? Credentials exposed?
3. **Efficiency**: Is the agent making progress or going in circles? Could it take a more direct approach?
4. **Completeness**: Are any steps being skipped? Is the verification adequate?

## Output Format
Respond with a concise review in this JSON format:
\`\`\`json
{
  "review": {
    "assessment": "on_track" | "needs_correction" | "critical_issue",
    "findings": "Brief description of what you found",
    "suggestions": ["Suggestion 1", "Suggestion 2", ...],
    "should_continue": true | false
  }
}
\`\`\`

## Rules
- Be concise — the executor has limited context window
- Focus on actionable feedback, not praise
- Flag security concerns with assessment "critical_issue"
- Set should_continue=false only for critical blockers
- If everything looks good, a brief "on track" is sufficient`;
}

/** Build the review prompt with recent execution context */
export function buildReviewPrompt(
  goal: string,
  recentSteps: AgentStep[],
  currentRound: number,
  maxRounds: number,
): string {
  const stepsSummary = recentSteps.map((s) => {
    const prefix = `[R${s.roundIndex}/${s.type}]`;
    if (s.type === 'tool_call' && s.toolCall) {
      const result = s.toolCall.result;
      const status = result ? (result.success ? 'OK' : 'FAIL') : 'PENDING';
      return `${prefix} ${s.toolCall.name}(${s.toolCall.arguments.slice(0, 100)}) → ${status}`;
    }
    return `${prefix} ${s.content.slice(0, 150)}`;
  }).join('\n');

  return `## Task Goal
${goal}

## Progress
Round ${currentRound} / ${maxRounds}

## Recent Actions (last ${recentSteps.length} steps)
${stepsSummary}

Please review these actions and provide your assessment.`;
}

/** Parse the reviewer's response into a structured review */
export function parseReview(text: string): {
  assessment: 'on_track' | 'needs_correction' | 'critical_issue';
  findings: string;
  suggestions: string[];
  shouldContinue: boolean;
} | null {
  const jsonMatch = text.match(/```json\s*([\s\S]*?)```/);
  const toParse = jsonMatch ? jsonMatch[1] : text;
  try {
    const parsed = JSON.parse(toParse);
    if (parsed.review) {
      const validAssessments = new Set(['on_track', 'needs_correction', 'critical_issue'] as const);
      return {
        assessment: validAssessments.has(parsed.review.assessment) ? parsed.review.assessment : 'on_track',
        findings: typeof parsed.review.findings === 'string' ? parsed.review.findings : '',
        suggestions: Array.isArray(parsed.review.suggestions) ? parsed.review.suggestions : [],
        shouldContinue: parsed.review.should_continue !== false,
      };
    }
  } catch { /* not a valid review response */ }
  return null;
}
