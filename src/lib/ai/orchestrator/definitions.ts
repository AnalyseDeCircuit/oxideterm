// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { AiToolDefinition } from '../providers';
import { hasDeniedCommands } from '../tools/toolDefinitions';
import { ORCHESTRATOR_TOOL_NAMES, type AiActionRisk, type OrchestratorToolName } from './types';

const TARGET_KIND_ENUM = ['all', 'saved-connection', 'ssh-node', 'terminal-session', 'local-shell', 'sftp-session', 'ide-workspace', 'settings', 'app-surface', 'rag-index'] as const;
const TARGET_VIEW_ENUM = ['connections', 'live_sessions', 'app_surfaces', 'files', 'all'] as const;
const TARGET_INTENT_ENUM = ['connection', 'command', 'terminal', 'settings', 'file', 'sftp', 'app_surface', 'knowledge', 'status', 'local', 'unknown'] as const;
const RESOURCE_KIND_ENUM = ['settings', 'file', 'directory', 'sftp', 'ide', 'rag'] as const;

export const ORCHESTRATOR_READ_TOOLS = new Set<OrchestratorToolName>([
  'list_targets',
  'select_target',
  'observe_terminal',
  'read_resource',
  'get_state',
  'recall_preferences',
]);

export const ORCHESTRATOR_WRITE_TOOLS = new Set<OrchestratorToolName>([
  'connect_target',
  'run_command',
  'send_terminal_input',
  'write_resource',
  'transfer_resource',
  'open_app_surface',
  'remember_preference',
]);

export function isOrchestratorToolName(name: string): name is OrchestratorToolName {
  return (ORCHESTRATOR_TOOL_NAMES as readonly string[]).includes(name);
}

export function orchestratorRiskForTool(name: string, args: Record<string, unknown> = {}): AiActionRisk {
  if (name === 'run_command') {
    if (hasDeniedCommands(name, args)) {
      return 'destructive';
    }
    return 'execute';
  }

  if (name === 'send_terminal_input') return 'interactive';
  if (name === 'write_resource' || name === 'transfer_resource') return 'write';
  if (name === 'connect_target' || name === 'open_app_surface' || name === 'remember_preference') return 'write';
  return 'read';
}

export function orchestratorApprovalKeyForTool(name: string, args: Record<string, unknown> = {}): string {
  if (name === 'write_resource') {
    const resource = typeof args.resource === 'string' ? args.resource : '';
    if (resource === 'settings' || resource === 'file') {
      return `write_resource:${resource}`;
    }
    return `write_resource:${resource || 'unsupported'}`;
  }

  return name;
}

export const ORCHESTRATOR_TOOL_DEFS: AiToolDefinition[] = [
  {
    name: 'list_targets',
    description: 'List available OxideTerm targets by view. Default view is connections for remote host discovery. Use view=all only for debugging or last-resort fallback.',
    parameters: {
      type: 'object',
      properties: {
        view: {
          type: 'string',
          enum: TARGET_VIEW_ENUM,
          description: 'Target view. Default: connections. Use connections for remote hosts; live_sessions for active shells/SFTP; app_surfaces for settings/UI; files for file-capable targets; all only for debug/fallback.',
        },
        query: { type: 'string', description: 'Optional filter text. Leave empty for broad discovery.' },
        kind: {
          type: 'string',
          enum: TARGET_KIND_ENUM,
          description: 'Optional legacy/fine-grained target kind filter. Prefer view for normal discovery.',
        },
      },
    },
  },
  {
    name: 'select_target',
    description: 'Select exactly one target from OxideTerm targets. Use only when the user named a specific target. Do not use for broad list/discovery requests.',
    parameters: {
      type: 'object',
      properties: {
        query: { type: 'string', description: 'Specific target name, host, user, session label, tab, or settings area.' },
        intent: {
          type: 'string',
          enum: TARGET_INTENT_ENUM,
          description: 'Required intended operation. Use knowledge for RAG/knowledge-base/runbook/documentation queries. This constrains the candidate pool so commands are not mistaken for targets.',
        },
        kind: {
          type: 'string',
          enum: TARGET_KIND_ENUM,
          description: 'Optional target kind filter.',
        },
      },
      required: ['query', 'intent'],
    },
  },
  {
    name: 'connect_target',
    description: 'Connect or open a selected target. For saved SSH connections, opens the saved connection through OxideTerm and returns live ssh-node and terminal-session targets.',
    parameters: {
      type: 'object',
      properties: {
        target_id: { type: 'string', description: 'Target ID from list_targets/select_target, usually saved-connection:*.' },
      },
      required: ['target_id'],
    },
  },
  {
    name: 'run_command',
    description: 'Run a command on an explicit target. Use ssh-node:* for direct remote execution, local-shell:default for local one-shot commands, or terminal-session:* when visible shell state matters.',
    parameters: {
      type: 'object',
      properties: {
        target_id: { type: 'string', description: 'Explicit target ID. Saved connections must be connected first.' },
        command: { type: 'string', description: 'Shell command to run.' },
        cwd: { type: 'string', description: 'Optional working directory.' },
        timeout_secs: { type: 'number', minimum: 1, maximum: 60, description: 'Timeout for direct/local command execution. Default: 30.' },
        await_output: { type: 'boolean', description: 'For terminal-session targets, wait for output. Default: true.' },
      },
      required: ['target_id', 'command'],
    },
  },
  {
    name: 'observe_terminal',
    description: 'Read a terminal target screen, buffer, readiness, and waiting-for-input hints. Use after run_command or before interactive input.',
    parameters: {
      type: 'object',
      properties: {
        target_id: { type: 'string', description: 'terminal-session:* target ID.' },
        max_chars: { type: 'number', minimum: 200, maximum: 12000, description: 'Maximum returned buffer characters. Default: 4000.' },
      },
      required: ['target_id'],
    },
  },
  {
    name: 'send_terminal_input',
    description: 'Send text, Enter, or control input to a visible terminal target. Read the terminal first when interacting with TUI or password prompts.',
    parameters: {
      type: 'object',
      properties: {
        target_id: { type: 'string', description: 'terminal-session:* target ID.' },
        text: { type: 'string', description: 'Text to send.' },
        append_enter: { type: 'boolean', description: 'Append Enter after text. Default: false.' },
        control: { type: 'string', enum: ['ctrl-c', 'ctrl-d', 'ctrl-z'], description: 'Optional control sequence.' },
      },
      required: ['target_id'],
    },
  },
  {
    name: 'read_resource',
    description: 'Read a resource from a target: settings section, remote file via agent/SFTP, SFTP directory, IDE file, or RAG search.',
    parameters: {
      type: 'object',
      properties: {
        target_id: { type: 'string', description: 'Target ID.' },
        resource: { type: 'string', enum: RESOURCE_KIND_ENUM, description: 'Resource kind.' },
        path: { type: 'string', description: 'File or directory path when applicable.' },
        section: { type: 'string', description: 'Settings section when resource=settings.' },
        query: { type: 'string', description: 'Search query for RAG or target-specific searches. For resource=rag, pass target_id="rag-index:default" plus query; path is not required.' },
      },
      required: ['target_id', 'resource'],
    },
  },
  {
    name: 'write_resource',
    description: 'Safely write a resource such as a settings value or remote file. For file edits, provide expected_hash or dry_run unless the user explicitly asked to overwrite.',
    parameters: {
      type: 'object',
      properties: {
        target_id: { type: 'string', description: 'Target ID.' },
        resource: { type: 'string', enum: RESOURCE_KIND_ENUM, description: 'Resource kind. Only settings and file are writable.' },
        section: { type: 'string', description: 'Settings section.' },
        key: { type: 'string', description: 'Settings key.' },
        value: { description: 'Settings value or structured resource value.' },
        path: { type: 'string', description: 'Remote file path.' },
        content: { type: 'string', description: 'File content.' },
        expected_hash: { type: 'string', description: 'Hash from prior read_resource result.' },
        dry_run: { type: 'boolean', description: 'Validate without writing.' },
      },
      required: ['target_id', 'resource'],
    },
  },
  {
    name: 'transfer_resource',
    description: 'Start an SFTP upload/download/transfer against an explicit SSH/SFTP target.',
    parameters: {
      type: 'object',
      properties: {
        target_id: { type: 'string', description: 'ssh-node:* or sftp-session:* target ID.' },
        direction: { type: 'string', enum: ['upload', 'download'], description: 'Transfer direction.' },
        source_path: { type: 'string', description: 'Local path for upload or remote path for download.' },
        destination_path: { type: 'string', description: 'Remote path for upload or local path for download.' },
      },
      required: ['target_id', 'direction', 'source_path', 'destination_path'],
    },
  },
  {
    name: 'open_app_surface',
    description: 'Open an OxideTerm app surface such as settings, connection manager, SFTP, IDE, file manager, or local terminal.',
    parameters: {
      type: 'object',
      properties: {
        surface: { type: 'string', enum: ['settings', 'connection_manager', 'connection_pool', 'connection_monitor', 'sftp', 'ide', 'file_manager', 'local_terminal', 'terminal'], description: 'Surface to open.' },
        target_id: { type: 'string', description: 'Optional target to open the surface for.' },
        section: { type: 'string', description: 'Optional settings section.' },
      },
      required: ['surface'],
    },
  },
  {
    name: 'get_state',
    description: 'Read compact state: connection status, transfer status, settings summary, active targets, or health. Use for diagnostics and verification.',
    parameters: {
      type: 'object',
      properties: {
        scope: { type: 'string', enum: ['connections', 'transfers', 'settings', 'targets', 'health', 'active'], description: 'State scope.' },
        target_id: { type: 'string', description: 'Optional target ID.' },
      },
      required: ['scope'],
    },
  },
  {
    name: 'remember_preference',
    description: 'Save a long-lived user preference for OxideSens memory. Do not use for transient task facts.',
    parameters: {
      type: 'object',
      properties: {
        preference: { type: 'string', description: 'Preference to remember.' },
      },
      required: ['preference'],
    },
  },
  {
    name: 'recall_preferences',
    description: 'Read saved long-lived OxideSens user preferences.',
    parameters: {
      type: 'object',
      properties: {},
    },
  },
];
