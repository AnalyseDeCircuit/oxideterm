// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Slash Commands Registry
 *
 * Defines all available /commands for the AI chat input.
 * Commands fall into two categories:
 *   - LLM commands: modify system prompt, then send to LLM
 *   - Client-only commands: handled entirely in frontend (e.g. /help, /clear)
 */

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export type SlashCommandCategory =
  | 'understanding'
  | 'troubleshooting'
  | 'meta';

export type SlashCommandDef = {
  /** Command name (without the leading /) */
  name: string;
  /** i18n key for the display label */
  labelKey: string;
  /** i18n key for the description shown in autocomplete */
  descriptionKey: string;
  /** Lucide icon name */
  icon: string;
  /** Category for grouping in autocomplete */
  category: SlashCommandCategory;
  /** Text appended to system prompt when this command is used */
  systemPromptModifier?: string;
  /** If true, handled entirely in frontend — never sent to LLM */
  clientOnly?: boolean;
};

// ═══════════════════════════════════════════════════════════════════════════
// Registry
// ═══════════════════════════════════════════════════════════════════════════

export const SLASH_COMMANDS: SlashCommandDef[] = [
  // ── Understanding ──
  {
    name: 'explain',
    labelKey: 'ai.slash.explain',
    descriptionKey: 'ai.slash.explain_desc',
    icon: 'BookOpen',
    category: 'understanding',
    systemPromptModifier:
      'The user wants an explanation. Be thorough and educational. Explain step-by-step what the command or output does, including any flags, options, or output fields. Provide examples where helpful.',
  },

  // ── Troubleshooting ──
  {
    name: 'fix',
    labelKey: 'ai.slash.fix',
    descriptionKey: 'ai.slash.fix_desc',
    icon: 'Wrench',
    category: 'troubleshooting',
    systemPromptModifier:
      'The user needs help fixing an error or problem. Diagnose the root cause step by step. Check the most common causes first. Use tools to gather diagnostic data when possible. Provide the exact fix with explanation.',
  },

  // ── Meta (client-only) ──
  {
    name: 'help',
    labelKey: 'ai.slash.help',
    descriptionKey: 'ai.slash.help_desc',
    icon: 'HelpCircle',
    category: 'meta',
    clientOnly: true,
  },
  {
    name: 'clear',
    labelKey: 'ai.slash.clear',
    descriptionKey: 'ai.slash.clear_desc',
    icon: 'Trash2',
    category: 'meta',
    clientOnly: true,
  },
  {
    name: 'compact',
    labelKey: 'ai.slash.compact',
    descriptionKey: 'ai.slash.compact_desc',
    icon: 'Archive',
    category: 'meta',
    clientOnly: true,
  },
];

// ═══════════════════════════════════════════════════════════════════════════
// Lookup Helpers
// ═══════════════════════════════════════════════════════════════════════════

const commandMap = new Map(SLASH_COMMANDS.map(c => [c.name, c]));

/** Resolve a command name to its definition. Returns undefined for unknown commands. */
export function resolveSlashCommand(name: string): SlashCommandDef | undefined {
  return commandMap.get(name);
}

/** Filter commands by partial name for autocomplete. */
export function filterSlashCommands(partial: string): SlashCommandDef[] {
  const lower = partial.toLowerCase();
  return SLASH_COMMANDS.filter(c => c.name.startsWith(lower));
}

/** Group commands by category for autocomplete display. */
export function groupSlashCommandsByCategory(): Map<SlashCommandCategory, SlashCommandDef[]> {
  const groups = new Map<SlashCommandCategory, SlashCommandDef[]>();
  for (const cmd of SLASH_COMMANDS) {
    const list = groups.get(cmd.category) || [];
    list.push(cmd);
    groups.set(cmd.category, list);
  }
  return groups;
}
