// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiToolResult } from '../../../types';
import { listAiTargets, getAiTarget } from '../capabilities/targets';
import { connectAiTarget } from '../capabilities/connections';
import { observeTerminalTarget, runCommandOnTarget, sendTerminalInput } from '../capabilities/terminal';
import {
  getState,
  openAppSurface,
  readResource,
  selectAiTarget,
  transferResource,
  writeResource,
} from '../capabilities/resources';
import type { AiActionResult, AiResourceKind, AiTarget, AiTargetView, OrchestratorToolContext, OrchestratorToolName } from './types';
import { actionResultToToolResult, failAction } from './result';
import { isOrchestratorToolName, ORCHESTRATOR_TOOL_DEFS } from './definitions';
import { useSettingsStore } from '../../../store/settingsStore';
import { commandRecordFromToolResult } from './ledger';
import { recordCliAgentCommand } from './cliAgents';

function stringArg(args: Record<string, unknown>, key: string): string | undefined {
  const value = args[key];
  return typeof value === 'string' ? value : undefined;
}

function boolArg(args: Record<string, unknown>, key: string): boolean | undefined {
  const value = args[key];
  return typeof value === 'boolean' ? value : undefined;
}

function numberArg(args: Record<string, unknown>, key: string): number | undefined {
  const value = args[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function targetViewArg(args: Record<string, unknown>): AiTargetView | undefined {
  const value = stringArg(args, 'view');
  if (value === 'connections' || value === 'live_sessions' || value === 'app_surfaces' || value === 'files' || value === 'all') {
    return value;
  }
  return undefined;
}

function resourceArg(args: Record<string, unknown>): AiResourceKind | undefined {
  const value = stringArg(args, 'resource');
  if (value === 'settings' || value === 'file' || value === 'directory' || value === 'sftp' || value === 'ide' || value === 'rag') {
    return value;
  }
  return undefined;
}

function recoveryActionsForTarget(target: AiTarget): AiActionResult['nextActions'] {
  if (target.kind === 'saved-connection' || target.kind === 'ssh-node') {
    return [{ action: 'connect_target', args: { target_id: target.id }, reason: 'Reconnect or open this SSH target before continuing.' }];
  }
  if (target.kind === 'terminal-session') {
    return [
      { action: 'observe_terminal', args: { target_id: target.id }, reason: 'Check whether the terminal has become ready.' },
      { action: 'list_targets', reason: 'Find a live terminal or SSH target if this one is stale.' },
    ];
  }
  return [{ action: 'list_targets', reason: 'Find a currently available target before continuing.' }];
}

function requiresLiveTarget(target: AiTarget): boolean {
  return target.kind === 'ssh-node' || target.kind === 'terminal-session' || target.kind === 'sftp-session';
}

async function requireTarget(
  targetId: string | undefined,
  risk: AiActionResult['risk'],
  options: { requireLive?: boolean; actionName?: string } = {},
): Promise<{ target?: Awaited<ReturnType<typeof getAiTarget>>; error?: AiActionResult }> {
  if (!targetId) {
    return { error: failAction('target_id is required.', 'missing_target_id', 'This task tool requires an explicit target_id.', risk, {
      nextActions: [{ action: 'list_targets', reason: 'Find the correct target before acting.' }],
    }) };
  }
  const target = await getAiTarget(targetId);
  if (!target) {
    return { error: failAction('Target not found.', 'target_not_found', `Target not found: ${targetId}`, risk, {
      nextActions: [{ action: 'list_targets', reason: 'Refresh available targets before continuing.' }],
    }) };
  }
  if (options.requireLive && requiresLiveTarget(target) && target.state !== 'connected') {
    return {
      error: failAction(
        'Target is not ready.',
        'target_not_ready',
        `${target.id} is ${target.state}; ${options.actionName ?? 'this action'} requires a connected target.`,
        risk,
        {
          target,
          nextActions: recoveryActionsForTarget(target),
        },
      ),
    };
  }
  return { target };
}

async function executeAction(name: OrchestratorToolName, args: Record<string, unknown>, context: OrchestratorToolContext): Promise<AiActionResult> {
  switch (name) {
    case 'list_targets': {
      const targets = await listAiTargets({
        query: stringArg(args, 'query'),
        kind: (stringArg(args, 'kind') ?? 'all') as never,
        view: targetViewArg(args),
      });
      return {
        ok: true,
        summary: `Found ${targets.length} target${targets.length === 1 ? '' : 's'}.`,
        targets,
        data: targets,
        output: targets.map((target) => `${target.id} — ${target.label} [${target.kind}, ${target.state}]`).join('\n') || 'No targets found.',
        risk: 'read',
      };
    }
    case 'select_target':
      return selectAiTarget({
        query: stringArg(args, 'query') ?? '',
        intent: stringArg(args, 'intent'),
        kind: stringArg(args, 'kind'),
      });
    case 'connect_target':
      return connectAiTarget(stringArg(args, 'target_id') ?? '');
    case 'run_command': {
      const { target, error } = await requireTarget(stringArg(args, 'target_id'), 'execute', {
        requireLive: true,
        actionName: 'run_command',
      });
      if (error) return error;
      return runCommandOnTarget({
        target: target!,
        command: stringArg(args, 'command') ?? '',
        cwd: stringArg(args, 'cwd'),
        timeoutSecs: numberArg(args, 'timeout_secs'),
        awaitOutput: boolArg(args, 'await_output'),
        dangerousCommandApproved: context.dangerousCommandApproved,
        abortSignal: context.abortSignal,
      });
    }
    case 'observe_terminal': {
      const { target, error } = await requireTarget(stringArg(args, 'target_id'), 'read');
      if (error) return error;
      return observeTerminalTarget(target!, numberArg(args, 'max_chars') ?? 4000);
    }
    case 'send_terminal_input': {
      const { target, error } = await requireTarget(stringArg(args, 'target_id'), 'interactive', {
        requireLive: true,
        actionName: 'send_terminal_input',
      });
      if (error) return error;
      return sendTerminalInput({
        target: target!,
        text: stringArg(args, 'text'),
        appendEnter: boolArg(args, 'append_enter'),
        control: stringArg(args, 'control'),
      });
    }
    case 'read_resource': {
      const { target, error } = await requireTarget(stringArg(args, 'target_id'), 'read', {
        requireLive: true,
        actionName: 'read_resource',
      });
      if (error) return error;
      return readResource({
        target: target!,
        resource: resourceArg(args) ?? '' as AiResourceKind,
        path: stringArg(args, 'path'),
        section: stringArg(args, 'section'),
        query: stringArg(args, 'query'),
      });
    }
    case 'write_resource': {
      const { target, error } = await requireTarget(stringArg(args, 'target_id'), 'write', {
        requireLive: true,
        actionName: 'write_resource',
      });
      if (error) return error;
      return writeResource({
        target: target!,
        resource: resourceArg(args) ?? '' as AiResourceKind,
        section: stringArg(args, 'section'),
        key: stringArg(args, 'key'),
        value: args.value,
        path: stringArg(args, 'path'),
        content: stringArg(args, 'content'),
        expectedHash: stringArg(args, 'expected_hash'),
        dryRun: boolArg(args, 'dry_run'),
      });
    }
    case 'transfer_resource': {
      const { target, error } = await requireTarget(stringArg(args, 'target_id'), 'write', {
        requireLive: true,
        actionName: 'transfer_resource',
      });
      if (error) return error;
      const direction = stringArg(args, 'direction');
      if (direction !== 'upload' && direction !== 'download') {
        return failAction('Transfer direction is required.', 'missing_transfer_direction', 'direction must be upload or download.', 'write', { target: target! });
      }
      return transferResource({
        target: target!,
        direction,
        sourcePath: stringArg(args, 'source_path') ?? '',
        destinationPath: stringArg(args, 'destination_path') ?? '',
      });
    }
    case 'open_app_surface':
      return openAppSurface({
        surface: stringArg(args, 'surface') ?? '',
        targetId: stringArg(args, 'target_id'),
        section: stringArg(args, 'section'),
      });
    case 'get_state':
      return getState(stringArg(args, 'scope') ?? 'targets', stringArg(args, 'target_id'));
    case 'remember_preference': {
      const preference = stringArg(args, 'preference')?.trim();
      if (!preference) return failAction('Preference is required.', 'missing_preference', 'remember_preference requires preference text.', 'write');
      const store = useSettingsStore.getState();
      const current = store.settings.ai.memory?.content?.trim() ?? '';
      const next = [current, `- ${preference}`].filter(Boolean).join('\n');
      store.updateAi('memory', { enabled: store.settings.ai.memory?.enabled ?? true, content: next });
      return { ok: true, summary: 'Preference remembered.', output: preference, risk: 'write' };
    }
    case 'recall_preferences': {
      const memory = useSettingsStore.getState().settings.ai.memory;
      return {
        ok: true,
        summary: memory?.content?.trim() ? 'Preferences recalled.' : 'No saved preferences.',
        data: memory,
        output: memory?.content?.trim() || 'No saved preferences.',
        risk: 'read',
      };
    }
  }
}

export async function executeOrchestratorTool(
  toolName: string,
  args: Record<string, unknown>,
  context: OrchestratorToolContext,
  toolCallId = '',
): Promise<AiToolResult> {
  const startedAt = performance.now();
  if (!isOrchestratorToolName(toolName)) {
    return actionResultToToolResult(
      toolCallId,
      toolName,
      failAction('Unknown orchestrator tool.', 'unknown_tool', `${toolName} is not an OxideSens task tool.`, 'read'),
      performance.now() - startedAt,
    );
  }

  const result = await executeAction(toolName, args, context);
  const previewProbe = actionResultToToolResult(toolCallId, toolName, result, performance.now() - startedAt);
  const exitCode = result.data && typeof result.data === 'object' && 'exitCode' in result.data
    ? (result.data as { exitCode?: number | null }).exitCode
    : undefined;
  const record = commandRecordFromToolResult({
    toolName,
    args,
    target: result.target,
    ok: result.ok,
    waitingForInput: result.waitingForInput,
    risk: result.risk,
    approvalMode: context.approvalMode,
    outputPreview: previewProbe.envelope?.outputPreview,
    rawOutputStored: Boolean(previewProbe.envelope?.rawOutput),
    exitCode,
  });
  if (record) {
    recordCliAgentCommand({
      command: record.command,
      targetId: record.targetId,
      sessionId: record.sessionId,
      nodeId: record.nodeId,
      status: record.status === 'waiting_for_input' ? 'waiting_for_input' : record.status === 'error' ? 'failed' : 'running',
    });
  }
  return actionResultToToolResult(toolCallId, toolName, result, performance.now() - startedAt, {
    commandRecordId: record?.commandId,
    policyDecision: context.policyDecision ? {
      decision: context.policyDecision.decision,
      risk: context.policyDecision.risk,
      reasonCode: context.policyDecision.reasonCode,
      matchedPolicyKey: context.policyDecision.matchedPolicyKey,
      approvalMode: context.policyDecision.approvalMode,
      profileId: context.policyDecision.profileId,
    } : undefined,
    profileId: context.profileId,
  });
}

export function getOrchestratorToolDefs() {
  return ORCHESTRATOR_TOOL_DEFS;
}
