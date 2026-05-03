use gpui::{
    AnyElement, Context, KeyDownEvent, MouseButton, ParentElement, Styled, Window, div, prelude::*,
    px, rgb, rgba,
};

use super::{
    form_state::{
        NewConnectionField, NewConnectionForm, SshAuthTab, current_connection_field_mut,
        next_connection_field,
    },
    ssh_flow::SshConnectionIntent,
};
use crate::workspace::WorkspaceApp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionButtonAction {
    Cancel,
    Test,
    Connect,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_new_connection_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = self.new_connection_form.as_mut() else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        if modifiers.platform {
            if key == "v" {
                self.paste_into_new_connection_field(cx);
            }
            return true;
        }

        match key {
            "escape" => {
                self.close_new_connection_form(window, cx);
                true
            }
            "enter" => {
                self.submit_new_connection_form(window, cx);
                true
            }
            "tab" => {
                form.focused_field = next_connection_field(form.focused_field, !modifiers.shift);
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "backspace" => {
                current_connection_field_mut(form).pop();
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "space" => {
                current_connection_field_mut(form).push(' ');
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            key if key.chars().count() == 1 && !modifiers.control && !modifiers.alt => {
                current_connection_field_mut(form).push_str(key);
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            _ => true,
        }
    }

    pub(in crate::workspace) fn paste_into_new_connection_field(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let single_line = normalized.lines().collect::<Vec<_>>().join(" ");
        current_connection_field_mut(form).push_str(&single_line);
        form.error = None;
        self.new_connection_caret_visible = true;
        cx.notify();
    }

    pub(in crate::workspace) fn render_new_connection_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba((theme.bg << 8) | 0xcc))
            .child(
                div()
                    .w(px(self.tokens.metrics.modal_width))
                    .rounded(px(self.tokens.radii.md))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_elevated))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .px(px(self.tokens.metrics.modal_header_padding_x))
                            .py(px(self.tokens.metrics.modal_header_padding_y))
                            .bg(rgb(theme.bg_panel))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.modal_title_font_size))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.i18n.t("ssh.form.title")),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(self.tokens.metrics.modal_description_font_size))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("ssh.form.subtitle")),
                            ),
                    )
                    .child(
                        div()
                            .p(px(self.tokens.metrics.modal_body_padding))
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.metrics.modal_body_gap))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(self.tokens.metrics.modal_section_gap))
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.name"),
                                        &form.name,
                                        self.i18n.t("ssh.form.name_placeholder"),
                                        NewConnectionField::Name,
                                        false,
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap(px(self.tokens.metrics.form_host_port_gap))
                                            .child(div().flex_1().child(
                                                self.render_connection_field(
                                                    self.i18n.t("ssh.form.host"),
                                                    &form.host,
                                                    self.i18n.t("ssh.form.host_placeholder"),
                                                    NewConnectionField::Host,
                                                    false,
                                                    cx,
                                                ),
                                            ))
                                            .child(
                                                div()
                                                    .w(px(self.tokens.metrics.form_port_width))
                                                    .child(self.render_connection_field(
                                                        self.i18n.t("ssh.form.port"),
                                                        &form.port,
                                                        "22".to_string(),
                                                        NewConnectionField::Port,
                                                        false,
                                                        cx,
                                                    )),
                                            ),
                                    )
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.username"),
                                        &form.username,
                                        "root".to_string(),
                                        NewConnectionField::Username,
                                        false,
                                        cx,
                                    ))
                                    .child(self.render_auth_tabs(form.auth_tab, cx))
                                    .when(form.auth_tab == SshAuthTab::Password, |content| {
                                        content
                                            .child(self.render_connection_field(
                                                self.i18n.t("ssh.form.password"),
                                                &form.password,
                                                String::new(),
                                                NewConnectionField::Password,
                                                true,
                                                cx,
                                            ))
                                            .child(self.render_connection_checkbox(
                                                self.i18n.t("ssh.form.save_password"),
                                                form.save_password,
                                                |form| form.save_password = !form.save_password,
                                                cx,
                                            ))
                                    })
                                    .child(self.render_connection_field(
                                        self.i18n.t("ssh.form.group"),
                                        &form.group,
                                        self.i18n.t("ssh.form.ungrouped"),
                                        NewConnectionField::Group,
                                        false,
                                        cx,
                                    ))
                                    .child(self.render_connection_checkbox(
                                        self.i18n.t("ssh.form.agent_forwarding"),
                                        form.agent_forwarding,
                                        |form| form.agent_forwarding = !form.agent_forwarding,
                                        cx,
                                    ))
                                    .child(self.render_connection_checkbox(
                                        self.i18n.t("ssh.form.save_connection"),
                                        form.save_connection,
                                        |form| form.save_connection = !form.save_connection,
                                        cx,
                                    )),
                            )
                            .when_some(form.error.clone(), |content, error| {
                                content.child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(theme.error))
                                        .child(error),
                                )
                            }),
                    )
                    .child(
                        div()
                            .h(px(self.tokens.metrics.modal_footer_height))
                            .px(px(self.tokens.metrics.modal_footer_padding_x))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_panel))
                            .child(self.render_connection_button(
                                self.i18n.t("ssh.form.cancel"),
                                false,
                                ConnectionButtonAction::Cancel,
                                cx,
                            ))
                            .child(self.render_connection_button(
                                self.i18n.t("ssh.form.test"),
                                false,
                                ConnectionButtonAction::Test,
                                cx,
                            ))
                            .child(self.render_connection_button(
                                self.i18n.t("ssh.form.connect"),
                                true,
                                ConnectionButtonAction::Connect,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_connection_field(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| form.focused_field == field);
        let display = if value.is_empty() {
            placeholder
        } else if secret {
            "•".repeat(value.chars().count())
        } else {
            value.to_string()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_field_gap))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.form_label_font_size))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(label),
            )
            .child(
                div()
                    .id(("connection-field", field as u32))
                    .h(px(self.tokens.metrics.form_input_height))
                    .px(px(self.tokens.metrics.form_input_padding_x))
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.md))
                    .bg(rgba((theme.bg << 8) | 0x80))
                    .border_1()
                    .border_color(if focused {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .text_size(px(self.tokens.metrics.form_text_font_size))
                    .text_color(if value.is_empty() {
                        rgb(theme.text_muted)
                    } else {
                        rgb(theme.text)
                    })
                    .cursor_pointer()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .when(
                                focused && value.is_empty() && self.new_connection_caret_visible,
                                |row| row.child(self.render_connection_caret()),
                            )
                            .child(
                                div()
                                    .text_color(if value.is_empty() {
                                        rgb(theme.text_muted)
                                    } else {
                                        rgb(theme.text)
                                    })
                                    .child(display),
                            )
                            .when(
                                focused && !value.is_empty() && self.new_connection_caret_visible,
                                |row| row.child(self.render_connection_caret()),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            if let Some(form) = this.new_connection_form.as_mut() {
                                form.focused_field = field;
                            }
                            this.new_connection_caret_visible = true;
                            window.focus(&this.focus_handle);
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_connection_caret(&self) -> AnyElement {
        div()
            .w(px(self.tokens.metrics.form_caret_width))
            .h(px(self.tokens.metrics.form_caret_height))
            .bg(rgb(self.tokens.ui.accent))
            .into_any_element()
    }

    fn render_auth_tabs(&self, active_tab: SshAuthTab, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let tabs = [
            (SshAuthTab::Password, "ssh.auth.password"),
            (SshAuthTab::DefaultKey, "ssh.auth.default_key"),
            (SshAuthTab::SshKey, "ssh.auth.ssh_key"),
            (SshAuthTab::Certificate, "ssh.auth.certificate"),
            (SshAuthTab::Agent, "ssh.auth.agent"),
            (SshAuthTab::TwoFactor, "ssh.auth.two_factor"),
        ];
        let mut row = div()
            .h(px(self.tokens.metrics.auth_tab_height))
            .flex()
            .flex_row()
            .p(px(self.tokens.metrics.auth_tab_padding))
            .rounded(px(self.tokens.radii.xs))
            .overflow_hidden()
            .bg(rgb(theme.bg_panel));
        for (tab, key) in tabs {
            let selected = tab == active_tab;
            row = row.child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .rounded(px(self.tokens.radii.xs))
                    .bg(if selected {
                        rgb(theme.bg)
                    } else {
                        rgb(theme.bg_panel)
                    })
                    .text_size(px(self.tokens.metrics.form_text_font_size))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if selected {
                        rgb(theme.text)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .child(self.i18n.t(key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(form) = this.new_connection_form.as_mut() {
                                form.auth_tab = tab;
                            }
                            cx.notify();
                        }),
                    ),
            );
        }
        row.into_any_element()
    }

    fn render_connection_checkbox(
        &self,
        label: String,
        checked: bool,
        toggle: fn(&mut NewConnectionForm),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .child(
                div()
                    .size(px(self.tokens.metrics.form_checkbox_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.xs))
                    .border_1()
                    .border_color(if checked {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .bg(if checked {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.bg)
                    })
                    .text_size(px(self.tokens.metrics.form_checkbox_glyph_size))
                    .text_color(rgb(theme.accent_text))
                    .child(if checked { "✓" } else { "" }),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.form_text_font_size))
                    .text_color(rgb(theme.text))
                    .child(label),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        toggle(form);
                    }
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_connection_button(
        &self,
        label: String,
        primary: bool,
        action: ConnectionButtonAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(self.tokens.metrics.form_button_height))
            .px(px(self.tokens.metrics.form_button_padding_x))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(if primary {
                rgb(theme.accent)
            } else {
                rgb(theme.bg_elevated)
            })
            .text_size(px(self.tokens.metrics.form_text_font_size))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if primary {
                rgb(theme.accent_text)
            } else {
                rgb(theme.text)
            })
            .cursor_pointer()
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| match action {
                    ConnectionButtonAction::Cancel => {
                        this.close_new_connection_form(window, cx);
                    }
                    ConnectionButtonAction::Test => {
                        this.start_new_connection_flow(SshConnectionIntent::Test, window, cx);
                    }
                    ConnectionButtonAction::Connect => {
                        this.submit_new_connection_form(window, cx);
                    }
                }),
            )
            .into_any_element()
    }
}
