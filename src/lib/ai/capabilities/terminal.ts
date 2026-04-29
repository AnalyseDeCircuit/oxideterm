// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { api, nodeIdeExecCommand } from '../../api';
import {
  beginTerminalCommandMark,
  findPaneBySessionId,
  getTerminalBuffer,
  readScreen,
  subscribeTerminalOutput,
  waitForTerminalReady,
  writeToTerminal,
} from '../../terminalRegistry';
import type { AiActionResult, AiTarget } from '../orchestrator/types';
import { failAction } from '../orchestrator/result';

const DEFAULT_WAIT_MS = 30_000;
const STABLE_MS = 400;

function trimTail(value: string, maxChars: number): string {
  if (value.length <= maxChars) return value;
  return `[trimmed ${value.length - maxChars} chars]\n${value.slice(-maxChars)}`;
}

function looksWaitingForInput(value: string): boolean {
  const tail = value.slice(-1000);
  return /(?:password|passphrase|sudo|验证码|口令|密码).*[:：]?\s*$/i.test(tail);
}

function commandOutput(stdout: string | null | undefined, stderr: string | null | undefined, exitCode: number | null | undefined): string {
  return [
    stdout ?? '',
    stderr ? `[stderr]\n${stderr}` : '',
    `[exit_code: ${exitCode ?? 'unknown'}]`,
  ].filter(Boolean).join('\n');
}

function hasCapturedCommandOutput(stdout: string | null | undefined, stderr: string | null | undefined): boolean {
  return Boolean(stdout?.trim() || stderr?.trim());
}

async function waitForTerminalDelta(
  sessionId: string,
  before: string,
  timeoutMs = DEFAULT_WAIT_MS,
  abortSignal?: AbortSignal,
): Promise<{ output: string; waitingForInput: boolean; timedOut: boolean }> {
  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return { output: '', waitingForInput: false, timedOut: true };
  }

  let last = getTerminalBuffer(paneId) ?? '';
  let changedAt = Date.now();
  let notified = false;
  const unsubscribe = subscribeTerminalOutput(sessionId, () => {
    notified = true;
  });

  try {
    const startedAt = Date.now();
    while (Date.now() - startedAt < timeoutMs) {
      if (abortSignal?.aborted) {
        return { output: '', waitingForInput: false, timedOut: true };
      }
      await new Promise((resolve) => setTimeout(resolve, notified ? 60 : 160));
      notified = false;
      const current = getTerminalBuffer(paneId) ?? '';
      if (current !== last) {
        last = current;
        changedAt = Date.now();
      }
      if (current !== before && Date.now() - changedAt >= STABLE_MS) {
        const delta = current.startsWith(before) ? current.slice(before.length) : current;
        return {
          output: delta.trim() || current.slice(-1000),
          waitingForInput: looksWaitingForInput(current),
          timedOut: false,
        };
      }
    }

    const current = getTerminalBuffer(paneId) ?? '';
    const delta = current.startsWith(before) ? current.slice(before.length) : current;
    return {
      output: delta.trim() || current.slice(-1000),
      waitingForInput: looksWaitingForInput(current),
      timedOut: true,
    };
  } finally {
    unsubscribe();
  }
}

export async function runCommandOnTarget(options: {
  target: AiTarget;
  command: string;
  cwd?: string;
  timeoutSecs?: number;
  awaitOutput?: boolean;
  dangerousCommandApproved?: boolean;
  abortSignal?: AbortSignal;
}): Promise<AiActionResult> {
  const { target, command } = options;
  if (!command.trim()) {
    return failAction('Command is required.', 'missing_command', 'run_command requires a command.', 'execute');
  }

  if (target.kind === 'saved-connection') {
    return failAction(
      'Connect the saved SSH target before running commands.',
      'saved_connection_not_connected',
      'Saved connection targets are not live shells. Call connect_target first, then run_command on the returned ssh-node or terminal-session target.',
      'execute',
      {
        target,
        nextActions: [{ action: 'connect_target', args: { target_id: target.id }, reason: 'Open the saved SSH connection first.' }],
      },
    );
  }

  if (target.kind === 'ssh-node') {
    const nodeId = target.refs.nodeId;
    if (!nodeId) {
      return failAction('SSH node target is missing nodeId.', 'missing_node_id', 'Target cannot run remote commands without nodeId.', 'execute', { target });
    }
    try {
      const result = await nodeIdeExecCommand(nodeId, command, options.cwd, options.timeoutSecs ?? 30);
      const output = commandOutput(result.stdout, result.stderr, result.exitCode);
      const hasOutput = hasCapturedCommandOutput(result.stdout, result.stderr);
      const ok = result.exitCode === 0 || (result.exitCode == null && hasOutput);
      return {
        ok,
        summary: result.exitCode === 0
          ? 'Remote command completed.'
          : result.exitCode == null && hasOutput
            ? 'Remote command output captured; exit code was not reported.'
            : `Remote command exited with ${result.exitCode ?? 'unknown'}.`,
        output,
        data: { exitCode: result.exitCode ?? null },
        ...(result.exitCode == null && hasOutput ? { observations: ['The remote command produced output, but the backend did not report an exit code.'] } : {}),
        target,
        risk: 'execute',
        ...(ok ? {} : {
          error: { code: 'remote_command_failed', message: `Exit code: ${result.exitCode ?? 'unknown'}`, recoverable: true },
        }),
      };
    } catch (error) {
      return failAction('Remote command failed.', 'remote_command_error', error instanceof Error ? error.message : String(error), 'execute', { target });
    }
  }

  if (target.kind === 'local-shell') {
    try {
      const result = await api.localExecCommand(command, options.cwd, options.timeoutSecs ?? 30, options.dangerousCommandApproved);
      const output = commandOutput(result.stdout, result.stderr, result.exitCode);
      const hasOutput = hasCapturedCommandOutput(result.stdout, result.stderr);
      const ok = !result.timedOut && (result.exitCode === 0 || (result.exitCode == null && hasOutput));
      return {
        ok,
        summary: result.timedOut
          ? 'Local command timed out.'
          : result.exitCode === 0
            ? 'Local command completed.'
            : result.exitCode == null && hasOutput
              ? 'Local command output captured; exit code was not reported.'
              : `Local command exited with ${result.exitCode ?? 'unknown'}.`,
        output,
        data: { exitCode: result.exitCode ?? null, timedOut: result.timedOut },
        ...(!result.timedOut && result.exitCode == null && hasOutput ? { observations: ['The local command produced output, but the backend did not report an exit code.'] } : {}),
        target,
        risk: 'execute',
        ...(ok ? {} : {
          error: { code: result.timedOut ? 'local_command_timeout' : 'local_command_failed', message: result.timedOut ? 'Command timed out.' : `Exit code: ${result.exitCode ?? 'unknown'}`, recoverable: true },
        }),
      };
    } catch (error) {
      return failAction('Local command failed.', 'local_command_error', error instanceof Error ? error.message : String(error), 'execute', { target });
    }
  }

  if (target.kind === 'terminal-session') {
    const sessionId = target.refs.sessionId;
    if (!sessionId) {
      return failAction('Terminal target is missing sessionId.', 'missing_session_id', 'Target cannot receive terminal input without sessionId.', 'interactive', { target });
    }
    const paneId = findPaneBySessionId(sessionId);
    if (!paneId) {
      return failAction('Terminal pane is not ready.', 'terminal_pane_missing', 'The visible terminal pane is not registered yet.', 'interactive', { target });
    }
    const ready = await waitForTerminalReady(sessionId, { timeoutMs: 3000, abortSignal: options.abortSignal });
    if (!ready.ready) {
      return failAction('Terminal is not ready.', 'terminal_not_ready', ready.reason ?? 'Terminal writer/listener is not ready.', 'interactive', { target });
    }
    const before = getTerminalBuffer(paneId) ?? '';
    const sent = writeToTerminal(paneId, `${command}\r`);
    if (!sent) {
      return failAction('Failed to send command to terminal.', 'terminal_send_failed', 'No terminal writer is registered for this session.', 'interactive', { target });
    }
    beginTerminalCommandMark(paneId, {
      command,
      source: 'ai',
      sessionId,
      cwd: typeof options.target.metadata?.cwd === 'string' ? options.target.metadata.cwd : undefined,
    });
    if (options.awaitOutput === false) {
      return {
        ok: true,
        summary: 'Command sent to terminal.',
        output: `Command sent: ${command}`,
        target,
        risk: 'interactive',
      };
    }

    const wait = await waitForTerminalDelta(sessionId, before, DEFAULT_WAIT_MS, options.abortSignal);
    return {
      ok: !wait.timedOut || Boolean(wait.output),
      summary: wait.timedOut ? 'Terminal command did not produce completed output.' : 'Terminal command output captured.',
      output: wait.output || 'No new output captured.',
      target,
      waitingForInput: wait.waitingForInput,
      risk: 'interactive',
      ...(wait.timedOut && !wait.output ? {
        error: { code: 'terminal_command_wait_timeout', message: 'No new output after 30s. The command may be waiting for input or still running.', recoverable: true },
      } : {}),
    };
  }

  return failAction('Target cannot run commands.', 'unsupported_command_target', `${target.kind} does not support command execution.`, 'execute', { target });
}

export async function observeTerminalTarget(target: AiTarget, maxChars = 4000): Promise<AiActionResult> {
  const sessionId = target.refs.sessionId;
  if (!sessionId) {
    return failAction('Terminal target is missing sessionId.', 'missing_session_id', 'observe_terminal requires a terminal-session target.', 'read', { target });
  }
  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return failAction('Terminal pane is not registered.', 'terminal_pane_missing', 'No visible pane is registered for this terminal session.', 'read', { target });
  }
  const buffer = getTerminalBuffer(paneId) ?? '';
  const screen = readScreen(paneId);
  const ready = await waitForTerminalReady(sessionId, { timeoutMs: 1 });
  const output = trimTail(buffer, maxChars);
  return {
    ok: true,
    summary: 'Terminal observed.',
    output,
    data: {
      buffer: output,
      screen,
      readiness: ready.state,
      waitingForInput: looksWaitingForInput(buffer),
    },
    waitingForInput: looksWaitingForInput(buffer),
    target,
    risk: 'read',
  };
}

export async function sendTerminalInput(options: {
  target: AiTarget;
  text?: string;
  appendEnter?: boolean;
  control?: string;
}): Promise<AiActionResult> {
  const sessionId = options.target.refs.sessionId;
  if (!sessionId) {
    return failAction('Terminal target is missing sessionId.', 'missing_session_id', 'send_terminal_input requires a terminal-session target.', 'interactive', { target: options.target });
  }
  const paneId = findPaneBySessionId(sessionId);
  if (!paneId) {
    return failAction('Terminal pane is not registered.', 'terminal_pane_missing', 'No visible pane is registered for this terminal session.', 'interactive', { target: options.target });
  }
  const controlMap: Record<string, string> = {
    'ctrl-c': '\x03',
    'ctrl-d': '\x04',
    'ctrl-z': '\x1a',
  };
  const payload = options.control
    ? controlMap[options.control] ?? ''
    : `${options.text ?? ''}${options.appendEnter ? '\r' : ''}`;
  if (!payload) {
    return failAction('No terminal input specified.', 'missing_terminal_input', 'Provide text or a supported control sequence.', 'interactive', { target: options.target });
  }
  const sent = writeToTerminal(paneId, payload);
  return {
    ok: sent,
    summary: sent ? 'Terminal input sent.' : 'Failed to send terminal input.',
    output: sent ? 'Input sent.' : 'No terminal writer is registered.',
    target: options.target,
    risk: 'interactive',
    ...(sent ? {} : {
      error: { code: 'terminal_send_failed', message: 'No terminal writer is registered.', recoverable: true },
    }),
  };
}
