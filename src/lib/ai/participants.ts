// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * @ Participants Registry
 *
 * Participants let users explicitly route their query to a specific domain
 * (e.g. `@sftp list /var/www`) regardless of which tab is currently active.
 *
 * When a participant is used:
 *   1. Its domain hint is appended to the system prompt
 *   2. Its preferred target intent/view guides the orchestrator
 */

import type { AiTargetIntent, AiTargetView } from './orchestrator/types';
import { ACTIVE_PARTICIPANT_NAMES } from './inputTokens';

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
  /** Text appended to system prompt */
  systemPromptModifier: string;
  /** Target intent hint for the orchestrator. */
  intentHint?: AiTargetIntent;
  /** Preferred target discovery view for this participant. */
  preferredTargetView?: AiTargetView;
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
    systemPromptModifier:
      'The user explicitly selected the terminal domain. Prefer terminal/session targets and use terminal-oriented actions when tool use is needed.',
    intentHint: 'terminal',
    preferredTargetView: 'live_sessions',
  },
  {
    name: 'sftp',
    labelKey: 'ai.participant.sftp',
    descriptionKey: 'ai.participant.sftp_desc',
    icon: 'FolderOpen',
    systemPromptModifier:
      'The user explicitly selected the SFTP domain. Prefer SFTP or file-capable remote targets and use file transfer/resource actions when tool use is needed.',
    intentHint: 'sftp',
    preferredTargetView: 'files',
  },
  {
    name: 'ide',
    labelKey: 'ai.participant.ide',
    descriptionKey: 'ai.participant.ide_desc',
    icon: 'Code2',
    systemPromptModifier:
      'The user explicitly selected the IDE domain. Prefer IDE workspace/file targets and use resource actions for code reading or editing.',
    intentHint: 'file',
    preferredTargetView: 'files',
  },
  {
    name: 'local',
    labelKey: 'ai.participant.local',
    descriptionKey: 'ai.participant.local_desc',
    icon: 'Monitor',
    systemPromptModifier:
      'The user explicitly selected the local domain. Prefer local-shell targets and avoid assuming a remote SSH target unless the user names one.',
    intentHint: 'local',
    preferredTargetView: 'app_surfaces',
  },
  {
    name: 'settings',
    labelKey: 'ai.participant.settings',
    descriptionKey: 'ai.participant.settings_desc',
    icon: 'Settings',
    systemPromptModifier:
      'The user explicitly selected the settings domain. Prefer settings targets and use settings read/write actions rather than depending on the current settings tab.',
    intentHint: 'settings',
    preferredTargetView: 'app_surfaces',
  },
  {
    name: 'knowledge',
    labelKey: 'ai.participant.knowledge',
    descriptionKey: 'ai.participant.knowledge_desc',
    icon: 'Library',
    systemPromptModifier:
      'The user explicitly selected the knowledge base domain. Prefer the rag-index target and use read_resource with resource="rag" for documentation, runbook, SOP, or knowledge queries.',
    intentHint: 'knowledge',
    preferredTargetView: 'files',
  },
];

if (import.meta.env.DEV) {
  const registryNames = PARTICIPANTS.map(p => p.name).join(',');
  const activeNames = ACTIVE_PARTICIPANT_NAMES.join(',');
  if (registryNames !== activeNames) {
    console.warn(`[Participants] Registry/input token mismatch: ${registryNames} !== ${activeNames}`);
  }
}

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
