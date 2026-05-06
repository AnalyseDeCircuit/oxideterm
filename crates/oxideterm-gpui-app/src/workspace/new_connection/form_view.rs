use gpui::{
    AnyElement, ClipboardItem, Context, KeyDownEvent, MouseButton, ParentElement, Styled, Window,
    div, prelude::*, px, rgb,
};

use super::{
    form_state::{
        NewConnectionField, NewConnectionForm, SshAuthTab, backspace_current_connection_field,
        clear_connection_selection, clear_current_connection_field, connection_field_is_selected,
        current_connection_field, insert_text_into_current_connection_field, next_connection_field,
        select_current_connection_field, text_from_keystroke,
    },
    ssh_flow::SshConnectionIntent,
};
use crate::workspace::WorkspaceApp;
use crate::workspace::ime::WorkspaceImeTarget;
use oxideterm_gpui_ui::{
    ButtonTone, TextInputView, button, checkbox, form_field, modal_body, modal_container,
    modal_footer, modal_header, modal_overlay, segmented_tab, segmented_tabs, text_input,
    text_input_anchor_probe,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionButtonAction {
    Cancel,
    Test,
    Connect,
    Save,
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
        let text_input = text_from_keystroke(&event.keystroke).map(str::to_string);

        if !form.field_focused {
            match key {
                "escape" => {
                    self.close_new_connection_form(window, cx);
                    return true;
                }
                "enter" => {
                    self.submit_new_connection_form(window, cx);
                    return true;
                }
                "tab" => {
                    form.field_focused = true;
                    self.new_connection_caret_visible = true;
                    cx.notify();
                    return true;
                }
                _ => return true,
            }
        }
        let focused_field_accepts_ime = matches!(
            form.focused_field,
            NewConnectionField::Name
                | NewConnectionField::Host
                | NewConnectionField::Username
                | NewConnectionField::Group
        );

        if modifiers.platform {
            match key {
                "a" => {
                    select_current_connection_field(form);
                    self.new_connection_caret_visible = true;
                    cx.notify();
                }
                "c" => {
                    if form.selected_field == Some(form.focused_field) {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            current_connection_field(form).to_string(),
                        ));
                    }
                }
                "x" => {
                    if form.selected_field == Some(form.focused_field) {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            current_connection_field(form).to_string(),
                        ));
                        clear_current_connection_field(form);
                        form.error = None;
                        self.new_connection_caret_visible = true;
                        cx.notify();
                    }
                }
                "v" => {
                    self.paste_into_new_connection_field(cx);
                }
                _ => {}
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
                form.focused_field =
                    next_connection_field(form.focused_field, form.auth_tab, !modifiers.shift);
                form.field_focused = true;
                clear_connection_selection(form);
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "backspace" => {
                backspace_current_connection_field(form);
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "space" => {
                if focused_field_accepts_ime {
                    return true;
                }
                insert_text_into_current_connection_field(form, " ");
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            _ => {
                if focused_field_accepts_ime {
                    return true;
                }
                let Some(text) = text_input else {
                    return true;
                };
                insert_text_into_current_connection_field(form, &text);
                form.error = None;
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
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
        insert_text_into_current_connection_field(form, &single_line);
        form.error = None;
        self.new_connection_caret_visible = true;
        cx.notify();
    }

    pub(in crate::workspace) fn render_new_connection_modal(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.new_connection_form.as_ref() else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        modal_overlay(
            &self.tokens,
            modal_container(&self.tokens)
                .max_h(px(modal_max_height))
                .flex()
                .flex_col()
                .child(modal_header(
                    &self.tokens,
                    if self.editing_saved_connection_id.is_some() {
                        self.i18n.t("sessionManager.edit_properties.title")
                    } else {
                        self.i18n.t("ssh.form.title")
                    },
                    if self.editing_saved_connection_id.is_some() {
                        self.i18n.t("sessionManager.edit_properties.description")
                    } else {
                        self.i18n.t("ssh.form.subtitle")
                    },
                ))
                .child(
                    modal_body(&self.tokens)
                        .id("new-connection-modal-body-scroll")
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_y_scroll()
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
                                        .child(div().flex_1().child(self.render_connection_field(
                                            self.i18n.t("ssh.form.host"),
                                            &form.host,
                                            self.i18n.t("ssh.form.host_placeholder"),
                                            NewConnectionField::Host,
                                            false,
                                            cx,
                                        )))
                                        .child(
                                            div().w(px(self.tokens.metrics.form_port_width)).child(
                                                self.render_connection_field(
                                                    self.i18n.t("ssh.form.port"),
                                                    &form.port,
                                                    "22".to_string(),
                                                    NewConnectionField::Port,
                                                    false,
                                                    cx,
                                                ),
                                            ),
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
                                .when(form.auth_tab == SshAuthTab::DefaultKey, |content| {
                                    content
                                        .child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.default_key_desc"),
                                        ))
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            &form.passphrase,
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            true,
                                            cx,
                                        ))
                                })
                                .when(form.auth_tab == SshAuthTab::SshKey, |content| {
                                    content
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.key_file"),
                                            &form.key_path,
                                            "~/.ssh/id_ed25519".to_string(),
                                            NewConnectionField::KeyPath,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            &form.passphrase,
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            true,
                                            cx,
                                        ))
                                })
                                .when(form.auth_tab == SshAuthTab::Certificate, |content| {
                                    content
                                        .child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.certificate_note"),
                                        ))
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.private_key"),
                                            &form.key_path,
                                            "~/.ssh/id_ed25519".to_string(),
                                            NewConnectionField::KeyPath,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.certificate"),
                                            &form.cert_path,
                                            "~/.ssh/id_ed25519-cert.pub".to_string(),
                                            NewConnectionField::CertPath,
                                            false,
                                            cx,
                                        ))
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            &form.passphrase,
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            true,
                                            cx,
                                        ))
                                })
                                .when(form.auth_tab == SshAuthTab::Agent, |content| {
                                    content.child(
                                        self.render_connection_hint(
                                            self.i18n.t("ssh.form.agent_desc"),
                                        ),
                                    )
                                })
                                .when(form.auth_tab == SshAuthTab::TwoFactor, |content| {
                                    content.child(self.render_connection_hint(
                                        self.i18n.t("ssh.form.two_factor_desc"),
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
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.error))
                                    .child(error),
                            )
                        }),
                )
                .child(
                    modal_footer(&self.tokens)
                        .flex_none()
                        .child(self.render_connection_button(
                            self.i18n.t("ssh.form.cancel"),
                            false,
                            ConnectionButtonAction::Cancel,
                            cx,
                        ))
                        .when(self.editing_saved_connection_id.is_none(), |footer| {
                            footer.child(self.render_connection_button(
                                self.i18n.t("ssh.form.test"),
                                false,
                                ConnectionButtonAction::Test,
                                cx,
                            ))
                        })
                        .child(self.render_connection_button(
                            if self.editing_saved_connection_id.is_some() {
                                self.i18n.t("sessionManager.edit_properties.save")
                            } else {
                                self.i18n.t("ssh.form.connect")
                            },
                            true,
                            if self.editing_saved_connection_id.is_some() {
                                ConnectionButtonAction::Save
                            } else {
                                ConnectionButtonAction::Connect
                            },
                            cx,
                        )),
                ),
        )
    }

    fn render_connection_hint(&self, text: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(text)
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
        let focused = self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| form.field_focused && form.focused_field == field);
        let selected_all = self
            .new_connection_form
            .as_ref()
            .is_some_and(|form| connection_field_is_selected(form, field));
        let target = WorkspaceImeTarget::NewConnection(field);
        let workspace = cx.entity();
        form_field(
            &self.tokens,
            label,
            text_input_anchor_probe(
                target.anchor_id(),
                text_input(
                    &self.tokens,
                    TextInputView {
                        value,
                        placeholder,
                        focused,
                        caret_visible: self.new_connection_caret_visible,
                        secret,
                        selected_all,
                        marked_text: self.marked_text_for_target(target),
                    },
                )
                .id(("connection-field", field as u32))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.field_focused = true;
                            form.focused_field = field;
                            clear_connection_selection(form);
                        }
                        this.ime_marked_text = None;
                        this.new_connection_caret_visible = true;
                        window.focus(&this.focus_handle);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            ),
        )
    }

    fn render_auth_tabs(&self, active_tab: SshAuthTab, cx: &mut Context<Self>) -> AnyElement {
        let tabs = [
            (SshAuthTab::Password, "ssh.auth.password"),
            (SshAuthTab::DefaultKey, "ssh.auth.default_key"),
            (SshAuthTab::SshKey, "ssh.auth.ssh_key"),
            (SshAuthTab::Certificate, "ssh.auth.certificate"),
            (SshAuthTab::Agent, "ssh.auth.agent"),
            (SshAuthTab::TwoFactor, "ssh.auth.two_factor"),
        ];
        let mut row = segmented_tabs(&self.tokens);
        for (tab, key) in tabs {
            let selected = tab == active_tab;
            row = row.child(
                segmented_tab(&self.tokens, self.i18n.t(key), selected).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.auth_tab = tab;
                            clear_connection_selection(form);
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
        checkbox(&self.tokens, label, checked)
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
        button(
            &self.tokens,
            label,
            if primary {
                ButtonTone::Primary
            } else {
                ButtonTone::Secondary
            },
        )
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
                ConnectionButtonAction::Save => {
                    this.submit_new_connection_form(window, cx);
                }
            }),
        )
        .into_any_element()
    }
}
