const AI_MCP_PANEL_BORDER_ALPHA: u32 = 0x66; // Tauri border-theme-border/40.
const AI_MCP_PANEL_BG_ALPHA: u32 = 0x4d; // Tauri bg-theme-bg-panel/30.
const AI_MCP_CODE_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg-panel/60.
const AI_MCP_TOOL_BORDER_ALPHA: u32 = 0x33; // Tauri border-theme-border/20.
const AI_MCP_DIALOG_WIDTH: f32 = 672.0; // Tauri DialogContent sm:max-w-2xl.
const AI_MCP_DIALOG_CONTENT_PX: f32 = 16.0; // Tauri px-4.
const AI_MCP_DIALOG_CONTENT_PY: f32 = 8.0; // Tauri py-2.
const AI_MCP_FORM_GAP: f32 = 16.0; // Tauri space-y-4.
const AI_MCP_FIELD_GAP: f32 = 8.0; // Tauri space-y-2 / gap-2.
const AI_MCP_CARD_ACTION_H: f32 = 28.0; // Tauri MCP card actions h-7.
const AI_MCP_CARD_ACTION_PX: f32 = 8.0; // Tauri px-2.
const AI_MCP_CARD_ICON_BUTTON: f32 = 28.0; // Tauri h-7 w-7 p-0.
const AI_MCP_ACTION_ICON: f32 = 14.0; // Tauri w-3.5 h-3.5.
const AI_MCP_STATUS_ICON: f32 = 12.0; // Tauri status icons w-3 h-3.

impl WorkspaceApp {
    fn ai_mcp_servers_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let configs = ai_mcp_configs(settings);
        let snapshots = self.ai_mcp_registry.snapshots();
        if snapshots.iter().any(|snapshot| {
            snapshot.status == "connecting"
                || (snapshot.status == "error" && snapshot.config.retry_on_disconnect)
        }) {
            cx.spawn(async move |weak, cx| {
                Timer::after(Duration::from_millis(500)).await;
                let _ = weak.update(cx, |_this, cx| cx.notify());
            })
            .detach();
        }

        let mut list = div().flex().flex_col().gap(px(12.0));
        if configs.is_empty() {
            list = list.child(
                div()
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | AI_MCP_PANEL_BORDER_ALPHA))
                    .rounded(px(self.tokens.radii.lg))
                    .py(px(32.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.mcp.no_servers")),
            );
        } else {
            for config in configs {
                let snapshot = snapshots
                    .iter()
                    .find(|snapshot| snapshot.config.id == config.id);
                list = list.child(self.ai_mcp_server_card(config, snapshot, cx));
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(separator(&self.tokens, SeparatorOrientation::Horizontal))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.ai_section_heading(
                        "settings_view.mcp.title",
                        "settings_view.mcp.description",
                    ))
                    .child(
                        button_with(
                            &self.tokens,
                            format!("+ {}", self.i18n.t("settings_view.mcp.add_server")),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.ai_mcp_add_dialog = Some(AiMcpServerDraft::default());
                                this.focused_settings_input = None;
                                this.open_settings_select = None;
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .into_any_element(),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    fn ai_mcp_server_card(
        &self,
        config: oxideterm_ai::McpServerConfig,
        snapshot: Option<&oxideterm_ai::McpServerStateSnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = snapshot.map(|snapshot| snapshot.status).unwrap_or("disconnected");
        let tools = snapshot
            .map(|snapshot| snapshot.tools.as_slice())
            .unwrap_or_default();
        let endpoint = snapshot
            .and_then(|snapshot| snapshot.endpoint_url.as_deref())
            .or(config.url.as_deref())
            .unwrap_or_default()
            .to_string();
        let command = if config.transport == oxideterm_ai::McpTransport::Stdio {
            Some(
                std::iter::once(config.command.clone().unwrap_or_default())
                    .chain(config.args.clone())
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        } else {
            None
        };
        let config_for_toggle = config.clone();
        let remove_id = config.id.clone();
        let refresh_id = config.id.clone();

        let mut card = div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | AI_MCP_PANEL_BORDER_ALPHA))
            .bg(rgba((self.tokens.ui.bg_panel << 8) | AI_MCP_PANEL_BG_ALPHA))
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(config.name.clone()),
                            )
                            .child(self.ai_mcp_status_badge(status))
                            .child(self.ai_mcp_transport_badge(config.transport))
                            .when(
                                snapshot.is_some_and(|snapshot| {
                                    snapshot.resolved_transport.as_deref() == Some("legacy-sse")
                                        && config.transport
                                            != oxideterm_ai::McpTransport::LegacySse
                                }),
                                |row| {
                                    row.child(
                                        div()
                                            .px(px(6.0))
                                            .py(px(2.0))
                                            .rounded(px(self.tokens.radii.sm))
                                            .bg(rgba((self.tokens.ui.warning << 8) | 0x1a))
                                            .text_size(px(10.0))
                                            .text_color(rgb(self.tokens.ui.warning))
                                            .child(self.i18n.t("settings_view.mcp.fallback_legacy_sse")),
                                    )
                                },
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(status == "connected", |row| {
                                row.child(self.ai_mcp_card_icon_button(
                                    LucideIcon::RefreshCw,
                                    rgb(self.tokens.ui.text_muted),
                                    false,
                                    move |this, _event, _window, cx| {
                                        let registry = this.ai_mcp_registry.clone();
                                        let server_id = refresh_id.clone();
                                        cx.spawn(async move |weak, cx| {
                                            let _ = registry.refresh_tools(&server_id).await;
                                            let _ = weak.update(cx, |_this, cx| cx.notify());
                                        })
                                        .detach();
                                        cx.stop_propagation();
                                    },
                                    cx,
                                ))
                            })
                            .child(self.ai_mcp_toggle_button(status, config_for_toggle, cx))
                            .child(self.ai_mcp_card_icon_button(
                                LucideIcon::Trash2,
                                rgb(self.tokens.ui.error),
                                false,
                                move |this, _event, _window, cx| {
                                    let registry = this.ai_mcp_registry.clone();
                                    let runtime = this.forwarding_runtime.clone();
                                    let server_id = remove_id.clone();
                                    cx.spawn(async move |weak, cx| {
                                        registry.disconnect_server(&server_id).await;
                                        let delete_registry = registry.clone();
                                        let server_id_for_delete = server_id.clone();
                                        let _ = runtime
                                            .spawn_blocking(move || {
                                                delete_registry.delete_auth_token(&server_id_for_delete)
                                            })
                                            .await;
                                        let _ = weak.update(cx, |this, cx| {
                                            this.edit_settings(
                                                |settings| {
                                                    settings.ai.mcp_servers.retain(|value| {
                                                        value
                                                            .get("id")
                                                            .and_then(serde_json::Value::as_str)
                                                            != Some(server_id.as_str())
                                                    });
                                                },
                                                cx,
                                            );
                                        });
                                    })
                                    .detach();
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    ),
            );

        if let Some(line) = command.filter(|line| !line.is_empty()) {
            card = card.child(self.ai_mcp_code_line(line));
        } else if !endpoint.is_empty() {
            card = card.child(self.ai_mcp_code_line(endpoint));
        }
        if let Some(error) = snapshot.and_then(|snapshot| snapshot.error.as_ref()) {
            card = card.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.error))
                    .child(error.clone()),
            );
        }
        if !tools.is_empty() {
            let mut chips = div().flex().flex_wrap().gap(px(4.0));
            for tool in tools {
                chips = chips.child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgba((self.tokens.ui.bg_panel << 8) | AI_MCP_CODE_BG_ALPHA))
                        .text_size(px(10.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(tool.name.clone()),
                );
            }
            card = card.child(
                div()
                    .mt(px(4.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | AI_MCP_TOOL_BORDER_ALPHA))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(10.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Wrench,
                                12.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .child(
                                self.i18n
                                    .t("settings_view.mcp.tools_count")
                                    .replace("{{count}}", &tools.len().to_string()),
                            ),
                    )
                    .child(chips),
            );
        }
        card.into_any_element()
    }

    fn ai_mcp_card_icon_button(
        &self,
        icon: LucideIcon,
        icon_color: Rgba,
        disabled: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w(px(AI_MCP_CARD_ICON_BUTTON))
            .h(px(AI_MCP_CARD_ICON_BUTTON))
            .rounded(px(self.tokens.radii.md))
            .flex()
            .items_center()
            .justify_center()
            .opacity(if disabled { 0.5 } else { 1.0 })
            .when(!disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba((self.tokens.ui.bg_hover << 8) | 0x80)))
                    .on_mouse_down(MouseButton::Left, cx.listener(on_click))
            })
            .child(Self::render_lucide_icon(
                icon,
                AI_MCP_ACTION_ICON,
                icon_color,
            ))
            .into_any_element()
    }

    fn ai_mcp_toggle_button(
        &self,
        status: &str,
        config: oxideterm_ai::McpServerConfig,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connected = status == "connected";
        let connecting = status == "connecting";
        let label = if connected {
            self.i18n.t("settings_view.mcp.disconnect")
        } else if connecting {
            self.i18n.t("settings_view.mcp.connecting")
        } else {
            self.i18n.t("settings_view.mcp.connect")
        };
        let icon = if connected {
            LucideIcon::StopCircle
        } else if connecting {
            LucideIcon::LoaderCircle
        } else {
            LucideIcon::Radio
        };
        div()
            .h(px(AI_MCP_CARD_ACTION_H))
            .px(px(AI_MCP_CARD_ACTION_PX))
            .rounded(px(self.tokens.radii.md))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .opacity(if connecting { 0.5 } else { 1.0 })
            .when(!connecting, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba((self.tokens.ui.bg_hover << 8) | 0x80)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            let registry = this.ai_mcp_registry.clone();
                            let config = config.clone();
                            cx.spawn(async move |weak, cx| {
                                if connected {
                                    registry.disconnect_server(&config.id).await;
                                } else {
                                    registry.connect_config(config).await;
                                }
                                let _ = weak.update(cx, |_this, cx| cx.notify());
                            })
                            .detach();
                            cx.stop_propagation();
                        }),
                    )
            })
            .child(Self::render_lucide_icon(
                icon,
                AI_MCP_ACTION_ICON,
                rgb(self.tokens.ui.text),
            ))
            .child(label)
            .into_any_element()
    }

    fn ai_mcp_status_badge(&self, status: &str) -> AnyElement {
        let (label_key, color) = match status {
            "connected" => ("settings_view.mcp.status_connected", self.tokens.ui.success),
            "connecting" => ("settings_view.mcp.status_connecting", self.tokens.ui.warning),
            "error" => ("settings_view.mcp.status_error", self.tokens.ui.error),
            _ => (
                "settings_view.mcp.status_disconnected",
                self.tokens.ui.text_muted,
            ),
        };
        div()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((color << 8) | 0x33))
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(color))
            .when(status == "connecting", |badge| {
                badge.child(Self::render_lucide_icon(
                    LucideIcon::LoaderCircle,
                    AI_MCP_STATUS_ICON,
                    rgb(color),
                ))
            })
            .when(status == "connected", |badge| {
                badge.child(Self::render_lucide_icon(
                    LucideIcon::Check,
                    AI_MCP_STATUS_ICON,
                    rgb(color),
                ))
            })
            .child(self.i18n.t(label_key))
            .into_any_element()
    }

    fn ai_mcp_transport_badge(&self, transport: oxideterm_ai::McpTransport) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg_panel))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(ai_mcp_transport_label(transport).to_uppercase())
            .into_any_element()
    }

    fn ai_mcp_code_line(&self, value: String) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba((self.tokens.ui.bg_panel << 8) | AI_MCP_CODE_BG_ALPHA))
                    .child(value),
            )
            .into_any_element()
    }

    fn render_ai_mcp_add_server_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let draft = self.ai_mcp_add_dialog.as_ref()?;
        let can_add = ai_mcp_draft_valid(draft, self.settings_store.settings());
        let transport_label = ai_mcp_transport_label(draft.transport);
        let auth_mode_label = match draft.auth_header_mode {
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

        Some(
            dialog_backdrop()
                .child(
                    dialog_content(&self.tokens)
                        .w(px(AI_MCP_DIALOG_WIDTH))
                        .max_w(relative(0.92))
                        .max_h(relative(0.86))
                        .shadow_lg()
                        .flex()
                        .flex_col()
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
                                .flex_1()
                                .min_h(px(0.0))
                                .overflow_y_scrollbar()
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
                                .children(self.ai_mcp_transport_fields(draft, auth_mode_label, cx)),
                        )
                        .child(
                            dialog_footer(&self.tokens)
                                .child(
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("settings_view.mcp.cancel"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Outline,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: false,
                                        },
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.close_ai_mcp_add_dialog();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                                )
                                .child(
                                    button_with(
                                        &self.tokens,
                                        self.i18n.t("settings_view.mcp.add"),
                                        ButtonOptions {
                                            variant: ButtonVariant::Default,
                                            size: ButtonSize::Sm,
                                            radius: ButtonRadius::Md,
                                            disabled: !can_add,
                                        },
                                    )
                                    .when(can_add, |button| {
                                        button.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.add_ai_mcp_server_from_draft(cx);
                                                cx.stop_propagation();
                                            }),
                                        )
                                    }),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn ai_mcp_transport_fields(
        &self,
        draft: &AiMcpServerDraft,
        auth_mode_label: String,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if draft.transport == oxideterm_ai::McpTransport::Stdio {
            return vec![
                self.ai_mcp_labeled_input(
                    "settings_view.mcp.command",
                    SettingsInput::AiMcpCommand,
                    "npx -y @modelcontextprotocol/server-example".to_string(),
                    cx,
                ),
                self.ai_mcp_labeled_input(
                    "settings_view.mcp.args",
                    SettingsInput::AiMcpArgs,
                    "--flag value".to_string(),
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
        value: String,
        placeholder: String,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret,
                    selected_all: false,
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w_full()
            .min_w(px(0.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
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
                    .child(self.i18n.t(label_key)),
            )
            .child(self.ai_mcp_text_input_control(
                input,
                self.current_settings_input_value(input),
                placeholder,
                false,
                cx,
            ))
            .into_any_element()
    }

    fn ai_mcp_labeled_select(
        &self,
        label_key: &str,
        select_id: SettingsSelect,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity();
        div()
            .flex()
            .flex_col()
            .gap(px(AI_MCP_FIELD_GAP))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(select_anchor_probe(
                select_id.anchor_id(),
                select_trigger(&self.tokens, value, false, false)
                    .w_full()
                    .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.focused_settings_input = None;
                        this.open_settings_select = if this.open_settings_select == Some(select_id) {
                            None
                        } else {
                            Some(select_id)
                        };
                        cx.stop_propagation();
                        cx.notify();
                    }),
                    ),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, _cx| {
                        this.select_anchors.insert(select_id.anchor_id(), anchor);
                    });
                },
            ))
            .into_any_element()
    }

    fn ai_mcp_key_value_editor(&self, env: bool, cx: &mut Context<Self>) -> AnyElement {
        let draft = self.ai_mcp_add_dialog.as_ref();
        let entries = if env {
            draft.map(|draft| draft.env.as_slice()).unwrap_or_default()
        } else {
            draft.map(|draft| draft.headers.as_slice()).unwrap_or_default()
        };
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
        for (index, _) in entries.iter().enumerate() {
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
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.ai_mcp_text_input_control(
                            key_input,
                            self.current_settings_input_value(key_input),
                            if env {
                                self.i18n.t("settings_view.mcp.env_key_placeholder")
                            } else {
                                self.i18n.t("settings_view.mcp.header_key_placeholder")
                            },
                            false,
                            cx,
                        ),
                    ))
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.ai_mcp_text_input_control(
                            value_input,
                            self.current_settings_input_value(value_input),
                            if env {
                                self.i18n.t("settings_view.mcp.env_value_placeholder")
                            } else {
                                self.i18n.t("settings_view.mcp.header_value_placeholder")
                            },
                            false,
                            cx,
                        ),
                    ))
                    .child(self.ai_icon_button(
                        LucideIcon::Trash2,
                        false,
                        move |this, _event, _window, cx| {
                            if let Some(draft) = this.ai_mcp_add_dialog.as_mut() {
                                if env {
                                    if index < draft.env.len() {
                                        draft.env.remove(index);
                                    }
                                } else if index < draft.headers.len() {
                                    draft.headers.remove(index);
                                }
                            }
                            cx.stop_propagation();
                            cx.notify();
                        },
                        cx,
                    )),
            );
        }
        rows = rows.child(
            button_with(
                &self.tokens,
                format!("+ {add_label}"),
                ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(draft) = this.ai_mcp_add_dialog.as_mut() {
                        if env {
                            draft.env.push((format!("KEY_{}", draft.env.len() + 1), String::new()));
                        } else {
                            draft
                                .headers
                                .push((format!("HEADER_{}", draft.headers.len() + 1), String::new()));
                        }
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            ),
        );
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
                        .text_size(px(11.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.mcp.extra_headers_hint")),
                )
            })
            .into_any_element()
    }

    fn ai_mcp_auth_token_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let secret = !self
            .ai_mcp_add_dialog
            .as_ref()
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
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.ai_mcp_text_input_control(
                            input,
                            self.current_settings_input_value(input),
                            self.i18n.t("settings_view.mcp.auth_token_placeholder"),
                            secret,
                            cx,
                        ),
                    ))
                    .child(self.ai_icon_button(
                        if secret { LucideIcon::Eye } else { LucideIcon::EyeOff },
                        false,
                        |this, _event, _window, cx| {
                            if let Some(draft) = this.ai_mcp_add_dialog.as_mut() {
                                draft.show_auth_token = !draft.show_auth_token;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn ai_mcp_retry_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let checked = self
            .ai_mcp_add_dialog
            .as_ref()
            .is_some_and(|draft| draft.retry_on_disconnect);
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.mcp.retry_on_disconnect")),
            )
            .child(
                checkbox(&self.tokens, String::new(), checked)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Some(draft) = this.ai_mcp_add_dialog.as_mut() {
                                draft.retry_on_disconnect = !checked;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
            )
            .into_any_element()
    }

    fn add_ai_mcp_server_from_draft(&mut self, cx: &mut Context<Self>) {
        let Some(mut draft) = self.ai_mcp_add_dialog.take() else {
            return;
        };
        if !ai_mcp_draft_valid(&draft, self.settings_store.settings()) {
            self.ai_mcp_add_dialog = Some(draft);
            cx.notify();
            return;
        }
        let id = format!("mcp-{}", uuid::Uuid::new_v4());
        let transport = draft.transport;
        let mut object = serde_json::Map::new();
        object.insert("id".to_string(), serde_json::json!(id));
        object.insert("name".to_string(), serde_json::json!(draft.name.trim()));
        object.insert(
            "transport".to_string(),
            serde_json::json!(ai_mcp_transport_value(transport)),
        );
        if !draft.url.trim().is_empty() {
            object.insert("url".to_string(), serde_json::json!(draft.url.trim()));
        }
        if !draft.command.trim().is_empty() {
            object.insert(
                "command".to_string(),
                serde_json::json!(draft.command.trim()),
            );
        }
        let args = ai_mcp_split_args(&draft.args);
        if !args.is_empty() {
            object.insert("args".to_string(), serde_json::json!(args));
        }
        if let Some(env) = ai_mcp_clean_record(&draft.env) {
            object.insert("env".to_string(), env);
        }
        let auth_header_name = draft.auth_header_name.trim();
        if !auth_header_name.is_empty() && auth_header_name != "Authorization" {
            object.insert(
                "authHeaderName".to_string(),
                serde_json::json!(auth_header_name),
            );
        }
        if draft.auth_header_mode != oxideterm_ai::McpAuthHeaderMode::Bearer {
            object.insert(
                "authHeaderMode".to_string(),
                serde_json::json!(ai_mcp_auth_mode_value(draft.auth_header_mode)),
            );
        }
        if let Some(headers) = ai_mcp_clean_record(&draft.headers) {
            object.insert("headers".to_string(), headers);
        }
        object.insert("enabled".to_string(), serde_json::json!(true));
        if draft.retry_on_disconnect {
            object.insert(
                "retryOnDisconnect".to_string(),
                serde_json::json!(draft.retry_on_disconnect),
            );
        }
        let config = serde_json::Value::Object(object);
        let should_store_auth_token = !draft.auth_token.is_empty()
            && draft.auth_header_mode != oxideterm_ai::McpAuthHeaderMode::None;
        if should_store_auth_token {
            let registry = self.ai_mcp_registry.clone();
            let runtime = self.forwarding_runtime.clone();
            let token = draft.auth_token.clone();
            let mut restore_draft = draft.clone();
            cx.spawn(async move |weak, cx| {
                let id_for_store = id.clone();
                let result = runtime
                    .spawn_blocking(move || {
                        registry.store_auth_token(&id_for_store, zeroize::Zeroizing::new(token))
                    })
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result.map_err(|error| error.to_string()));
                let _ = weak.update(cx, |this, cx| {
                    match result {
                        Ok(()) => {
                            zeroize::Zeroize::zeroize(&mut restore_draft.auth_token);
                            this.focused_settings_input = None;
                            this.settings_input_draft.clear();
                            this.open_settings_select = None;
                            this.edit_settings(
                                move |settings| {
                                    settings.ai.mcp_servers.push(config.clone());
                                },
                                cx,
                            );
                        }
                        Err(error) => {
                            this.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                            this.ai_mcp_add_dialog = Some(restore_draft);
                            cx.notify();
                        }
                    }
                });
            })
            .detach();
            cx.notify();
            return;
        }

        zeroize::Zeroize::zeroize(&mut draft.auth_token);
        self.focused_settings_input = None;
        self.settings_input_draft.clear();
        self.open_settings_select = None;
        self.edit_settings(
            move |settings| {
                settings.ai.mcp_servers.push(config.clone());
            },
            cx,
        );
    }

    fn close_ai_mcp_add_dialog(&mut self) {
        if let Some(mut draft) = self.ai_mcp_add_dialog.take() {
            zeroize::Zeroize::zeroize(&mut draft.auth_token);
        }
        self.focused_settings_input = None;
        self.settings_input_draft.clear();
        self.open_settings_select = None;
    }
}

fn ai_mcp_configs(settings: &PersistedSettings) -> Vec<oxideterm_ai::McpServerConfig> {
    settings
        .ai
        .mcp_servers
        .iter()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

fn ai_mcp_draft_valid(draft: &AiMcpServerDraft, settings: &PersistedSettings) -> bool {
    let name = draft.name.trim();
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        && !ai_mcp_configs(settings)
            .iter()
            .any(|server| server.name == name)
}

fn ai_mcp_transport_label(transport: oxideterm_ai::McpTransport) -> String {
    match transport {
        oxideterm_ai::McpTransport::Stdio => "stdio",
        oxideterm_ai::McpTransport::StreamableHttp | oxideterm_ai::McpTransport::Sse => {
            "Streamable HTTP"
        }
        oxideterm_ai::McpTransport::LegacySse => "Legacy SSE",
    }
    .to_string()
}

fn ai_mcp_transport_value(transport: oxideterm_ai::McpTransport) -> &'static str {
    match transport {
        oxideterm_ai::McpTransport::Stdio => "stdio",
        oxideterm_ai::McpTransport::StreamableHttp | oxideterm_ai::McpTransport::Sse => {
            "streamable-http"
        }
        oxideterm_ai::McpTransport::LegacySse => "legacy-sse",
    }
}

fn ai_mcp_auth_mode_value(mode: oxideterm_ai::McpAuthHeaderMode) -> &'static str {
    match mode {
        oxideterm_ai::McpAuthHeaderMode::Bearer => "bearer",
        oxideterm_ai::McpAuthHeaderMode::Raw => "raw",
        oxideterm_ai::McpAuthHeaderMode::None => "none",
    }
}

fn ai_mcp_clean_record(entries: &[(String, String)]) -> Option<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (key, value) in entries {
        let key = key.trim();
        if !key.is_empty() {
            map.insert(key.to_string(), serde_json::json!(value));
        }
    }
    (!map.is_empty()).then(|| serde_json::Value::Object(map))
}

fn ai_mcp_split_args(args: &str) -> Vec<String> {
    args.split_whitespace().map(str::to_string).collect()
}
