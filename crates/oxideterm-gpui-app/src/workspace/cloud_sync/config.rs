// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl CloudSyncPageRenderer {
    pub(super) fn render_cloud_sync_config_connection_card(&self, cx: &mut App) -> AnyElement {
        let (config_rows, auto_upload_enabled, busy) = {
            let cloud_sync = self.cloud_sync.read(cx);
            (
                cloud_sync_config_rows(
                    &cloud_sync.view.form.backend_type,
                    &cloud_sync.view.form.auth_mode,
                ),
                cloud_sync.view.form.auto_upload_enabled,
                cloud_sync.operation_in_flight(),
            )
        };
        let mut connection_rows = Vec::with_capacity(config_rows.len());
        for row in config_rows {
            connection_rows.push(match row {
                CloudSyncConfigRow::BackendSelect => self.render_backend_select(cx),
                CloudSyncConfigRow::AuthModeSelect => self.render_auth_mode_select(cx),
                CloudSyncConfigRow::Text(field) => self.render_text_field(
                    field.label_key,
                    field.input,
                    field.placeholder_key,
                    false,
                    cx,
                ),
                CloudSyncConfigRow::Secret(field) => self.render_secret_field(
                    field.label_key,
                    field.input,
                    field.placeholder_key,
                    field.secret_key,
                    cx,
                ),
                CloudSyncConfigRow::AutoUploadToggle => self.render_form_toggle(
                    "plugin.cloud_sync.settings.auto_upload_enabled",
                    auto_upload_enabled,
                    {
                        let cloud_sync = self.cloud_sync.clone();
                        move |_event, _window, cx| {
                            cloud_sync.update(cx, |cloud_sync, cx| {
                                cloud_sync.view.form.auto_upload_enabled =
                                    !cloud_sync.view.form.auto_upload_enabled;
                                cloud_sync.clear_select_focus();
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }
                    },
                    cx,
                ),
                CloudSyncConfigRow::ConflictSelect => self.render_conflict_select(cx),
            });
        }
        self.render
            .plugin_card()
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        self.render
                            .section_title("plugin.cloud_sync.sections.connection_settings", cx),
                    )
                    .child(self.render_cloud_sync_toolbar_button(
                        LucideIcon::Save,
                        "plugin.cloud_sync.actions.save_settings",
                        CloudSyncActionTone::Muted,
                        busy,
                        self.intent_listener(CloudSyncUiIntent::SaveConfiguration),
                    )),
            )
            .child(cloud_sync_form_grid(connection_rows))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_scope_card(&self, cx: &mut App) -> AnyElement {
        let scope = normalize_sync_scope(
            Some(&self.cloud_sync.read(cx).controller.store.state().sync_scope),
            &[],
        );
        let mut toggles = vec![
            self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_connections",
                scope.sync_connections,
                |scope, next| scope.sync_connections = Some(next),
                cx,
            ),
            self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_forwards",
                scope.sync_forwards,
                |scope, next| scope.sync_forwards = Some(next),
                cx,
            ),
            self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_quick_commands",
                scope.sync_quick_commands,
                |scope, next| scope.sync_quick_commands = Some(next),
                cx,
            ),
            self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_serial_profiles",
                scope.sync_serial_profiles,
                |scope, next| scope.sync_serial_profiles = Some(next),
                cx,
            ),
            self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_telnet_profiles",
                scope.sync_telnet_profiles,
                |scope, next| scope.sync_telnet_profiles = Some(next),
                cx,
            ),
            self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_mosh_profiles",
                scope.sync_mosh_profiles,
                |scope, next| scope.sync_mosh_profiles = Some(next),
                cx,
            ),
            self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_remote_desktop_profiles",
                scope.sync_remote_desktop_profiles,
                |scope, next| scope.sync_remote_desktop_profiles = Some(next),
                cx,
            ),
            self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_sensitive_credentials",
                scope.sync_sensitive_credentials,
                |scope, next| scope.sync_sensitive_credentials = Some(next),
                cx,
            ),
            self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_app_settings",
                scope.sync_app_settings,
                |scope, next| scope.sync_app_settings = Some(next),
                cx,
            ),
        ];

        if scope.sync_app_settings {
            for section_id in OXIDE_APP_SETTINGS_SECTION_IDS {
                let section_id = (*section_id).to_string();
                let label = cloud_sync_app_settings_section_label_key(&section_id)
                    .map(|key| self.i18n.t(key))
                    .unwrap_or_else(|| section_id.clone());
                toggles.push(self.render_scope_section_toggle(
                    format!("cloud-sync-scope-section-{section_id}"),
                    label,
                    scope.app_settings_sections.contains(&section_id),
                    section_id,
                    cx,
                ));
            }
            toggles.push(self.render_scope_bool_toggle(
                "plugin.cloud_sync.settings.include_local_terminal_env_vars",
                scope.include_local_terminal_env_vars,
                |scope, next| scope.include_local_terminal_env_vars = Some(next),
                cx,
            ));
        }

        toggles.push(self.render_scope_bool_toggle(
            "plugin.cloud_sync.settings.sync_plugin_settings",
            scope.sync_plugin_settings,
            |scope, next| scope.sync_plugin_settings = Some(next),
            cx,
        ));

        self.render
            .plugin_card()
            .child(
                self.render
                    .section_title("plugin.cloud_sync.sections.sync_scope", cx),
            )
            .child(cloud_sync_toggle_grid(&self.tokens, toggles))
            .into_any_element()
    }

    fn render_scope_bool_toggle(
        &self,
        label_key: &'static str,
        checked: bool,
        update: fn(&mut RawSyncScope, bool),
        cx: &mut App,
    ) -> AnyElement {
        let cloud_sync = self.cloud_sync.clone();
        self.render_toggle(
            label_key,
            checked,
            move |_event, _window, cx| {
                cloud_sync.update(cx, |cloud_sync, cx| {
                    if label_key == "plugin.cloud_sync.settings.sync_sensitive_credentials"
                        && !checked
                    {
                        cloud_sync.view.confirm = Some(CloudSyncConfirm::EnableSensitiveSync);
                        cloud_sync.view.confirm_presence.reopen();
                        // Pointer-opened confirms have no keyboard footer focus yet.
                        cloud_sync.view.confirm_focused_action = None;
                        cx.notify();
                        return;
                    }
                    update(
                        &mut cloud_sync.controller.store.state_mut().sync_scope,
                        !checked,
                    );
                    cloud_sync.clear_select_focus();
                    cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                        CloudSyncUiIntent::FinishScopeEdit,
                    ));
                    cx.notify();
                });
                cx.stop_propagation();
            },
            cx,
        )
    }

    fn render_scope_section_toggle(
        &self,
        label_identity: String,
        label: String,
        checked: bool,
        section_id: String,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let cloud_sync = self.cloud_sync.clone();
        cloud_sync_toggle(
            &self.tokens,
            self.render.selectable_text(
                SelectableTextRole::NonSelectable,
                "cloud-sync-scope-section-toggle",
                label_identity,
                label,
                theme.text_muted,
                cx,
            ),
            checked,
            move |_event, _window, cx| {
                cloud_sync.update(cx, |cloud_sync, cx| {
                    let mut sections = normalize_sync_scope(
                        Some(&cloud_sync.controller.store.state().sync_scope),
                        &[],
                    )
                    .app_settings_sections;
                    if sections.iter().any(|section| section == &section_id) {
                        sections.retain(|section| section != &section_id);
                    } else {
                        sections.push(section_id.clone());
                    }
                    cloud_sync
                        .controller
                        .store
                        .state_mut()
                        .sync_scope
                        .app_settings_sections = Some(sections);
                    cloud_sync.clear_select_focus();
                    cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                        CloudSyncUiIntent::FinishScopeEdit,
                    ));
                    cx.notify();
                });
                cx.stop_propagation();
            },
        )
    }

    fn render_text_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        placeholder_key: &str,
        secret: bool,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = self.input.focused_input == Some(input);
        let input_control = {
            let cloud_sync = self.cloud_sync.read(cx);
            let value = if focused {
                self.input.active_value.as_deref().unwrap_or_default()
            } else {
                // Unfocused values remain borrowed from the Entity; secret text is
                // masked by the input primitive and never copied into the renderer.
                cloud_sync_form_input_value_ref(&cloud_sync.view.form, input).unwrap_or_default()
            };
            text_input(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder: self.i18n.t(placeholder_key),
                    focused,
                    caret_visible: self.input.caret_visible,
                    secret,
                    selected_all: false,
                    selected_range: self.input.selected_range.clone(),
                    marked_text: self.input.marked_text.as_deref(),
                },
            )
            .w_full()
            .min_w(px(0.0))
            .cursor(CursorStyle::IBeam)
        };
        let target = WorkspaceImeTarget::Settings(input);
        let click_cloud_sync = self.cloud_sync.clone();
        let move_cloud_sync = self.cloud_sync.clone();
        let anchor_cloud_sync = self.cloud_sync.clone();
        cloud_sync_field_row(
            &self.tokens,
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme.text_muted))
                .child(self.render.selectable_text(
                    SelectableTextRole::NonSelectable,
                    "cloud-sync-text-field-label",
                    label_key,
                    self.i18n.t(label_key),
                    theme.text_muted,
                    cx,
                ))
                .into_any_element(),
            text_input_anchor_probe(
                target.anchor_id(),
                input_control
                    .on_mouse_down(
                        MouseButton::Left,
                        move |event: &MouseDownEvent, window, cx| {
                            click_cloud_sync.update(cx, |cloud_sync, cx| {
                                cloud_sync.clear_select_focus();
                                cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                                    CloudSyncUiIntent::BeginInputSelection {
                                        input,
                                        event: event.clone(),
                                        source_window: window.window_handle(),
                                    },
                                ));
                                cx.notify();
                            });
                            cx.stop_propagation();
                        },
                    )
                    .on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                        move_cloud_sync.update(cx, |_cloud_sync, cx| {
                            cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                                CloudSyncUiIntent::UpdateInputSelection {
                                    event: event.clone(),
                                    source_window: window.window_handle(),
                                },
                            ));
                        });
                    }),
                move |anchor, window, cx| {
                    anchor_cloud_sync.update(cx, |_cloud_sync, cx| {
                        cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                            CloudSyncUiIntent::UpdateInputAnchor {
                                anchor,
                                source_window: window.window_handle(),
                            },
                        ));
                    });
                },
            )
            .into_any_element(),
        )
    }

    fn render_secret_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        placeholder_key: &str,
        secret_key: &'static str,
        cx: &mut App,
    ) -> AnyElement {
        let stored = self
            .cloud_sync
            .read(cx)
            .controller
            .store
            .state()
            .secret_hints
            .get(secret_key)
            .copied()
            .unwrap_or(false);
        let placeholder = if stored {
            "plugin.cloud_sync.placeholders.secret_stored"
        } else {
            placeholder_key
        };
        let action = stored.then(|| {
            let label = self.i18n.t(label_key);
            let cloud_sync = self.cloud_sync.clone();
            self.render.inline_button(
                "plugin.cloud_sync.actions.clear_secret",
                move |_event, _window, cx| {
                    cloud_sync.update(cx, |cloud_sync, cx| {
                        cloud_sync.view.confirm = Some(CloudSyncConfirm::ClearSecret {
                            key: secret_key.to_string(),
                            label: label.clone(),
                        });
                        cloud_sync.view.confirm_presence.reopen();
                        // Pointer-opened confirms have no keyboard footer focus yet.
                        cloud_sync.view.confirm_focused_action = None;
                        cloud_sync.clear_select_focus();
                        cx.notify();
                    });
                    cx.stop_propagation();
                },
            )
        });
        cloud_sync_secret_row(
            self.render_text_field(label_key, input, placeholder, true, cx),
            action,
        )
    }

    fn render_backend_select(&self, cx: &mut App) -> AnyElement {
        let label = {
            let cloud_sync = self.cloud_sync.read(cx);
            self.i18n.t(cloud_sync_backend_label_key(
                &cloud_sync.view.form.backend_type,
            ))
        };
        self.render_select_field(
            "plugin.cloud_sync.settings.backend_type",
            CloudSyncSelect::Backend,
            label,
            cx,
        )
    }

    fn render_auth_mode_select(&self, cx: &mut App) -> AnyElement {
        let current = match self.cloud_sync.read(cx).view.form.auth_mode {
            AuthMode::Bearer => self.i18n.t("plugin.cloud_sync.auth.bearer"),
            AuthMode::Basic => self.i18n.t("plugin.cloud_sync.auth.basic"),
            AuthMode::None => self.i18n.t("plugin.cloud_sync.auth.none"),
        };
        self.render_select_field(
            "plugin.cloud_sync.settings.auth_mode",
            CloudSyncSelect::AuthMode,
            current,
            cx,
        )
    }

    fn render_conflict_select(&self, cx: &mut App) -> AnyElement {
        let current = match self.cloud_sync.read(cx).view.form.default_conflict_strategy {
            ConflictStrategy::Merge => self.i18n.t("plugin.cloud_sync.conflict.merge"),
            ConflictStrategy::Replace => self.i18n.t("plugin.cloud_sync.conflict.replace"),
            ConflictStrategy::Skip => self.i18n.t("plugin.cloud_sync.conflict.skip"),
            ConflictStrategy::Rename => self.i18n.t("plugin.cloud_sync.conflict.rename"),
        };
        self.render_select_field(
            "plugin.cloud_sync.settings.default_conflict_strategy",
            CloudSyncSelect::ConflictStrategy,
            current,
            cx,
        )
    }

    fn selected_option_index(&self, select: CloudSyncSelect, cx: &App) -> usize {
        let cloud_sync = self.cloud_sync.read(cx);
        let settings = CloudSyncSettings {
            backend_type: cloud_sync.view.form.backend_type.clone(),
            auth_mode: cloud_sync.view.form.auth_mode.clone(),
            default_conflict_strategy: cloud_sync.view.form.default_conflict_strategy.clone(),
            ..CloudSyncSettings::default()
        };
        cloud_sync_selected_option_spec_index(&settings, select)
    }

    fn render_select_field(
        &self,
        label_key: &str,
        select: CloudSyncSelect,
        value: String,
        cx: &mut App,
    ) -> AnyElement {
        let (open, focused, focus_origin) = {
            let cloud_sync = self.cloud_sync.read(cx);
            (
                cloud_sync.view.open_select == Some(select),
                cloud_sync.view.focused_select == Some(select),
                cloud_sync.view.select_focus_origin,
            )
        };
        let focus_visible = browser_behavior::browser_focus_visible(focused, focus_origin);
        let anchor_id = Self::select_anchor_id(select);
        let trigger = self.render_select_trigger(select, value, open, focused, focus_visible, cx);
        let anchor_cloud_sync = self.cloud_sync.clone();
        cloud_sync_select_field(
            &self.tokens,
            self.render.selectable_text(
                SelectableTextRole::NonSelectable,
                "cloud-sync-select-label",
                label_key,
                self.i18n.t(label_key),
                self.tokens.ui.text_muted,
                cx,
            ),
            div()
                .relative()
                .w_full()
                .child(select_anchor_probe(
                    anchor_id,
                    trigger,
                    move |anchor, window, cx| {
                        anchor_cloud_sync.update(cx, |_cloud_sync, cx| {
                            cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                                CloudSyncUiIntent::UpdateSelectAnchor {
                                    anchor,
                                    source_window: window.window_handle(),
                                },
                            ));
                        });
                    },
                ))
                .into_any_element(),
            None,
        )
    }

    fn render_select_trigger(
        &self,
        select: CloudSyncSelect,
        value: String,
        open: bool,
        focused: bool,
        focus_visible: bool,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected_index = self.selected_option_index(select, cx);
        let cloud_sync = self.cloud_sync.clone();
        cloud_sync_select_trigger(
            &self.tokens,
            open,
            focused,
            focus_visible,
            self.render.selectable_text(
                SelectableTextRole::NonSelectable,
                "cloud-sync-select-value",
                format!("{select:?}"),
                value,
                theme.text,
                cx,
            ),
            move |_event, _window, cx| {
                cloud_sync.update(cx, |cloud_sync, cx| {
                    if cloud_sync.view.open_select == Some(select) {
                        cloud_sync.close_select();
                    } else {
                        browser_behavior::toggle_browser_highlighted_select_from_pointer(
                            &mut cloud_sync.view.open_select,
                            &mut cloud_sync.view.focused_select,
                            &mut cloud_sync.view.select_focus_origin,
                            &mut cloud_sync.view.select_highlighted,
                            select,
                            selected_index,
                        );
                    }
                    cx.notify();
                });
                cx.stop_propagation();
            },
        )
    }

    fn select_anchor_id(select: CloudSyncSelect) -> SelectAnchorId {
        match select {
            CloudSyncSelect::Backend => SelectAnchorId::CloudSyncBackend,
            CloudSyncSelect::AuthMode => SelectAnchorId::CloudSyncAuthMode,
            CloudSyncSelect::ConflictStrategy => SelectAnchorId::CloudSyncConflictStrategy,
        }
    }

    fn render_toggle(
        &self,
        label_key: &str,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut App,
    ) -> AnyElement {
        cloud_sync_toggle(
            &self.tokens,
            self.render.selectable_text(
                SelectableTextRole::NonSelectable,
                "cloud-sync-toggle-label",
                label_key,
                self.i18n.t(label_key),
                self.tokens.ui.text_muted,
                cx,
            ),
            checked,
            listener,
        )
    }

    fn render_form_toggle(
        &self,
        label_key: &str,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut App,
    ) -> AnyElement {
        cloud_sync_form_toggle(
            &self.tokens,
            self.render.selectable_text(
                SelectableTextRole::NonSelectable,
                "cloud-sync-form-toggle-label",
                label_key,
                self.i18n.t(label_key),
                self.tokens.ui.text_muted,
                cx,
            ),
            checked,
            listener,
        )
    }
}

impl CloudSyncWorkspaceEntity {
    pub(super) fn clear_select_focus(&mut self) {
        browser_behavior::clear_browser_highlighted_select_focus(
            &mut self.view.open_select,
            &mut self.view.focused_select,
            &mut self.view.select_focus_origin,
            &mut self.view.select_highlighted,
        );
    }

    fn close_select(&mut self) {
        // Dropdown dismissal is synchronous and independent of motion settings.
        self.view.open_select = None;
        self.view.select_highlighted = None;
    }
}

impl WorkspaceApp {
    pub(super) fn cloud_sync_select_options(
        &self,
        select: CloudSyncSelect,
        cx: &App,
    ) -> Vec<CloudSyncSelectOption> {
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync.read(cx).view.form.backend_type.clone(),
            auth_mode: self.cloud_sync.read(cx).view.form.auth_mode.clone(),
            default_conflict_strategy: self
                .cloud_sync
                .read(cx)
                .view
                .form
                .default_conflict_strategy
                .clone(),
            ..CloudSyncSettings::default()
        };
        cloud_sync_select_option_specs(&settings, select)
            .into_iter()
            .map(|option| CloudSyncSelectOption {
                label: self.i18n.t(cloud_sync_select_label_key(option.label_key)),
                selected: option.selected,
                action: option.action,
            })
            .collect()
    }

    pub(super) fn cloud_sync_selected_option_index(
        &self,
        select: CloudSyncSelect,
        cx: &App,
    ) -> usize {
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync.read(cx).view.form.backend_type.clone(),
            auth_mode: self.cloud_sync.read(cx).view.form.auth_mode.clone(),
            default_conflict_strategy: self
                .cloud_sync
                .read(cx)
                .view
                .form
                .default_conflict_strategy
                .clone(),
            ..CloudSyncSettings::default()
        };
        cloud_sync_selected_option_spec_index(&settings, select)
    }

    pub(super) fn cloud_sync_focusable_selects(&self, cx: &App) -> Vec<CloudSyncSelect> {
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync.read(cx).view.form.backend_type.clone(),
            auth_mode: self.cloud_sync.read(cx).view.form.auth_mode.clone(),
            default_conflict_strategy: self
                .cloud_sync
                .read(cx)
                .view
                .form
                .default_conflict_strategy
                .clone(),
            ..CloudSyncSettings::default()
        };
        cloud_sync_focusable_selects(&settings)
    }

    pub(super) fn cloud_sync_select_anchor_id(select: CloudSyncSelect) -> SelectAnchorId {
        match select {
            CloudSyncSelect::Backend => SelectAnchorId::CloudSyncBackend,
            CloudSyncSelect::AuthMode => SelectAnchorId::CloudSyncAuthMode,
            CloudSyncSelect::ConflictStrategy => SelectAnchorId::CloudSyncConflictStrategy,
        }
    }

    pub(super) fn clear_cloud_sync_select_focus(&mut self, cx: &mut Context<Self>) {
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            browser_behavior::clear_browser_highlighted_select_focus(
                &mut cloud_sync.view.open_select,
                &mut cloud_sync.view.focused_select,
                &mut cloud_sync.view.select_focus_origin,
                &mut cloud_sync.view.select_highlighted,
            );
        });
    }

    pub(super) fn close_cloud_sync_select(&mut self, cx: &mut Context<Self>) {
        // Dropdown dismissal is synchronous and independent of motion settings.
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.open_select = None;
            cloud_sync.view.select_highlighted = None;
        });
    }

    pub(super) fn apply_cloud_sync_select_action(
        &mut self,
        action: CloudSyncSelectAction,
        cx: &mut Context<Self>,
    ) {
        // Tauri's Radix Select uses the same onValueChange path for mouse and
        // keyboard selection. Keep native mutations centralized so Enter and
        // pointer clicks cannot drift apart.
        let trigger_select = self.cloud_sync.update(cx, |cloud_sync, _cx| match action {
            CloudSyncSelectAction::Backend(backend) => {
                cloud_sync.view.form.backend_type = backend.clone();
                if matches!(backend, BackendType::Dropbox) {
                    cloud_sync.view.form.auth_mode = AuthMode::Bearer;
                } else if matches!(
                    backend,
                    BackendType::GithubGist | BackendType::Git | BackendType::S3
                ) {
                    cloud_sync.view.form.auth_mode = AuthMode::None;
                }
                CloudSyncSelect::Backend
            }
            CloudSyncSelectAction::AuthMode(auth_mode) => {
                cloud_sync.view.form.auth_mode = auth_mode;
                CloudSyncSelect::AuthMode
            }
            CloudSyncSelectAction::ConflictStrategy(strategy) => {
                cloud_sync.view.form.default_conflict_strategy = strategy;
                CloudSyncSelect::ConflictStrategy
            }
        });
        self.close_cloud_sync_select(cx);
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.focused_select = Some(trigger_select);
            cloud_sync.view.select_highlighted = None;
        });
        cx.notify();
    }

    pub(in crate::workspace) fn handle_cloud_sync_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        let effect = reduce_cloud_sync_select_key(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            CloudSyncSelectKeyState {
                open_select: self.cloud_sync.read(cx).view.open_select,
                focused_select: self.cloud_sync.read(cx).view.focused_select,
                highlighted_option: self.cloud_sync.read(cx).view.select_highlighted,
            },
            &self.cloud_sync_focusable_selects(cx),
            |select| self.cloud_sync_selected_option_index(select, cx),
            |select| self.cloud_sync_select_options(select, cx).len(),
        );
        let CloudSyncSelectKeyEffect::Handled {
            state,
            keyboard_focus_origin,
            selected_action_index,
        } = effect
        else {
            return false;
        };
        let previous_open_select = self.cloud_sync.read(cx).view.open_select;
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.open_select = state.open_select;
            cloud_sync.view.focused_select = state.focused_select;
            cloud_sync.view.select_highlighted = state.highlighted_option;
            if keyboard_focus_origin {
                cloud_sync.view.select_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
            }
        });
        if previous_open_select.is_some()
            && state.open_select.is_none()
            && event.keystroke.key.as_str() == "escape"
        {
            // Escape closes the dropdown synchronously like pointer dismissal.
            self.close_cloud_sync_select(cx);
        }
        if let (Some(select), Some(index)) = (
            self.cloud_sync.read(cx).view.focused_select,
            selected_action_index,
        ) {
            if let Some(action) = self
                .cloud_sync_select_options(select, cx)
                .get(index)
                .map(|option| option.action.clone())
            {
                self.apply_cloud_sync_select_action(action, cx);
            }
        }
        cx.notify();
        true
    }

    pub(super) fn render_cloud_sync_select_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Cloud Sync uses one open state so dropdown rendering cannot outlive
        // logical dismissal or depend on the animation profile.
        let select = self.cloud_sync.read(cx).view.open_select?;
        let anchor_id = Self::cloud_sync_select_anchor_id(select);
        let anchor = self.select_anchors.get(&anchor_id).copied()?;
        let width =
            f32::from(anchor.bounds.size.width).max(self.tokens.metrics.ui_select_min_width);
        let mut popup = select_panel_overlay_popup_with_max_height(
            &self.tokens,
            width,
            self.tokens.metrics.ui_select_max_height,
        );
        let highlighted = self
            .cloud_sync
            .read(cx)
            .view
            .select_highlighted
            .filter(|(highlighted_select, _)| *highlighted_select == select)
            .map(|(_, index)| index)
            .unwrap_or_else(|| self.cloud_sync_selected_option_index(select, cx));
        let options = self.cloud_sync_select_options(select, cx);
        for (index, option) in options.into_iter().enumerate() {
            let label = option.label;
            let selected = option.selected;
            let action = option.action;
            let option_el = select_option_highlighted(
                &self.tokens,
                label.clone(),
                selected,
                highlighted == index,
            )
            .on_mouse_move(cx.listener(move |this, _event, _window, cx| {
                if this.cloud_sync.read(cx).view.select_highlighted != Some((select, index)) {
                    this.cloud_sync.update(cx, |cloud_sync, _cx| {
                        cloud_sync.view.select_highlighted = Some((select, index));
                    });
                    cx.notify();
                }
            }));
            popup = popup.child(select_option_action(
                option_el,
                false,
                false,
                cx.listener(move |this, _event, _window, cx| {
                    this.close_cloud_sync_select(cx);
                    this.cloud_sync.update(cx, |cloud_sync, _cx| {
                        cloud_sync.view.select_focus_origin =
                            Some(browser_behavior::BrowserFocusOrigin::Pointer);
                    });
                    this.apply_cloud_sync_select_action(action.clone(), cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }
        let popup = overlay_content_boundary(popup).into_any_element();

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
                            .child(popup),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    pub(super) fn save_cloud_sync_configuration(&mut self, cx: &mut Context<Self>) {
        self.persist_cloud_sync_configuration(true, cx);
    }

    pub(super) fn persist_cloud_sync_configuration(
        &mut self,
        show_success_toast: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        self.apply_focused_cloud_sync_input_draft(cx);
        self.invalidate_cloud_sync_snapshot_caches(cx);
        let (settings, interval) = {
            let cloud_sync = self.cloud_sync.read(cx);
            cloud_sync_settings_from_form(&cloud_sync.view.form)
        };
        let mut provider = CloudSyncKeychainSecretProvider::new(
            self.cloud_sync
                .read(cx)
                .controller
                .store
                .state()
                .secret_hints
                .clone(),
        );
        let secret_handoff = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.form.take_secret_handoff()
        });
        let secret_result = store_cloud_sync_touched_secrets(&secret_handoff, &mut provider);
        if let Err(error) = secret_result {
            // Credential-store failures contain operation context and platform
            // status details, never the submitted secret values.
            let error_message = format!("{error:#}");
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.view.form.restore_secret_handoff(secret_handoff);
                cloud_sync.controller.store.state_mut().last_error = Some(error_message.clone());
            });
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.settings_saved_failed_title"),
                Some(error_message),
                TerminalNoticeVariant::Error,
                cx,
            );
            return false;
        }
        let save_result = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.store.state_mut().settings = settings;
            cloud_sync.controller.store.state_mut().secret_hints = provider.hints().clone();
            cloud_sync.controller.store.save()
        });
        if let Err(error) = save_result {
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.controller.store.state_mut().last_error = Some(error.to_string());
            });
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.settings_saved_failed_title"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
            return false;
        } else {
            // Successful synchronous persistence drops and zeroizes the
            // transferred buffers before the UI draft is normalized.
            drop(secret_handoff);
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                normalize_cloud_sync_interval_draft(&mut cloud_sync.view.form, interval);
                reset_cloud_sync_secret_drafts(&mut cloud_sync.view.form);
                cloud_sync.controller.store.state_mut().last_error = None;
            });
            if show_success_toast {
                self.push_cloud_sync_toast(
                    self.i18n.t("plugin.cloud_sync.toast.settings_saved_title"),
                    None,
                    TerminalNoticeVariant::Success,
                    cx,
                );
            }
            self.reschedule_cloud_sync_auto_upload(cx);
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        true
    }
}
