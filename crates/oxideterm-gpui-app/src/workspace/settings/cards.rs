use super::*;
pub(in crate::workspace) use oxideterm_gpui_settings_view::components::SettingsCalloutTone;

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_select_trigger(
        &self,
        select_id: SettingsSelect,
        value: String,
        placeholder: bool,
        disabled: bool,
    ) -> Div {
        self.settings_components().select_trigger(
            value,
            placeholder,
            disabled,
            self.open_settings_select == Some(select_id),
            self.settings_select_focus_origin,
        )
    }

    pub(in crate::workspace) fn settings_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        disabled: bool,
        width: Option<f32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_select_control_with_trigger_style(
            select_id,
            value,
            disabled,
            width,
            |trigger| trigger,
            cx,
        )
    }

    pub(in crate::workspace) fn settings_select_control_with_trigger_style(
        &self,
        select_id: SettingsSelect,
        value: String,
        disabled: bool,
        width: Option<f32>,
        trigger_style: impl FnOnce(Div) -> Div,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let trigger =
            trigger_style(self.settings_select_trigger(select_id, value, false, disabled));
        self.settings_components().select_control(
            select_id,
            disabled,
            width,
            trigger,
            Self::settings_window_dispatch(cx),
        )
    }

    pub(in crate::workspace) fn settings_components(
        &self,
    ) -> oxideterm_gpui_settings_view::components::SettingsComponents<'_> {
        oxideterm_gpui_settings_view::components::SettingsComponents {
            tokens: &self.tokens,
            i18n: &self.i18n,
            settings: self.settings_store.settings(),
            has_background: self.settings_background_active(),
        }
    }

    pub(in crate::workspace) fn settings_card_header_text(
        &self,
        title: String,
        description: Option<String>,
        icon: Option<AnyElement>,
        actions: Option<AnyElement>,
    ) -> AnyElement {
        self.settings_components()
            .settings_card_header_text(title, description, icon, actions)
    }

    pub(in crate::workspace) fn settings_card(
        &self,
        title_key: &str,
        description_key: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.settings_components()
            .settings_card(title_key, description_key, rows)
    }

    pub(in crate::workspace) fn settings_card_with_header(
        &self,
        title_key: &str,
        description_key: &str,
        icon: Option<AnyElement>,
        actions: Option<AnyElement>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.settings_components().settings_card_with_header(
            title_key,
            description_key,
            icon,
            actions,
            rows,
        )
    }

    pub(in crate::workspace) fn plain_settings_card(&self, rows: Vec<AnyElement>) -> AnyElement {
        self.settings_components().plain_settings_card(rows)
    }

    pub(in crate::workspace) fn settings_callout(
        &self,
        tone: SettingsCalloutTone,
        icon: Option<LucideIcon>,
        body: AnyElement,
    ) -> AnyElement {
        self.settings_components()
            .settings_callout(tone, icon.map(LucideIcon::path), body)
    }

    pub(in crate::workspace) fn card_separator(&self) -> AnyElement {
        self.settings_components().card_separator()
    }

    pub(in crate::workspace) fn settings_background_active(&self) -> bool {
        self.background_surface_active("settings")
    }

    pub(in crate::workspace) fn settings_panel_background(&self, color: u32) -> Rgba {
        self.settings_components().settings_panel_background(color)
    }

    pub(in crate::workspace) fn text_badge(&self, label: String, color: u32) -> AnyElement {
        self.settings_components().text_badge(label, color)
    }

    pub(in crate::workspace) fn standard_footer_action_button(
        &self,
        label: String,
        variant: ButtonVariant,
        action: ConfirmDialogAction,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        // Tauri DialogFooter buttons are normal shadcn Buttons, but their
        // focus-visible ring is owned by keyboard navigation rather than mouse
        // hover. Route activation through the workspace Button guard so
        // disabled/loading footers cannot dispatch while preserving that ring.
        self.workspace_confirm_footer_action_button(
            label,
            variant,
            action,
            disabled,
            self.standard_confirm_focus(),
            move |this, event, window, cx| {
                this.clear_standard_confirm_focus();
                listener(this, event, window, cx);
                cx.stop_propagation();
            },
            cx,
        )
    }

    pub(in crate::workspace) fn split_confirm_footer_button(
        &self,
        label: String,
        action: ConfirmDialogAction,
        destructive: bool,
        draw_right_separator: bool,
    ) -> Div {
        let text_color = if destructive {
            self.tokens.ui.error
        } else {
            self.tokens.ui.text_muted
        };
        let hover_bg = if destructive {
            rgba((self.tokens.ui.error << 8) | 0x1a)
        } else {
            rgba((self.tokens.ui.bg_hover << 8) | 0x80)
        };
        let hover_text = if destructive {
            self.tokens.ui.error
        } else {
            self.tokens.ui.text
        };

        // Some Tauri confirm dialogs use a split footer instead of shadcn
        // DialogFooter spacing. Use the shared split footer primitive so AI
        // and settings confirms share button focus-visible behavior.
        split_footer_button(
            &self.tokens,
            label,
            SplitFooterButtonOptions {
                text_color: rgb(text_color),
                hover_text_color: rgb(hover_text),
                hover_background: hover_bg,
                font_weight: if destructive {
                    gpui::FontWeight::SEMIBOLD
                } else {
                    gpui::FontWeight::MEDIUM
                },
                focus_visible: self.standard_confirm_focus() == Some(action),
                right_separator: draw_right_separator,
                separator_color: Some(rgba((self.tokens.ui.border << 8) | 0x66)),
                disabled: false,
                loading: false,
                height: None,
                padding_y: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_sm),
                edge: SplitFooterButtonEdge::None,
            },
        )
    }

    pub(in crate::workspace) fn split_confirm_footer_action_button(
        &self,
        label: String,
        action: ConfirmDialogAction,
        destructive: bool,
        draw_right_separator: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        // Split confirm footers are visually different from DialogFooter, but
        // Tauri still routes pointer activation through the same Radix action
        // lifecycle. Keep focus cleanup and event isolation shared with
        // standard_footer_action_button.
        self.split_confirm_footer_button(label, action, destructive, draw_right_separator)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| {
                    this.clear_standard_confirm_focus();
                    listener(this, event, window, cx);
                    cx.stop_propagation();
                }),
            )
    }

    pub(in crate::workspace) fn update_settings_slider_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(slider) = self.settings_workspace.read(cx).slider_drag() {
            self.apply_settings_slider_from_position(slider, f32::from(event.position.x), cx);
        }
    }

    pub(in crate::workspace) fn apply_settings_slider_from_position(
        &mut self,
        slider: SettingsSlider,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_settings_view::slider::SliderEdit;
        let Some(anchor) = self
            .select_anchors
            .get(&settings_slider_anchor_id(slider))
            .copied()
        else {
            return;
        };
        let edit = self.settings_workspace.update(cx, |entity, cx| {
            entity.update_slider(
                slider,
                x,
                anchor,
                self.settings_store.settings(),
                &self.tokens,
                cx,
            )
        });
        match edit {
            Some(SliderEdit::Settings(edit)) => self.edit_settings(edit, cx),
            Some(SliderEdit::WindowOpacity(value)) => {
                self.edit_settings(|settings| settings.appearance.window_opacity = value, cx);
                // Detached windows also consume this setting.
                cx.refresh_windows();
            }
            Some(SliderEdit::Background(edit)) => self.edit_background_style(edit, cx),
            None => {}
        }
    }

    pub(in crate::workspace) fn finish_settings_slider_drag(&mut self, cx: &mut Context<Self>) {
        if self
            .settings_workspace
            .update(cx, |settings, cx| settings.finish_slider_drag(cx))
        {
            cx.notify();
        }
    }

    pub(in crate::workspace) fn close_settings_select(&mut self) {
        browser_behavior::close_browser_trigger_select(
            &mut self.open_settings_select,
            &mut self.settings_select_focus_origin,
        );
        self.open_settings_select_owner_window_id = None;
    }

    pub(in crate::workspace) fn release_settings_select_window(
        &mut self,
        window_id: gpui::WindowId,
    ) -> bool {
        self.settings_select_anchors
            .retain(|(anchor_window_id, _), _| *anchor_window_id != window_id);
        if self.open_settings_select_owner_window_id == Some(window_id) {
            self.close_settings_select();
            return true;
        }
        false
    }

    pub(in crate::workspace) fn apply_settings_input_draft(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) {
        if let SettingsInput::LocalShellPath(index) = input {
            let shells = self.effective_local_shells_for_settings(self.settings_store.settings());
            if let Some(shell) = shells.get(index) {
                let draft = self.settings_input_draft.trim();
                if draft.is_empty() {
                    self.set_local_shell_path(&shell.id, None, cx);
                    if !self.local_shells.iter().any(|known| known.id == shell.id) {
                        // Removing an undiscovered shell also removes its indexed input.
                        self.focused_settings_input = None;
                        self.clear_settings_input_draft(input);
                        self.clear_ime_selection();
                    }
                } else if expand_local_terminal_cwd(draft).is_absolute() {
                    self.set_local_shell_path(&shell.id, Some(draft.to_string()), cx);
                }
            }
            cx.notify();
            return;
        }
        use oxideterm_gpui_settings_view::input::SettingsDraftEdit;
        let edit = self.settings_workspace.read(cx).prepare_draft_edit(
            self.settings_store.settings(),
            input,
            &self.settings_input_draft,
        );
        match edit {
            SettingsDraftEdit::Background(background) => {
                self.edit_background_style(move |style| *style = background, cx);
                return;
            }
            SettingsDraftEdit::Settings(next) => {
                self.edit_settings(move |settings| *settings = next, cx);
                return;
            }
            SettingsDraftEdit::Invalid => {
                self.settings_views.surface.update(cx, |_, cx| cx.notify());
                return;
            }
            SettingsDraftEdit::Unhandled => {}
        }

        if ai_state::AiWorkspaceEntity::owns_settings_input(input) {
            // Entity-owned inputs are updated directly by the IME adapter and
            // must not be copied into the legacy settings page model.
            cx.notify();
            return;
        }
        let cloud_sync_input = {
            let cloud_sync = self.cloud_sync.read(cx);
            cloud_sync_form_input_value_ref(&cloud_sync.view.form, input).is_some()
        };
        if cloud_sync_input {
            // The root draft remains the only owner while the IME is focused;
            // persistence moves it back through `apply_focused_*`.
            cx.notify();
            return;
        }
        match input {
            SettingsInput::PublicMcpPort => {
                self.public_mcp
                    .set_port_draft(self.settings_input_draft.clone());
                cx.notify();
            }
            SettingsInput::NativePluginInstalledPageSize
            | SettingsInput::NativePluginMarketplacePageSize
            | SettingsInput::NativePluginInstalledPageJump
            | SettingsInput::NativePluginMarketplacePageJump => {
                // Commit on Enter or Apply so typing does not move the plugin list.
                cx.notify();
            }
            SettingsInput::NativePluginInstallUrl => {
                let draft = self.settings_input_draft.trim().to_string();
                self.update_plugin_manager_state(cx, |manager| {
                    manager.install_url_draft = draft;
                });
                cx.notify();
            }
            SettingsInput::NativePluginInstallChecksum => {
                let draft = self.settings_input_draft.trim().to_string();
                self.update_plugin_manager_state(cx, |manager| {
                    manager.install_checksum_draft = draft;
                });
                cx.notify();
            }
            SettingsInput::NativePluginRegistryUrl => {
                let draft = self.settings_input_draft.trim().to_string();
                self.update_plugin_manager_state(cx, |manager| {
                    manager.registry_url_draft = draft;
                });
                cx.notify();
            }
            SettingsInput::NativePluginMarketplaceSearch => {
                let draft = self.settings_input_draft.trim().to_string();
                self.update_plugin_manager_state(cx, |manager| {
                    manager.marketplace_search_draft = draft;
                });
                cx.notify();
            }
            SettingsInput::PortableCurrentPassword
            | SettingsInput::PortableNewPassword
            | SettingsInput::PortableConfirmPassword => {}
            SettingsInput::AppLockCurrentPassword
            | SettingsInput::AppLockNewPassword
            | SettingsInput::AppLockConfirmPassword => {}
            SettingsInput::ManagedKeyFilePath
            | SettingsInput::ManagedKeyFileName
            | SettingsInput::ManagedKeyFilePassphrase
            | SettingsInput::ManagedKeyPasteName
            | SettingsInput::ManagedKeyPastePrivateKey
            | SettingsInput::ManagedKeyPastePassphrase
            | SettingsInput::ManagedKeyRenameName
            | SettingsInput::ConnectionImportTargetGroup => {}
            SettingsInput::LocalPrivilegeLabel
            | SettingsInput::TotpName
            | SettingsInput::TotpSecret
            | SettingsInput::TotpPattern
            | SettingsInput::TotpPeriod
            | SettingsInput::LocalPrivilegeUsernameHint
            | SettingsInput::LocalPrivilegeSecret
            | SettingsInput::LocalPrivilegePromptPatterns
            | SettingsInput::LocalPrivilegeHostPatterns
            | SettingsInput::LocalPrivilegeSecondFactorPatterns
            | SettingsInput::LocalPrivilegeSecondFactorUserHint => {}
            SettingsInput::NetworkProxyPassword
            | SettingsInput::NetworkProxyTestHost
            | SettingsInput::NetworkProxyTestPort => {}
            SettingsInput::PluginSetting(index) => {
                let Some(setting) = self
                    .plugin_entity
                    .read(cx)
                    .registry()
                    .contributions()
                    .settings
                    .get(index)
                    .cloned()
                else {
                    cx.notify();
                    return;
                };
                let value = match plugin_setting_draft_to_value(
                    &setting.definition.setting_type,
                    &self.settings_input_draft,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.plugin_entity.update(cx, |plugins, _cx| {
                            plugins
                                .registry_mut()
                                .record_manager_error(setting.plugin_id.clone(), error);
                        });
                        cx.notify();
                        return;
                    }
                };
                if let Err(error) = self.set_native_plugin_setting_value_and_emit(
                    &setting.plugin_id,
                    &setting.definition.id,
                    value,
                    cx,
                ) {
                    self.plugin_entity.update(cx, |plugins, _cx| {
                        plugins
                            .registry_mut()
                            .record_manager_error(setting.plugin_id.clone(), error);
                    });
                }
                cx.notify();
            }
            _ => {
                cx.notify();
            }
        }
    }
}
