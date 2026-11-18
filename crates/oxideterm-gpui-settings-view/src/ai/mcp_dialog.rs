use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, Render, Rgba, ScrollHandle,
    Window, div, prelude::*, px, relative, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    ConfirmDialogAction,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions, icon_button, toolbar_button,
    },
    checkbox::checkbox,
    icons::LucideIcon,
    modal::{
        dialog_content, dialog_description, dialog_footer, dialog_header, dialog_title,
        dismissible_dialog_backdrop,
    },
    text_input::TextInputContentAlign,
};
use oxideterm_i18n::I18n;
use oxideterm_settings_model::ai_mcp_transport_label;
use oxideterm_theme::ThemeTokens;

use super::mcp::{AiMcpDispatcher, AiMcpRequest};
use crate::{
    SettingsInput, SettingsSelect, components::SettingsControlsSource,
    dialog::settings_dialog_transition, entity::SettingsWorkspaceEntity,
};

const AI_MCP_DIALOG_WIDTH: f32 = 672.0; // Tauri DialogContent sm:max-w-2xl.
const AI_MCP_DIALOG_CONTENT_PX: f32 = 16.0; // Tauri px-4.
const AI_MCP_DIALOG_CONTENT_PY: f32 = 8.0; // Tauri py-2.
const AI_MCP_FORM_GAP: f32 = 16.0; // Tauri space-y-4.
const AI_MCP_FIELD_GAP: f32 = 8.0; // Tauri space-y-2 / gap-2.
const AI_MCP_ARGS_TEXTAREA_MIN_H: f32 = 84.0; // Tauri textarea-sized MCP args field.

pub struct AiMcpDialogView {
    state: Entity<SettingsWorkspaceEntity>,
    controls: SettingsControlsSource,
    request: AiMcpDispatcher,
    tokens: ThemeTokens,
    i18n: I18n,
    scroll: ScrollHandle,
    footer_focus: Option<ConfirmDialogAction>,
}

impl AiMcpDialogView {
    pub fn new(
        state: Entity<SettingsWorkspaceEntity>,
        controls: SettingsControlsSource,
        request: AiMcpDispatcher,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            controls,
            request,
            tokens: oxideterm_theme::default_tokens(),
            i18n: I18n::default(),
            scroll: ScrollHandle::new(),
            footer_focus: None,
        }
    }

    pub fn set_focus(&mut self, focus: Option<ConfirmDialogAction>, cx: &mut Context<Self>) {
        self.footer_focus = focus;
        cx.notify();
    }

    fn finish(&mut self, submit: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.footer_focus = None;
        (self.request)(AiMcpRequest::FinishDialog { submit }, window, cx);
    }

    fn standard_footer_action_button(
        &self,
        label: String,
        variant: ButtonVariant,
        action: ConfirmDialogAction,
        disabled: bool,
        cx: &Context<Self>,
    ) -> Div {
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    disabled,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                },
                focus_visible: self.footer_focus == Some(action),
                ..Default::default()
            },
            cx.listener(move |this, _, window, cx| {
                this.finish(action == ConfirmDialogAction::Confirm, window, cx);
                cx.stop_propagation();
            }),
        )
    }

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        action: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let enabled = !(options.button.disabled || options.loading);
        toolbar_button(&self.tokens, label, icon, options).on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                if enabled {
                    action(event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            },
        )
    }

    fn ai_icon_button(
        &self,
        icon: LucideIcon,
        disabled: bool,
        action: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> AnyElement {
        let color = if icon == LucideIcon::Trash2 {
            self.tokens.ui.error
        } else {
            self.tokens.ui.text_muted
        };
        icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, 15.0, rgb(color)),
            IconButtonOptions {
                disabled,
                hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x80)),
                ..IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md)
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event, window, cx| {
                if !disabled {
                    action(this, event, window, cx);
                } else {
                    cx.stop_propagation();
                }
            }),
        )
        .into_any_element()
    }

    fn render_selectable_display_text(
        &self,
        scope: &str,
        key: &str,
        value: String,
        color: u32,
        cx: &App,
    ) -> AnyElement {
        match (self.controls)(cx) {
            Some(controls) => (controls.label)(scope, key, value, color, cx),
            None => div().child(value).into_any_element(),
        }
    }

    fn ai_textarea_row(
        &self,
        input: SettingsInput,
        label: String,
        hint: String,
        placeholder: String,
        value: String,
        min_height: f32,
        cx: &App,
    ) -> AnyElement {
        let Some(controls) = (self.controls)(cx) else {
            return div().into_any_element();
        };
        super::textarea(
            &controls,
            input,
            label,
            hint,
            placeholder,
            value,
            min_height,
            cx,
        )
    }

    fn render_ai_mcp_add_server_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let controls = (self.controls)(cx)?;
        let state = &self.state.read(cx).ai_mcp;
        let draft = state.draft()?;
        let (can_add, transport, auth_header_mode, phase) = (
            state.valid(controls.settings),
            draft.transport,
            draft.auth_header_mode,
            state.phase(),
        );
        let transport_label = ai_mcp_transport_label(transport);
        let auth_mode_label = match auth_header_mode {
            oxideterm_ai::McpAuthHeaderMode::Bearer => {
                self.i18n.t("settings_view.mcp.auth_header_mode_bearer")
            }
            oxideterm_ai::McpAuthHeaderMode::Raw => {
                self.i18n.t("settings_view.mcp.auth_header_mode_raw")
            }
            oxideterm_ai::McpAuthHeaderMode::None => {
                self.i18n.t("settings_view.mcp.auth_header_mode_none")
            }
        };

        let backdrop = dismissible_dialog_backdrop().on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                this.finish(false, window, cx);
                cx.stop_propagation();
            }),
        );
        let form = dialog_content(&self.tokens)
            .w(px(AI_MCP_DIALOG_WIDTH))
            .max_w(relative(0.92))
            .max_h(relative(0.86))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                dialog_header(&self.tokens)
                    .child(dialog_title(
                        &self.tokens,
                        self.i18n.t("settings_view.mcp.add_server_title"),
                    ))
                    .child(dialog_description(
                        &self.tokens,
                        self.i18n.t("settings_view.mcp.add_server_description"),
                    )),
            )
            .child(
                div()
                    .id("ai-mcp-add-server-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(px(AI_MCP_DIALOG_CONTENT_PX))
                    .py(px(AI_MCP_DIALOG_CONTENT_PY))
                    .flex()
                    .flex_col()
                    .gap(px(AI_MCP_FORM_GAP))
                    .child(self.ai_mcp_labeled_input(
                        "settings_view.mcp.server_name",
                        SettingsInput::AiMcpName,
                        "my-mcp-server".to_string(),
                        cx,
                    ))
                    .child(self.ai_mcp_labeled_select(
                        "settings_view.mcp.transport",
                        SettingsSelect::AiMcpTransport,
                        transport_label,
                        cx,
                    ))
                    .children(self.ai_mcp_transport_fields(transport, auth_mode_label, cx)),
            )
            .child(
                dialog_footer(&self.tokens)
                    .child(self.standard_footer_action_button(
                        self.i18n.t("settings_view.mcp.cancel"),
                        ButtonVariant::Outline,
                        ConfirmDialogAction::Cancel,
                        false,
                        cx,
                    ))
                    .child(self.standard_footer_action_button(
                        self.i18n.t("settings_view.mcp.add"),
                        ButtonVariant::Default,
                        ConfirmDialogAction::Confirm,
                        !can_add,
                        cx,
                    )),
            );
        Some(settings_dialog_transition(
            &self.tokens,
            "ai-mcp-dialog-form",
            backdrop,
            form,
            phase,
        ))
    }

    fn ai_mcp_transport_fields(
        &self,
        transport: oxideterm_ai::McpTransport,
        auth_mode_label: String,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if transport == oxideterm_ai::McpTransport::Stdio {
            return vec![
                self.ai_mcp_labeled_input(
                    "settings_view.mcp.command",
                    SettingsInput::AiMcpCommand,
                    "npx".to_string(),
                    cx,
                ),
                self.ai_textarea_row(
                    SettingsInput::AiMcpArgs,
                    self.i18n.t("settings_view.mcp.args"),
                    String::new(),
                    "-y @modelcontextprotocol/server-example".to_string(),
                    String::new(),
                    AI_MCP_ARGS_TEXTAREA_MIN_H,
                    cx,
                ),
                self.ai_mcp_key_value_editor(true, cx),
            ];
        }
        vec![
            self.ai_mcp_labeled_input(
                "settings_view.mcp.url",
                SettingsInput::AiMcpUrl,
                "http://localhost:3000".to_string(),
                cx,
            ),
            div()
                .grid()
                .grid_cols(2)
                .gap(px(12.0))
                .child(self.ai_mcp_labeled_input(
                    "settings_view.mcp.auth_header_name",
                    SettingsInput::AiMcpAuthHeaderName,
                    "Authorization".to_string(),
                    cx,
                ))
                .child(self.ai_mcp_labeled_select(
                    "settings_view.mcp.auth_header_mode",
                    SettingsSelect::AiMcpAuthMode,
                    auth_mode_label,
                    cx,
                ))
                .into_any_element(),
            self.ai_mcp_auth_token_input(cx),
            self.ai_mcp_key_value_editor(false, cx),
            self.ai_mcp_retry_row(cx),
        ]
    }

    fn ai_mcp_text_input_control(
        &self,
        input: SettingsInput,
        placeholder: String,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        (self.controls)(cx)
            .map(|controls| {
                controls.field(
                    input,
                    "",
                    placeholder,
                    None,
                    TextInputContentAlign::Start,
                    secret,
                    cx,
                )
            })
            .unwrap_or_else(|| div().into_any_element())
    }

    fn ai_mcp_labeled_input(
        &self,
        label_key: &str,
        input: SettingsInput,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(AI_MCP_FIELD_GAP))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_selectable_display_text(
                        "ai-mcp-field-label",
                        label_key,
                        self.i18n.t(label_key),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child(self.ai_mcp_text_input_control(input, placeholder, false, cx))
            .into_any_element()
    }

    fn ai_mcp_labeled_select(
        &self,
        label_key: &str,
        select_id: SettingsSelect,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(AI_MCP_FIELD_GAP))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_selectable_display_text(
                        "ai-mcp-select-label",
                        label_key,
                        self.i18n.t(label_key),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .child(
                (self.controls)(cx)
                    .map(|controls| {
                        controls.settings_select_control(select_id, value, false, None, cx)
                    })
                    .unwrap_or_else(|| div().into_any_element()),
            )
            .into_any_element()
    }

    fn ai_mcp_key_value_editor(&self, env: bool, cx: &mut Context<Self>) -> AnyElement {
        let entry_count = self.state.read(cx).ai_mcp.draft().map_or(0, |draft| {
            if env {
                draft.env.len()
            } else {
                draft.headers.len()
            }
        });
        let title = if env {
            self.i18n.t("settings_view.mcp.env_vars")
        } else {
            self.i18n.t("settings_view.mcp.extra_headers")
        };
        let add_label = if env {
            self.i18n.t("settings_view.mcp.add_env_var")
        } else {
            self.i18n.t("settings_view.mcp.add_header")
        };
        let mut rows = div().flex().flex_col().gap(px(AI_MCP_FIELD_GAP));
        for index in 0..entry_count {
            let key_input = if env {
                SettingsInput::AiMcpEnvKey(index)
            } else {
                SettingsInput::AiMcpHeaderKey(index)
            };
            let value_input = if env {
                SettingsInput::AiMcpEnvValue(index)
            } else {
                SettingsInput::AiMcpHeaderValue(index)
            };
            rows = rows.child(
                div()
                    .flex()
                    .gap(px(AI_MCP_FIELD_GAP))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.ai_mcp_text_input_control(
                                key_input,
                                if env {
                                    self.i18n.t("settings_view.mcp.env_key_placeholder")
                                } else {
                                    self.i18n.t("settings_view.mcp.header_key_placeholder")
                                },
                                false,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.ai_mcp_text_input_control(
                                value_input,
                                if env {
                                    self.i18n.t("settings_view.mcp.env_value_placeholder")
                                } else {
                                    self.i18n.t("settings_view.mcp.header_value_placeholder")
                                },
                                false,
                                cx,
                            )),
                    )
                    .child(self.ai_icon_button(
                        LucideIcon::Trash2,
                        false,
                        move |this, _event, _window, cx| {
                            this.state
                                .update(cx, |state, cx| state.remove_ai_mcp_row(env, index, cx));
                            cx.stop_propagation();
                        },
                        cx,
                    )),
            );
        }
        rows = rows.child(self.workspace_toolbar_action_button(
            add_label,
            Some(Self::render_lucide_icon(
                LucideIcon::Plus,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_gap: Some(6.0),
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                this.state.update(cx, |state, cx| {
                    state.update_ai_mcp(
                        |draft| {
                            let (rows, prefix) = if env {
                                (&mut draft.env, "KEY")
                            } else {
                                (&mut draft.headers, "HEADER")
                            };
                            rows.push((format!("{prefix}_{}", rows.len() + 1), String::new()));
                        },
                        cx,
                    )
                });
                cx.stop_propagation();
            }),
        ));
        div()
            .flex()
            .flex_col()
            .gap(px(AI_MCP_FIELD_GAP))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(title),
            )
            .child(rows)
            .when(!env, |section| {
                section.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.mcp.extra_headers_hint")),
                )
            })
            .into_any_element()
    }

    fn ai_mcp_auth_token_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let secret = !self
            .state
            .read(cx)
            .ai_mcp
            .draft()
            .is_some_and(|draft| draft.show_auth_token);
        let input = SettingsInput::AiMcpAuthToken;
        div()
            .flex()
            .flex_col()
            .gap(px(AI_MCP_FIELD_GAP))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.mcp.auth_token")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(AI_MCP_FIELD_GAP))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.ai_mcp_text_input_control(
                                input,
                                self.i18n.t("settings_view.mcp.auth_token_placeholder"),
                                secret,
                                cx,
                            )),
                    )
                    .child(self.ai_icon_button(
                        if secret {
                            LucideIcon::Eye
                        } else {
                            LucideIcon::EyeOff
                        },
                        false,
                        |this, _event, _window, cx| {
                            this.state.update(cx, |state, cx| {
                                state.update_ai_mcp(
                                    |draft| draft.show_auth_token = !draft.show_auth_token,
                                    cx,
                                )
                            });
                            cx.stop_propagation();
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn ai_mcp_retry_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let checked = self
            .state
            .read(cx)
            .ai_mcp
            .draft()
            .is_some_and(|draft| draft.retry_on_disconnect);
        let Some(controls) = (self.controls)(cx) else {
            return div().into_any_element();
        };
        controls.setting_row(
            "settings_view.mcp.retry_on_disconnect",
            "",
            checkbox(&self.tokens, "mcp-retry", String::new(), checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.state.update(cx, |state, cx| {
                            state.update_ai_mcp(
                                |draft| draft.retry_on_disconnect = !draft.retry_on_disconnect,
                                cx,
                            )
                        });
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
            cx,
        )
    }
}

impl Render for AiMcpDialogView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(controls) = (self.controls)(cx) {
            self.tokens = *controls.tokens;
            self.i18n = controls.i18n.clone();
        }
        self.render_ai_mcp_add_server_dialog(cx)
            .unwrap_or_else(|| div().into_any_element())
    }
}
