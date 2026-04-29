// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * # References Registry
 *
 * References let users attach specific context to their query
 * (e.g. `#buffer`, `#pane:2`, `#error`).
 *
 * Each reference has an async `resolve()` that fetches real context content.
 * Multiple references can be combined: `#buffer #error fix this`.
 */

import { api } from '@/lib/api';
import {
  getActiveTerminalBuffer,
  getActivePaneMetadata,
  getCombinedPaneContext,
  getActiveTerminalSelection,
} from '@/lib/terminalRegistry';
import { useAppStore } from '@/store/appStore';
import { ACTIVE_REFERENCE_TYPES } from './inputTokens';

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

export type ReferenceDef = {
  /** Reference type (used after #) */
  type: string;
  /** i18n key for display label */
  labelKey: string;
  /** i18n key for description */
  descriptionKey: string;
  /** Lucide icon name */
  icon: string;
  /** Whether this reference accepts a value (e.g. #pane:2) */
  acceptsValue?: boolean;
  /** Placeholder for the value portion */
  valuePlaceholder?: string;
  /** Async resolver that returns the context text */
  resolve: (value?: string) => Promise<string | null>;
};

// ═══════════════════════════════════════════════════════════════════════════
// Resolvers
// ═══════════════════════════════════════════════════════════════════════════

async function resolveBuffer(): Promise<string | null> {
  const activeTabId = useAppStore.getState().activeTabId;
  const tabs = useAppStore.getState().tabs;
  const activeTab = tabs.find(t => t.id === activeTabId);
  if (!activeTab) return null;

  // Try registry first (works for both SSH and local)
  const buffer = getActiveTerminalBuffer(activeTab.id);
  if (buffer) return buffer;

  // Fallback to backend API for SSH terminals
  const meta = getActivePaneMetadata();
  if (meta?.terminalType === 'terminal' && meta.sessionId) {
    const lines = await api.getScrollBuffer(meta.sessionId, 0, 100);
    if (lines.length > 0) return lines.map(l => l.text).join('\n');
  }

  return null;
}

async function resolveSelection(): Promise<string | null> {
  return getActiveTerminalSelection();
}

async function resolveError(): Promise<string | null> {
  // Get terminal buffer and extract the last error-like output
  const buffer = await resolveBuffer();
  if (!buffer) return null;

  const lines = buffer.split('\n');
  const errorPatterns = [
    /error/i, /failed/i, /fatal/i, /exception/i, /panic/i,
    /denied/i, /not found/i, /no such/i, /cannot/i, /unable/i,
    /segfault/i, /traceback/i, /command not found/i,
  ];

  // Find the last block of lines containing error patterns
  let lastErrorEnd = -1;
  for (let i = lines.length - 1; i >= 0; i--) {
    if (errorPatterns.some(p => p.test(lines[i]))) {
      lastErrorEnd = i;
      break;
    }
  }

  if (lastErrorEnd < 0) return null;

  // Include context lines before and after the error
  const start = Math.max(0, lastErrorEnd - 15);
  const end = Math.min(lines.length, lastErrorEnd + 5);
  return lines.slice(start, end).join('\n');
}

async function resolvePane(value?: string): Promise<string | null> {
  if (!value) return null;
  const paneIndex = parseInt(value, 10);
  if (isNaN(paneIndex)) return null;

  const activeTabId = useAppStore.getState().activeTabId;
  if (!activeTabId) return null;

  // Get combined context from all panes, then extract the requested one
  const combined = getCombinedPaneContext(activeTabId, 4000);
  if (!combined) return null;

  // The combined context has sections like "=== Pane 1 ===\n..."
  const sections = combined.split(/=== Pane \d+ ===/);
  if (paneIndex >= 1 && paneIndex < sections.length) {
    return sections[paneIndex].trim();
  }
  return null;
}

async function resolveCwd(): Promise<string | null> {
  // CWD detection requires running a command via the terminal.
  // For now, extract from the last prompt line in the terminal buffer.
  const buffer = await resolveBuffer();
  if (!buffer) return null;

  const lines = buffer.split('\n');
  // Try to find CWD from common prompt formats: user@host:~/path$, [user@host path]$
  for (let i = lines.length - 1; i >= Math.max(0, lines.length - 10); i--) {
    const cwdMatch = lines[i].match(/[:]\s*(~[^\s$#]*|\/[^\s$#]*)/);
    if (cwdMatch) return cwdMatch[1];
  }
  return null;
}

// ═══════════════════════════════════════════════════════════════════════════
// Registry
// ═══════════════════════════════════════════════════════════════════════════

export const REFERENCES: ReferenceDef[] = [
  {
    type: 'buffer',
    labelKey: 'ai.reference.buffer',
    descriptionKey: 'ai.reference.buffer_desc',
    icon: 'ScrollText',
    resolve: resolveBuffer,
  },
  {
    type: 'selection',
    labelKey: 'ai.reference.selection',
    descriptionKey: 'ai.reference.selection_desc',
    icon: 'TextSelect',
    resolve: resolveSelection,
  },
  {
    type: 'error',
    labelKey: 'ai.reference.error',
    descriptionKey: 'ai.reference.error_desc',
    icon: 'AlertTriangle',
    resolve: resolveError,
  },
  {
    type: 'pane',
    labelKey: 'ai.reference.pane',
    descriptionKey: 'ai.reference.pane_desc',
    icon: 'Columns',
    acceptsValue: true,
    valuePlaceholder: '1',
    resolve: resolvePane,
  },
  {
    type: 'cwd',
    labelKey: 'ai.reference.cwd',
    descriptionKey: 'ai.reference.cwd_desc',
    icon: 'FolderOpen',
    resolve: resolveCwd,
  },
];

if (import.meta.env.DEV) {
  const registryTypes = REFERENCES.map(r => r.type).join(',');
  const activeTypes = ACTIVE_REFERENCE_TYPES.join(',');
  if (registryTypes !== activeTypes) {
    console.warn(`[References] Registry/input token mismatch: ${registryTypes} !== ${activeTypes}`);
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// Lookup Helpers
// ═══════════════════════════════════════════════════════════════════════════

const referenceMap = new Map(REFERENCES.map(r => [r.type, r]));

/** Resolve a reference type to its definition. */
export function resolveReferenceType(type: string): ReferenceDef | undefined {
  return referenceMap.get(type);
}

/** Filter references by partial type for autocomplete. */
export function filterReferences(partial: string): ReferenceDef[] {
  const lower = partial.toLowerCase();
  return REFERENCES.filter(r => r.type.startsWith(lower));
}

/**
 * Resolve all references in a parsed input and return combined context text.
 * Each resolved reference is formatted as a labeled section.
 */
export async function resolveAllReferences(
  refs: Array<{ type: string; value?: string }>,
): Promise<string> {
  const parts: string[] = [];

  for (const ref of refs) {
    const def = referenceMap.get(ref.type);
    if (!def) continue;

    try {
      const content = await def.resolve(ref.value);
      if (content) {
        const label = ref.value ? `#${ref.type}:${ref.value}` : `#${ref.type}`;
        parts.push(`--- ${label} ---\n${content}`);
      }
    } catch (e) {
      console.warn(`[References] Failed to resolve #${ref.type}:`, e);
    }
  }

  return parts.join('\n\n');
}
