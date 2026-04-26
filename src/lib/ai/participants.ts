// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * @ Participants Registry
 *
 * Participants let users explicitly route their query to a specific domain
 * (e.g. `@sftp list /var/www`) regardless of which tab is currently active.
 *
 * When a participant is used:
 *   1. Its tool subset overrides the tab-based tool selection
 *   2. Its context injection runs automatically
 *   3. Its system prompt modifier is appended
 */

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export type ParticipantDef = {
  /** Unique name (used after @) */
  name: string;
  /** i18n key for display label */
  labelKey: string;
  /** i18n key for description */
  descriptionKey: string;
  /** Lucide icon name */
  icon: string;
  /** Tool names that should be included (additive to global set).
   *  These are the tab-specific tools that would normally only appear
   *  when the corresponding tab is active. */
  includeTools: string[];
  /** Text appended to system prompt */
  systemPromptModifier: string;
  /** Auto-inject this context type */
  autoContext?: 'terminal' | 'sftp' | 'ide' | 'local';
};

// ═══════════════════════════════════════════════════════════════════════════
// Registry
// ═══════════════════════════════════════════════════════════════════════════

export const PARTICIPANTS: ParticipantDef[] = [
  {
    name: 'terminal',
    labelKey: 'ai.participant.terminal',
    descriptionKey: 'ai.participant.terminal_desc',
    icon: 'Terminal',
    includeTools: [
      // Session tools are global, but emphasise terminal context
      'list_targets', 'list_capabilities', 'list_sessions', 'list_tabs',
      'terminal_exec', 'get_terminal_buffer', 'search_terminal',
      'await_terminal_output', 'send_control_sequence', 'batch_exec',
      'read_screen', 'send_keys', 'send_mouse',
    ],
    systemPromptModifier:
      'The user is asking about their SSH remote terminal session. Focus on terminal operations, shell commands, and remote system administration.',
    autoContext: 'terminal',
  },
  {
    name: 'sftp',
    labelKey: 'ai.participant.sftp',
    descriptionKey: 'ai.participant.sftp_desc',
    icon: 'FolderOpen',
    includeTools: [
      'list_targets', 'list_capabilities',
      'sftp_list_dir', 'sftp_read_file', 'sftp_stat', 'sftp_get_cwd',
    ],
    systemPromptModifier:
      'The user is asking about remote file management via SFTP. Focus on file operations, directory navigation, file content inspection, and file transfers.',
    autoContext: 'sftp',
  },
  {
    name: 'ide',
    labelKey: 'ai.participant.ide',
    descriptionKey: 'ai.participant.ide_desc',
    icon: 'Code2',
    includeTools: [
      'list_targets', 'list_capabilities',
      'ide_get_open_files', 'ide_get_file_content', 'ide_get_project_info', 'ide_apply_edit',
    ],
    systemPromptModifier:
      'The user is asking about code editing in IDE mode. Focus on the project structure, code analysis, editing operations, and programming assistance.',
    autoContext: 'ide',
  },
  {
    name: 'connection',
    labelKey: 'ai.participant.connection',
    descriptionKey: 'ai.participant.connection_desc',
    icon: 'Link',
    includeTools: [
      'list_targets', 'list_capabilities', 'list_sessions', 'list_tabs',
      'list_saved_connections', 'search_saved_connections', 'get_session_tree',
      'connect_saved_session', 'connect_saved_connection_by_query',
      'get_pool_stats', 'set_pool_config',
      'list_connections', 'get_connection_health',
      'list_port_forwards', 'get_detected_ports', 'create_port_forward', 'stop_port_forward',
    ],
    systemPromptModifier:
      'The user is asking about SSH connections, session management, or port forwarding. Focus on connection status, configuration, and network infrastructure.',
  },
  {
    name: 'system',
    labelKey: 'ai.participant.system',
    descriptionKey: 'ai.participant.system_desc',
    icon: 'Activity',
    includeTools: [
      'list_targets', 'list_capabilities',
      'get_all_health', 'get_resource_metrics',
      'get_event_log', 'get_transfer_status', 'get_recording_status',
      'get_broadcast_status', 'get_app_status', 'get_app_stats',
    ],
    systemPromptModifier:
      'The user is asking about system monitoring, health checks, or resource usage. Focus on diagnostics, performance metrics, and operational status.',
  },
  {
    name: 'local',
    labelKey: 'ai.participant.local',
    descriptionKey: 'ai.participant.local_desc',
    icon: 'Monitor',
    includeTools: [
      'list_targets', 'list_capabilities', 'list_sessions', 'list_tabs',
      'local_list_shells', 'local_get_terminal_info', 'local_exec', 'local_get_drives',
    ],
    systemPromptModifier:
      'The user is asking about their local terminal (not SSH). Focus on local shell operations, local filesystem, and local system administration.',
    autoContext: 'local',
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Lookup Helpers
// ═══════════════════════════════════════════════════════════════════════════

const participantMap = new Map(PARTICIPANTS.map(p => [p.name, p]));

/** Resolve a participant name to its definition. */
export function resolveParticipant(name: string): ParticipantDef | undefined {
  return participantMap.get(name);
}

/** Filter participants by partial name for autocomplete. */
export function filterParticipants(partial: string): ParticipantDef[] {
  const lower = partial.toLowerCase();
  return PARTICIPANTS.filter(p => p.name.startsWith(lower));
}

/**
 * Merge tool include-lists from multiple participants into a single set.
 * Used when user specifies multiple participants (e.g. `@terminal @sftp`).
 */
export function mergeParticipantTools(names: string[]): Set<string> {
  const tools = new Set<string>();
  for (const name of names) {
    const p = participantMap.get(name);
    if (p) {
      for (const tool of p.includeTools) {
        tools.add(tool);
      }
    }
  }
  return tools;
}
