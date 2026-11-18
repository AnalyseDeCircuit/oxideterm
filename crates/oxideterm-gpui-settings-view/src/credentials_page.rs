use crate::{
    SettingsInput, SettingsSelect,
    components::SettingsControls,
    entity::{PrivilegeCredentialSnapshot, SettingsWorkspaceEntity},
    surface::nav::render_icon,
};
use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, Rgba, ScrollHandle, Window,
    div, prelude::*, px, rgb, rgba,
};
use oxideterm_connections::{
    ConnectionStore, LOCAL_SHELL_PRIVILEGE_CONNECTION_ID, PrivilegePromptType,
    PrivilegeResponseMode, SSH_COMMON_PRIVILEGE_SCOPE_ID, SavedPrivilegeCredential,
    totp::{TotpAlgorithm, TotpError},
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions, icon_button, toolbar_button,
    },
    checkbox::checkbox,
    form_field,
    scroll::ScrollableElement,
    text_input::TextInputContentAlign,
};
use oxideterm_i18n::I18n;
use std::{collections::HashSet, ops::Deref, rc::Rc};
mod totp;

const PRIVILEGE_SCOPE_LIST_WIDTH: f32 = 280.0; // Match the current scope rail width on comfortable layouts.
const PRIVILEGE_DETAIL_MIN_WIDTH: f32 = 320.0; // Wrap the detail pane before fixed controls crush its labels.
const PRIVILEGE_FORM_FIELD_MIN_WIDTH: f32 = 260.0; // Keep paired fields readable before the form wraps to one column.
const PRIVILEGE_SCOPE_LIST_MAX_HEIGHT: f32 = 360.0;

#[derive(Clone, Debug, Eq, PartialEq)]
struct SettingsPrivilegeScopeRow {
    id: String,
    title: String,
    subtitle: String,
    username_placeholder: String,
    credential_count: usize,
    local: bool,
    ssh_common: bool,
}

pub fn settings_privilege_prompt_type_label(
    i18n: &I18n,
    prompt_type: PrivilegePromptType,
) -> String {
    i18n.t(match prompt_type {
        PrivilegePromptType::Sudo => "sessionManager.privilege_credentials.prompt_type.sudo",
        PrivilegePromptType::Su => "sessionManager.privilege_credentials.prompt_type.su",
        PrivilegePromptType::Custom => "sessionManager.privilege_credentials.prompt_type.custom",
    })
}

pub fn settings_privilege_response_mode_label(i18n: &I18n, mode: PrivilegeResponseMode) -> String {
    i18n.t(match mode {
        PrivilegeResponseMode::PasswordOnly => {
            "sessionManager.privilege_credentials.response_mode.password_only"
        }
        PrivilegeResponseMode::PasswordThenTotp => {
            "sessionManager.privilege_credentials.response_mode.password_then_totp"
        }
        PrivilegeResponseMode::PasswordWithTotp => {
            "sessionManager.privilege_credentials.response_mode.password_with_totp"
        }
    })
}

mod actions;
pub use actions::CredentialOutcome;

#[derive(Clone)]
pub enum CredentialRequest {
    SelectScope(String),
    BeginPrivilege,
    EditPrivilege(String, String),
    DeletePrivilege(String, String),
    CancelPrivilege,
    SavePrivilege,
    EditTotp(Option<String>),
    DeleteTotp(String),
    CancelTotp,
    SaveTotp,
}

pub struct CredentialPage<'a> {
    pub controls: SettingsControls<'a>,
    pub state: &'a Entity<SettingsWorkspaceEntity>,
    pub store: &'a ConnectionStore,
    pub scroll: &'a dyn Fn(&str) -> ScrollHandle,
    pub dispatch: Rc<dyn Fn(CredentialRequest, &mut App)>,
}

impl<'a> Deref for CredentialPage<'a> {
    type Target = SettingsControls<'a>;

    fn deref(&self) -> &Self::Target {
        &self.controls
    }
}

impl CredentialPage<'_> {
    pub fn section(&self, section: oxideterm_settings_model::SettingsSectionId, cx: &App) -> AnyElement {
        use oxideterm_settings_model::SettingsSectionId::*;
        match section {
            PrivilegeCredentials => self.credentials(cx),
            PrivilegeTotp => self.totp(cx),
            _ => div().into_any_element(),
        }
    }

    fn press(
        &self,
        request: CredentialRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let dispatch = self.dispatch.clone();
        move |_, _, cx| {
            dispatch(request.clone(), cx);
            cx.stop_propagation();
        }
    }

    fn select<F>(&self, update: F) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<F>
    where
        F: Fn(&mut SettingsWorkspaceEntity, &mut Context<SettingsWorkspaceEntity>) + 'static,
    {
        let state = self.state.downgrade();
        move |_, _, cx| {
            let _ = state.update(cx, |state, cx| update(state, cx));
            cx.stop_propagation();
        }
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    press(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }

    fn workspace_icon_action_button(
        &self,
        icon: &'static str,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        press: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _: &App,
    ) -> Div {
        let enabled = !(options.disabled || options.loading);
        icon_button(self.tokens, render_icon(icon, size, color), options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    press(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }

    pub fn credentials(&self, cx: &App) -> AnyElement {
        let theme = self.tokens.ui;
        let (selected_scope_id, editor_open) = {
            let settings = self.state.read(cx);
            let (scope_id, _editing, editor_open, _error) = settings.privilege_layout_flags();
            (scope_id.map(str::to_string), editor_open)
        };
        let scopes = self.settings_privilege_scope_rows();
        let active_scope_id = self.settings_privilege_active_scope_id(selected_scope_id.as_deref());
        let active_scope = scopes
            .iter()
            .find(|scope| scope.id == active_scope_id)
            .cloned()
            .unwrap_or_else(|| self.settings_local_privilege_scope_row());
        let credentials = self
            .store
            .list_privilege_credentials(&active_scope.id)
            .unwrap_or_default();
        let scope_scroll = (self.scroll)("settings-privilege-scopes-scroll");

        let mut scope_list = div().flex().flex_col().gap(px(2.0));
        for scope in scopes {
            let selected = scope.id == active_scope.id;
            let scope_id = scope.id.clone();
            scope_list = scope_list.child(
                div()
                    .w_full()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if selected {
                        rgba((theme.accent << 8) | 0x99)
                    } else {
                        rgba(theme.border << 8)
                    })
                    .bg(if selected {
                        rgba((theme.accent << 8) | 0x14)
                    } else {
                        rgba(theme.bg_panel << 8)
                    })
                    .px(px(10.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .cursor_pointer()
                    .hover(move |row| row.bg(rgba((theme.bg_hover << 8) | 0xcc)))
                    .on_mouse_down(
                        MouseButton::Left,
                        self.press(CredentialRequest::SelectScope(scope_id)),
                    )
                    .child(render_icon(
                        if scope.local {
                            "lucide/terminal.svg"
                        } else {
                            "lucide/server.svg"
                        },
                        16.0,
                        if selected {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text_muted)
                        },
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(scope.title.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(scope.subtitle.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .min_w(px(20.0))
                            .text_align(gpui::TextAlign::Right)
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(scope.credential_count.to_string()),
                    ),
            );
        }

        let editor_visible = credentials.is_empty() || editor_open;
        let privilege = editor_visible.then(|| self.state.read(cx).privilege_snapshot());

        let body = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .gap(px(16.0))
            .items_start()
            .child(
                div()
                    .w(px(PRIVILEGE_SCOPE_LIST_WIDTH))
                    .max_w_full()
                    .min_w(px(0.0))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("settings_view.privilege_credentials.scopes")),
                    )
                    .child(
                        div()
                            .id("settings-privilege-scopes")
                            .relative()
                            .w_full()
                            .max_h(px(PRIVILEGE_SCOPE_LIST_MAX_HEIGHT))
                            .child(
                                div()
                                    .id("settings-privilege-scopes-scroll")
                                    .w_full()
                                    .max_h(px(PRIVILEGE_SCOPE_LIST_MAX_HEIGHT))
                                    .overflow_y_scroll()
                                    .track_scroll(&scope_scroll)
                                    .on_scroll(|_, _, cx| cx.stop_propagation())
                                    .child(scope_list),
                            )
                            .vertical_scrollbar(&scope_scroll),
                    ),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex_basis(px(PRIVILEGE_DETAIL_MIN_WIDTH))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(self.settings_privilege_active_header(
                        &active_scope,
                        !editor_visible,
                        cx,
                    ))
                    .child(self.settings_privilege_credential_list(&active_scope, &credentials, cx))
                    .when(editor_visible, |detail| {
                        detail.child(self.settings_privilege_credential_form(
                            &active_scope,
                            !credentials.is_empty(),
                            privilege.as_ref().expect("visible privilege editor"),
                            cx,
                        ))
                    }),
            );

        self.settings_card(
            "settings_view.privilege_credentials.title",
            "settings_view.privilege_credentials.description",
            vec![body.into_any_element()],
        )
    }

    fn settings_privilege_active_header(
        &self,
        scope: &SettingsPrivilegeScopeRow,
        show_new_action: bool,
        _cx: &App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(scope.title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(scope.subtitle.clone()),
                    ),
            )
            .when(show_new_action, |header| {
                header.child(
                    self.workspace_toolbar_action_button(
                        self.i18n.t("sessionManager.privilege_credentials.new"),
                        Some(
                            render_icon("lucide/plus.svg", 14.0, rgb(theme.text_muted))
                                .into_any_element(),
                        ),
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        self.press(CredentialRequest::BeginPrivilege),
                    ),
                )
            })
            .into_any_element()
    }

    fn settings_privilege_credential_list(
        &self,
        scope: &SettingsPrivilegeScopeRow,
        credentials: &[SavedPrivilegeCredential],
        cx: &App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut list = div().w_full().min_w(px(0.0)).flex().flex_col();
        if credentials.is_empty() {
            return list
                .child(
                    div()
                        .py(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("sessionManager.privilege_credentials.empty")),
                )
                .into_any_element();
        }

        for credential in credentials {
            let edit_scope = scope.id.clone();
            let edit_credential = credential.id.clone();
            let delete_scope = scope.id.clone();
            let delete_id = credential.id.clone();
            list = list.child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x80))
                    .px(px(8.0))
                    .py(px(8.0))
                    .child(render_icon(
                        "lucide/key-round.svg",
                        16.0,
                        rgb(theme.warning),
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(theme.text))
                                    .child(credential.label.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!(
                                        "{} · {}",
                                        settings_privilege_prompt_type_label(
                                            self.i18n,
                                            credential.prompt_type
                                        ),
                                        settings_privilege_response_mode_label(
                                            self.i18n,
                                            credential.response_mode
                                        )
                                    )),
                            ),
                    )
                    .when(!credential.enabled, |row| {
                        row.child(
                            div()
                                .rounded_full()
                                .px(px(8.0))
                                .py(px(2.0))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .bg(rgba((theme.bg_hover << 8) | 0x99))
                                .child(self.i18n.t("settings_view.privilege_credentials.disabled")),
                        )
                    })
                    .child(self.workspace_toolbar_action_button(
                        self.i18n.t("sessionManager.privilege_credentials.edit"),
                        None,
                        ToolbarButtonOptions {
                            button: ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                ..ButtonOptions::default()
                            },
                            ..ToolbarButtonOptions::default()
                        },
                        self.press(CredentialRequest::EditPrivilege(
                            edit_scope,
                            edit_credential,
                        )),
                    ))
                    .child(self.workspace_icon_action_button(
                        "lucide/trash-2.svg",
                        14.0,
                        rgb(theme.text_muted),
                        IconButtonOptions {
                            hover_background: Some(rgb(theme.bg_hover)),
                            ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                        },
                        self.press(CredentialRequest::DeletePrivilege(delete_scope, delete_id)),
                        cx,
                    )),
            );
        }
        list.into_any_element()
    }

    fn settings_privilege_credential_form(
        &self,
        scope: &SettingsPrivilegeScopeRow,
        show_cancel: bool,
        privilege: &PrivilegeCredentialSnapshot,
        cx: &App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let editing = privilege.credential_id.is_some();
        let field_slot = |field: AnyElement| {
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex_basis(px(PRIVILEGE_FORM_FIELD_MIN_WIDTH))
                .child(field)
        };
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .border_t_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .pt(px(14.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.i18n.t(if editing {
                        "sessionManager.privilege_credentials.edit"
                    } else {
                        "sessionManager.privilege_credentials.new"
                    })),
            )
            // The paired fields use flexible slots so wide settings surfaces do
            // not leave an oversized empty panel while narrow ones wrap cleanly.
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .child(field_slot(
                        self.settings_privilege_text_field(
                            "sessionManager.privilege_credentials.label",
                            SettingsInput::LocalPrivilegeLabel,
                            &privilege.label,
                            self.i18n
                                .t("sessionManager.privilege_credentials.label_placeholder"),
                            false,
                            cx,
                        ),
                    ))
                    .child(field_slot(self.settings_privilege_prompt_type_field(
                        privilege.prompt_type,
                        cx,
                    )))
                    .child(field_slot(self.settings_privilege_response_mode_field(
                        privilege.response_mode,
                        cx,
                    ))),
            )
            .when(scope.ssh_common, |panel| {
                panel
                    .child(
                        self.settings_privilege_host_patterns_field(&privilege.host_patterns, cx),
                    )
                    .child(
                        self.settings_privilege_hint(
                            self.i18n
                                .t("sessionManager.privilege_credentials.host_patterns_hint"),
                        ),
                    )
            })
            .child(self.settings_privilege_prompt_patterns_field(&privilege.prompt_patterns, cx))
            .child(
                self.settings_privilege_hint(
                    self.i18n
                        .t("sessionManager.privilege_credentials.prompt_patterns_hint"),
                ),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .child(field_slot(self.settings_privilege_text_field(
                        "sessionManager.privilege_credentials.secret",
                        SettingsInput::LocalPrivilegeSecret,
                        "",
                        if editing {
                            self.i18n
                                .t("sessionManager.privilege_credentials.secret_keep_placeholder")
                        } else {
                            self.i18n
                                .t("sessionManager.privilege_credentials.secret_placeholder")
                        },
                        true,
                        cx,
                    )))
                    .child(field_slot(self.settings_privilege_text_field(
                        "sessionManager.privilege_credentials.username_hint",
                        SettingsInput::LocalPrivilegeUsernameHint,
                        &privilege.username_hint,
                        scope.username_placeholder.clone(),
                        false,
                        cx,
                    ))),
            )
            .when(privilege.response_mode.waits_for_totp(), |panel| {
                panel
                    .child(self.settings_privilege_second_factor_patterns_field(
                        &privilege.second_factor_patterns,
                        cx,
                    ))
                    .child(self.settings_privilege_hint(
                        self.i18n.t(
                            "sessionManager.privilege_credentials.second_factor_patterns_hint",
                        ),
                    ))
            })
            .when(privilege.response_mode.uses_totp(), |panel| {
                panel
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .flex()
                            .flex_wrap()
                            .gap(px(12.0))
                            .child(field_slot(self.settings_privilege_totp_field(
                                privilege.totp_credential_id.as_deref(),
                                cx,
                            )))
                            .when(privilege.response_mode.waits_for_totp(), |row| {
                                row.child(field_slot(self.settings_privilege_text_field(
                                    "sessionManager.privilege_credentials.second_factor_user_hint",
                                    SettingsInput::LocalPrivilegeSecondFactorUserHint,
                                    &privilege.second_factor_user_hint,
                                    self.i18n.t(
                                        "sessionManager.privilege_credentials.second_factor_user_hint_placeholder",
                                    ),
                                    false,
                                    cx,
                                )))
                            }),
                    )
                    .child(self.settings_privilege_hint(
                        self.i18n
                            .t("sessionManager.privilege_credentials.totp_hint"),
                    ))
                    .when(
                        privilege.response_mode == PrivilegeResponseMode::PasswordWithTotp,
                        |panel| {
                            panel.child(self.settings_privilege_hint(
                                self.i18n.t(
                                    "sessionManager.privilege_credentials.two_stage_combined_hint",
                                ),
                            ))
                        },
                    )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        self.select(SettingsWorkspaceEntity::toggle_privilege_enabled),
                    )
                    .child(checkbox(&self.tokens, "privilege-enabled", String::new(), privilege.enabled))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("sessionManager.privilege_credentials.enabled")),
                    ),
            )
            .child(self.settings_privilege_auto_submit_row(privilege.require_click_to_send, cx))
            .child(
                self.settings_privilege_hint(
                    self.i18n
                        .t("sessionManager.privilege_credentials.auto_submit_hint"),
                ),
            )
            .when_some(privilege.error.clone(), |panel, error| {
                panel.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.error))
                        .child(error),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(8.0))
                    .when(show_cancel, |row| {
                        row.child(
                            self.workspace_toolbar_action_button(
                                self.i18n
                                    .t("sessionManager.privilege_credentials.cancel_edit"),
                                None,
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        ..ButtonOptions::default()
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                self.press(CredentialRequest::CancelPrivilege),
                            ),
                        )
                    })
                    .child(
                        self.workspace_toolbar_action_button(
                            self.i18n.t("sessionManager.privilege_credentials.save"),
                            Some(
                                render_icon(
                                    "lucide/save.svg",
                                    14.0,
                                    rgb(theme.text_muted),
                                )
                                .into_any_element(),
                            ),
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Default,
                                    size: ButtonSize::Sm,
                                    disabled: privilege.label.trim().is_empty(),
                                    ..ButtonOptions::default()
                                },
                                ..ToolbarButtonOptions::default()
                            },
                            self.press(CredentialRequest::SavePrivilege),
                        ),
                    ),
            )
            .into_any_element()
    }

    fn settings_privilege_text_field(
        &self,
        label_key: &'static str,
        input: SettingsInput,
        value: impl AsRef<str>,
        placeholder: String,
        secret: bool,
        cx: &App,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t(label_key),
            if secret {
                // Entity-owned secret controls borrow their live value in the shared
                // input renderer so frame snapshots never duplicate credentials.
                self.field(
                    input,
                    value.as_ref(),
                    placeholder,
                    None,
                    TextInputContentAlign::Start,
                    true,
                    cx,
                )
            } else {
                self.settings_text_input_control_fill(input, value, placeholder, cx)
            },
        )
    }

    fn settings_privilege_prompt_type_field(
        &self,
        prompt_type: PrivilegePromptType,
        cx: &App,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("sessionManager.privilege_credentials.prompt_type_label"),
            self.settings_select_control(
                SettingsSelect::LocalPrivilegePromptType,
                settings_privilege_prompt_type_label(self.i18n, prompt_type),
                false,
                None,
                cx,
            ),
        )
    }

    fn settings_privilege_response_mode_field(
        &self,
        response_mode: PrivilegeResponseMode,
        cx: &App,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("sessionManager.privilege_credentials.response_mode_label"),
            self.settings_select_control(
                SettingsSelect::LocalPrivilegeResponseMode,
                settings_privilege_response_mode_label(self.i18n, response_mode),
                false,
                None,
                cx,
            ),
        )
    }

    fn settings_privilege_totp_field(&self, selected: Option<&str>, cx: &App) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n
                .t("sessionManager.privilege_credentials.totp_credential"),
            self.settings_select_control(
                SettingsSelect::LocalPrivilegeTotpCredential,
                self.settings_privilege_totp_label(selected),
                false,
                None,
                cx,
            ),
        )
    }

    fn settings_privilege_totp_label(&self, selected: Option<&str>) -> String {
        selected
            .and_then(|id| {
                self.store
                    .totp_credentials()
                    .iter()
                    .find(|credential| credential.id == id)
            })
            .map(|credential| {
                if credential.enabled {
                    credential.name.clone()
                } else {
                    format!(
                        "{} ({})",
                        credential.name,
                        self.i18n.t("settings_view.privilege_credentials.disabled")
                    )
                }
            })
            .unwrap_or_else(|| {
                self.i18n
                    .t("sessionManager.privilege_credentials.totp_none")
            })
    }

    fn settings_privilege_auto_submit_row(
        &self,
        require_click_to_send: bool,
        _cx: &App,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                self.select(SettingsWorkspaceEntity::toggle_privilege_auto_submit),
            )
            .child(checkbox(
                &self.tokens,
                "privilege-auto-submit",
                String::new(),
                !require_click_to_send,
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        self.i18n
                            .t("sessionManager.privilege_credentials.auto_submit"),
                    ),
            )
            .into_any_element()
    }

    fn settings_privilege_prompt_patterns_field(&self, value: &str, cx: &App) -> AnyElement {
        self.settings_privilege_patterns_field(
            SettingsInput::LocalPrivilegePromptPatterns,
            value,
            "sessionManager.privilege_credentials.prompt_patterns",
            "sessionManager.privilege_credentials.prompt_patterns_placeholder",
            cx,
        )
    }

    fn settings_privilege_host_patterns_field(&self, value: &str, cx: &App) -> AnyElement {
        self.settings_privilege_patterns_field(
            SettingsInput::LocalPrivilegeHostPatterns,
            value,
            "sessionManager.privilege_credentials.host_patterns",
            "sessionManager.privilege_credentials.host_patterns_placeholder",
            cx,
        )
    }

    fn settings_privilege_second_factor_patterns_field(&self, value: &str, cx: &App) -> AnyElement {
        self.settings_privilege_patterns_field(
            SettingsInput::LocalPrivilegeSecondFactorPatterns,
            value,
            "sessionManager.privilege_credentials.second_factor_patterns",
            "sessionManager.privilege_credentials.second_factor_patterns_placeholder",
            cx,
        )
    }

    fn settings_privilege_hint(&self, text: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(text)
            .into_any_element()
    }

    fn settings_privilege_scope_rows(&self) -> Vec<SettingsPrivilegeScopeRow> {
        let mut rows = vec![
            self.settings_local_privilege_scope_row(),
            SettingsPrivilegeScopeRow {
                id: SSH_COMMON_PRIVILEGE_SCOPE_ID.to_string(),
                title: self
                    .i18n
                    .t("settings_view.privilege_credentials.ssh_common_scope"),
                subtitle: self
                    .i18n
                    .t("settings_view.privilege_credentials.ssh_common_scope_hint"),
                username_placeholder: String::new(),
                credential_count: self
                    .store
                    .list_privilege_credentials(SSH_COMMON_PRIVILEGE_SCOPE_ID)
                    .map(|credentials| credentials.len())
                    .unwrap_or(0),
                local: false,
                ssh_common: true,
            },
        ];
        // Saved SSH connections already own privilege credential metadata in the
        // connection store. Expose those explicit scopes instead of letting the
        // terminal helper infer remote secrets from host/title/runtime matches.
        rows.extend(
            self.store
                .connections()
                .iter()
                .map(|connection| self.settings_saved_connection_privilege_scope_row(connection)),
        );
        rows
    }

    fn settings_local_privilege_scope_row(&self) -> SettingsPrivilegeScopeRow {
        SettingsPrivilegeScopeRow {
            id: LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string(),
            title: self
                .i18n
                .t("settings_view.privilege_credentials.local_scope"),
            subtitle: self
                .i18n
                .t("settings_view.privilege_credentials.local_scope_hint"),
            username_placeholder: self.settings_local_username_hint().unwrap_or_else(|| {
                self.i18n
                    .t("settings_view.local_terminal.privilege_username_placeholder")
            }),
            credential_count: self
                .store
                .list_privilege_credentials(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
                .map(|credentials| credentials.len())
                .unwrap_or(0),
            local: true,
            ssh_common: false,
        }
    }

    fn settings_saved_connection_privilege_scope_row(
        &self,
        connection: &oxideterm_connections::SavedConnection,
    ) -> SettingsPrivilegeScopeRow {
        SettingsPrivilegeScopeRow {
            id: connection.id.clone(),
            title: connection.name.clone(),
            subtitle: format!(
                "{}@{}:{}",
                connection.username, connection.host, connection.port
            ),
            username_placeholder: connection.username.clone(),
            credential_count: connection.privilege_credentials.len(),
            local: false,
            ssh_common: false,
        }
    }

    fn settings_privilege_active_scope_id(&self, selected: Option<&str>) -> String {
        if let Some(selected) = selected
            && self
                .settings_privilege_scope_rows()
                .iter()
                .any(|scope| scope.id == selected)
        {
            return selected.to_string();
        }
        LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string()
    }

    fn settings_local_username_hint(&self) -> Option<String> {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
    fn settings_privilege_patterns_field(
        &self,
        input: SettingsInput,
        value: &str,
        label_key: &'static str,
        placeholder_key: &'static str,
        cx: &App,
    ) -> AnyElement {
        form_field(
            self.tokens,
            self.i18n.t(label_key),
            {
                let frame = self.ime.frame(input, value, cx);
                let scroll = (self.scroll)(&format!("multiline-input-{}", frame.anchor_id.0));
                crate::components::settings_multiline_patterns_field(
                    self.tokens, frame, &self.i18n.t(placeholder_key),
                    &scroll, self.controls.dispatch.clone(),
                )
            },
        )
    }
}
