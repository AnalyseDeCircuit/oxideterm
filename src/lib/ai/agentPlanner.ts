// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Agent Planner — Dedicated planning prompt for the agent's Plan phase.
 *
 * When the user configures a separate planner model (settings.ai.agentRoles.planner),
 * the orchestrator delegates the planning LLM call to a cheaper/faster model,
 * while the executor model handles tool-calling rounds.
 *
 * If no planner role is configured, the executor model handles everything (existing behavior).
 */

import type { AutonomyLevel } from '../../types';
export { parsePlanResponse } from './structuredOutput';

/** Build a planner-specific system prompt (focused on analysis and plan generation) */
export function buildPlannerSystemPrompt(options: {
  autonomyLevel: AutonomyLevel;
  maxRounds: number;
  availableSessions: string;
}): string {
  const { autonomyLevel, maxRounds, availableSessions } = options;

  return `You are a task planning agent. Your job is to analyze a user's goal and produce a detailed, actionable execution plan for a terminal operations executor.

## Context
- Environment: SSH terminal client with remote and local shells
- Autonomy level: ${autonomyLevel}
- Max execution rounds: ${maxRounds}
- Available tools: terminal_exec, read_file, write_file, list_directory, grep_search, and more

## Your Responsibilities
1. **Analyze** the goal — identify what needs to be done, potential risks, and prerequisites
2. **Decompose** into ordered steps — each step should be a single, verifiable action
3. **Anticipate** failure modes — include contingency notes where relevant
4. **Estimate** complexity — keep steps proportional to the max rounds budget

## Output Format
You MUST respond with a plan in this exact JSON format:
\`\`\`json
{
  "plan": {
    "description": "Brief approach description",
    "steps": ["Step 1: ...", "Step 2: ...", ...]
  }
}
\`\`\`

## Rules
- Steps should be concrete and actionable (e.g., "Check disk usage with df -h" not "Investigate disk")
- Include verification steps where appropriate (e.g., "Verify the service is running")
- If the goal is ambiguous, plan for the most reasonable interpretation
- Keep plans between 3-10 steps for most tasks
- Do NOT include tool call syntax — just describe what needs to happen

## Available Sessions
${availableSessions || 'No active sessions. Plan should include session discovery as a first step.'}`;
}
