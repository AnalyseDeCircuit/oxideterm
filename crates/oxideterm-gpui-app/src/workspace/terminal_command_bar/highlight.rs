// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::root::init::{terminal_highlight_rules, terminal_preference_overrides};
use gpui::Div;
use oxideterm_settings_model::TerminalSettingsPage;

const TERMINAL_HIGHLIGHT_POPOVER_WIDTH: f32 = 340.0;
const TERMINAL_HIGHLIGHT_POPOVER_BOTTOM: f32 = 44.0;

impl WorkspaceApp {
    pub(super) fn active_terminal_highlight_override(&self, cx: &App) -> bool {
        self.active_pane(cx).is_some_and(|pane| {
            let pane = pane.read(cx);
            pane.session_highlight_rule_set_id().is_some()
                || pane
                    .preference_overrides_snapshot()
                    .highlight_rule_set_id
                    .is_some()
        })
    }

    fn active_terminal_saved_profile_id(&self, cx: &App) -> Option<String> {
        let session_id = self.active_terminal_session_id(cx)?;
        if let Some(profile_id) = self.telnet_terminal_profile_ids.get(&session_id) {
            return Some(profile_id.clone());
        }
        let node_id = self
            .workspace_runtime
            .read(cx)
            .ssh_terminal_node_id(session_id)?;
        self.ssh_nodes.get(&node_id)?.saved_connection_id.clone()
    }

    fn apply_active_saved_highlight_preferences(
        &mut self,
        saved_profile_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(session_id) = self.active_terminal_session_id(cx) else {
            return;
        };
        if self
            .telnet_terminal_profile_ids
            .get(&session_id)
            .is_some_and(|profile_id| profile_id == saved_profile_id)
        {
            let Some(terminal_options) = self
                .connection_store
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == saved_profile_id)
                .map(|profile| profile.terminal.clone())
            else {
                return;
            };
            let preference_overrides = terminal_preference_overrides(
                terminal_options,
                &self.settings_store.settings().terminal,
            );
            let Some(pane_id) = self.active_pane_id(cx) else {
                return;
            };
            let Some(pane) = self.active_pane(cx) else {
                return;
            };
            let application_preferences = self.terminal_preferences_for_pane(pane_id, cx);
            pane.update(cx, |pane, cx| {
                pane.set_preference_overrides(preference_overrides, application_preferences, cx);
            });
            return;
        }

        self.apply_saved_connection_terminal_preferences(saved_profile_id, cx);
    }

    fn active_highlight_source_label(&self, cx: &App) -> String {
        let Some(pane) = self.active_pane(cx) else {
            return self
                .i18n
                .t("terminal.highlight_override.source_global_base");
        };
        let pane = pane.read(cx);
        if let Some(id) = pane.session_highlight_rule_set_id() {
            let name = self.highlight_rule_set_name(id);
            return self
                .i18n
                .t("terminal.highlight_override.source_session")
                .replace("{{name}}", &name);
        }
        if let Some(id) = pane
            .preference_overrides_snapshot()
            .highlight_rule_set_id
            .as_deref()
        {
            let name = self.highlight_rule_set_name(id);
            return self
                .i18n
                .t("terminal.highlight_override.source_connection")
                .replace("{{name}}", &name);
        }
        let name = self
            .settings_store
            .settings()
            .terminal
            .default_highlight_rule_set_name()
            .map(str::to_string)
            .unwrap_or_else(|| {
                self.i18n
                    .t("settings_view.terminal.highlight_rules.rule_set_global_base")
            });
        self.i18n
            .t("terminal.highlight_override.source_global")
            .replace("{{name}}", &name)
    }

    fn highlight_rule_set_name(&self, id: &str) -> String {
        if id == GLOBAL_HIGHLIGHT_RULE_SET_ID {
            return self
                .settings_store
                .settings()
                .terminal
                .default_highlight_rule_set_name()
                .map(str::to_string)
                .unwrap_or_else(|| {
                    self.i18n
                        .t("settings_view.terminal.highlight_rules.rule_set_global_base")
                });
        }
        self.settings_store
            .settings()
            .terminal
            .highlight_rule_set(id)
            .map(|rule_set| rule_set.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    pub(super) fn toggle_terminal_highlight_popover(&mut self, cx: &mut Context<Self>) {
        self.terminal_highlight_popover_open = !self.terminal_highlight_popover_open;
        if self.terminal_highlight_popover_open {
            self.close_terminal_quick_commands_popover(cx);
            self.dismiss_terminal_broadcast_menu(cx);
            self.close_terminal_cwd_picker(cx);
            self.close_terminal_git_branch_picker(cx);
            self.close_terminal_project_panel(cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn dismiss_terminal_highlight_popover(&mut self) -> bool {
        std::mem::take(&mut self.terminal_highlight_popover_open)
    }

    fn apply_active_session_highlight_rule_set(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(pane_id) = self.active_pane_id(cx) else {
            return;
        };
        let Some(pane) = self.active_pane(cx) else {
            return;
        };
        let settings = self.settings_store.settings();
        let rules = if id == GLOBAL_HIGHLIGHT_RULE_SET_ID {
            settings.terminal.effective_highlight_rules()
        } else {
            let Some(rule_set) = settings.terminal.highlight_rule_set(&id) else {
                return;
            };
            &rule_set.rules
        };
        let highlight_override = TerminalHighlightRuleSetOverride {
            id,
            rules: terminal_highlight_rules(rules),
        };
        let preferences = self.terminal_preferences_for_pane(pane_id, cx);
        pane.update(cx, |pane, cx| {
            pane.set_session_highlight_override(Some(highlight_override), preferences, cx);
        });
        cx.notify();
    }

    fn clear_active_session_highlight_override(&mut self, cx: &mut Context<Self>) {
        let Some(pane_id) = self.active_pane_id(cx) else {
            return;
        };
        let Some(pane) = self.active_pane(cx) else {
            return;
        };
        let preferences = self.terminal_preferences_for_pane(pane_id, cx);
        pane.update(cx, |pane, cx| {
            pane.set_session_highlight_override(None, preferences, cx);
        });
        cx.notify();
    }

    fn save_active_highlight_override_to_connection(&mut self, cx: &mut Context<Self>) {
        let Some(saved_profile_id) = self.active_terminal_saved_profile_id(cx) else {
            return;
        };
        let Some(rule_set_id) = self.active_pane(cx).and_then(|pane| {
            pane.read(cx)
                .session_highlight_rule_set_id()
                .map(str::to_string)
        }) else {
            return;
        };
        let saved_rule_set_id =
            (rule_set_id != GLOBAL_HIGHLIGHT_RULE_SET_ID).then_some(rule_set_id);
        match self
            .connection_store
            .set_terminal_highlight_rule_set(&saved_profile_id, saved_rule_set_id)
        {
            Ok(true) => {
                self.queue_cloud_sync_dirty_refresh(cx);
                self.clear_active_session_highlight_override(cx);
                self.apply_active_saved_highlight_preferences(&saved_profile_id, cx);
                self.send_settings_notice(
                    self.i18n
                        .t("terminal.highlight_override.saved_to_connection"),
                    TerminalNoticeVariant::Success,
                    cx,
                );
            }
            Ok(false) => {}
            Err(error) => {
                self.send_settings_notice(error.to_string(), TerminalNoticeVariant::Error, cx)
            }
        }
    }

    fn reset_active_connection_highlight_override(&mut self, cx: &mut Context<Self>) {
        let Some(saved_profile_id) = self.active_terminal_saved_profile_id(cx) else {
            return;
        };
        match self
            .connection_store
            .set_terminal_highlight_rule_set(&saved_profile_id, None)
        {
            Ok(true) => {
                self.queue_cloud_sync_dirty_refresh(cx);
                self.clear_active_session_highlight_override(cx);
                self.apply_active_saved_highlight_preferences(&saved_profile_id, cx);
            }
            Ok(false) => {}
            Err(error) => {
                self.send_settings_notice(error.to_string(), TerminalNoticeVariant::Error, cx)
            }
        }
    }

    fn open_highlight_rule_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal_highlight_popover_open = false;
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Terminal, cx);
            settings.set_terminal_page(TerminalSettingsPage::Highlight, cx);
        });
        self.open_settings(window, cx);
    }

    pub(super) fn render_terminal_highlight_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let active_pane = self.active_pane(cx);
        let session_rule_set_id = active_pane.as_ref().and_then(|pane| {
            pane.read(cx)
                .session_highlight_rule_set_id()
                .map(str::to_string)
        });
        let connection_rule_set_id = active_pane.as_ref().and_then(|pane| {
            pane.read(cx)
                .preference_overrides_snapshot()
                .highlight_rule_set_id
        });
        let saved_profile_id = self.active_terminal_saved_profile_id(cx);
        let settings = self.settings_store.settings();
        let anchor_left = self
            .select_anchors
            .get(&SelectAnchorId::TerminalHighlightRuleSet)
            .map(|anchor| {
                (f32::from(anchor.bounds.right()) - TERMINAL_HIGHLIGHT_POPOVER_WIDTH).max(12.0)
            });
        let mut popover = context_menu_event_boundary({
            let popover = div()
                .absolute()
                .bottom(px(TERMINAL_HIGHLIGHT_POPOVER_BOTTOM))
                .w(px(TERMINAL_HIGHLIGHT_POPOVER_WIDTH))
                .max_h(px(460.0))
                .overflow_hidden()
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg_elevated << 8) | 0xf7))
                .shadow_lg()
                .p(px(8.0))
                .text_size(px(12.0));
            if let Some(left) = anchor_left {
                popover.left(px(left))
            } else {
                popover.right(px(12.0))
            }
        })
        .child(
            div()
                .px(px(8.0))
                .py(px(6.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t("terminal.highlight_override.title")),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme.text_muted))
                        .child(self.active_highlight_source_label(cx)),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("terminal.highlight_override.override_hint")),
                ),
        )
        .child(self.card_separator());

        let mut choices = div().w_full().flex().flex_col();
        let inherited_selected = session_rule_set_id.is_none();
        choices = choices.child(self.terminal_highlight_rule_set_row(
            self.i18n.t("terminal.highlight_override.use_inherited"),
            inherited_selected,
            cx.listener(|this, _event, _window, cx| {
                this.clear_active_session_highlight_override(cx);
                cx.stop_propagation();
            }),
        ));

        let global_name = settings
            .terminal
            .default_highlight_rule_set_name()
            .map(str::to_string)
            .unwrap_or_else(|| {
                self.i18n
                    .t("settings_view.terminal.highlight_rules.rule_set_global_base")
            });
        choices = choices.child(
            self.terminal_highlight_rule_set_row(
                self.i18n
                    .t("terminal.highlight_override.use_global_for_session")
                    .replace("{{name}}", &global_name),
                session_rule_set_id.as_deref() == Some(GLOBAL_HIGHLIGHT_RULE_SET_ID),
                cx.listener(|this, _event, _window, cx| {
                    this.apply_active_session_highlight_rule_set(
                        GLOBAL_HIGHLIGHT_RULE_SET_ID.to_string(),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ),
        );

        for rule_set in &settings.terminal.highlight_rule_sets {
            let id = rule_set.id.clone();
            choices = choices.child(self.terminal_highlight_rule_set_row(
                rule_set.name.clone(),
                session_rule_set_id.as_deref() == Some(id.as_str()),
                cx.listener(move |this, _event, _window, cx| {
                    this.apply_active_session_highlight_rule_set(id.clone(), cx);
                    cx.stop_propagation();
                }),
            ));
        }

        popover = popover
            .child(choices.max_h(px(300.0)).overflow_y_scrollbar())
            .child(self.card_separator());
        if saved_profile_id.is_some() && session_rule_set_id.is_some() {
            popover = popover.child(
                self.terminal_highlight_action_row(
                    LucideIcon::Save,
                    self.i18n
                        .t("terminal.highlight_override.save_to_connection"),
                    cx.listener(|this, _event, _window, cx| {
                        this.save_active_highlight_override_to_connection(cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        if saved_profile_id.is_some() && connection_rule_set_id.is_some() {
            popover = popover.child(self.terminal_highlight_action_row(
                LucideIcon::RotateCcw,
                self.i18n.t("terminal.highlight_override.reset_connection"),
                cx.listener(|this, _event, _window, cx| {
                    this.reset_active_connection_highlight_override(cx);
                    cx.stop_propagation();
                }),
            ));
        }
        popover
            .child(self.terminal_highlight_action_row(
                LucideIcon::Settings,
                self.i18n.t("terminal.highlight_override.manage_rule_sets"),
                cx.listener(|this, _event, window, cx| {
                    this.open_highlight_rule_settings(window, cx);
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    fn terminal_highlight_rule_set_row(
        &self,
        label: String,
        selected: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let theme = self.tokens.ui;
        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .hover(|row| row.bg(rgba((theme.bg_hover << 8) | 0xb3)))
            .child(if selected {
                Self::render_lucide_icon(LucideIcon::Check, 13.0, rgb(theme.accent))
            } else {
                div().size(px(13.0)).into_any_element()
            })
            .child(div().flex_1().truncate().child(label))
            .on_mouse_down(MouseButton::Left, listener)
    }

    fn terminal_highlight_action_row(
        &self,
        icon: LucideIcon,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let theme = self.tokens.ui;
        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .text_color(rgb(theme.text))
            .hover(|row| row.bg(rgba((theme.bg_hover << 8) | 0xb3)))
            .child(Self::render_lucide_icon(icon, 13.0, rgb(theme.text_muted)))
            .child(label)
            .on_mouse_down(MouseButton::Left, listener)
    }
}
