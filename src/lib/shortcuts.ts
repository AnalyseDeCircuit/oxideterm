// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

/**
 * Shared keyboard shortcuts data for Help views and ShortcutsModal.
 *
 * Reads effective bindings from keybindingRegistry (respects user overrides).
 * Context-specific shortcuts (file manager, SFTP, editor) remain hardcoded
 * since they are not part of the global keybinding registry.
 */

import type { TFunction } from 'i18next';
import { type ActionId, getDisplayBinding } from '@/lib/keybindingRegistry';

export type ShortcutEntry = {
  label: string;
  mac: string;
  other: string;
};

export type ShortcutCategory = {
  id: string;
  title: string;
  shortcuts: ShortcutEntry[];
};

/** Build a ShortcutEntry from the keybinding registry. */
function fromRegistry(actionId: ActionId, label: string): ShortcutEntry {
  const binding = getDisplayBinding(actionId);
  return {
    label,
    mac: binding?.mac ?? '',
    other: binding?.other ?? '',
  };
}

/**
 * Build the full shortcut categories list using the current t() function.
 * Both SettingsView and KeyboardShortcutsModal call this.
 */
export function getShortcutCategories(t: TFunction): ShortcutCategory[] {
  return [
    {
      id: 'app',
      title: t('settings_view.help.category_app'),
      shortcuts: [
        fromRegistry('app.newTerminal', t('settings_view.help.shortcut_new_tab')),
        fromRegistry('app.shellLauncher', t('settings_view.help.shortcut_shell_launcher')),
        fromRegistry('app.closeTab', t('settings_view.help.shortcut_close_tab')),
        fromRegistry('app.closeOtherTabs', t('settings_view.help.shortcut_close_other_tabs')),
        fromRegistry('app.nextTab', t('settings_view.help.shortcut_next_tab')),
        fromRegistry('app.prevTab', t('settings_view.help.shortcut_prev_tab')),
        // goToTab is a range (1-9); show the base binding with "1-9" suffix
        (() => {
          const tab1 = getDisplayBinding('app.goToTab1');
          return {
            label: t('settings_view.help.shortcut_go_to_tab'),
            mac: tab1 ? tab1.mac.replace(/1$/, '') + '1-9' : '',
            other: tab1 ? tab1.other.replace(/1$/, '') + '1-9' : '',
          };
        })(),
        fromRegistry('app.newConnection', t('settings_view.help.shortcut_new_connection')),
        fromRegistry('app.navBack', t('settings_view.help.shortcut_nav_back')),
        fromRegistry('app.navForward', t('settings_view.help.shortcut_nav_forward')),
        fromRegistry('app.commandPalette', t('settings_view.help.shortcut_command_palette')),
        fromRegistry('app.toggleSidebar', t('settings_view.help.shortcut_toggle_sidebar')),
        fromRegistry('app.settings', t('settings_view.help.shortcut_settings')),
        fromRegistry('app.zenMode', t('settings_view.help.shortcut_zen_mode')),
        fromRegistry('app.showShortcuts', t('settings_view.help.shortcut_keyboard_shortcuts')),
        fromRegistry('app.fontIncrease', t('settings_view.help.shortcut_font_increase')),
        fromRegistry('app.fontDecrease', t('settings_view.help.shortcut_font_decrease')),
        fromRegistry('app.fontReset', t('settings_view.help.shortcut_font_reset')),
      ],
    },
    {
      id: 'terminal',
      title: t('settings_view.help.category_terminal'),
      shortcuts: [
        fromRegistry('terminal.search', t('settings_view.help.shortcut_find')),
        fromRegistry('terminal.paste', t('settings_view.help.shortcut_paste')),
        fromRegistry('terminal.aiPanel', t('settings_view.help.shortcut_ai_panel')),
        fromRegistry('terminal.recording', t('settings_view.help.shortcut_recording')),
        fromRegistry('terminal.closePanel', t('settings_view.help.shortcut_close_panel')),
      ],
    },
    {
      id: 'split',
      title: t('settings_view.help.category_split'),
      shortcuts: [
        fromRegistry('split.horizontal', t('settings_view.help.shortcut_split_h')),
        fromRegistry('split.vertical', t('settings_view.help.shortcut_split_v')),
        fromRegistry('split.closePane', t('settings_view.help.shortcut_close_pane')),
        // navPane is a group (left/right); show modifier prefix + "Arrow"
        (() => {
          const navL = getDisplayBinding('split.navLeft');
          return {
            label: t('settings_view.help.shortcut_nav_pane'),
            mac: navL ? navL.mac.replace(/←$/, '') + 'Arrow' : '',
            other: navL ? navL.other.replace(/←$/, '') + 'Arrow' : '',
          };
        })(),
      ],
    },
    {
      id: 'file_manager',
      title: t('settings_view.help.category_file_manager'),
      shortcuts: [
        { label: t('settings_view.help.shortcut_select_all'), mac: '⌘A', other: 'Ctrl+A' },
        { label: t('settings_view.help.shortcut_copy'), mac: '⌘C', other: 'Ctrl+C' },
        { label: t('settings_view.help.shortcut_cut'), mac: '⌘X', other: 'Ctrl+X' },
        { label: t('settings_view.help.shortcut_paste'), mac: '⌘V', other: 'Ctrl+V' },
        { label: t('settings_view.help.shortcut_rename'), mac: 'F2', other: 'F2' },
        { label: t('settings_view.help.shortcut_delete'), mac: 'Delete', other: 'Delete' },
        { label: t('settings_view.help.shortcut_quick_look'), mac: 'Space', other: 'Space' },
        { label: t('settings_view.help.shortcut_open'), mac: 'Enter', other: 'Enter' },
      ],
    },
    {
      id: 'sftp',
      title: t('settings_view.help.category_sftp'),
      shortcuts: [
        { label: t('settings_view.help.shortcut_select_all'), mac: '⌘A', other: 'Ctrl+A' },
        { label: t('settings_view.help.shortcut_quick_look'), mac: 'Space', other: 'Space' },
        { label: t('settings_view.help.shortcut_sftp_enter_dir'), mac: 'Enter', other: 'Enter' },
        { label: t('settings_view.help.shortcut_sftp_upload'), mac: '→', other: '→' },
        { label: t('settings_view.help.shortcut_sftp_download'), mac: '←', other: '←' },
        { label: t('settings_view.help.shortcut_rename'), mac: 'F2', other: 'F2' },
        { label: t('settings_view.help.shortcut_delete'), mac: 'Delete', other: 'Delete' },
      ],
    },
    {
      id: 'editor',
      title: t('settings_view.help.category_editor'),
      shortcuts: [
        { label: t('settings_view.help.shortcut_save'), mac: '⌘S', other: 'Ctrl+S' },
        { label: t('settings_view.help.shortcut_close'), mac: 'Esc', other: 'Esc' },
      ],
    },
    {
      id: 'palette',
      title: t('settings_view.help.category_palette'),
      shortcuts: [
        fromRegistry('palette.eventLog', t('settings_view.help.shortcut_event_log')),
        fromRegistry('palette.aiSidebar', t('settings_view.help.shortcut_ai_sidebar')),
        fromRegistry('palette.broadcast', t('settings_view.help.shortcut_broadcast')),
      ],
    },
  ];
}
