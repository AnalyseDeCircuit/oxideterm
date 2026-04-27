// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { Tab, TabType } from '../../../../types';
import type { ToolCapability, ToolTarget } from './types';
import { createToolTarget } from './targets';

export interface TargetDiscoveryState {
  tabs: readonly Tab[];
  activeTabId: string | null;
  sshNodes: ReadonlyArray<{
    id: string;
    host?: string;
    username?: string;
    port?: number;
    runtime?: {
      status?: string | null;
      connectionId?: string | null;
      terminalIds?: string[] | null;
      sftpSessionId?: string | null;
    };
  }>;
  localTerminals: ReadonlyMap<string, {
    running?: boolean;
    shell?: {
      label?: string;
      path?: string;
    };
  }>;
}

export interface ToolCapabilityStatus {
  targetId: string;
  targetLabel: string;
  capability: ToolCapability;
  available: boolean;
  notes?: string;
}

const SINGLETON_TAB_CAPABILITIES: Partial<Record<TabType, ToolCapability[]>> = {
  settings: ['settings.read', 'settings.write'],
  connection_monitor: ['state.list'],
  connection_pool: ['state.list'],
  topology: ['state.list'],
  file_manager: ['filesystem.read', 'filesystem.write'],
  session_manager: ['state.list', 'navigation.open'],
  plugin_manager: ['plugin.invoke', 'state.list'],
  launcher: ['navigation.open'],
  ai_agent: ['state.list'],
  activity: ['state.list'],
  notifications: ['state.list'],
  event_log: ['state.list'],
};

function sshNodeCapabilities(status?: string): ToolCapability[] {
  const connected = status === 'connected' || status === 'active' || status === 'ready';
  if (!connected) {
    return ['state.list'];
  }

  return [
    'command.run',
    'filesystem.read',
    'filesystem.write',
    'filesystem.search',
    'network.forward',
    'state.list',
    'navigation.open',
  ];
}

function terminalSessionCapabilities(): ToolCapability[] {
  return ['terminal.send', 'terminal.observe', 'terminal.wait', 'state.list'];
}

function tabCapabilities(tab: Tab): ToolCapability[] {
  switch (tab.type) {
    case 'terminal':
    case 'local_terminal':
      return terminalSessionCapabilities();
    case 'sftp':
      return ['filesystem.read', 'filesystem.write', 'state.list'];
    case 'ide':
      return ['filesystem.read', 'filesystem.write', 'filesystem.search', 'state.list'];
    case 'forwards':
      return ['network.forward', 'state.list'];
    default:
      return SINGLETON_TAB_CAPABILITIES[tab.type] ?? ['state.list'];
  }
}

function terminalSessionIdsForTab(tab: Tab): string[] {
  if (tab.rootPane?.type === 'leaf') {
    return [tab.rootPane.sessionId];
  }
  if (tab.rootPane?.type === 'group') {
    const ids: string[] = [];
    const visit = (node: NonNullable<Tab['rootPane']>) => {
      if (node.type === 'leaf') {
        ids.push(node.sessionId);
        return;
      }
      node.children.forEach(visit);
    };
    visit(tab.rootPane);
    return ids;
  }
  return tab.sessionId ? [tab.sessionId] : [];
}

export function buildToolTargets(state: TargetDiscoveryState): ToolTarget[] {
  const activeTab = state.tabs.find((tab) => tab.id === state.activeTabId);
  const targets: ToolTarget[] = [
    createToolTarget({
      id: 'local-shell:default',
      kind: 'local-shell',
      label: 'Local shell',
      active: activeTab?.type === 'local_terminal',
      capabilities: ['command.run', 'navigation.open', 'state.list'],
    }),
    createToolTarget({
      id: 'app:settings',
      kind: 'app-tab',
      label: 'Settings',
      active: activeTab?.type === 'settings',
      capabilities: ['settings.read', 'settings.write', 'navigation.open', 'state.list'],
      metadata: { tabType: 'settings', singleton: true },
    }),
  ];

  for (const [sessionId, info] of state.localTerminals) {
    const label = info.shell?.label || info.shell?.path || 'Local terminal';
    targets.push(createToolTarget({
      id: `terminal-session:${sessionId}`,
      kind: 'terminal-session',
      label: `${label} (${info.running ? 'running' : 'stopped'})`,
      active: activeTab?.type === 'local_terminal' && terminalSessionIdsForTab(activeTab).includes(sessionId),
      sessionId,
      capabilities: terminalSessionCapabilities(),
      metadata: {
        terminalType: 'local_terminal',
        running: Boolean(info.running),
      },
    }));
  }

  for (const node of state.sshNodes) {
    const status = node.runtime?.status ?? 'unknown';
    const host = `${node.username || '?'}@${node.host || '?'}:${node.port ?? 22}`;
    targets.push(createToolTarget({
      id: `ssh-node:${node.id}`,
      kind: 'ssh-node',
      label: `${host} (${status})`,
      active: activeTab?.nodeId === node.id,
      nodeId: node.id,
      capabilities: sshNodeCapabilities(status),
      metadata: {
        status,
        connectionId: node.runtime?.connectionId,
        terminalIds: node.runtime?.terminalIds ?? [],
        sftpSessionId: node.runtime?.sftpSessionId,
      },
    }));

    for (const sessionId of node.runtime?.terminalIds ?? []) {
      targets.push(createToolTarget({
        id: `terminal-session:${sessionId}`,
        kind: 'terminal-session',
        label: `${host} terminal ${sessionId}`,
        active: activeTab?.type === 'terminal' && terminalSessionIdsForTab(activeTab).includes(sessionId),
        nodeId: node.id,
        sessionId,
        capabilities: terminalSessionCapabilities(),
        metadata: {
          terminalType: 'terminal',
          status,
        },
      }));
    }
  }

  for (const tab of state.tabs) {
    const kind = tab.type === 'sftp'
      ? 'sftp-session'
      : tab.type === 'ide'
        ? 'ide-workspace'
        : 'app-tab';
    targets.push(createToolTarget({
      id: `tab:${tab.id}`,
      kind,
      label: `${tab.title} [${tab.type}]`,
      active: tab.id === state.activeTabId,
      nodeId: tab.nodeId,
      sessionId: tab.sessionId,
      tabId: tab.id,
      capabilities: tabCapabilities(tab),
      metadata: {
        tabType: tab.type,
        terminalSessionIds: terminalSessionIdsForTab(tab),
      },
    }));
  }

  return targets;
}

export function buildCapabilityStatuses(targets: readonly ToolTarget[]): ToolCapabilityStatus[] {
  return targets.flatMap((target) => target.capabilities.map((capability) => ({
    targetId: target.id,
    targetLabel: target.label,
    capability,
    available: true,
    notes: target.active ? 'active target' : undefined,
  })));
}
