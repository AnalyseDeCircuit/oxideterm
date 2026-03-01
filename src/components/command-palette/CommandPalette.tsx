/**
 * Command Palette v2.0
 *
 * Built on cmdk with:
 * - Mode prefix switching: (empty)=all, >=commands, @=sessions, #=connections
 * - Fuzzy match highlighting
 * - Keyboard shortcut hints
 * - MRU (most recently used) sorting
 * - Plugin command injection
 */

import React, { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Terminal,
  Plus,
  Settings,
  PanelLeft,
  Maximize2,
  X,
  Zap,
  Server,
  Keyboard,
  Hand,
  Columns2,
  Rows,
  Radio,
  Puzzle,
} from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '../ui/dialog';
import {
  Command,
  CommandInput,
  CommandList,
  CommandGroup,
  CommandItem,
  CommandEmpty,
  CommandShortcut,
} from '../ui/command';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/store/appStore';
import { useSettingsStore } from '@/store/settingsStore';
import { useBroadcastStore } from '@/store/broadcastStore';
import { useLocalTerminalStore } from '@/store/localTerminalStore';
import { usePluginStore } from '@/store/pluginStore';
import { connectToSaved } from '@/lib/connectToSaved';
import { useToast } from '@/hooks/useToast';
import type { ConnectionInfo } from '@/types';

// ─── Types ────────────────────────────────────────────────────────────

type PaletteSection =
  | 'recent'
  | 'quick_connect'
  | 'commands'
  | 'sessions'
  | 'connections'
  | 'help'
  | 'plugins';

type PaletteItem = {
  id: string;
  label: string;
  section: PaletteSection;
  icon: React.ReactNode;
  detail?: string;
  shortcut?: { mac: string; other: string };
  action: () => void | Promise<void>;
  /** cmdk search value — lowercased label + detail for matching */
  value: string;
};

type PaletteMode = 'all' | 'commands' | 'sessions' | 'connections';

type CommandPaletteProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onOpenShortcuts?: () => void;
};

// ─── Constants ────────────────────────────────────────────────────────

const QUICK_CONNECT_RE = /^([^@\s]+)@([^:\s]+)(?::(\d+))?$/;

const isMac = typeof navigator !== 'undefined' && /Mac|iPod|iPhone|iPad/.test(navigator.platform);

/** Map command IDs to their keyboard shortcuts */
const SHORTCUT_MAP: Record<string, { mac: string; other: string }> = {
  'cmd:new_terminal': { mac: '⌘T', other: 'Ctrl+T' },
  'cmd:new_connection': { mac: '⌘N', other: 'Ctrl+N' },
  'cmd:settings': { mac: '⌘,', other: 'Ctrl+,' },
  'cmd:toggle_sidebar': { mac: '⌘\\', other: 'Ctrl+\\' },
  'cmd:zen_mode': { mac: '⌘⇧Z', other: 'Ctrl+Shift+Z' },
  'cmd:close_tab': { mac: '⌘W', other: 'Ctrl+W' },
  'cmd:split_horizontal': { mac: '⌘⇧E', other: 'Ctrl+Shift+E' },
  'cmd:split_vertical': { mac: '⌘⇧D', other: 'Ctrl+Shift+D' },
  'cmd:broadcast_toggle': { mac: '⌘B', other: 'Ctrl+B' },
  'cmd:show_shortcuts': { mac: '⌘/', other: 'Ctrl+/' },
};

// ─── Highlight helper ─────────────────────────────────────────────────

/**
 * Returns React nodes with matched characters wrapped in <mark>.
 * Uses the same subsequence logic that cmdk's default filter employs.
 */
function highlightMatches(text: string, query: string): React.ReactNode {
  if (!query) return text;

  const lowerText = text.toLowerCase();
  const lowerQuery = query.toLowerCase();

  // Try substring match first (highlight contiguous range)
  const subIdx = lowerText.indexOf(lowerQuery);
  if (subIdx !== -1) {
    return (
      <>
        {text.slice(0, subIdx)}
        <mark className="bg-transparent text-theme-accent font-semibold">{text.slice(subIdx, subIdx + query.length)}</mark>
        {text.slice(subIdx + query.length)}
      </>
    );
  }

  // Subsequence match — highlight each matched character individually
  const result: React.ReactNode[] = [];
  let qi = 0;
  for (let i = 0; i < text.length; i++) {
    if (qi < lowerQuery.length && lowerText[i] === lowerQuery[qi]) {
      result.push(
        <mark key={i} className="bg-transparent text-theme-accent font-semibold">{text[i]}</mark>,
      );
      qi++;
    } else {
      result.push(text[i]);
    }
  }
  return <>{result}</>;
}

// ─── Mode parsing ─────────────────────────────────────────────────────

function parseMode(raw: string): { mode: PaletteMode; search: string } {
  if (raw.startsWith('>')) return { mode: 'commands', search: raw.slice(1).trimStart() };
  if (raw.startsWith('@')) return { mode: 'sessions', search: raw.slice(1).trimStart() };
  if (raw.startsWith('#')) return { mode: 'connections', search: raw.slice(1).trimStart() };
  return { mode: 'all', search: raw };
}

// Stable empty array — prevents Zustand selector from returning a new
// reference on every render when commandPaletteMru is undefined.
const EMPTY_MRU: string[] = [];

// ─── Component ────────────────────────────────────────────────────────

export const CommandPalette: React.FC<CommandPaletteProps> = ({ open, onOpenChange, onOpenShortcuts }) => {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [rawQuery, setRawQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);
  const isComposingRef = useRef(false);

  const { mode, search } = useMemo(() => parseMode(rawQuery), [rawQuery]);

  // Store selectors
  const tabs = useAppStore((s) => s.tabs);
  const savedConnections = useAppStore((s) => s.savedConnections);
  const pluginCommands = usePluginStore((s) => s.commands);
  const mru = useSettingsStore((s) => s.settings.commandPaletteMru ?? EMPTY_MRU);

  // ── Build command items ──────────────────────────────────────────

  const commandItems = useMemo<PaletteItem[]>(() => {
    const items: Array<Omit<PaletteItem, 'value'>> = [
      {
        id: 'cmd:new_terminal',
        label: t('command_palette.cmd_new_terminal'),
        section: 'commands',
        icon: <Terminal className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:new_terminal'],
        action: async () => {
          const { createTerminal } = useLocalTerminalStore.getState();
          const { createTab } = useAppStore.getState();
          const info = await createTerminal();
          createTab('local_terminal', info.id);
        },
      },
      {
        id: 'cmd:new_connection',
        label: t('command_palette.cmd_new_connection'),
        section: 'commands',
        icon: <Plus className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:new_connection'],
        action: () => useAppStore.getState().toggleModal('newConnection', true),
      },
      {
        id: 'cmd:settings',
        label: t('command_palette.cmd_settings'),
        section: 'commands',
        icon: <Settings className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:settings'],
        action: () => useAppStore.getState().createTab('settings'),
      },
      {
        id: 'cmd:toggle_sidebar',
        label: t('command_palette.cmd_toggle_sidebar'),
        section: 'commands',
        icon: <PanelLeft className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:toggle_sidebar'],
        action: () => useSettingsStore.getState().toggleSidebar(),
      },
      {
        id: 'cmd:zen_mode',
        label: t('command_palette.cmd_zen_mode'),
        section: 'commands',
        icon: <Maximize2 className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:zen_mode'],
        action: () => useSettingsStore.getState().toggleZenMode(),
      },
      {
        id: 'cmd:close_tab',
        label: t('command_palette.cmd_close_tab'),
        section: 'commands',
        icon: <X className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:close_tab'],
        action: async () => {
          const { activeTabId: tabId, closeTab } = useAppStore.getState();
          if (tabId) await closeTab(tabId);
        },
      },
      {
        id: 'cmd:split_horizontal',
        label: t('command_palette.cmd_split_horizontal'),
        section: 'commands',
        icon: <Rows className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:split_horizontal'],
        action: () => {
          window.dispatchEvent(new CustomEvent('oxideterm:split', { detail: { direction: 'horizontal' } }));
        },
      },
      {
        id: 'cmd:split_vertical',
        label: t('command_palette.cmd_split_vertical'),
        section: 'commands',
        icon: <Columns2 className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:split_vertical'],
        action: () => {
          window.dispatchEvent(new CustomEvent('oxideterm:split', { detail: { direction: 'vertical' } }));
        },
      },
      {
        id: 'cmd:broadcast_toggle',
        label: t('command_palette.cmd_broadcast_toggle'),
        section: 'commands',
        icon: <Radio className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:broadcast_toggle'],
        action: () => useBroadcastStore.getState().toggle(),
      },
      // ── Help ──
      {
        id: 'cmd:show_shortcuts',
        label: t('command_palette.cmd_show_shortcuts'),
        section: 'help',
        icon: <Keyboard className="h-4 w-4" />,
        shortcut: SHORTCUT_MAP['cmd:show_shortcuts'],
        action: () => onOpenShortcuts?.(),
      },
      {
        id: 'cmd:show_welcome',
        label: t('command_palette.cmd_show_welcome'),
        section: 'help',
        icon: <Hand className="h-4 w-4" />,
        action: () => useSettingsStore.getState().resetOnboarding(),
      },
    ];
    return items.map((i) => ({ ...i, value: buildValue(i) }));
  }, [t, onOpenShortcuts]);

  // ── Connection items ─────────────────────────────────────────────

  const connectionItems = useMemo<PaletteItem[]>(() => {
    return savedConnections.map((conn: ConnectionInfo) => {
      const label = conn.name || `${conn.username}@${conn.host}`;
      const detail = conn.name ? `${conn.username}@${conn.host}:${conn.port}` : `:${conn.port}`;
      return {
        id: `conn:${conn.id}`,
        label,
        section: 'connections' as const,
        icon: <Server className="h-4 w-4" />,
        detail,
        action: async () => {
          const { createTab } = useAppStore.getState();
          await connectToSaved(conn.id, { createTab, toast, t });
        },
        value: `${label} ${detail}`.toLowerCase(),
      };
    });
  }, [savedConnections, toast, t]);

  // ── Session items (all tab types) ────────────────────────────────

  const sessionItems = useMemo<PaletteItem[]>(() => {
    return tabs.map((tab) => ({
      id: `session:${tab.id}`,
      label: tab.title,
      section: 'sessions' as const,
      icon: <Terminal className="h-4 w-4" />,
      detail: tab.type === 'local_terminal' ? 'Local' : tab.type,
      action: () => useAppStore.getState().setActiveTab(tab.id),
      value: `${tab.title} ${tab.type}`.toLowerCase(),
    }));
  }, [tabs]);

  // ── Plugin command items ─────────────────────────────────────────

  const pluginItems = useMemo<PaletteItem[]>(() => {
    if (!pluginCommands || pluginCommands.size === 0) return [];
    const items: PaletteItem[] = [];
    for (const [, entry] of pluginCommands) {
      items.push({
        id: `plugin:${entry.pluginId}:${entry.id}`,
        label: entry.label,
        section: 'plugins',
        icon: <Puzzle className="h-4 w-4" />,
        shortcut: entry.shortcut ? { mac: entry.shortcut, other: entry.shortcut } : undefined,
        action: entry.handler,
        value: entry.label.toLowerCase(),
      });
    }
    return items;
  }, [pluginCommands]);

  // ── Quick connect item ───────────────────────────────────────────

  const quickConnectItem = useMemo<PaletteItem | null>(() => {
    const match = QUICK_CONNECT_RE.exec(search.trim());
    if (!match) return null;
    const [, username, host, portStr] = match;
    const port = portStr ? parseInt(portStr, 10) : 22;
    return {
      id: 'quick_connect',
      label: `${t('command_palette.quick_connect')}: ${username}@${host}:${port}`,
      section: 'quick_connect',
      icon: <Zap className="h-4 w-4" />,
      action: () => useAppStore.getState().toggleModal('newConnection', true, { host, port, username }),
      value: `quick_connect ${username}@${host}:${port}`,
    };
  }, [search, t]);

  // ── MRU recent items ─────────────────────────────────────────────

  const recentItems = useMemo<PaletteItem[]>(() => {
    if (mode !== 'all' || search) return [];
    const allItems = [...commandItems, ...connectionItems, ...sessionItems, ...pluginItems];
    const recent: PaletteItem[] = [];
    for (const id of mru) {
      const item = allItems.find((i) => i.id === id);
      if (item && recent.length < 5) {
        recent.push({ ...item, section: 'recent' as const });
      }
    }
    return recent;
  }, [mru, mode, search, commandItems, connectionItems, sessionItems, pluginItems]);

  // ── Execute ──────────────────────────────────────────────────────

  const executeItem = useCallback(
    (item: PaletteItem) => {
      onOpenChange(false);
      // Record MRU
      useSettingsStore.getState().recordCommandMru(item.id);
      requestAnimationFrame(() => {
        item.action();
      });
    },
    [onOpenChange],
  );

  // ── Placeholder based on mode ────────────────────────────────────

  const placeholder = useMemo(() => {
    switch (mode) {
      case 'commands':
        return t('command_palette.placeholder_commands');
      case 'sessions':
        return t('command_palette.placeholder_sessions');
      case 'connections':
        return t('command_palette.placeholder_connections');
      default:
        return t('command_palette.placeholder');
    }
  }, [mode, t]);

  // ── Reset on open/close ──────────────────────────────────────────

  useEffect(() => {
    if (open) {
      setRawQuery('');
      if (savedConnections.length === 0) {
        useAppStore.getState().loadSavedConnections();
      }
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open, savedConnections.length]);

  // ── Mode badge label ─────────────────────────────────────────────

  const modeBadge = useMemo(() => {
    switch (mode) {
      case 'commands': return '>';
      case 'sessions': return '@';
      case 'connections': return '#';
      default: return null;
    }
  }, [mode]);

  // ── Visible sections based on mode ───────────────────────────────

  const showCommands = mode === 'all' || mode === 'commands';
  const showSessions = mode === 'all' || mode === 'sessions';
  const showConnections = mode === 'all' || mode === 'connections';
  const showHelp = mode === 'all' || mode === 'commands';
  const showPlugins = mode === 'all' || mode === 'commands';

  // ── Render ───────────────────────────────────────────────────────

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-w-[560px] p-0 gap-0 top-[20%] translate-y-0 overflow-hidden"
        onOpenAutoFocus={(e) => e.preventDefault()}
        aria-describedby={undefined}
      >
        <DialogTitle className="sr-only">
          {t('command_palette.title')}
        </DialogTitle>

        <Command
          filter={(value, search) => {
            // Quick connect always visible
            if (value.startsWith('quick_connect')) return 1;
            // Default substring/subsequence filter
            if (!search) return 1;
            const lv = value.toLowerCase();
            const ls = search.toLowerCase();
            if (lv.includes(ls)) return 1;
            // subsequence
            let qi = 0;
            for (let i = 0; i < lv.length && qi < ls.length; i++) {
              if (lv[i] === ls[qi]) qi++;
            }
            return qi === ls.length ? 0.5 : 0;
          }}
          shouldFilter={true}
        >
          {/* ── Input area ── */}
          <div className="flex items-center border-b border-theme-border px-3">
            {modeBadge && (
              <span className="mr-1.5 shrink-0 rounded bg-theme-accent/20 px-1.5 py-0.5 text-xs font-mono font-semibold text-theme-accent">
                {modeBadge}
              </span>
            )}
            <CommandInput
              ref={inputRef}
              value={rawQuery}
              onValueChange={setRawQuery}
              placeholder={placeholder}
              onCompositionStart={() => { isComposingRef.current = true; }}
              onCompositionEnd={() => { isComposingRef.current = false; }}
              className="border-0"
            />
          </div>

          <CommandList>
            <CommandEmpty>{t('command_palette.no_results')}</CommandEmpty>

            {/* ── Quick Connect ── */}
            {quickConnectItem && (
              <CommandGroup heading={t('command_palette.quick_connect')}>
                <CommandItem
                  value={quickConnectItem.value}
                  onSelect={() => executeItem(quickConnectItem)}
                  forceMount
                >
                  <span className="shrink-0 text-theme-text-muted">{quickConnectItem.icon}</span>
                  <span className="truncate">{quickConnectItem.label}</span>
                </CommandItem>
              </CommandGroup>
            )}

            {/* ── Recent (MRU) ── */}
            {recentItems.length > 0 && (
              <CommandGroup heading={t('command_palette.section_recent')}>
                {recentItems.map((item) => (
                  <PaletteCommandItem
                    key={`recent:${item.id}`}
                    item={item}
                    search={search}
                    onSelect={() => executeItem(item)}
                  />
                ))}
              </CommandGroup>
            )}

            {/* ── Commands ── */}
            {showCommands && (
              <CommandGroup heading={t('command_palette.section_commands')}>
                {commandItems
                  .filter((i) => i.section === 'commands')
                  .map((item) => (
                    <PaletteCommandItem
                      key={item.id}
                      item={item}
                      search={search}
                      onSelect={() => executeItem(item)}
                    />
                  ))}
              </CommandGroup>
            )}

            {/* ── Sessions ── */}
            {showSessions && sessionItems.length > 0 && (
              <CommandGroup heading={t('command_palette.section_sessions')}>
                {sessionItems.map((item) => (
                  <PaletteCommandItem
                    key={item.id}
                    item={item}
                    search={search}
                    onSelect={() => executeItem(item)}
                  />
                ))}
              </CommandGroup>
            )}

            {/* ── Connections ── */}
            {showConnections && connectionItems.length > 0 && (
              <CommandGroup heading={t('command_palette.section_connections')}>
                {connectionItems.map((item) => (
                  <PaletteCommandItem
                    key={item.id}
                    item={item}
                    search={search}
                    onSelect={() => executeItem(item)}
                  />
                ))}
              </CommandGroup>
            )}

            {/* ── Plugin Commands ── */}
            {showPlugins && pluginItems.length > 0 && (
              <CommandGroup heading={t('command_palette.section_plugins')}>
                {pluginItems.map((item) => (
                  <PaletteCommandItem
                    key={item.id}
                    item={item}
                    search={search}
                    onSelect={() => executeItem(item)}
                  />
                ))}
              </CommandGroup>
            )}

            {/* ── Help ── */}
            {showHelp && (
              <CommandGroup heading={t('command_palette.section_help')}>
                {commandItems
                  .filter((i) => i.section === 'help')
                  .map((item) => (
                    <PaletteCommandItem
                      key={item.id}
                      item={item}
                      search={search}
                      onSelect={() => executeItem(item)}
                    />
                  ))}
              </CommandGroup>
            )}
          </CommandList>

          {/* ── Footer hint ── */}
          <div className="border-t border-theme-border px-3 py-1.5 text-[11px] text-theme-text-muted select-none flex items-center gap-3">
            <span>{t('command_palette.mode_hint')}</span>
          </div>
        </Command>
      </DialogContent>
    </Dialog>
  );
};

// ─── Individual item component ────────────────────────────────────────

type PaletteCommandItemProps = {
  item: PaletteItem;
  search: string;
  onSelect: () => void;
};

const PaletteCommandItem: React.FC<PaletteCommandItemProps> = ({ item, search, onSelect }) => {
  return (
    <CommandItem value={item.value} onSelect={onSelect}>
      <span className={cn(
        'shrink-0 text-theme-text-muted',
        'group-data-[selected=true]:text-theme-accent',
      )}>
        {item.icon}
      </span>
      <span className="truncate">
        {highlightMatches(item.label, search)}
      </span>
      {item.detail && (
        <span className="ml-1 truncate text-xs text-theme-text-muted">
          {highlightMatches(item.detail, search)}
        </span>
      )}
      {item.shortcut && (
        <CommandShortcut>
          {isMac ? item.shortcut.mac : item.shortcut.other}
        </CommandShortcut>
      )}
    </CommandItem>
  );
};

// ─── Helpers ──────────────────────────────────────────────────────────

function buildValue(item: Omit<PaletteItem, 'value'>): string {
  return [item.label, item.detail].filter(Boolean).join(' ').toLowerCase();
}
