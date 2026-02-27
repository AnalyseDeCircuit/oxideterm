import React, { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Terminal,
  Plus,
  Settings,
  PanelLeft,
  Maximize2,
  X,
  Search,
  Zap,
  Server,
  Keyboard,
  Hand,
  Columns2,
  Rows,
} from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogTitle,
} from '../ui/dialog';
import { Input } from '../ui/input';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/store/appStore';
import { useSettingsStore } from '@/store/settingsStore';
import { useLocalTerminalStore } from '@/store/localTerminalStore';
import { connectToSaved } from '@/lib/connectToSaved';
import { useToast } from '@/hooks/useToast';
import type { ConnectionInfo } from '@/types';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type PaletteItem = {
  id: string;
  label: string;
  section: 'commands' | 'connections' | 'sessions' | 'quick_connect' | 'help';
  icon: React.ReactNode;
  detail?: string;
  action: () => void | Promise<void>;
};

type CommandPaletteProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onOpenShortcuts?: () => void;
};

// ---------------------------------------------------------------------------
// Fuzzy match helper
// ---------------------------------------------------------------------------

function fuzzyMatch(query: string, text: string): boolean {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (t.includes(q)) return true;
  // character-by-character subsequence match
  let qi = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) qi++;
  }
  return qi === q.length;
}

// ---------------------------------------------------------------------------
// Quick-connect pattern: user@host or user@host:port
// ---------------------------------------------------------------------------

const QUICK_CONNECT_RE = /^([^@\s]+)@([^:\s]+)(?::(\d+))?$/;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const CommandPalette: React.FC<CommandPaletteProps> = ({ open, onOpenChange, onOpenShortcuts }) => {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const isComposingRef = useRef(false);

  // Store selectors — minimal subscriptions
  const tabs = useAppStore((s) => s.tabs);
  const savedConnections = useAppStore((s) => s.savedConnections);

  // ---- Build command items ------------------------------------------------

  const commandItems = useMemo<PaletteItem[]>(() => {
    const items: PaletteItem[] = [
      {
        id: 'cmd:new_terminal',
        label: t('command_palette.cmd_new_terminal'),
        section: 'commands',
        icon: <Terminal className="h-4 w-4" />,
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
        action: () => {
          useAppStore.getState().toggleModal('newConnection', true);
        },
      },
      {
        id: 'cmd:settings',
        label: t('command_palette.cmd_settings'),
        section: 'commands',
        icon: <Settings className="h-4 w-4" />,
        action: () => {
          useAppStore.getState().createTab('settings');
        },
      },
      {
        id: 'cmd:toggle_sidebar',
        label: t('command_palette.cmd_toggle_sidebar'),
        section: 'commands',
        icon: <PanelLeft className="h-4 w-4" />,
        action: () => {
          useSettingsStore.getState().toggleSidebar();
        },
      },
      {
        id: 'cmd:zen_mode',
        label: t('command_palette.cmd_zen_mode'),
        section: 'commands',
        icon: <Maximize2 className="h-4 w-4" />,
        action: () => {
          useSettingsStore.getState().toggleZenMode();
        },
      },
      {
        id: 'cmd:close_tab',
        label: t('command_palette.cmd_close_tab'),
        section: 'commands',
        icon: <X className="h-4 w-4" />,
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
        action: () => {
          // Trigger Cmd/Ctrl+Shift+E via keyboard event simulation is not reliable;
          // dispatch a custom event that useSplitPaneShortcuts can listen to.
          window.dispatchEvent(new CustomEvent('oxideterm:split', { detail: { direction: 'horizontal' } }));
        },
      },
      {
        id: 'cmd:split_vertical',
        label: t('command_palette.cmd_split_vertical'),
        section: 'commands',
        icon: <Columns2 className="h-4 w-4" />,
        action: () => {
          window.dispatchEvent(new CustomEvent('oxideterm:split', { detail: { direction: 'vertical' } }));
        },
      },
      // ---- Help commands ----
      {
        id: 'cmd:show_shortcuts',
        label: t('command_palette.cmd_show_shortcuts'),
        section: 'help',
        icon: <Keyboard className="h-4 w-4" />,
        action: () => onOpenShortcuts?.(),
      },
      {
        id: 'cmd:show_welcome',
        label: t('command_palette.cmd_show_welcome'),
        section: 'help',
        icon: <Hand className="h-4 w-4" />,
        action: () => {
          useSettingsStore.getState().resetOnboarding();
        },
      },
    ];
    return items;
  }, [t, onOpenShortcuts]);

  // ---- Build connection items ---------------------------------------------

  const connectionItems = useMemo<PaletteItem[]>(() => {
    return savedConnections.map((conn: ConnectionInfo) => ({
      id: `conn:${conn.id}`,
      label: conn.name || `${conn.username}@${conn.host}`,
      section: 'connections' as const,
      icon: <Server className="h-4 w-4" />,
      detail: conn.name ? `${conn.username}@${conn.host}:${conn.port}` : `:${conn.port}`,
      action: async () => {
        const { createTab } = useAppStore.getState();
        await connectToSaved(conn.id, {
          createTab,
          toast,
          t,
        });
      },
    }));
  }, [savedConnections, toast, t]);

  // ---- Build active session items -----------------------------------------

  const sessionItems = useMemo<PaletteItem[]>(() => {
    return tabs
      .filter((tab) => tab.type === 'terminal' || tab.type === 'local_terminal')
      .map((tab) => ({
        id: `session:${tab.id}`,
        label: tab.title,
        section: 'sessions' as const,
        icon: <Terminal className="h-4 w-4" />,
        detail: tab.type === 'local_terminal' ? 'Local' : undefined,
        action: () => {
          useAppStore.getState().setActiveTab(tab.id);
        },
      }));
  }, [tabs]);

  // ---- Build quick-connect item (dynamic) ---------------------------------

  const quickConnectItem = useMemo<PaletteItem | null>(() => {
    const match = QUICK_CONNECT_RE.exec(query.trim());
    if (!match) return null;
    const [, username, host, portStr] = match;
    const port = portStr ? parseInt(portStr, 10) : 22;
    return {
      id: 'quick_connect',
      label: `${t('command_palette.quick_connect')}: ${username}@${host}:${port}`,
      section: 'quick_connect',
      icon: <Zap className="h-4 w-4" />,
      action: () => {
        // Open new connection modal with pre-filled host/port/username
        useAppStore.getState().toggleModal('newConnection', true, { host, port, username });
      },
    };
  }, [query, t]);

  // ---- Filter & assemble --------------------------------------------------

  const filteredItems = useMemo(() => {
    const allItems = [...commandItems, ...connectionItems, ...sessionItems];
    if (quickConnectItem) allItems.unshift(quickConnectItem);

    if (!query.trim()) return allItems;

    return allItems.filter(
      (item) =>
        item.section === 'quick_connect' ||
        fuzzyMatch(query, item.label) ||
        (item.detail && fuzzyMatch(query, item.detail))
    );
  }, [query, commandItems, connectionItems, sessionItems, quickConnectItem]);

  // ---- Section grouping ---------------------------------------------------

  type Section = { key: string; label: string; items: PaletteItem[] };

  const sections = useMemo<Section[]>(() => {
    const sectionOrder: Array<{ key: PaletteItem['section']; label: string }> = [
      { key: 'quick_connect', label: t('command_palette.quick_connect') },
      { key: 'commands', label: t('command_palette.section_commands') },
      { key: 'sessions', label: t('command_palette.section_sessions') },
      { key: 'connections', label: t('command_palette.section_connections') },
      { key: 'help', label: t('command_palette.section_help') },
    ];

    return sectionOrder
      .map(({ key, label }) => ({
        key,
        label,
        items: filteredItems.filter((item) => item.section === key),
      }))
      .filter((s) => s.items.length > 0);
  }, [filteredItems, t]);

  // Flat list of items for keyboard navigation
  const flatItems = useMemo(() => sections.flatMap((s) => s.items), [sections]);

  // ---- Reset on open/close ------------------------------------------------

  useEffect(() => {
    if (open) {
      setQuery('');
      setSelectedIndex(0);
      // Load saved connections if empty
      if (savedConnections.length === 0) {
        useAppStore.getState().loadSavedConnections();
      }
      // Focus input after dialog opens
      requestAnimationFrame(() => {
        inputRef.current?.focus();
      });
    }
  }, [open, savedConnections.length]);

  // ---- Clamp selectedIndex on list change ---------------------------------

  useEffect(() => {
    if (selectedIndex >= flatItems.length) {
      setSelectedIndex(Math.max(0, flatItems.length - 1));
    }
  }, [flatItems.length, selectedIndex]);

  // ---- Scroll selected into view ------------------------------------------

  useEffect(() => {
    if (!listRef.current) return;
    const selected = listRef.current.querySelector('[data-selected="true"]');
    if (selected) {
      selected.scrollIntoView({ block: 'nearest' });
    }
  }, [selectedIndex]);

  // ---- Execute item -------------------------------------------------------

  const executeItem = useCallback(
    (item: PaletteItem) => {
      onOpenChange(false);
      // Defer action to let dialog close animation start
      requestAnimationFrame(() => {
        item.action();
      });
    },
    [onOpenChange]
  );

  // ---- Keyboard navigation ------------------------------------------------

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      // Ignore key events during IME composition (e.g. Chinese / Japanese input)
      const nativeEvent = e.nativeEvent as KeyboardEvent & { isComposing?: boolean; keyCode?: number };
      if (
        isComposingRef.current ||
        nativeEvent.isComposing ||
        nativeEvent.keyCode === 229 ||
        e.key === 'Process'
      ) {
        return;
      }

      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex((i) => (i + 1) % Math.max(1, flatItems.length));
          break;
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex((i) => (i - 1 + flatItems.length) % Math.max(1, flatItems.length));
          break;
        case 'Enter':
          e.preventDefault();
          if (flatItems[selectedIndex]) {
            executeItem(flatItems[selectedIndex]);
          }
          break;
        case 'Escape':
          e.preventDefault();
          onOpenChange(false);
          break;
      }
    },
    [flatItems, selectedIndex, executeItem, onOpenChange]
  );

  // Reset selectedIndex when query changes
  useEffect(() => {
    setSelectedIndex(0);
  }, [query]);

  // ---- Render -------------------------------------------------------------

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-w-[520px] p-0 gap-0 top-[20%] translate-y-0 overflow-hidden"
        onKeyDown={handleKeyDown}
        // Prevent Dialog from stealing focus from input
        onOpenAutoFocus={(e) => e.preventDefault()}
      >
        {/* Accessible title — visually hidden */}
        <DialogTitle className="sr-only">
          {t('command_palette.section_commands')}
        </DialogTitle>

        {/* Search input */}
        <div className="flex items-center border-b border-theme-border px-3">
          <Search className="h-4 w-4 shrink-0 text-theme-text-muted" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onCompositionStart={() => {
              isComposingRef.current = true;
            }}
            onCompositionEnd={() => {
              isComposingRef.current = false;
            }}
            placeholder={t('command_palette.placeholder')}
            className="h-10 border-0 bg-transparent focus-visible:ring-0 focus-visible:ring-offset-0 placeholder:text-theme-text-muted"
            autoComplete="off"
            spellCheck={false}
          />
        </div>

        {/* Results list */}
        <div
          ref={listRef}
          className="max-h-[320px] overflow-y-auto overscroll-contain py-1"
        >
          {flatItems.length === 0 ? (
            <div className="py-6 text-center text-sm text-theme-text-muted">
              {t('command_palette.no_results')}
            </div>
          ) : (
            sections.map((section) => (
              <div key={section.key}>
                <div className="px-3 py-1.5 text-xs font-medium text-theme-text-muted select-none">
                  {section.label}
                </div>
                {section.items.map((item) => {
                  const globalIdx = flatItems.indexOf(item);
                  const isSelected = globalIdx === selectedIndex;
                  return (
                    <button
                      key={item.id}
                      data-selected={isSelected}
                      className={cn(
                        'flex w-full items-center gap-2.5 px-3 py-1.5 text-sm text-theme-text',
                        'outline-none cursor-pointer select-none',
                        'hover:bg-theme-bg-hover',
                        isSelected && 'bg-theme-accent/15 text-theme-accent'
                      )}
                      onClick={() => executeItem(item)}
                      onMouseEnter={() => setSelectedIndex(globalIdx)}
                    >
                      <span
                        className={cn(
                          'shrink-0 text-theme-text-muted',
                          isSelected && 'text-theme-accent'
                        )}
                      >
                        {item.icon}
                      </span>
                      <span className="truncate">{item.label}</span>
                      {item.detail && (
                        <span className="ml-auto truncate text-xs text-theme-text-muted">
                          {item.detail}
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
};
