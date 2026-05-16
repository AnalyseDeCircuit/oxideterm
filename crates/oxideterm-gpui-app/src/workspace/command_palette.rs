use super::*;
use gpui_component::scroll::ScrollableElement;
use oxideterm_gpui_ui::modal::{dialog_backdrop, dialog_content};

#[derive(Clone, Copy)]
struct CommandPaletteCommand {
    action_id: &'static str,
}

impl WorkspaceApp {
    pub(super) fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette.open = true;
        self.command_palette.query.clear();
        self.command_palette.selected_index = 0;
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        self.command_palette.open = false;
        self.command_palette.query.clear();
        self.command_palette.selected_index = 0;
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn open_shortcuts_modal(&mut self, cx: &mut Context<Self>) {
        self.shortcuts_modal.open = true;
        self.shortcuts_modal.query.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn close_shortcuts_modal(&mut self, cx: &mut Context<Self>) {
        self.shortcuts_modal.open = false;
        self.shortcuts_modal.query.clear();
        self.ime_marked_text = None;
        cx.notify();
    }

    pub(super) fn handle_command_palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        match key {
            "escape" if !event.keystroke.modifiers.platform => self.close_command_palette(cx),
            "enter" if !event.keystroke.modifiers.platform => {
                self.execute_selected_command_palette_item(window, cx);
            }
            "arrowdown" | "down" => {
                let count = self.filtered_command_palette_commands().len();
                if count > 0 {
                    self.command_palette.selected_index =
                        (self.command_palette.selected_index + 1).min(count - 1);
                    cx.notify();
                }
            }
            "arrowup" | "up" => {
                self.command_palette.selected_index =
                    self.command_palette.selected_index.saturating_sub(1);
                cx.notify();
            }
            "backspace" if !event.keystroke.modifiers.platform => {
                self.command_palette.query.pop();
                self.command_palette.selected_index = 0;
                cx.notify();
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && !text.chars().any(char::is_control)
                {
                    self.command_palette.query.push_str(text);
                    self.command_palette.selected_index = 0;
                    cx.notify();
                }
            }
        }
    }

    pub(super) fn handle_shortcuts_modal_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        match key {
            "escape" if !event.keystroke.modifiers.platform => self.close_shortcuts_modal(cx),
            "backspace" if !event.keystroke.modifiers.platform => {
                self.shortcuts_modal.query.pop();
                cx.notify();
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref()
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && !text.chars().any(char::is_control)
                {
                    self.shortcuts_modal.query.push_str(text);
                    cx.notify();
                }
            }
        }
    }

    fn execute_selected_command_palette_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let commands = self.filtered_command_palette_commands();
        let Some(command) = commands.get(self.command_palette.selected_index).copied() else {
            return;
        };
        self.command_palette.open = false;
        self.command_palette.query.clear();
        self.command_palette.selected_index = 0;
        let _ = self.dispatch_keybinding_action(command.action_id, window, cx);
    }

    fn filtered_command_palette_commands(&self) -> Vec<CommandPaletteCommand> {
        let query = self.command_palette.query.trim().to_lowercase();
        command_palette_commands()
            .into_iter()
            .filter(|command| {
                if query.is_empty() {
                    return true;
                }
                let label = self
                    .i18n
                    .t(&format!(
                        "settings_view.keybindings.actions.{}",
                        command.action_id
                    ))
                    .to_lowercase();
                label.contains(&query) || command.action_id.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub(super) fn render_command_palette(&self, cx: &mut Context<Self>) -> AnyElement {
        let commands = self.filtered_command_palette_commands();
        let query_text = if self.command_palette.query.is_empty() {
            self.i18n.t("settings_view.keybindings.search_placeholder")
        } else {
            self.command_palette.query.clone()
        };
        let empty = self.i18n.t("settings_view.keybindings.no_results");

        dialog_backdrop()
            .child(
                dialog_content(&self.tokens)
                    .w(px(640.0))
                    .max_h(px(520.0))
                    .child(
                        div()
                            .px(px(24.0))
                            .py(px(18.0))
                            .border_b_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(self.i18n.t("command_palette.title")),
                            )
                            .child(
                                div()
                                    .mt(px(14.0))
                                    .h(px(44.0))
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(rgb(self.tokens.ui.border))
                                    .bg(rgb(self.tokens.ui.bg))
                                    .px(px(14.0))
                                    .flex()
                                    .items_center()
                                    .text_size(px(15.0))
                                    .text_color(if self.command_palette.query.is_empty() {
                                        rgb(self.tokens.ui.text_muted)
                                    } else {
                                        rgb(self.tokens.ui.text)
                                    })
                                    .child(query_text),
                            ),
                    )
                    .child(
                        div()
                            .max_h(px(390.0))
                            .overflow_y_scrollbar()
                            .p(px(8.0))
                            .children(if commands.is_empty() {
                                vec![
                                    div()
                                        .px(px(16.0))
                                        .py(px(20.0))
                                        .text_size(px(14.0))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(empty)
                                        .into_any_element(),
                                ]
                            } else {
                                commands
                                    .iter()
                                    .enumerate()
                                    .map(|(index, command)| {
                                        let selected = index == self.command_palette.selected_index;
                                        self.render_command_palette_row(*command, selected, cx)
                                    })
                                    .collect()
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_command_palette_row(
        &self,
        command: CommandPaletteCommand,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(&format!(
            "settings_view.keybindings.actions.{}",
            command.action_id
        ));
        let shortcut = crate::keybindings::action_definition(command.action_id).map(|definition| {
            crate::keybindings::format_combo(&crate::keybindings::effective_combo(
                definition,
                &self.settings_store.settings().keybindings.overrides,
                crate::keybindings::KeybindingSide::current(),
            ))
        });
        div()
            .id(command.action_id)
            .h(px(42.0))
            .rounded(px(self.tokens.radii.sm))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .bg(if selected {
                rgba((self.tokens.ui.accent << 8) | 0x26)
            } else {
                rgba(0x00000000)
            })
            .text_color(rgb(self.tokens.ui.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.command_palette.open = false;
                    this.command_palette.query.clear();
                    this.command_palette.selected_index = 0;
                    let _ = this.dispatch_keybinding_action(command.action_id, window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(div().text_size(px(14.0)).child(label))
            .when_some(shortcut, |row, shortcut| {
                row.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(shortcut),
                )
            })
            .into_any_element()
    }

    pub(super) fn render_shortcuts_modal(&self, _cx: &mut Context<Self>) -> AnyElement {
        let rows = self.filtered_shortcut_rows();
        let query_text = if self.shortcuts_modal.query.is_empty() {
            self.i18n.t("settings_view.keybindings.search_placeholder")
        } else {
            self.shortcuts_modal.query.clone()
        };
        dialog_backdrop()
            .child(
                dialog_content(&self.tokens)
                    .w(px(760.0))
                    .max_h(px(640.0))
                    .child(
                        div()
                            .px(px(24.0))
                            .py(px(18.0))
                            .border_b_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(self.i18n.t("layout.empty.keyboard_shortcuts")),
                            )
                            .child(
                                div()
                                    .mt(px(14.0))
                                    .h(px(44.0))
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(rgb(self.tokens.ui.border))
                                    .bg(rgb(self.tokens.ui.bg))
                                    .px(px(14.0))
                                    .flex()
                                    .items_center()
                                    .text_size(px(15.0))
                                    .text_color(if self.shortcuts_modal.query.is_empty() {
                                        rgb(self.tokens.ui.text_muted)
                                    } else {
                                        rgb(self.tokens.ui.text)
                                    })
                                    .child(query_text),
                            ),
                    )
                    .child(
                        div()
                            .max_h(px(500.0))
                            .overflow_y_scrollbar()
                            .p(px(16.0))
                            .children(rows),
                    ),
            )
            .into_any_element()
    }

    fn filtered_shortcut_rows(&self) -> Vec<AnyElement> {
        let query = self.shortcuts_modal.query.trim().to_lowercase();
        let side = crate::keybindings::KeybindingSide::current();
        let overrides = &self.settings_store.settings().keybindings.overrides;
        let mut rows = Vec::new();
        for definition in crate::keybindings::ACTION_DEFINITIONS.iter() {
            let label = self.i18n.t(&definition.label_key());
            let scope = self.i18n.t(definition.scope.label_key());
            let shortcut = crate::keybindings::format_combo(&crate::keybindings::effective_combo(
                definition, overrides, side,
            ));
            if !query.is_empty()
                && !label.to_lowercase().contains(&query)
                && !shortcut.to_lowercase().contains(&query)
                && !scope.to_lowercase().contains(&query)
            {
                continue;
            }
            rows.push((scope, label, shortcut));
        }
        for (category_key, shortcut_rows) in shortcut_reference_rows() {
            let scope = self.i18n.t(category_key);
            for (label_key, mac, other) in shortcut_rows {
                let label = self.i18n.t(label_key);
                let shortcut = if cfg!(target_os = "macos") {
                    mac
                } else {
                    other
                }
                .to_string();
                if !query.is_empty()
                    && !label.to_lowercase().contains(&query)
                    && !shortcut.to_lowercase().contains(&query)
                    && !scope.to_lowercase().contains(&query)
                {
                    continue;
                }
                rows.push((scope.clone(), label, shortcut));
            }
        }
        let row_count = rows.len();
        rows.into_iter()
            .enumerate()
            .map(|(index, (scope, label, shortcut))| {
                self.render_shortcut_row(scope, label, shortcut, index + 1 < row_count)
            })
            .collect()
    }

    fn render_shortcut_row(
        &self,
        scope: String,
        label: String,
        shortcut: String,
        show_separator: bool,
    ) -> AnyElement {
        div()
            .min_h(px(38.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .when(show_separator, |row| {
                row.border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .w(px(120.0))
                            .text_size(px(12.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(scope),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(label),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(shortcut),
            )
            .into_any_element()
    }
}

fn command_palette_commands() -> Vec<CommandPaletteCommand> {
    [
        "app.newTerminal",
        "app.newConnection",
        "app.settings",
        "app.toggleSidebar",
        "app.zenMode",
        "palette.eventLog",
        "palette.aiSidebar",
        "app.closeTab",
        "split.horizontal",
        "split.vertical",
        "palette.broadcast",
        "app.showShortcuts",
        "app.nextTab",
        "app.prevTab",
        "app.closeOtherTabs",
        "app.navBack",
        "app.navForward",
        "app.shellLauncher",
        "app.fontIncrease",
        "app.fontDecrease",
        "app.fontReset",
    ]
    .into_iter()
    .map(|action_id| CommandPaletteCommand { action_id })
    .collect()
}

fn shortcut_reference_rows() -> Vec<(
    &'static str,
    Vec<(&'static str, &'static str, &'static str)>,
)> {
    vec![
        (
            "settings_view.help.category_file_manager",
            vec![
                ("settings_view.help.shortcut_select_all", "⌘A", "Ctrl+A"),
                ("settings_view.help.shortcut_copy", "⌘C", "Ctrl+C"),
                ("settings_view.help.shortcut_cut", "⌘X", "Ctrl+X"),
                ("settings_view.help.shortcut_paste", "⌘V", "Ctrl+V"),
                ("settings_view.help.shortcut_rename", "F2", "F2"),
                ("settings_view.help.shortcut_delete", "Delete", "Delete"),
                ("settings_view.help.shortcut_quick_look", "Space", "Space"),
                ("settings_view.help.shortcut_open", "Enter", "Enter"),
            ],
        ),
        (
            "settings_view.help.category_sftp",
            vec![
                ("settings_view.help.shortcut_select_all", "⌘A", "Ctrl+A"),
                ("settings_view.help.shortcut_quick_look", "Space", "Space"),
                (
                    "settings_view.help.shortcut_sftp_enter_dir",
                    "Enter",
                    "Enter",
                ),
                ("settings_view.help.shortcut_sftp_upload", "→", "→"),
                ("settings_view.help.shortcut_sftp_download", "←", "←"),
                ("settings_view.help.shortcut_rename", "F2", "F2"),
                ("settings_view.help.shortcut_delete", "Delete", "Delete"),
            ],
        ),
        (
            "settings_view.help.category_editor",
            vec![
                ("settings_view.help.shortcut_save", "⌘S", "Ctrl+S"),
                ("settings_view.help.shortcut_find", "⌘F", "Ctrl+F"),
                ("settings_view.help.shortcut_copy", "⌘C", "Ctrl+C"),
                ("settings_view.help.shortcut_paste", "⌘V", "Ctrl+V"),
                ("settings_view.help.shortcut_close", "Esc", "Esc"),
            ],
        ),
    ]
}
