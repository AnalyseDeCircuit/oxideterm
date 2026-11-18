use gpui::{Context, Entity, Subscription};
use oxideterm_gpui_ui::motion::ExitPhase;

use crate::entity::SettingsWorkspaceEntity;

/// Window commands and modal ownership depend on these transitions, not on page drafts.
#[derive(Default, PartialEq, Eq)]
pub struct SettingsWindowChrome {
    pub data_directory: Option<ExitPhase>,
    pub theme_editor: Option<ExitPhase>,
    pub command_specs: bool,
    pub ssh_import: Option<ExitPhase>,
    pub portable_password: Option<ExitPhase>,
    pub managed_key: Option<ExitPhase>,
    pub keybinding_reset: Option<ExitPhase>,
    pub navigation_editor: bool,
    pub terminal_trigger_editor: bool,
    pub semantic_scheme_editor: bool,
    pub highlight_rule_editor_index: Option<usize>,
    pub recording_keybinding: bool,
    pub public_mcp_client: Option<String>,
}

impl SettingsWindowChrome {
    fn from_settings(settings: &SettingsWorkspaceEntity) -> Self {
        Self {
            data_directory: settings
                .data_directory_confirm()
                .is_some()
                .then(|| settings.data_directory_confirm_phase()),
            theme_editor: settings
                .theme_editor_open()
                .then(|| settings.theme_editor_phase()),
            command_specs: settings.command_specs_open(),
            ssh_import: settings
                .ssh_config_import_dialog_open()
                .then(|| settings.ssh_config_import_dialog_phase()),
            portable_password: settings
                .portable_password_dialog_open()
                .then(|| settings.portable_password_dialog_phase()),
            managed_key: settings
                .managed_key_dialog_open()
                .then(|| settings.managed_key_dialog_phase()),
            keybinding_reset: settings
                .keybinding_reset_confirm_snapshot()
                .map(|reset| reset.phase),
            navigation_editor: settings.navigation_editor_open(),
            terminal_trigger_editor: settings.terminal_triggers.editor_open,
            semantic_scheme_editor: settings.highlight.semantic_scheme_editor_open,
            highlight_rule_editor_index: settings.highlight.highlight_rule_editor_index,
            recording_keybinding: settings.keybinding_recording_action_id().is_some(),
            public_mcp_client: settings.public_mcp_client_editor.clone(),
        }
    }
}

pub struct SettingsWindowChromeEntity {
    pub state: SettingsWindowChrome,
    _settings_changed: Subscription,
}

impl SettingsWindowChromeEntity {
    pub fn new(settings: &Entity<SettingsWorkspaceEntity>, cx: &mut Context<Self>) -> Self {
        let state = SettingsWindowChrome::from_settings(settings.read(cx));
        let settings_changed = cx.observe(settings, |chrome, settings, cx| {
            let state = SettingsWindowChrome::from_settings(settings.read(cx));
            if chrome.state != state {
                chrome.state = state;
                cx.notify();
            }
        });
        Self {
            state,
            _settings_changed: settings_changed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    #[gpui::test]
    fn trigger_and_highlight_edits_keep_window_chrome_stable(cx: &mut TestAppContext) {
        use crate::{SettingsInput, entity::SettingsWorkspaceEvent};

        let settings = cx.new(SettingsWorkspaceEntity::new);
        let chrome = cx.new(|cx| SettingsWindowChromeEntity::new(&settings, cx));
        let notifications = Rc::new(Cell::new(0));
        let _subscription = cx.update(|cx| {
            cx.observe(&chrome, {
                let notifications = Rc::clone(&notifications);
                move |_, _| notifications.set(notifications.get() + 1)
            })
        });
        settings.update(cx, |settings, cx| {
            settings.terminal_triggers.new_rule();
            cx.notify();
        });
        assert!(chrome.read_with(cx, |chrome, _| chrome.state.terminal_trigger_editor));
        let before = notifications.get();
        settings.update(cx, |settings, cx| {
            let input = SettingsInput::TerminalTriggerDelayMs;
            settings.focus_settings_entity_input(input, cx);
            settings.replace_settings_entity_input(input, Some(0..1), "invalid", cx);
            assert_eq!(settings.settings_entity_input_value(input), Some("invalid"));
            assert!(!settings.apply_active_trigger_input());
            assert_eq!(
                settings
                    .terminal_triggers
                    .editor
                    .as_ref()
                    .unwrap()
                    .timing
                    .delay_ms,
                0
            );
            settings.replace_settings_entity_input(input, Some(0..7), "350", cx);
            assert_eq!(
                settings
                    .terminal_triggers
                    .editor
                    .as_ref()
                    .unwrap()
                    .timing
                    .delay_ms,
                350
            );
        });
        assert_eq!(notifications.get(), before);

        let mut events = cx.events(&settings);
        let input = SettingsInput::HighlightPattern(0);
        settings.update(cx, |settings, cx| {
            settings.focus_highlight_input(input, "warn".to_string(), cx);
            settings.replace_settings_entity_input(input, Some(0..4), "error", cx);
            assert_eq!(settings.settings_entity_input_value(input), Some("error"));
        });
        assert_eq!(
            events.try_recv().unwrap(),
            SettingsWorkspaceEvent::HighlightDraftChanged(input)
        );
        assert_eq!(notifications.get(), before);
    }

    #[gpui::test]
    fn highlight_preference_refresh_waits_for_the_latest_edit(cx: &mut TestAppContext) {
        use crate::entity::SettingsWorkspaceEvent;
        use std::time::Duration;

        let settings = cx.new(SettingsWorkspaceEntity::new);
        let mut events = cx.events(&settings);
        settings.update(
            cx,
            SettingsWorkspaceEntity::queue_highlight_preferences_refresh,
        );
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
        settings.update(
            cx,
            SettingsWorkspaceEntity::queue_highlight_preferences_refresh,
        );
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        assert!(events.try_recv().is_err());

        cx.executor().advance_clock(Duration::from_millis(60));
        cx.run_until_parked();
        assert_eq!(
            events.try_recv().unwrap(),
            SettingsWorkspaceEvent::RefreshTerminalHighlightPreferences
        );
        assert!(events.try_recv().is_err());
    }

    #[gpui::test]
    fn window_chrome_updates_for_dialog_transitions_not_input_edits(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        let chrome = cx.new(|cx| SettingsWindowChromeEntity::new(&settings, cx));
        let notifications = Rc::new(Cell::new(0));
        let _subscription = cx.update(|cx| {
            cx.observe(&chrome, {
                let notifications = Rc::clone(&notifications);
                move |_, _| notifications.set(notifications.get() + 1)
            })
        });
        let input = crate::SettingsInput::KeybindingSearch;
        settings.update(cx, |settings, cx| {
            assert!(settings.focus_settings_entity_input(input, cx));
            assert!(settings.replace_settings_entity_input(input, None, "copy", cx));
        });
        assert_eq!(notifications.get(), 0);

        settings.update(cx, |settings, cx| {
            settings.start_keybinding_recording("app.copy", cx)
        });
        assert!(chrome.read_with(cx, |chrome, _| chrome.state.recording_keybinding));
        let recording_notification = notifications.get();
        assert_eq!(recording_notification, 1);
        settings.update(cx, |settings, cx| {
            settings.open_keybinding_reset_confirm(cx);
        });
        assert_eq!(
            chrome.read_with(cx, |chrome, _| chrome.state.keybinding_reset),
            Some(ExitPhase::Visible)
        );
        assert_eq!(notifications.get(), recording_notification + 1);

        let before_editor = notifications.get();
        settings.update(cx, |settings, cx| {
            settings.open_command_specs(
                "commands.json".into(),
                "1 command".into(),
                "{}".into(),
                cx,
            );
        });
        assert!(chrome.read_with(cx, |chrome, _| chrome.state.command_specs));
        assert_eq!(notifications.get(), before_editor + 1);

        let input = crate::SettingsInput::TerminalCommandSpecsJson;
        settings.update(cx, |settings, cx| {
            assert!(settings.replace_settings_entity_input(
                input,
                Some(0..2),
                r#"{"specs":[]}"#,
                cx
            ));
            assert_eq!(
                settings.settings_entity_input_value(input),
                Some(r#"{"specs":[]}"#)
            );
        });
        assert_eq!(notifications.get(), before_editor + 1);

        settings.update(cx, |settings, cx| {
            assert!(settings.close_command_specs(cx));
            assert_eq!(settings.settings_entity_input_value(input), None);
        });
        assert!(!chrome.read_with(cx, |chrome, _| chrome.state.command_specs));
        assert_eq!(notifications.get(), before_editor + 2);
    }
}
