// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { connectToSaved } from '../../connectToSaved';
import { waitForTerminalReady } from '../../terminalRegistry';
import { useAppStore } from '../../../store/appStore';
import { useSessionTreeStore } from '../../../store/sessionTreeStore';
import i18n from '../../../i18n';
import type { AiActionResult, AiTarget } from '../orchestrator/types';
import { failAction } from '../orchestrator/result';
import { getAiTarget } from './targets';

function noopToast(): void {
  // The orchestrator reports failures in the chat transcript; avoid duplicate toasts.
}

export async function connectAiTarget(targetId: string): Promise<AiActionResult> {
  const target = await getAiTarget(targetId);
  if (!target) {
    return failAction('Target not found.', 'target_not_found', `Target not found: ${targetId}`, 'write', {
      nextActions: [{ action: 'list_targets', reason: 'Refresh available targets before connecting.' }],
    });
  }

  if (target.kind === 'ssh-node' || target.kind === 'terminal-session') {
    if (target.kind === 'ssh-node' && target.state !== 'connected') {
      const nodeId = target.refs.nodeId;
      if (!nodeId) {
        return failAction('SSH target is missing nodeId.', 'missing_node_id', 'The selected SSH target cannot be reconnected without a node id.', 'write', { target });
      }
      try {
        await useSessionTreeStore.getState().connectNodeWithAncestors(nodeId);
        const sessionId = await useSessionTreeStore.getState().createTerminalForNode(nodeId);
        const refreshedNode = useSessionTreeStore.getState().getNode(nodeId);
        const connectionId = refreshedNode?.runtime.connectionId ?? target.refs.connectionId;
        useAppStore.getState().createTab('terminal', sessionId);
        await waitForTerminalReady(sessionId, { timeoutMs: 5000 }).catch(() => null);
        const liveTarget: AiTarget = {
          ...target,
          state: 'connected',
          refs: { ...target.refs, sessionId, connectionId },
          capabilities: ['command.run', 'filesystem.read', 'filesystem.write', 'state.list', 'navigation.open'],
        };
        const terminalTarget: AiTarget = {
          id: `terminal-session:${sessionId}`,
          kind: 'terminal-session',
          label: `${target.label} terminal`,
          state: 'connected',
          capabilities: ['terminal.observe', 'terminal.send', 'terminal.wait', 'state.list'],
          refs: { nodeId, sessionId, connectionId },
          metadata: { terminalType: 'terminal' },
        };
        return {
          ok: true,
          summary: `Reconnected ${target.label}.`,
          target: liveTarget,
          targets: [liveTarget, terminalTarget],
          data: { nodeId, sessionId },
          output: `Connected target ${liveTarget.id}; visible terminal ${terminalTarget.id}.`,
          risk: 'write',
        };
      } catch (error) {
        return failAction('SSH target reconnect failed.', 'ssh_reconnect_failed', error instanceof Error ? error.message : String(error), 'write', {
          target,
          nextActions: [{ action: 'list_targets', reason: 'Refresh target state before retrying.' }],
        });
      }
    }

    if (target.state !== 'connected') {
      return failAction('Target is not ready.', 'target_not_ready', `${target.id} is ${target.state}; wait for it to become connected before continuing.`, 'write', {
        target,
        nextActions: [{ action: 'list_targets', reason: 'Refresh available targets before retrying.' }],
      });
    }

    return {
      ok: true,
      summary: 'Target is already live.',
      target,
      data: { nodeId: target.refs.nodeId ?? '', sessionId: target.refs.sessionId ?? '' },
      risk: 'write',
    };
  }

  if (target.kind !== 'saved-connection') {
    return failAction('Target cannot be connected as SSH.', 'unsupported_connect_target', `${target.kind} is not a saved SSH connection.`, 'write', { target });
  }

  const connectionId = target.refs.connectionId;
  if (!connectionId) {
    return failAction('Saved connection is missing connectionId.', 'missing_connection_id', 'The selected saved connection has no connection id.', 'write', { target });
  }

  try {
    const result = await connectToSaved(connectionId, {
      createTab: (type, sessionId) => useAppStore.getState().createTab(type, sessionId),
      toast: noopToast,
      t: (key, options) => i18n.t(key, options as Record<string, unknown>),
    });

    if (!result) {
      return failAction('Connection did not complete.', 'connect_failed', 'The saved connection flow did not return a live terminal.', 'write', {
        target,
        nextActions: [{ action: 'select_target', args: { query: target.label }, reason: 'Re-select the target and retry if credentials were updated.' }],
      });
    }

    await waitForTerminalReady(result.sessionId, { timeoutMs: 5000 }).catch(() => null);

    const sshTarget: AiTarget = {
      id: `ssh-node:${result.nodeId}`,
      kind: 'ssh-node',
      label: target.label,
      state: 'connected',
      capabilities: ['command.run', 'filesystem.read', 'filesystem.write', 'state.list', 'navigation.open'],
      refs: { nodeId: result.nodeId, sessionId: result.sessionId, connectionId },
      metadata: target.metadata,
    };
    const terminalTarget: AiTarget = {
      id: `terminal-session:${result.sessionId}`,
      kind: 'terminal-session',
      label: `${target.label} terminal`,
      state: 'connected',
      capabilities: ['terminal.observe', 'terminal.send', 'terminal.wait', 'state.list'],
      refs: { nodeId: result.nodeId, sessionId: result.sessionId, connectionId },
      metadata: { terminalType: 'terminal' },
    };

    return {
      ok: true,
      summary: `Connected ${target.label}.`,
      data: result,
      target: sshTarget,
      targets: [sshTarget, terminalTarget],
      output: `Connected target ${sshTarget.id}; visible terminal ${terminalTarget.id}.`,
      risk: 'write',
    };
  } catch (error) {
    return failAction('Connection failed.', 'connect_error', error instanceof Error ? error.message : String(error), 'write', { target });
  }
}
