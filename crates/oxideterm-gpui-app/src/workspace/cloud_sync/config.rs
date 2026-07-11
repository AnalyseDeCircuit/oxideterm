// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn render_cloud_sync_scope_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let raw_scope = &self.cloud_sync.controller.store.state().sync_scope;
        let scope = normalize_sync_scope(Some(raw_scope), &[]);
        let mut toggles = vec![
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_connections",
                scope.sync_connections,
                |scope, next| scope.sync_connections = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_forwards",
                scope.sync_forwards,
                |scope, next| scope.sync_forwards = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_quick_commands",
                scope.sync_quick_commands,
                |scope, next| scope.sync_quick_commands = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_serial_profiles",
                scope.sync_serial_profiles,
                |scope, next| scope.sync_serial_profiles = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_raw_tcp_profiles",
                scope.sync_raw_tcp_profiles,
                |scope, next| scope.sync_raw_tcp_profiles = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_raw_udp_profiles",
                scope.sync_raw_udp_profiles,
                |scope, next| scope.sync_raw_udp_profiles = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_sensitive_credentials",
                scope.sync_sensitive_credentials,
                |scope, next| scope.sync_sensitive_credentials = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
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
                toggles.push(self.render_cloud_sync_scope_section_toggle(
                    format!("cloud-sync-scope-section-{section_id}"),
                    label,
                    scope.app_settings_sections.contains(&section_id),
                    section_id,
                    cx,
                ));
            }
            toggles.push(self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.include_local_terminal_env_vars",
                scope.include_local_terminal_env_vars,
                |scope, next| scope.include_local_terminal_env_vars = Some(next),
                cx,
            ));
        }

        toggles.push(self.render_cloud_sync_scope_bool_toggle(
            "plugin.cloud_sync.settings.sync_plugin_settings",
            scope.sync_plugin_settings,
            |scope, next| scope.sync_plugin_settings = Some(next),
            cx,
        ));

        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(
                self.render_cloud_sync_section_title("plugin.cloud_sync.sections.sync_scope", cx),
            )
            .child(cloud_sync_toggle_grid(&self.tokens, toggles))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_scope_bool_toggle(
        &self,
        label_key: &'static str,
        checked: bool,
        update: fn(&mut RawSyncScope, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_cloud_sync_toggle(
            label_key,
            checked,
            cx.listener(
                move |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    if label_key == "plugin.cloud_sync.settings.sync_sensitive_credentials"
                        && !checked
                    {
                        this.cloud_sync.view.confirm = Some(CloudSyncConfirm::EnableSensitiveSync);
                        this.cloud_sync.view.confirm_presence.reopen();
                        // Pointer-opened confirms should not paint a footer focus state
                        // until keyboard navigation explicitly enters the footer.
                        this.cloud_sync.view.confirm_focused_action = None;
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    update(
                        &mut this.cloud_sync.controller.store.state_mut().sync_scope,
                        !checked,
                    );
                    this.finish_cloud_sync_scope_edit(cx);
                    cx.stop_propagation();
                },
            ),
            cx,
        )
    }

    pub(super) fn render_cloud_sync_scope_section_toggle(
        &self,
        label_identity: String,
        label: String,
        checked: bool,
        section_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        cloud_sync_toggle(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-scope-section-toggle",
                label_identity,
                label,
                theme.text_muted,
                cx,
            ),
            checked,
            cx.listener(
                move |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    this.toggle_cloud_sync_app_settings_section(&section_id);
                    this.finish_cloud_sync_scope_edit(cx);
                    cx.stop_propagation();
                },
            ),
        )
    }

    pub(super) fn toggle_cloud_sync_app_settings_section(&mut self, section_id: &str) {
        let mut sections = normalize_sync_scope(
            Some(&self.cloud_sync.controller.store.state().sync_scope),
            &[],
        )
        .app_settings_sections;
        if sections.iter().any(|section| section == section_id) {
            sections.retain(|section| section != section_id);
        } else {
            sections.push(section_id.to_string());
        }
        self.cloud_sync
            .controller
            .store
            .state_mut()
            .sync_scope
            .app_settings_sections = Some(sections);
    }

    pub(super) fn finish_cloud_sync_scope_edit(&mut self, cx: &mut Context<Self>) {
        self.clear_cloud_sync_select_focus();
        self.refresh_cloud_sync_local_dirty_state();
        self.save_cloud_sync_state();
        cx.notify();
    }

    pub(super) fn render_cloud_sync_text_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        placeholder_key: &str,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.current_settings_input_value(input)
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        cloud_sync_field_row(
            &self.tokens,
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme.text_muted))
                .child(self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "cloud-sync-text-field-label",
                    label_key,
                    self.i18n.t(label_key),
                    theme.text_muted,
                    cx,
                ))
                .into_any_element(),
            text_input_anchor_probe(
                target.anchor_id(),
                text_input(
                    &self.tokens,
                    TextInputView {
                        value: &value,
                        placeholder: self.i18n.t(placeholder_key),
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
                        this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(
                    |this, event: &gpui::MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    },
                )),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            )
            .into_any_element(),
        )
    }

    pub(super) fn render_cloud_sync_secret_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        placeholder_key: &str,
        secret_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stored = self
            .cloud_sync
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
        let action = if stored {
            let label = self.i18n.t(label_key);
            Some(self.render_cloud_sync_inline_button(
                "plugin.cloud_sync.actions.clear_secret",
                cx.listener(
                    move |this: &mut WorkspaceApp,
                          _event,
                          _window,
                          cx: &mut Context<WorkspaceApp>| {
                        this.cloud_sync.view.confirm = Some(CloudSyncConfirm::ClearSecret {
                            key: secret_key.to_string(),
                            label: label.clone(),
                        });
                        this.cloud_sync.view.confirm_presence.reopen();
                        // Pointer-opened confirms should not paint a footer focus state
                        // until keyboard navigation explicitly enters the footer.
                        this.cloud_sync.view.confirm_focused_action = None;
                        this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
                cx,
            ))
        } else {
            None
        };
        cloud_sync_secret_row(
            self.render_cloud_sync_text_field(label_key, input, placeholder, true, cx),
            action,
        )
    }

    pub(super) fn render_cloud_sync_backend_select(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_cloud_sync_select_field(
            "plugin.cloud_sync.settings.backend_type",
            CloudSyncSelect::Backend,
            self.i18n.t(cloud_sync_backend_label_key(
                &self.cloud_sync.view.form.backend_type,
            )),
            cx,
        )
    }

    pub(super) fn render_cloud_sync_auth_mode_select(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = match self.cloud_sync.view.form.auth_mode {
            AuthMode::Bearer => self.i18n.t("plugin.cloud_sync.auth.bearer"),
            AuthMode::Basic => self.i18n.t("plugin.cloud_sync.auth.basic"),
            AuthMode::None => self.i18n.t("plugin.cloud_sync.auth.none"),
        };
        self.render_cloud_sync_select_field(
            "plugin.cloud_sync.settings.auth_mode",
            CloudSyncSelect::AuthMode,
            current,
            cx,
        )
    }

    pub(super) fn render_cloud_sync_conflict_select(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = match self.cloud_sync.view.form.default_conflict_strategy {
            ConflictStrategy::Merge => self.i18n.t("plugin.cloud_sync.conflict.merge"),
            ConflictStrategy::Replace => self.i18n.t("plugin.cloud_sync.conflict.replace"),
            ConflictStrategy::Skip => self.i18n.t("plugin.cloud_sync.conflict.skip"),
            ConflictStrategy::Rename => self.i18n.t("plugin.cloud_sync.conflict.rename"),
        };
        self.render_cloud_sync_select_field(
            "plugin.cloud_sync.settings.default_conflict_strategy",
            CloudSyncSelect::ConflictStrategy,
            current,
            cx,
        )
    }

    pub(super) fn cloud_sync_select_options(
        &self,
        select: CloudSyncSelect,
    ) -> Vec<CloudSyncSelectOption> {
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync.view.form.backend_type.clone(),
            auth_mode: self.cloud_sync.view.form.auth_mode.clone(),
            default_conflict_strategy: self.cloud_sync.view.form.default_conflict_strategy.clone(),
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

    pub(super) fn cloud_sync_selected_option_index(&self, select: CloudSyncSelect) -> usize {
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync.view.form.backend_type.clone(),
            auth_mode: self.cloud_sync.view.form.auth_mode.clone(),
            default_conflict_strategy: self.cloud_sync.view.form.default_conflict_strategy.clone(),
            ..CloudSyncSettings::default()
        };
        cloud_sync_selected_option_spec_index(&settings, select)
    }

    pub(super) fn cloud_sync_focusable_selects(&self) -> Vec<CloudSyncSelect> {
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync.view.form.backend_type.clone(),
            auth_mode: self.cloud_sync.view.form.auth_mode.clone(),
            default_conflict_strategy: self.cloud_sync.view.form.default_conflict_strategy.clone(),
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

    pub(super) fn toggle_cloud_sync_select_from_pointer(
        &mut self,
        select: CloudSyncSelect,
        cx: &mut Context<Self>,
    ) {
        if self.cloud_sync.view.open_select == Some(select) {
            self.begin_cloud_sync_select_exit(cx);
            return;
        }
        let selected_index = self.cloud_sync_selected_option_index(select);
        browser_behavior::toggle_browser_highlighted_select_from_pointer(
            &mut self.cloud_sync.view.open_select,
            &mut self.cloud_sync.view.focused_select,
            &mut self.cloud_sync.view.select_focus_origin,
            &mut self.cloud_sync.view.select_highlighted,
            select,
            selected_index,
        );
        self.cloud_sync.view.rendered_select = Some(select);
    }

    pub(super) fn clear_cloud_sync_select_focus(&mut self) {
        browser_behavior::clear_browser_highlighted_select_focus(
            &mut self.cloud_sync.view.open_select,
            &mut self.cloud_sync.view.focused_select,
            &mut self.cloud_sync.view.select_focus_origin,
            &mut self.cloud_sync.view.select_highlighted,
        );
        self.cloud_sync.view.rendered_select = None;
    }

    pub(super) fn close_cloud_sync_select_for_scroll(&mut self) -> bool {
        let changed = close_cloud_sync_select_on_container_scroll(
            &mut self.cloud_sync.view.open_select,
            &mut self.cloud_sync.view.focused_select,
            &mut self.cloud_sync.view.select_highlighted,
        );
        if changed || self.cloud_sync.view.rendered_select.is_some() {
            // Scroll invalidates window-space trigger geometry, so it cannot animate safely.
            self.cloud_sync.view.rendered_select = None;
            return true;
        }
        false
    }

    pub(super) fn begin_cloud_sync_select_exit(&mut self, _cx: &mut Context<Self>) -> bool {
        if self.cloud_sync.view.open_select.is_none()
            && self.cloud_sync.view.rendered_select.is_none()
        {
            return false;
        }
        self.cloud_sync.view.open_select = None;
        self.cloud_sync.view.rendered_select = None;
        self.cloud_sync.view.select_highlighted = None;
        true
    }

    pub(super) fn apply_cloud_sync_select_action(
        &mut self,
        action: CloudSyncSelectAction,
        cx: &mut Context<Self>,
    ) {
        // Tauri's Radix Select uses the same onValueChange path for mouse and
        // keyboard selection. Keep native mutations centralized so Enter and
        // pointer clicks cannot drift apart.
        let trigger_select = match action {
            CloudSyncSelectAction::Backend(backend) => {
                self.cloud_sync.view.form.backend_type = backend.clone();
                if matches!(backend, BackendType::Dropbox) {
                    self.cloud_sync.view.form.auth_mode = AuthMode::Bearer;
                } else if matches!(
                    backend,
                    BackendType::GithubGist | BackendType::Git | BackendType::S3
                ) {
                    self.cloud_sync.view.form.auth_mode = AuthMode::None;
                }
                CloudSyncSelect::Backend
            }
            CloudSyncSelectAction::AuthMode(auth_mode) => {
                self.cloud_sync.view.form.auth_mode = auth_mode;
                CloudSyncSelect::AuthMode
            }
            CloudSyncSelectAction::ConflictStrategy(strategy) => {
                self.cloud_sync.view.form.default_conflict_strategy = strategy;
                CloudSyncSelect::ConflictStrategy
            }
        };
        self.begin_cloud_sync_select_exit(cx);
        self.cloud_sync.view.focused_select = Some(trigger_select);
        self.cloud_sync.view.select_highlighted = None;
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
                open_select: self.cloud_sync.view.open_select,
                focused_select: self.cloud_sync.view.focused_select,
                highlighted_option: self.cloud_sync.view.select_highlighted,
            },
            &self.cloud_sync_focusable_selects(),
            |select| self.cloud_sync_selected_option_index(select),
            |select| self.cloud_sync_select_options(select).len(),
        );
        let CloudSyncSelectKeyEffect::Handled {
            state,
            keyboard_focus_origin,
            selected_action_index,
        } = effect
        else {
            return false;
        };
        let previous_open_select = self.cloud_sync.view.open_select;
        self.cloud_sync.view.open_select = state.open_select;
        self.cloud_sync.view.focused_select = state.focused_select;
        self.cloud_sync.view.select_highlighted = state.highlighted_option;
        if keyboard_focus_origin {
            self.cloud_sync.view.select_focus_origin =
                Some(browser_behavior::BrowserFocusOrigin::Keyboard);
        }
        if previous_open_select.is_some()
            && state.open_select.is_none()
            && event.keystroke.key.as_str() == "escape"
        {
            // Escape is a manual dismissal and keeps the frozen trigger geometry.
            self.cloud_sync.view.open_select = previous_open_select;
            self.begin_cloud_sync_select_exit(cx);
        } else if previous_open_select.is_some() && state.open_select.is_none() {
            // Focus traversal is a programmatic close and must not retain stale geometry.
            self.cloud_sync.view.rendered_select = None;
        } else if let Some(select) = state.open_select {
            self.cloud_sync.view.rendered_select = Some(select);
        }
        if let (Some(select), Some(index)) =
            (self.cloud_sync.view.focused_select, selected_action_index)
        {
            if let Some(action) = self
                .cloud_sync_select_options(select)
                .get(index)
                .map(|option| option.action.clone())
            {
                self.apply_cloud_sync_select_action(action, cx);
            }
        }
        cx.notify();
        true
    }

    pub(super) fn render_cloud_sync_select_field(
        &self,
        label_key: &str,
        select: CloudSyncSelect,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let open = self.cloud_sync.view.open_select == Some(select);
        let focused = self.cloud_sync.view.focused_select == Some(select);
        let focus_visible = browser_behavior::browser_focus_visible(
            focused,
            self.cloud_sync.view.select_focus_origin,
        );
        let anchor_id = Self::cloud_sync_select_anchor_id(select);
        let workspace = cx.entity();
        let trigger =
            self.render_cloud_sync_select_trigger(select, value, open, focused, focus_visible, cx);
        cloud_sync_select_field(
            &self.tokens,
            self.render_selectable_text_scoped(
                "cloud-sync-select-label",
                label_key,
                self.i18n.t(label_key),
                theme.text_muted,
                cx,
            ),
            div()
                .relative()
                .w_full()
                .child(select_anchor_probe(
                    anchor_id,
                    trigger,
                    move |anchor, _window, cx| {
                        let _ = workspace.update(cx, |this, cx| {
                            this.update_select_anchor(anchor, cx);
                        });
                    },
                ))
                .into_any_element(),
            None,
        )
    }

    pub(super) fn render_cloud_sync_select_trigger(
        &self,
        select: CloudSyncSelect,
        value: String,
        open: bool,
        focused: bool,
        focus_visible: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        cloud_sync_select_trigger(
            &self.tokens,
            open,
            focused,
            focus_visible,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-select-value",
                format!("{select:?}"),
                value,
                theme.text,
                cx,
            ),
            cx.listener(
                move |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    this.toggle_cloud_sync_select_from_pointer(select, cx);
                    cx.stop_propagation();
                    cx.notify();
                },
            ),
        )
    }

    pub(super) fn render_cloud_sync_select_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let select = self.cloud_sync.view.rendered_select?;
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
            .view
            .select_highlighted
            .filter(|(highlighted_select, _)| *highlighted_select == select)
            .map(|(_, index)| index)
            .unwrap_or_else(|| self.cloud_sync_selected_option_index(select));
        let options = self.cloud_sync_select_options(select);
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
                if this.cloud_sync.view.select_highlighted != Some((select, index)) {
                    this.cloud_sync.view.select_highlighted = Some((select, index));
                    cx.notify();
                }
            }));
            popup = popup.child(select_option_action(
                option_el,
                false,
                false,
                cx.listener(move |this, _event, _window, cx| {
                    this.begin_cloud_sync_select_exit(cx);
                    this.cloud_sync.view.select_focus_origin =
                        Some(browser_behavior::BrowserFocusOrigin::Pointer);
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
                        this.begin_cloud_sync_select_exit(cx);
                        this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event, window, cx| {
                        this.begin_cloud_sync_select_exit(cx);
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

    pub(super) fn render_cloud_sync_toggle(
        &self,
        label_key: &str,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Toggle labels are control text, so they match Tauri select-none behavior.
        cloud_sync_toggle(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-toggle-label",
                label_key,
                self.i18n.t(label_key),
                theme.text_muted,
                cx,
            ),
            checked,
            listener,
        )
    }

    pub(super) fn render_cloud_sync_inline_button(
        &self,
        label_key: &str,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        // Cloud Sync inline actions are shadcn-style outline buttons in Tauri;
        // keep their chrome on the shared toolbar primitive instead of local
        // div/button styling.
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            cloud_sync_inline_button_options(&self.tokens),
            listener,
        )
        .into_any_element()
    }

    pub(super) fn save_cloud_sync_configuration(&mut self, cx: &mut Context<Self>) {
        self.persist_cloud_sync_configuration(true, cx);
    }

    pub(super) fn persist_cloud_sync_configuration(
        &mut self,
        show_success_toast: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        self.apply_focused_cloud_sync_input_draft();
        self.invalidate_cloud_sync_snapshot_caches();
        let (settings, interval) = cloud_sync_settings_from_form(&self.cloud_sync.view.form);
        let mut provider = CloudSyncKeychainSecretProvider::new(
            self.cloud_sync
                .controller
                .store
                .state()
                .secret_hints
                .clone(),
        );
        let secret_result =
            store_cloud_sync_touched_secrets(&self.cloud_sync.view.form, &mut provider);
        if let Err(error) = secret_result {
            self.cloud_sync.controller.store.state_mut().last_error = Some(error.to_string());
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.settings_saved_failed_title"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
            );
            return false;
        }
        self.cloud_sync.controller.store.state_mut().settings = settings;
        self.cloud_sync.controller.store.state_mut().secret_hints = provider.hints().clone();
        if let Err(error) = self.cloud_sync.controller.store.save() {
            self.cloud_sync.controller.store.state_mut().last_error = Some(error.to_string());
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.settings_saved_failed_title"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
            );
            return false;
        } else {
            normalize_cloud_sync_interval_draft(&mut self.cloud_sync.view.form, interval);
            reset_cloud_sync_secret_drafts(&mut self.cloud_sync.view.form);
            self.cloud_sync.controller.store.state_mut().last_error = None;
            if show_success_toast {
                self.push_cloud_sync_toast(
                    self.i18n.t("plugin.cloud_sync.toast.settings_saved_title"),
                    None,
                    TerminalNoticeVariant::Success,
                );
            }
            self.reschedule_cloud_sync_auto_upload(cx);
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        true
    }
}
