// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { getAiRuntimeEpoch } from './runtimeEpoch';

export type CliAgentKind = 'codex' | 'claude' | 'gemini' | 'opencode';
export type CliAgentStatus = 'running' | 'waiting_for_input' | 'completed' | 'failed' | 'stopped';

export type CliAgentSession = {
  id: string;
  kind: CliAgentKind;
  label: string;
  status: CliAgentStatus;
  targetId?: string;
  sessionId?: string;
  nodeId?: string;
  command: string;
  startedAt: number;
  updatedAt: number;
  runtimeEpoch: string;
};

const AGENT_PATTERNS: Array<[CliAgentKind, RegExp]> = [
  ['codex', /^(?:env\s+\S+=\S+\s+)*(?:npx\s+)?(?:@openai\/)?codex(?:\s|$)/i],
  ['claude', /^(?:env\s+\S+=\S+\s+)*(?:npx\s+)?(?:@anthropic-ai\/)?claude(?:\s|$)/i],
  ['gemini', /^(?:env\s+\S+=\S+\s+)*(?:npx\s+)?(?:@google\/)?gemini(?:\s|$)/i],
  ['opencode', /^(?:env\s+\S+=\S+\s+)*(?:npx\s+)?opencode(?:\s|$)/i],
];

const sessions = new Map<string, CliAgentSession>();
const inputBuffers = new Map<string, string>();

function sessionKey(kind: CliAgentKind, targetKey: string): string {
  return `cli-agent:${kind}:${targetKey}`;
}

export function detectCliAgentKind(command: string): CliAgentKind | null {
  const trimmed = command.trim();
  for (const [kind, pattern] of AGENT_PATTERNS) {
    if (pattern.test(trimmed)) return kind;
  }
  return null;
}

export function recordCliAgentCommand(input: {
  command: string;
  targetId?: string;
  sessionId?: string;
  nodeId?: string;
  status?: CliAgentStatus;
}): CliAgentSession | null {
  const kind = detectCliAgentKind(input.command);
  if (!kind) return null;
  const targetKey = input.sessionId ?? input.nodeId ?? input.targetId ?? 'unknown';
  const id = sessionKey(kind, targetKey);
  const now = Date.now();
  const existing = sessions.get(id);
  const next: CliAgentSession = {
    id,
    kind,
    label: `${kind} agent`,
    status: input.status ?? existing?.status ?? 'running',
    targetId: input.targetId,
    sessionId: input.sessionId,
    nodeId: input.nodeId,
    command: input.command,
    startedAt: existing?.startedAt ?? now,
    updatedAt: now,
    runtimeEpoch: getAiRuntimeEpoch(),
  };
  sessions.set(id, next);
  return next;
}

export function observeCliAgentTerminalInput(input: {
  data: string;
  targetId?: string;
  sessionId?: string;
  nodeId?: string;
}): CliAgentSession | null {
  const key = input.sessionId ?? input.nodeId ?? input.targetId ?? 'unknown';
  let buffer = inputBuffers.get(key) ?? '';
  let detected: CliAgentSession | null = null;

  for (const char of input.data) {
    if (char === '\r' || char === '\n') {
      const command = buffer.trim();
      buffer = '';
      if (command) {
        detected = recordCliAgentCommand({
          command,
          targetId: input.targetId,
          sessionId: input.sessionId,
          nodeId: input.nodeId,
        }) ?? detected;
      }
      continue;
    }
    if (char === '\u007f' || char === '\b') {
      buffer = buffer.slice(0, -1);
      continue;
    }
    if (char >= ' ' && char !== '\x1b') {
      buffer += char;
    }
  }

  inputBuffers.set(key, buffer.slice(-512));
  return detected;
}

export function listCliAgentSessions(): CliAgentSession[] {
  return Array.from(sessions.values()).sort((a, b) => b.updatedAt - a.updatedAt);
}

export function clearCliAgentSessions(): void {
  sessions.clear();
  inputBuffers.clear();
}
