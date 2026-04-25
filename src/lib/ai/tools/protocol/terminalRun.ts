// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { nodeIdeExecCommand } from '../../../api';

export interface TerminalRunRequest {
  nodeId: string;
  command: string;
  cwd?: string;
  timeoutSecs: number;
}

export interface TerminalRunData {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  timedOut: boolean;
}

export async function terminalRunRemote(request: TerminalRunRequest): Promise<TerminalRunData> {
  const result = await nodeIdeExecCommand(request.nodeId, request.command, request.cwd, request.timeoutSecs);
  return {
    stdout: result.stdout,
    stderr: result.stderr,
    exitCode: result.exitCode,
    timedOut: result.exitCode === null,
  };
}
