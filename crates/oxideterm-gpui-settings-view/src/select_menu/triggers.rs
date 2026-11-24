use super::*;
use oxideterm_terminal_triggers::{
    LocalProcessSpec, TerminalTriggerAction, TerminalTriggerDispatch, TerminalTriggerMatchMode,
    TerminalTriggerScope,
};

impl SettingsSelectMenu<'_> {
    pub(super) fn trigger_popup(
        &self,
        open_select: SettingsSelect,
        width: f32,
        cx: &App,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        match open_select {
            SettingsSelect::TerminalTriggerMatchMode => {
                let draft = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft()?;
                let current = draft.matcher.mode;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for mode in [
                    TerminalTriggerMatchMode::Literal,
                    TerminalTriggerMatchMode::Regex,
                ] {
                    let label = self.i18n.t(match mode {
                        TerminalTriggerMatchMode::Literal => {
                            "settings_view.terminal.triggers.literal"
                        }
                        TerminalTriggerMatchMode::Regex => "settings_view.terminal.triggers.regex",
                    });
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, mode == current),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_match_mode(mode)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            SettingsSelect::TerminalTriggerAction | SettingsSelect::TerminalTriggerClickAction => {
                let draft = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft()?;
                let nested = open_select == SettingsSelect::TerminalTriggerClickAction;
                let action = if nested {
                    self.settings_workspace
                        .read(cx)
                        .terminal_triggers
                        .terminal_trigger_draft_action()?
                } else {
                    &draft.action
                };
                let current = match action {
                    TerminalTriggerAction::SendText { .. } => 0,
                    TerminalTriggerAction::RunQuickCommand { .. } => 1,
                    TerminalTriggerAction::LaunchLocalProcess { .. } => 2,
                    TerminalTriggerAction::OpenUrl { .. } => 3,
                    TerminalTriggerAction::CustomLink { .. } => 4,
                    TerminalTriggerAction::CustomMenu { .. } => 5,
                    TerminalTriggerAction::ShowTooltip { .. } => 6,
                };
                let mut popup = select_overlay_popup(&self.tokens, width);
                for action_index in if nested {
                    vec![0, 1, 2, 3]
                } else {
                    vec![0, 1, 2, 4, 5, 6]
                } {
                    let label_key = match action_index {
                        0 => "settings_view.terminal.triggers.action_send_text",
                        1 => "settings_view.terminal.triggers.action_quick_command",
                        2 => "settings_view.terminal.triggers.action_local_process",
                        3 => "settings_view.terminal.triggers.open_url",
                        4 => "settings_view.terminal.triggers.custom_link",
                        5 => "settings_view.terminal.triggers.custom_menu",
                        _ => "settings_view.terminal.triggers.show_tooltip",
                    };
                    let first_command = self.first_command.clone();
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t(label_key),
                            action_index == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            let action = match action_index {
                                0 => TerminalTriggerAction::SendText {
                                    text: String::new(),
                                    append_enter: false,
                                },
                                1 => {
                                    let quick_command_id = first_command.clone();
                                    TerminalTriggerAction::RunQuickCommand { quick_command_id }
                                }
                                2 => TerminalTriggerAction::LaunchLocalProcess {
                                    process: LocalProcessSpec::DirectProgram {
                                        executable: String::new(),
                                        arguments: Vec::new(),
                                        working_directory: None,
                                    },
                                },
                                3 => TerminalTriggerAction::OpenUrl { url: String::new() },
                                4 => TerminalTriggerAction::CustomLink {
                                    action: Box::new(TerminalTriggerAction::OpenUrl {
                                        url: String::new(),
                                    }),
                                },
                                5 => TerminalTriggerAction::CustomMenu {
                                    items: vec![
                                        oxideterm_terminal_triggers::TerminalTriggerMenuItem {
                                            label: String::new(),
                                            action: TerminalTriggerAction::OpenUrl {
                                                url: String::new(),
                                            },
                                        },
                                    ],
                                },
                                _ => TerminalTriggerAction::ShowTooltip {
                                    text: String::new(),
                                },
                            };
                            if nested {
                                this.send(
                                    SelectMenuRequest::Trigger(
                                        true,
                                        Box::new(move |state| {
                                            state.select_terminal_trigger_click_action(action)
                                        }),
                                    ),
                                    _window,
                                    cx,
                                );
                            } else {
                                this.send(
                                    SelectMenuRequest::Trigger(
                                        true,
                                        Box::new(move |state| {
                                            state.select_terminal_trigger_action(action)
                                        }),
                                    ),
                                    _window,
                                    cx,
                                );
                            }
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            SettingsSelect::TerminalTriggerProcessMode => {
                let action = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft_action()?;
                let current_shell = matches!(
                    action,
                    TerminalTriggerAction::LaunchLocalProcess {
                        process: LocalProcessSpec::ExplicitShell { .. }
                    }
                );
                let mut popup = select_overlay_popup(&self.tokens, width);
                for shell in [false, true] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t(if shell {
                                "settings_view.terminal.triggers.explicit_shell"
                            } else {
                                "settings_view.terminal.triggers.direct_program"
                            }),
                            shell == current_shell,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_process_mode(shell)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            SettingsSelect::TerminalTriggerQuickCommand => {
                let current = match self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft_action()?
                {
                    TerminalTriggerAction::RunQuickCommand { quick_command_id } => {
                        quick_command_id.clone()
                    }
                    _ => String::new(),
                };
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width,
                    self.tokens.metrics.settings_theme_select_popup_max_height,
                );
                for (id, name) in &self.trigger_commands {
                    let id = id.clone();
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, name.clone(), id == current),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            let id = id.clone();
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_quick_command(id)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            SettingsSelect::TerminalTriggerTiming => {
                let draft = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft()?;
                let current = draft.timing.dispatch;
                let mut popup = select_overlay_popup(&self.tokens, width);
                for dispatch in [
                    TerminalTriggerDispatch::Immediate,
                    TerminalTriggerDispatch::AfterNextLineBreak,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n.t(match dispatch {
                                TerminalTriggerDispatch::Immediate => {
                                    "settings_view.terminal.triggers.immediate"
                                }
                                TerminalTriggerDispatch::AfterNextLineBreak => {
                                    "settings_view.terminal.triggers.after_next_line_break"
                                }
                            }),
                            dispatch == current,
                        ),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_timing(dispatch)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            SettingsSelect::TerminalTriggerScope => {
                let draft = self
                    .settings_workspace
                    .read(cx)
                    .terminal_triggers
                    .terminal_trigger_draft()?;
                let current = match &draft.scope {
                    TerminalTriggerScope::AllTerminals => 0,
                    TerminalTriggerScope::LocalTerminals => 1,
                    TerminalTriggerScope::SavedConnections { .. } => 2,
                };
                let saved_connections = match &draft.scope {
                    TerminalTriggerScope::SavedConnections { connections } => connections.clone(),
                    _ => Vec::new(),
                };
                let mut popup = select_overlay_popup(&self.tokens, width);
                for scope_index in 0..3 {
                    let label_key = match scope_index {
                        0 => "settings_view.terminal.triggers.all_terminals",
                        1 => "settings_view.terminal.triggers.local_terminals",
                        _ => "settings_view.terminal.triggers.saved_connections",
                    };
                    let existing_connections = saved_connections.clone();
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(label_key), scope_index == current),
                        false,
                        false,
                        self.action(move |this, _event, _window, cx| {
                            this.send(SelectMenuRequest::Close, _window, cx);
                            let scope = match scope_index {
                                0 => TerminalTriggerScope::AllTerminals,
                                1 => TerminalTriggerScope::LocalTerminals,
                                _ => TerminalTriggerScope::SavedConnections {
                                    connections: existing_connections.clone(),
                                },
                            };
                            this.send(
                                SelectMenuRequest::Trigger(
                                    false,
                                    Box::new(move |state| {
                                        state.select_terminal_trigger_scope(scope)
                                    }),
                                ),
                                _window,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            _ => None,
        }
    }
}
