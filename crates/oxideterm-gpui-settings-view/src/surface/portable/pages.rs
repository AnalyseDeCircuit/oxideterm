use super::*;
use crate::{
    appearance_page::CoreAppearancePage,
    components::{SettingsWindowDispatch, TerminalPreviewFont, settings_mono_font_family},
    help_page::{HelpPresentation, HelpRequest, SettingsMarkdownDialog},
    keybinding_page::{KeybindingPageView, KeybindingRequest, KeybindingTextRole},
    proxy_page::ProxyPresentation,
    select_menu::{CoreSelectMenu, settings_select_popover},
    terminal_page::TerminalSettingsSection,
    theme_editor::{ThemeEditorBody, ThemeEditorFrame},
};
use oxideterm_gpui_ui::motion::ExitPhase;

pub(super) fn tokens(settings: &PersistedSettings, dark: bool) -> ThemeTokens {
    use oxideterm_theme::{UiDensityProfile, UiRadii};
    // Only RGB swatches are needed here. The runtime terminal parser interns selection colors.
    let app = oxideterm_settings_model::ThemeTarget::Application.resolved_id(settings, dark);
    let terminal = oxideterm_settings_model::ThemeTarget::Terminal.resolved_id(settings, dark);
    let mut tokens = ThemeTokens::from_builtin(oxideterm_theme::theme_by_id(app));
    tokens.ui = oxideterm_settings_model::theme_ui_colors(settings, app);
    tokens.terminal = crate::theme_picker::appearance_theme_palette(settings, terminal);
    tokens.refresh_palette_metrics();
    let radius = settings.appearance.border_radius as f32;
    tokens.radii = UiRadii {
        xs: (radius - 4.0).max(0.0),
        sm: (radius - 2.0).max(0.0),
        md: radius,
        lg: radius + 4.0,
        active_indicator: radius.clamp(1.0, 2.0),
    };
    tokens.apply_density(match settings.appearance.ui_density {
        oxideterm_settings::UiDensity::Compact => UiDensityProfile::Compact,
        oxideterm_settings::UiDensity::Comfortable => UiDensityProfile::Comfortable,
        oxideterm_settings::UiDensity::Spacious => UiDensityProfile::Spacious,
    });
    tokens.apply_ui_font_scale(
        settings.appearance.ui_font_size as f32 / oxideterm_settings::DEFAULT_UI_FONT_SIZE as f32,
    );
    tokens.apply_motion(match settings.appearance.animation_speed {
        oxideterm_settings::AnimationSpeed::Off => oxideterm_theme::UiMotionProfile::Off,
        oxideterm_settings::AnimationSpeed::Reduced => oxideterm_theme::UiMotionProfile::Reduced,
        oxideterm_settings::AnimationSpeed::Normal => oxideterm_theme::UiMotionProfile::Normal,
        oxideterm_settings::AnimationSpeed::Fast => oxideterm_theme::UiMotionProfile::Fast,
    });
    tokens
}

fn font_preview(settings: &PersistedSettings, cx: &App) -> TerminalPreviewFont {
    use oxideterm_gpui_terminal_view::metrics::{
        resolve_portable_terminal_font, terminal_letter_spacing_for_font,
        terminal_line_height_for_font,
    };
    let term = &settings.terminal;
    let resolved = resolve_portable_terminal_font(cx.text_system(), term);
    let size = px(term.font_size as f32);
    let spacing = (term.character_spacing
        * oxideterm_settings::TERMINAL_CHARACTER_SPACING_BASELINE_SCALE) as f32;
    TerminalPreviewFont {
        line_height: terminal_line_height_for_font(
            cx.text_system(),
            &resolved.font,
            size,
            term.line_height as f32,
        ),
        letter_spacing: terminal_letter_spacing_for_font(
            cx.text_system(),
            &resolved.font,
            size,
            spacing,
        ),
        font: resolved.font,
        font_size: size,
        unavailable_families: resolved.unavailable_families,
    }
}

pub(super) fn render_section(
    owner: &Entity<PageState>,
    tokens: &ThemeTokens,
    dark: bool,
    section: SettingsSectionId,
    dispatch: SettingsWindowDispatch,
    cx: &App,
) -> AnyElement {
    use SettingsSectionId::*;
    let state = owner.read(cx);
    let c = state.controls(tokens, dispatch);
    match section {
        GeneralLanguage | GeneralWindowBehavior | GeneralExternalConnectionUris => {
            c.general_preference_section(section, cx)
        }
        AiGeneralSettings | AiGeneralPrivacy | AiToolsWebSearch => {
            crate::ai::basic::basic_section(&c, section, true, cx)
        }
        AiContextControls => {
            let enabled = state.settings.ai.enabled;
            let card = c.settings_card(
                "settings_view.ai.context_controls",
                "settings_view.ai.context_automatic_hint",
                vec![
                    c.bool_row(
                        "settings_view.ai.context_source_ide",
                        "settings_view.ai.context_source_ide_hint",
                        state.settings.ai.context_sources.ide,
                        crate::options::set_ai_context_source_ide,
                        cx,
                    ),
                    c.card_separator(),
                    c.bool_row(
                        "settings_view.ai.context_source_sftp",
                        "settings_view.ai.context_source_sftp_hint",
                        state.settings.ai.context_sources.sftp,
                        crate::options::set_ai_context_source_sftp,
                        cx,
                    ),
                ],
            );
            crate::ai::available(card, enabled)
        }
        AiContextSystemPrompt => {
            let enabled = state.settings.ai.enabled;
            let has_custom = !state.settings.ai.custom_system_prompt.trim().is_empty();
            let card = c.settings_card(
                "settings_view.ai.system_prompt_title",
                "settings_view.ai.system_prompt_hint",
                vec![c.setting_row(
                    if has_custom {
                        "settings_view.ai.system_prompt_status_custom"
                    } else {
                        "settings_view.ai.system_prompt_status_default"
                    },
                    "",
                    c.settings_text_input_control(
                        SettingsInput::AiSystemPrompt,
                        state.settings.ai.custom_system_prompt.clone(),
                        state.i18n.t("settings_view.ai.system_prompt_hint"),
                        360.0,
                        cx,
                    ),
                    cx,
                )],
            );
            crate::ai::available(card, enabled)
        }
        AiContextMemory => {
            let enabled = state.settings.ai.enabled;
            let has_saved = !state.settings.ai.memory.content.trim().is_empty()
                || state
                    .settings
                    .ai
                    .memory
                    .entries
                    .iter()
                    .any(|entry| !entry.content.trim().is_empty());
            let card = c.settings_card(
                "settings_view.ai.memory_title",
                "settings_view.ai.memory_hint",
                vec![
                    c.bool_row(
                        "settings_view.ai.memory_enabled",
                        "settings_view.ai.memory_enabled_hint",
                        state.settings.ai.memory.enabled,
                        crate::options::set_ai_memory_enabled,
                        cx,
                    ),
                    c.card_separator(),
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(state.i18n.t(if has_saved {
                            "settings_view.ai.memory_status_saved"
                        } else {
                            "settings_view.ai.memory_status_empty"
                        }))
                        .into_any_element(),
                ],
            );
            crate::ai::available(card, enabled)
        }
        AiContextModelWindows => {
            let enabled = state.settings.ai.enabled;
            let providers = oxideterm_settings_model::ai_provider_views(&state.settings);
            let mut rows: Vec<AnyElement> = Vec::new();
            for provider in &providers {
                for model in &provider.models {
                    if !rows.is_empty() {
                        rows.push(c.card_separator());
                    }
                    rows.push(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(tokens.metrics.ui_text_sm))
                                    .text_color(rgb(tokens.ui.text))
                                    .child(format!("{} · {}", provider.name, model)),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .font_family("Lilex")
                                    .text_size(px(tokens.metrics.ui_text_xs))
                                    .text_color(rgb(tokens.ui.text_muted))
                                    .child("128K"),
                            )
                            .into_any_element(),
                    );
                }
            }
            if rows.is_empty() {
                rows.push(
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(state.i18n.t("settings_view.ai.model_context_windows_empty"))
                        .into_any_element(),
                );
            }
            let card = c.settings_card(
                "settings_view.ai.model_context_windows",
                "settings_view.ai.model_context_windows_hint",
                rows,
            );
            crate::ai::available(card, enabled)
        }
        AiToolsToolUse => {
            let enabled = state.settings.ai.enabled;
            let approved = oxideterm_settings_model::ai_tool_auto_approved_count(&state.settings);
            let total = oxideterm_settings_model::ai_tool_auto_approve_total_count(&state.settings);
            let expanded = state.ai_tool_policy_expanded;
            let toggle_owner = owner.downgrade();
            let expand_btn = oxideterm_gpui_ui::button::button_with(
                tokens,
                state.i18n.t(if expanded {
                    "settings_view.ai.tool_use_collapse"
                } else {
                    "settings_view.ai.tool_use_expand"
                }),
                oxideterm_gpui_ui::button::ButtonOptions {
                    variant: oxideterm_gpui_ui::button::ButtonVariant::Outline,
                    size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                    radius: oxideterm_gpui_ui::button::ButtonRadius::Md,
                    disabled: false,
                },
            )
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                let _ = toggle_owner.update(cx, |s, cx| {
                    s.ai_tool_policy_expanded = !s.ai_tool_policy_expanded;
                    cx.notify();
                });
                cx.stop_propagation();
            })
            .into_any_element();

            let controls = c.settings_card(
                "settings_view.ai.tool_use",
                "",
                vec![
                    c.select_setting_row(
                        "settings_view.ide.agent_mode_label",
                        "settings_view.ide.agent_mode_hint",
                        SettingsSelect::IdeAgentMode,
                        crate::labels::ide_agent_label(state.settings.ide.agent_mode, &state.i18n),
                        tokens.metrics.settings_select_width,
                        cx,
                    ),
                    c.card_separator(),
                    c.bool_row(
                        "settings_view.ai.tool_use_enabled",
                        "settings_view.ai.tool_use_enabled_hint",
                        state.settings.ai.tool_use.enabled,
                        crate::options::set_ai_tool_use_enabled,
                        cx,
                    ),
                    c.card_separator(),
                    c.number_row(
                        "settings_view.ai.tool_use_max_rounds",
                        "settings_view.ai.tool_use_max_rounds_hint",
                        SettingsInput::AiToolUseMaxRounds,
                        state
                            .settings
                            .ai
                            .tool_use
                            .max_rounds
                            .unwrap_or(oxideterm_settings::DEFAULT_AI_TOOL_MAX_ROUNDS),
                        cx,
                    ),
                ],
            );

            let policy_rows = if !expanded {
                vec![crate::ai::settings_ai_tool_collapsed_summary(
                    tokens,
                    format!(
                        "{} · {}",
                        state.i18n.t("settings_view.ai.tool_use_policy_summary"),
                        state
                            .i18n
                            .t("settings_view.ai.tool_use_collapsed_summary")
                            .replace("{{approved}}", &approved.to_string())
                            .replace("{{total}}", &total.to_string())
                    ),
                )]
            } else {
                let group_cards: Vec<AnyElement> =
                    oxideterm_settings_model::ai_tool_policy_groups(&state.settings)
                        .into_iter()
                        .enumerate()
                        .map(|(group_idx, group)| {
                            let state_kind = group.state();
                            let locked = state_kind
                                == oxideterm_settings_model::AiToolPolicyGroupState::Locked;
                            let all_enabled = state_kind
                                == oxideterm_settings_model::AiToolPolicyGroupState::AllApproved;
                            let edit_owner = owner.downgrade();
                            let title = state.i18n.t(group.title_key);
                            let trailing = if locked {
                                div()
                                    .text_size(px(tokens.metrics.ui_text_xs))
                                    .text_color(rgb(tokens.ui.text_muted))
                                    .child(state.i18n.t("settings_view.ai.tool_fixed_policy"))
                                    .into_any_element()
                            } else {
                                oxideterm_gpui_ui::switch::switch(
                                    tokens,
                                    format!("ai-policy-group-{group_idx}"),
                                    String::new(),
                                    all_enabled,
                                )
                                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                    let _ = edit_owner.update(cx, |s, cx| {
                                        oxideterm_settings_model::set_ai_tool_policy_group_approval(
                                            &mut s.settings,
                                            &group,
                                            !all_enabled,
                                        );
                                        s.changed(cx);
                                    });
                                    cx.stop_propagation();
                                })
                                .into_any_element()
                            };
                            div()
                                .w_full()
                                .p_3()
                                .rounded(px(tokens.radii.md))
                                .border_1()
                                .border_color(gpui::rgba((tokens.ui.border << 8) | 0x80))
                                .bg(rgb(tokens.ui.bg_sunken))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .text_size(px(tokens.metrics.ui_text_xs))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(tokens.ui.text))
                                        .child(title),
                                )
                                .child(trailing)
                                .into_any_element()
                        })
                        .collect();
                vec![crate::ai::settings_ai_tool_expanded_body(
                    tokens,
                    state.settings.ai.tool_use.enabled,
                    vec![
                        div()
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(state.i18n.t("settings_view.ai.tool_use_approve_hint"))
                            .into_any_element(),
                        crate::ai::settings_ai_tool_policy_grid(group_cards),
                        crate::ai::settings_ai_policy_warning(
                            tokens,
                            state.i18n.t("settings_view.ai.tool_policy_warning"),
                        ),
                    ],
                )]
            };
            let policy = c.settings_card_with_header(
                "settings_view.ai.tool_auto_approval_title",
                "",
                None,
                Some(expand_btn),
                policy_rows,
            );
            let stack = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(tokens.metrics.settings_page_gap))
                .child(controls)
                .child(policy)
                .into_any_element();
            crate::ai::available(stack, enabled)
        }
        AiToolsAgents => {
            let enabled = state.settings.ai.enabled;
            let options: oxideterm_ai::agent::AgentOptions = state
                .settings
                .ai
                .extra
                .get("subagents")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let concurrency = state
                .settings
                .ai
                .extra
                .get("subagentConcurrency")
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as usize)
                .unwrap_or(oxideterm_ai::agent::DEFAULT_AGENT_CONCURRENCY)
                .clamp(1, oxideterm_ai::agent::MAX_AGENT_CONCURRENCY);
            let card = c.settings_card(
                "ai.agents.settings_title",
                "ai.agents.settings_hint",
                vec![
                    c.bool_row(
                        "ai.agents.allow",
                        "ai.agents.allow_hint",
                        options.enabled,
                        |settings, next| {
                            let mut opts: oxideterm_ai::agent::AgentOptions = settings
                                .ai
                                .extra
                                .get("subagents")
                                .and_then(|v| serde_json::from_value(v.clone()).ok())
                                .unwrap_or_default();
                            opts.enabled = next;
                            if let Ok(val) = serde_json::to_value(opts) {
                                settings.ai.extra.insert("subagents".into(), val);
                            }
                        },
                        cx,
                    ),
                    c.card_separator(),
                    c.setting_row(
                        "ai.agents.concurrency",
                        "ai.agents.concurrency_hint",
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .children((1..=oxideterm_ai::agent::MAX_AGENT_CONCURRENCY).map(|n| {
                                let active = n == concurrency;
                                let edit_owner = owner.downgrade();
                                div()
                                    .id(("ai-agent-concurrency", n))
                                    .px_2()
                                    .py_1()
                                    .rounded(px(tokens.radii.sm))
                                    .border_1()
                                    .border_color(if active {
                                        rgb(tokens.ui.accent)
                                    } else {
                                        gpui::rgba((tokens.ui.border << 8) | 0x80)
                                    })
                                    .bg(if active {
                                        gpui::rgba((tokens.ui.accent << 8) | 0x26)
                                    } else {
                                        rgb(tokens.ui.bg_sunken)
                                    })
                                    .font_family("Lilex")
                                    .text_size(px(tokens.metrics.ui_text_xs))
                                    .text_color(rgb(if active {
                                        tokens.ui.text
                                    } else {
                                        tokens.ui.text_muted
                                    }))
                                    .cursor_pointer()
                                    .child(n.to_string())
                                    .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                        let _ = edit_owner.update(cx, |s, cx| {
                                            s.settings.ai.extra.insert(
                                                "subagentConcurrency".into(),
                                                serde_json::json!(n),
                                            );
                                            s.changed(cx);
                                        });
                                        cx.stop_propagation();
                                    })
                            }))
                            .into_any_element(),
                        cx,
                    ),
                ],
            );
            crate::ai::available(card, enabled)
        }
        AiToolsSkills => {
            let enabled = state.settings.ai.enabled;
            let card = c.settings_card(
                "settings_view.ai.skills_title",
                "settings_view.ai.skills_hint",
                vec![
                    div()
                        .text_size(px(tokens.metrics.ui_text_xs))
                        .text_color(rgb(tokens.ui.text_muted))
                        .child(state.i18n.t("settings_view.ai.skills_empty"))
                        .into_any_element(),
                ],
            );
            crate::ai::available(card, enabled)
        }
        AiToolsMcpServers => {
            let enabled = state.settings.ai.enabled;
            let card = c.settings_card(
                "settings_view.ai.mcp_title",
                "settings_view.ai.mcp_hint",
                vec![],
            );
            crate::ai::available(card, enabled)
        }
        PortableRuntime => c.settings_card(
            "settings_view.general.portable_runtime",
            "",
            vec![
                div()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgb(tokens.ui.text_muted))
                    .line_height(px(18.0))
                    .child(
                        state
                            .i18n
                            .t("settings_view.general.portable_activation_disabled"),
                    )
                    .into_any_element(),
            ],
        ),
        PrivilegeCredentials => c.settings_card(
            "settings_view.privilege_credentials.scopes",
            "",
            vec![
                div()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgb(tokens.ui.text_muted))
                    .line_height(px(18.0))
                    .child(state.i18n.t("sessionManager.privilege_credentials.empty"))
                    .into_any_element(),
            ],
        ),
        KnowledgeCollections | KnowledgeEmbeddingConfig => {
            let text_row = |label: String| {
                div()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgb(tokens.ui.text_muted))
                    .child(label)
                    .into_any_element()
            };
            let list = section == KnowledgeCollections;
            let rows = match &state.knowledge {
                PortableKnowledgeLibrary::Ready { collections, .. }
                    if list && !collections.is_empty() =>
                {
                    collections.iter().cloned().map(text_row).collect()
                }
                PortableKnowledgeLibrary::Ready { keyword_ready, .. } if !list => {
                    let mut rows = Vec::new();
                    if *keyword_ready {
                        rows.push(text_row(
                            state.i18n.t("settings_view.knowledge.keyword_search_ready"),
                        ));
                    }
                    rows.push(text_row(
                        state.i18n.t("settings_view.knowledge.keyword_only"),
                    ));
                    rows
                }
                status => vec![text_row(state.i18n.t(match status {
                    PortableKnowledgeLibrary::Unavailable => {
                        "settings_view.knowledge.library_unavailable"
                    }
                    PortableKnowledgeLibrary::Loading => "settings_view.known_hosts.loading",
                    PortableKnowledgeLibrary::Failed => {
                        "settings_view.knowledge.navigator_load_failed"
                    }
                    PortableKnowledgeLibrary::Ready { .. } => {
                        "settings_view.knowledge.no_collections"
                    }
                }))],
            };
            if list {
                c.settings_card(
                    "settings_view.knowledge.collections",
                    "settings_view.knowledge.create_description",
                    rows,
                )
            } else {
                c.settings_card(
                    "settings_view.knowledge.semantic_search",
                    "settings_view.knowledge.semantic_search_description",
                    rows,
                )
            }
        }
        ConnectionsKnownHosts => {
            let hosts = &state.known_hosts;
            let owner = owner.downgrade();
            c.known_hosts_list(
                &hosts.snapshot,
                &hosts.query,
                hosts.selected.as_ref(),
                &hosts.scroll,
                hosts.handler.is_some(),
                None,
                Rc::new(move |request, cx| {
                    let _ = owner.update(cx, |s, cx| s.known_hosts_action(request, cx));
                }),
                cx,
            )
        }
        NetworkSharedProxy | NetworkRouting => {
            let owner = owner.downgrade();
            ProxyPresentation {
                controls: c,
                password: None,
                probe: state.probe.form.clone(),
                can_probe: state.probe.handler.is_some(),
                dispatch: Rc::new(move |request, cx| {
                    let _ = owner.update(cx, |s, cx| s.proxy_action(request, cx));
                }),
            }
            .section(section, cx)
        }
        HelpVersion | HelpDiagnostics | HelpTechStack | HelpResources | HelpSafety | HelpLegal => {
            let owner = owner.downgrade();
            let page = HelpPresentation {
                controls: c,
                year: &chrono::Utc::now().format("%Y").to_string(),
                dispatch: Rc::new(move |request, cx| match request {
                    HelpRequest::Url(url) => cx.open_url(url),
                    HelpRequest::Legal | HelpRequest::ThirdParty => {
                        let _ = owner.update(cx, |s, cx| {
                            s.fields.blur();
                            s.document = Some((
                                matches!(request, HelpRequest::ThirdParty),
                                Default::default(),
                            ));
                            cx.notify();
                        });
                    }
                    _ => unreachable!("desktop help operations are not mounted"),
                }),
            };
            if state.host == SettingsHost::Browser {
                page.browser_section(section,state.browser_report.clone(),cx)
            } else {
                page.section(section,cx)
            }
        }
        SftpOpening | SftpPerformance | SftpConflict => c.settings_sftp_section(section, cx),
        IdeEditing | IdeTypography | IdeAgent | IdeAgentPrivacy => {
            c.settings_ide_section(section, state.ide_custom, cx)
        }
        AppearanceTheme | AppearanceBackground | AppearanceLayout | AppearanceEffects | AppearanceAppIcon => {
            let settings_owner=owner;
            let owner = owner.downgrade();
            let page = CoreAppearancePage {
                controls: c,
                system_dark: dark,
                edit_scheme: state.edit_scheme(dark),
                onboarding: false,
                desktop: false,
                dispatch: Rc::new(move |action, cx| {
                    let _ = owner.update(cx, |s, cx| s.appearance(action, dark, cx));
                }),
            };
            match section {
                AppearanceBackground => wallpaper::card(&page,settings_owner,cx),
                AppearanceTheme => {
                    let rows = page.appearance_theme_controls_with_hints(&state.settings, true, cx);
                    page.appearance_card(state.i18n.t("settings_view.appearance.theme"), None, rows)
                }
                AppearanceLayout => page.appearance_layout_card(&state.settings, cx),
                AppearanceEffects => {
                    let rows = vec![page.setting_row(
                        "settings_view.appearance.animation",
                        "settings_view.appearance.animation_hint",
                        page.settings_select_control(
                            SettingsSelect::AppearanceAnimation,
                            crate::animation_label(
                                state.settings.appearance.animation_speed,
                                &state.i18n,
                            ),
                            false,
                            Some(tokens.metrics.settings_select_width),
                            cx,
                        ),
                        cx,
                    )];
                    page.appearance_card(
                        state.i18n.t("settings_view.appearance.effects"),
                        None,
                        rows,
                    )
                }
                AppearanceAppIcon => {
                    if let Some((variants, size, source)) = state.app_icon_picker {
                        page.appearance_app_icon_card(&state.settings, variants, size, source)
                    } else {
                        div().into_any_element()
                    }
                }
                _ => unreachable!(),
            }
        }
        KeybindingsToolbar | KeybindingsScope(_) => keybindings(owner, &c, false, section, cx),
        TerminalLocalShell => c.settings_card(
            "settings_view.local_terminal.shell",
            "",
            vec![
                c.setting_row(
                    "settings_view.local_terminal.default_cwd",
                    "settings_view.local_terminal.default_cwd_hint",
                    c.settings_text_input_control(
                        SettingsInput::LocalDefaultCwd,
                        state
                            .settings
                            .local_terminal
                            .default_cwd
                            .clone()
                            .unwrap_or_default(),
                        "~".to_string(),
                        tokens.metrics.settings_select_width,
                        cx,
                    ),
                    cx,
                ),
            ],
        ),
        TerminalLocalShellProfile => c.settings_card(
            "settings_view.local_terminal.shell_profile",
            "",
            vec![c.bool_row(
                "settings_view.local_terminal.load_shell_profile",
                "settings_view.local_terminal.load_shell_profile_hint",
                state.settings.local_terminal.load_shell_profile,
                crate::options::set_load_shell_profile,
                cx,
            )],
        ),
        TerminalLocalPrivilegeCredentials => c.settings_card(
            "settings_view.privilege_credentials.scopes",
            "",
            vec![
                div()
                    .text_size(px(tokens.metrics.ui_text_xs))
                    .text_color(rgb(tokens.ui.text_muted))
                    .child(state.i18n.t("sessionManager.privilege_credentials.empty"))
                    .into_any_element(),
            ],
        ),
        TerminalCommandBarControls => {
            let s = &state.settings.terminal;
            let rows = vec![
                c.bool_row(
                    "settings_view.terminal.command_bar",
                    "settings_view.terminal.command_bar_hint",
                    s.command_bar.enabled,
                    crate::options::set_command_bar_enabled,
                    cx,
                ),
                c.bool_row(
                    "settings_view.terminal.command_bar_git_status",
                    "settings_view.terminal.command_bar_git_status_hint",
                    s.command_bar.git_status,
                    crate::options::set_command_bar_git_status,
                    cx,
                ),
                c.bool_row(
                    "settings_view.terminal.command_bar_project_tasks",
                    "settings_view.terminal.command_bar_project_tasks_hint",
                    s.command_bar.project_tasks,
                    crate::options::set_command_bar_project_tasks,
                    cx,
                ),
                c.bool_row(
                    "settings_view.terminal.command_bar_ssh_latency",
                    "settings_view.terminal.command_bar_ssh_latency_hint",
                    s.command_bar.ssh_latency,
                    crate::options::set_command_bar_ssh_latency,
                    cx,
                ),
                c.select_setting_row(
                    "settings_view.terminal.ssh_latency_interval",
                    "settings_view.terminal.ssh_latency_interval_hint",
                    SettingsSelect::SshLatencyInterval,
                    s.command_bar.ssh_latency_interval_secs.to_string(),
                    tokens.metrics.settings_select_narrow_width,
                    cx,
                ),
                c.bool_row(
                    "settings_view.terminal.command_bar_current_directory_awareness",
                    "settings_view.terminal.command_bar_current_directory_awareness_hint",
                    s.command_bar.show_current_directory,
                    crate::options::set_command_bar_show_current_directory,
                    cx,
                ),
                c.bool_row(
                    "settings_view.terminal.autosuggest_local_history",
                    "settings_view.terminal.autosuggest_local_history_hint",
                    s.autosuggest.local_shell_history,
                    crate::options::set_autosuggest_local_history,
                    cx,
                ),
                c.bool_row(
                    "terminal.history.save",
                    "terminal.history.save_hint",
                    s.autosuggest.save_history,
                    crate::options::set_save_command_history,
                    cx,
                ),
                c.number_row(
                    "terminal.history.retention",
                    "terminal.history.retention_hint",
                    SettingsInput::CommandHistoryRetention,
                    i64::from(s.autosuggest.history_retention_days),
                    cx,
                ),
            ];
            c.settings_card(
                "settings_view.terminal.command_bar",
                "",
                c.separated_settings_rows(rows),
            )
        }
        TerminalCommandBarFocusHandoff => c.settings_card(
            "settings_view.terminal.command_bar_focus_handoff",
            "settings_view.terminal.command_bar_focus_handoff_hint",
            vec![],
        ),
        TerminalCommandBarQuickCommands => {
            let s = &state.settings.terminal.command_bar;
            c.settings_card(
                "settings_view.terminal.quick_commands",
                "",
                vec![
                    c.bool_row(
                        "settings_view.terminal.quick_commands",
                        "settings_view.terminal.quick_commands_hint",
                        s.quick_commands_enabled,
                        crate::options::set_quick_commands_enabled,
                        cx,
                    ),
                    c.card_separator(),
                    c.bool_row(
                        "settings_view.terminal.quick_bar",
                        "settings_view.terminal.quick_bar_hint",
                        s.quick_bar_enabled,
                        crate::options::set_quick_bar_enabled,
                        cx,
                    ),
                    c.card_separator(),
                    c.bool_row(
                        "settings_view.terminal.quick_commands_confirm",
                        "settings_view.terminal.quick_commands_confirm_hint",
                        s.quick_commands_confirm_before_run,
                        crate::options::set_quick_commands_confirm,
                        cx,
                    ),
                    c.card_separator(),
                    c.bool_row(
                        "settings_view.terminal.quick_commands_toast",
                        "settings_view.terminal.quick_commands_toast_hint",
                        s.quick_commands_show_toast,
                        crate::options::set_quick_commands_toast,
                        cx,
                    ),
                ],
            )
        }
        TerminalAwarenessControls => c.settings_card(
            "settings_view.terminal.awareness_title",
            "settings_view.terminal.awareness_description",
            vec![
                c.bool_row(
                    "settings_view.terminal.awareness_enabled",
                    "settings_view.terminal.awareness_enabled_hint",
                    state
                        .settings
                        .terminal
                        .command_bar
                        .current_directory_awareness,
                    crate::options::set_command_bar_current_directory_awareness,
                    cx,
                ),
                c.card_separator(),
                c.select_setting_row(
                    "settings_view.connections.shell_integration.mode_label",
                    "settings_view.connections.shell_integration.mode_hint",
                    SettingsSelect::RemoteShellIntegrationMode,
                    crate::labels::remote_shell_integration_mode_label(
                        state.settings.terminal.remote_shell_integration_mode,
                        &state.i18n,
                    ),
                    tokens.metrics.settings_select_width,
                    cx,
                ),
            ],
        ),
        TerminalAwarenessShellIntegration => c.settings_card(
            "settings_view.connections.shell_integration.mode_label",
            "settings_view.connections.shell_integration.mode_hint",
            vec![],
        ),
        TerminalAwarenessTriggers => c.settings_card(
            "settings_view.terminal.triggers.title",
            "settings_view.terminal.triggers.description",
            vec![],
        ),
        TerminalHighlightSelection => c.settings_card(
            "terminal.highlight_override.selection_matches",
            "",
            vec![c.bool_row(
                "terminal.highlight_override.selection_matches",
                "terminal.highlight_override.selection_matches_default_hint",
                state.settings.terminal.selection_highlighting,
                |settings, enabled| settings.terminal.selection_highlighting = enabled,
                cx,
            )],
        ),
        TerminalHighlightSemantic => c.settings_card(
            "settings_view.terminal.highlight_rules.semantic_coloring",
            "settings_view.terminal.highlight_rules.semantic_coloring_hint",
            vec![
                c.bool_row(
                    "settings_view.terminal.highlight_rules.semantic_coloring",
                    "settings_view.terminal.highlight_rules.semantic_coloring_hint",
                    state.settings.terminal.semantic_coloring,
                    |settings, enabled| settings.terminal.semantic_coloring = enabled,
                    cx,
                ),
                c.card_separator(),
                c.select_setting_row(
                    "settings_view.terminal.highlight_rules.semantic_scheme",
                    "settings_view.terminal.highlight_rules.semantic_scheme_hint",
                    SettingsSelect::TerminalSemanticScheme,
                    crate::options::terminal_semantic_scheme_label(
                        state.settings.terminal.semantic_scheme,
                        &state.i18n,
                    ),
                    tokens.metrics.settings_select_width,
                    cx,
                ),
            ],
        ),
        TerminalHighlightRules => {
            let count = state.settings.terminal.highlight_rules.len();
            let rows = if count == 0 {
                vec![]
            } else {
                state
                    .settings
                    .terminal
                    .highlight_rules
                    .iter()
                    .enumerate()
                    .map(|(index, rule)| {
                        let edit_owner = owner.downgrade();
                        let enabled = rule.enabled;
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(tokens.metrics.ui_text_xs))
                                    .text_color(rgb(tokens.ui.text))
                                    .child(format!("{} · {}", rule.label, rule.pattern)),
                            )
                            .child(
                                oxideterm_gpui_ui::switch::switch(
                                    tokens,
                                    format!("highlight-rule-{index}"),
                                    String::new(),
                                    enabled,
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    move |_, _, cx| {
                                        let _ = edit_owner.update(cx, |s, cx| {
                                            if let Some(r) =
                                                s.settings.terminal.highlight_rules.get_mut(index)
                                            {
                                                r.enabled = !r.enabled;
                                                s.changed(cx);
                                            }
                                        });
                                        cx.stop_propagation();
                                    },
                                ),
                            )
                            .into_any_element()
                    })
                    .collect()
            };
            c.settings_card(
                "settings_view.terminal.highlight_rules.title",
                "settings_view.terminal.highlight_rules.description",
                c.separated_settings_rows(rows),
            )
        }
        _ => c.terminal_settings_section(
            match section {
                TerminalDisplayFont => TerminalSettingsSection::Font {
                    custom_cjk: state.terminal_custom,
                    preview: font_preview(&state.settings, cx),
                },
                TerminalDisplayBehavior => TerminalSettingsSection::DisplayBehavior,
                TerminalDisplayCursor => TerminalSettingsSection::Cursor,
                TerminalDisplayCommandMarks => TerminalSettingsSection::CommandMarks,
                TerminalDisplayBuffer => TerminalSettingsSection::Buffer,
                TerminalInputClipboard => TerminalSettingsSection::Clipboard,
                TerminalInputMouseLinks => TerminalSettingsSection::MouseLinks,
                TerminalInputKeyboard => TerminalSettingsSection::Keyboard,
                TerminalTransferInBand => TerminalSettingsSection::InBandTransfer,
                TerminalTransferXmodem => TerminalSettingsSection::Xmodem,
                TerminalLoggingSession => TerminalSettingsSection::SessionLogging {
                    default_directory: "~/logs/terminal".into(),
                },
                _ => unreachable!("only supported sections are mounted"),
            },
            cx,
        ),
    }
}

fn keybindings(
    owner: &Entity<PageState>,
    c: &SettingsControls<'_>,
    reset: bool,
    section: SettingsSectionId,
    cx: &App,
) -> AnyElement {
    let state = owner.read(cx);
    let search = |cx: &App| {
        c.keybinding_search_field(
            c.ime.frame(
                SettingsInput::KeybindingSearch,
                state.shortcuts.keybinding_search_query(),
                cx,
            ),
            c.dispatch.clone(),
        )
    };
    let label = |_: KeybindingTextRole, _, value: String, color: u32| {
        div().text_color(rgb(color)).child(value).into_any_element()
    };
    let weak = owner.downgrade();
    let page = KeybindingPageView {
        components: c.components,
        state: owner,
        catalog: &state.catalog,
        search: &search,
        label: &label,
        dispatch: Rc::new(move |req: KeybindingRequest, cx| {
            let _ = weak.update(cx, |s, cx| s.shortcut(req, cx));
        }),
    };
    if reset {
        page.render_keybinding_reset_all_confirm_dialog(cx)
    } else {
        page.settings_keybindings_section(section, cx)
    }
}

impl PortableSettingsSurface {
    pub(super) fn overlays(
        &self,
        tokens: &ThemeTokens,
        dispatch: SettingsWindowDispatch,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let mut layers = Vec::new();
        let state = self.state.read(cx);
        if state.ai_confirm.read(cx).state().is_some() {
            layers.push(state.ai_confirm.clone().into_any_element());
        }
        if let Some((third_party, scroll)) = &state.document {
            let owner = self.state.downgrade();
            layers.push(
                SettingsMarkdownDialog {
                    tokens,
                    i18n: &state.i18n,
                    phase: ExitPhase::Visible,
                    focus: None,
                    options: oxideterm_gpui_markdown::MarkdownOptions::from_theme(tokens),
                    actions: Default::default(),
                }
                .legal_notice(*third_party, scroll, move |_, cx| {
                    let _ = owner.update(cx, |s, cx| {
                        s.document = None;
                        cx.notify();
                    });
                }),
            );
        }
        if state.reset_open {
            layers.push(keybindings(
                &self.state,
                &state.controls(tokens, dispatch.clone()),
                true,
                SettingsSectionId::KeybindingsToolbar,
                cx,
            ));
        }
        if let Some(editor) = state.editor.clone() {
            let frame = ThemeEditorFrame {
                tokens: *tokens,
                i18n: state.i18n.clone(),
                mono_font: settings_mono_font_family(&state.settings),
                scroll: state.editor_scroll.clone(),
                open_select: state.select,
                select_origin: Some(oxideterm_gpui_ui::select::BrowserFocusOrigin::Pointer),
            };
            let owner = self.state.downgrade();
            let ime = self.state.downgrade();
            let body = ThemeEditorBody {
                settings: &self.state,
                frame,
                background: None,
                window: dispatch,
                ime: &move |id, value, _, cx| {
                    Some(SettingsImeSource::frame(
                        ime.upgrade()?.read(cx),
                        id,
                        value,
                        cx,
                    ))
                },
                dispatch: Rc::new(move |r, _, cx| {
                    let _ = owner.update(cx, |s, cx| s.editor_action(r, cx));
                }),
                preview: &|editor, frame, _| {
                    let terminal =
                        crate::theme_picker::preview_palette(frame.tokens.terminal, |key| {
                            let index = oxideterm_settings_model::TERMINAL_THEME_COLOR_FIELDS
                                .iter()
                                .position(|f| f.json_key == key)?;
                            oxideterm_settings_model::parse_color_hex(
                                editor.terminal_colors.get(index)?,
                            )
                        });
                    Some(crate::settings_theme_editor_preview(
                        &frame.tokens,
                        &editor.name,
                        terminal,
                        oxideterm_settings_model::editor_ui_colors(&editor.ui_colors),
                        frame.mono_font.clone(),
                        None,
                        editor.background.scope,
                    ))
                },
            };
            layers.extend(body.render_modal(&editor, ExitPhase::Visible, cx));
        }
        let state = self.state.read(cx);
        if let Some(select) = state.select
            && let Some(&anchor) = state.anchors.get(&select.anchor_id())
        {
            let owner = self.state.downgrade();
            let width = f32::from(anchor.bounds.size.width);
            let popup = if select.theme_target().is_some() {
                Some(self.picker.clone().into_any_element())
            } else if select == SettingsSelect::AiProviderTemplate {
                let selected_type = self.ai_providers.read(cx).provider_type().to_owned();
                let providers = self.ai_providers.downgrade();
                let close_owner = self.state.downgrade();
                let mut list = oxideterm_gpui_ui::select::select_overlay_popup(tokens, width);
                for template in oxideterm_ai::AI_PROVIDER_TEMPLATES {
                    let ptype = template.provider_type;
                    let providers = providers.clone();
                    let close_owner = close_owner.clone();
                    list = list.child(oxideterm_gpui_ui::select::select_option_action(
                        oxideterm_gpui_ui::select::select_option(
                            tokens,
                            state.i18n.t(template.label_key),
                            selected_type == ptype,
                        ),
                        false,
                        false,
                        move |_, _, cx| {
                            let _ = providers.update(cx, |v, cx| v.select_type(ptype, cx));
                            let _ = close_owner.update(cx, |s, cx| {
                                s.select = None;
                                cx.notify();
                            });
                            cx.stop_propagation();
                        },
                    ));
                }
                Some(list.into_any_element())
            } else {
                CoreSelectMenu {
                    settings: &state.settings,
                    tokens,
                    i18n: &state.i18n,
                    owner: &self.state,
                    dispatch: Rc::new(move |req, _, cx| {
                        let _ = owner.update(cx, |s, cx| s.menu_action(req, cx));
                    }),
                }
                .render(state.route.route_snapshot().active_tab, select, width, cx)
            };
            if let Some(popup) = popup {
                let owner = self.state.clone();
                layers.push(settings_select_popover(
                    tokens,
                    select,
                    anchor,
                    popup,
                    move |_, cx| {
                        owner.update(cx, |s, cx| {
                            s.select = None;
                            cx.notify();
                        })
                    },
                ));
            }
        }
        layers
    }
}
