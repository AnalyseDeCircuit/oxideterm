// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

export type AcpAgentAuthStatus = 'unknown' | 'not_required' | 'required' | 'authenticated' | 'expired';

export type AcpAgentAuthState = {
  status: AcpAgentAuthStatus;
  accountLabel?: string | null;
};

export type AcpAgentCapabilityPolicy = {
  fsReadTextFile: boolean;
  fsWriteTextFile: boolean;
  terminal: boolean;
};

export type AcpAgentRuntimeStatus = {
  state: 'unknown' | 'ready' | 'auth_required' | 'error';
  lastErrorKind?: string | null;
};

export type AcpAgentConfig = {
  id: string;
  displayName: string;
  command: string;
  args: string[];
  env: Record<string, string>;
  cwd: string | null;
  enabled: boolean;
  auth: AcpAgentAuthState;
  capabilityPolicy: AcpAgentCapabilityPolicy;
  status: AcpAgentRuntimeStatus;
};

export function defaultAcpAgentCapabilityPolicy(): AcpAgentCapabilityPolicy {
  return {
    // Host capabilities start closed until ACP request handlers enforce policy.
    fsReadTextFile: false,
    fsWriteTextFile: false,
    terminal: false,
  };
}

export function defaultAcpAgentAuthState(): AcpAgentAuthState {
  return { status: 'unknown', accountLabel: null };
}

export function defaultAcpAgentRuntimeStatus(): AcpAgentRuntimeStatus {
  return { state: 'unknown', lastErrorKind: null };
}
