// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import { api } from '../../api';
import { useAppStore } from '../../../store/appStore';
import { useIdeStore } from '../../../store/ideStore';
import { useLocalTerminalStore } from '../../../store/localTerminalStore';
import { useSessionTreeStore } from '../../../store/sessionTreeStore';
import { getAllEntries, getTerminalReadiness } from '../../terminalRegistry';
import type { AiTarget, AiTargetKind, AiTargetView } from '../orchestrator/types';

function includesQuery(target: AiTarget, query: string): boolean {
  if (!query) return true;
  const haystack = [
    target.id,
    target.kind,
    target.label,
    ...Object.values(target.refs),
    ...Object.values(target.metadata ?? {}),
  ].join(' ').toLowerCase();
  return haystack.includes(query.toLowerCase());
}

function targetInView(target: AiTarget, view: AiTargetView): boolean {
  switch (view) {
    case 'connections':
      return target.kind === 'saved-connection' || target.kind === 'ssh-node';
    case 'live_sessions':
      return target.kind === 'terminal-session'
        || target.kind === 'sftp-session'
        || (target.kind === 'ssh-node' && target.state === 'connected');
    case 'app_surfaces':
      return target.kind === 'settings'
        || target.kind === 'app-surface'
        || target.kind === 'local-shell'
        || target.kind === 'rag-index';
    case 'files':
      return target.kind === 'sftp-session'
        || target.kind === 'ide-workspace'
        || target.kind === 'rag-index'
        || (target.kind === 'ssh-node' && target.capabilities.some((capability) => capability.startsWith('filesystem.')));
    case 'all':
    default:
      return true;
  }
}

function nodeTargetState(status: unknown, connected: boolean): AiTarget['state'] {
  if (connected) return 'connected';
  if (status === 'connecting') return 'opening';
  if (status === 'link-down' || status === 'error' || status === 'failed') return 'stale';
  if (status === 'disconnected') return 'unavailable';
  return 'available';
}

export async function listAiTargets(options: { query?: string; kind?: AiTargetKind | 'all'; view?: AiTargetView } = {}): Promise<AiTarget[]> {
  const query = options.query?.trim() ?? '';
  const kind = options.kind ?? 'all';
  const view = options.view ?? 'connections';
  const targets: AiTarget[] = [];

  try {
    const connections = await api.getConnections();
    for (const connection of connections) {
      targets.push({
        id: `saved-connection:${connection.id}`,
        kind: 'saved-connection',
        label: `${connection.name || connection.host} (${connection.username}@${connection.host}:${connection.port})`,
        state: 'available',
        capabilities: ['navigation.open', 'state.list'],
        refs: { connectionId: connection.id },
        metadata: {
          host: connection.host,
          port: connection.port,
          username: connection.username,
          name: connection.name,
          group: connection.group ?? null,
        },
      });
    }
  } catch {
    // Keep target discovery best-effort; runtime targets are still useful.
  }

  const appState = useAppStore.getState();
  const tabsBySession = new Map<string, string>();
  for (const tab of appState.tabs) {
    if (typeof tab.sessionId === 'string') {
      tabsBySession.set(tab.sessionId, tab.id);
    }
    targets.push({
      id: `app-surface:${tab.type}:${tab.id}`,
      kind: 'app-surface',
      label: `${tab.title || tab.type}`,
      state: appState.activeTabId === tab.id ? 'connected' : 'available',
      capabilities: ['navigation.open', 'state.list'],
      refs: { tabId: tab.id, sessionId: typeof tab.sessionId === 'string' ? tab.sessionId : undefined },
      metadata: { tabType: tab.type },
    });
  }

  for (const node of useSessionTreeStore.getState().nodes) {
      const connected = node.runtime?.status === 'connected' || node.runtime?.status === 'active';
      if (node.runtime?.connectionId || connected) {
      const host = typeof node.host === 'string' ? node.host : node.id;
      targets.push({
        id: `ssh-node:${node.id}`,
        kind: 'ssh-node',
        label: `${node.username ? `${node.username}@` : ''}${host}${node.port ? `:${node.port}` : ''}`,
        state: nodeTargetState(node.runtime?.status, connected),
        capabilities: ['command.run', 'filesystem.read', 'filesystem.write', 'state.list', 'navigation.open'],
        refs: {
          nodeId: node.id,
          connectionId: node.runtime?.connectionId ?? undefined,
          sessionId: node.runtime?.terminalIds?.[0],
        },
        metadata: {
          host: node.host,
          port: node.port,
          username: node.username,
          status: node.runtime?.status,
          terminalIds: node.runtime?.terminalIds ?? [],
          sftpSessionId: node.runtime?.sftpSessionId,
        },
      });
    }

    if (node.runtime?.sftpSessionId) {
      targets.push({
        id: `sftp-session:${node.runtime.sftpSessionId}`,
        kind: 'sftp-session',
        label: `SFTP ${node.host ?? node.id}`,
        state: 'connected',
        capabilities: ['filesystem.read', 'filesystem.write', 'state.list'],
        refs: { nodeId: node.id, sessionId: node.runtime.sftpSessionId, connectionId: node.runtime.connectionId ?? undefined },
        metadata: { host: node.host },
      });
    }
  }

  for (const entry of getAllEntries()) {
    const readiness = getTerminalReadiness(entry.sessionId);
    targets.push({
      id: `terminal-session:${entry.sessionId}`,
      kind: 'terminal-session',
      label: `${entry.terminalType === 'local_terminal' ? 'Local terminal' : 'SSH terminal'} ${entry.sessionId.slice(0, 8)}`,
      state: readiness?.writerReady ? 'connected' : 'opening',
      capabilities: ['terminal.observe', 'terminal.send', 'terminal.wait', 'state.list'],
      refs: { sessionId: entry.sessionId, tabId: entry.tabId },
      metadata: {
        paneId: entry.paneId,
        terminalType: entry.terminalType,
      },
    });
  }

  for (const terminal of useLocalTerminalStore.getState().terminals.values()) {
    targets.push({
      id: `terminal-session:${terminal.id}`,
      kind: 'terminal-session',
      label: `Local terminal ${terminal.shell?.label ?? terminal.id.slice(0, 8)}`,
      state: terminal.running === false ? 'stale' : 'connected',
      capabilities: ['terminal.observe', 'terminal.send', 'terminal.wait', 'state.list'],
      refs: { sessionId: terminal.id, tabId: tabsBySession.get(terminal.id) },
      metadata: { terminalType: 'local_terminal', shell: terminal.shell },
    });
  }

  targets.push({
    id: 'local-shell:default',
    kind: 'local-shell',
    label: 'Local shell',
    state: 'available',
    capabilities: ['command.run', 'navigation.open', 'state.list'],
    refs: {},
  });

  targets.push({
    id: 'settings:app',
    kind: 'settings',
    label: 'Settings',
    state: 'available',
    capabilities: ['settings.read', 'settings.write', 'navigation.open', 'state.list'],
    refs: {},
  });

  targets.push({
    id: 'rag-index:default',
    kind: 'rag-index',
    label: 'Knowledge base',
    state: 'available',
    capabilities: ['state.list', 'filesystem.search'],
    refs: {},
  });

  const ide = useIdeStore.getState();
  if (ide.project || ide.tabs.length > 0 || ide.nodeId) {
    targets.push({
      id: `ide-workspace:${ide.nodeId ?? 'active'}`,
      kind: 'ide-workspace',
      label: ide.project?.name ?? 'IDE workspace',
      state: 'connected',
      capabilities: ['filesystem.read', 'filesystem.write', 'navigation.open', 'state.list'],
      refs: { nodeId: ide.nodeId ?? undefined, tabId: ide.activeTabId ?? undefined },
      metadata: { rootPath: ide.project?.rootPath, activeTabId: ide.activeTabId },
    });
  }

  const deduped = [...new Map(targets.map((target) => [target.id, target])).values()];
  return deduped.filter((target) => (
    (kind === 'all' || target.kind === kind)
    && targetInView(target, view)
    && includesQuery(target, query)
  ));
}

export async function getAiTarget(targetId: string): Promise<AiTarget | null> {
  const targets = await listAiTargets({ view: 'all' });
  return targets.find((target) => target.id === targetId) ?? null;
}
