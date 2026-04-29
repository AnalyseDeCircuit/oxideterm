// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Unified Input Parser
 *
 * Extracts /commands, @participants, and #references from user input.
 * Known tokens are optional, composable, and stripped from the clean text
 * that gets sent to the LLM as the user message content. Unknown @/# tokens
 * are preserved as normal text so user input is not silently swallowed.
 */

import {
  ACTIVE_PARTICIPANT_NAME_SET,
  ACTIVE_REFERENCE_TYPE_SET,
} from './inputTokens';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export type SlashCommandMatch = {
  name: string;
  raw: string;
};

export type ParticipantMatch = {
  name: string;
  raw: string;
};

export type ReferenceMatch = {
  type: string;
  value?: string;
  raw: string;
};

export type ParsedInput = {
  /** Matched slash command (must be at start of input), or null */
  slashCommand: SlashCommandMatch | null;
  /** All @participant mentions found */
  participants: ParticipantMatch[];
  /** All #reference tokens found */
  references: ReferenceMatch[];
  /** User text with all syntax tokens removed */
  cleanText: string;
  /** Original unmodified input */
  rawText: string;
};

// ═══════════════════════════════════════════════════════════════════════════
// Patterns
// ═══════════════════════════════════════════════════════════════════════════

/** Slash command: must be the very first token, e.g. `/explain some text` */
const SLASH_RE = /^\/([a-z_]+)\s*/;

/** @participant anywhere in text, e.g. `@terminal` `@sftp` */
const PARTICIPANT_RE = /@([a-z_]+)/g;

/** #reference anywhere in text, e.g. `#buffer` `#pane:2` */
const REFERENCE_RE = /#([a-z_]+)(?::(\S+))?/g;

// ═══════════════════════════════════════════════════════════════════════════
// Parser
// ═══════════════════════════════════════════════════════════════════════════

export function parseUserInput(raw: string): ParsedInput {
  let text = raw;

  // 1. Extract slash command (first token only)
  let slashCommand: SlashCommandMatch | null = null;
  const slashMatch = SLASH_RE.exec(text);
  if (slashMatch) {
    slashCommand = { name: slashMatch[1], raw: slashMatch[0] };
    text = text.slice(slashMatch[0].length);
  }

  // 2. Extract @participants
  const participants: ParticipantMatch[] = [];
  const seenParticipants = new Set<string>();
  let participantMatch: RegExpExecArray | null;
  // Reset lastIndex for global regex
  PARTICIPANT_RE.lastIndex = 0;
  while ((participantMatch = PARTICIPANT_RE.exec(text)) !== null) {
    const name = participantMatch[1];
    if (ACTIVE_PARTICIPANT_NAME_SET.has(name) && !seenParticipants.has(name)) {
      seenParticipants.add(name);
      participants.push({ name, raw: participantMatch[0] });
    }
  }

  // 3. Extract #references
  const references: ReferenceMatch[] = [];
  let refMatch: RegExpExecArray | null;
  REFERENCE_RE.lastIndex = 0;
  while ((refMatch = REFERENCE_RE.exec(text)) !== null) {
    const type = refMatch[1];
    if (ACTIVE_REFERENCE_TYPE_SET.has(type)) {
      references.push({
        type,
        value: refMatch[2] || undefined,
        raw: refMatch[0],
      });
    }
  }

  // 4. Build clean text: strip known @mentions and #references only.
  let cleanText = text;
  // Remove @participant tokens
  for (const p of participants) {
    cleanText = cleanText.replace(p.raw, '');
  }
  // Remove #reference tokens
  for (const r of references) {
    cleanText = cleanText.replace(r.raw, '');
  }
  // Collapse multiple spaces and trim
  cleanText = cleanText.replace(/\s{2,}/g, ' ').trim();

  return {
    slashCommand,
    participants,
    references,
    cleanText,
    rawText: raw,
  };
}

// ═══════════════════════════════════════════════════════════════════════════
// Autocomplete helpers
// ═══════════════════════════════════════════════════════════════════════════

/** Returns the partial token at cursor for autocomplete trigger detection. */
export function getTokenAtCursor(text: string, cursorPos: number): { type: 'slash' | 'participant' | 'reference' | null; partial: string; start: number } {
  // Walk backwards from cursor to find the token start
  let i = cursorPos - 1;
  while (i >= 0 && /\S/.test(text[i])) i--;
  const tokenStart = i + 1;
  const token = text.slice(tokenStart, cursorPos);

  if (token.startsWith('/') && tokenStart === 0) {
    return { type: 'slash', partial: token.slice(1), start: tokenStart };
  }
  if (token.startsWith('@')) {
    return { type: 'participant', partial: token.slice(1), start: tokenStart };
  }
  if (token.startsWith('#')) {
    return { type: 'reference', partial: token.slice(1), start: tokenStart };
  }
  return { type: null, partial: '', start: tokenStart };
}
