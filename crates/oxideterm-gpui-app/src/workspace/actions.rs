use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_gpui_ui::text_input::{text_caret, text_input_anchor_probe};

#[derive(Default)]
pub(super) struct SearchBarState {
    pub(super) visible: bool,
    pub(super) query: String,
    pub(super) active_match: Option<usize>,
}

impl WorkspaceApp {
    pub(super) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.visible = true;
        window.focus(&self.focus_handle);
        if let Some(pane) = self.active_pane() {
            let query = (!self.search.query.is_empty()).then(|| self.search.query.clone());
            let _ = pane.update(cx, |pane, cx| {
                pane.set_search_query(query, self.search.active_match, cx);
            });
        }
        cx.notify();
    }

    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.visible = false;
        self.search.active_match = None;
        self.ime_marked_text = None;
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.set_search_query(None, None, cx));
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn update_search_query(&mut self, cx: &mut Context<Self>) {
        let query = (!self.search.query.is_empty()).then(|| self.search.query.clone());
        self.search.active_match = query.as_ref().map(|_| 0);
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| {
                pane.set_search_query(query, self.search.active_match, cx);
            });
        }
        cx.notify();
    }

    pub(super) fn search_next(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| {
                pane.select_next_search_result(forward, cx);
            });
        }
    }

    pub(super) fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.copy_to_clipboard(cx));
        }
    }

    pub(super) fn paste(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| pane.paste_from_clipboard(cx));
        }
    }

    pub(super) fn handle_workspace_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.new_connection_form.is_some() {
            let _ = self.handle_new_connection_key(event, window, cx);
            return;
        }

        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if self.active_surface == ActiveSurface::Settings && self.open_settings_select.is_some() {
            if key == "escape" && !modifiers.platform {
                self.open_settings_select = None;
                cx.notify();
            }
            return;
        }

        if self.active_surface == ActiveSurface::Settings && self.focused_settings_input.is_some() {
            let _ = self.handle_settings_input_key(event, cx);
            return;
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::SessionManager)
            && self.session_manager.focused_input.is_some()
        {
            let _ = self.handle_session_manager_key(event, window, cx);
            return;
        }

        if self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Sftp)
        {
            let _ = self.handle_sftp_key(event, cx);
            return;
        }

        if self.terminal_command_bar_focused {
            self.handle_terminal_command_bar_key(event, window, cx);
            return;
        }

        if self.active_surface == ActiveSurface::Settings && key == "escape" && !modifiers.platform
        {
            self.close_settings(window, cx);
            return;
        }

        if self.search.visible && !modifiers.platform {
            match key {
                "escape" => self.close_search(window, cx),
                "enter" => self.search_next(!modifiers.shift, cx),
                "backspace" => {
                    self.search.query.pop();
                    self.update_search_query(cx);
                }
                _ => {}
            }
            return;
        }
    }

    pub(super) fn handle_terminal_command_bar_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform {
            return;
        }

        match key {
            "escape" => {
                self.terminal_command_bar_focused = false;
                self.ime_marked_text = None;
                self.focus_active_pane(window, cx);
                cx.notify();
            }
            "enter" => self.submit_terminal_command_bar(window, cx),
            "backspace" => {
                self.terminal_command_bar_draft.pop();
                self.ime_marked_text = None;
                cx.notify();
            }
            _ => {}
        }
    }

    pub(super) fn submit_terminal_command_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = self.terminal_command_bar_draft.trim().to_string();
        if command.is_empty() {
            return;
        }

        if let Some(pane) = self.active_pane() {
            let _ = pane.update(cx, |pane, cx| {
                pane.send_command_line(&command, cx);
            });
        }

        self.terminal_command_bar_draft.clear();
        self.ime_marked_text = None;
        if self.terminal_command_should_handoff_focus(&command) {
            self.terminal_command_bar_focused = false;
            self.focus_active_pane(window, cx);
        }
        cx.notify();
    }

    fn terminal_command_should_handoff_focus(&self, command: &str) -> bool {
        let Some(first_token) = command.split_whitespace().next() else {
            return false;
        };
        let command_name = first_token.rsplit('/').next().unwrap_or(first_token);
        self.settings_store
            .settings()
            .terminal
            .command_bar
            .focus_handoff_commands
            .iter()
            .any(|candidate| candidate == command_name)
    }

    pub(super) fn switch_locale(
        &mut self,
        locale: Locale,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.i18n.set_locale(locale);
        self.settings_store.settings_mut().general.language = settings_language_from_locale(locale);
        let _ = self.settings_store.save();
        self.sync_tab_titles(cx);
        let panes = self
            .panes
            .iter()
            .map(|(pane_id, pane)| (*pane_id, pane.clone()))
            .collect::<Vec<_>>();
        for (pane_id, pane) in panes {
            let preferences = self.terminal_preferences_for_pane(pane_id);
            let _ = pane.update(cx, |pane, cx| {
                pane.set_preferences(preferences, cx);
            });
        }

        let menus = crate::platform::app_menus(&self.i18n);
        let _ = cx.update_window(window.window_handle(), move |_root, _window, app| {
            app.set_menus(menus);
        });
        cx.notify();
    }

    pub(super) fn sync_tab_titles(&mut self, _cx: &App) {
        for tab in &mut self.tabs {
            if let TabTitleSource::I18nKey(key) = tab.title_source {
                tab.title = self.i18n.t(key);
            }
        }
    }

    pub(super) fn render_search_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::Search;
        let workspace = cx.entity();
        let query = if self.search.query.is_empty() {
            self.i18n.t("search.placeholder")
        } else {
            self.search.query.clone()
        };
        div()
            .h(px(self.tokens.metrics.searchbar_height))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .bg(rgb(theme.bg_panel))
            .border_b_1()
            .border_color(rgb(theme.border))
            .text_size(px(self.tokens.metrics.searchbar_font_size))
            .text_color(rgb(theme.text))
            .child(text_input_anchor_probe(
                target.anchor_id(),
                div()
                    .flex_1()
                    .h(px(self.tokens.metrics.search_input_height))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgb(theme.bg))
                    .text_color(if self.search.query.is_empty() {
                        rgb(theme.text_muted)
                    } else {
                        rgb(theme.text)
                    })
                    .child(query)
                    .when_some(self.marked_text_for_target(target), |input, marked| {
                        input.child(
                            div()
                                .underline()
                                .text_color(rgb(theme.text))
                                .child(marked.to_string()),
                        )
                    }),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            ))
            .child(
                div()
                    .px_2()
                    .cursor_pointer()
                    .child(self.i18n.t("search.previous"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.search_next(false, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .cursor_pointer()
                    .child(self.i18n.t("search.next"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.search_next(true, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .cursor_pointer()
                    .child(self.i18n.t("search.close"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.close_search(window, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_terminal_surface(
        &self,
        root_pane: &PaneNode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let terminal = self.render_pane_tree(root_pane, cx);
        if !self.settings_store.settings().terminal.command_bar.enabled {
            return terminal;
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_1().min_h(px(0.0)).child(terminal))
            .child(self.render_terminal_command_bar(cx))
            .into_any_element()
    }

    fn render_terminal_command_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        const COMMAND_BAR_BG_ALPHA: u32 = 0xf2; // Tauri bg-theme-bg/95
        const COMMAND_BAR_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70
        const COMMAND_BAR_INPUT_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-border/45
        const COMMAND_BAR_FOCUSED_BORDER_ALPHA: u32 = 0x73; // Tauri border-theme-accent/45

        let theme = self.tokens.ui;
        let target = WorkspaceImeTarget::TerminalCommandBar;
        let workspace = cx.entity();
        let focused = self.terminal_command_bar_focused;
        let command_text = if self.terminal_command_bar_draft.is_empty() {
            self.i18n.t("terminal.command_bar.command_placeholder")
        } else {
            self.terminal_command_bar_draft.clone()
        };
        let target_label = self
            .active_tab()
            .map(|tab| match tab.kind {
                TabKind::LocalTerminal => self.i18n.t("terminal.command_bar.local_shell"),
                TabKind::SshTerminal => tab.title.clone(),
                _ => tab.title.clone(),
            })
            .unwrap_or_else(|| self.i18n.t("terminal.command_bar.remote_shell"));

        div()
            .relative()
            .flex_none()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | COMMAND_BAR_BORDER_ALPHA))
            .bg(rgba((theme.bg << 8) | COMMAND_BAR_BG_ALPHA))
            .px(px(12.0))
            .py(px(4.0))
            .shadow_lg()
            .child(
                div()
                    .min_h(px(24.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(target_label),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .pt(px(4.0))
                    .border_t_1()
                    .border_color(if focused {
                        rgba((theme.accent << 8) | COMMAND_BAR_FOCUSED_BORDER_ALPHA)
                    } else {
                        rgba((theme.border << 8) | COMMAND_BAR_INPUT_BORDER_ALPHA)
                    })
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.terminal_command_bar_focused = true;
                            this.ime_marked_text = None;
                            window.focus(&this.focus_handle);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronRight,
                        16.0,
                        rgb(theme.text_muted),
                    ))
                    .child(text_input_anchor_probe(
                        target.anchor_id(),
                        div()
                            .h(px(24.0))
                            .flex_1()
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .text_size(px(13.0))
                            .text_color(if self.terminal_command_bar_draft.is_empty() {
                                rgb(theme.text_muted)
                            } else {
                                rgb(theme.text)
                            })
                            .child(command_text)
                            .when_some(self.marked_text_for_target(target), |input, marked| {
                                input.child(
                                    div()
                                        .underline()
                                        .text_color(rgb(theme.text))
                                        .child(marked.to_string()),
                                )
                            })
                            .when(focused, |input| {
                                input.child(text_caret(
                                    &self.tokens,
                                    self.new_connection_caret_visible,
                                ))
                            }),
                        move |anchor, _window, cx| {
                            let _ = workspace.update(cx, |this, cx| {
                                this.update_text_input_anchor(anchor, cx);
                            });
                        },
                    )),
            )
            .into_any_element()
    }
}
