impl WorkspaceApp {
    fn settings_card(
        &self,
        title_key: &str,
        _description_key: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(
                div()
                    .mb(px(self.tokens.metrics.settings_card_title_nudge_y))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(title_key).to_uppercase()),
            )
            .children(rows)
            .into_any_element()
    }

    fn plain_settings_card(&self, rows: Vec<AnyElement>) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .children(rows)
            .into_any_element()
    }

    fn terminal_input_settings_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(16.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(
                        self.i18n
                            .t("settings_view.terminal.input_safety")
                            .to_uppercase(),
                    ),
            )
            .child(self.checkbox_row(
                "settings_view.terminal.paste_protection",
                "settings_view.terminal.paste_protection_hint",
                settings.terminal.paste_protection,
                set_paste_protection,
                cx,
            ))
            .child(self.settings_row_with_margin(
                self.checkbox_row(
                    "settings_view.terminal.osc52_clipboard",
                    "settings_view.terminal.osc52_clipboard_hint",
                    settings.terminal.osc52_clipboard,
                    set_osc52_clipboard,
                    cx,
                ),
                16.0,
            ));

        if !cfg!(target_os = "macos") {
            rows = rows.child(self.settings_row_with_margin(
                self.checkbox_row(
                    "settings_view.terminal.smart_copy",
                    "settings_view.terminal.smart_copy_hint",
                    settings.terminal.smart_copy,
                    set_smart_copy,
                    cx,
                ),
                16.0,
            ));
        }

        rows.child(self.settings_row_with_margin(
            self.checkbox_row(
                "settings_view.terminal.copy_on_select",
                "settings_view.terminal.copy_on_select_hint",
                settings.terminal.copy_on_select,
                set_copy_on_select,
                cx,
            ),
            16.0,
        ))
        .child(self.settings_row_with_margin(
            self.checkbox_row(
                "settings_view.terminal.middle_click_paste",
                "settings_view.terminal.middle_click_paste_hint",
                settings.terminal.middle_click_paste,
                set_middle_click_paste,
                cx,
            ),
            16.0,
        ))
        .child(self.settings_row_with_margin(
            self.checkbox_row(
                "settings_view.terminal.selection_requires_shift",
                "settings_view.terminal.selection_requires_shift_hint",
                settings.terminal.selection_requires_shift,
                set_selection_requires_shift,
                cx,
            ),
            16.0,
        ))
        .child(
            div()
                .my(px(20.0))
                .h(px(1.0))
                .w_full()
                .bg(rgba((self.tokens.ui.border << 8) | 0x80)),
        )
        .child(self.checkbox_row(
            "settings_view.terminal.autosuggest_local_history",
            "settings_view.terminal.autosuggest_local_history_hint",
            settings.terminal.autosuggest.local_shell_history,
            set_autosuggest_local_history,
            cx,
        ))
        .into_any_element()
    }

    fn settings_row_with_margin(&self, row: AnyElement, margin_top: f32) -> AnyElement {
        div().mt(px(margin_top)).child(row).into_any_element()
    }

    fn card_title(&self, title_key: &str) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(title_key).to_uppercase())
            .into_any_element()
    }

    fn card_separator(&self) -> AnyElement {
        div()
            .h(px(1.0))
            .w_full()
            .bg(rgba((self.tokens.ui.border << 8) | 0x80))
            .into_any_element()
    }

    fn settings_background_active(&self) -> bool {
        self.terminal_background_preferences("settings").is_some()
    }

    fn settings_panel_background(&self, color: u32) -> Rgba {
        if self.settings_background_active() {
            rgba((color << 8) | alpha_byte(self.tokens.metrics.panel_vibrancy_alpha))
        } else {
            rgb(color)
        }
    }

    fn text_badge(&self, label: String, color: u32) -> AnyElement {
        div()
            .px(px(8.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((color << 8) | 0x1a))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(color))
            .child(label)
            .into_any_element()
    }

    fn outline_button(&self, label: String, size: ButtonSize) -> AnyElement {
        button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant: ButtonVariant::Outline,
                size,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .into_any_element()
    }

    fn terminal_page_switcher(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut tabs = div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(self.settings_panel_background(theme.bg_card))
            .p(px(8.0));

        for page in TerminalSettingsPage::all() {
            let page_id = *page;
            let active = self.terminal_settings_page == page_id;
            let item = div()
                .rounded(px(self.tokens.radii.md))
                .px(px(12.0))
                .py(px(6.0))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                })
                .bg(if active {
                    rgba((theme.accent << 8) | 0x26)
                } else {
                    rgba(0x00000000)
                })
                .cursor_pointer()
                .hover(move |style| {
                    if active {
                        style
                    } else {
                        style.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
                    }
                })
                .child(self.i18n.t(page_id.label_key()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.terminal_settings_page = page_id;
                        cx.notify();
                    }),
                );
            tabs = tabs.child(item);
        }

        tabs.into_any_element()
    }

    fn value_row(&self, label_key: &str, hint_key: &str, value: String) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            select_trigger(&self.tokens, value, false, false)
                .w(px(self.tokens.metrics.settings_select_width))
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn update_select_anchor(
        &mut self,
        anchor: OverlayAnchor,
        cx: &mut Context<Self>,
    ) {
        if self.select_anchors.get(&anchor.id) != Some(&anchor) {
            let should_notify = self
                .open_settings_select
                .is_some_and(|select| select.anchor_id() == anchor.id)
                || (self.open_new_connection_select == Some(NewConnectionSelect::Group)
                    && anchor.id == SelectAnchorId::NewConnectionGroup)
                || (matches!(
                    anchor.id,
                    SelectAnchorId::AiPanelRoot
                        | SelectAnchorId::AiConversationList
                        | SelectAnchorId::AiChatMenu
                        | SelectAnchorId::AiModelSelector
                        | SelectAnchorId::AiProfileSelector
                        | SelectAnchorId::AiSafetyMenu
                        | SelectAnchorId::AiContextPopover
                ) && self.has_ai_sidebar_floating_overlay())
                || self
                    .settings_slider_drag
                    .is_some_and(|slider| settings_slider_anchor_id(slider) == anchor.id);
            self.select_anchors.insert(anchor.id, anchor);
            if should_notify {
                cx.notify();
            }
        }
    }

    pub(super) fn handle_settings_input_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.focused_settings_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;

        match key {
            "escape" => {
                self.focused_settings_input = None;
                self.clear_settings_input_draft(input);
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "enter" => {
                if input == SettingsInput::ConnectionNewGroup {
                    if self.create_settings_connection_group(cx) {
                        self.focused_settings_input = None;
                    }
                    self.new_connection_caret_visible = true;
                    cx.notify();
                    return true;
                }
                if settings_input_accepts_newline(input) {
                    self.settings_input_draft.push('\n');
                    self.apply_settings_input_draft(input, cx);
                    return true;
                }
                self.focused_settings_input = None;
                self.clear_settings_input_draft(input);
                self.new_connection_caret_visible = true;
                cx.notify();
                true
            }
            "backspace" if !modifiers.platform && !modifiers.control => {
                self.settings_input_draft.pop();
                self.apply_settings_input_draft(input, cx);
                true
            }
            _ => true,
        }
    }

    pub(super) fn blur_text_inputs(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        if let Some(input) = self.focused_settings_input.take() {
            self.clear_settings_input_draft(input);
            self.ime_marked_text = None;
            changed = true;
        }
        if self.open_settings_select.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.open_new_connection_select.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.terminal_command_bar_focused {
            self.terminal_command_bar_focused = false;
            self.ime_marked_text = None;
            changed = true;
        }
        if let Some(player) = self.terminal_cast_player.as_mut()
            && player.search_focused
        {
            player.search_focused = false;
            self.ime_marked_text = None;
            changed = true;
        }
        if self.terminal_quick_commands_open || self.terminal_quick_command_pending.is_some() {
            self.terminal_quick_commands_open = false;
            self.terminal_quick_command_pending = None;
            changed = true;
        }
        if self.session_manager.focused_input.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.launcher.focused_input.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if self.graphics.focused_input.take().is_some() {
            self.ime_marked_text = None;
            changed = true;
        }
        if let Some(form) = self.new_connection_form.as_mut()
            && form.field_focused
        {
            form.field_focused = false;
            form.selected_field = None;
            self.ime_marked_text = None;
            changed = true;
        }
        if changed {
            self.new_connection_caret_visible = true;
            cx.notify();
        }
    }

    pub(super) fn update_settings_slider_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(slider) = self.settings_slider_drag {
            self.apply_settings_slider_from_position(slider, f32::from(event.position.x), cx);
        }
    }

    fn apply_settings_slider_from_position(
        &mut self,
        slider: SettingsSlider,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        match slider {
            SettingsSlider::TerminalFontSize => {
                self.set_font_size_from_position(x, cx);
            }
            SettingsSlider::AppearanceBorderRadius => {
                self.set_settings_slider_from_position(
                    SelectAnchorId::SettingsAppearanceBorderRadiusSlider,
                    x,
                    0.0,
                    24.0,
                    |settings, value| settings.appearance.border_radius = value.round() as i64,
                    cx,
                );
            }
            SettingsSlider::AppearanceBackgroundOpacity => {
                self.set_settings_slider_from_position(
                    SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider,
                    x,
                    3.0,
                    50.0,
                    |settings, value| {
                        settings.terminal.background_opacity = value.round() as f64 / 100.0
                    },
                    cx,
                );
            }
            SettingsSlider::AppearanceBackgroundBlur => {
                self.set_background_blur_preview_from_position(x, cx);
            }
        }
    }

    pub(super) fn finish_settings_slider_drag(&mut self, cx: &mut Context<Self>) {
        if self.settings_slider_drag.take().is_some() {
            cx.notify();
        }
    }

    fn focus_settings_input(
        &mut self,
        input: SettingsInput,
        current_value: String,
        cx: &mut Context<Self>,
    ) {
        self.open_settings_select = None;
        self.focused_settings_input = Some(input);
        self.settings_input_draft = current_value;
        self.new_connection_caret_visible = true;
        cx.notify();
    }

    fn clear_settings_input_draft(&mut self, input: SettingsInput) {
        if settings_input_is_secret(input) {
            zeroize::Zeroize::zeroize(&mut self.settings_input_draft);
        }
        self.settings_input_draft.clear();
    }

    fn current_settings_input_value(&self, input: SettingsInput) -> String {
        let settings = self.settings_store.settings();
        match input {
            SettingsInput::TerminalFontSize => settings.terminal.font_size.to_string(),
            SettingsInput::TerminalLineHeight => compact_decimal(settings.terminal.line_height),
            SettingsInput::IdeFontSize => settings
                .ide
                .font_size
                .map(|value| value.to_string())
                .unwrap_or_default(),
            SettingsInput::IdeLineHeight => settings
                .ide
                .line_height
                .map(compact_decimal)
                .unwrap_or_default(),
            SettingsInput::AppearanceUiFont => settings.appearance.ui_font_family.clone(),
            SettingsInput::LocalDefaultCwd => settings
                .local_terminal
                .default_cwd
                .clone()
                .unwrap_or_default(),
            SettingsInput::LocalGitBashPath => settings
                .local_terminal
                .git_bash_path
                .clone()
                .unwrap_or_default(),
            SettingsInput::LocalOhMyPoshTheme => settings
                .local_terminal
                .oh_my_posh_theme
                .clone()
                .unwrap_or_default(),
            SettingsInput::ConnectionDefaultUsername => {
                settings.connection_defaults.username.clone()
            }
            SettingsInput::ConnectionDefaultPort => settings.connection_defaults.port.to_string(),
            SettingsInput::ConnectionNewGroup => self.settings_connection_new_group.clone(),
            SettingsInput::SftpSpeedLimitKbps => settings.sftp.speed_limit_kbps.to_string(),
            SettingsInput::InBandTransferMaxChunkBytes => {
                settings.terminal.in_band_transfer.max_chunk_bytes.to_string()
            }
            SettingsInput::InBandTransferMaxFileCount => {
                settings.terminal.in_band_transfer.max_file_count.to_string()
            }
            SettingsInput::InBandTransferMaxTotalBytes => settings
                .terminal
                .in_band_transfer
                .max_total_bytes
                .to_string(),
            SettingsInput::TerminalCommandBarFocusHandoff => settings
                .terminal
                .command_bar
                .focus_handoff_commands
                .join("\n"),
            SettingsInput::KeybindingSearch => self.keybinding_search_query.clone(),
            SettingsInput::CustomThemeName => self
                .theme_editor
                .as_ref()
                .map(|editor| editor.name.clone())
                .unwrap_or_default(),
            SettingsInput::CustomThemeTerminalColor(index) => self
                .theme_editor
                .as_ref()
                .and_then(|editor| editor.terminal_colors.get(index).cloned())
                .unwrap_or_default(),
            SettingsInput::CustomThemeUiColor(index) => self
                .theme_editor
                .as_ref()
                .and_then(|editor| editor.ui_colors.get(index).cloned())
                .unwrap_or_default(),
            SettingsInput::HighlightLabel(index) => settings
                .terminal
                .highlight_rules
                .get(index)
                .map(|rule| rule.label.clone())
                .unwrap_or_default(),
            SettingsInput::HighlightPattern(index) => settings
                .terminal
                .highlight_rules
                .get(index)
                .map(|rule| rule.pattern.clone())
                .unwrap_or_default(),
            SettingsInput::HighlightForeground(index) => settings
                .terminal
                .highlight_rules
                .get(index)
                .and_then(|rule| rule.foreground.clone())
                .unwrap_or_default(),
            SettingsInput::HighlightBackground(index) => settings
                .terminal
                .highlight_rules
                .get(index)
                .and_then(|rule| rule.background.clone())
                .unwrap_or_default(),
            SettingsInput::AiProviderName(index) => settings
                .ai
                .providers
                .get(index)
                .and_then(|provider| ai_provider_string(provider, "name"))
                .unwrap_or_default(),
            SettingsInput::AiProviderBaseUrl(index) => settings
                .ai
                .providers
                .get(index)
                .and_then(|provider| ai_provider_string(provider, "baseUrl"))
                .unwrap_or_default(),
            SettingsInput::AiProviderDefaultModel(index) => settings
                .ai
                .providers
                .get(index)
                .and_then(|provider| ai_provider_string(provider, "defaultModel"))
                .unwrap_or_default(),
            SettingsInput::AiProviderApiKey(_) => String::new(),
            SettingsInput::AiProfileName(index) => settings
                .ai
                .execution_profiles
                .get("profiles")
                .and_then(|profiles| profiles.as_array())
                .and_then(|profiles| profiles.get(index))
                .and_then(|profile| profile.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            SettingsInput::AiProfileModel(index) => settings
                .ai
                .execution_profiles
                .get("profiles")
                .and_then(|profiles| profiles.as_array())
                .and_then(|profiles| profiles.get(index))
                .and_then(|profile| profile.get("model"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            SettingsInput::AiSystemPrompt => settings.ai.custom_system_prompt.clone(),
            SettingsInput::AiMemoryContent => settings.ai.memory.content.clone(),
            SettingsInput::AiModelContextWindow(provider_index, model_index) => settings
                .ai
                .providers
                .get(provider_index)
                .and_then(ai_provider_id)
                .and_then(|provider_id| {
                    let model = settings
                        .ai
                        .providers
                        .get(provider_index)
                        .and_then(|provider| provider.get("models"))
                        .and_then(serde_json::Value::as_array)
                        .and_then(|models| models.get(model_index))
                        .and_then(serde_json::Value::as_str)?;
                    settings
                        .ai
                        .user_context_windows
                        .get(&provider_id)
                        .and_then(|windows| windows.get(model))
                        .and_then(serde_json::Value::as_i64)
                        .or_else(|| {
                            Some(
                                ai_model_context_window_info(
                                    model,
                                    &settings.ai.model_context_windows,
                                    Some(&provider_id),
                                    &settings.ai.user_context_windows,
                                )
                                .value,
                            )
                        })
                        .map(|value| value.to_string())
                })
                .unwrap_or_default(),
            SettingsInput::AiActiveModelMaxResponseTokens => settings
                .ai
                .active_provider_id
                .as_ref()
                .zip(settings.ai.active_model.as_ref())
                .and_then(|(provider_id, model)| {
                    settings
                        .ai
                        .model_max_response_tokens
                        .get(provider_id)
                        .and_then(|models| models.get(model))
                        .and_then(serde_json::Value::as_i64)
                })
                .map(|value| value.to_string())
                .unwrap_or_default(),
            SettingsInput::AiEmbeddingModel => settings
                .ai
                .embedding_config
                .as_ref()
                .and_then(|config| config.get("model"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            SettingsInput::AiMcpName => self
                .ai_mcp_add_dialog
                .as_ref()
                .map(|draft| draft.name.clone())
                .unwrap_or_default(),
            SettingsInput::AiMcpCommand => self
                .ai_mcp_add_dialog
                .as_ref()
                .map(|draft| draft.command.clone())
                .unwrap_or_default(),
            SettingsInput::AiMcpArgs => self
                .ai_mcp_add_dialog
                .as_ref()
                .map(|draft| draft.args.clone())
                .unwrap_or_default(),
            SettingsInput::AiMcpUrl => self
                .ai_mcp_add_dialog
                .as_ref()
                .map(|draft| draft.url.clone())
                .unwrap_or_default(),
            SettingsInput::AiMcpAuthHeaderName => self
                .ai_mcp_add_dialog
                .as_ref()
                .map(|draft| draft.auth_header_name.clone())
                .unwrap_or_default(),
            SettingsInput::AiMcpAuthToken => self
                .ai_mcp_add_dialog
                .as_ref()
                .map(|draft| draft.auth_token.clone())
                .unwrap_or_default(),
            SettingsInput::AiMcpEnvKey(index) => self
                .ai_mcp_add_dialog
                .as_ref()
                .and_then(|draft| draft.env.get(index))
                .map(|(key, _)| key.clone())
                .unwrap_or_default(),
            SettingsInput::AiMcpEnvValue(index) => self
                .ai_mcp_add_dialog
                .as_ref()
                .and_then(|draft| draft.env.get(index))
                .map(|(_, value)| value.clone())
                .unwrap_or_default(),
            SettingsInput::AiMcpHeaderKey(index) => self
                .ai_mcp_add_dialog
                .as_ref()
                .and_then(|draft| draft.headers.get(index))
                .map(|(key, _)| key.clone())
                .unwrap_or_default(),
            SettingsInput::AiMcpHeaderValue(index) => self
                .ai_mcp_add_dialog
                .as_ref()
                .and_then(|draft| draft.headers.get(index))
                .map(|(_, value)| value.clone())
                .unwrap_or_default(),
            SettingsInput::KnowledgeCollectionName => self.knowledge_new_collection_name.clone(),
            SettingsInput::KnowledgeDocumentTitle => self.knowledge_new_document_title.clone(),
        }
    }

    pub(super) fn apply_settings_input_draft(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) {
        match input {
            SettingsInput::TerminalFontSize => {
                if let Ok(value) = self.settings_input_draft.parse::<i64>() {
                    self.edit_settings(
                        |settings| settings.terminal.font_size = value.clamp(8, 32),
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::TerminalLineHeight => {
                if let Ok(value) = self.settings_input_draft.parse::<f64>() {
                    self.edit_settings(
                        |settings| settings.terminal.line_height = value.clamp(0.8, 2.0),
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::IdeFontSize => {
                let value = self.settings_input_draft.trim();
                if value.is_empty() {
                    self.edit_settings(|settings| settings.ide.font_size = None, cx);
                } else if let Ok(value) = value.parse::<i64>() {
                    self.edit_settings(|settings| settings.ide.font_size = Some(value.clamp(8, 32)), cx);
                } else {
                    cx.notify();
                }
            }
            SettingsInput::IdeLineHeight => {
                let value = self.settings_input_draft.trim();
                if value.is_empty() {
                    self.edit_settings(|settings| settings.ide.line_height = None, cx);
                } else if let Ok(value) = value.parse::<f64>() {
                    self.edit_settings(
                        |settings| settings.ide.line_height = Some(value.clamp(0.8, 3.0)),
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::AppearanceUiFont => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(|settings| settings.appearance.ui_font_family = value, cx);
            }
            SettingsInput::LocalDefaultCwd => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    |settings| {
                        settings.local_terminal.default_cwd =
                            (!value.is_empty()).then(|| value.clone());
                    },
                    cx,
                );
            }
            SettingsInput::LocalGitBashPath => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    |settings| {
                        settings.local_terminal.git_bash_path =
                            (!value.is_empty()).then(|| value.clone());
                    },
                    cx,
                );
            }
            SettingsInput::LocalOhMyPoshTheme => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    |settings| {
                        settings.local_terminal.oh_my_posh_theme =
                            (!value.is_empty()).then(|| value.clone());
                    },
                    cx,
                );
            }
            SettingsInput::ConnectionDefaultUsername => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(|settings| settings.connection_defaults.username = value, cx);
            }
            SettingsInput::ConnectionDefaultPort => {
                if let Ok(value) = self.settings_input_draft.parse::<i64>() {
                    self.edit_settings(
                        |settings| settings.connection_defaults.port = value.clamp(1, 65535),
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::ConnectionNewGroup => {
                self.settings_connection_new_group = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::SftpSpeedLimitKbps => {
                if let Ok(value) = self.settings_input_draft.parse::<i64>() {
                    self.edit_settings(
                        |settings| settings.sftp.speed_limit_kbps = value.max(0),
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::InBandTransferMaxChunkBytes => {
                if let Ok(value) = self.settings_input_draft.parse::<i64>() {
                    self.edit_settings(
                        |settings| {
                            settings.terminal.in_band_transfer.max_chunk_bytes = value.max(1024)
                        },
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::InBandTransferMaxFileCount => {
                if let Ok(value) = self.settings_input_draft.parse::<i64>() {
                    self.edit_settings(
                        |settings| settings.terminal.in_band_transfer.max_file_count = value.max(1),
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::InBandTransferMaxTotalBytes => {
                if let Ok(value) = self.settings_input_draft.parse::<i64>() {
                    self.edit_settings(
                        |settings| {
                            settings.terminal.in_band_transfer.max_total_bytes = value.max(1024)
                        },
                        cx,
                    );
                } else {
                    cx.notify();
                }
            }
            SettingsInput::TerminalCommandBarFocusHandoff => {
                let commands = parse_focus_handoff_command_list(&self.settings_input_draft);
                self.edit_settings(
                    move |settings| settings.terminal.command_bar.focus_handoff_commands = commands,
                    cx,
                );
            }
            SettingsInput::KeybindingSearch => {
                self.keybinding_search_query = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::CustomThemeName => {
                if let Some(editor) = self.theme_editor.as_mut() {
                    editor.name = self.settings_input_draft.clone();
                }
                cx.notify();
            }
            SettingsInput::CustomThemeTerminalColor(index) => {
                self.apply_theme_editor_color(ThemeEditorSection::Terminal, index, cx);
            }
            SettingsInput::CustomThemeUiColor(index) => {
                self.apply_theme_editor_color(ThemeEditorSection::Ui, index, cx);
            }
            SettingsInput::HighlightLabel(index) => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_highlight_rule(index, move |rule| rule.label = value.clone(), cx);
            }
            SettingsInput::HighlightPattern(index) => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_highlight_rule(index, move |rule| rule.pattern = value.clone(), cx);
            }
            SettingsInput::HighlightForeground(index) => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_highlight_rule(
                    index,
                    move |rule| {
                        rule.foreground = (!value.is_empty()).then(|| value.clone());
                    },
                    cx,
                );
            }
            SettingsInput::HighlightBackground(index) => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_highlight_rule(
                    index,
                    move |rule| {
                        rule.background = (!value.is_empty()).then(|| value.clone());
                    },
                    cx,
                );
            }
            SettingsInput::AiProviderName(index) => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    |settings| {
                        ai_update_provider(settings, index, |provider| {
                            provider.insert("name".to_string(), serde_json::json!(value.clone()));
                        });
                    },
                    cx,
                );
            }
            SettingsInput::AiProviderBaseUrl(index) => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    |settings| {
                        ai_update_provider(settings, index, |provider| {
                            provider.insert("baseUrl".to_string(), serde_json::json!(value.clone()));
                        });
                    },
                    cx,
                );
            }
            SettingsInput::AiProviderDefaultModel(index) => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    |settings| {
                        ai_update_provider(settings, index, |provider| {
                            provider.insert(
                                "defaultModel".to_string(),
                                serde_json::json!(value.clone()),
                            );
                        });
                    },
                    cx,
                );
            }
            SettingsInput::AiProviderApiKey(_) => {
                cx.notify();
            }
            SettingsInput::AiProfileName(index) => {
                let value = self.settings_input_draft.clone();
                self.edit_settings(
                    move |settings| {
                        ai_patch_execution_profile(settings, index, |profile| {
                            profile.insert("name".to_string(), serde_json::json!(value.clone()));
                            profile.insert(
                                "updatedAt".to_string(),
                                serde_json::json!(current_time_millis()),
                            );
                        });
                    },
                    cx,
                );
            }
            SettingsInput::AiProfileModel(index) => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    move |settings| {
                        ai_patch_execution_profile(settings, index, |profile| {
                            profile.insert(
                                "model".to_string(),
                                if value.is_empty() {
                                    serde_json::Value::Null
                                } else {
                                    serde_json::json!(value.clone())
                                },
                            );
                            profile.insert(
                                "updatedAt".to_string(),
                                serde_json::json!(current_time_millis()),
                            );
                        });
                    },
                    cx,
                );
            }
            SettingsInput::AiSystemPrompt => {
                let value = self.settings_input_draft.clone();
                self.edit_settings(|settings| settings.ai.custom_system_prompt = value.clone(), cx);
            }
            SettingsInput::AiMemoryContent => {
                let value = self.settings_input_draft.clone();
                self.edit_settings(|settings| settings.ai.memory.content = value.clone(), cx);
            }
            SettingsInput::AiModelContextWindow(provider_index, model_index) => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    move |settings| {
                        let Some(provider_id) = settings
                            .ai
                            .providers
                            .get(provider_index)
                            .and_then(ai_provider_id)
                        else {
                            return;
                        };
                        let Some(model) = settings
                            .ai
                            .providers
                            .get(provider_index)
                            .and_then(|provider| provider.get("models"))
                            .and_then(serde_json::Value::as_array)
                            .and_then(|models| models.get(model_index))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                        else {
                            return;
                        };
                        set_ai_user_context_window(settings, &provider_id, &model, value.parse().ok());
                    },
                    cx,
                );
            }
            SettingsInput::AiActiveModelMaxResponseTokens => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    move |settings| {
                        let Some(provider_id) = settings.ai.active_provider_id.clone() else {
                            return;
                        };
                        let Some(model) = settings.ai.active_model.clone() else {
                            return;
                        };
                        set_ai_model_max_response_tokens(settings, &provider_id, &model, value.parse().ok());
                    },
                    cx,
                );
            }
            SettingsInput::AiEmbeddingModel => {
                let value = self.settings_input_draft.trim().to_string();
                self.edit_settings(
                    move |settings| {
                        let mut config = settings
                            .ai
                            .embedding_config
                            .take()
                            .unwrap_or_else(|| serde_json::json!({ "providerId": null, "model": "" }));
                        if let Some(object) = config.as_object_mut() {
                            object.insert("model".to_string(), serde_json::json!(value.clone()));
                        }
                        settings.ai.embedding_config = Some(config);
                    },
                    cx,
                );
            }
            SettingsInput::AiMcpName => {
                if let Some(draft) = self.ai_mcp_add_dialog.as_mut() {
                    draft.name = self.settings_input_draft.trim().to_string();
                }
                cx.notify();
            }
            SettingsInput::AiMcpCommand => {
                if let Some(draft) = self.ai_mcp_add_dialog.as_mut() {
                    draft.command = self.settings_input_draft.trim().to_string();
                }
                cx.notify();
            }
            SettingsInput::AiMcpArgs => {
                if let Some(draft) = self.ai_mcp_add_dialog.as_mut() {
                    draft.args = self.settings_input_draft.clone();
                }
                cx.notify();
            }
            SettingsInput::AiMcpUrl => {
                if let Some(draft) = self.ai_mcp_add_dialog.as_mut() {
                    draft.url = self.settings_input_draft.trim().to_string();
                }
                cx.notify();
            }
            SettingsInput::AiMcpAuthHeaderName => {
                if let Some(draft) = self.ai_mcp_add_dialog.as_mut() {
                    draft.auth_header_name = self.settings_input_draft.trim().to_string();
                }
                cx.notify();
            }
            SettingsInput::AiMcpAuthToken => {
                if let Some(draft) = self.ai_mcp_add_dialog.as_mut() {
                    draft.auth_token = self.settings_input_draft.clone();
                }
                cx.notify();
            }
            SettingsInput::AiMcpEnvKey(index) => {
                if let Some((key, _)) = self
                    .ai_mcp_add_dialog
                    .as_mut()
                    .and_then(|draft| draft.env.get_mut(index))
                {
                    *key = self.settings_input_draft.trim().to_string();
                }
                cx.notify();
            }
            SettingsInput::AiMcpEnvValue(index) => {
                if let Some((_, value)) = self
                    .ai_mcp_add_dialog
                    .as_mut()
                    .and_then(|draft| draft.env.get_mut(index))
                {
                    *value = self.settings_input_draft.clone();
                }
                cx.notify();
            }
            SettingsInput::AiMcpHeaderKey(index) => {
                if let Some((key, _)) = self
                    .ai_mcp_add_dialog
                    .as_mut()
                    .and_then(|draft| draft.headers.get_mut(index))
                {
                    *key = self.settings_input_draft.trim().to_string();
                }
                cx.notify();
            }
            SettingsInput::AiMcpHeaderValue(index) => {
                if let Some((_, value)) = self
                    .ai_mcp_add_dialog
                    .as_mut()
                    .and_then(|draft| draft.headers.get_mut(index))
                {
                    *value = self.settings_input_draft.clone();
                }
                cx.notify();
            }
            SettingsInput::KnowledgeCollectionName => {
                self.knowledge_new_collection_name = self.settings_input_draft.clone();
                cx.notify();
            }
            SettingsInput::KnowledgeDocumentTitle => {
                self.knowledge_new_document_title = self.settings_input_draft.clone();
                cx.notify();
            }
        }
    }

    fn edit_highlight_rule(
        &mut self,
        index: usize,
        edit: impl FnOnce(&mut HighlightRule),
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            move |settings| {
                if let Some(rule) = settings.terminal.highlight_rules.get_mut(index) {
                    edit(rule);
                }
                settings.terminal.highlight_rules =
                    reindex_highlight_rules(settings.terminal.highlight_rules.clone());
            },
            cx,
        );
    }

    fn add_highlight_rule(&mut self, cx: &mut Context<Self>) {
        self.add_highlight_preset(vec![create_default_highlight_rule(|_| {})], cx);
    }

    fn add_highlight_preset(&mut self, rules: Vec<HighlightRule>, cx: &mut Context<Self>) {
        self.edit_settings(
            move |settings| {
                settings.terminal.highlight_rules.extend(rules);
                settings.terminal.highlight_rules =
                    reindex_highlight_rules(settings.terminal.highlight_rules.clone())
                        .into_iter()
                        .take(MAX_HIGHLIGHT_RULES)
                        .collect();
            },
            cx,
        );
    }

    fn remove_highlight_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        self.edit_settings(
            move |settings| {
                if index < settings.terminal.highlight_rules.len() {
                    settings.terminal.highlight_rules.remove(index);
                }
                settings.terminal.highlight_rules =
                    reindex_highlight_rules(settings.terminal.highlight_rules.clone());
            },
            cx,
        );
    }

    fn move_highlight_rule(&mut self, index: usize, direction: isize, cx: &mut Context<Self>) {
        self.edit_settings(
            move |settings| {
                let len = settings.terminal.highlight_rules.len();
                let next = if direction < 0 {
                    index.checked_sub(1)
                } else if index + 1 < len {
                    Some(index + 1)
                } else {
                    None
                };
                if let Some(next) = next {
                    settings.terminal.highlight_rules.swap(index, next);
                }
                settings.terminal.highlight_rules =
                    reindex_highlight_rules(settings.terminal.highlight_rules.clone());
            },
            cx,
        );
    }

    fn set_font_size_from_position(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::SettingsTerminalFontSizeSlider)
            .copied()
        else {
            return;
        };
        let left = f32::from(anchor.bounds.left());
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let percent = ((x - left) / width).clamp(0.0, 1.0);
        let value = (8.0 + percent * (32.0 - 8.0)).round() as i64;
        if self.settings_store.settings().terminal.font_size != value {
            self.edit_settings(|settings| settings.terminal.font_size = value, cx);
        }
    }

    fn set_settings_slider_from_position(
        &mut self,
        anchor_id: SelectAnchorId,
        x: f32,
        min: f32,
        max: f32,
        apply: fn(&mut PersistedSettings, f32),
        cx: &mut Context<Self>,
    ) {
        let Some(anchor) = self.select_anchors.get(&anchor_id).copied() else {
            return;
        };
        let left = f32::from(anchor.bounds.left());
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let percent = ((x - left) / width).clamp(0.0, 1.0);
        let value = min + percent * (max - min);
        self.edit_settings(|settings| apply(settings, value), cx);
    }

    fn set_background_blur_preview_from_position(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(anchor) = self
            .select_anchors
            .get(&SelectAnchorId::SettingsAppearanceBackgroundBlurSlider)
            .copied()
        else {
            return;
        };
        let left = f32::from(anchor.bounds.left());
        let width = f32::from(anchor.bounds.size.width).max(1.0);
        let percent = ((x - left) / width).clamp(0.0, 1.0);
        let value = (percent * 20.0).round() as i64;
        if self.background_blur_preview == Some(value)
            || (self.background_blur_preview.is_none()
                && self.settings_store.settings().terminal.background_blur == value)
        {
            return;
        }

        self.background_blur_preview = Some(value);
        self.background_blur_commit_generation =
            self.background_blur_commit_generation.wrapping_add(1);
        let generation = self.background_blur_commit_generation;
        cx.notify();

        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(150)).await;
            let _ = weak.update(cx, |this, cx| {
                this.commit_background_blur_preview(generation, cx);
            });
        })
        .detach();
    }

    fn commit_background_blur_preview(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.background_blur_commit_generation != generation {
            return;
        }
        let Some(value) = self.background_blur_preview.take() else {
            return;
        };
        if self.settings_store.settings().terminal.background_blur != value {
            self.edit_settings(|settings| settings.terminal.background_blur = value, cx);
        } else {
            cx.notify();
        }
    }
}

fn settings_input_accepts_newline(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::TerminalCommandBarFocusHandoff
            | SettingsInput::AiSystemPrompt
            | SettingsInput::AiMemoryContent
            | SettingsInput::AiMcpArgs
    )
}

fn settings_input_is_secret(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::AiProviderApiKey(_) | SettingsInput::AiMcpAuthToken
    )
}

fn parse_focus_handoff_command_list(input: &str) -> Vec<String> {
    let mut commands = Vec::new();
    for token in input.split(|ch: char| ch.is_whitespace() || ch == ',') {
        let token = token.trim().to_lowercase();
        if token.is_empty()
            || !token
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '+' | '-'))
            || commands.iter().any(|existing| existing == &token)
        {
            continue;
        }
        commands.push(token);
    }
    commands
}
