use gpui::{
    AnchoredPositionMode, AnyElement, ClipboardItem, Context, Corner, KeyDownEvent, MouseButton,
    ParentElement, PathPromptOptions, SharedString, Styled, Window, anchored, deferred, div, point,
    prelude::*, px, rgb, rgba,
};

use super::{
    form_state::{
        NewConnectionField, NewConnectionForm, NewConnectionSelect, SavedConnectionPromptAction,
        SshAuthTab, backspace_current_connection_field, clear_connection_selection,
        clear_current_connection_field, connection_field_is_selected, current_connection_field,
        insert_text_into_current_connection_field, new_connection_form_mode, next_connection_field,
        next_jump_connection_field, select_current_connection_field, text_from_keystroke,
    },
    ssh_flow::SshConnectionIntent,
};
use crate::assets::LucideIcon;
use crate::workspace::WorkspaceApp;
use crate::workspace::ime::WorkspaceImeTarget;
use oxideterm_gpui_ui::{
    ButtonTone, TextInputView, button,
    button::{ButtonOptions, ButtonSize, ButtonVariant, button_with},
    checkbox, form_field, modal_body, modal_container, modal_footer, modal_header, modal_overlay,
    radio_group::{radio_group, radio_group_item},
    segmented_tab, segmented_tabs,
    select::{
        SelectAnchorId, select_anchor_probe, select_option, select_overlay_popup_with_max_height,
        select_trigger,
    },
    text_input, text_input_anchor_probe,
};

const TAURI_EDIT_MODAL_WIDTH: f32 = 500.0; // Tauri sm:max-w-[500px]
const TAURI_EDIT_COLOR_FALLBACK: u32 = 0x22d3ee;
const TAURI_EDIT_COLOR_FALLBACK_TEXT: &str = "#22d3ee";
const TAURI_PROMPT_ERROR_ALPHA: u32 = 0x1a; // Tailwind red-500/10
const TAURI_PROMPT_ERROR_BORDER_ALPHA: u32 = 0x80; // Tailwind red-500/50
const TAURI_PASSWORD_ICON_BUTTON_SIZE: f32 = 28.0; // Tauri h-7 w-7
const TAURI_PASSWORD_ICON_BUTTON_OFFSET: f32 = 4.0; // Tauri right-1 top-1
const TAURI_PASSWORD_ICON_SIZE: f32 = 16.0; // Tauri h-4 w-4
const TAURI_JUMP_MODAL_WIDTH: f32 = 425.0; // Tauri sm:max-w-[425px]
const TAURI_PROXY_CHAIN_MAX_HEIGHT: f32 = 250.0; // Tauri max-h-[250px]
const TAURI_PROXY_CHAIN_NODE_SIZE: f32 = 32.0; // Tauri w-8 h-8
const TAURI_PROXY_CHAIN_LINE_WIDTH: f32 = 32.0; // Tauri w-8

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

        if self.open_new_connection_select.is_some()
            && matches!(key, "escape" | "enter" | "tab")
            && !modifiers.platform
        {
            self.open_new_connection_select = None;
            cx.notify();
            return true;
        }

        if !form.field_focused {
            match key {
                "escape" => {
                    if form.jump_server_form.is_some() {
                        form.jump_server_form = None;
                        cx.notify();
                        return true;
                    }
                    self.close_new_connection_form(window, cx);
                    return true;
                }
                "enter" => {
                    if form.jump_server_form.is_some() {
                        self.add_pending_jump_server(cx);
                        return true;
                    }
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

        let password_locked = self.editing_saved_connection_id.is_some()
            && self.saved_connection_prompt_action.is_none()
            && form.focused_field == NewConnectionField::Password
            && !form.password_loaded;
        if password_locked && !matches!(key, "escape" | "enter" | "tab") {
            return true;
        }

        let focused_field_accepts_ime = matches!(
            form.focused_field,
            NewConnectionField::Name
                | NewConnectionField::Host
                | NewConnectionField::Username
                | NewConnectionField::Group
                | NewConnectionField::Color
                | NewConnectionField::JumpHost
                | NewConnectionField::JumpUsername
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
                if form.jump_server_form.is_some() {
                    form.jump_server_form = None;
                    form.field_focused = false;
                    self.ime_marked_text = None;
                    cx.notify();
                    return true;
                }
                self.close_new_connection_form(window, cx);
                true
            }
            "enter" => {
                if form.jump_server_form.is_some() {
                    self.add_pending_jump_server(cx);
                    return true;
                }
                self.submit_new_connection_form(window, cx);
                true
            }
            "tab" => {
                form.focused_field = if let Some(jump_form) = form.jump_server_form.as_ref() {
                    next_jump_connection_field(
                        form.focused_field,
                        jump_form.auth_tab,
                        !modifiers.shift,
                    )
                } else {
                    next_connection_field(form.focused_field, form.auth_tab, !modifiers.shift)
                };
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
        if self.editing_saved_connection_id.is_some()
            && self.saved_connection_prompt_action.is_none()
            && form.focused_field == NewConnectionField::Password
            && !form.password_loaded
        {
            return;
        }
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
        let mode = new_connection_form_mode(
            self.editing_saved_connection_id.as_deref(),
            self.saved_connection_prompt_action,
        );
        let prompt_mode = mode == super::form_state::NewConnectionFormMode::SavedConnectionPrompt;
        let edit_properties_mode = mode.submits_saved_connection_properties();
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let title = if prompt_mode {
            self.i18n
                .t("sessionManager.connect_prompt.title")
                .replace("{{name}}", &form.name)
        } else if edit_properties_mode {
            self.i18n.t("sessionManager.edit_properties.title")
        } else {
            self.i18n.t("ssh.form.title")
        };
        let description = if prompt_mode {
            format!("{}@{}:{}", form.username, form.host, form.port)
        } else if edit_properties_mode {
            self.i18n.t("sessionManager.edit_properties.description")
        } else {
            self.i18n.t("ssh.form.subtitle")
        };
        let has_required_fields = !form.host.trim().is_empty()
            && !form.username.trim().is_empty()
            && form.port.trim().parse::<u16>().is_ok();
        let primary_disabled = form.pending || !has_required_fields;
        modal_overlay(
            &self.tokens,
            modal_container(&self.tokens)
                .w(px(if prompt_mode || edit_properties_mode {
                    TAURI_EDIT_MODAL_WIDTH
                } else {
                    self.tokens.metrics.modal_width
                }))
                .max_h(px(modal_max_height))
                .flex()
                .flex_col()
                .child(modal_header(&self.tokens, title, description))
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
                                .when(!prompt_mode, |content| {
                                    content
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
                                })
                                .when_some(
                                    if prompt_mode {
                                        form.error.clone()
                                    } else {
                                        None
                                    },
                                    |content, error| {
                                        content.child(self.render_prompt_error_box(error))
                                    },
                                )
                                .child(if prompt_mode {
                                    self.render_prompt_auth_radios(form.auth_tab, cx)
                                } else {
                                    self.render_auth_tabs(form.auth_tab, edit_properties_mode, cx)
                                })
                                .when(form.auth_tab == SshAuthTab::Password, |content| {
                                    if edit_properties_mode {
                                        content
                                            .child(self.render_edit_saved_password_field(form, cx))
                                            .child(self.render_connection_hint(
                                                self.i18n.t(
                                                    "sessionManager.edit_properties.password_hint",
                                                ),
                                            ))
                                            .when_some(
                                                form.password_error.clone(),
                                                |content, error| {
                                                    content.child(
                                                        div()
                                                            .text_size(px(self
                                                                .tokens
                                                                .metrics
                                                                .ui_text_xs))
                                                            .text_color(rgb(self.tokens.ui.error))
                                                            .child(error),
                                                    )
                                                },
                                            )
                                    } else if prompt_mode {
                                        content.child(self.render_connection_field(
                                            self.i18n.t("ssh.form.password"),
                                            &form.password,
                                            String::new(),
                                            NewConnectionField::Password,
                                            true,
                                            cx,
                                        ))
                                    } else {
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
                                    }
                                })
                                .when(
                                    form.auth_tab == SshAuthTab::DefaultKey
                                        && !prompt_mode
                                        && !edit_properties_mode,
                                    |content| {
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
                                    },
                                )
                                .when(
                                    form.auth_tab == SshAuthTab::SshKey
                                        || ((prompt_mode || edit_properties_mode)
                                            && form.auth_tab == SshAuthTab::DefaultKey),
                                    |content| {
                                        let key_label = if edit_properties_mode {
                                            self.i18n.t("sessionManager.edit_properties.key_path")
                                        } else {
                                            self.i18n.t("ssh.form.key_file")
                                        };
                                        let key_field = if prompt_mode {
                                            self.render_connection_field(
                                                key_label,
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                key_label,
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                cx,
                                            )
                                        };
                                        content
                                            .child(key_field)
                                            .child(self.render_connection_field(
                                                self.i18n.t("ssh.form.passphrase"),
                                                &form.passphrase,
                                                self.i18n.t("ssh.form.passphrase_placeholder"),
                                                NewConnectionField::Passphrase,
                                                true,
                                                cx,
                                            ))
                                            .when(edit_properties_mode, |content| {
                                                content.child(self.render_connection_hint(
                                                    self.i18n.t(
                                                        "sessionManager.edit_properties.passphrase_hint",
                                                    ),
                                                ))
                                            })
                                    },
                                )
                                .when(form.auth_tab == SshAuthTab::Certificate, |content| {
                                    let content = if prompt_mode {
                                        content
                                    } else {
                                        content.child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.certificate_note"),
                                        ))
                                    };
                                    content
                                        .child(if prompt_mode {
                                            self.render_connection_field(
                                                self.i18n.t("ssh.form.private_key"),
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                self.i18n.t("ssh.form.private_key"),
                                                &form.key_path,
                                                "~/.ssh/id_ed25519".to_string(),
                                                NewConnectionField::KeyPath,
                                                cx,
                                            )
                                        })
                                        .child(if prompt_mode {
                                            self.render_connection_field(
                                                self.i18n.t("ssh.form.certificate"),
                                                &form.cert_path,
                                                "~/.ssh/id_ed25519-cert.pub".to_string(),
                                                NewConnectionField::CertPath,
                                                false,
                                                cx,
                                            )
                                        } else {
                                            self.render_connection_field_with_browse(
                                                self.i18n.t("ssh.form.certificate"),
                                                &form.cert_path,
                                                "~/.ssh/id_ed25519-cert.pub".to_string(),
                                                NewConnectionField::CertPath,
                                                cx,
                                            )
                                        })
                                        .child(self.render_connection_field(
                                            self.i18n.t("ssh.form.passphrase"),
                                            &form.passphrase,
                                            self.i18n.t("ssh.form.passphrase_placeholder"),
                                            NewConnectionField::Passphrase,
                                            true,
                                            cx,
                                        ))
                                        .when(edit_properties_mode, |content| {
                                            content.child(self.render_connection_hint(
                                                self.i18n.t(
                                                    "sessionManager.edit_properties.passphrase_hint",
                                                ),
                                            ))
                                        })
                                })
                                .when(form.auth_tab == SshAuthTab::Agent, |content| {
                                    content.child(
                                        self.render_connection_hint(
                                            self.i18n.t("ssh.form.agent_desc"),
                                        ),
                                    )
                                })
                                .when(
                                    form.auth_tab == SshAuthTab::TwoFactor
                                        && !prompt_mode
                                        && !edit_properties_mode,
                                    |content| {
                                        content.child(self.render_connection_hint(
                                            self.i18n.t("ssh.form.two_factor_desc"),
                                        ))
                                    },
                                )
                                .child(self.render_connection_group_select(
                                    if edit_properties_mode {
                                        self.i18n.t("sessionManager.edit_properties.group")
                                    } else {
                                        self.i18n.t("ssh.form.group")
                                    },
                                    &form.group,
                                    cx,
                                ))
                                .when(edit_properties_mode, |content| {
                                    content.child(self.render_edit_color_field(&form.color, cx))
                                })
                                .when(!prompt_mode && !edit_properties_mode, |content| {
                                    content
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
                                        ))
                                        .child(self.render_proxy_chain_section(cx))
                                }),
                        )
                        .when_some(
                            if prompt_mode {
                                None
                            } else {
                                form.error.clone()
                            },
                            |content, error| {
                                content.child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(theme.error))
                                        .child(error),
                                )
                            },
                        ),
                )
                .child(
                    modal_footer(&self.tokens)
                        .flex_none()
                        .child(self.render_connection_button(
                            self.i18n.t("ssh.form.cancel"),
                            false,
                            ConnectionButtonAction::Cancel,
                            false,
                            cx,
                        ))
                        .when(
                            self.editing_saved_connection_id.is_none()
                                && self.saved_connection_prompt_action.is_none(),
                            |footer| {
                                footer.child(self.render_connection_button(
                                    self.i18n.t("ssh.form.test"),
                                    false,
                                    ConnectionButtonAction::Test,
                                    primary_disabled,
                                    cx,
                                ))
                            },
                        )
                        .child(self.render_connection_button(
                            if self.saved_connection_prompt_action
                                == Some(SavedConnectionPromptAction::Test)
                            {
                                self.i18n.t("ssh.form.test")
                            } else if self.saved_connection_prompt_action
                                == Some(SavedConnectionPromptAction::Connect)
                            {
                                self.i18n.t("ssh.form.connect")
                            } else if self.editing_saved_connection_id.is_some() {
                                self.i18n.t("sessionManager.edit_properties.save")
                            } else {
                                self.i18n.t("ssh.form.connect")
                            },
                            true,
                            if self.editing_saved_connection_id.is_some()
                                && self.saved_connection_prompt_action.is_none()
                            {
                                ConnectionButtonAction::Save
                            } else {
                                ConnectionButtonAction::Connect
                            },
                            primary_disabled,
                            cx,
                        )),
                ),
        )
    }

    pub(in crate::workspace) fn render_new_connection_select_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.open_new_connection_select != Some(NewConnectionSelect::Group) {
            return None;
        }
        let anchor = *self
            .select_anchors
            .get(&SelectAnchorId::NewConnectionGroup)?;
        let width =
            f32::from(anchor.bounds.size.width).max(self.tokens.metrics.ui_select_min_width);
        let viewport_height = f32::from(window.viewport_size().height);
        let popup_gap = self.tokens.metrics.settings_select_popup_gap;
        let below = viewport_height - f32::from(anchor.bounds.bottom()) - popup_gap;
        let above = f32::from(anchor.bounds.top()) - popup_gap;
        let opens_above = below < self.tokens.metrics.ui_select_max_height && above > below;
        let max_height = if opens_above { above } else { below }
            .max(self.tokens.metrics.ui_control_height)
            .min(self.tokens.metrics.ui_select_max_height);

        let current_group = self
            .new_connection_form
            .as_ref()
            .map(|form| form.group.as_str())
            .unwrap_or_default();
        let ungrouped_label = self.connection_form_ungrouped_label();
        let mut popup = select_overlay_popup_with_max_height(&self.tokens, width, max_height)
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                select_option(
                    &self.tokens,
                    ungrouped_label.clone(),
                    self.connection_form_group_is_ungrouped(current_group),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.set_new_connection_group(ungrouped_label.clone(), cx);
                        cx.stop_propagation();
                    }),
                ),
            );

        let groups = self.connection_form_group_options(current_group);
        for group in groups.iter().cloned() {
            let selected = group == current_group;
            popup = popup.child(
                select_option(&self.tokens, group.clone(), selected).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.set_new_connection_group(group.clone(), cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        if groups.is_empty() {
            popup = popup.child(
                div()
                    .relative()
                    .flex()
                    .w_full()
                    .items_center()
                    .rounded(px(self.tokens.radii.xs))
                    .py(px(self.tokens.metrics.ui_menu_item_padding_y))
                    .px(px(self.tokens.metrics.ui_menu_item_padding_x))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .opacity(0.65)
                    .child(self.i18n.t("ssh.form.create_groups_hint")),
            );
        }

        let (anchor_corner, position, offset_y) = if opens_above {
            (
                Corner::BottomLeft,
                point(anchor.bounds.left(), anchor.bounds.top()),
                -popup_gap,
            )
        } else {
            (Corner::TopLeft, anchor.bounds.bottom_left(), popup_gap)
        };

        Some(
            deferred(
                anchored()
                    .anchor(anchor_corner)
                    .position(position)
                    .offset(point(px(0.0), px(offset_y)))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(popup),
            )
            .with_priority(200)
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn render_add_jump_server_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(jump_form) = self
            .new_connection_form
            .as_ref()
            .and_then(|form| form.jump_server_form.as_ref())
        else {
            return div().into_any_element();
        };
        let add_disabled = !jump_form.complete();
        modal_overlay(
            &self.tokens,
            modal_container(&self.tokens)
                .w(px(TAURI_JUMP_MODAL_WIDTH))
                .child(modal_header(
                    &self.tokens,
                    self.i18n.t("ssh.form.proxy_jump_title"),
                    String::new(),
                ))
                .child(
                    modal_body(&self.tokens)
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(
                            div()
                                .flex()
                                .gap_4()
                                .child(div().flex_1().child(self.render_connection_field(
                                    self.i18n.t("ssh.form.proxy_jump_host"),
                                    &jump_form.host,
                                    self.i18n.t("ssh.form.proxy_jump_host_placeholder"),
                                    NewConnectionField::JumpHost,
                                    false,
                                    cx,
                                )))
                                .child(div().w(px(self.tokens.metrics.form_port_width)).child(
                                    self.render_connection_field(
                                        self.i18n.t("ssh.form.proxy_jump_port"),
                                        &jump_form.port,
                                        "22".to_string(),
                                        NewConnectionField::JumpPort,
                                        false,
                                        cx,
                                    ),
                                )),
                        )
                        .child(self.render_connection_field(
                            self.i18n.t("ssh.form.proxy_jump_username"),
                            &jump_form.username,
                            self.i18n.t("ssh.form.proxy_jump_username_placeholder"),
                            NewConnectionField::JumpUsername,
                            false,
                            cx,
                        ))
                        .child(
                            self.render_connection_hint(
                                self.i18n.t("ssh.form.proxy_jump_kbi_hint"),
                            ),
                        )
                        .child(self.render_jump_auth_tabs(jump_form.auth_tab, cx))
                        .when(jump_form.auth_tab == SshAuthTab::DefaultKey, |content| {
                            content.child(
                                self.render_connection_hint(
                                    self.i18n.t("ssh.form.default_key_desc"),
                                ),
                            )
                        })
                        .when(jump_form.auth_tab == SshAuthTab::SshKey, |content| {
                            content
                                .child(self.render_connection_field_with_browse(
                                    self.i18n.t("ssh.form.proxy_jump_key_path"),
                                    &jump_form.key_path,
                                    self.i18n.t("ssh.form.proxy_jump_key_path_placeholder"),
                                    NewConnectionField::JumpKeyPath,
                                    cx,
                                ))
                                .child(self.render_connection_field(
                                    self.i18n.t("ssh.form.passphrase"),
                                    &jump_form.passphrase,
                                    String::new(),
                                    NewConnectionField::JumpPassphrase,
                                    true,
                                    cx,
                                ))
                        })
                        .when(jump_form.auth_tab == SshAuthTab::Certificate, |content| {
                            content
                                .child(self.render_connection_field_with_browse(
                                    self.i18n.t("ssh.form.private_key"),
                                    &jump_form.key_path,
                                    self.i18n.t("ssh.form.proxy_jump_key_path_placeholder"),
                                    NewConnectionField::JumpKeyPath,
                                    cx,
                                ))
                                .child(self.render_connection_field_with_browse(
                                    self.i18n.t("ssh.form.certificate"),
                                    &jump_form.cert_path,
                                    "~/.ssh/id_ed25519-cert.pub".to_string(),
                                    NewConnectionField::JumpCertPath,
                                    cx,
                                ))
                                .child(self.render_connection_field(
                                    self.i18n.t("ssh.form.passphrase"),
                                    &jump_form.passphrase,
                                    String::new(),
                                    NewConnectionField::JumpPassphrase,
                                    true,
                                    cx,
                                ))
                        })
                        .when(jump_form.auth_tab == SshAuthTab::Password, |content| {
                            content.child(self.render_connection_field(
                                self.i18n.t("ssh.form.password"),
                                &jump_form.password,
                                String::new(),
                                NewConnectionField::JumpPassword,
                                true,
                                cx,
                            ))
                        })
                        .when(jump_form.auth_tab == SshAuthTab::Agent, |content| {
                            content.child(self.render_connection_hint(
                                self.i18n.t("ssh.form.proxy_jump_agent_desc"),
                            ))
                        })
                        .child(self.render_connection_checkbox(
                            self.i18n.t("ssh.form.agent_forwarding"),
                            jump_form.agent_forwarding,
                            |form| {
                                if let Some(jump_form) = form.jump_server_form.as_mut() {
                                    jump_form.agent_forwarding = !jump_form.agent_forwarding;
                                }
                            },
                            cx,
                        )),
                )
                .child(
                    modal_footer(&self.tokens)
                        .child(self.render_jump_cancel_button(cx))
                        .child(self.render_jump_add_button(add_disabled, cx)),
                ),
        )
    }

    fn render_proxy_chain_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let (hops, expanded) = self
            .new_connection_form
            .as_ref()
            .map(|form| (form.proxy_hops.clone(), form.proxy_chain_expanded))
            .unwrap_or_default();
        let mut list = div()
            .id("new-connection-proxy-chain-scroll")
            .flex()
            .flex_col()
            .gap_2()
            .max_h(px(TAURI_PROXY_CHAIN_MAX_HEIGHT))
            .overflow_y_scroll();
        if hops.is_empty() {
            list = list.child(
                div()
                    .py(px(24.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ssh.form.proxy_chain_empty")),
            );
        } else {
            for (index, hop) in hops.iter().cloned().enumerate() {
                list = list.child(self.render_proxy_hop_summary(index, hop, cx));
            }
        }

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.i18n.t("ssh.form.proxy_chain_title")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(!hops.is_empty(), |row| {
                                row.child(self.render_proxy_chain_toggle(expanded, cx))
                            })
                            .child(self.render_add_jump_button(cx)),
                    ),
            )
            .child(if expanded {
                list.into_any_element()
            } else {
                div()
                    .py(px(24.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(if hops.is_empty() {
                        self.i18n.t("ssh.form.proxy_chain_empty")
                    } else {
                        self.i18n
                            .t("ssh.form.proxy_chain_count")
                            .replace("{{count}}", &hops.len().to_string())
                    })
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_proxy_chain_toggle(&self, expanded: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size(px(self.tokens.metrics.ui_button_icon_size))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .hover({
                let bg = rgba((self.tokens.ui.bg_hover << 8) | 0xaa);
                move |button| button.bg(bg)
            })
            .child(Self::render_lucide_icon(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                16.0,
                rgb(self.tokens.ui.text),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        form.proxy_chain_expanded = !form.proxy_chain_expanded;
                        form.field_focused = false;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_add_jump_button(&self, cx: &mut Context<Self>) -> AnyElement {
        button_with(
            &self.tokens,
            self.i18n.t("ssh.form.proxy_chain_add_jump"),
            ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                ..ButtonOptions::default()
            },
        )
        .child(Self::render_lucide_icon(
            LucideIcon::Plus,
            16.0,
            rgb(self.tokens.ui.text),
        ))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                if let Some(form) = this.new_connection_form.as_mut() {
                    form.jump_server_form = Some(super::form_state::NewConnectionProxyHop::new());
                    form.field_focused = true;
                    form.focused_field = NewConnectionField::JumpHost;
                    form.selected_field = None;
                }
                this.open_new_connection_select = None;
                this.new_connection_caret_visible = true;
                window.focus(&this.focus_handle);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_jump_add_button(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        button_with(
            &self.tokens,
            self.i18n.t("ssh.form.proxy_jump_add"),
            ButtonOptions {
                variant: ButtonVariant::Default,
                size: ButtonSize::Default,
                ..ButtonOptions::default()
            },
        )
        .opacity(if disabled { 0.5 } else { 1.0 })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                if !disabled {
                    this.add_pending_jump_server(cx);
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_jump_cancel_button(&self, cx: &mut Context<Self>) -> AnyElement {
        button_with(
            &self.tokens,
            self.i18n.t("ssh.form.cancel"),
            ButtonOptions {
                variant: ButtonVariant::Secondary,
                size: ButtonSize::Default,
                ..ButtonOptions::default()
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, _window, cx| {
                if let Some(form) = this.new_connection_form.as_mut() {
                    form.jump_server_form = None;
                    form.field_focused = false;
                    form.selected_field = None;
                }
                this.ime_marked_text = None;
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_proxy_hop_summary(
        &self,
        index: usize,
        hop: super::form_state::NewConnectionProxyHop,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let auth_label = match hop.auth_tab {
            SshAuthTab::DefaultKey => self.i18n.t("ssh.auth.default_key"),
            SshAuthTab::SshKey => self.i18n.t("ssh.auth.ssh_key"),
            SshAuthTab::Certificate => self.i18n.t("ssh.auth.certificate"),
            SshAuthTab::Password => self.i18n.t("ssh.auth.password"),
            SshAuthTab::Agent => self.i18n.t("ssh.auth.agent"),
            SshAuthTab::TwoFactor => self.i18n.t("ssh.auth.two_factor"),
        };
        let auth_icon = if matches!(hop.auth_tab, SshAuthTab::SshKey | SshAuthTab::DefaultKey) {
            LucideIcon::Key
        } else {
            LucideIcon::Lock
        };
        div()
            .relative()
            .child(
                div()
                    .absolute()
                    .left(px(TAURI_PROXY_CHAIN_NODE_SIZE / 2.0))
                    .top_0()
                    .bottom_0()
                    .w(px(1.0))
                    .when(index > 0, |line| {
                        line.child(
                            div()
                                .absolute()
                                .top(px(TAURI_PROXY_CHAIN_NODE_SIZE / 2.0))
                                .w(px(TAURI_PROXY_CHAIN_LINE_WIDTH))
                                .h(px(1.0))
                                .bg(rgb(self.tokens.ui.text_muted)),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .top(px(0.0))
                            .size(px(TAURI_PROXY_CHAIN_NODE_SIZE))
                            .rounded_full()
                            .border_2()
                            .border_color(rgb(self.tokens.ui.border))
                            .bg(rgb(self.tokens.ui.bg))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Self::render_lucide_icon(
                                auth_icon,
                                16.0,
                                rgb(self.tokens.ui.text_muted),
                            )),
                    ),
            )
            .child(
                div().flex().items_start().gap(px(24.0)).pl(px(48.0)).child(
                    div()
                        .flex_1()
                        .border_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .rounded(px(self.tokens.radii.lg))
                        .p(px(self.tokens.spacing.three))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_sm))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(format!(
                                            "{}. {}",
                                            index + 1,
                                            self.i18n.t("ssh.form.proxy_chain_jump_server")
                                        )),
                                )
                                .child(self.render_remove_jump_button(index, cx)),
                        )
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_host"),
                            hop.host,
                        ))
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_port"),
                            hop.port,
                        ))
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_username"),
                            hop.username,
                        ))
                        .child(self.render_proxy_hop_line(
                            self.i18n.t("ssh.form.proxy_chain_auth"),
                            auth_label,
                        )),
                ),
            )
            .into_any_element()
    }

    fn render_proxy_hop_line(&self, label: String, value: String) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .child(
                div()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("{label}:")),
            )
            .child(div().font_weight(gpui::FontWeight::MEDIUM).child(value))
            .into_any_element()
    }

    fn render_remove_jump_button(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                LucideIcon::Trash2,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut()
                        && index < form.proxy_hops.len()
                    {
                        form.proxy_hops.remove(index);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_jump_auth_tabs(&self, active_tab: SshAuthTab, cx: &mut Context<Self>) -> AnyElement {
        let tabs = [
            (SshAuthTab::DefaultKey, "ssh.auth.default_key"),
            (SshAuthTab::SshKey, "ssh.auth.ssh_key"),
            (SshAuthTab::Certificate, "ssh.auth.certificate"),
            (SshAuthTab::Password, "ssh.auth.password"),
            (SshAuthTab::Agent, "ssh.auth.agent"),
        ];
        let mut row = segmented_tabs(&self.tokens);
        for (tab, key) in tabs {
            row = row.child(
                segmented_tab(&self.tokens, self.i18n.t(key), tab == active_tab).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut()
                            && let Some(jump_form) = form.jump_server_form.as_mut()
                        {
                            jump_form.auth_tab = tab;
                            form.focused_field = match tab {
                                SshAuthTab::Password => NewConnectionField::JumpPassword,
                                SshAuthTab::SshKey | SshAuthTab::Certificate => {
                                    NewConnectionField::JumpKeyPath
                                }
                                _ => NewConnectionField::JumpHost,
                            };
                            clear_connection_selection(form);
                        }
                        cx.notify();
                    }),
                ),
            );
        }
        form_field(&self.tokens, self.i18n.t("ssh.form.proxy_jump_auth"), row)
    }

    fn add_pending_jump_server(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        let Some(jump_form) = form.jump_server_form.take() else {
            return;
        };
        if !jump_form.complete() {
            form.jump_server_form = Some(jump_form);
            form.error = Some(self.i18n.t("ssh.form.proxy_jump_required"));
            cx.notify();
            return;
        }
        form.proxy_hops.push(jump_form);
        form.proxy_chain_expanded = true;
        form.field_focused = false;
        form.selected_field = None;
        form.error = None;
        self.ime_marked_text = None;
        cx.notify();
    }

    fn render_connection_hint(&self, text: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(text)
            .into_any_element()
    }

    fn render_prompt_error_box(&self, error: String) -> AnyElement {
        let error_color = self.tokens.ui.error;
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((error_color << 8) | TAURI_PROMPT_ERROR_BORDER_ALPHA))
            .bg(rgba((error_color << 8) | TAURI_PROMPT_ERROR_ALPHA))
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.two))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(error_color))
            .child(error)
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
        form_field(
            &self.tokens,
            label,
            self.render_connection_input(value, placeholder, field, secret, cx),
        )
    }

    fn render_edit_saved_password_field(
        &self,
        form: &NewConnectionForm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = if form.password_loaded {
            form.password.as_str()
        } else {
            ""
        };
        let icon = if form.password_loading {
            LucideIcon::LoaderCircle
        } else if form.password_visible {
            LucideIcon::EyeOff
        } else {
            LucideIcon::Eye
        };
        let secret = form.password_loaded && !form.password_visible;
        form_field(
            &self.tokens,
            self.i18n.t("sessionManager.edit_properties.saved_password"),
            div()
                .relative()
                .child(
                    self.render_connection_input(
                        value,
                        self.i18n
                            .t("sessionManager.edit_properties.password_placeholder"),
                        NewConnectionField::Password,
                        secret,
                        cx,
                    ),
                )
                .child(
                    div()
                        .absolute()
                        .right(px(TAURI_PASSWORD_ICON_BUTTON_OFFSET))
                        .top(px(TAURI_PASSWORD_ICON_BUTTON_OFFSET))
                        .size(px(TAURI_PASSWORD_ICON_BUTTON_SIZE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.sm))
                        .opacity(if form.password_loading { 0.5 } else { 1.0 })
                        .cursor_pointer()
                        .hover({
                            let bg = rgba((self.tokens.ui.bg_hover << 8) | 0x99);
                            move |button| button.bg(bg)
                        })
                        .child(Self::render_lucide_icon(
                            icon,
                            TAURI_PASSWORD_ICON_SIZE,
                            rgb(self.tokens.ui.text_muted),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.toggle_edit_saved_password_visibility(cx);
                                cx.stop_propagation();
                            }),
                        ),
                ),
        )
    }

    fn render_connection_field_with_browse(
        &self,
        label: String,
        value: &str,
        placeholder: String,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            label,
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(self.render_connection_input(value, placeholder, field, false, cx)),
                )
                .child(
                    button_with(
                        &self.tokens,
                        self.i18n.t("sessionManager.edit_properties.browse"),
                        ButtonOptions {
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Sm,
                            ..ButtonOptions::default()
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.open_new_connection_select = None;
                            this.pick_new_connection_path(field, cx);
                            cx.stop_propagation();
                        }),
                    ),
                ),
        )
    }

    fn render_connection_group_select(
        &self,
        label: String,
        value: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_label = if self.connection_form_group_is_ungrouped(value) {
            self.connection_form_ungrouped_label()
        } else {
            value.trim().to_string()
        };
        let anchor_id = SelectAnchorId::NewConnectionGroup;
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, selected_label, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if let Some(form) = this.new_connection_form.as_mut() {
                        form.field_focused = false;
                        form.selected_field = None;
                    }
                    this.ime_marked_text = None;
                    this.open_new_connection_select =
                        if this.open_new_connection_select == Some(NewConnectionSelect::Group) {
                            None
                        } else {
                            Some(NewConnectionSelect::Group)
                        };
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        form_field(
            &self.tokens,
            label,
            select_anchor_probe(anchor_id, trigger, move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_select_anchor(anchor, cx);
                });
            }),
        )
    }

    fn set_new_connection_group(&mut self, group: String, cx: &mut Context<Self>) {
        if let Some(form) = self.new_connection_form.as_mut() {
            form.group = group;
            form.field_focused = false;
            form.selected_field = None;
            form.error = None;
        }
        self.open_new_connection_select = None;
        self.ime_marked_text = None;
        cx.notify();
    }

    fn connection_form_group_options(&self, current_group: &str) -> Vec<String> {
        let mut groups = self.connection_store.groups().to_vec();
        let current = current_group.trim();
        if !current.is_empty()
            && !self.connection_form_group_is_ungrouped(current)
            && !groups.iter().any(|group| group == current)
        {
            groups.push(current.to_string());
        }
        groups.sort();
        groups.dedup();
        groups
    }

    fn connection_form_group_is_ungrouped(&self, group: &str) -> bool {
        let group = group.trim();
        group.is_empty()
            || group == "Ungrouped"
            || group == "未分组"
            || group == self.i18n.t("ssh.form.ungrouped")
            || group == self.i18n.t("sessionManager.edit_properties.ungrouped")
    }

    fn connection_form_ungrouped_label(&self) -> String {
        self.i18n.t("ssh.form.ungrouped")
    }

    fn pick_new_connection_path(&mut self, field: NewConnectionField, cx: &mut Context<Self>) {
        if !matches!(
            field,
            NewConnectionField::KeyPath
                | NewConnectionField::CertPath
                | NewConnectionField::JumpKeyPath
                | NewConnectionField::JumpCertPath
        ) {
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("sessionManager.edit_properties.browse"),
            )),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let path = path.to_string_lossy().to_string();
            let _ = weak.update(cx, |this, cx| {
                if let Some(form) = this.new_connection_form.as_mut() {
                    match field {
                        NewConnectionField::KeyPath => form.key_path = path,
                        NewConnectionField::CertPath => form.cert_path = path,
                        NewConnectionField::JumpKeyPath => {
                            let Some(jump_form) = form.jump_server_form.as_mut() else {
                                return;
                            };
                            jump_form.key_path = path;
                        }
                        NewConnectionField::JumpCertPath => {
                            let Some(jump_form) = form.jump_server_form.as_mut() else {
                                return;
                            };
                            jump_form.cert_path = path;
                        }
                        _ => return,
                    }
                    form.focused_field = field;
                    form.field_focused = true;
                    form.error = None;
                    clear_connection_selection(form);
                }
                this.new_connection_caret_visible = true;
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_edit_saved_password_visibility(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.new_connection_form.as_mut() else {
            return;
        };
        if form.password_loading {
            return;
        }
        if form.password_loaded {
            form.password_visible = !form.password_visible;
            form.password_error = None;
            cx.notify();
            return;
        }

        let Some(connection_id) = self.editing_saved_connection_id.clone() else {
            return;
        };
        form.password_loading = true;
        form.password_error = None;
        cx.notify();

        let store = self.connection_store.clone();
        cx.spawn(async move |weak, cx| {
            let result = store.get_connection_password(&connection_id);
            let _ = weak.update(cx, |this, cx| {
                if let Some(form) = this.new_connection_form.as_mut() {
                    form.password_loading = false;
                    match result {
                        Ok(password) => {
                            form.password = password;
                            form.password_loaded = true;
                            form.password_visible = true;
                            form.password_error = None;
                            form.focused_field = NewConnectionField::Password;
                            form.field_focused = true;
                            clear_connection_selection(form);
                            this.new_connection_caret_visible = true;
                        }
                        Err(error) => {
                            form.password_error = Some(error.to_string());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_connection_input(
        &self,
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
                    this.open_new_connection_select = None;
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
        )
        .into_any_element()
    }

    fn render_prompt_auth_radios(
        &self,
        active_tab: SshAuthTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let choices = [
            (SshAuthTab::Password, "ssh.auth.password"),
            (SshAuthTab::SshKey, "ssh.auth.ssh_key"),
            (SshAuthTab::Agent, "ssh.auth.agent"),
            (SshAuthTab::Certificate, "ssh.auth.certificate"),
        ];
        let mut group = radio_group(&self.tokens).flex().flex_row().gap_4();
        for (tab, key) in choices {
            let selected = tab == active_tab
                || (tab == SshAuthTab::SshKey && active_tab == SshAuthTab::DefaultKey);
            group = group.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .child(radio_group_item(&self.tokens, selected, false))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(key)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(form) = this.new_connection_form.as_mut() {
                                form.auth_tab = tab;
                                clear_connection_selection(form);
                            }
                            this.open_new_connection_select = None;
                            cx.notify();
                        }),
                    ),
            );
        }
        form_field(
            &self.tokens,
            self.i18n.t("sessionManager.edit_properties.auth_type"),
            group,
        )
    }

    fn render_auth_tabs(
        &self,
        active_tab: SshAuthTab,
        edit_properties_mode: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tabs: Vec<(SshAuthTab, &str)> = if edit_properties_mode {
            vec![
                (
                    SshAuthTab::Password,
                    "sessionManager.edit_properties.auth_password",
                ),
                (
                    SshAuthTab::SshKey,
                    "sessionManager.edit_properties.auth_key",
                ),
                (SshAuthTab::Certificate, "ssh.auth.certificate"),
                (
                    SshAuthTab::Agent,
                    "sessionManager.edit_properties.auth_agent",
                ),
            ]
        } else {
            vec![
                (SshAuthTab::Password, "ssh.auth.password"),
                (SshAuthTab::DefaultKey, "ssh.auth.default_key"),
                (SshAuthTab::SshKey, "ssh.auth.ssh_key"),
                (SshAuthTab::Certificate, "ssh.auth.certificate"),
                (SshAuthTab::Agent, "ssh.auth.agent"),
                (SshAuthTab::TwoFactor, "ssh.auth.two_factor"),
            ]
        };
        let mut row = segmented_tabs(&self.tokens);
        for (tab, key) in tabs {
            let selected = tab == active_tab
                || (edit_properties_mode
                    && tab == SshAuthTab::SshKey
                    && active_tab == SshAuthTab::DefaultKey);
            row = row.child(
                segmented_tab(&self.tokens, self.i18n.t(key), selected).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if let Some(form) = this.new_connection_form.as_mut() {
                            form.auth_tab = tab;
                            clear_connection_selection(form);
                        }
                        this.open_new_connection_select = None;
                        cx.notify();
                    }),
                ),
            );
        }
        row.into_any_element()
    }

    fn render_edit_color_field(&self, value: &str, cx: &mut Context<Self>) -> AnyElement {
        let swatch = parse_form_hex_color(value).unwrap_or(TAURI_EDIT_COLOR_FALLBACK);
        form_field(
            &self.tokens,
            self.i18n.t("sessionManager.edit_properties.color"),
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .size(px(self.tokens.metrics.form_input_height))
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgb(self.tokens.ui.border))
                        .bg(rgb(swatch)),
                )
                .child(div().flex_1().child(self.render_connection_input(
                    value,
                    TAURI_EDIT_COLOR_FALLBACK_TEXT.to_string(),
                    NewConnectionField::Color,
                    false,
                    cx,
                )))
                .when(!value.is_empty(), |row| {
                    row.child(
                        button(
                            &self.tokens,
                            self.i18n.t("sessionManager.edit_properties.clear_color"),
                            ButtonTone::Secondary,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Some(form) = this.new_connection_form.as_mut() {
                                    form.color.clear();
                                    clear_connection_selection(form);
                                }
                                cx.notify();
                            }),
                        ),
                    )
                }),
        )
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
                    this.open_new_connection_select = None;
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
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant: if primary {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Secondary
                },
                disabled,
                ..ButtonOptions::default()
            },
        );
        if disabled {
            return control.into_any_element();
        }
        control
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

fn parse_form_hex_color(value: &str) -> Option<u32> {
    let trimmed = value.trim().trim_start_matches('#');
    if trimmed.len() != 6 {
        return None;
    }
    u32::from_str_radix(trimmed, 16).ok()
}
