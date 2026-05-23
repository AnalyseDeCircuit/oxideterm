const AI_TEXTAREA_SYSTEM_PROMPT_MIN_H: f32 = 80.0; // Tauri rows=4 min-h-[80px].
const AI_TEXTAREA_MEMORY_MIN_H: f32 = 120.0; // Tauri rows=5 min-h-[120px].
const AI_TOOL_NUMBER_INPUT_W: f32 = 96.0; // Tauri w-24.
const AI_TOOL_POLICY_CARD_BG_ALPHA: u32 = 0x4d; // Tauri bg-theme-bg-panel/30.
const AI_TOOL_POLICY_ROW_BG_ALPHA: u32 = 0x40; // Tauri bg-theme-bg/25.
const AI_TOOL_POLICY_BORDER_ALPHA: u32 = 0x99; // Tauri border-theme-border/60.

struct AiToolPolicyItem {
    key: Option<&'static str>,
    label_key: &'static str,
    checked: bool,
    locked: bool,
}

struct AiToolPolicyGroup {
    title_key: &'static str,
    description_key: &'static str,
    items: Vec<AiToolPolicyItem>,
}

fn ai_execution_profile_signature(profile: &serde_json::Value, default_profile_id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Profile cards expose the serialized profile fields plus default status.
    // Hash the stable JSON representation so edits splice the right row.
    serde_json::to_string(profile)
        .unwrap_or_default()
        .hash(&mut hasher);
    ai_execution_profile_id(profile)
        .as_deref()
        .map(|id| id == default_profile_id)
        .unwrap_or(false)
        .hash(&mut hasher);
    hasher.finish()
}

fn ai_provider_model_row_signature(provider_id: &str, model: &str, override_value: Option<&serde_json::Value>) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Model override rows expose provider/model identity and the current
    // override cell. Hash those fields so only changed rows are remeasured.
    provider_id.hash(&mut hasher);
    model.hash(&mut hasher);
    override_value
        .map(serde_json::Value::to_string)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

fn ai_provider_card_signature(
    provider: &AiProviderView,
    expanded: bool,
    models_expanded: bool,
    has_key: bool,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Provider cards expose config fields, expansion state, model chip count,
    // and key-control visibility. Keep this signature scoped to height/visible
    // card content so the inner ListState remeasures without moving the outer
    // settings section.
    provider.id.hash(&mut hasher);
    provider.name.hash(&mut hasher);
    provider.provider_type.hash(&mut hasher);
    provider.enabled.hash(&mut hasher);
    provider.custom.hash(&mut hasher);
    provider.default_model.hash(&mut hasher);
    provider.base_url.hash(&mut hasher);
    provider.models.len().hash(&mut hasher);
    expanded.hash(&mut hasher);
    models_expanded.hash(&mut hasher);
    has_key.hash(&mut hasher);
    hasher.finish()
}

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
                    .child(self.ai_settings_select_control(
                        SettingsSelect::AiProfileProvider(index),
                        provider_label,
                        180.0,
                        cx,
                    ))
                    .child(self.settings_text_input_control(
                        SettingsInput::AiProfileModel(index),
                        self.current_settings_input_value(SettingsInput::AiProfileModel(index)),
                        self.i18n.t("settings_view.ai.profile_inherit_model"),
                        180.0,
                        cx,
                    ))
                    .child(self.ai_settings_select_control(
                        SettingsSelect::AiProfileReasoning(index),
                        self.ai_reasoning_display(reasoning),
                        160.0,
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
        let expanded = self.ai_provider_settings_expanded;
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
                    this.ai_provider_settings_expanded = !this.ai_provider_settings_expanded;
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
            .expanded_ai_providers
            .get(&provider.id)
            .copied()
            .unwrap_or(active_provider);
        if !expanded {
            return 72.0;
        }
        let models_expanded = self.expanded_ai_provider_models.contains(&provider.id);
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
                    .expanded_ai_providers
                    .get(&provider.id)
                    .copied()
                    .unwrap_or(active_provider);
                ai_provider_card_signature(
                    provider,
                    expanded,
                    self.expanded_ai_provider_models.contains(&provider.id),
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
        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.ai_section_title("settings_view.ai.context_controls"))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(24.0))
                    .child(self.ai_context_select_field(
                        "settings_view.ai.max_context",
                        "settings_view.ai.max_context_hint",
                        SettingsSelect::AiContextMaxChars,
                        self.ai_context_max_chars_label(settings.ai.context_max_chars),
                        cx,
                    ))
                    .child(self.ai_context_select_field(
                        "settings_view.ai.buffer_history",
                        "settings_view.ai.buffer_history_hint",
                        SettingsSelect::AiContextVisibleLines,
                        self.ai_context_visible_lines_label(settings.ai.context_visible_lines),
                        cx,
                    )),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.context_sources").to_uppercase()),
                    )
                    .child(self.ai_context_source_row(
                        "settings_view.ai.context_source_ide",
                        "settings_view.ai.context_source_ide_hint",
                        settings.ai.context_sources.ide,
                        set_ai_context_source_ide,
                        cx,
                    ))
                    .child(self.ai_context_source_row(
                        "settings_view.ai.context_source_sftp",
                        "settings_view.ai.context_source_sftp_hint",
                        settings.ai.context_sources.sftp,
                        set_ai_context_source_sftp,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn ai_context_select_field(
        &self,
        label_key: &str,
        hint_key: &str,
        select_id: SettingsSelect,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.ai_context_select_control(select_id, label, cx))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(hint_key)),
            )
            .into_any_element()
    }

    fn ai_context_select_control(
        &self,
        select_id: SettingsSelect,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_select_control(select_id, label, false, None, cx)
    }

    fn ai_context_source_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(AI_CONTEXT_SOURCE_ROW_GAP))
            .cursor_pointer()
            .child(checkbox(&self.tokens, String::new(), checked))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
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
        match value {
            2_000 => self.i18n.t("settings_view.ai.chars_2000"),
            4_000 => self.i18n.t("settings_view.ai.chars_4000"),
            8_000 => self.i18n.t("settings_view.ai.chars_8000"),
            16_000 => self.i18n.t("settings_view.ai.chars_16000"),
            32_000 => self.i18n.t("settings_view.ai.chars_32000"),
            other => other.to_string(),
        }
    }

    fn ai_context_visible_lines_label(&self, value: i64) -> String {
        match value {
            50 => self.i18n.t("settings_view.ai.lines_50"),
            100 => self.i18n.t("settings_view.ai.lines_100"),
            200 => self.i18n.t("settings_view.ai.lines_200"),
            400 => self.i18n.t("settings_view.ai.lines_400"),
            other => other.to_string(),
        }
    }

    fn ai_system_prompt_section(
        &self,
        settings: &PersistedSettings,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.ai_section_title("settings_view.ai.system_prompt_title"))
            .child(self.ai_textarea_row(
                SettingsInput::AiSystemPrompt,
                self.i18n.t("settings_view.ai.custom_system_prompt"),
                self.i18n.t("settings_view.ai.system_prompt_hint"),
                self.i18n.t("settings_view.ai.system_prompt_placeholder"),
                settings.ai.custom_system_prompt.clone(),
                AI_TEXTAREA_SYSTEM_PROMPT_MIN_H,
                cx,
            ))
            .child(self.ai_separator())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Brain,
                        16.0,
                        rgb(self.tokens.ui.text),
                    ))
                    .child(self.ai_section_title("settings_view.ai.memory_title")),
            )
            .child(self.bool_row(
                "settings_view.ai.memory_enabled",
                "settings_view.ai.memory_enabled_hint",
                settings.ai.memory.enabled,
                set_ai_memory_enabled,
                cx,
            ))
            .child(self.ai_textarea_row(
                SettingsInput::AiMemoryContent,
                String::new(),
                self.i18n.t("settings_view.ai.memory_hint"),
                self.i18n.t("settings_view.ai.memory_placeholder"),
                settings.ai.memory.content.clone(),
                AI_TEXTAREA_MEMORY_MIN_H,
                cx,
            ))
            .child(
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
            )
            .child(self.ai_separator())
            .child(self.ai_global_reasoning_section(settings, cx))
            .child(self.ai_model_reasoning_overrides_section(settings, providers, cx))
            .child(self.ai_active_model_max_response_tokens_row(settings, cx))
            .child(self.ai_separator())
            .child(self.ai_model_context_windows_section(settings, providers, cx))
            .into_any_element()
    }

    fn ai_global_reasoning_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .mb(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.ai.reasoning_title").to_uppercase()),
            )
            .child(self.ai_context_select_control(
                SettingsSelect::AiGlobalReasoning,
                self.ai_reasoning_display(ai_reasoning_profile_value(settings.ai.reasoning_effort)),
                cx,
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.reasoning_hint")),
            )
            .into_any_element()
    }

    fn ai_tool_use_section(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
        let approved_count = settings
            .ai
            .tool_use
            .auto_approve_tools
            .values()
            .filter(|value| value.as_bool() == Some(true))
            .count();
        let total_count = settings.ai.tool_use.auto_approve_tools.len();
        let mut policy_groups = div().grid().grid_cols(2).gap(px(12.0));
        for group in self.ai_tool_policy_groups(settings) {
            policy_groups = policy_groups.child(self.ai_tool_policy_group(group, cx));
        }

        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(16.0))
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
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Wrench,
                                16.0,
                                rgb(self.tokens.ui.text),
                            ))
                            .child(self.ai_section_title("settings_view.ai.tool_use")),
                    )
                    .child(self.ai_tool_expand_button(cx)),
            )
            .child(self.bool_row(
                "settings_view.ai.tool_use_enabled",
                "settings_view.ai.tool_use_enabled_hint",
                settings.ai.tool_use.enabled,
                set_ai_tool_use_enabled,
                cx,
            ))
            .when(!self.ai_tool_use_expanded, |section| {
                section.child(
                    div()
                        .ml(px(16.0))
                        .pl(px(16.0))
                        .border_l_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!(
                            "{} · {}",
                            self.i18n.t("settings_view.ai.tool_use_policy_summary"),
                            self.i18n
                                .t("settings_view.ai.tool_use_collapsed_summary")
                                .replace("{{approved}}", &approved_count.to_string())
                                .replace("{{total}}", &total_count.to_string())
                        )),
                )
            })
            .when(self.ai_tool_use_expanded, |section| {
                section.child(
                    div()
                        .ml(px(16.0))
                        .pl(px(16.0))
                        .border_l_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                        .flex()
                        .flex_col()
                        .gap(px(20.0))
                        .opacity(if settings.ai.tool_use.enabled { 1.0 } else { 0.4 })
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.ai.tool_use_approve_hint")),
                        )
                        .child(self.ai_tool_number_input_row(
                            "settings_view.ai.tool_use_max_rounds",
                            "settings_view.ai.tool_use_max_rounds_hint",
                            SettingsInput::AiToolUseMaxRounds,
                            settings
                                .ai
                                .tool_use
                                .max_rounds
                                .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_ROUNDS),
                            cx,
                        ))
                        .child(self.ai_tool_number_input_row(
                            "settings_view.ai.tool_use_max_calls_per_round",
                            "settings_view.ai.tool_use_max_calls_per_round_hint",
                            SettingsInput::AiToolUseMaxCallsPerRound,
                            settings
                                .ai
                                .tool_use
                                .max_calls_per_round
                                .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND),
                            cx,
                        ))
                        .child(policy_groups)
                        .child(self.ai_disabled_tools_notice(settings, cx))
                        .child(self.ai_policy_warning()),
                )
            })
            .child(self.ai_separator())
            .child(self.ai_mcp_summary_section(settings, cx))
            .into_any_element()
    }

    fn ai_tool_number_input_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | AI_TOOL_POLICY_BORDER_ALPHA))
            .bg(rgba((self.tokens.ui.bg_panel << 8) | AI_TOOL_POLICY_CARD_BG_ALPHA))
            .p(px(12.0))
            .child(self.setting_row(
                label_key,
                hint_key,
                self.number_input(input, value.to_string(), AI_TOOL_NUMBER_INPUT_W, cx),
                cx,
            ))
            .into_any_element()
    }

    fn ai_tool_policy_groups(&self, settings: &PersistedSettings) -> Vec<AiToolPolicyGroup> {
        let auto = &settings.ai.tool_use.auto_approve_tools;
        let checked = |key: &str| auto.get(key).and_then(serde_json::Value::as_bool) == Some(true);
        vec![
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_read_title",
                description_key: "settings_view.ai.tool_policy_read_desc",
                items: vec![AiToolPolicyItem {
                    key: None,
                    label_key: "settings_view.ai.tool_policy_read_auto",
                    checked: true,
                    locked: true,
                }],
            },
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_execute_title",
                description_key: "settings_view.ai.tool_policy_execute_desc",
                items: vec![AiToolPolicyItem {
                    key: Some("run_command"),
                    label_key: "settings_view.ai.tool_policy_execute_run_command",
                    checked: checked("run_command"),
                    locked: false,
                }],
            },
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_interactive_title",
                description_key: "settings_view.ai.tool_policy_interactive_desc",
                items: vec![AiToolPolicyItem {
                    key: Some("send_terminal_input"),
                    label_key: "settings_view.ai.tool_policy_interactive_send_input",
                    checked: checked("send_terminal_input"),
                    locked: false,
                }],
            },
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_navigation_title",
                description_key: "settings_view.ai.tool_policy_navigation_desc",
                items: vec![
                    AiToolPolicyItem {
                        key: Some("connect_target"),
                        label_key: "settings_view.ai.tool_policy_connect_target",
                        checked: checked("connect_target"),
                        locked: false,
                    },
                    AiToolPolicyItem {
                        key: Some("open_app_surface"),
                        label_key: "settings_view.ai.tool_policy_open_surface",
                        checked: checked("open_app_surface"),
                        locked: false,
                    },
                ],
            },
            AiToolPolicyGroup {
                title_key: "settings_view.ai.tool_policy_write_title",
                description_key: "settings_view.ai.tool_policy_write_desc",
                items: vec![
                    AiToolPolicyItem {
                        key: Some("write_resource:settings"),
                        label_key: "settings_view.ai.tool_policy_write_settings",
                        checked: checked("write_resource:settings"),
                        locked: false,
                    },
                    AiToolPolicyItem {
                        key: Some("write_resource:file"),
                        label_key: "settings_view.ai.tool_policy_write_file",
                        checked: checked("write_resource:file"),
                        locked: false,
                    },
                    AiToolPolicyItem {
                        key: Some("transfer_resource"),
                        label_key: "settings_view.ai.tool_policy_transfer_resource",
                        checked: checked("transfer_resource"),
                        locked: false,
                    },
                    AiToolPolicyItem {
                        key: Some("remember_preference"),
                        label_key: "settings_view.ai.tool_policy_remember_preference",
                        checked: checked("remember_preference"),
                        locked: false,
                    },
                ],
            },
        ]
    }

    fn ai_section_heading(&self, title_key: &str, hint_key: &str) -> AnyElement {
        div()
            .min_w(px(0.0))
            .flex_1()
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .whitespace_nowrap()
                    .child(self.i18n.t(title_key).to_uppercase()),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(hint_key)),
            )
            .into_any_element()
    }

    fn ai_collapsible_header(
        &self,
        title_key: &str,
        summary: String,
        expanded: bool,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .mb(px(16.0))
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .px(px(4.0))
            .py(px(4.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba((self.tokens.ui.bg_hover << 8) | 0x66))
                    .text_color(rgb(self.tokens.ui.text))
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(title_key).to_uppercase()),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(summary),
                    ),
            )
            .child(Self::render_lucide_icon(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                16.0,
                rgb(self.tokens.ui.text_muted),
            ))
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

    fn ai_settings_select_control(
        &self,
        select_id: SettingsSelect,
        label: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.settings_select_control(select_id, label, false, Some(width), cx)
    }

    fn ai_reasoning_display(&self, value: &str) -> String {
        let key = match value {
            "off" | "none" => "settings_view.ai.reasoning_off",
            "low" | "minimal" => "settings_view.ai.reasoning_low",
            "medium" => "settings_view.ai.reasoning_medium",
            "high" => "settings_view.ai.reasoning_high",
            "max" | "xhigh" => "settings_view.ai.reasoning_max",
            _ => "settings_view.ai.reasoning_auto",
        };
        self.i18n.t(key)
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
        let theme = self.tokens.ui;
        let mut textarea = div()
            .w_full()
            .min_h(px(min_height))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x66)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(2.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(20.0))
            .text_color(rgb(theme.text))
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

        if display_value.is_empty() {
            for line in placeholder.split('\n') {
                textarea = textarea.child(
                    div()
                        .min_h(px(20.0))
                        .text_color(rgba((theme.text_muted << 8) | 0x66))
                        .child(line.to_string()),
                );
            }
        } else {
            for line in display_value.split('\n') {
                textarea = textarea.child(div().min_h(px(20.0)).child(line.to_string()));
            }
        }

        if let Some(marked) = self.marked_text_for_target(target) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        if focused {
            textarea = textarea.child(text_caret(
                &self.tokens,
                self.new_connection_caret_visible,
            ));
        }

        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        });

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .when(!label.is_empty(), |row| {
                row.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(label),
                )
            })
            .child(control)
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .line_height(px(18.0))
                    .child(hint),
            )
            .into_any_element()
    }

    fn ai_model_reasoning_overrides_section(
        &self,
        settings: &PersistedSettings,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let providers_with_models: Vec<_> = providers
            .iter()
            .enumerate()
            .filter(|(_, provider)| !provider.models.is_empty())
            .collect();
        div()
            .mt(px(8.0))
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .child(self.ai_model_reasoning_header(cx))
            .when(self.ai_model_reasoning_expanded, |section| {
                if providers_with_models.is_empty() {
                    section.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .italic()
                            .child(self.i18n.t("settings_view.ai.model_reasoning_overrides_empty")),
                    )
                } else {
                    let mut list = div().flex().flex_col().gap(px(16.0));
                    for (provider_index, provider) in providers_with_models {
                        list = list.child(self.ai_model_reasoning_provider(
                            provider_index,
                            settings,
                            provider,
                            cx,
                        ));
                    }
                    section.child(list)
                }
            })
            .into_any_element()
    }

    fn ai_model_reasoning_header(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .mb(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(4.0))
            .py(px(4.0))
            .flex()
            .items_start()
            .justify_between()
            .gap(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgba(
                        (self.tokens.ui.bg_hover << 8) | AI_CONTEXT_PROVIDER_HOVER_ALPHA,
                    ))
                    .text_color(rgb(self.tokens.ui.text))
            })
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(
                                self.i18n
                                    .t("settings_view.ai.model_reasoning_overrides")
                                    .to_uppercase(),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.model_reasoning_overrides_hint")),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .child(Self::render_lucide_icon(
                        if self.ai_model_reasoning_expanded {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        16.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.ai_model_reasoning_expanded = !this.ai_model_reasoning_expanded;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn ai_model_reasoning_provider(
        &self,
        provider_index: usize,
        settings: &PersistedSettings,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let expanded = self
            .expanded_ai_model_reasoning_providers
            .contains(&provider_id);
        let override_count = provider
            .models
            .iter()
            .filter(|model| {
                settings
                    .ai
                    .reasoning_model_overrides
                    .get(&provider_id)
                    .and_then(|models| models.get(model.as_str()))
                    .is_some()
            })
            .count();
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .mb(px(4.0))
                    .rounded(px(self.tokens.radii.sm))
                    .px(px(4.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(provider.name.to_uppercase()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                self.i18n
                                    .t("settings_view.ai.model_reasoning_provider_summary")
                                    .replace("{{count}}", &provider.models.len().to_string())
                                    .replace("{{overrides}}", &override_count.to_string()),
                            )
                            .child(Self::render_lucide_icon(
                                if expanded {
                                    LucideIcon::ChevronDown
                                } else {
                                    LucideIcon::ChevronRight
                                },
                                14.0,
                                rgb(self.tokens.ui.text_muted),
                            )),
                    )
                    .hover(|style| {
                        style
                            .bg(rgba(
                                (self.tokens.ui.bg_hover << 8) | AI_CONTEXT_PROVIDER_HOVER_ALPHA,
                            ))
                            .text_color(rgb(self.tokens.ui.text))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if this
                                .expanded_ai_model_reasoning_providers
                                .contains(&provider_id)
                            {
                                this.expanded_ai_model_reasoning_providers.remove(&provider_id);
                            } else {
                                this.expanded_ai_model_reasoning_providers
                                    .insert(provider_id.clone());
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        if expanded {
            let models = provider.models.clone();
            let state = self.sync_ai_reasoning_model_list_state(settings, &provider.id, &models);
            let spec = self.ai_provider_model_row_list_spec();
            let workspace = cx.entity();
            let provider_id_for_rows = provider.id.clone();
            let list_height =
                models.len() as f32 * AI_PROVIDER_MODEL_ROW_LIST_ESTIMATED_HEIGHT;
            let rows = div()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba(
                    (self.tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_BORDER_ALPHA,
                ))
                .overflow_hidden()
                .h(px(list_height))
                .child(tauri_virtual_list(
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
                ));
            section = section.child(rows);
        }
        section.into_any_element()
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
        let current = settings
            .ai
            .reasoning_model_overrides
            .get(provider_id)
            .and_then(|models| models.get(model))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("__inherit__")
            .to_string();
        let label = if current == "__inherit__" {
            self.i18n.t("settings_view.ai.reasoning_inherit_provider")
        } else {
            self.ai_reasoning_display(&current)
        };
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .when(model_index > 0, |row| {
                row.border_t_1().border_color(rgba(
                    (self.tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_TOP_BORDER_ALPHA,
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_family(settings_mono_font_family(settings))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .overflow_hidden()
                    .child(model.to_string()),
            )
            .child(self.ai_settings_select_control(
                SettingsSelect::AiModelReasoning(provider_index, model_index),
                label,
                160.0,
                cx,
            ))
            .into_any_element()
    }

    fn ai_model_context_windows_section(
        &self,
        settings: &PersistedSettings,
        providers: &[AiProviderView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let providers_with_models: Vec<_> = providers
            .iter()
            .enumerate()
            .filter(|(_, provider)| !provider.models.is_empty())
            .collect();
        div()
            .opacity(if settings.ai.enabled { 1.0 } else { 0.5 })
            .flex()
            .flex_col()
            .child(self.ai_context_windows_header(cx))
            .when(self.ai_context_windows_expanded, |section| {
                if providers_with_models.is_empty() {
                    section.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .italic()
                            .child(self.i18n.t("settings_view.ai.model_context_windows_empty")),
                    )
                } else {
                    let mut list = div()
                        .max_w(px(AI_PROVIDER_MAX_W))
                        .flex()
                        .flex_col()
                        .gap(px(16.0));
                    for (provider_index, provider) in providers_with_models {
                        list = list.child(self.ai_context_window_provider(
                            provider_index,
                            settings,
                            provider,
                            cx,
                        ));
                    }
                    section.child(list)
                }
            })
            .into_any_element()
    }

    fn ai_context_windows_header(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .mb(px(16.0))
            .w_full()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .items_start()
            .justify_between()
            .gap(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(
                                self.i18n
                                    .t("settings_view.ai.model_context_windows")
                                    .to_uppercase(),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.model_context_windows_hint")),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .child(Self::render_lucide_icon(
                        if self.ai_context_windows_expanded {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        16.0,
                        rgb(self.tokens.ui.text_muted),
                    )),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.ai_context_windows_expanded = !this.ai_context_windows_expanded;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn ai_context_window_provider(
        &self,
        provider_index: usize,
        settings: &PersistedSettings,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let expanded = self.expanded_ai_context_providers.contains(&provider_id);
        let override_count = provider
            .models
            .iter()
            .filter(|model| {
                settings
                    .ai
                    .user_context_windows
                    .get(&provider.id)
                    .and_then(|windows| windows.get(model.as_str()))
                    .is_some()
            })
            .count();
        let mut section = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .mb(px(4.0))
                    .rounded(px(self.tokens.radii.sm))
                    .px(px(4.0))
                    .py(px(4.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(provider.name.to_uppercase()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                self.i18n
                                    .t("settings_view.ai.ctx_provider_summary")
                                    .replace("{{count}}", &provider.models.len().to_string())
                                    .replace("{{overrides}}", &override_count.to_string()),
                            )
                            .child(Self::render_lucide_icon(
                                if expanded {
                                    LucideIcon::ChevronDown
                                } else {
                                    LucideIcon::ChevronRight
                                },
                                14.0,
                                rgb(self.tokens.ui.text_muted),
                            )),
                    )
                    .hover(|style| {
                        style
                            .bg(rgba(
                                (self.tokens.ui.bg_hover << 8) | AI_CONTEXT_PROVIDER_HOVER_ALPHA,
                            ))
                            .text_color(rgb(self.tokens.ui.text))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if this.expanded_ai_context_providers.contains(&provider_id) {
                                this.expanded_ai_context_providers.remove(&provider_id);
                            } else {
                                this.expanded_ai_context_providers.insert(provider_id.clone());
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            );
        if expanded {
            let models = provider.models.clone();
            let state = self.sync_ai_context_model_list_state(settings, &provider.id, &models);
            let spec = self.ai_provider_model_row_list_spec();
            let workspace = cx.entity();
            let provider_id_for_rows = provider.id.clone();
            let list_height =
                models.len() as f32 * AI_PROVIDER_MODEL_ROW_LIST_ESTIMATED_HEIGHT;
            let rows = div()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba(
                    (self.tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_BORDER_ALPHA,
                ))
                .overflow_hidden()
                .h(px(list_height))
                .child(tauri_virtual_list(
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
                ));
            section = section.child(rows);
        }
        section.into_any_element()
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
        let has_override = settings
            .ai
            .user_context_windows
            .get(provider_id)
            .and_then(|windows| windows.get(model))
            .is_some();
        let info = ai_model_context_window_info(
            model,
            &settings.ai.model_context_windows,
            Some(provider_id),
            &settings.ai.user_context_windows,
        );
        let input = SettingsInput::AiModelContextWindow(provider_index, model_index);
        let reset_provider_id = provider_id.to_string();
        let reset_model = model.to_string();
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .bg(if has_override {
                rgba((self.tokens.ui.accent << 8) | AI_CONTEXT_USER_OVERRIDE_BG_ALPHA)
            } else {
                rgba((self.tokens.ui.bg << 8) | 0x00)
            })
            .when(model_index > 0, |row| {
                row.border_t_1().border_color(rgba(
                    (self.tokens.ui.border << 8) | AI_CONTEXT_PROVIDER_ROW_TOP_BORDER_ALPHA,
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_family(settings_mono_font_family(settings))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .overflow_hidden()
                    .child(model.to_string()),
            )
            .child(self.ai_context_source_badge(info.source))
            .child(self.settings_text_input_control(
                input,
                self.current_settings_input_value(input),
                "Auto".to_string(),
                AI_CONTEXT_NUMBER_W,
                cx,
            ))
            .child(
                div()
                    .w(px(AI_CONTEXT_RESET_SLOT_W))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(has_override, |slot| {
                        slot.child(
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
                                ),
                        )
                    }),
            )
            .into_any_element()
    }

    fn ai_context_source_badge(&self, source: ContextWindowSource) -> AnyElement {
        let (text_color, bg_color) = self.ai_context_source_badge_colors(source);
        div()
            .rounded(px(self.tokens.radii.sm))
            .px(px(AI_CONTEXT_SOURCE_BADGE_PX))
            .py(px(AI_CONTEXT_SOURCE_BADGE_PY))
            .text_size(px(AI_CONTEXT_SOURCE_BADGE_TEXT_SIZE))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(text_color)
            .bg(bg_color)
            .child(self.i18n.t(source.i18n_key()))
            .into_any_element()
    }

    fn ai_context_source_badge_colors(&self, source: ContextWindowSource) -> (Rgba, Rgba) {
        match source {
            ContextWindowSource::User => (
                rgb(AI_CONTEXT_SOURCE_USER_COLOR),
                rgba((AI_CONTEXT_SOURCE_USER_COLOR << 8) | AI_CONTEXT_SOURCE_BADGE_BG_ALPHA),
            ),
            ContextWindowSource::Api => (
                rgb(AI_CONTEXT_SOURCE_API_COLOR),
                rgba((AI_CONTEXT_SOURCE_API_COLOR << 8) | AI_CONTEXT_SOURCE_BADGE_BG_ALPHA),
            ),
            ContextWindowSource::Name => (
                rgb(AI_CONTEXT_SOURCE_NAME_COLOR),
                rgba((AI_CONTEXT_SOURCE_NAME_COLOR << 8) | AI_CONTEXT_SOURCE_BADGE_BG_ALPHA),
            ),
            ContextWindowSource::Pattern | ContextWindowSource::Default => (
                rgba((self.tokens.ui.text_muted << 8) | AI_CONTEXT_SOURCE_DEFAULT_TEXT_ALPHA),
                rgba((self.tokens.ui.border << 8) | AI_CONTEXT_SOURCE_DEFAULT_BG_ALPHA),
            ),
        }
    }

    fn ai_active_model_max_response_tokens_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(model) = settings.ai.active_model.clone() else {
            return div().into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.ai_section_title("settings_view.ai.max_response_tokens"))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.max_response_tokens_hint")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .font_family(settings_mono_font_family(settings))
                            .child(format!("{model}:")),
                    )
                    .child(self.settings_text_input_control(
                        SettingsInput::AiActiveModelMaxResponseTokens,
                        self.current_settings_input_value(SettingsInput::AiActiveModelMaxResponseTokens),
                        "Auto".to_string(),
                        128.0,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn ai_tool_expand_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.ai_tool_use_expanded;
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
                this.ai_tool_use_expanded = !this.ai_tool_use_expanded;
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
        let mut items = div().mt(px(12.0)).flex().flex_col().gap(px(8.0));
        for item in group.items {
            let tool_key = item.key.map(str::to_string);
            let checked = item.checked;
            let locked = item.locked;
            items = items.child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                    .bg(rgba((self.tokens.ui.bg << 8) | AI_TOOL_POLICY_ROW_BG_ALPHA))
                    .px(px(10.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(item.label_key))
                    .child(
                        checkbox(&self.tokens, String::new(), checked)
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
                                                settings.ai.tool_use.auto_approve_tools.insert(
                                                    tool_key.clone(),
                                                    serde_json::json!(!checked),
                                                );
                                            },
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                )
                            })
                            .into_any_element(),
                    ),
            );
        }

        div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | AI_TOOL_POLICY_BORDER_ALPHA,
            ))
            .bg(rgba(
                (self.tokens.ui.bg_panel << 8) | AI_TOOL_POLICY_CARD_BG_ALPHA,
            ))
            .p(px(12.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t(group.title_key)),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(group.description_key)),
            )
            .child(items)
            .into_any_element()
    }

    fn ai_disabled_tools_notice(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
        let count = settings.ai.tool_use.disabled_tools.len();
        if count == 0 {
            return div().into_any_element();
        }
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.warning << 8) | 0x33))
            .bg(rgba((self.tokens.ui.warning << 8) | 0x1a))
            .p(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.warning))
                    .child(
                        self.i18n
                            .t("settings_view.ai.tool_use_disabled_tools_title")
                            .replace("{{count}}", &count.to_string()),
                    ),
            )
            .child(
                // Restoring disabled tools is a ghost small Button; share the
                // same toolbar button path as the rest of AI settings actions.
                self.workspace_toolbar_action_button(
                    self.i18n.t("settings_view.ai.tool_use_restore_disabled_tools"),
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
            .into_any_element()
    }

    fn ai_policy_warning(&self) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((self.tokens.ui.warning << 8) | 0x33))
            .bg(rgba((self.tokens.ui.warning << 8) | 0x1a))
            .p(px(12.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(18.0))
            .text_color(rgb(self.tokens.ui.warning))
            .child(self.i18n.t("settings_view.ai.tool_policy_warning"))
            .into_any_element()
    }

    fn ai_mcp_summary_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ai_mcp_servers_section(settings, cx)
    }
}
