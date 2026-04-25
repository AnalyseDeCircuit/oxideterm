// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

import type { ToolCapability, ToolRisk } from './types';
import { hasDeniedCommands } from '../toolDefinitions';

const SENSITIVE_KEY_RE = /(?:password|passphrase|token|secret|api[_-]?key|private[_-]?key|access[_-]?key|auth|credential)/i;
const SENSITIVE_VALUE_RE = /(?:password|passphrase|token|secret|api[_-]?key|private key|bearer\s+[A-Za-z0-9._~+/-]+|sk-[A-Za-z0-9_-]{8,})/i;
const HIGH_RISK_TOOL_RISKS = new Set<ToolRisk>([
  'destructive',
  'credential-sensitive',
  'network-expose',
  'settings-change',
]);

const READ_TOOLS = new Set([
  'get_terminal_buffer',
  'search_terminal',
  'await_terminal_output',
  'read_screen',
  'read_file',
  'list_directory',
  'grep_search',
  'git_status',
  'sftp_list_dir',
  'sftp_read_file',
  'sftp_stat',
  'sftp_get_cwd',
  'ide_get_open_files',
  'ide_get_file_content',
  'ide_get_project_info',
  'list_tabs',
  'list_sessions',
  'list_targets',
  'list_capabilities',
  'list_connections',
  'get_connection_health',
  'get_detected_ports',
  'list_port_forwards',
  'get_settings',
  'get_pool_stats',
  'get_all_health',
  'get_resource_metrics',
  'list_saved_connections',
  'search_saved_connections',
  'get_session_tree',
  'list_plugins',
  'search_docs',
]);

const WRITE_FILE_TOOLS = new Set([
  'write_file',
  'sftp_write_file',
  'ide_replace_string',
  'ide_insert_text',
  'ide_create_file',
]);

const INTERACTIVE_INPUT_TOOLS = new Set([
  'send_keys',
  'send_mouse',
  'send_control_sequence',
  'batch_exec',
]);

const NAVIGATION_TOOLS = new Set([
  'open_tab',
  'open_session_tab',
  'open_local_terminal',
  'connect_saved_session',
]);

function capabilityRisk(capability?: ToolCapability): ToolRisk | undefined {
  switch (capability) {
    case 'filesystem.read':
    case 'filesystem.search':
    case 'terminal.observe':
    case 'terminal.wait':
    case 'state.list':
    case 'settings.read':
      return 'read';
    case 'filesystem.write':
      return 'write-file';
    case 'command.run':
      return 'execute-command';
    case 'terminal.send':
      return 'interactive-input';
    case 'network.forward':
      return 'network-expose';
    case 'settings.write':
      return 'settings-change';
    case 'navigation.open':
    case 'plugin.invoke':
    case 'mcp.invoke':
      return undefined;
    default:
      return undefined;
  }
}

function valueContainsCredential(value: unknown, keyHint = ''): boolean {
  if (keyHint && keyHint !== 'keys' && SENSITIVE_KEY_RE.test(keyHint)) {
    return true;
  }

  if (typeof value === 'string') {
    return SENSITIVE_VALUE_RE.test(value);
  }

  if (Array.isArray(value)) {
    return value.some((item) => valueContainsCredential(item));
  }

  if (value && typeof value === 'object') {
    return Object.entries(value as Record<string, unknown>).some(([key, nested]) => (
      valueContainsCredential(nested, key)
    ));
  }

  return false;
}

export function inferToolRisk(
  toolName: string,
  args: Record<string, unknown> = {},
  capability?: ToolCapability,
): ToolRisk {
  if (hasDeniedCommands(toolName, args)) {
    return 'destructive';
  }

  if (valueContainsCredential(args)) {
    return 'credential-sensitive';
  }

  const fromCapability = capabilityRisk(capability);
  if (fromCapability) {
    return fromCapability;
  }

  if (READ_TOOLS.has(toolName)) {
    return 'read';
  }
  if (WRITE_FILE_TOOLS.has(toolName)) {
    return 'write-file';
  }
  if (INTERACTIVE_INPUT_TOOLS.has(toolName)) {
    return 'interactive-input';
  }
  if (toolName === 'terminal_exec' || toolName === 'local_exec') {
    return typeof args.session_id === 'string' ? 'interactive-input' : 'execute-command';
  }
  if (toolName === 'create_port_forward' || toolName === 'stop_port_forward') {
    return 'network-expose';
  }
  if (toolName === 'update_setting' || toolName === 'set_pool_config') {
    return 'settings-change';
  }
  if (NAVIGATION_TOOLS.has(toolName)) {
    return 'read';
  }
  if (toolName.startsWith('mcp::')) {
    return 'read';
  }

  return 'read';
}

export function isHighRiskToolRisk(risk: ToolRisk): boolean {
  return HIGH_RISK_TOOL_RISKS.has(risk);
}

export function sanitizeToolArguments<T = unknown>(value: T): T {
  const sanitize = (input: unknown, keyHint = ''): unknown => {
    if (keyHint && keyHint !== 'keys' && SENSITIVE_KEY_RE.test(keyHint)) {
      return '[redacted]';
    }

    if (typeof input === 'string') {
      return SENSITIVE_VALUE_RE.test(input) ? '[redacted]' : input;
    }

    if (Array.isArray(input)) {
      return input.map((item) => sanitize(item));
    }

    if (input && typeof input === 'object') {
      return Object.fromEntries(
        Object.entries(input as Record<string, unknown>).map(([key, nested]) => [key, sanitize(nested, key)]),
      );
    }

    return input;
  };

  return sanitize(value) as T;
}
