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

fn ai_mcp_server_signature(
    config: &oxideterm_ai::McpServerConfig,
    snapshot: Option<&oxideterm_ai::McpServerStateSnapshot>,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Do not hash auth_token. The visible card is driven by public config,
    // status, endpoint, error text, and tool names.
    config.id.hash(&mut hasher);
    config.name.hash(&mut hasher);
    format!("{:?}", config.transport).hash(&mut hasher);
    config.url.hash(&mut hasher);
    config.command.hash(&mut hasher);
    config.args.hash(&mut hasher);
    config.env.len().hash(&mut hasher);
    config.auth_header_name.hash(&mut hasher);
    config.auth_header_mode.map(|mode| format!("{mode:?}")).hash(&mut hasher);
    config.headers.len().hash(&mut hasher);
    config.enabled.hash(&mut hasher);
    config.retry_on_disconnect.hash(&mut hasher);
    if let Some(snapshot) = snapshot {
        snapshot.status.hash(&mut hasher);
        snapshot.endpoint_url.hash(&mut hasher);
        snapshot.error.hash(&mut hasher);
        snapshot
            .tools
            .iter()
            .for_each(|tool| tool.name.hash(&mut hasher));
    }
    hasher.finish()
}

impl WorkspaceApp {
    fn ai_mcp_servers_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let configs = ai_mcp_configs(settings);
        let snapshots = self.ai_mcp_registry.snapshots();
        let configured_server_ids: HashSet<_> =
            configs.iter().map(|config| config.id.as_str()).collect();
        // Only live configured MCP rows should drive the retry/status ticker.
        // Stale registry snapshots can otherwise keep the AI settings page
        // repainting even when the MCP section is visually empty.
        if snapshots.iter().any(|snapshot| {
            configured_server_ids.contains(snapshot.config.id.as_str())
                && (snapshot.status == "connecting"
                    || (snapshot.status == "error" && snapshot.config.retry_on_disconnect))
        }) {
            cx.spawn(async move |weak, cx| {
                Timer::after(Duration::from_millis(500)).await;
                let _ = weak.update(cx, |_this, cx| cx.notify());
            })
            .detach();
        }

        let list = if configs.is_empty() {
            div().flex().flex_col().gap(px(12.0)).child(
                div()
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | AI_MCP_PANEL_BORDER_ALPHA))
                    .rounded(px(self.tokens.radii.lg))
                    .py(px(32.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.mcp.no_servers")),
            )
        } else {
            self.sync_ai_mcp_server_list_state(&configs, &snapshots);
            let state = self.ai_mcp_server_list_state.clone();
            let spec = self.ai_mcp_server_list_spec();
            let workspace = cx.entity();
            let configs_for_rows = configs.clone();
            let snapshots_for_rows = snapshots.clone();
            div()
                .h(px(
                    configs.len() as f32 * AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT,
                ))
                .child(tauri_virtual_list(
                    state,
                    spec,
                    move |index, _window, cx| {
                        let Some(config) = configs_for_rows.get(index).cloned() else {
                            return div().into_any_element();
                        };
                        let snapshots = snapshots_for_rows.clone();
                        workspace.update(cx, |this, cx| {
                            let snapshot = snapshots
                                .iter()
                                .find(|snapshot| snapshot.config.id == config.id);
                            div()
                                .pb(px(12.0))
                                .child(this.ai_mcp_server_card(config, snapshot, cx))
                                .into_any_element()
                        })
                    },
                ))
        };

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
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.mcp.add_server"),
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
                            cx.listener(|this, _event, _window, cx| {
                                this.ai_mcp_add_dialog = Some(AiMcpServerDraft::default());
                                this.focused_settings_input = None;
                                this.close_settings_select();
                                this.clear_standard_confirm_focus();
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

    fn sync_ai_mcp_server_list_state(
        &self,
        configs: &[oxideterm_ai::McpServerConfig],
        snapshots: &[oxideterm_ai::McpServerStateSnapshot],
    ) {
        let signatures = configs
            .iter()
            .map(|config| {
                ai_mcp_server_signature(
                    config,
                    snapshots
                        .iter()
                        .find(|snapshot| snapshot.config.id == config.id),
                )
            })
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.ai_mcp_server_list_state,
            &mut self.ai_mcp_server_list_cache.borrow_mut(),
            "ai-mcp-servers",
            &signatures,
            self.ai_mcp_server_list_spec(),
        );
    }

    fn ai_mcp_server_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT),
            AI_MCP_SERVER_LIST_OVERSCAN,
        )
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
            card = card.child(self.ai_mcp_code_line(line, cx));
        } else if !endpoint.is_empty() {
            card = card.child(self.ai_mcp_code_line(endpoint, cx));
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
        self.workspace_icon_action_button(
            icon,
            AI_MCP_ACTION_ICON,
            icon_color,
            IconButtonOptions {
                disabled,
                hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x80)),
                // MCP cards map Tauri disabled icon actions (`opacity-50`);
                // the workspace wrapper now owns the disabled action guard.
                disabled_opacity: 0.5,
                ..IconButtonOptions::opaque_toolbar(AI_MCP_CARD_ICON_BUTTON, ButtonRadius::Md)
            },
            on_click,
            cx,
        )
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
        let mut options = ToolbarButtonOptions::compact_text(
            ButtonVariant::Ghost,
            ButtonRadius::Md,
            AI_MCP_CARD_ACTION_H,
            AI_MCP_CARD_ACTION_PX,
            self.tokens.metrics.ui_text_xs,
        );
        options.button.disabled = connecting;
        options.icon_gap = Some(4.0);
        options.text_color = Some(rgb(self.tokens.ui.text));
        options.hover_background = Some(rgba((self.tokens.ui.bg_hover << 8) | 0x80));
        // Tauri MCP connect/disconnect is a compact shadcn-style card action.
        // Keep loading/disabled behavior in the shared button primitive so the
        // connecting state cannot still submit.
        options.loading = connecting;
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                icon,
                AI_MCP_ACTION_ICON,
                rgb(self.tokens.ui.text),
            )),
            options,
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

    fn ai_mcp_code_line(&self, value: String, cx: &mut Context<Self>) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba((self.tokens.ui.bg_panel << 8) | AI_MCP_CODE_BG_ALPHA))
                    .child(self.render_selectable_display_text(
                        "ai-mcp-code-line",
                        &value,
                        value.clone(),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
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
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        // Tauri McpServersPanel binds Add Server Dialog
                        // onOpenChange to setShowAddDialog(false).
                        this.close_ai_mcp_add_dialog();
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    dialog_content(&self.tokens)
                        .w(px(AI_MCP_DIALOG_WIDTH))
                        .max_w(relative(0.92))
                        .max_h(relative(0.86))
                        .shadow_lg()
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
                                .selectable_overflow_y_scrollbar(
                                    &self.selectable_text_scroll_handle("ai-mcp-add-server-scroll"),
                                )
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
                                    self.standard_footer_action_button(
                                        self.i18n.t("settings_view.mcp.cancel"),
                                        ButtonVariant::Outline,
                                        ConfirmDialogAction::Cancel,
                                        false,
                                        |this, _event, _window, cx| {
                                            this.close_ai_mcp_add_dialog();
                                            cx.notify();
                                        },
                                        cx,
                                    ),
                                )
                                .child(
                                    self.standard_footer_action_button(
                                        self.i18n.t("settings_view.mcp.add"),
                                        ButtonVariant::Default,
                                        ConfirmDialogAction::Confirm,
                                        !can_add,
                                        |this, _event, _window, cx| {
                                            this.add_ai_mcp_server_from_draft(cx);
                                        },
                                        cx,
                                    ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn handle_ai_mcp_add_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(draft) = self.ai_mcp_add_dialog.as_ref() else {
            return false;
        };
        let can_add = ai_mcp_draft_valid(draft, self.settings_store.settings());
        if self.open_settings_select.is_some() || self.focused_settings_input.is_some() {
            return false;
        }

        let key = event.keystroke.key.as_str();
        let footer_focused = self.standard_confirm_focus_owner().is_some();
        if matches!(key, "enter" | "space" | " ") && !footer_focused {
            return false;
        }

        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.close_ai_mcp_add_dialog();
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                if can_add {
                    self.add_ai_mcp_server_from_draft(cx);
                } else {
                    // Disabled primary buttons remain in the dialog; restore
                    // focus to the first footer action like a browser footer loop.
                    self.reset_standard_confirm_focus();
                    cx.notify();
                }
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
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
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w_full()
            .min_w(px(0.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
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
                    .child(self.render_selectable_display_text(
                        "ai-mcp-field-label",
                        label_key,
                        self.i18n.t(label_key),
                        self.tokens.ui.text,
                        cx,
                    )),
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
            .child(self.settings_select_control(select_id, value, false, None, cx))
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
            self.workspace_toolbar_action_button(
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
                            this.close_settings_select();
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
        self.close_settings_select();
        self.clear_standard_confirm_focus();
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
        self.close_settings_select();
        self.clear_standard_confirm_focus();
    }
}

fn settings_input_is_ai_mcp(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::AiMcpName
            | SettingsInput::AiMcpCommand
            | SettingsInput::AiMcpArgs
            | SettingsInput::AiMcpUrl
            | SettingsInput::AiMcpAuthHeaderName
            | SettingsInput::AiMcpAuthToken
            | SettingsInput::AiMcpEnvKey(_)
            | SettingsInput::AiMcpEnvValue(_)
            | SettingsInput::AiMcpHeaderKey(_)
            | SettingsInput::AiMcpHeaderValue(_)
    )
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
