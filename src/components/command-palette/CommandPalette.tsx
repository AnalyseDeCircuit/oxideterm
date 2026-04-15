// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

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
  ChevronRight,
  ChevronLeft,
  XCircle,
  Layers,
  ArrowLeft,
  ArrowRight,
  FolderOpen,
  Palette,
  ZoomIn,
  ZoomOut,
  RotateCcw,
  RectangleHorizontal,
  Minus,
  Type,
  ListTree,
  Bookmark,
  HardDrive,
  ArrowUpDown,
  Bot,
  Unplug,
  RefreshCw,
  Ban,
  Activity,
  Stethoscope,
  Clapperboard,
  Paperclip,
  Trash2,
  Gauge,
  PanelBottomClose,
  SquareStack,
  Focus,
  Globe,
  RotateCw,
  AlertTriangle,
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
import { removeConnectionsById, useAppStore } from '@/store/appStore';
import { useSettingsStore } from '@/store/settingsStore';
import { useBroadcastStore } from '@/store/broadcastStore';
import { useLocalTerminalStore } from '@/store/localTerminalStore';
import { usePluginStore } from '@/store/pluginStore';
import { useActivityStore } from '@/store/activityStore';

import { connectToSaved } from '@/lib/connectToSaved';
import { useToast } from '@/hooks/useToast';
import { getAllThemeNames } from '@/lib/themes';
import { api } from '@/lib/api';
import { useReconnectOrchestratorStore } from '@/store/reconnectOrchestratorStore';
import { useSessionTreeStore } from '@/store/sessionTreeStore';
import type { ConnectionInfo, PaneNode } from '@/types';
import { platform } from '@/lib/platform';
import { type ActionId, getDisplayBinding } from '@/lib/keybindingRegistry';

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

const isMac = platform.isMac;

/** Map command IDs to their registry ActionIds for shortcut display */
const CMD_TO_ACTION: Record<string, ActionId> = {
  'cmd:new_terminal': 'app.newTerminal',
  'cmd:new_connection': 'app.newConnection',
  'cmd:settings': 'app.settings',
  'cmd:toggle_sidebar': 'app.toggleSidebar',
  'cmd:zen_mode': 'app.zenMode',
  'cmd:toggle_panel': 'palette.eventLog',
  'cmd:toggle_ai_sidebar': 'palette.aiSidebar',
  'cmd:close_tab': 'app.closeTab',
  'cmd:split_horizontal': 'split.horizontal',
  'cmd:split_vertical': 'split.vertical',
  'cmd:broadcast_toggle': 'palette.broadcast',
  'cmd:show_shortcuts': 'app.showShortcuts',
  'cmd:next_tab': 'app.nextTab',
  'cmd:prev_tab': 'app.prevTab',
  'cmd:close_other_tabs': 'app.closeOtherTabs',
  'cmd:go_back': 'app.navBack',
  'cmd:go_forward': 'app.navForward',
  'cmd:shell_launcher': 'app.shellLauncher',
  'cmd:font_increase': 'app.fontIncrease',
  'cmd:font_decrease': 'app.fontDecrease',
  'cmd:font_reset': 'app.fontReset',
};

/** Get keyboard shortcut display strings for a command, reading from the keybinding registry. */
function getCommandShortcut(cmdId: string): { mac: string; other: string } | undefined {
  const actionId = CMD_TO_ACTION[cmdId];
  if (!actionId) return undefined;
  return getDisplayBinding(actionId);
}

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
        shortcut: getCommandShortcut('cmd:new_terminal'),
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
        shortcut: getCommandShortcut('cmd:new_connection'),
        action: () => useAppStore.getState().toggleModal('newConnection', true),
      },
      {
        id: 'cmd:settings',
        label: t('command_palette.cmd_settings'),
        section: 'commands',
        icon: <Settings className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:settings'),
        action: () => useAppStore.getState().createTab('settings'),
      },
      {
        id: 'cmd:toggle_sidebar',
        label: t('command_palette.cmd_toggle_sidebar'),
        section: 'commands',
        icon: <PanelLeft className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:toggle_sidebar'),
        action: () => useSettingsStore.getState().toggleSidebar(),
      },
      {
        id: 'cmd:zen_mode',
        label: t('command_palette.cmd_zen_mode'),
        section: 'commands',
        icon: <Maximize2 className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:zen_mode'),
        action: () => useSettingsStore.getState().toggleZenMode(),
      },
      {
        id: 'cmd:toggle_panel',
        label: t('command_palette.cmd_toggle_panel'),
        section: 'commands',
        icon: <Rows className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:toggle_panel'),
        action: () => {
          useActivityStore.getState().setActiveView('event_log');
          useAppStore.getState().createTab('activity');
        },
      },
      {
        id: 'cmd:toggle_ai_sidebar',
        label: t('command_palette.cmd_toggle_ai_sidebar'),
        section: 'commands',
        icon: <PanelLeft className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:toggle_ai_sidebar'),
        action: () => useSettingsStore.getState().toggleAiSidebar(),
      },
      {
        id: 'cmd:close_tab',
        label: t('command_palette.cmd_close_tab'),
        section: 'commands',
        icon: <X className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:close_tab'),
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
        shortcut: getCommandShortcut('cmd:split_horizontal'),
        action: () => {
          window.dispatchEvent(new CustomEvent('oxideterm:split', { detail: { direction: 'horizontal' } }));
        },
      },
      {
        id: 'cmd:split_vertical',
        label: t('command_palette.cmd_split_vertical'),
        section: 'commands',
        icon: <Columns2 className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:split_vertical'),
        action: () => {
          window.dispatchEvent(new CustomEvent('oxideterm:split', { detail: { direction: 'vertical' } }));
        },
      },
      {
        id: 'cmd:broadcast_toggle',
        label: t('command_palette.cmd_broadcast_toggle'),
        section: 'commands',
        icon: <Radio className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:broadcast_toggle'),
        action: () => useBroadcastStore.getState().toggle(),
      },
      // ── Help ──
      {
        id: 'cmd:show_shortcuts',
        label: t('command_palette.cmd_show_shortcuts'),
        section: 'help',
        icon: <Keyboard className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:show_shortcuts'),
        action: () => onOpenShortcuts?.(),
      },
      {
        id: 'cmd:show_welcome',
        label: t('command_palette.cmd_show_welcome'),
        section: 'help',
        icon: <Hand className="h-4 w-4" />,
        action: () => useSettingsStore.getState().resetOnboarding(),
      },

      // ═══════════════════════════════════════════════════════════════
      // Tab Management
      // ═══════════════════════════════════════════════════════════════
      {
        id: 'cmd:next_tab',
        label: t('command_palette.cmd_next_tab'),
        section: 'commands',
        icon: <ChevronRight className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:next_tab'),
        action: () => useAppStore.getState().nextTab(),
      },
      {
        id: 'cmd:prev_tab',
        label: t('command_palette.cmd_prev_tab'),
        section: 'commands',
        icon: <ChevronLeft className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:prev_tab'),
        action: () => useAppStore.getState().prevTab(),
      },
      {
        id: 'cmd:close_other_tabs',
        label: t('command_palette.cmd_close_other_tabs'),
        section: 'commands',
        icon: <XCircle className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:close_other_tabs'),
        action: async () => {
          const { tabs, activeTabId, closeTab } = useAppStore.getState();
          if (!activeTabId) return;
          const others = tabs.filter((tab) => tab.id !== activeTabId);
          for (const tab of others) {
            await closeTab(tab.id);
          }
        },
      },
      {
        id: 'cmd:close_all_tabs',
        label: t('command_palette.cmd_close_all_tabs'),
        section: 'commands',
        icon: <Layers className="h-4 w-4" />,
        action: async () => {
          const { tabs, closeTab } = useAppStore.getState();
          for (const tab of [...tabs]) {
            await closeTab(tab.id);
          }
        },
      },
      {
        id: 'cmd:go_back',
        label: t('command_palette.cmd_go_back'),
        section: 'commands',
        icon: <ArrowLeft className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:go_back'),
        action: () => useAppStore.getState().navigateBack(),
      },
      {
        id: 'cmd:go_forward',
        label: t('command_palette.cmd_go_forward'),
        section: 'commands',
        icon: <ArrowRight className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:go_forward'),
        action: () => useAppStore.getState().navigateForward(),
      },
      {
        id: 'cmd:open_connection_manager',
        label: t('command_palette.cmd_open_connection_manager'),
        section: 'commands',
        icon: <FolderOpen className="h-4 w-4" />,
        action: () => useAppStore.getState().toggleModal('connectionManager', true),
      },

      // ═══════════════════════════════════════════════════════════════
      // Theme / Appearance
      // ═══════════════════════════════════════════════════════════════
      {
        id: 'cmd:theme_next',
        label: t('command_palette.cmd_theme_next'),
        section: 'commands',
        icon: <Palette className="h-4 w-4" />,
        action: () => {
          const s = useSettingsStore.getState();
          const names = getAllThemeNames();
          const idx = names.indexOf(s.settings.terminal.theme);
          s.updateTerminal('theme', names[(idx + 1) % names.length]);
        },
      },
      {
        id: 'cmd:theme_prev',
        label: t('command_palette.cmd_theme_prev'),
        section: 'commands',
        icon: <Palette className="h-4 w-4" />,
        action: () => {
          const s = useSettingsStore.getState();
          const names = getAllThemeNames();
          const idx = names.indexOf(s.settings.terminal.theme);
          s.updateTerminal('theme', names[(idx - 1 + names.length) % names.length]);
        },
      },
      {
        id: 'cmd:font_increase',
        label: t('command_palette.cmd_font_increase'),
        section: 'commands',
        icon: <ZoomIn className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:font_increase'),
        action: () => {
          const s = useSettingsStore.getState();
          const cur = s.settings.terminal.fontSize;
          if (cur < 32) s.updateTerminal('fontSize', cur + 1);
        },
      },
      {
        id: 'cmd:font_decrease',
        label: t('command_palette.cmd_font_decrease'),
        section: 'commands',
        icon: <ZoomOut className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:font_decrease'),
        action: () => {
          const s = useSettingsStore.getState();
          const cur = s.settings.terminal.fontSize;
          if (cur > 8) s.updateTerminal('fontSize', cur - 1);
        },
      },
      {
        id: 'cmd:font_reset',
        label: t('command_palette.cmd_font_reset'),
        section: 'commands',
        icon: <RotateCcw className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:font_reset'),
        action: () => useSettingsStore.getState().updateTerminal('fontSize', 14),
      },
      {
        id: 'cmd:cursor_block',
        label: t('command_palette.cmd_cursor_block'),
        section: 'commands',
        icon: <RectangleHorizontal className="h-4 w-4" />,
        action: () => useSettingsStore.getState().updateTerminal('cursorStyle', 'block'),
      },
      {
        id: 'cmd:cursor_bar',
        label: t('command_palette.cmd_cursor_bar'),
        section: 'commands',
        icon: <Minus className="h-4 w-4" />,
        action: () => useSettingsStore.getState().updateTerminal('cursorStyle', 'bar'),
      },
      {
        id: 'cmd:cursor_underline',
        label: t('command_palette.cmd_cursor_underline'),
        section: 'commands',
        icon: <Type className="h-4 w-4" />,
        action: () => useSettingsStore.getState().updateTerminal('cursorStyle', 'underline'),
      },

      // ═══════════════════════════════════════════════════════════════
      // Sidebar Navigation
      // ═══════════════════════════════════════════════════════════════
      {
        id: 'cmd:sidebar_sessions',
        label: t('command_palette.cmd_sidebar_sessions'),
        section: 'commands',
        icon: <ListTree className="h-4 w-4" />,
        action: () => {
          const s = useSettingsStore.getState();
          if (s.settings.sidebarUI.collapsed) s.toggleSidebar();
          s.setSidebarSection('sessions');
        },
      },
      {
        id: 'cmd:sidebar_saved',
        label: t('command_palette.cmd_sidebar_saved'),
        section: 'commands',
        icon: <Bookmark className="h-4 w-4" />,
        action: () => {
          const s = useSettingsStore.getState();
          if (s.settings.sidebarUI.collapsed) s.toggleSidebar();
          s.setSidebarSection('saved');
        },
      },
      {
        id: 'cmd:sidebar_sftp',
        label: t('command_palette.cmd_sidebar_sftp'),
        section: 'commands',
        icon: <HardDrive className="h-4 w-4" />,
        action: () => {
          const s = useSettingsStore.getState();
          if (s.settings.sidebarUI.collapsed) s.toggleSidebar();
          s.setSidebarSection('sftp');
        },
      },
      {
        id: 'cmd:sidebar_forwards',
        label: t('command_palette.cmd_sidebar_forwards'),
        section: 'commands',
        icon: <ArrowUpDown className="h-4 w-4" />,
        action: () => {
          const s = useSettingsStore.getState();
          if (s.settings.sidebarUI.collapsed) s.toggleSidebar();
          s.setSidebarSection('forwards');
        },
      },
      {
        id: 'cmd:sidebar_connections',
        label: t('command_palette.cmd_sidebar_connections'),
        section: 'commands',
        icon: <Server className="h-4 w-4" />,
        action: () => {
          const s = useSettingsStore.getState();
          if (s.settings.sidebarUI.collapsed) s.toggleSidebar();
          s.setSidebarSection('connections');
        },
      },
      {
        id: 'cmd:sidebar_ai',
        label: t('command_palette.cmd_sidebar_ai'),
        section: 'commands',
        icon: <Bot className="h-4 w-4" />,
        action: () => {
          const s = useSettingsStore.getState();
          if (s.settings.sidebarUI.aiSidebarCollapsed) s.toggleAiSidebar();
        },
      },

      // ═══════════════════════════════════════════════════════════════
      // Connection Operations
      // ═══════════════════════════════════════════════════════════════
      {
        id: 'cmd:disconnect_all',
        label: t('command_palette.cmd_disconnect_all'),
        section: 'commands',
        icon: <Unplug className="h-4 w-4" />,
        action: async () => {
          const conns = useAppStore.getState().connections;
          const disconnectedIds: string[] = [];
          for (const [, conn] of conns) {
            try {
              await api.sshDisconnect(conn.id);
              disconnectedIds.push(conn.id);
            } catch {
              /* skip */
            }
          }
          useAppStore.setState((state) => ({
            connections: removeConnectionsById(state.connections, disconnectedIds),
          }));
        },
      },
      {
        id: 'cmd:reconnect_all',
        label: t('command_palette.cmd_reconnect_all'),
        section: 'commands',
        icon: <RefreshCw className="h-4 w-4" />,
        action: () => {
          // Trigger reconnect for all link-down nodes via orchestrator
          const linkDownIds = useSessionTreeStore.getState().linkDownNodeIds;
          const orchestrator = useReconnectOrchestratorStore.getState();
          for (const nodeId of linkDownIds) {
            orchestrator.scheduleReconnect(nodeId);
          }
        },
      },
      {
        id: 'cmd:cancel_reconnect',
        label: t('command_palette.cmd_cancel_reconnect'),
        section: 'commands',
        icon: <Ban className="h-4 w-4" />,
        action: () => useReconnectOrchestratorStore.getState().cancelAll(),
      },
      {
        id: 'cmd:open_connection_pool',
        label: t('command_palette.cmd_open_connection_pool'),
        section: 'commands',
        icon: <Activity className="h-4 w-4" />,
        action: () => useAppStore.getState().createTab('connection_pool'),
      },
      {
        id: 'cmd:health_check',
        label: t('command_palette.cmd_health_check'),
        section: 'commands',
        icon: <Stethoscope className="h-4 w-4" />,
        action: async () => {
          const status = await api.getAllHealthStatus();
          const total = Object.keys(status).length;
          const healthy = Object.values(status).filter((s) => s.status === 'Healthy').length;
          toast({
            title: t('command_palette.health_result', { healthy, total }),
          });
        },
      },

      // ═══════════════════════════════════════════════════════════════
      // Terminal Operations
      // ═══════════════════════════════════════════════════════════════
      {
        id: 'cmd:shell_launcher',
        label: t('command_palette.cmd_shell_launcher'),
        section: 'commands',
        icon: <Clapperboard className="h-4 w-4" />,
        shortcut: getCommandShortcut('cmd:shell_launcher'),
        action: () => {
          window.dispatchEvent(new CustomEvent('oxideterm:shell-launcher'));
        },
      },
      {
        id: 'cmd:detach_terminal',
        label: t('command_palette.cmd_detach_terminal'),
        section: 'commands',
        icon: <Paperclip className="h-4 w-4" />,
        action: async () => {
          const { activeTabId, tabs } = useAppStore.getState();
          const tab = tabs.find((t) => t.id === activeTabId);
          if (tab?.type === 'local_terminal' && tab.sessionId) {
            await useLocalTerminalStore.getState().detachTerminal(tab.sessionId);
          }
        },
      },
      {
        id: 'cmd:cleanup_dead',
        label: t('command_palette.cmd_cleanup_dead'),
        section: 'commands',
        icon: <Trash2 className="h-4 w-4" />,
        action: async () => {
          const removed = await useLocalTerminalStore.getState().cleanupDeadSessions();
          toast({ title: t('command_palette.cleanup_result', { count: removed.length }) });
        },
      },
      {
        id: 'cmd:toggle_fps',
        label: t('command_palette.cmd_toggle_fps'),
        section: 'commands',
        icon: <Gauge className="h-4 w-4" />,
        action: () => {
          const s = useSettingsStore.getState();
          s.updateTerminal('showFpsOverlay', !s.settings.terminal.showFpsOverlay);
        },
      },

      // ═══════════════════════════════════════════════════════════════
      // Pane Management
      // ═══════════════════════════════════════════════════════════════
      {
        id: 'cmd:close_pane',
        label: t('command_palette.cmd_close_pane'),
        section: 'commands',
        icon: <PanelBottomClose className="h-4 w-4" />,
        action: () => {
          const { activeTabId, tabs } = useAppStore.getState();
          const tab = tabs.find((t) => t.id === activeTabId);
          if (tab?.activePaneId && activeTabId) {
            useAppStore.getState().closePane(activeTabId, tab.activePaneId);
          }
        },
      },
      {
        id: 'cmd:reset_panes',
        label: t('command_palette.cmd_reset_panes'),
        section: 'commands',
        icon: <SquareStack className="h-4 w-4" />,
        action: () => {
          const id = useAppStore.getState().activeTabId;
          if (id) useAppStore.getState().resetToSinglePane(id);
        },
      },
      {
        id: 'cmd:focus_next_pane',
        label: t('command_palette.cmd_focus_next_pane'),
        section: 'commands',
        icon: <Focus className="h-4 w-4" />,
        action: () => {
          const { activeTabId, tabs } = useAppStore.getState();
          const tab = tabs.find((t) => t.id === activeTabId);
          if (!tab?.rootPane || !activeTabId) return;
          // Collect all leaf pane IDs from the tree
          const collectLeafIds = (node: PaneNode): string[] =>
            node.type === 'leaf' ? [node.id] : node.children.flatMap(collectLeafIds);
          const leafIds = collectLeafIds(tab.rootPane);
          if (leafIds.length < 2) return;
          const curIdx = leafIds.indexOf(tab.activePaneId ?? '');
          const nextIdx = (curIdx + 1) % leafIds.length;
          useAppStore.getState().setActivePaneId(activeTabId, leafIds[nextIdx]);
        },
      },

      // ═══════════════════════════════════════════════════════════════
      // Miscellaneous
      // ═══════════════════════════════════════════════════════════════
      {
        id: 'cmd:open_plugin_manager',
        label: t('command_palette.cmd_open_plugin_manager'),
        section: 'commands',
        icon: <Puzzle className="h-4 w-4" />,
        action: () => useAppStore.getState().createTab('plugin_manager'),
      },
      {
        id: 'cmd:open_topology',
        label: t('command_palette.cmd_open_topology'),
        section: 'commands',
        icon: <Globe className="h-4 w-4" />,
        action: () => useAppStore.getState().createTab('topology'),
      },
      {
        id: 'cmd:reset_settings',
        label: t('command_palette.cmd_reset_settings'),
        section: 'commands',
        icon: <AlertTriangle className="h-4 w-4" />,
        action: () => {
          if (window.confirm(t('command_palette.confirm_reset_settings'))) {
            useSettingsStore.getState().resetToDefaults();
          }
        },
      },
      {
        id: 'cmd:reload_window',
        label: t('command_palette.cmd_reload_window'),
        section: 'commands',
        icon: <RotateCw className="h-4 w-4" />,
        action: () => window.location.reload(),
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
        recent.push({ ...item, section: 'recent' as const, value: `recent:${item.value}` });
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
        className="max-w-[560px] p-0 gap-0 top-[15%] translate-y-0 overflow-hidden rounded-lg shadow-2xl"
        overlayClassName="bg-black/40"
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

          {/* ── Footer ── */}
          <div className="border-t border-theme-border px-3 py-1.5 text-[11px] text-theme-text-muted select-none flex items-center gap-3">
            <span>{t('command_palette.footer_hint')}</span>
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
