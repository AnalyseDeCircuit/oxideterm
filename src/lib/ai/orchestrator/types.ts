// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export type AiTargetKind =
  | 'saved-connection'
  | 'ssh-node'
  | 'terminal-session'
  | 'local-shell'
  | 'sftp-session'
  | 'ide-workspace'
  | 'settings'
  | 'app-surface'
  | 'rag-index';

export type AiTargetView = 'connections' | 'live_sessions' | 'app_surfaces' | 'files' | 'all';

export type AiTargetIntent =
  | 'connection'
  | 'command'
  | 'terminal'
  | 'settings'
  | 'file'
  | 'sftp'
  | 'app_surface'
  | 'knowledge'
  | 'status'
  | 'local'
  | 'unknown';

export type AiResourceKind = 'settings' | 'file' | 'directory' | 'sftp' | 'ide' | 'rag';

export type AiTargetState = 'available' | 'connected' | 'opening' | 'stale' | 'unavailable';

export type AiActionRisk =
  | 'read'
  | 'write'
  | 'execute'
  | 'interactive'
  | 'destructive'
  | 'credential';

export type AiTarget = {
  id: string;
  kind: AiTargetKind;
  label: string;
  state: AiTargetState;
  capabilities: string[];
  refs: {
    connectionId?: string;
    nodeId?: string;
    sessionId?: string;
    tabId?: string;
  };
  metadata?: Record<string, unknown>;
};

export type AiActionNextAction = {
  action: string;
  args?: Record<string, unknown>;
  reason: string;
};

export type AiActionResult<T = unknown> = {
  ok: boolean;
  summary: string;
  /**
   * True when the result came from current runtime state or a completed action.
   * False for dry-runs, recoverable errors, unsupported branches, or fallback-only observations.
   */
  verified?: boolean;
  /** Frontend runtime epoch. Changes after WebView reload, making stale transcript facts visible to the model. */
  runtimeEpoch?: string;
  /** Optional lightweight state version for store snapshots. */
  stateVersion?: string;
  data?: T;
  target?: AiTarget;
  targets?: AiTarget[];
  output?: string;
  observations?: string[];
  error?: {
    code: string;
    message: string;
    recoverable: boolean;
  };
  nextActions?: AiActionNextAction[];
  waitingForInput?: boolean;
  risk: AiActionRisk;
};

export type OrchestratorToolContext = {
  activeSessionId?: string | null;
  activeTerminalType?: 'terminal' | 'local_terminal' | null;
  dangerousCommandApproved?: boolean;
  approvalMode?: 'default' | 'bypass';
  policyDecision?: import('./policy').AiPolicyDecision;
  profileId?: string;
  abortSignal?: AbortSignal;
  skipFocus?: boolean;
};

export const ORCHESTRATOR_TOOL_NAMES = [
  'list_targets',
  'select_target',
  'connect_target',
  'run_command',
  'observe_terminal',
  'send_terminal_input',
  'read_resource',
  'write_resource',
  'transfer_resource',
  'open_app_surface',
  'get_state',
  'remember_preference',
  'recall_preferences',
] as const;

export type OrchestratorToolName = typeof ORCHESTRATOR_TOOL_NAMES[number];
