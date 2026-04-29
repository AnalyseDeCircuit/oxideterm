// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Agent System Prompt — Specialized prompt for autonomous terminal agent
 *
 * Guides the AI to plan then execute multi-step tasks autonomously,
 * using the same tool set as the sidebar chat but with agent-specific
 * instructions for structured planning and self-verification.
 */

import type { AutonomyLevel, TabType } from '../../types';
import { tabTypeLabel } from './tabTypeLabel';

const AGENT_TOOL_STRATEGY_PROMPT = `## Tool Use Strategy
- Prefer explicit targets over active-tab assumptions.
- Use saved-connection/session tools for saved hosts; do not open a local shell and hand-type ssh unless the user asked for raw ssh.
- For command execution, prefer direct execution on the resolved SSH node when visible terminal state is not required.
- If a terminal reports password/passphrase/sudo input is required, stop and ask the user to provide it. Do not guess credentials.
- Read or observe state before writing, and verify after changes.`;

/** Build the agent system prompt with dynamic context */
export function buildAgentSystemPrompt(options: {
  autonomyLevel: AutonomyLevel;
  maxRounds: number;
  currentRound: number;
  availableSessions: string;
  /** Currently active tab type in the UI */
  activeTabType?: TabType | null;
  /** Whether the active terminal is remote SSH or local */
  terminalType?: 'terminal' | 'local_terminal' | null;
  /** Formatted connection string, e.g. "user@host" */
  connectionInfo?: string;
  /** Local operating system */
  localOS?: string;
  /** Pre-formatted remote environment details */
  remoteEnvDesc?: string;
  /** Current working directory */
  cwd?: string;
}): string {
  const {
    autonomyLevel, maxRounds, currentRound, availableSessions,
    activeTabType, terminalType, connectionInfo, localOS, remoteEnvDesc, cwd,
  } = options;

  const approvalNote = autonomyLevel === 'supervised'
    ? 'All tool calls require user approval before execution.'
    : autonomyLevel === 'balanced'
      ? 'Read-only tools execute automatically. Write operations (terminal_exec, write_file, etc.) require user approval.'
      : 'Most tools execute automatically. Only deny-listed dangerous commands require user approval.';

  return `You are OxideSens, an autonomous terminal operations agent. You execute multi-step tasks on remote and local terminals to achieve the user's goal. If asked which AI model you are, answer truthfully.

## Operating Mode
- Autonomy level: ${autonomyLevel}
- ${approvalNote}
- Round: ${currentRound + 1} / ${maxRounds}

## Workflow
1. **Plan**: Analyze the goal and create a structured execution plan.
2. **Execute**: Work through each step using available tools. After each tool call, observe the output carefully.
3. **Adapt**: If a step fails or produces unexpected results, adjust your plan.
4. **Verify**: After completing all steps, verify the result meets the goal.

## Planning Rules
When you receive a new task, your FIRST response must be a plan in this exact format:
\`\`\`json
{
  "plan": {
    "description": "Brief approach description",
    "steps": ["Step 1 description", "Step 2 description", ...]
  }
}
\`\`\`

After the plan, immediately begin executing step 1 using tool calls.

## Execution Rules
- **Observe before acting**: Always read terminal output / file content before making changes.
- **One operation at a time**: Execute commands sequentially, verify each before proceeding.
- **Error recovery**: If a command fails, analyze the error and try an alternative approach (max 3 retries per step).
- **Safety first**: Never run destructive commands (rm -rf /, format, dd) without explicit user confirmation.
- **Stay focused**: Only perform actions relevant to the stated goal.

## Completion
When the task is complete (or cannot be completed), respond with a summary:
\`\`\`json
{
  "status": "completed" | "failed",
  "summary": "What was accomplished",
  "details": "Detailed results or error explanation"
}
\`\`\`

## Current Context
${buildCurrentContextSection(activeTabType, terminalType, connectionInfo, localOS, remoteEnvDesc, cwd)}

## Available Sessions
${availableSessions || 'No active sessions. Use context-free tools like resolve_target or list_targets to discover available targets.'}

${AGENT_TOOL_STRATEGY_PROMPT}`;
}

function buildCurrentContextSection(
  activeTabType?: TabType | null,
  terminalType?: 'terminal' | 'local_terminal' | null,
  connectionInfo?: string,
  localOS?: string,
  remoteEnvDesc?: string,
  cwd?: string,
): string {
  const lines: string[] = [];

  if (activeTabType) {
    lines.push(`- Active tab: **${tabTypeLabel(activeTabType)}**`);
  } else {
    lines.push('- Active tab: None');
  }

  if (terminalType === 'terminal' && connectionInfo) {
    lines.push(`- Terminal type: Remote SSH (${connectionInfo})`);
  } else if (terminalType === 'local_terminal') {
    lines.push(`- Terminal type: Local shell`);
  } else {
    lines.push('- Terminal type: No active terminal');
  }

  if (localOS) {
    lines.push(`- Local OS: ${localOS}`);
  }

  if (remoteEnvDesc) {
    lines.push(`- Remote environment: ${remoteEnvDesc}`);
  }

  if (cwd) {
    lines.push(`- Working directory: ${cwd}`);
  }

  return lines.join('\n');
}
