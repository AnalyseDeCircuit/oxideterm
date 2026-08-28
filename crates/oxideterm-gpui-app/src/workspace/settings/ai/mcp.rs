use super::*;

pub(in crate::workspace) const AI_MCP_PANEL_BORDER_ALPHA: u32 = 0x66; // Tauri border-theme-border/40.
pub(in crate::workspace) const AI_MCP_PANEL_BG_ALPHA: u32 = 0x4d; // Tauri bg-theme-bg-panel/30.
pub(in crate::workspace) const AI_MCP_CODE_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg-panel/60.
pub(in crate::workspace) const AI_MCP_TOOL_BORDER_ALPHA: u32 = 0x33; // Tauri border-theme-border/20.
pub(in crate::workspace) const AI_MCP_DIALOG_WIDTH: f32 = 672.0; // Tauri DialogContent sm:max-w-2xl.
pub(in crate::workspace) const AI_MCP_DIALOG_CONTENT_PX: f32 = 16.0; // Tauri px-4.
pub(in crate::workspace) const AI_MCP_DIALOG_CONTENT_PY: f32 = 8.0; // Tauri py-2.
pub(in crate::workspace) const AI_MCP_FORM_GAP: f32 = 16.0; // Tauri space-y-4.
pub(in crate::workspace) const AI_MCP_FIELD_GAP: f32 = 8.0; // Tauri space-y-2 / gap-2.
pub(in crate::workspace) const AI_MCP_CARD_ACTION_H: f32 = 28.0; // Tauri MCP card actions h-7.
pub(in crate::workspace) const AI_MCP_CARD_ACTION_PX: f32 = 8.0; // Tauri px-2.
pub(in crate::workspace) const AI_MCP_CARD_ICON_BUTTON: f32 = 28.0; // Tauri h-7 w-7 p-0.
pub(in crate::workspace) const AI_MCP_ACTION_ICON: f32 = 14.0; // Tauri w-3.5 h-3.5.
pub(in crate::workspace) const AI_MCP_STATUS_ICON: f32 = 12.0; // Tauri status icons w-3 h-3.
pub(in crate::workspace) const AI_MCP_ARGS_TEXTAREA_MIN_H: f32 = 84.0; // Tauri textarea-sized MCP args field.

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_mcp_servers_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let configs = ai_mcp_configs(settings);
        let snapshots = self.ai_entity.read(cx).mcp_registry().snapshots();
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
            self.ai_entity.update(cx, |ai, cx| {
                ai.request_mcp_status_tick(Duration::from_millis(500), cx);
            });
        }

        let list = if configs.is_empty() {
            div().flex().flex_col().gap(px(12.0)).child(
                div()
                    .border_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | AI_MCP_PANEL_BORDER_ALPHA,
                    ))
                    .rounded(px(self.tokens.radii.lg))
                    .py(px(32.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.mcp.no_servers")),
            )
        } else {
            self.sync_ai_mcp_server_list_state(&configs, &snapshots, cx);
            let state = self
                .ai_entity
                .read(cx)
                .model_ui()
                .mcp_server_list_state
                .clone();
            let spec = self.ai_mcp_server_list_spec();
            let workspace = cx.entity();
            let configs_for_rows = configs.clone();
            let snapshots_for_rows = snapshots;
            div()
                .h(px(
                    configs.len() as f32 * AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT
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
                                this.ai_entity.update(cx, |ai, cx| {
                                    ai.open_mcp_add_dialog(cx);
                                });
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

    pub(in crate::workspace) fn sync_ai_mcp_server_list_state(
        &self,
        configs: &[oxideterm_ai::McpServerConfig],
        snapshots: &[oxideterm_ai::McpServerStateSnapshot],
        cx: &App,
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
        let ai = self.ai_entity.read(cx);
        let model_ui = ai.model_ui();
        sync_tauri_variable_list_state_by_signatures(
            &model_ui.mcp_server_list_state,
            &mut model_ui.mcp_server_list_cache.borrow_mut(),
            "ai-mcp-servers",
            &signatures,
            self.ai_mcp_server_list_spec(),
        );
    }

    pub(in crate::workspace) fn ai_mcp_server_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_MCP_SERVER_LIST_ESTIMATED_HEIGHT),
            AI_MCP_SERVER_LIST_OVERSCAN,
        )
    }

    pub(in crate::workspace) fn ai_mcp_server_card(
        &self,
        config: oxideterm_ai::McpServerConfig,
        snapshot: Option<&oxideterm_ai::McpServerStateSnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = snapshot
            .map(|snapshot| snapshot.status)
            .unwrap_or("disconnected");
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
            .border_color(rgba(
                (self.tokens.ui.border << 8) | AI_MCP_PANEL_BORDER_ALPHA,
            ))
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
                                        && config.transport != oxideterm_ai::McpTransport::LegacySse
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
                                            .child(
                                                self.i18n
                                                    .t("settings_view.mcp.fallback_legacy_sse"),
                                            ),
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
                                        this.ai_entity.update(cx, |ai, cx| {
                                            ai.refresh_mcp_tools(refresh_id.clone(), cx);
                                        });
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
                                    this.ai_entity.update(cx, |ai, cx| {
                                        ai.remove_mcp_server(remove_id.clone(), cx);
                                    });
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
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | AI_MCP_TOOL_BORDER_ALPHA,
                    ))
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

    pub(in crate::workspace) fn ai_mcp_card_icon_button(
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

    pub(in crate::workspace) fn ai_mcp_toggle_button(
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
        let icon = if connecting {
            self.render_loading_icon(
                (
                    gpui::SharedString::from(format!("mcp-connect-spinner-{}", config.id)),
                    0usize,
                ),
                AI_MCP_ACTION_ICON,
                rgb(self.tokens.ui.text),
            )
        } else {
            Self::render_lucide_icon(icon, AI_MCP_ACTION_ICON, rgb(self.tokens.ui.text))
        };
        self.workspace_toolbar_action_button(
            label,
            Some(icon),
            options,
            cx.listener(move |this, _event, _window, cx| {
                this.ai_entity.update(cx, |ai, cx| {
                    ai.set_mcp_server_connected(config.clone(), connected, cx);
                });
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_mcp_status_badge(&self, status: &str) -> AnyElement {
        let (label_key, color) = match status {
            "connected" => ("settings_view.mcp.status_connected", self.tokens.ui.success),
            "connecting" => (
                "settings_view.mcp.status_connecting",
                self.tokens.ui.warning,
            ),
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
                badge.child(self.render_loading_icon(
                    "mcp-status-connecting",
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

    pub(in crate::workspace) fn ai_mcp_transport_badge(
        &self,
        transport: oxideterm_ai::McpTransport,
    ) -> AnyElement {
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

    pub(in crate::workspace) fn ai_mcp_code_line(
        &self,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

    pub(in crate::workspace) fn render_ai_mcp_add_server_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (can_add, transport, auth_header_mode, dialog_presence) = {
            let ai_workspace = self.ai_entity.read(cx);
            if !ai_workspace.mcp_dialog_is_open() {
                return None;
            }
            (
                ai_workspace.mcp_draft_is_valid(self.settings_store.settings()),
                ai_workspace
                    .mcp_transport()
                    .unwrap_or(oxideterm_ai::McpTransport::Stdio),
                ai_workspace
                    .mcp_auth_mode()
                    .unwrap_or(oxideterm_ai::McpAuthHeaderMode::Bearer),
                ai_workspace.mcp_dialog_presence(),
            )
        };
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
            cx.listener(|this, _event, _window, cx| {
                // Tauri McpServersPanel binds Add Server Dialog
                // onOpenChange to setShowAddDialog(false).
                this.close_ai_mcp_add_dialog(cx);
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let form = dialog_content(&self.tokens)
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
                    .children(self.ai_mcp_transport_fields(transport, auth_mode_label, cx)),
            )
            .child(
                dialog_footer(&self.tokens)
                    .child(self.standard_footer_action_button(
                        self.i18n.t("settings_view.mcp.cancel"),
                        ButtonVariant::Outline,
                        ConfirmDialogAction::Cancel,
                        false,
                        |this, _event, _window, cx| {
                            this.close_ai_mcp_add_dialog(cx);
                        },
                        cx,
                    ))
                    .child(self.standard_footer_action_button(
                        self.i18n.t("settings_view.mcp.add"),
                        ButtonVariant::Default,
                        ConfirmDialogAction::Confirm,
                        !can_add,
                        |this, _event, _window, cx| {
                            this.submit_ai_mcp_add_dialog(cx);
                        },
                        cx,
                    )),
            );
        Some(settings_dialog_transition(
            &self.tokens,
            "ai-mcp-dialog-form",
            backdrop,
            form,
            dialog_presence.phase(),
        ))
    }

    pub(in crate::workspace) fn handle_ai_mcp_add_dialog_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let can_add = {
            let ai_workspace = self.ai_entity.read(cx);
            if !ai_workspace.mcp_dialog_is_open() {
                return false;
            }
            ai_workspace.mcp_draft_is_valid(self.settings_store.settings())
        };
        if self.open_settings_select.is_some()
            || self.focused_settings_input.is_some()
            || self.ai_entity.read(cx).focused_settings_input().is_some()
        {
            return false;
        }

        let key = event.keystroke.key.as_str();
        let footer_focused = self.standard_confirm_focus_owner().is_some();
        if matches!(key, "enter" | "space" | " ") && !footer_focused {
            return false;
        }

        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.close_ai_mcp_add_dialog(cx);
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                if can_add {
                    self.submit_ai_mcp_add_dialog(cx);
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

    pub(in crate::workspace) fn ai_mcp_transport_fields(
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

    pub(in crate::workspace) fn ai_mcp_text_input_control(
        &self,
        input: SettingsInput,
        placeholder: String,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = WorkspaceImeTarget::Settings(input);
        let input_control = {
            let ai_workspace = self.ai_entity.read(cx);
            let focused = ai_workspace.focused_settings_input() == Some(input);
            text_input(
                &self.tokens,
                TextInputView {
                    value: ai_workspace.settings_input_value(input).unwrap_or_default(),
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
        };
        self.text_input_with_workspace_ime(
            target,
            input_control.w_full().min_w(px(0.0)),
            move |this, cx| {
                this.focus_settings_input(input, String::new(), cx);
            },
            cx,
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn ai_mcp_labeled_input(
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

    pub(in crate::workspace) fn ai_mcp_labeled_select(
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

    pub(in crate::workspace) fn ai_mcp_key_value_editor(
        &self,
        env: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry_count = self.ai_entity.read(cx).mcp_record_len(env);
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
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.remove_mcp_record_entry(env, index, cx);
                            });
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
                this.ai_entity.update(cx, |ai, cx| {
                    ai.add_mcp_record_entry(env, cx);
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
                        .text_size(px(11.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.mcp.extra_headers_hint")),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_mcp_auth_token_input(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let secret = !self.ai_entity.read(cx).mcp_auth_token_visible();
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
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.toggle_mcp_auth_token_visibility(cx);
                            });
                            cx.stop_propagation();
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_mcp_retry_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let checked = self.ai_entity.read(cx).mcp_retry_enabled();
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
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.toggle_mcp_retry(cx);
                            });
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element(),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn close_ai_mcp_add_dialog(&mut self, cx: &mut Context<Self>) {
        self.close_settings_select();
        self.clear_standard_confirm_focus();
        self.ime_marked_text = None;
        self.clear_ime_selection();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.ai_entity.update(cx, |ai, cx| {
            ai.begin_mcp_dialog_exit(false, delay, HashSet::new(), cx);
        });
        cx.notify();
    }

    pub(in crate::workspace) fn submit_ai_mcp_add_dialog(&mut self, cx: &mut Context<Self>) {
        self.close_settings_select();
        self.clear_standard_confirm_focus();
        self.ime_marked_text = None;
        self.clear_ime_selection();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        let configured_names = ai_mcp_configs(self.settings_store.settings())
            .into_iter()
            .map(|config| config.name)
            .collect();
        self.ai_entity.update(cx, |ai, cx| {
            ai.begin_mcp_dialog_exit(true, delay, configured_names, cx);
        });
        cx.notify();
    }
}
