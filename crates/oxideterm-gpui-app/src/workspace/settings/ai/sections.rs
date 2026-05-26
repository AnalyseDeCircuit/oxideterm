const AI_TEXTAREA_SYSTEM_PROMPT_MIN_H: f32 = 80.0; // Tauri rows=4 min-h-[80px].
const AI_TEXTAREA_MEMORY_MIN_H: f32 = 120.0; // Tauri rows=5 min-h-[120px].
const AI_TOOL_NUMBER_INPUT_W: f32 = 96.0; // Tauri w-24.

impl WorkspaceApp {
    fn ai_execution_profiles_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let profiles = settings
            .ai
            .execution_profiles
            .get("profiles")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let default_profile_id = ai_default_execution_profile(settings).unwrap_or_else(|| "default".to_string());

        self.sync_ai_execution_profile_list_state(&profiles, &default_profile_id);
        let state = self.ai_execution_profile_list_state.clone();
        let spec = self.ai_execution_profile_list_spec();
        let workspace = cx.entity();
        let profile_count = profiles.len();
        let profile_list = div()
            .h(px(
                profile_count as f32 * AI_EXECUTION_PROFILE_LIST_ESTIMATED_HEIGHT,
            ))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.ai_execution_profile_list_item(index, cx)
                    })
                },
            ));

        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | AI_PROVIDER_SECTION_BORDER_ALPHA,
            ))
            .bg(rgba((self.tokens.ui.bg << 8) | AI_PROVIDER_SECTION_BG_ALPHA))
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.ai_section_heading(
                        "settings_view.ai.execution_profiles",
                        "settings_view.ai.execution_profiles_hint",
                    ))
                    .child(
                        // Tauri's profile add action is an outline small
                        // Button with a leading Plus icon. Keep it on the
                        // shared toolbar primitive with the same compact gap.
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.ai.profile_add"),
                            Some(Self::render_lucide_icon(
                                LucideIcon::Plus,
                                14.0,
                                rgb(self.tokens.ui.text_muted),
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
                                this.edit_settings(ai_add_execution_profile, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                    ),
            )
            .child(profile_list)
            .into_any_element()
    }

    fn sync_ai_execution_profile_list_state(
        &self,
        profiles: &[serde_json::Value],
        default_profile_id: &str,
    ) {
        let signatures = profiles
            .iter()
            .map(|profile| ai_execution_profile_signature(profile, default_profile_id))
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.ai_execution_profile_list_state,
            &mut self.ai_execution_profile_list_cache.borrow_mut(),
            "ai-execution-profiles",
            &signatures,
            self.ai_execution_profile_list_spec(),
        );
    }

    fn ai_execution_profile_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_EXECUTION_PROFILE_LIST_ESTIMATED_HEIGHT),
            AI_EXECUTION_PROFILE_LIST_OVERSCAN,
        )
    }

    fn ai_execution_profile_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let profiles = settings
            .ai
            .execution_profiles
            .get("profiles")
            .and_then(serde_json::Value::as_array);
        let Some(profiles) = profiles else {
            return div().into_any_element();
        };
        let Some(profile) = profiles.get(index) else {
            return div().into_any_element();
        };
        let default_profile_id =
            ai_default_execution_profile(settings).unwrap_or_else(|| "default".to_string());
        let profile_count = profiles.len();
        div()
            .pb(px(8.0))
            .child(self.ai_execution_profile_card(
                index,
                profile,
                &default_profile_id,
                profile_count,
                settings,
                cx,
            ))
            .into_any_element()
    }

    fn ai_execution_profile_card(
        &self,
        index: usize,
        profile: &serde_json::Value,
        default_profile_id: &str,
        profile_count: usize,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let profile_id = ai_execution_profile_id(profile).unwrap_or_else(|| format!("profile-{index}"));
        let is_default = profile_id == default_profile_id;
        let provider_id = profile
            .get("providerId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let provider_label = provider_id
            .as_deref()
            .and_then(|id| {
                settings
                    .ai
                    .providers
                    .iter()
                    .find(|provider| ai_provider_id(provider).as_deref() == Some(id))
                    .and_then(|provider| ai_provider_string(provider, "name"))
            })
            .unwrap_or_else(|| self.i18n.t("settings_view.ai.profile_inherit_provider"));
        let reasoning = profile
            .get("reasoningEffort")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto");

        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x73))
            // Profile rows are nested surfaces, not standalone settings cards;
            // avoid stacking another translucent shadow inside OxideSens.
            .bg(rgba((self.tokens.ui.bg_card << 8) | 0x73))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.settings_text_input_control(
                        SettingsInput::AiProfileName(index),
                        self.current_settings_input_value(SettingsInput::AiProfileName(index)),
                        self.i18n.t("settings_view.ai.profile_add"),
                        180.0,
                        cx,
                    ))
                    .child(self.ai_profile_default_button(index, profile_id.clone(), is_default, cx))
                    .child(self.ai_icon_button(
                        LucideIcon::Copy,
                        false,
                        move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| ai_duplicate_execution_profile(settings, index),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.ai_icon_button(
                        LucideIcon::Trash2,
                        profile_count <= 1,
                        move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| ai_delete_execution_profile(settings, index),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .gap(px(8.0))
                    .child(self.settings_select_control(
                        SettingsSelect::AiProfileProvider(index),
                        provider_label,
                        false,
                        Some(180.0),
                        cx,
                    ))
                    .child(self.settings_text_input_control(
                        SettingsInput::AiProfileModel(index),
                        self.current_settings_input_value(SettingsInput::AiProfileModel(index)),
                        self.i18n.t("settings_view.ai.profile_inherit_model"),
                        180.0,
                        cx,
                    ))
                    .child(self.settings_select_control(
                        SettingsSelect::AiProfileReasoning(index),
                        self.i18n.t(ai_reasoning_label_key(reasoning)),
                        false,
                        Some(160.0),
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn ai_provider_settings_section(
        &self,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.settings_page.ai_provider_settings_expanded;
        let summary = self.i18n_count(
            "settings_view.ai.provider_settings_summary",
            self.settings_store.settings().ai.providers.len(),
        );
        self.sync_ai_provider_card_list_state(providers);
        let provider_list = if expanded {
            let state = self.ai_provider_card_list_state.clone();
            let spec = self.ai_provider_card_list_spec();
            let workspace = cx.entity();
            let list_height = self.ai_provider_card_list_estimated_height(providers);
            Some(
                div()
                    .mt(px(12.0))
                    .h(px(list_height))
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            workspace.update(cx, |this, cx| {
                                this.ai_provider_card_list_item(index, cx)
                            })
                        },
                    ))
                    .into_any_element(),
            )
        } else {
            None
        };

        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .child(self.ai_collapsible_header(
                "settings_view.ai.provider_settings",
                summary,
                expanded,
                |this, _event, _window, cx| {
                    this.settings_page.toggle_ai_section(AiSettingsSection::ProviderSettings);
                    cx.stop_propagation();
                    cx.notify();
                },
                cx,
            ))
            .when_some(provider_list, |section, provider_list| {
                section.child(provider_list)
            })
            .into_any_element()
    }

    fn ai_provider_card_list_estimated_height(&self, providers: &[AiProviderView]) -> f32 {
        providers
            .iter()
            .map(|provider| self.ai_provider_card_estimated_height(provider))
            .sum::<f32>()
            + AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT
    }

    fn ai_provider_card_estimated_height(&self, provider: &AiProviderView) -> f32 {
        let active_provider = self
            .settings_store
            .settings()
            .ai
            .active_provider_id
            .as_deref()
            == Some(provider.id.as_str());
        let expanded = self
            .settings_page.expanded_ai_providers
            .get(&provider.id)
            .copied()
            .unwrap_or(active_provider);
        if !expanded {
            return 72.0;
        }
        let models_expanded = self.settings_page.expanded_ai_provider_models.contains(&provider.id);
        let visible_model_count = if models_expanded {
            provider.models.len()
        } else {
            provider.models.len().min(AI_PROVIDER_VISIBLE_MODEL_LIMIT)
        };
        let chip_rows = visible_model_count
            .div_ceil(AI_PROVIDER_MODEL_CHIPS_PER_VIRTUAL_ROW)
            .max(1);
        let key_input_height = if self.ai_provider_key_display_state(provider).shows_key_control() {
            72.0
        } else {
            0.0
        };
        // This mirrors the nested card structure: header, toolbar, two-column
        // fields, model-chip rows, optional API-key editor, and row spacing.
        72.0 + 52.0
            + 112.0
            + 34.0
            + chip_rows as f32 * AI_PROVIDER_MODEL_CHIP_ROW_ESTIMATED_HEIGHT
            + key_input_height
            + 16.0
    }

    fn sync_ai_provider_card_list_state(&self, providers: &[AiProviderView]) {
        let mut signatures = providers
            .iter()
            .map(|provider| {
                let active_provider = self
                    .settings_store
                    .settings()
                    .ai
                    .active_provider_id
                    .as_deref()
                    == Some(provider.id.as_str());
                let expanded = self
                    .settings_page.expanded_ai_providers
                    .get(&provider.id)
                    .copied()
                    .unwrap_or(active_provider);
                ai_provider_card_signature(
                    provider,
                    expanded,
                    self.settings_page.expanded_ai_provider_models.contains(&provider.id),
                    self.ai_provider_has_key_cached(&provider.id),
                )
            })
            .collect::<Vec<_>>();
        // The add-provider controls are the final virtual row inside this
        // section. Keep a stable sentinel signature for that fixed row.
        signatures.push(0xadd0_0001);
        sync_tauri_variable_list_state_by_signatures(
            &self.ai_provider_card_list_state,
            &mut self.ai_provider_card_list_cache.borrow_mut(),
            "ai-provider-cards",
            &signatures,
            self.ai_provider_card_list_spec(),
        );
    }

    fn ai_provider_card_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_PROVIDER_CARD_LIST_ESTIMATED_HEIGHT),
            AI_PROVIDER_CARD_LIST_OVERSCAN,
        )
    }

    fn ai_provider_card_list_item(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let providers = ai_provider_views(self.settings_store.settings());
        if index == providers.len() {
            return div()
                .pb(px(12.0))
                .child(self.ai_provider_add_controls(cx))
                .into_any_element();
        }
        let Some(provider) = providers.get(index) else {
            return div().into_any_element();
        };
        div()
            .pb(px(12.0))
            .child(self.ai_provider_card(index, provider, cx))
            .into_any_element()
    }

    fn ai_context_controls_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_context_controls_section(
            AI_PROVIDER_MAX_W,
            self.ai_section_title("settings_view.ai.context_controls"),
            vec![
                self.ai_context_select_field(
                        "settings_view.ai.max_context",
                        "settings_view.ai.max_context_hint",
                        SettingsSelect::AiContextMaxChars,
                        self.ai_context_max_chars_label(settings.ai.context_max_chars),
                        cx,
                    ),
                self.ai_context_select_field(
                        "settings_view.ai.buffer_history",
                        "settings_view.ai.buffer_history_hint",
                        SettingsSelect::AiContextVisibleLines,
                        self.ai_context_visible_lines_label(settings.ai.context_visible_lines),
                        cx,
                    ),
            ],
            settings_ai_context_sources_group(
                &self.tokens,
                self.i18n.t("settings_view.ai.context_sources"),
                vec![
                    self.ai_context_source_row(
                        "settings_view.ai.context_source_ide",
                        "settings_view.ai.context_source_ide_hint",
                        settings.ai.context_sources.ide,
                        set_ai_context_source_ide,
                        cx,
                    ),
                    self.ai_context_source_row(
                        "settings_view.ai.context_source_sftp",
                        "settings_view.ai.context_source_sftp_hint",
                        settings.ai.context_sources.sftp,
                        set_ai_context_source_sftp,
                        cx,
                    ),
                ],
            ),
        )
    }

    fn ai_context_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_context_select_field(
            &self.tokens,
            self.i18n.t(label_key),
            self.settings_select_control(select_id, label, false, None, cx),
            self.i18n.t(hint_key),
        )
    }

    fn ai_context_source_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_context_source_row(
            &self.tokens,
            self.i18n.t(label_key),
            self.i18n.t(hint_key),
            checkbox(&self.tokens, String::new(), checked).into_any_element(),
        )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(|settings| setter(settings, !checked), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn ai_context_max_chars_label(&self, value: i64) -> String {
        ai_context_max_chars_label_key(value)
            .map(|key| self.i18n.t(key))
            .unwrap_or_else(|| value.to_string())
    }

    fn ai_context_visible_lines_label(&self, value: i64) -> String {
        ai_context_visible_lines_label_key(value)
            .map(|key| self.i18n.t(key))
            .unwrap_or_else(|| value.to_string())
    }

    fn ai_system_prompt_section(
        &self,
        settings: &PersistedSettings,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_system_prompt_section(
            AI_PROVIDER_MAX_W,
            self.ai_section_title("settings_view.ai.system_prompt_title"),
            self.ai_textarea_row(
                SettingsInput::AiSystemPrompt,
                self.i18n.t("settings_view.ai.custom_system_prompt"),
                self.i18n.t("settings_view.ai.system_prompt_hint"),
                self.i18n.t("settings_view.ai.system_prompt_placeholder"),
                settings.ai.custom_system_prompt.clone(),
                AI_TEXTAREA_SYSTEM_PROMPT_MIN_H,
                cx,
            ),
            self.ai_separator(),
            settings_ai_icon_heading(
                Self::render_lucide_icon(
                        LucideIcon::Brain,
                        16.0,
                        rgb(self.tokens.ui.text),
                    ),
                self.ai_section_title("settings_view.ai.memory_title"),
            )
            .into_any_element(),
            self.bool_row(
                "settings_view.ai.memory_enabled",
                "settings_view.ai.memory_enabled_hint",
                settings.ai.memory.enabled,
                set_ai_memory_enabled,
                cx,
            ),
            self.ai_textarea_row(
                SettingsInput::AiMemoryContent,
                String::new(),
                self.i18n.t("settings_view.ai.memory_hint"),
                self.i18n.t("settings_view.ai.memory_placeholder"),
                settings.ai.memory.content.clone(),
                AI_TEXTAREA_MEMORY_MIN_H,
                cx,
            ),
                // Tauri renders memory clear as a ghost small Button. Keep it
                // on the shared toolbar primitive so disabled state does not
                // need custom per-section button styling.
                self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.ai.memory_clear"),
                    None,
                    ToolbarButtonOptions {
                        button: ButtonOptions {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Md,
                            disabled: settings.ai.memory.content.trim().is_empty(),
                        },
                        ..ToolbarButtonOptions::default()
                    },
                    cx.listener(|this, _event, _window, cx| {
                        this.edit_settings(|settings| settings.ai.memory.content.clear(), cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
            self.ai_separator(),
            vec![
                self.ai_global_reasoning_section(settings, cx),
                self.ai_model_reasoning_overrides_section(settings, providers, cx),
                self.ai_active_model_max_response_tokens_row(settings, cx),
                self.ai_separator(),
                self.ai_model_context_windows_section(settings, providers, cx),
            ],
        )
    }

    fn ai_global_reasoning_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_global_reasoning_section(
            &self.tokens,
            self.i18n.t("settings_view.ai.reasoning_title"),
            self.settings_select_control(
                SettingsSelect::AiGlobalReasoning,
                self.i18n.t(ai_reasoning_label_key(ai_reasoning_profile_value(
                    settings.ai.reasoning_effort,
                ))),
                false,
                None,
                cx,
            ),
            self.i18n.t("settings_view.ai.reasoning_hint"),
            AI_PROVIDER_MAX_W,
        )
    }

    fn ai_tool_use_section(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
        let approved_count = ai_tool_auto_approved_count(settings);
        let total_count = ai_tool_auto_approve_total_count(settings);
        let policy_groups = settings_ai_tool_policy_grid(
            ai_tool_policy_groups(settings)
                .into_iter()
                .map(|group| self.ai_tool_policy_group(group, cx))
                .collect(),
        );

        let collapsed_summary = (!self.settings_page.ai_tool_use_expanded).then(|| {
            settings_ai_tool_collapsed_summary(
                &self.tokens,
                format!(
                    "{} · {}",
                    self.i18n.t("settings_view.ai.tool_use_policy_summary"),
                    self.i18n
                        .t("settings_view.ai.tool_use_collapsed_summary")
                        .replace("{{approved}}", &approved_count.to_string())
                        .replace("{{total}}", &total_count.to_string())
                ),
            )
        });
        let expanded_body = self.settings_page.ai_tool_use_expanded.then(|| {
            settings_ai_tool_expanded_body(
                &self.tokens,
                settings.ai.tool_use.enabled,
                vec![
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.ai.tool_use_approve_hint"))
                        .into_any_element(),
                    self.ai_tool_number_input_row(
                        "settings_view.ai.tool_use_max_rounds",
                        "settings_view.ai.tool_use_max_rounds_hint",
                        SettingsInput::AiToolUseMaxRounds,
                        settings
                            .ai
                            .tool_use
                            .max_rounds
                            .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_ROUNDS),
                        cx,
                    ),
                    self.ai_tool_number_input_row(
                        "settings_view.ai.tool_use_max_calls_per_round",
                        "settings_view.ai.tool_use_max_calls_per_round_hint",
                        SettingsInput::AiToolUseMaxCallsPerRound,
                        settings
                            .ai
                            .tool_use
                            .max_calls_per_round
                            .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND),
                        cx,
                    ),
                    policy_groups,
                    self.ai_disabled_tools_notice(settings, cx),
                    settings_ai_policy_warning(
                        &self.tokens,
                        self.i18n.t("settings_view.ai.tool_policy_warning"),
                    ),
                ],
            )
        });

        settings_ai_tool_use_section(
            AI_PROVIDER_MAX_W,
            settings_ai_icon_heading(
                Self::render_lucide_icon(LucideIcon::Wrench, 16.0, rgb(self.tokens.ui.text)),
                self.ai_section_title("settings_view.ai.tool_use"),
            ),
            self.ai_tool_expand_button(cx),
            self.bool_row(
                "settings_view.ai.tool_use_enabled",
                "settings_view.ai.tool_use_enabled_hint",
                settings.ai.tool_use.enabled,
                set_ai_tool_use_enabled,
                cx,
            ),
            collapsed_summary,
            expanded_body,
            self.ai_separator(),
            self.ai_mcp_servers_section(settings, cx),
        )
    }

    fn ai_tool_number_input_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_tool_number_input_card(
            &self.tokens,
            self.setting_row(
                label_key,
                hint_key,
                self.number_input(input, value.to_string(), AI_TOOL_NUMBER_INPUT_W, cx),
                cx,
            ),
        )
    }

    fn ai_section_heading(&self, title_key: &str, hint_key: &str) -> AnyElement {
        settings_ai_section_heading(&self.tokens, self.i18n.t(title_key), self.i18n.t(hint_key))
    }

    fn ai_collapsible_header(
        &self,
        title_key: &str,
        summary: String,
        expanded: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_ai_collapsible_header(
            &self.tokens,
            self.i18n.t(title_key).to_uppercase(),
            summary,
            Self::render_lucide_icon(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                16.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
            .on_mouse_down(MouseButton::Left, cx.listener(on_click))
            .into_any_element()
    }

    fn ai_icon_button(
        &self,
        icon: LucideIcon,
        disabled: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon_color = if matches!(icon, LucideIcon::Trash2) {
            rgb(self.tokens.ui.error)
        } else {
            rgb(self.tokens.ui.text_muted)
        };

        self.workspace_icon_action_button(
            icon,
            15.0,
            icon_color,
            IconButtonOptions {
                disabled,
                hover_background: Some(rgba((self.tokens.ui.bg_hover << 8) | 0x80)),
                // Tauri AI action buttons are fully opaque until disabled; the
                // workspace wrapper owns the disabled activation guard.
                ..IconButtonOptions::opaque_toolbar(30.0, ButtonRadius::Md)
            },
            on_click,
            cx,
        )
        .into_any_element()
    }

    fn ai_profile_default_button(
        &self,
        _index: usize,
        profile_id: String,
        is_default: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Execution profile default action is a normal small shadcn Button in
        // Tauri. Route it through the workspace toolbar action wrapper so
        // default-profile cards share the same action guard as provider cards.
        self.workspace_toolbar_action_button(
            if is_default {
                self.i18n.t("settings_view.ai.profile_default")
            } else {
                self.i18n.t("settings_view.ai.profile_set_default")
            },
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if is_default {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Outline
                    },
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, _window, cx| {
                this.edit_settings(
                    |settings| ai_set_default_execution_profile(settings, profile_id.clone()),
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn ai_textarea_row(
        &self,
        input: SettingsInput,
        label: String,
        hint: String,
        placeholder: String,
        value: String,
        min_height: f32,
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
        let marked_text = self
            .marked_text_for_target(target)
            .map(|marked| marked.to_string());
        let caret = focused.then(|| {
            text_caret(&self.tokens, self.new_connection_caret_visible).into_any_element()
        });
        let textarea = settings_ai_textarea_surface(
            &self.tokens,
            min_height,
            focused,
            display_value,
            &placeholder,
            marked_text,
            caret,
        )
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
            );

        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        });

        settings_ai_textarea_row(&self.tokens, label, control.into_any_element(), hint)
    }

    fn ai_model_reasoning_overrides_section(
        &self,
        settings: &PersistedSettings,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_panels = ai_model_reasoning_panels(settings, providers);
        div()
            .mt(px(8.0))
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .child(self.ai_model_reasoning_header(cx))
            .when(self.settings_page.ai_model_reasoning_expanded, |section| {
                if provider_panels.is_empty() {
                    section.child(settings_ai_model_empty_text(
                        &self.tokens,
                        self.i18n.t("settings_view.ai.model_reasoning_overrides_empty"),
                    ))
                } else {
                    let mut list = div().flex().flex_col().gap(px(16.0));
                    for panel in provider_panels {
                        list = list.child(self.ai_model_reasoning_provider(settings, panel, cx));
                    }
                    section.child(list)
                }
            })
            .into_any_element()
    }

    fn ai_model_reasoning_header(&self, cx: &mut Context<Self>) -> AnyElement {
        settings_ai_model_reasoning_header(
            &self.tokens,
            self.i18n.t("settings_view.ai.model_reasoning_overrides"),
            self.i18n.t("settings_view.ai.model_reasoning_overrides_hint"),
            Self::render_lucide_icon(
                if self.settings_page.ai_model_reasoning_expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                16.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.settings_page.toggle_ai_section(AiSettingsSection::ModelReasoning);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn ai_model_reasoning_provider(
        &self,
        settings: &PersistedSettings,
        panel: AiProviderModelPanel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = panel.provider_id.clone();
        let expanded = self
            .settings_page.expanded_ai_model_reasoning_providers
            .contains(&provider_id);
        let header_provider_id = provider_id.clone();
        let header = settings_ai_model_provider_header(
            &self.tokens,
            panel.provider_name.clone(),
            self.i18n
                .t("settings_view.ai.model_reasoning_provider_summary")
                .replace("{{count}}", &panel.model_count.to_string())
                .replace("{{overrides}}", &panel.override_count.to_string()),
            Self::render_lucide_icon(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                14.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.settings_page
                    .toggle_ai_model_reasoning_provider(header_provider_id.clone());
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let rows = if expanded {
            let models = panel.models.clone();
            let state = self.sync_ai_reasoning_model_list_state(settings, &provider_id, &models);
            let spec = self.ai_provider_model_row_list_spec();
            let workspace = cx.entity();
            let provider_id_for_rows = provider_id.clone();
            let provider_index = panel.provider_index;
            let list_height =
                models.len() as f32 * AI_PROVIDER_MODEL_ROW_LIST_ESTIMATED_HEIGHT;
            Some(settings_ai_model_row_list_frame(
                &self.tokens,
                list_height,
                tauri_virtual_list(
                    state,
                    spec,
                    move |model_index, _window, cx| {
                        let Some(model) = models.get(model_index).cloned() else {
                            return div().into_any_element();
                        };
                        let provider_id = provider_id_for_rows.clone();
                        workspace.update(cx, |this, cx| {
                            let settings = this.settings_store.settings();
                            this.ai_model_reasoning_row(
                                provider_index,
                                model_index,
                                settings,
                                &provider_id,
                                &model,
                                cx,
                            )
                        })
                    },
                )
                .into_any_element(),
            ))
        } else {
            None
        };
        settings_ai_model_provider_section(header, rows)
    }

    fn sync_ai_reasoning_model_list_state(
        &self,
        settings: &PersistedSettings,
        provider_id: &str,
        models: &[String],
    ) -> ListState {
        let signatures = models
            .iter()
            .map(|model| {
                ai_provider_model_row_signature(
                    provider_id,
                    model,
                    settings
                        .ai
                        .reasoning_model_overrides
                        .get(provider_id)
                        .and_then(|overrides| overrides.get(model)),
                )
            })
            .collect::<Vec<_>>();
        let state = {
            let mut states = self.ai_reasoning_model_list_states.borrow_mut();
            states
                .entry(provider_id.to_string())
                .or_insert_with(|| {
                    // Reasoning override rows are measured independently per
                    // provider so expanding one provider does not perturb
                    // another provider's virtual table.
                    ListState::new(
                        AI_PROVIDER_MODEL_ROW_LIST_INITIAL_ITEM_COUNT,
                        ListAlignment::Top,
                        self.ai_provider_model_row_list_spec().overdraw(),
                    )
                    .measure_all()
                })
                .clone()
        };
        {
            let mut caches = self.ai_reasoning_model_list_caches.borrow_mut();
            let cache = caches.entry(provider_id.to_string()).or_default();
            sync_tauri_variable_list_state_by_signatures(
                &state,
                cache,
                &format!("ai-reasoning-models:{provider_id}"),
                &signatures,
                self.ai_provider_model_row_list_spec(),
            );
        }
        state
    }

    fn ai_model_reasoning_row(
        &self,
        provider_index: usize,
        model_index: usize,
        settings: &PersistedSettings,
        provider_id: &str,
        model: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = ai_model_reasoning_row_model(settings, provider_id, model);
        settings_ai_model_reasoning_row(
            &self.tokens,
            settings_mono_font_family(settings),
            model.to_string(),
            self.settings_select_control(
                SettingsSelect::AiModelReasoning(provider_index, model_index),
                self.i18n.t(row.label_key),
                false,
                Some(160.0),
                cx,
            ),
            model_index == 0,
        )
    }

    fn ai_model_context_windows_section(
        &self,
        settings: &PersistedSettings,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_panels = ai_model_context_window_panels(settings, providers);
        div()
            .opacity(if settings.ai.enabled { 1.0 } else { 0.5 })
            .flex()
            .flex_col()
            .child(self.ai_context_windows_header(cx))
            .when(self.settings_page.ai_context_windows_expanded, |section| {
                if provider_panels.is_empty() {
                    section.child(settings_ai_model_empty_text(
                        &self.tokens,
                        self.i18n.t("settings_view.ai.model_context_windows_empty"),
                    ))
                } else {
                    let mut list = div()
                        .max_w(px(AI_PROVIDER_MAX_W))
                        .flex()
                        .flex_col()
                        .gap(px(16.0));
                    for panel in provider_panels {
                        list = list.child(self.ai_context_window_provider(settings, panel, cx));
                    }
                    section.child(list)
                }
            })
            .into_any_element()
    }

    fn ai_context_windows_header(&self, cx: &mut Context<Self>) -> AnyElement {
        settings_ai_context_windows_header(
            &self.tokens,
            self.i18n.t("settings_view.ai.model_context_windows"),
            self.i18n.t("settings_view.ai.model_context_windows_hint"),
            Self::render_lucide_icon(
                if self.settings_page.ai_context_windows_expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                16.0,
                rgb(self.tokens.ui.text_muted),
            ),
            AI_PROVIDER_MAX_W,
        )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.settings_page.toggle_ai_section(AiSettingsSection::ContextWindows);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn ai_context_window_provider(
        &self,
        settings: &PersistedSettings,
        panel: AiProviderModelPanel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = panel.provider_id.clone();
        let expanded = self.settings_page.expanded_ai_context_providers.contains(&provider_id);
        let header_provider_id = provider_id.clone();
        let header = settings_ai_model_provider_header(
            &self.tokens,
            panel.provider_name.clone(),
            self.i18n
                .t("settings_view.ai.ctx_provider_summary")
                .replace("{{count}}", &panel.model_count.to_string())
                .replace("{{overrides}}", &panel.override_count.to_string()),
            Self::render_lucide_icon(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                14.0,
                rgb(self.tokens.ui.text_muted),
            ),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.settings_page
                    .toggle_ai_context_provider(header_provider_id.clone());
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let rows = if expanded {
            let models = panel.models.clone();
            let state = self.sync_ai_context_model_list_state(settings, &provider_id, &models);
            let spec = self.ai_provider_model_row_list_spec();
            let workspace = cx.entity();
            let provider_id_for_rows = provider_id.clone();
            let provider_index = panel.provider_index;
            let list_height =
                models.len() as f32 * AI_PROVIDER_MODEL_ROW_LIST_ESTIMATED_HEIGHT;
            Some(settings_ai_model_row_list_frame(
                &self.tokens,
                list_height,
                tauri_virtual_list(
                    state,
                    spec,
                    move |model_index, _window, cx| {
                        let Some(model) = models.get(model_index).cloned() else {
                            return div().into_any_element();
                        };
                        let provider_id = provider_id_for_rows.clone();
                        workspace.update(cx, |this, cx| {
                            let settings = this.settings_store.settings();
                            this.ai_context_window_row(
                                provider_index,
                                model_index,
                                settings,
                                &provider_id,
                                &model,
                                cx,
                            )
                        })
                    },
                )
                .into_any_element(),
            ))
        } else {
            None
        };
        settings_ai_model_provider_section(header, rows)
    }

    fn sync_ai_context_model_list_state(
        &self,
        settings: &PersistedSettings,
        provider_id: &str,
        models: &[String],
    ) -> ListState {
        let signatures = models
            .iter()
            .map(|model| {
                ai_provider_model_row_signature(
                    provider_id,
                    model,
                    settings
                        .ai
                        .user_context_windows
                        .get(provider_id)
                        .and_then(|windows| windows.get(model)),
                )
            })
            .collect::<Vec<_>>();
        let state = {
            let mut states = self.ai_context_model_list_states.borrow_mut();
            states
                .entry(provider_id.to_string())
                .or_insert_with(|| {
                    // Context override rows are measured independently per
                    // provider for the same reason as reasoning rows.
                    ListState::new(
                        AI_PROVIDER_MODEL_ROW_LIST_INITIAL_ITEM_COUNT,
                        ListAlignment::Top,
                        self.ai_provider_model_row_list_spec().overdraw(),
                    )
                    .measure_all()
                })
                .clone()
        };
        {
            let mut caches = self.ai_context_model_list_caches.borrow_mut();
            let cache = caches.entry(provider_id.to_string()).or_default();
            sync_tauri_variable_list_state_by_signatures(
                &state,
                cache,
                &format!("ai-context-models:{provider_id}"),
                &signatures,
                self.ai_provider_model_row_list_spec(),
            );
        }
        state
    }

    fn ai_provider_model_row_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(AI_PROVIDER_MODEL_ROW_LIST_ESTIMATED_HEIGHT),
            AI_PROVIDER_MODEL_ROW_LIST_OVERSCAN,
        )
    }

    fn ai_context_window_row(
        &self,
        provider_index: usize,
        model_index: usize,
        settings: &PersistedSettings,
        provider_id: &str,
        model: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = ai_model_context_window_row_model(settings, provider_id, model);
        let input = SettingsInput::AiModelContextWindow(provider_index, model_index);
        let reset_provider_id = provider_id.to_string();
        let reset_model = model.to_string();
        let reset = row.has_override.then(|| {
            div()
                .cursor_pointer()
                .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x99))
                .hover(|style| style.text_color(rgb(self.tokens.ui.text)))
                .child(Self::render_lucide_icon(
                    LucideIcon::X,
                    12.0,
                    rgb(self.tokens.ui.text_muted),
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        let provider_id = reset_provider_id.clone();
                        let model = reset_model.clone();
                        this.edit_settings(
                            move |settings| {
                                set_ai_user_context_window(
                                    settings,
                                    &provider_id,
                                    &model,
                                    None,
                                );
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
        });
        settings_ai_context_window_row(
            &self.tokens,
            settings_mono_font_family(settings),
            model.to_string(),
            settings_ai_context_source_badge_for_source(
                &self.tokens,
                self.i18n.t(row.source.i18n_key()),
                row.source,
            ),
            self.settings_text_input_control(
                input,
                self.current_settings_input_value(input),
                "Auto".to_string(),
                AI_CONTEXT_NUMBER_W,
                cx,
            )
            .into_any_element(),
            reset,
            row.has_override,
            model_index == 0,
        )
    }

    fn ai_active_model_max_response_tokens_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(model) = settings.ai.active_model.clone() else {
            return div().into_any_element();
        };
        settings_ai_active_model_max_response_tokens_row(
            &self.tokens,
            self.i18n.t("settings_view.ai.max_response_tokens"),
            self.i18n.t("settings_view.ai.max_response_tokens_hint"),
            format!("{model}:"),
            self.settings_text_input_control(
                SettingsInput::AiActiveModelMaxResponseTokens,
                self.current_settings_input_value(SettingsInput::AiActiveModelMaxResponseTokens),
                "Auto".to_string(),
                128.0,
                cx,
            ),
            settings_mono_font_family(settings),
        )
    }

    fn ai_tool_expand_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.settings_page.ai_tool_use_expanded;
        // Tool-policy expand/collapse is an outline small Button in Tauri.
        // Route it through the same shared primitive as other settings
        // command buttons.
        self.workspace_toolbar_action_button(
            if expanded {
                self.i18n.t("settings_view.ai.tool_use_collapse")
            } else {
                self.i18n.t("settings_view.ai.tool_use_expand")
            },
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(|this, _event, _window, cx| {
                this.settings_page.toggle_ai_section(AiSettingsSection::ToolUse);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn ai_tool_policy_group(
        &self,
        group: AiToolPolicyGroup,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut items = Vec::new();
        for item in group.items {
            let tool_key = item.key.map(str::to_string);
            let checked = item.checked;
            let locked = item.locked;
            let control = checkbox(&self.tokens, String::new(), checked)
                .opacity(if locked { 0.5 } else { 1.0 })
                .when(!locked, |checkbox| {
                    checkbox.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            let Some(tool_key) = tool_key.clone() else {
                                return;
                            };
                            this.edit_settings(
                                move |settings| {
                                    settings
                                        .ai
                                        .tool_use
                                        .auto_approve_tools
                                        .insert(tool_key.clone(), serde_json::json!(!checked));
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                })
                .into_any_element();
            items.push(settings_ai_tool_policy_item(
                &self.tokens,
                self.i18n.t(item.label_key),
                control,
            ));
        }

        settings_ai_tool_policy_group(
            &self.tokens,
            self.i18n.t(group.title_key),
            self.i18n.t(group.description_key),
            items,
        )
    }

    fn ai_disabled_tools_notice(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = settings.ai.tool_use.disabled_tools.len();
        if count == 0 {
            return div().into_any_element();
        }
        settings_ai_disabled_tools_notice(
            &self.tokens,
            self.i18n
                .t("settings_view.ai.tool_use_disabled_tools_title")
                .replace("{{count}}", &count.to_string()),
            self.workspace_toolbar_action_button(
                self.i18n
                    .t("settings_view.ai.tool_use_restore_disabled_tools"),
                None,
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(|this, _event, _window, cx| {
                    this.edit_settings(|settings| settings.ai.tool_use.disabled_tools.clear(), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element(),
        )
    }

}
