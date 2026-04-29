// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import {
  nodeAgentReadFile,
  nodeAgentWriteFile,
  nodeSftpListDir,
  nodeSftpPreview,
  nodeSftpStartDirectoryTransfer,
  nodeSftpUpload,
  nodeSftpDownload,
  nodeSftpWrite,
} from '../../api';
import { ragSearch } from '../../api';
import { useAppStore } from '../../../store/appStore';
import { useEventLogStore } from '../../../store/eventLogStore';
import { useLocalTerminalStore } from '../../../store/localTerminalStore';
import { useSettingsStore } from '../../../store/settingsStore';
import { useSessionTreeStore } from '../../../store/sessionTreeStore';
import { useTransferStore, type TransferState } from '../../../store/transferStore';
import type { TabType } from '../../../types';
import { getAllEntries } from '../../terminalRegistry';
import type { AiActionResult, AiResourceKind, AiTarget, AiTargetIntent, AiTargetKind, AiTargetView } from '../orchestrator/types';
import { failAction } from '../orchestrator/result';
import { getAiRuntimeEpoch, makeAiStateVersion } from '../orchestrator/runtimeEpoch';
import { getAiTarget, listAiTargets } from './targets';

function truncate(value: string, maxChars = 12000): string {
  if (value.length <= maxChars) return value;
  return `${value.slice(0, maxChars)}\n[truncated ${value.length - maxChars} chars]`;
}

function nodeIdFromTarget(target: AiTarget): string | null {
  return target.refs.nodeId ?? null;
}

function jsonOutput(value: unknown): string {
  return truncate(JSON.stringify(value, null, 2));
}

const TARGET_INTENTS: ReadonlySet<string> = new Set([
  'connection',
  'command',
  'terminal',
  'settings',
  'file',
  'sftp',
  'app_surface',
  'knowledge',
  'status',
  'local',
  'unknown',
]);

const RESOURCE_KINDS: ReadonlySet<string> = new Set(['settings', 'file', 'directory', 'sftp', 'ide', 'rag']);

function normalizeIntent(intent: string | undefined): AiTargetIntent | null {
  if (!intent || !TARGET_INTENTS.has(intent)) return null;
  return intent as AiTargetIntent;
}

function viewForIntent(intent: AiTargetIntent): AiTargetView {
  switch (intent) {
    case 'connection':
    case 'status':
      return 'connections';
    case 'command':
    case 'terminal':
      return 'live_sessions';
    case 'local':
      return 'app_surfaces';
    case 'settings':
    case 'app_surface':
      return 'app_surfaces';
    case 'file':
    case 'sftp':
    case 'knowledge':
      return 'files';
    case 'unknown':
    default:
      return 'connections';
  }
}

function isCommandLikeQuery(query: string): boolean {
  const trimmed = query.trim();
  if (!trimmed) return false;
  if (/^(?:sudo\s+)?(?:pwd|ls|cd|cat|tail|head|grep|find|ps|top|htop|df|du|free|whoami|id|uname|docker|kubectl|systemctl|journalctl|git|npm|pnpm|yarn|cargo|python|node|ssh)\b/.test(trimmed)) {
    return true;
  }
  return /[;&|`$<>]/.test(trimmed) || /\s-{1,2}\w/.test(trimmed);
}

function kindFromString(kind: string | undefined): AiTargetKind | 'all' | undefined {
  if (!kind || kind === 'all') return kind as 'all' | undefined;
  const kinds: AiTargetKind[] = ['saved-connection', 'ssh-node', 'terminal-session', 'local-shell', 'sftp-session', 'ide-workspace', 'settings', 'app-surface', 'rag-index'];
  return kinds.includes(kind as AiTargetKind) ? kind as AiTargetKind : undefined;
}

function resourceFromString(resource: string): AiResourceKind | null {
  return RESOURCE_KINDS.has(resource) ? resource as AiResourceKind : null;
}

export async function selectAiTarget(options: { query: string; kind?: string; intent?: string }): Promise<AiActionResult> {
  const intent = normalizeIntent(options.intent);
  if (!intent) {
    return failAction('Target intent is required.', 'missing_target_intent', 'select_target requires intent: connection, command, terminal, settings, file, sftp, app_surface, knowledge, status, local, or unknown.', 'read', {
      verified: false,
      nextActions: [{ action: 'list_targets', args: { view: 'connections', query: options.query }, reason: 'Inspect the correct target view before selecting.' }],
    });
  }

  if ((intent === 'command' || intent === 'terminal') && isCommandLikeQuery(options.query)) {
    return failAction('Command text is not a target.', 'command_query_not_target', `"${options.query}" looks like a command. Select a live SSH or terminal target first, then call run_command with this command.`, 'read', {
      verified: false,
      nextActions: [
        { action: 'list_targets', args: { view: intent === 'command' ? 'live_sessions' : 'connections' }, reason: 'Choose the execution target before running the command.' },
      ],
    });
  }

  const view = viewForIntent(intent);
  const targets = await listAiTargets({ query: options.query, kind: kindFromString(options.kind), view });
  if (targets.length === 0) {
    return failAction('No matching target found.', 'target_not_found', `No target matched "${options.query}".`, 'read', {
      verified: false,
      nextActions: [
        { action: 'list_targets', args: { view, query: options.query }, reason: 'Inspect available targets and ask the user to choose.' },
        ...(intent === 'command' || intent === 'terminal'
          ? [{ action: 'list_targets', args: { view: 'connections', query: options.query }, reason: 'If the named host is saved but not live, connect it before running commands.' }]
          : []),
      ],
    });
  }

  if (targets.length > 1) {
    return {
      ok: false,
      summary: 'Multiple targets match. Ask the user to choose one.',
      targets,
      output: targets.map((target, index) => `${index + 1}. ${target.id} — ${target.label} [${target.kind}]`).join('\n'),
      error: { code: 'target_disambiguation_required', message: 'Multiple targets match this query.', recoverable: true },
      nextActions: [{ action: 'select_target', args: { query: options.query, intent, kind: options.kind }, reason: 'Retry with a more specific label, host, or target id.' }],
      verified: false,
      risk: 'read',
    };
  }

  const target = targets[0];
  return {
    ok: true,
    summary: `Selected target: ${target.label}`,
    target,
    data: target,
    output: jsonOutput(target),
    verified: true,
    risk: 'read',
  };
}

export async function readResource(options: {
  target: AiTarget;
  resource: AiResourceKind;
  path?: string;
  section?: string;
  query?: string;
}): Promise<AiActionResult> {
  const { target, resource } = options;
  if (!resourceFromString(resource)) {
    return failAction('Unsupported resource read.', 'unsupported_resource', `Cannot read unsupported resource "${resource}".`, 'read', { target, verified: false });
  }

  if (resource === 'settings') {
    const settings = useSettingsStore.getState().settings;
    const section = options.section && options.section in settings
      ? (settings as unknown as Record<string, unknown>)[options.section]
      : settings;
    return {
      ok: true,
      summary: options.section ? `Read settings section ${options.section}.` : 'Read settings.',
      data: section,
      output: jsonOutput(section),
      target,
      verified: true,
      risk: 'read',
    };
  }

  if (target.kind === 'rag-index' || resource === 'rag') {
    const query = options.query ?? options.path ?? '';
    const results = await ragSearch({ query, collectionIds: [], topK: 8 });
    return { ok: true, summary: `Found ${results.length} knowledge results.`, data: results, output: jsonOutput(results), target, verified: true, risk: 'read' };
  }

  const nodeId = nodeIdFromTarget(target);
  if (!nodeId && target.kind !== 'sftp-session') {
    return failAction('Target cannot read resources.', 'unsupported_read_target', `${target.kind} does not expose readable resources.`, 'read', { target, verified: false });
  }

  const path = options.path;
  if (!path) {
    return failAction('Resource path is required.', 'missing_path', 'read_resource requires path for file or directory resources.', 'read', { target, verified: false });
  }

  if (resource === 'directory' || resource === 'sftp') {
    const entries = nodeId ? await nodeSftpListDir(nodeId, path) : [];
    return { ok: true, summary: `Listed ${entries.length} entries.`, data: entries, output: jsonOutput(entries), target, verified: true, risk: 'read' };
  }

  if (resource === 'file' || resource === 'ide') {
    if (nodeId) {
      try {
        const result = await nodeAgentReadFile(nodeId, path);
        return { ok: true, summary: `Read remote file ${path}.`, data: result, output: truncate(result.content), target, verified: true, risk: 'read' };
      } catch {
        const preview = await nodeSftpPreview(nodeId, path);
        return { ok: true, summary: `Read remote file preview ${path}.`, data: preview, output: jsonOutput(preview), target, verified: true, risk: 'read' };
      }
    }
  }

  return failAction('Unsupported resource read.', 'unsupported_resource', `Cannot read resource "${resource}" from ${target.kind}.`, 'read', { target, verified: false });
}

export async function writeResource(options: {
  target: AiTarget;
  resource: AiResourceKind;
  section?: string;
  key?: string;
  value?: unknown;
  path?: string;
  content?: string;
  expectedHash?: string;
  dryRun?: boolean;
}): Promise<AiActionResult> {
  const { target, resource } = options;
  if (resource !== 'settings' && resource !== 'file') {
    return failAction('Unsupported resource write.', 'unsupported_resource_write', `write_resource only supports settings or file, not "${resource}".`, 'write', {
      target,
      verified: false,
      nextActions: [{ action: 'read_resource', args: { target_id: target.id, resource, path: options.path, section: options.section }, reason: 'Read or inspect the resource instead of writing it.' }],
    });
  }

  if (resource === 'settings') {
    const { section, key } = options;
    if (!section || !key) {
      return failAction('Settings section and key are required.', 'missing_settings_key', 'write_resource(settings) requires section and key.', 'write', { target, verified: false });
    }
    if (options.dryRun) {
      return { ok: true, summary: `Dry-run settings write ${section}.${key}.`, data: { section, key, value: options.value }, output: 'Dry-run only; settings were not changed.', target, verified: false, risk: 'write' };
    }
    const store = useSettingsStore.getState() as unknown as Record<string, unknown>;
    const updaterName = `update${section.charAt(0).toUpperCase()}${section.slice(1)}`;
    const updater = store[updaterName];
    if (typeof updater !== 'function') {
      return failAction('Settings section cannot be updated.', 'unsupported_settings_section', `No updater found for ${section}.`, 'write', { target, verified: false });
    }
    (updater as (key: string, value: unknown) => void)(key, options.value);
    return { ok: true, summary: `Updated settings ${section}.${key}.`, data: { section, key, value: options.value }, output: `${section}.${key} updated.`, target, verified: true, risk: 'write' };
  }

  const nodeId = nodeIdFromTarget(target);
  if (!nodeId) {
    return failAction('Target cannot write resources.', 'unsupported_write_target', `${target.kind} does not expose writable resources.`, 'write', { target, verified: false });
  }

  if (resource === 'file') {
    if (!options.path || typeof options.content !== 'string') {
      return failAction('Path and content are required.', 'missing_file_write_args', 'write_resource(file) requires path and content.', 'write', { target, verified: false });
    }
    if (options.dryRun) {
      return { ok: true, summary: `Dry-run file write ${options.path}.`, output: 'Dry-run only; file was not changed.', target, verified: false, risk: 'write' };
    }
    try {
      const result = await nodeAgentWriteFile(nodeId, options.path, options.content, options.expectedHash);
      return { ok: true, summary: `Wrote remote file ${options.path}.`, data: result, output: jsonOutput(result), target, verified: true, risk: 'write' };
    } catch {
      const result = await nodeSftpWrite(nodeId, options.path, options.content);
      return { ok: true, summary: `Wrote remote file ${options.path}.`, data: result, output: jsonOutput(result), target, verified: true, risk: 'write' };
    }
  }

  return failAction('Unsupported resource write.', 'unsupported_resource', `Cannot write resource "${resource}" from ${target.kind}.`, 'write', { target, verified: false });
}

export async function transferResource(options: {
  target: AiTarget;
  direction: 'upload' | 'download';
  sourcePath: string;
  destinationPath: string;
}): Promise<AiActionResult> {
  const nodeId = nodeIdFromTarget(options.target);
  if (!nodeId) {
    return failAction('SFTP transfer requires an SSH/SFTP target.', 'missing_node_id', 'transfer_resource requires a target with nodeId.', 'write', { target: options.target, verified: false });
  }

  const transferId = crypto.randomUUID();
  try {
    const directory = /[\\/]$/.test(options.sourcePath) || /[\\/]$/.test(options.destinationPath);
    if (directory) {
      const response = await nodeSftpStartDirectoryTransfer(
        nodeId,
        options.direction,
        options.direction === 'upload' ? options.sourcePath : options.destinationPath,
        options.direction === 'upload' ? options.destinationPath : options.sourcePath,
        transferId,
        'auto',
      );
      return { ok: true, summary: `Started ${options.direction} directory transfer.`, data: response, output: jsonOutput(response), target: options.target, verified: true, risk: 'write' };
    }

    if (options.direction === 'upload') {
      await nodeSftpUpload(nodeId, options.sourcePath, options.destinationPath, transferId);
    } else {
      await nodeSftpDownload(nodeId, options.sourcePath, options.destinationPath, transferId);
    }
    return { ok: true, summary: `Completed ${options.direction} transfer.`, data: { transferId }, output: `transfer_id=${transferId}`, target: options.target, verified: true, risk: 'write' };
  } catch (error) {
    return failAction('SFTP transfer failed.', 'sftp_transfer_failed', error instanceof Error ? error.message : String(error), 'write', { target: options.target, verified: false });
  }
}

export async function openAppSurface(options: { surface: string; targetId?: string; section?: string }): Promise<AiActionResult> {
  const app = useAppStore.getState();
  const target = options.targetId ? await getAiTarget(options.targetId) : undefined;

  if (options.surface === 'local_terminal' || options.surface === 'terminal') {
    const terminal = await useLocalTerminalStore.getState().createTerminal();
    app.createTab('local_terminal', terminal.id);
    const openedTarget: AiTarget = {
      id: `terminal-session:${terminal.id}`,
      kind: 'terminal-session',
      label: `Local terminal ${terminal.shell.label}`,
      state: 'connected',
      capabilities: ['terminal.observe', 'terminal.send', 'terminal.wait', 'state.list'],
      refs: { sessionId: terminal.id },
      metadata: { terminalType: 'local_terminal' },
    };
    return { ok: true, summary: 'Opened local terminal.', target: openedTarget, data: openedTarget, output: jsonOutput(openedTarget), verified: true, risk: 'write' };
  }

  const surfaceToTab: Record<string, TabType> = {
    settings: 'settings',
    connection_manager: 'session_manager',
    connection_pool: 'connection_pool',
    connection_monitor: 'connection_monitor',
    file_manager: 'file_manager',
    sftp: 'sftp',
    ide: 'ide',
  };
  const tabType = surfaceToTab[options.surface];
  if (!tabType) {
    return failAction('Unknown app surface.', 'unknown_app_surface', `Unknown app surface: ${options.surface}`, 'write', { target: target ?? undefined, verified: false });
  }
  app.createTab(tabType, target?.refs.sessionId, { nodeId: target?.refs.nodeId });
  if (options.surface === 'settings' && options.section) {
    window.dispatchEvent(new CustomEvent('oxideterm:open-settings-tab', { detail: { tab: options.section } }));
  }
  return { ok: true, summary: `Opened ${options.surface}.`, target: target ?? undefined, output: `Opened ${options.surface}.`, verified: true, risk: 'write' };
}

const STATE_SCOPES = new Set(['connections', 'transfers', 'settings', 'targets', 'health', 'active']);

function compactTarget(target: AiTarget) {
  return {
    id: target.id,
    kind: target.kind,
    label: target.label,
    state: target.state,
    capabilities: target.capabilities,
    refs: target.refs,
  };
}

function countBy<T extends string>(values: T[]): Record<T, number> {
  return values.reduce((acc, value) => {
    acc[value] = (acc[value] ?? 0) + 1;
    return acc;
  }, {} as Record<T, number>);
}

function summarizeTransfers() {
  const transfers = useTransferStore.getState().getAllTransfers();
  const states = transfers.map((transfer) => transfer.state);
  const stateCounts = countBy(states as TransferState[]);
  const counts = {
    pending: 0,
    active: 0,
    paused: 0,
    completed: 0,
    cancelled: 0,
    error: 0,
  };
  for (const [state, count] of Object.entries(stateCounts)) {
    counts[state as TransferState] = count;
  }
  const activeOrRecent = transfers
    .filter((transfer) => ['pending', 'active', 'paused', 'error'].includes(transfer.state))
    .concat(transfers.filter((transfer) => ['completed', 'cancelled'].includes(transfer.state)).slice(-5))
    .slice(-20)
    .map((transfer) => ({
      id: transfer.id,
      nodeId: transfer.nodeId,
      name: transfer.name,
      direction: transfer.direction,
      state: transfer.state,
      size: transfer.size,
      transferred: transfer.transferred,
      error: transfer.error,
      startTime: transfer.startTime,
      endTime: transfer.endTime,
    }));
  return {
    total: transfers.length,
    counts,
    transfers: activeOrRecent,
  };
}

export async function getState(scope: string, targetId?: string): Promise<AiActionResult> {
  const target = targetId ? await getAiTarget(targetId) : undefined;
  const runtimeEpoch = getAiRuntimeEpoch();
  if (!STATE_SCOPES.has(scope)) {
    return failAction('Unknown state scope.', 'unknown_state_scope', `Unknown get_state scope "${scope}". Valid scopes: connections, transfers, settings, targets, health, active.`, 'read', {
      target: target ?? undefined,
      verified: false,
      nextActions: [{ action: 'get_state', args: { scope: 'targets' }, reason: 'Inspect valid target state instead.' }],
    });
  }

  if (scope === 'targets') {
    const [connections, liveSessions, appSurfaces, files, all] = await Promise.all([
      listAiTargets({ view: 'connections' }),
      listAiTargets({ view: 'live_sessions' }),
      listAiTargets({ view: 'app_surfaces' }),
      listAiTargets({ view: 'files' }),
      listAiTargets({ view: 'all' }),
    ]);
    const data = {
      runtimeEpoch,
      views: {
        connections: { count: connections.length, targets: connections.map(compactTarget) },
        live_sessions: { count: liveSessions.length, targets: liveSessions.map(compactTarget) },
        app_surfaces: { count: appSurfaces.length, targets: appSurfaces.map(compactTarget) },
        files: { count: files.length, targets: files.map(compactTarget) },
        all: { count: all.length },
      },
    };
    return { ok: true, summary: `Found ${all.length} total targets across views.`, targets: all, data, output: jsonOutput(data), verified: true, runtimeEpoch, stateVersion: makeAiStateVersion('targets', [all.length, connections.length, liveSessions.length, appSurfaces.length, files.length]), risk: 'read' };
  }

  if (scope === 'active') {
    const app = useAppStore.getState();
    const activeTab = app.tabs.find((tab) => tab.id === app.activeTabId) ?? null;
    const tree = useSessionTreeStore.getState();
    const activeNode = tree.selectedNodeId ? tree.nodes.find((node) => node.id === tree.selectedNodeId) ?? null : null;
    const activeSessionId = activeTab && typeof activeTab.sessionId === 'string' ? activeTab.sessionId : activeNode?.runtime?.terminalIds?.[0] ?? null;
    const targets = await listAiTargets({ view: 'all' });
    const currentTargets = targets.filter((candidate) => (
      candidate.refs.tabId === activeTab?.id
      || candidate.refs.sessionId === activeSessionId
      || candidate.refs.nodeId === activeNode?.id
    ));
    const data = {
      runtimeEpoch,
      activeTab: activeTab ? { id: activeTab.id, type: activeTab.type, title: activeTab.title, sessionId: activeTab.sessionId ?? null } : null,
      activeNode: activeNode ? { id: activeNode.id, host: activeNode.host, username: activeNode.username, status: activeNode.runtime?.status ?? null, terminalIds: activeNode.runtime?.terminalIds ?? [] } : null,
      activeSessionId,
      targets: currentTargets.map(compactTarget),
    };
    return { ok: true, summary: activeTab || activeNode ? 'Read active runtime state.' : 'No active tab or terminal session.', targets: currentTargets, data, output: jsonOutput(data), verified: true, runtimeEpoch, stateVersion: makeAiStateVersion('active', [activeTab?.id, activeNode?.id, activeSessionId]), risk: 'read' };
  }
  if (scope === 'settings') {
    const settings = useSettingsStore.getState().settings;
    const summary = {
      ai: { enabled: settings.ai.enabled, toolUse: settings.ai.toolUse },
      terminal: { renderer: settings.terminal.renderer, encoding: settings.terminal.terminalEncoding },
      sftp: { directoryParallelism: settings.sftp?.directoryParallelism },
    };
    return { ok: true, summary: 'Read settings summary.', data: summary, output: jsonOutput(summary), target: target ?? undefined, verified: true, runtimeEpoch, stateVersion: makeAiStateVersion('settings', [settings.ai.enabled, settings.terminal.renderer, settings.terminal.terminalEncoding]), risk: 'read' };
  }
  if (scope === 'connections') {
    const targets = await listAiTargets({ view: 'connections' });
    const data = {
      runtimeEpoch,
      total: targets.length,
      counts: {
        saved: targets.filter((entry) => entry.kind === 'saved-connection').length,
        live: targets.filter((entry) => entry.kind === 'ssh-node' && entry.state === 'connected').length,
        linkDown: targets.filter((entry) => entry.kind === 'ssh-node' && entry.state === 'stale').length,
        error: targets.filter((entry) => entry.kind === 'ssh-node' && entry.metadata?.status === 'error').length,
      },
      targets: targets.map(compactTarget),
    };
    return { ok: true, summary: `Found ${targets.length} connection targets.`, targets, data, output: jsonOutput(data), target: target ?? undefined, verified: true, runtimeEpoch, stateVersion: makeAiStateVersion('connections', [targets.length, data.counts.live, data.counts.linkDown, data.counts.error]), risk: 'read' };
  }
  if (scope === 'transfers') {
    const data = { runtimeEpoch, ...summarizeTransfers() };
    return { ok: true, summary: `Found ${data.total} tracked transfers.`, data, output: jsonOutput(data), target: target ?? undefined, verified: true, runtimeEpoch, stateVersion: makeAiStateVersion('transfers', [data.total, data.counts.active, data.counts.pending, data.counts.error]), risk: 'read' };
  }
  if (scope === 'health') {
    const app = useAppStore.getState();
    const tree = useSessionTreeStore.getState();
    const transfers = summarizeTransfers();
    const events = useEventLogStore.getState().entries;
    const recent = events.filter((entry) => Date.now() - entry.timestamp < 10 * 60 * 1000);
    const data = {
      runtimeEpoch,
      tabs: { open: app.tabs.length, activeTabId: app.activeTabId },
      terminalRegistry: { entries: getAllEntries().length },
      localTerminals: { count: useLocalTerminalStore.getState().terminals.size },
      sshNodes: {
        total: tree.nodes.length,
        states: countBy(tree.nodes.map((node) => String(node.runtime?.status ?? 'unknown'))),
      },
      transfers: { total: transfers.total, counts: transfers.counts },
      recentEvents: {
        total: recent.length,
        warnings: recent.filter((entry) => entry.severity === 'warn').length,
        errors: recent.filter((entry) => entry.severity === 'error').length,
      },
    };
    return { ok: true, summary: 'Read OxideTerm health state.', data, output: jsonOutput(data), target: target ?? undefined, verified: true, runtimeEpoch, stateVersion: makeAiStateVersion('health', [app.tabs.length, getAllEntries().length, transfers.total, recent.length]), risk: 'read' };
  }

  return failAction('Unknown state scope.', 'unknown_state_scope', `Unknown get_state scope "${scope}".`, 'read', { target: target ?? undefined, verified: false });
}
