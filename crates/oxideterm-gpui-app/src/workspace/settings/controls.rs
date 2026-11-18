use oxideterm_gpui_settings_view::select_menu::{LocalShellOption, SelectMenuRequest, SettingsSelectMenu};
use super::*;
use oxideterm_gpui_settings_view::components::{SettingsWindowDispatch, SettingsWindowRequest};
use oxideterm_gpui_settings_view::theme_picker::{SettingsThemePicker, SettingsThemePickerFrame};

const KNOWLEDGE_SCOPE_SELECT_MAX_HEIGHT: f32 = 320.0;

impl WorkspaceApp {
    pub(in crate::workspace) fn mount_settings_theme_picker(cx: &mut Context<Self>) -> Entity<SettingsThemePicker> {
        let owner = cx.entity().downgrade();
        let frame = Rc::new({
            let owner = owner.clone();
            move |window: &Window, cx: &App| {
                let owner = owner.upgrade()?;
                let app = owner.read(cx);
                let select = app.open_settings_select?;
                select.theme_target()?;
                let window_id = window.window_handle().window_id();
                if app.open_settings_select_owner_window_id != Some(window_id) {
                    return None;
                }
                let anchor = app.settings_select_anchors.get(&(window_id, select.anchor_id()))?;
                Some(SettingsThemePickerFrame {
                    tokens: app.tokens,
                    i18n: app.i18n.clone(),
                    width: f32::from(anchor.bounds.size.width).max(app.tokens.metrics.ui_select_min_width),
                    select,
                })
            }
        });
        let apply = Rc::new(move |select: SettingsSelect, id, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| {
                if app.open_settings_select == Some(select) && let Some(target) = select.theme_target() {
                    app.close_settings_select();
                    app.edit_settings(|settings| target.apply_for_scheme(settings, select.theme_system_dark(), id), cx);
                }
            });
        });
        cx.new(|_| SettingsThemePicker::new(frame, apply))
    }

    pub(super) fn settings_window_dispatch(cx: &Context<Self>) -> SettingsWindowDispatch {
        Self::settings_window_dispatch_for(cx.entity().downgrade())
    }

    pub(super) fn settings_window_dispatch_for(owner: gpui::WeakEntity<Self>) -> SettingsWindowDispatch {
        Rc::new(move |request, window, cx| {
            let _ = owner.update(cx, |app, cx| match request {
                SettingsWindowRequest::Edit(edit) => app.edit_settings(edit, cx),
                SettingsWindowRequest::OpenSelect(select) => {
                    app.open_settings_select_from_pointer(select, window.window_handle().window_id(), cx);
                    cx.notify();
                }
                SettingsWindowRequest::SelectAnchor(anchor) => {
                    app.update_settings_select_anchor(window.window_handle().window_id(), anchor, cx);
                }
                SettingsWindowRequest::BeginInputSelection(input, event) => {
                    let current = app.current_settings_input_value(input, cx);
                    app.focus_settings_input(input, current, cx);
                    app.ime_marked_text = None;
                    window.focus(&app.focus_handle, cx);
                    app.begin_ime_selection_from_mouse_down(WorkspaceImeTarget::Settings(input), &event, window, cx);
                }
                SettingsWindowRequest::DragInputSelection(event) => {
                    app.update_ime_selection_drag_from_mouse_move(&event, window, cx);
                }
                SettingsWindowRequest::SliderAnchor(anchor) => app.update_select_anchor(anchor, cx),
                SettingsWindowRequest::BeginSlider(slider, position) => {
                    app.close_settings_select();
                    app.focused_settings_input = None;
                    app.settings_slider_drag = Some(slider);
                    app.apply_settings_slider_from_position(slider, position, cx);
                }
                SettingsWindowRequest::MoveSlider(event) => app.update_settings_slider_drag(&event, cx),
                SettingsWindowRequest::FinishSlider => app.finish_settings_slider_drag(cx),
                SettingsWindowRequest::PickSessionLogDirectory => app.pick_terminal_session_log_directory(cx),
            });
        })
    }

    pub(in crate::workspace) fn render_settings_select_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Select content follows the logical open state directly. Dropdowns
        // never retain a second render-only state for exit animation.
        let open_select = self.open_settings_select?;
        let window_id = window.window_handle().window_id();
        if self.open_settings_select_owner_window_id != Some(window_id) {
            return None;
        }
        let anchor = self
            .settings_select_anchors
            .get(&(window_id, open_select.anchor_id()))
            .copied()?;
        let width =
            f32::from(anchor.bounds.size.width).max(self.tokens.metrics.ui_select_min_width);
        let popup = if open_select.theme_target().is_some() {
            self.settings_theme_picker.clone().into_any_element()
        } else {
            self.render_settings_select_popup(open_select, window_id, width, cx)?
        };

        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event, window, cx| {
                        this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(Corner::TopLeft)
                            .position(anchor.bounds.bottom_left())
                            .offset(point(
                                px(0.0),
                                px(self.tokens.metrics.settings_select_popup_gap),
                            ))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                                &self.tokens,
                                SharedString::from(format!(
                                    "settings-select-popup-{open_select:?}"
                                )),
                                div().child(popup),
                                -6.0,
                                point(0.5, 0.0),
                            )),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    fn render_settings_select_popup(
        &self,
        open_select: SettingsSelect,
        window_id: gpui::WindowId,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let settings = self.settings_store.settings();
        let knowledge_dialog_visible = self
            .ai_entity
            .read(cx)
            .knowledge_document_dialog_owned_by(window_id);
        let active_tab = if knowledge_dialog_visible {
            SettingsTab::Knowledge
        } else {
            self.settings_workspace.read(cx).route_snapshot().active_tab
        };

        let owner = cx.entity().downgrade();
        let dispatch = Rc::new(move |request, window: &mut Window, cx: &mut App| {
            let _ = owner.update(cx, |app, cx| match request {
                SelectMenuRequest::Close => {
                    app.close_settings_select();
                    cx.notify();
                }
                SelectMenuRequest::Blur => app.blur_text_inputs(cx),
                SelectMenuRequest::Focus(input) => {
                    let value = app.current_settings_input_value(input, cx);
                    app.focus_settings_input(input, value, cx);
                    window.focus(&app.focus_handle, cx);
                }
                SelectMenuRequest::Edit(edit) => app.edit_settings(edit, cx),
                SelectMenuRequest::Background(edit) => app.edit_background_style(edit, cx),
                SelectMenuRequest::RemoteShell(mode) => app.remote_shell_integration_mode_changed(mode, cx),
                SelectMenuRequest::Trigger(clear_input, edit) => app.edit_terminal_trigger(clear_input, edit, cx),
                SelectMenuRequest::CloseSemantic => app.close_semantic_scheme_editor(cx),
                SelectMenuRequest::CloseHighlight => app.close_highlight_rule_editor(cx),
                SelectMenuRequest::Highlight(index, edit) => app.edit_highlight_rule(index, edit, cx),
                SelectMenuRequest::HighlightPreset(rules) => app.add_highlight_preset(rules, cx),
            });
        });
        let shells = if matches!(open_select, SettingsSelect::LocalShell | SettingsSelect::LocalShellSemanticScheme(_)) {
            self.effective_local_shells_for_settings(settings).into_iter().map(|shell| LocalShellOption {
                id: shell.id, label: shell.label,
            }).collect()
        } else {
            Vec::new()
        };
        let commands = &self.terminal.read(cx).quick_commands.store.commands;
        let trigger_commands = if open_select == SettingsSelect::TerminalTriggerQuickCommand {
            commands.iter()
                .filter(|command| oxideterm_quick_commands::quick_command_can_run_non_interactively(command))
                .map(|command| (command.id.clone(), command.name.clone())).collect()
        } else {
            Vec::new()
        };
        let select_menu = SettingsSelectMenu {
            settings,
            i18n: &self.i18n,
            tokens: &self.tokens,
            settings_workspace: &self.settings_workspace,
            shells,
            first_command: commands.first().map(|command| command.id.clone()).unwrap_or_default(),
            trigger_commands,
            dispatch,
        };
        if let Some(popup) = select_menu.render(active_tab, open_select, width, cx) {
            return Some(popup);
        }

        let popup = match (active_tab, open_select) {
            (_, SettingsSelect::PluginMarketplaceSort) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for sort in [
                    oxideterm_settings::PluginMarketplaceSort::Name,
                    oxideterm_settings::PluginMarketplaceSort::RecentUpdates,
                    oxideterm_settings::PluginMarketplaceSort::NewestListings,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.i18n
                                .t(plugin_manager::plugin_marketplace_sort_label(sort)),
                            sort == settings.plugin_marketplace_sort,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _, _, cx| {
                            this.close_settings_select();
                            this.set_plugin_marketplace_sort(sort, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::IdeAgentMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for mode in [
                    IdeAgentMode::Ask,
                    IdeAgentMode::Enabled,
                    IdeAgentMode::Disabled,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            ide_agent_label(mode, &self.i18n),
                            mode == settings.ide.agent_mode,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(|settings| settings.ide.agent_mode = mode, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Privilege, SettingsSelect::LocalPrivilegePromptType) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = self.settings_workspace.read(cx).privilege_prompt_type();
                for prompt_type in [
                    PrivilegePromptType::Sudo,
                    PrivilegePromptType::Su,
                    PrivilegePromptType::Custom,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.settings_privilege_prompt_type_label(prompt_type),
                            current == prompt_type,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_privilege_prompt_type(prompt_type, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Privilege, SettingsSelect::LocalPrivilegeResponseMode) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = self.settings_workspace.read(cx).privilege_response_mode();
                for response_mode in [
                    PrivilegeResponseMode::PasswordOnly,
                    PrivilegeResponseMode::PasswordThenTotp,
                    PrivilegeResponseMode::PasswordWithTotp,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            self.settings_privilege_response_mode_label(response_mode),
                            current == response_mode,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_privilege_response_mode(response_mode, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Privilege, SettingsSelect::LocalPrivilegeTotpCredential) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let selected = self
                    .settings_workspace
                    .read(cx)
                    .privilege_snapshot()
                    .totp_credential_id;
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n
                            .t("sessionManager.privilege_credentials.totp_none"),
                        selected.is_none(),
                    ),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_settings_select();
                        this.settings_workspace.update(cx, |settings, cx| {
                            settings.set_privilege_totp_credential(None, cx);
                        });
                        cx.stop_propagation();
                    }),
                ));
                for credential in self.connection_store.totp_credentials() {
                    let credential_id = credential.id.clone();
                    let label = if credential.enabled {
                        credential.name.clone()
                    } else {
                        format!(
                            "{} ({})",
                            credential.name,
                            self.i18n.t("settings_view.privilege_credentials.disabled")
                        )
                    };
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            label,
                            selected.as_deref() == Some(credential_id.as_str()),
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            let credential_id = credential_id.clone();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_privilege_totp_credential(Some(credential_id), cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ConnectionImportSource) => {
                let selected_source = self.settings_workspace.read(cx).connection_import_source();
                let mut popup = select_overlay_popup(&self.tokens, width);
                for source in connection_import_source_options().iter().copied() {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_import_source_label(source, &self.i18n),
                            source == selected_source,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.set_connection_import_source(source, cx);
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Connections, SettingsSelect::ConnectionImportDuplicateStrategy) => {
                let selected_strategy = self
                    .settings_workspace
                    .read(cx)
                    .connection_import_duplicate_strategy();
                let mut popup = select_overlay_popup(&self.tokens, width);
                for strategy in [
                    ConnectionImportDuplicateStrategy::Skip,
                    ConnectionImportDuplicateStrategy::Rename,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            connection_import_duplicate_strategy_label(strategy, &self.i18n),
                            strategy == selected_strategy,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.set_connection_import_duplicate_strategy(strategy, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Network, SettingsSelect::NetworkProxyAuth) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                let current = settings
                    .network
                    .upstream_proxy
                    .as_ref()
                    .map(|proxy| match &proxy.auth {
                        SettingsUpstreamProxyAuth::None => NetworkProxyAuthMode::None,
                        SettingsUpstreamProxyAuth::Password { .. } => {
                            NetworkProxyAuthMode::Password
                        }
                    })
                    .unwrap_or(NetworkProxyAuthMode::None);
                let has_saved_password =
                    settings
                        .network
                        .upstream_proxy
                        .as_ref()
                        .is_some_and(|proxy| {
                            matches!(
                                &proxy.auth,
                                SettingsUpstreamProxyAuth::Password {
                                    keychain_id: Some(_),
                                    ..
                                }
                            )
                        });
                for mode in [NetworkProxyAuthMode::None, NetworkProxyAuthMode::Password] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            network_proxy_auth_label(mode, &self.i18n),
                            mode == current,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.finish_network_proxy_password_action(None, cx);
                            });
                            if mode == NetworkProxyAuthMode::None
                                && has_saved_password
                                && let Err(error) = this
                                    .connection_store
                                    .delete_global_upstream_proxy_password()
                            {
                                this.settings_workspace.update(cx, |settings, cx| {
                                    settings.set_network_proxy_password_status(
                                        Some(error.to_string()),
                                        cx,
                                    );
                                });
                                cx.stop_propagation();
                                return;
                            }
                            this.edit_settings(
                                move |settings| {
                                    if let Some(proxy) = settings.network.upstream_proxy.as_mut() {
                                        proxy.auth = match mode {
                                            NetworkProxyAuthMode::None => {
                                                SettingsUpstreamProxyAuth::None
                                            }
                                            NetworkProxyAuthMode::Password => {
                                                SettingsUpstreamProxyAuth::Password {
                                                    username: String::new(),
                                                    keychain_id: None,
                                                }
                                            }
                                        };
                                    }
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::AiProviderTemplate) => {
                let mut popup = select_overlay_popup(&self.tokens, width.max(AI_PROVIDER_SELECT_W));
                for template in AI_PROVIDER_TEMPLATES {
                    let provider_type = template.provider_type;
                    let is_selected =
                        self.ai_entity.read(cx).settings_new_provider_type() == provider_type;
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, self.i18n.t(template.label_key), is_selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.select_settings_provider_type(provider_type, cx);
                            });
                            cx.stop_propagation();
                            // WorkspaceApp owns the surrounding settings render.
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Knowledge, SettingsSelect::AiEmbeddingProvider) => {
                let current = settings
                    .ai
                    .embedding_config
                    .as_ref()
                    .and_then(|config| config.get("providerId"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width.max(AI_PROVIDER_SELECT_W),
                    320.0,
                );
                popup = popup.child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n
                            .t("settings_view.knowledge.auto_embedding_provider"),
                        current.is_none(),
                    ),
                    false,
                    false,
                    cx.listener(move |this, _event, _window, cx| {
                        this.close_settings_select();
                        this.edit_settings(
                            |settings| {
                                let model = settings
                                    .ai
                                    .embedding_config
                                    .as_ref()
                                    .and_then(|config| config.get("model"))
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_string();
                                settings.ai.embedding_config = Some(serde_json::json!({
                                    "providerId": null,
                                    "model": model
                                }));
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ));
                for provider in ai_provider_views(settings) {
                    let provider_id = provider.id.clone();
                    let selected = current.as_deref() == Some(provider.id.as_str());
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, provider.name, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            let provider_id = provider_id.clone();
                            this.close_settings_select();
                            this.edit_settings(
                                move |settings| {
                                    let model = settings
                                        .ai
                                        .embedding_config
                                        .as_ref()
                                        .and_then(|config| config.get("model"))
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or_default()
                                        .to_string();
                                    settings.ai.embedding_config = Some(serde_json::json!({
                                        "providerId": provider_id,
                                        "model": model
                                    }));
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Knowledge, SettingsSelect::KnowledgeCollectionScope) => {
                let selected_connection_id = self
                    .ai_entity
                    .read(cx)
                    .knowledge_new_collection_connection_id()
                    .map(str::to_string);
                let connection_options = self
                    .connection_store
                    .connections()
                    .iter()
                    .map(|connection| (connection.id.clone(), connection.name.clone()))
                    .collect::<Vec<_>>();
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width.max(220.0),
                    KNOWLEDGE_SCOPE_SELECT_MAX_HEIGHT,
                )
                .child(select_option_action(
                    select_option(
                        &self.tokens,
                        self.i18n.t("settings_view.knowledge.scope_global"),
                        selected_connection_id.is_none(),
                    ),
                    false,
                    false,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_settings_select();
                        this.ai_entity.update(cx, |entity, cx| {
                            entity.set_knowledge_collection_connection_id(None);
                            cx.notify();
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ));
                for (connection_id, connection_name) in connection_options {
                    let selected = selected_connection_id.as_deref() == Some(&connection_id);
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, connection_name, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.ai_entity.update(cx, |entity, cx| {
                                entity.set_knowledge_collection_connection_id(Some(
                                    connection_id.clone(),
                                ));
                                cx.notify();
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Knowledge, SettingsSelect::KnowledgeDocumentFormat) => {
                let mut popup = select_overlay_popup(&self.tokens, width.max(220.0));
                for format in ["markdown", "plaintext"] {
                    let label = match format {
                        "plaintext" => self.i18n.t("settings_view.knowledge.format_plain_text"),
                        _ => self.i18n.t("settings_view.knowledge.format_markdown"),
                    };
                    let selected =
                        self.ai_entity.read(cx).knowledge_new_document_format() == format;
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.ai_entity.update(cx, |entity, cx| {
                                entity.set_knowledge_document_format(format.to_string());
                                cx.notify();
                            });
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::AiWebSearchProvider) => {
                let current = settings.ai.web_search.provider_id.clone();
                let mut popup = select_panel_overlay_popup_with_max_height(
                    &self.tokens,
                    width.max(AI_PROVIDER_SELECT_W),
                    320.0,
                );
                let options =
                    std::iter::once((None, self.i18n.t("settings_view.ai.web_provider_none")))
                        .chain(
                            ai_provider_views(settings)
                                .into_iter()
                                .filter(oxideterm_ai::web::official_deepseek_provider)
                                .map(|provider| (Some(provider.id), provider.name)),
                        );
                for (provider_id, label) in options {
                    let selected = current == provider_id;
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, selected),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            let provider_id = provider_id.clone();
                            this.close_settings_select();
                            this.edit_settings(
                                move |settings| settings.ai.web_search.provider_id = provider_id,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::AiMcpTransport) => {
                let current = self
                    .ai_entity
                    .read(cx)
                    .mcp_transport()
                    .unwrap_or(oxideterm_ai::McpTransport::Stdio);
                let mut popup = select_overlay_popup(&self.tokens, width.max(220.0));
                for (transport, label) in [
                    (oxideterm_ai::McpTransport::Stdio, "stdio"),
                    (
                        oxideterm_ai::McpTransport::StreamableHttp,
                        "Streamable HTTP (auto fallback)",
                    ),
                    (oxideterm_ai::McpTransport::LegacySse, "Legacy SSE"),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, transport == current),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.set_mcp_transport(transport, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Ai, SettingsSelect::AiMcpAuthMode) => {
                let current = self
                    .ai_entity
                    .read(cx)
                    .mcp_auth_mode()
                    .unwrap_or(oxideterm_ai::McpAuthHeaderMode::Bearer);
                let mut popup = select_overlay_popup(&self.tokens, width.max(220.0));
                for (mode, label) in [
                    (
                        oxideterm_ai::McpAuthHeaderMode::Bearer,
                        self.i18n.t("settings_view.mcp.auth_header_mode_bearer"),
                    ),
                    (
                        oxideterm_ai::McpAuthHeaderMode::Raw,
                        self.i18n.t("settings_view.mcp.auth_header_mode_raw"),
                    ),
                    (
                        oxideterm_ai::McpAuthHeaderMode::None,
                        self.i18n.t("settings_view.mcp.auth_header_mode_none"),
                    ),
                ] {
                    popup = popup.child(select_option_action(
                        select_option(&self.tokens, label, mode == current),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.ai_entity.update(cx, |ai, cx| {
                                ai.set_mcp_auth_mode(mode, cx);
                            });
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            (SettingsTab::Sftp, SettingsSelect::SftpPresentation) => {
                let mut popup = select_overlay_popup(&self.tokens, width);
                for preference in [
                    oxideterm_settings::SftpPresentationPreference::Ask,
                    oxideterm_settings::SftpPresentationPreference::Tab,
                    oxideterm_settings::SftpPresentationPreference::Sidebar,
                ] {
                    popup = popup.child(select_option_action(
                        select_option(
                            &self.tokens,
                            oxideterm_gpui_settings_view::sftp_page::sftp_presentation_label(preference, &self.i18n),
                            preference == settings.sftp.presentation,
                        ),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.close_settings_select();
                            this.edit_settings(
                                |settings| settings.sftp.presentation = preference,
                                cx,
                            );
                            if preference != oxideterm_settings::SftpPresentationPreference::Sidebar
                                && let Some(node_id) = this.embedded_sftp_node_id.clone()
                            {
                                this.close_embedded_sftp_for_node(&node_id, cx);
                            }
                            cx.stop_propagation();
                        }),
                    ));
                }
                Some(popup)
            }
            _ => None,
        }?;
        Some(overlay_content_boundary(popup).into_any_element())
    }

    pub(in crate::workspace) fn open_settings_select_from_pointer(
        &mut self,
        select_id: SettingsSelect,
        owner_window_id: gpui::WindowId,
        cx: &mut Context<Self>,
    ) {
        // Browser select triggers opened by pointer do not show a focus-visible
        // ring. Keep the origin and open/toggle rule in one place so settings,
        // AI provider, and knowledge selects do not drift apart.
        self.clear_terminal_trigger_input_focus(cx);
        self.blur_highlight_input(oxideterm_gpui_settings_view::highlight_page::is_highlight_input, cx);
        self.focused_settings_input = None;
        self.ai_entity.update(cx, |entity, cx| {
            entity.blur_settings_input(cx);
        });
        if self.open_settings_select == Some(select_id)
            && self.open_settings_select_owner_window_id == Some(owner_window_id)
        {
            self.close_settings_select();
            return;
        }
        if select_id.theme_target().is_some() {
            let settings = self.settings_store.settings();
            self.settings_theme_picker.update(cx, |picker, cx| picker.open(select_id, settings, cx));
        }
        self.open_settings_select = Some(select_id);
        self.open_settings_select_owner_window_id = Some(owner_window_id);
        self.settings_select_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Pointer);
    }

    pub(in crate::workspace) fn handle_appearance_theme_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self
            .open_settings_select
            .and_then(SettingsSelect::theme_target)
        else {
            return false;
        };
        let system_dark = self
            .open_settings_select
            .and_then(SettingsSelect::theme_system_dark);
        if event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" | "tab" => {
                self.close_settings_select();
                cx.notify();
                true
            }
            "enter" | "space" | " " => {
                if let Some(id) = self.settings_theme_picker.read(cx).preview().map(str::to_owned) {
                    self.close_settings_select();
                    self.edit_settings(
                        |settings| target.apply_for_scheme(settings, system_dark, id),
                        cx,
                    );
                }
                true
            }
            "up" | "arrowup" | "down" | "arrowdown" | "home" | "end" => {
                self.settings_theme_picker.update(cx, |picker, cx| picker.navigate(&event.keystroke.key, cx));
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn select_setting_row(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = self.settings_select_control(select_id, value, false, Some(width), cx);

        self.setting_row(label_key, hint_key, control, cx)
    }

    pub(in crate::workspace) fn bool_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let control = self.settings_components().toggle_control(
            label_key, checked, setter, Self::settings_window_dispatch(cx),
        );
        self.setting_row(label_key, hint_key, control, cx)
    }

    pub(in crate::workspace) fn number_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Numeric rows use the same focus, IME, caret, and draft pipeline as
        // every other settings text field instead of simulating a click stepper.
        let control = self.number_input(
            input,
            value.to_string(),
            self.tokens.metrics.settings_number_input_width,
            cx,
        );
        self.setting_row(label_key, hint_key, control, cx)
    }

    pub(in crate::workspace) fn setting_row(
        &self,
        label_key: &str,
        hint_key: &str,
        control: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.render_selectable_text_scoped(
            "settings-row-label",
            label_key,
            self.i18n.t(label_key),
            self.tokens.ui.text,
            cx,
        );
        let hint = (!hint_key.is_empty()).then(|| {
            self.render_selectable_text_scoped(
                "settings-row-hint",
                hint_key,
                self.i18n.t(hint_key),
                self.tokens.ui.text_muted,
                cx,
            )
        });
        settings_row(&self.tokens, label, hint, control)
    }
}
