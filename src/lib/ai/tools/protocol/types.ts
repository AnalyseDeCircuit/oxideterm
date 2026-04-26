// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export type ToolCapability =
  | 'command.run'
  | 'terminal.send'
  | 'terminal.observe'
  | 'terminal.wait'
  | 'filesystem.read'
  | 'filesystem.write'
  | 'filesystem.search'
  | 'navigation.open'
  | 'state.list'
  | 'network.forward'
  | 'settings.read'
  | 'settings.write'
  | 'plugin.invoke'
  | 'mcp.invoke';

export type ToolTargetKind =
  | 'local-shell'
  | 'ssh-node'
  | 'terminal-session'
  | 'sftp-session'
  | 'ide-workspace'
  | 'app-tab'
  | 'mcp-server'
  | 'rag-index';

export interface ToolTarget {
  id: string;
  kind: ToolTargetKind;
  label: string;
  active?: boolean;
  nodeId?: string;
  sessionId?: string;
  tabId?: string;
  capabilities: ToolCapability[];
  metadata?: Record<string, unknown>;
}

export type ToolRisk =
  | 'read'
  | 'write-file'
  | 'execute-command'
  | 'interactive-input'
  | 'destructive'
  | 'network-expose'
  | 'settings-change'
  | 'credential-sensitive';

export interface ToolResultError {
  code: string;
  message: string;
  recoverable: boolean;
}

export interface ToolResultMeta {
  toolName: string;
  capability?: ToolCapability;
  targetId?: string;
  durationMs: number;
  truncated?: boolean;
}

export type ToolNextAction = {
  tool: string;
  args?: Record<string, unknown>;
  reason: string;
  priority: 'recommended' | 'optional' | 'fallback';
};

export type ToolResultDisambiguation = {
  prompt: string;
  options: Array<{
    id: string;
    label: string;
    args?: Record<string, unknown>;
  }>;
};

export interface ToolResultEnvelope<TData = unknown> {
  ok: boolean;
  summary: string;
  data?: TData;
  output: string;
  warnings?: string[];
  error?: ToolResultError;
  observations?: string[];
  targets?: Array<{ id: string; kind: string; label: string; metadata?: Record<string, unknown> }>;
  nextActions?: ToolNextAction[];
  disambiguation?: ToolResultDisambiguation;
  recoverable?: boolean;
  waitingForInput?: boolean;
  meta: ToolResultMeta;
}
