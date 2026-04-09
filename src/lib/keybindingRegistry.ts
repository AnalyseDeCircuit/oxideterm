// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Keybinding Registry — single source of truth for all keyboard shortcuts.
 *
 * Every built-in shortcut is declared here with its default key combo.
 * User overrides are stored separately in keybindingStore (diff-based).
 * At runtime the registry merges defaults + overrides and exposes:
 *   - getBinding(actionId)  → current effective KeyCombo
 *   - matchAction(event)    → ActionId | null
 *   - getDefaults()         → full default table
 *   - findConflicts(combo, scope) → ActionId[]
 */

import { platform } from '@/lib/platform';

// ─── Action IDs ──────────────────────────────────────────────────────

export type ActionScope = 'global' | 'terminal' | 'split' | 'palette';

export type ActionId =
  // Global
  | 'app.newTerminal'
  | 'app.shellLauncher'
  | 'app.closeTab'
  | 'app.closeOtherTabs'
  | 'app.newConnection'
  | 'app.settings'
  | 'app.toggleSidebar'
  | 'app.commandPalette'
  | 'app.zenMode'
  | 'app.nextTab'
  | 'app.prevTab'
  | 'app.navBack'
  | 'app.navForward'
  | 'app.goToTab1'
  | 'app.goToTab2'
  | 'app.goToTab3'
  | 'app.goToTab4'
  | 'app.goToTab5'
  | 'app.goToTab6'
  | 'app.goToTab7'
  | 'app.goToTab8'
  | 'app.goToTab9'
  | 'app.fontIncrease'
  | 'app.fontDecrease'
  | 'app.fontReset'
  | 'app.showShortcuts'
  // Terminal
  | 'terminal.search'
  | 'terminal.paste'
  | 'terminal.aiPanel'
  | 'terminal.recording'
  | 'terminal.closePanel'
  // Split
  | 'split.horizontal'
  | 'split.vertical'
  | 'split.closePane'
  | 'split.navLeft'
  | 'split.navRight'
  // Palette
  | 'palette.eventLog'
  | 'palette.aiSidebar'
  | 'palette.broadcast';

// ─── Key Combo ───────────────────────────────────────────────────────

/**
 * Normalised key combo representation.
 * Modifier flags + key name (lowercase, e.g. "t", "f", "arrowleft", "\\", "/").
 *
 * For display we convert to human-readable strings ("⌘T", "Ctrl+Shift+F").
 */
export type KeyCombo = {
  key: string;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  meta: boolean;
};

// ─── Action Definition ───────────────────────────────────────────────

export type ActionDefinition = {
  id: ActionId;
  scope: ActionScope;
  /** Default key combo on macOS */
  mac: KeyCombo;
  /** Default key combo on Windows / Linux */
  other: KeyCombo;
  /**
   * Whether the shortcut should fire even when a terminal is focused.
   * - 'always': fires regardless
   * - 'when-panel-open': only when a UI panel (search/ai) is open
   * - 'never': let the key pass through to the terminal
   */
  terminalBehavior: 'always' | 'when-panel-open' | 'never';
};

// ─── Helpers ─────────────────────────────────────────────────────────

function k(key: string, mods: { ctrl?: boolean; shift?: boolean; alt?: boolean; meta?: boolean } = {}): KeyCombo {
  return {
    key: key.toLowerCase(),
    ctrl: mods.ctrl ?? false,
    shift: mods.shift ?? false,
    alt: mods.alt ?? false,
    meta: mods.meta ?? false,
  };
}

/** macOS Cmd shortcut */
function cmd(key: string, shift = false): KeyCombo {
  return k(key, { meta: true, shift });
}

/** Windows/Linux Ctrl shortcut */
function ctrl(key: string, shift = false): KeyCombo {
  return k(key, { ctrl: true, shift });
}

function ctrlAlt(key: string): KeyCombo {
  return k(key, { ctrl: true, alt: true });
}

function cmdAlt(key: string): KeyCombo {
  return k(key, { meta: true, alt: true });
}

function isPrintableSymbolKey(key: string): boolean {
  return key.length === 1 && !/^[a-z0-9 ]$/i.test(key);
}

function isLayoutAltSymbolKey(key: string): boolean {
  return ['[', ']', '{', '}', '\\', '|', '@', '#', '~'].includes(key);
}

/**
 * Normalize a combo for cross-layout storage and comparison.
 *
 * - Printable symbol keys already encode their shifted glyph in `key`, so the
 *   `shift` flag is redundant and removed.
 * - On many international layouts, symbols like `[`, `]`, `\\`, `{`, `}` are
 *   produced with Option/AltGr while holding Cmd/Ctrl. We normalize away that
 *   layout-generated `alt` modifier so stored shortcuts remain stable across
 *   DE/FR/UK-style layouts.
 */
export function normalizeKeyCombo(combo: KeyCombo): KeyCombo {
  if (!isPrintableSymbolKey(combo.key)) {
    return combo;
  }

  const normalized: KeyCombo = {
    ...combo,
    shift: false,
  };

  if ((combo.ctrl || combo.meta) && isLayoutAltSymbolKey(combo.key)) {
    normalized.alt = false;
  }

  return normalized;
}

// ─── Default Bindings ────────────────────────────────────────────────

const defaults: ActionDefinition[] = [
  // ── Global ──
  { id: 'app.newTerminal',     scope: 'global', mac: cmd('t'),         other: ctrl('t'),               terminalBehavior: 'always' },
  { id: 'app.shellLauncher',   scope: 'global', mac: cmd('t', true),   other: ctrl('t', true),         terminalBehavior: 'always' },
  { id: 'app.closeTab',        scope: 'global', mac: cmd('w'),         other: ctrl('w'),               terminalBehavior: 'never' },
  { id: 'app.closeOtherTabs',  scope: 'global', mac: cmd('w', true),   other: ctrl('w', true),         terminalBehavior: 'always' },
  { id: 'app.newConnection',   scope: 'global', mac: cmd('n'),         other: ctrl('n'),               terminalBehavior: 'always' },
  { id: 'app.settings',        scope: 'global', mac: cmd(','),         other: ctrl(','),               terminalBehavior: 'always' },
  { id: 'app.toggleSidebar',   scope: 'global', mac: cmd('\\'),        other: ctrl('\\'),              terminalBehavior: 'always' },
  { id: 'app.commandPalette',  scope: 'global', mac: cmd('k'),         other: ctrl('k'),               terminalBehavior: 'always' },
  { id: 'app.zenMode',         scope: 'global', mac: cmd('z', true),   other: ctrl('z', true),         terminalBehavior: 'always' },
  { id: 'app.nextTab',         scope: 'global', mac: k('}', { meta: true }), other: k('tab', { ctrl: true }), terminalBehavior: 'always' },
  { id: 'app.prevTab',         scope: 'global', mac: k('{', { meta: true }), other: k('tab', { ctrl: true, shift: true }), terminalBehavior: 'always' },
  { id: 'app.navBack',         scope: 'global', mac: k('[', { meta: true }), other: k('arrowleft', { alt: true }), terminalBehavior: 'always' },
  { id: 'app.navForward',      scope: 'global', mac: k(']', { meta: true }), other: k('arrowright', { alt: true }), terminalBehavior: 'always' },
  { id: 'app.goToTab1',        scope: 'global', mac: cmd('1'),         other: ctrl('1'),               terminalBehavior: 'always' },
  { id: 'app.goToTab2',        scope: 'global', mac: cmd('2'),         other: ctrl('2'),               terminalBehavior: 'always' },
  { id: 'app.goToTab3',        scope: 'global', mac: cmd('3'),         other: ctrl('3'),               terminalBehavior: 'always' },
  { id: 'app.goToTab4',        scope: 'global', mac: cmd('4'),         other: ctrl('4'),               terminalBehavior: 'always' },
  { id: 'app.goToTab5',        scope: 'global', mac: cmd('5'),         other: ctrl('5'),               terminalBehavior: 'always' },
  { id: 'app.goToTab6',        scope: 'global', mac: cmd('6'),         other: ctrl('6'),               terminalBehavior: 'always' },
  { id: 'app.goToTab7',        scope: 'global', mac: cmd('7'),         other: ctrl('7'),               terminalBehavior: 'always' },
  { id: 'app.goToTab8',        scope: 'global', mac: cmd('8'),         other: ctrl('8'),               terminalBehavior: 'always' },
  { id: 'app.goToTab9',        scope: 'global', mac: cmd('9'),         other: ctrl('9'),               terminalBehavior: 'always' },
  { id: 'app.fontIncrease',    scope: 'global', mac: k('=', { meta: true }),  other: k('=', { ctrl: true }),  terminalBehavior: 'always' },
  { id: 'app.fontDecrease',    scope: 'global', mac: k('-', { meta: true }),  other: k('-', { ctrl: true }),  terminalBehavior: 'always' },
  { id: 'app.fontReset',       scope: 'global', mac: cmd('0'),         other: ctrl('0'),               terminalBehavior: 'always' },
  { id: 'app.showShortcuts',   scope: 'global', mac: cmd('/'),         other: ctrl('/'),               terminalBehavior: 'always' },

  // ── Terminal ──
  { id: 'terminal.search',     scope: 'terminal', mac: cmd('f'),         other: ctrl('f', true),         terminalBehavior: 'always' },
  { id: 'terminal.paste',      scope: 'terminal', mac: cmd('v'),         other: ctrl('v', true),         terminalBehavior: 'always' },
  { id: 'terminal.aiPanel',    scope: 'terminal', mac: cmd('i'),         other: ctrl('i', true),         terminalBehavior: 'always' },
  { id: 'terminal.recording',  scope: 'terminal', mac: cmd('r', true),   other: ctrl('r', true),         terminalBehavior: 'always' },
  { id: 'terminal.closePanel', scope: 'terminal', mac: k('escape'),      other: k('escape'),             terminalBehavior: 'when-panel-open' },

  // ── Split ──
  { id: 'split.horizontal',    scope: 'split', mac: cmd('e', true),    other: ctrl('e', true),         terminalBehavior: 'always' },
  { id: 'split.vertical',      scope: 'split', mac: cmd('d', true),    other: ctrl('d', true),         terminalBehavior: 'always' },
  { id: 'split.closePane',     scope: 'split', mac: cmd('w', true),    other: ctrl('w', true),         terminalBehavior: 'always' },
  { id: 'split.navLeft',       scope: 'split', mac: cmdAlt('arrowleft'),  other: ctrlAlt('arrowleft'),  terminalBehavior: 'always' },
  { id: 'split.navRight',      scope: 'split', mac: cmdAlt('arrowright'), other: ctrlAlt('arrowright'), terminalBehavior: 'always' },

  // ── Palette ──
  { id: 'palette.eventLog',    scope: 'palette', mac: cmd('j'),         other: ctrl('j'),               terminalBehavior: 'always' },
  { id: 'palette.aiSidebar',   scope: 'palette', mac: cmd('a', true),   other: ctrl('a', true),         terminalBehavior: 'always' },
  { id: 'palette.broadcast',   scope: 'palette', mac: cmd('b'),         other: ctrl('b'),               terminalBehavior: 'always' },
];

// ─── Registry API ────────────────────────────────────────────────────

const defaultMap = new Map<ActionId, ActionDefinition>();
for (const d of defaults) {
  defaultMap.set(d.id, d);
}

/** User overrides: actionId → { mac?, other? } partial KeyCombo replacements */
let overrides = new Map<ActionId, { mac?: KeyCombo; other?: KeyCombo }>();

/**
 * Replace all user overrides (called by keybindingStore on init / update).
 * Creates a defensive copy to prevent external mutation.
 */
export function setOverrides(o: Map<ActionId, { mac?: KeyCombo; other?: KeyCombo }>): void {
  overrides = new Map(o);
}

/**
 * Get the full list of default action definitions.
 */
export function getDefaults(): readonly ActionDefinition[] {
  return defaults;
}

/**
 * Get the default definition for an action.
 */
export function getDefaultDefinition(id: ActionId): ActionDefinition | undefined {
  return defaultMap.get(id);
}

/**
 * Get the effective (default + override) key combo for an action on the current platform.
 */
export function getBinding(id: ActionId): KeyCombo | undefined {
  const def = defaultMap.get(id);
  if (!def) return undefined;
  const side = platform.isMac ? 'mac' : 'other';
  const override = overrides.get(id);
  return override?.[side] ?? def[side];
}

/**
 * Get the effective key combo for both platforms.
 */
export function getBindingBoth(id: ActionId): { mac: KeyCombo; other: KeyCombo } | undefined {
  const def = defaultMap.get(id);
  if (!def) return undefined;
  const override = overrides.get(id);
  return {
    mac: override?.mac ?? def.mac,
    other: override?.other ?? def.other,
  };
}

/**
 * Check if a KeyboardEvent matches a KeyCombo.
 *
 * For single-character non-alphanumeric keys (symbols like `}`, `{`, `+`),
 * the shift check is skipped because the shift state is already encoded in
 * the character itself (e.g. pressing Shift+] produces `}` in the event).
 */
export function eventMatchesCombo(e: KeyboardEvent, combo: KeyCombo): boolean {
  const normalized = normalizeKeyCombo(combo);

  if (e.key.toLowerCase() !== normalized.key) return false;
  if (e.ctrlKey !== normalized.ctrl) return false;
  // Skip shift check for symbol keys — their shift state is implicit in the character
  const isSymbol = isPrintableSymbolKey(normalized.key);
  if (!isSymbol) {
    if (e.shiftKey !== normalized.shift) return false;
  }

  const allowLayoutAlt =
    isLayoutAltSymbolKey(normalized.key) &&
    (normalized.ctrl || normalized.meta) &&
    !normalized.alt;

  if (!allowLayoutAlt && e.altKey !== normalized.alt) return false;
  if (e.metaKey !== normalized.meta) return false;
  return true;
}

/**
 * Given a KeyboardEvent, find the matching ActionId (if any) on the current platform.
 * Checks against effective (default + override) bindings.
 */
export function matchAction(e: KeyboardEvent, scopeFilter?: ActionScope | ActionScope[]): ActionId | null {
  const scopes = scopeFilter
    ? (Array.isArray(scopeFilter) ? scopeFilter : [scopeFilter])
    : undefined;

  for (const def of defaults) {
    if (scopes && !scopes.includes(def.scope)) continue;

    const combo = getBinding(def.id);
    if (combo && eventMatchesCombo(e, combo)) {
      return def.id;
    }
  }
  return null;
}

/**
 * Find actions whose current binding conflicts with a proposed combo in the same scope.
 */
export function findConflicts(combo: KeyCombo, scope: ActionScope, excludeAction?: ActionId): ActionId[] {
  const conflicts: ActionId[] = [];
  const normalizedCandidate = normalizeKeyCombo(combo);
  for (const def of defaults) {
    if (def.scope !== scope) continue;
    if (excludeAction && def.id === excludeAction) continue;

    const current = getBinding(def.id);
    if (current && combosEqual(normalizeKeyCombo(current), normalizedCandidate)) {
      conflicts.push(def.id);
    }
  }
  return conflicts;
}

/**
 * Compare two KeyCombos for equality.
 */
export function combosEqual(a: KeyCombo, b: KeyCombo): boolean {
  const left = normalizeKeyCombo(a);
  const right = normalizeKeyCombo(b);
  return (
    left.key === right.key &&
    left.ctrl === right.ctrl &&
    left.shift === right.shift &&
    left.alt === right.alt &&
    left.meta === right.meta
  );
}

// ─── Display helpers ─────────────────────────────────────────────────

const MAC_MOD_ORDER = ['ctrl', 'alt', 'shift', 'meta'] as const;
const MAC_MOD_SYMBOLS: Record<string, string> = {
  ctrl: '⌃',
  alt: '⌥',
  shift: '⇧',
  meta: '⌘',
};

/**
 * Convert a KeyCombo into a human-readable string.
 *   macOS:        "⌘T", "⌘⇧E", "⌃`"
 *   Windows/Linux: "Ctrl+T", "Ctrl+Shift+E"
 */
export function formatCombo(combo: KeyCombo, forMac = platform.isMac): string {
  if (forMac) {
    let s = '';
    for (const mod of MAC_MOD_ORDER) {
      if (combo[mod]) s += MAC_MOD_SYMBOLS[mod];
    }
    s += formatKeyName(combo.key, true);
    return s;
  }

  const parts: string[] = [];
  if (combo.ctrl) parts.push('Ctrl');
  if (combo.alt) parts.push('Alt');
  if (combo.shift) parts.push('Shift');
  parts.push(formatKeyName(combo.key, false));
  return parts.join('+');
}

function formatKeyName(key: string, isMac: boolean): string {
  const map: Record<string, string> = {
    arrowleft: isMac ? '←' : '←',
    arrowright: isMac ? '→' : '→',
    arrowup: isMac ? '↑' : '↑',
    arrowdown: isMac ? '↓' : '↓',
    escape: 'Esc',
    tab: 'Tab',
    enter: 'Enter',
    backspace: isMac ? '⌫' : 'Backspace',
    delete: isMac ? '⌦' : 'Delete',
    ' ': 'Space',
  };
  if (map[key]) return map[key];
  if (key.length === 1) return key.toUpperCase();
  return key.charAt(0).toUpperCase() + key.slice(1);
}

/**
 * Get the display string for an action on both platforms (useful for help screens / command palette).
 */
export function getDisplayBinding(id: ActionId): { mac: string; other: string } | undefined {
  const both = getBindingBoth(id);
  if (!both) return undefined;
  return {
    mac: formatCombo(both.mac, true),
    other: formatCombo(both.other, false),
  };
}
