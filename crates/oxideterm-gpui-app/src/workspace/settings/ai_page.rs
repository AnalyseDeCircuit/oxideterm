const AI_PROVIDER_SECTION_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70.
const AI_PROVIDER_SECTION_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg/60.
const AI_PROVIDER_MODEL_BORDER_ALPHA: u32 = 0x80; // Tauri border-theme-border/50.
const AI_PROVIDER_MODEL_ACTIVE_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10.
const AI_PROVIDER_MODEL_ACTIVE_BORDER_ALPHA: u32 = 0x99; // Tauri border-theme-accent/60.
const AI_PROVIDER_GRID_MIN_W: f32 = 180.0;
const AI_PROVIDER_SELECT_W: f32 = 224.0; // Tauri w-56.
const AI_PROVIDER_MAX_W: f32 = 768.0; // Tauri max-w-3xl.
const AI_PROVIDER_VISIBLE_MODEL_LIMIT: usize = 8;
const AI_CONFIRM_DIALOG_WIDTH: f32 = 448.0; // Tauri DialogContent max-w-md.
const AI_KEY_REMOVE_DIALOG_WIDTH: f32 = 360.0; // Tauri useConfirm compact title-only dialog.
const AI_CONFIRM_BULLET_SIZE: f32 = 4.0; // Tauri w-1 h-1.

impl WorkspaceApp {
    fn ai_settings_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings_store.settings();
        let mut disabled_body = div()
            .flex()
            .flex_col()
            .opacity(if settings.ai.enabled { 1.0 } else { 0.5 })
            .child(self.ai_execution_profiles_section(settings))
            .child(self.ai_separator())
            .child(self.ai_provider_settings_section(cx))
            .child(self.ai_separator())
            .child(self.ai_context_controls_section(settings, cx))
            .child(self.ai_separator())
            .child(self.ai_system_prompt_section(settings, cx))
            .child(self.ai_separator())
            .child(self.ai_tool_use_section(settings, cx));

        if !settings.ai.enabled {
            disabled_body = disabled_body.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            });
        }

        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .p(px(20.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(16.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.ai.general").to_uppercase()),
            )
            .child(self.ai_enabled_row(settings.ai.enabled, cx))
            .child(self.ai_privacy_notice())
            .child(self.ai_separator())
            .child(disabled_body)
            .into_any_element()
    }

    fn ai_enabled_row(&self, enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .mb(px(24.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.ai.enable")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.enable_hint")),
                    ),
            )
            .child(
                checkbox(&self.tokens, String::new(), enabled)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if !enabled && !this.settings_store.settings().ai.enabled_confirmed {
                                this.show_ai_enable_confirm = true;
                                cx.notify();
                            } else {
                                this.edit_settings(
                                    |settings| set_ai_enabled(settings, !enabled),
                                    cx,
                                );
                            }
                        }),
                    )
                    .into_any_element(),
            )
            .into_any_element()
    }

    fn ai_privacy_notice(&self) -> AnyElement {
        div()
            .mb(px(24.0))
            .p(px(12.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .line_height(px(18.0))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("settings_view.ai.privacy_notice"),
                        self.i18n.t("settings_view.ai.privacy_text")
                    )),
            )
            .into_any_element()
    }

    fn ai_separator(&self) -> AnyElement {
        div()
            .my(px(24.0))
            .child(self.card_separator())
            .into_any_element()
    }

    fn ai_section_title(&self, key: &str) -> AnyElement {
        div()
            .mb(px(16.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(key).to_uppercase())
            .into_any_element()
    }

    fn i18n_count(&self, key: &str, count: usize) -> String {
        self.i18n.t(key).replace("{{count}}", &count.to_string())
    }

    fn ai_i18n_error(&self, key: &str, error: &str) -> String {
        self.i18n.t(key).replace("{{error}}", error)
    }

    fn ai_execution_profiles_section(&self, settings: &PersistedSettings) -> AnyElement {
        div()
            .mb(px(24.0))
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
                    .child(
                        div()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(
                                        self.i18n
                                            .t("settings_view.ai.execution_profiles")
                                            .to_uppercase(),
                                    ),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.i18n.t("settings_view.ai.execution_profiles_hint")),
                            ),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            format!("+ {}", self.i18n.t("settings_view.ai.profile_add")),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: true,
                            },
                        )
                        .into_any_element(),
                    ),
            )
            .child(self.value_row(
                "settings_view.ai.execution_profiles",
                "settings_view.ai.execution_profiles_hint",
                settings
                    .ai
                    .execution_profiles
                    .get("profiles")
                    .and_then(|profiles| profiles.as_array())
                    .map(Vec::len)
                    .unwrap_or(0)
                    .to_string(),
            ))
            .into_any_element()
    }

    fn ai_provider_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let providers = ai_provider_views(self.settings_store.settings());
        let mut provider_list = div()
            .w_full()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(12.0));
        for (index, provider) in providers.into_iter().enumerate() {
            provider_list = provider_list.child(self.ai_provider_card(index, provider, cx));
        }

        let expanded = self.ai_provider_settings_expanded;
        let summary = self.i18n_count(
            "settings_view.ai.provider_settings_summary",
            self.settings_store.settings().ai.providers.len(),
        );

        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .child(
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
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(
                                        self.i18n
                                            .t("settings_view.ai.provider_settings")
                                            .to_uppercase(),
                                    ),
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
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.ai_provider_settings_expanded =
                                !this.ai_provider_settings_expanded;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .when(expanded, |section| {
                section
                    .child(
                        div()
                            .mb(px(24.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(provider_list)
                            .child(self.ai_provider_add_controls(cx)),
                    )
            })
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
                    .child(self.number_row(
                        "settings_view.ai.max_context",
                        "settings_view.ai.max_context_hint",
                        settings.ai.context_max_chars,
                        2000,
                        2000,
                        32000,
                        set_ai_context_max_chars,
                        cx,
                    ))
                    .child(self.number_row(
                        "settings_view.ai.buffer_history",
                        "settings_view.ai.buffer_history_hint",
                        settings.ai.context_visible_lines,
                        20,
                        20,
                        1000,
                        set_ai_context_lines,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(self.bool_row(
                        "settings_view.ai.context_source_ide",
                        "settings_view.ai.context_source_ide_hint",
                        settings.ai.context_sources.ide,
                        set_ai_context_source_ide,
                        cx,
                    ))
                    .child(self.bool_row(
                        "settings_view.ai.context_source_sftp",
                        "settings_view.ai.context_source_sftp_hint",
                        settings.ai.context_sources.sftp,
                        set_ai_context_source_sftp,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn ai_system_prompt_section(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.ai_section_title("settings_view.ai.system_prompt_title"))
            .child(self.value_row(
                "settings_view.ai.custom_system_prompt",
                "settings_view.ai.system_prompt_hint",
                if settings.ai.custom_system_prompt.trim().is_empty() {
                    self.i18n.t("settings_view.ai.system_prompt_placeholder")
                } else {
                    settings.ai.custom_system_prompt.clone()
                },
            ))
            .child(self.bool_row(
                "settings_view.ai.memory_enabled",
                "settings_view.ai.memory_enabled_hint",
                settings.ai.memory.enabled,
                set_ai_memory_enabled,
                cx,
            ))
            .child(self.value_row(
                "settings_view.ai.memory_title",
                "settings_view.ai.memory_hint",
                if settings.ai.memory.content.trim().is_empty() {
                    self.i18n.t("settings_view.ai.memory_placeholder")
                } else {
                    settings.ai.memory.content.clone()
                },
            ))
            .child(self.cycle_row(
                "settings_view.ai.reasoning_title",
                "settings_view.ai.reasoning_hint",
                ai_reasoning_label(settings.ai.reasoning_effort),
                cycle_ai_reasoning,
                cx,
            ))
            .into_any_element()
    }

    fn ai_tool_use_section(&self, settings: &PersistedSettings, cx: &mut Context<Self>) -> AnyElement {
        div()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.ai_section_title("settings_view.ai.tool_use_enabled"))
            .child(self.bool_row(
                "settings_view.ai.tool_use_enabled",
                "settings_view.ai.tool_use_enabled_hint",
                settings.ai.tool_use.enabled,
                set_ai_tool_use_enabled,
                cx,
            ))
            .child(self.number_row(
                "settings_view.ai.tool_use_max_rounds",
                "settings_view.ai.tool_use_max_rounds_hint",
                settings.ai.tool_use.max_rounds.unwrap_or(10),
                1,
                1,
                30,
                set_ai_tool_use_max_rounds,
                cx,
            ))
            .child(self.count_row(
                "settings_view.ai.tool_use_policy_summary",
                "settings_view.ai.tool_use_approve_hint",
                settings.ai.tool_use.auto_approve_tools.len(),
            ))
            .child(self.count_row(
                "settings_view.mcp.title",
                "settings_view.mcp.description",
                settings.ai.mcp_servers.len(),
            ))
            .child(self.value_row(
                "settings_view.ai.embedding_title",
                "settings_view.ai.embedding_description",
                if settings.ai.embedding_config.is_some() {
                    self.i18n.t("settings_view.knowledge.semantic_search_using")
                } else {
                    self.i18n
                        .t("settings_view.knowledge.semantic_search_not_configured")
                },
            ))
            .into_any_element()
    }

    fn ai_provider_add_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = ai_provider_template_by_type(&self.ai_new_provider_type);
        let anchor_id = SettingsSelect::AiProviderTemplate.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, self.i18n.t(selected.label_key), false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select =
                        if this.open_settings_select == Some(SettingsSelect::AiProviderTemplate) {
                            None
                        } else {
                            Some(SettingsSelect::AiProviderTemplate)
                        };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        div()
            .w_full()
            .max_w(px(AI_PROVIDER_MAX_W))
            .flex()
            .flex_wrap()
            .items_end()
            .gap(px(12.0))
            .child(
                div()
                    .grid()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.provider_template")),
                    )
                    .child(
                        div()
                            .relative()
                            .w(px(AI_PROVIDER_SELECT_W))
                            .child(select_anchor_probe(
                                anchor_id,
                                trigger,
                                move |anchor, _window, cx| {
                                    let _ = workspace.update(cx, |this, cx| {
                                        this.update_select_anchor(anchor, cx);
                                    });
                                },
                            )),
                    ),
            )
            .child(
                button_with(
                    &self.tokens,
                    format!("+ {}", self.i18n.t("settings_view.ai.add_provider")),
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
                        this.add_ai_provider_from_selected_template(cx);
                    }),
                ),
            )
            .into_any_element()
    }

    fn ai_provider_card(
        &self,
        index: usize,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.expanded_ai_providers.contains(&provider.id);
        let active_provider = self
            .settings_store
            .settings()
            .ai
            .active_provider_id
            .as_deref()
            == Some(provider.id.as_str());
        let models_expanded = self.expanded_ai_provider_models.contains(&provider.id);
        let visible_models = if models_expanded {
            provider.models.clone()
        } else {
            provider
                .models
                .iter()
                .take(AI_PROVIDER_VISIBLE_MODEL_LIMIT)
                .cloned()
                .collect()
        };
        let hidden_count = provider.models.len().saturating_sub(visible_models.len());

        let mut card = div()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(if active_provider {
                rgba((self.tokens.ui.accent << 8) | 0x99)
            } else {
                rgba((self.tokens.ui.border << 8) | AI_PROVIDER_SECTION_BORDER_ALPHA)
            })
            .bg(if active_provider {
                rgba((self.tokens.ui.accent << 8) | 0x0d)
            } else {
                rgba((self.tokens.ui.bg << 8) | 0xb3)
            })
            .flex()
            .flex_col()
            .child(self.ai_provider_card_header(&provider, active_provider, expanded, cx));

        if expanded {
            card = card
                .child(self.ai_provider_expanded_toolbar(index, &provider, cx))
                .child(self.ai_provider_fields(index, &provider, cx))
                .child(self.ai_provider_models(index, &provider, visible_models, hidden_count, cx));

            if self
                .ai_provider_key_display_state(&provider)
                .shows_key_control()
            {
                card = card.child(self.ai_provider_key_input(index, &provider, cx));
            }
        }

        card.into_any_element()
    }

    fn ai_provider_card_header(
        &self,
        provider: &AiProviderView,
        active_provider: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let provider_key_state = self.ai_provider_key_display_state(provider);
        let provider_has_key = provider_key_state.has_usable_key();
        div()
            .w_full()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .p(px(16.0))
            .cursor_pointer()
            .child(
                div()
                .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(provider.name.clone()),
                            )
                            .child(self.ai_provider_type_badge(provider.provider_type.clone()))
                            .when(active_provider, |row| {
                                row.child(self.ai_provider_badge(
                                    self.i18n.t("settings_view.ai.active"),
                                    self.tokens.ui.accent,
                                    0x33,
                                ))
                            })
                            .child(self.ai_provider_badge(
                                if provider.enabled {
                                    self.i18n.t("settings_view.ai.provider_enabled")
                                } else {
                                    self.i18n.t("settings_view.ai.provider_disabled")
                                },
                                if provider.enabled {
                                    self.tokens.ui.success
                                } else {
                                    self.tokens.ui.text_muted
                                },
                                if provider.enabled { 0x1a } else { 0x33 },
                            )),
                    )
                    .child(
                        div()
                            .mt(px(0.0))
                            .flex()
                            .flex_wrap()
                            .gap_x(px(16.0))
                            .gap_y(px(4.0))
                            .text_size(px(11.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(format!(
                                "{}: {}",
                                self.i18n.t("settings_view.ai.default_model"),
                                if provider.default_model.trim().is_empty() {
                                    "—".to_string()
                                } else {
                                    provider.default_model.clone()
                                }
                            ))
                            .child(self.i18n_count(
                                "settings_view.ai.provider_models_summary",
                                provider.models.len(),
                            ))
                            .when(provider_key_state.shows_key_control(), |row| {
                                row.child(format!(
                                    "{}: {}",
                                    self.i18n.t("settings_view.ai.api_key"),
                                    if provider_has_key {
                                        self.i18n.t("settings_view.ai.api_key_stored")
                                    } else {
                                        self.i18n.t("settings_view.ai.api_key_missing")
                                    }
                                ))
                            }),
                    )
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(!active_provider, |row| {
                        row.child(self.ai_provider_active_button(provider, active_provider, cx))
                    })
                    .child(Self::render_lucide_icon(
                        if expanded {
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
                cx.listener(move |this, _event, _window, cx| {
                    toggle_string_set(&mut this.expanded_ai_providers, &provider_id);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn ai_provider_type_badge(&self, provider_type: String) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgb(self.tokens.ui.bg_panel))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(10.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(provider_type.to_uppercase())
            .into_any_element()
    }

    fn ai_provider_badge(&self, label: String, color: u32, bg_alpha: u32) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((color << 8) | bg_alpha))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(color))
            .child(label)
            .into_any_element()
    }

    fn ai_provider_active_button(
        &self,
        provider: &AiProviderView,
        active_provider: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider = provider.clone();
        let label = if active_provider {
            self.i18n.t("settings_view.ai.active")
        } else {
            self.i18n.t("settings_view.ai.set_active")
        };
        button_with(
            &self.tokens,
            label,
            ButtonOptions {
                variant: if active_provider {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Outline
                },
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.edit_settings(
                    |settings| {
                        ai_set_active_provider_selection(
                            &mut settings.ai.active_provider_id,
                            &mut settings.ai.active_model,
                            &provider,
                        );
                    },
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn ai_provider_enabled_toggle(
        &self,
        index: usize,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        checkbox(&self.tokens, String::new(), enabled)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(
                        |settings| {
                            ai_update_provider(settings, index, |provider| {
                                provider.insert("enabled".to_string(), serde_json::json!(!enabled));
                            });
                        },
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    fn ai_provider_remove_button(
        &self,
        index: usize,
        _name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        button_with(
            &self.tokens,
            self.i18n.t("settings_view.ai.remove"),
            ButtonOptions {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .text_color(rgb(self.tokens.ui.error))
        .hover(|style| style.bg(rgba((self.tokens.ui.error << 8) | 0x1a)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                if let Some(provider_id) = this
                    .settings_store
                    .settings()
                    .ai
                    .providers
                    .get(index)
                    .and_then(ai_provider_id)
                {
                    let _ = this.ai_key_store.delete_provider_key(&provider_id);
                    this.ai_provider_key_status.remove(&provider_id);
                }
                this.edit_settings(
                    |settings| {
                        ai_remove_provider_at(
                            &mut settings.ai.providers,
                            &mut settings.ai.active_provider_id,
                            &mut settings.ai.active_model,
                            index,
                        );
                    },
                    cx,
                );
            }),
        )
        .into_any_element()
    }

    fn ai_provider_expanded_toolbar(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .border_t_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
            .px(px(16.0))
            .pt(px(12.0))
            .pb(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .child(self.ai_provider_enabled_toggle(index, provider.enabled, cx))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.ai.provider_enabled")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.ai_provider_refresh_button(index, provider.clone(), cx))
                    .when(provider.custom, |row| {
                        row.child(self.ai_provider_remove_button(index, provider.name.clone(), cx))
                    }),
            )
            .into_any_element()
    }

    fn ai_provider_refresh_button(
        &self,
        index: usize,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let refreshing = self.ai_model_refreshing.contains(&provider.id);
        button_with(
            &self.tokens,
            self.i18n.t("settings_view.ai.refresh_models"),
            ButtonOptions {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: refreshing,
            },
        )
        .opacity(if refreshing { 0.5 } else { 1.0 })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.refresh_ai_provider_models(index, provider.clone(), cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn ai_provider_fields(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px(px(16.0))
            .pb(px(12.0))
            .grid()
            .grid_cols(2)
            .gap(px(12.0))
            .child(self.ai_provider_field(
                "settings_view.ai.provider_name",
                self.settings_text_input_control(
                    SettingsInput::AiProviderName(index),
                    provider.name.clone(),
                    self.i18n.t("settings_view.ai.provider_name"),
                    AI_PROVIDER_GRID_MIN_W,
                    cx,
                ),
            ))
            .child(self.ai_provider_field(
                "settings_view.ai.base_url",
                self.settings_text_input_control(
                    SettingsInput::AiProviderBaseUrl(index),
                    provider.base_url.clone(),
                    if provider.provider_type == "openai_compatible" {
                        "http://localhost:1234/v1".to_string()
                    } else {
                        String::new()
                    },
                    AI_PROVIDER_GRID_MIN_W,
                    cx,
                ),
            ))
            .child(self.ai_provider_field(
                "settings_view.ai.default_model",
                self.settings_text_input_control(
                    SettingsInput::AiProviderDefaultModel(index),
                    provider.default_model.clone(),
                    self.i18n.t("settings_view.ai.default_model"),
                    AI_PROVIDER_GRID_MIN_W,
                    cx,
                ),
            ))
            .into_any_element()
    }

    fn ai_provider_field(&self, label_key: &str, control: AnyElement) -> AnyElement {
        div()
            .min_w(px(0.0))
            .grid()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(control)
            .into_any_element()
    }

    fn ai_provider_models(
        &self,
        index: usize,
        provider: &AiProviderView,
        visible_models: Vec<String>,
        hidden_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        let models_expanded = self.expanded_ai_provider_models.contains(&provider.id);
        let mut body = div().flex().flex_col().gap(px(6.0)).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(16.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!(
                            "{} ({})",
                            self.i18n.t("settings_view.ai.available_models"),
                            provider.models.len()
                        )),
                )
                .when(provider.models.len() > AI_PROVIDER_VISIBLE_MODEL_LIMIT, |row| {
                    row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(self.tokens.ui.accent))
                            .cursor_pointer()
                            .child(if models_expanded {
                                self.i18n.t("settings_view.ai.show_fewer_models")
                            } else {
                                self.i18n_count(
                                    "settings_view.ai.show_all_models",
                                    provider.models.len(),
                                )
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    toggle_string_set(
                                        &mut this.expanded_ai_provider_models,
                                        &provider_id,
                                    );
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
                }),
        );

        if !visible_models.is_empty() {
            let mut chips = div()
                .px(px(16.0))
                .pb(px(16.0))
                .flex()
                .flex_wrap()
                .gap(px(4.0));
            for model in visible_models {
                chips = chips.child(self.ai_provider_model_chip(
                    index,
                    model.clone(),
                    provider.default_model == model,
                    cx,
                ));
            }
            if hidden_count > 0 {
                chips = chips.child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .text_size(px(10.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(format!("+{hidden_count}")),
                );
            }
            body = body.child(chips);
        }

        body.into_any_element()
    }

fn ai_provider_model_chip(
        &self,
        index: usize,
        model: String,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model_for_edit = model.clone();
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if active {
                rgba((self.tokens.ui.accent << 8) | AI_PROVIDER_MODEL_ACTIVE_BORDER_ALPHA)
            } else {
                rgba((self.tokens.ui.border << 8) | AI_PROVIDER_MODEL_BORDER_ALPHA)
            })
            .bg(if active {
                rgba((self.tokens.ui.accent << 8) | AI_PROVIDER_MODEL_ACTIVE_BG_ALPHA)
            } else {
                rgb(self.tokens.ui.bg)
            })
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(10.0))
            .text_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .cursor_pointer()
            .hover(|style| {
                style
                    .text_color(rgb(self.tokens.ui.text))
                    .border_color(rgb(self.tokens.ui.border))
            })
            .child(model)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(
                        |settings| {
                            ai_set_provider_default_model(
                                &mut settings.ai.providers,
                                index,
                                model_for_edit.clone(),
                            );
                        },
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn ai_provider_has_key(&self, provider_id: &str) -> bool {
        self.ai_provider_key_status
            .get(provider_id)
            .copied()
            .unwrap_or_else(|| self.ai_key_store.has_provider_key(provider_id))
    }

    fn ai_provider_key_display_state(
        &self,
        provider: &AiProviderView,
    ) -> AiProviderKeyDisplayState {
        ai_provider_key_display_state(
            &provider.provider_type,
            self.ai_provider_has_key(&provider.id),
        )
    }

    fn ai_provider_key_input(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.ai_provider_key_display_state(provider) {
            AiProviderKeyDisplayState::Keyless => div().into_any_element(),
            AiProviderKeyDisplayState::Stored => self.ai_provider_stored_key_input(index, provider, cx),
            AiProviderKeyDisplayState::Missing => self.ai_provider_empty_key_input(index, cx),
        }
    }

    fn ai_provider_empty_key_input(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let input = SettingsInput::AiProviderApiKey(index);
        let focused = self.focused_settings_input == Some(input);
        let draft = if focused {
            self.settings_input_draft.as_str()
        } else {
            ""
        };
        let save_disabled = draft.trim().is_empty();
        div()
            .px(px(16.0))
            .pb(px(16.0))
            .grid()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.api_key")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        self.ai_provider_secret_input(
                            input,
                            draft,
                            "sk-...".to_string(),
                            focused,
                            cx,
                        ),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("settings_view.ai.save"),
                            ButtonOptions {
                                variant: ButtonVariant::Secondary,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: save_disabled,
                            },
                        )
                        .h(px(32.0))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.save_ai_provider_api_key(index, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element(),
                    ),
            )
            .into_any_element()
    }

    fn ai_provider_stored_key_input(
        &self,
        index: usize,
        provider: &AiProviderView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let provider_id = provider.id.clone();
        div()
            .px(px(16.0))
            .pb(px(16.0))
            .grid()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.ai.api_key")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .h(px(32.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .rounded(px(self.tokens.radii.sm))
                            .border_1()
                            .border_color(rgba(
                                (self.tokens.ui.border << 8) | AI_PROVIDER_MODEL_BORDER_ALPHA,
                            ))
                            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .italic()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("••••••••••••••••"),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("settings_view.ai.remove"),
                            ButtonOptions {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .h(px(32.0))
                        .text_color(rgb(self.tokens.ui.error))
                        .hover(|style| style.bg(rgba((self.tokens.ui.error << 8) | 0x1a)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.ai_provider_key_remove_confirm =
                                    Some((index, provider_id.clone()));
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                        .into_any_element(),
                    ),
            )
            .into_any_element()
    }

    fn ai_provider_secret_input(
        &self,
        input: SettingsInput,
        value: &str,
        placeholder: String,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: true,
                    selected_all: false,
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w_full()
            .h(px(32.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if this.focused_settings_input != Some(input) {
                        this.focus_settings_input(input, String::new(), cx);
                    }
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

    fn save_ai_provider_api_key(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(provider_id) = self
            .settings_store
            .settings()
            .ai
            .providers
            .get(index)
            .and_then(ai_provider_id)
        else {
            return;
        };
        if self.focused_settings_input != Some(SettingsInput::AiProviderApiKey(index)) {
            cx.notify();
            return;
        }

        // Match Tauri ProviderKeyInput: the visible UI draft is moved into a
        // zeroizing owner before crossing into the keychain boundary, and it is
        // never written into persisted settings.
        let Some(secret) = ai_take_provider_key_secret(&mut self.settings_input_draft) else {
            cx.notify();
            return;
        };
        match self.ai_key_store.store_provider_key(&provider_id, secret) {
            Ok(()) => {
                self.ai_provider_key_status.insert(provider_id.clone(), true);
                self.focused_settings_input = None;
                if let Some(provider) = self
                    .settings_store
                    .settings()
                    .ai
                    .providers
                    .get(index)
                    .and_then(ai_provider_view)
                {
                    self.refresh_ai_provider_models(index, provider, cx);
                }
            }
            Err(error) => {
                self.push_ai_settings_toast(
                    self.ai_i18n_error("settings_view.ai.save_failed", &error.to_string()),
                    TerminalNoticeVariant::Error,
                );
            }
        }
        cx.notify();
    }

    fn remove_ai_provider_api_key(
        &mut self,
        _index: usize,
        provider_id: &str,
        cx: &mut Context<Self>,
    ) {
        match self.ai_key_store.delete_provider_key(provider_id) {
            Ok(()) => {
                self.ai_provider_key_status
                    .insert(provider_id.to_string(), false);
            }
            Err(error) => {
                self.push_ai_settings_toast(
                    self.ai_i18n_error("settings_view.ai.remove_failed", &error.to_string()),
                    TerminalNoticeVariant::Error,
                );
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn refresh_ai_provider_models(
        &mut self,
        index: usize,
        provider: AiProviderView,
        cx: &mut Context<Self>,
    ) {
        if self.ai_model_refreshing.contains(&provider.id) {
            cx.notify();
            return;
        }

        let api_key = match ai_provider_refresh_key_policy(&provider.provider_type) {
            AiProviderRefreshKeyPolicy::NoKey => None,
            AiProviderRefreshKeyPolicy::OptionalStoredKey => {
                self.ai_key_store.get_provider_key(&provider.id).ok().flatten()
            }
            AiProviderRefreshKeyPolicy::RequiredStoredKey => {
                match self.ai_key_store.get_provider_key(&provider.id) {
                    Ok(Some(key)) => Some(key),
                    Ok(None) => {
                        self.ai_provider_key_status.insert(provider.id.clone(), false);
                        self.push_ai_settings_toast(
                            self.i18n.t("settings_view.ai.api_key_missing"),
                            TerminalNoticeVariant::Warning,
                        );
                        cx.notify();
                        return;
                    }
                    Err(error) => {
                        self.push_ai_settings_toast(
                            self.ai_i18n_error(
                                "settings_view.ai.refresh_failed",
                                &error.to_string(),
                            ),
                            TerminalNoticeVariant::Error,
                        );
                        cx.notify();
                        return;
                    }
                }
            }
        };

        self.next_ai_model_refresh_generation =
            self.next_ai_model_refresh_generation.saturating_add(1);
        let generation = self.next_ai_model_refresh_generation;
        self.ai_model_refresh_generations
            .insert(provider.id.clone(), generation);
        self.ai_model_refreshing.insert(provider.id.clone());
        cx.notify();

        let provider_id = provider.id.clone();
        cx.spawn(async move |weak, cx| {
            let result = fetch_provider_models(provider, api_key).await;
            let _ = weak.update(cx, |this, cx| {
                if this.ai_model_refresh_generations.get(&provider_id) != Some(&generation) {
                    return;
                }
                this.ai_model_refreshing.remove(&provider_id);
                match result {
                    Ok(refresh) => {
                        this.edit_settings(
                            |settings| {
                                ai_apply_provider_model_refresh(
                                    &mut settings.ai.providers,
                                    &mut settings.ai.model_context_windows,
                                    index,
                                    &provider_id,
                                    refresh,
                                );
                            },
                            cx,
                        );
                    }
                    Err(error) => {
                        this.push_ai_settings_toast(
                            this.ai_i18n_error(
                                "settings_view.ai.refresh_failed",
                                &error.to_string(),
                            ),
                            TerminalNoticeVariant::Error,
                        );
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn push_ai_settings_toast(
        &mut self,
        title: String,
        variant: TerminalNoticeVariant,
    ) {
        self.workspace_toasts.push(WorkspaceToast {
            notice: TerminalNotice {
                title,
                description: None,
                status_text: None,
                progress: None,
                variant,
            },
            expires_at: Instant::now() + Duration::from_secs(4),
        });
    }

    pub(in crate::workspace) fn render_ai_enable_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        dialog_backdrop()
            .child(
                div()
                    .w(px(AI_CONFIRM_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(14.0))
                            .border_b_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(self.i18n.t("settings_view.ai_confirm.title")),
                            )
                            .child(
                                div()
                                    .mt(px(6.0))
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.i18n.t("settings_view.ai_confirm.description")),
                            ),
                    )
                    .child(
                        div()
                            .p(px(16.0))
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.i18n.t("settings_view.ai_confirm.intro")),
                            )
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                                    .bg(rgba((self.tokens.ui.bg_panel << 8) | 0x4d))
                                    .p(px(12.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_local",
                                    ))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_no_server",
                                    ))
                                    .child(self.ai_confirm_bullet(
                                        "settings_view.ai_confirm.point_context",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(12.0))
                            .border_t_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("settings_view.ai_confirm.cancel"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.show_ai_enable_confirm = false;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("settings_view.ai_confirm.enable"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.edit_settings(
                                            |settings| {
                                                settings.ai.enabled = true;
                                                settings.ai.enabled_confirmed = true;
                                            },
                                            cx,
                                        );
                                        this.show_ai_enable_confirm = false;
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn ai_confirm_bullet(&self, label_key: &str) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .child(
                div()
                    .mt(px(6.0))
                    .size(px(AI_CONFIRM_BULLET_SIZE))
                    .rounded(px(AI_CONFIRM_BULLET_SIZE / 2.0))
                    .bg(rgb(self.tokens.ui.text_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_ai_provider_key_remove_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        dialog_backdrop()
            .child(
                div()
                    .w(px(AI_KEY_REMOVE_DIALOG_WIDTH))
                    .max_w(relative(0.92))
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(self.settings_panel_background(self.tokens.ui.bg_panel))
                    .shadow_lg()
                    .overflow_hidden()
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(14.0))
                            .border_b_1()
                            .border_color(rgb(self.tokens.ui.border))
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child(self.i18n.t("settings_view.ai.remove_confirm")),
                            ),
                    )
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(12.0))
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("common.actions.cancel"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.ai_provider_key_remove_confirm = None;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("settings_view.ai.remove"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Destructive,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        if let Some((index, provider_id)) =
                                            this.ai_provider_key_remove_confirm.take()
                                        {
                                            this.remove_ai_provider_api_key(
                                                index,
                                                &provider_id,
                                                cx,
                                            );
                                        }
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn add_ai_provider_from_selected_template(&mut self, cx: &mut Context<Self>) {
        let template = ai_provider_template_by_type(&self.ai_new_provider_type);
        let now_ms = current_time_millis();
        let id = generated_provider_id(template.provider_type, now_ms);
        let label = self.i18n.t(template.label_key);
        self.edit_settings(
            |settings| {
                ai_add_provider_from_template(
                    &mut settings.ai.providers,
                    &mut settings.ai.active_provider_id,
                    &mut settings.ai.active_model,
                    template,
                    id,
                    label,
                    now_ms,
                );
            },
            cx,
        );
    }
}

fn ai_provider_views(settings: &PersistedSettings) -> Vec<AiProviderView> {
    ai_provider_views_from_values(&settings.ai.providers)
}

fn ai_update_provider(
    settings: &mut PersistedSettings,
    index: usize,
    update: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) {
    ai_update_provider_values(&mut settings.ai.providers, index, update);
}

fn toggle_string_set(set: &mut HashSet<String>, value: &str) {
    if !set.remove(value) {
        set.insert(value.to_string());
    }
}

fn current_time_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
