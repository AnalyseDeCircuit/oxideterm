use super::*;
use crate::workspace::settings::{APPEARANCE_BORDER_RADIUS_MAX, APPEARANCE_BORDER_RADIUS_MIN};
use oxideterm_gpui_settings_view::{
    animation_label, animation_options, settings_appearance_radius_control,
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with,
};
use oxideterm_gpui_ui::checkbox;
use oxideterm_gpui_ui::tabs::{tabs_list, tabs_trigger};

impl WorkspaceApp {
    pub(in crate::workspace) fn render_onboarding_welcome_stage(
        &self,
        wide: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const WELCOME_ICON_SIZE: f32 = 52.0;
        const WELCOME_ICON_RADIUS: f32 = 11.5;
        let settings = self.settings_store.settings();
        let wordmark_size = self.tokens.metrics.welcome_wordmark_font_size;
        let icon_source = crate::app_icon::app_icon_trimmed_source_sized(
            settings.appearance.app_icon,
            WELCOME_ICON_SIZE,
        );
        let brand_badge = oxideterm_gpui_ui::theme_card_surface_shadow(
            div()
                .size(px(WELCOME_ICON_SIZE))
                .relative()
                .flex_none()
                .rounded(px(WELCOME_ICON_RADIUS))
                .child(
                    gpui::img(icon_source)
                        .size(px(WELCOME_ICON_SIZE))
                        .object_fit(ObjectFit::Contain),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(WELCOME_ICON_RADIUS))
                        .border_1()
                        .border_color(rgba((self.tokens.ui.border << 8) | 0x80)),
                ),
            &self.tokens,
        );
        let hero = div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(22.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .child(brand_badge)
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(wordmark_size))
                                    .line_height(px(wordmark_size + 4.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text_heading))
                                    .child("OxideTerm"),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.i18n.t("onboarding.subtitle")),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(self.tokens.metrics.ui_text_sm * 1.6))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("onboarding.project_intro")),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(if wide { 2 } else { 1 })
                    .gap_x(px(20.0))
                    .gap_y(px(14.0))
                    .children([
                        self.onboarding_feature_tile(LucideIcon::Zap, "highlight_performance"),
                        self.onboarding_feature_tile(LucideIcon::Lock, "highlight_security_arch"),
                        self.onboarding_feature_tile(LucideIcon::Cpu, "highlight_crossplatform"),
                        self.onboarding_feature_tile(LucideIcon::Puzzle, "highlight_extensible"),
                    ]),
            )
            .into_any_element();

        let disclaimer = div()
            .when(wide, |col| col.w(px(400.0)).flex_none())
            .when(!wide, |col| col.w_full())
            .child(self.render_onboarding_disclaimer(cx))
            .into_any_element();

        div()
            .w_full()
            .pt(px(if wide { 12.0 } else { 0.0 }))
            .flex()
            .when(wide, |row| row.flex_row().items_start().gap(px(40.0)))
            .when(!wide, |col| col.flex_col().gap(px(20.0)))
            .child(hero)
            .child(disclaimer)
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_disclaimer(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accepted = self.onboarding.disclaimer_accepted;
        div()
            .p(px(22.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((
                self.tokens.ui.border << 8
            ) | ONBOARDING_CARD_BORDER_ALPHA))
            .bg(self.onboarding_card_bg())
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .child(div().flex_1().min_w(px(0.0)).child(
                        self.onboarding_step_heading(
                            "onboarding.disclaimer_title",
                            "onboarding.disclaimer_desc",
                        ),
                    ))
                    .child(
                        div()
                            .size(px(28.0))
                            .flex_none()
                            .rounded_full()
                            .bg(rgba((
                                if accepted {
                                    self.tokens.ui.accent
                                } else {
                                    self.tokens.ui.warning
                                } << 8
                            ) | ONBOARDING_ACCENT_SUBTLE_ALPHA))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Self::render_lucide_icon(
                                if accepted {
                                    LucideIcon::Check
                                } else {
                                    LucideIcon::Lock
                                },
                                13.5,
                                rgb(if accepted {
                                    self.tokens.ui.accent
                                } else {
                                    self.tokens.ui.warning
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.onboarding_info_row(
                        LucideIcon::AlertTriangle,
                        self.tokens.ui.warning,
                        "onboarding.disclaimer_no_warranty",
                        "onboarding.disclaimer_no_warranty_text",
                    ))
                    .child(self.onboarding_info_row(
                        LucideIcon::Shield,
                        self.tokens.ui.text_muted,
                        "onboarding.disclaimer_data_security",
                        "onboarding.disclaimer_data_security_text",
                    ))
                    .child(self.onboarding_info_row(
                        LucideIcon::Brain,
                        self.tokens.ui.text_muted,
                        "onboarding.disclaimer_ai",
                        "onboarding.disclaimer_ai_text",
                    )),
            )
            .child(self.onboarding_inline_link(
                LucideIcon::ExternalLink,
                self.i18n.t("onboarding.disclaimer_gpl_note"),
                |this, cx| this.open_onboarding_disclaimer(cx),
                cx,
            ))
            .child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if accepted {
                        rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
                    } else {
                        rgba((self.tokens.ui.warning << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
                    })
                    .bg(if accepted {
                        rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
                    } else {
                        self.onboarding_sunken_bg()
                    })
                    .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                    .px(px(12.0))
                    .py(px(11.0))
                    .cursor(CursorStyle::PointingHand)
                    .child(checkbox::checkbox(
                        &self.tokens,
                        "onboarding-disclaimer-accept",
                        self.i18n.t("onboarding.disclaimer_accept"),
                        accepted,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.toggle_onboarding_disclaimer_acceptance(cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_appearance(
        &self,
        wide: bool,
        content_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let control_width = if wide {
            368.0
        } else {
            (content_width - 40.0).clamp(240.0, 420.0)
        };
        let controls = div()
            .when(wide, |col| col.w(px(368.0)).flex_none())
            .when(!wide, |col| col.w_full())
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.onboarding_step_heading(
                "onboarding.appearance_title",
                "onboarding.appearance_desc",
            ))
            .child(self.onboarding_hairline())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(self.appearance_theme_controls_with_hints(settings, false, cx)),
            )
            .child(self.onboarding_hairline())
            .child(self.onboarding_font_picker(cx))
            .child(self.onboarding_hairline())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(self.onboarding_corner_style_picker(cx))
                    .child(self.onboarding_radius_picker(cx))
                    .child(self.onboarding_animation_picker(control_width, cx)),
            )
            .into_any_element();

        let showcase = div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.render_onboarding_live_terminal_specimen(settings))
            .child(self.appearance_theme_preview_compact(settings, cx))
            .child(self.onboarding_quiet_note("onboarding.background_image_hint", &[]))
            .into_any_element();

        div()
            .w_full()
            .flex()
            .when(wide, |row| row.flex_row().items_start().gap(px(32.0)))
            .when(!wide, |col| col.flex_col().gap(px(22.0)))
            .child(controls)
            .child(showcase)
            .into_any_element()
    }

    fn onboarding_corner_style_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        use oxideterm_settings::CornerStyle;
        let selected = self.settings_store.settings().appearance.corner_style;
        let items = [
            (
                CornerStyle::Circular,
                "settings_view.appearance.corner_circular",
            ),
            (
                CornerStyle::Continuous,
                "settings_view.appearance.corner_continuous",
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (style, key))| {
            tabs_trigger(&self.tokens, self.i18n.t(key), selected == style)
                .id(("onboarding-corner-style", index))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(
                            |settings| settings.appearance.corner_style = style,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
        });
        self.onboarding_section(
            "settings_view.appearance.corner_style",
            None,
            tabs_list(&self.tokens).children(items).into_any_element(),
        )
    }

    fn render_onboarding_live_terminal_specimen(&self, settings: &PersistedSettings) -> AnyElement {
        let term = self.tokens.terminal;
        let font_family = SharedString::from(settings_mono_font_family(settings));
        let font_size = (settings.terminal.font_size as f32).clamp(11.0, 20.0);
        let line_height = (font_size * 1.55).round();
        let radius = self.tokens.radii.lg;
        div()
            .rounded(px(radius))
            .border_1()
            .border_color(rgba((
                self.tokens.ui.border << 8
            ) | ONBOARDING_CARD_BORDER_ALPHA))
            .bg(rgb(term.background))
            .overflow_hidden()
            .child(
                div()
                    .px(px(14.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x38))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Terminal,
                                12.5,
                                rgb(term.bright_black),
                            ))
                            .child(
                                div()
                                    .font_family(font_family.clone())
                                    .text_size(px(self.tokens.metrics.ui_text_2xs))
                                    .text_color(rgb(term.bright_black))
                                    .child("oxide@prod:~/services"),
                            ),
                    )
                    .child(
                        div()
                            .font_family(font_family.clone())
                            .text_size(px(self.tokens.metrics.ui_text_2xs))
                            .text_color(rgb(term.bright_black))
                            .child(format!(
                                "{} · {}px",
                                settings
                                    .terminal
                                    .font_family
                                    .terminal_family_name(&settings.terminal.custom_font_family),
                                settings.terminal.font_size
                            )),
                    ),
            )
            .child(
                div()
                    .p(px(16.0))
                    .font_family(font_family)
                    .text_size(px(font_size))
                    .line_height(px(line_height))
                    .text_color(rgb(term.foreground))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.0))
                            .child(div().text_color(rgb(term.green)).child("oxide@prod"))
                            .child(div().text_color(rgb(term.cyan)).child("~/services"))
                            .child(div().text_color(rgb(term.foreground)).child("ssh edge-01 => status --all")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(10.0))
                            .child(div().text_color(rgb(term.bright_green)).child("● active"))
                            .child(div().text_color(rgb(term.blue)).child("sftp+tunnel"))
                            .child(div().text_color(rgb(term.yellow)).child("12ms RTT"))
                            .child(div().text_color(rgb(term.magenta)).child("Aa 01 => 中文对齐")),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_font_picker(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        let mut grid = div().grid().grid_cols(2).gap(px(8.0));
        for (family, label, _bundled) in ONBOARDING_FONT_OPTIONS {
            let selected = settings.terminal.font_family == family;
            let label = if family == FontFamily::Custom {
                self.i18n.t("settings_view.terminal.custom_font")
            } else {
                label.trim_end_matches(" (Subset)").to_string()
            };
            grid = grid.child(
                div()
                    .min_w_0()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(if selected {
                        rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_BORDER_ALPHA)
                    } else {
                        rgba((self.tokens.ui.border << 8) | ONBOARDING_CARD_BORDER_ALPHA)
                    })
                    .bg(if selected {
                        rgba((self.tokens.ui.accent << 8) | ONBOARDING_ACCENT_SUBTLE_ALPHA)
                    } else {
                        self.onboarding_sunken_bg()
                    })
                    .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                    .px(px(10.0))
                    .py(px(7.0))
                    .flex()
                    .justify_between()
                    .items_center()
                    .gap(px(6.0))
                    .cursor(CursorStyle::PointingHand)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .font_family(SharedString::from(family.terminal_family_name("")))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .when(selected, |text| text.font_weight(gpui::FontWeight::MEDIUM))
                            .text_color(rgb(if selected {
                                self.tokens.ui.text_heading
                            } else {
                                self.tokens.ui.text
                            }))
                            .child(label),
                    )
                    .when(selected, |row| {
                        row.child(Self::render_lucide_icon(
                            LucideIcon::Check,
                            13.0,
                            rgb(self.tokens.ui.accent),
                        ))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| settings.terminal.font_family = family,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        let font_size = settings.terminal.font_size;
        self.onboarding_section(
            "onboarding.select_font",
            None,
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(grid)
                .child(oxideterm_gpui_ui::motion::auto_height(
                    &self.tokens,
                    "onboarding-custom-font",
                    (settings.terminal.font_family == FontFamily::Custom).then(|| {
                        self.settings_text_input_control_fill(
                            SettingsInput::TerminalCustomFontFamily,
                            settings.terminal.custom_font_family.clone(),
                            "'Sarasa Fixed SC', 'Fira Code', monospace".to_string(),
                            cx,
                        )
                    }),
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.i18n.t("onboarding.font_size")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(self.onboarding_font_size_button("-", -1, cx))
                                .child(
                                    div()
                                        .min_w(px(46.0))
                                        .text_align(gpui::TextAlign::Center)
                                        .font_family(SharedString::from(
                                            FontFamily::Jetbrains.terminal_family_name(""),
                                        ))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .child(format!("{font_size}px")),
                                )
                                .child(self.onboarding_font_size_button("+", 1, cx)),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn onboarding_font_size_button(
        &self,
        label: &'static str,
        delta: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        button_with(
            &self.tokens,
            label.to_string(),
            ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Icon,
                radius: ButtonRadius::Sm,
                disabled: false,
            },
        )
        .size(px(26.0))
        .cursor(CursorStyle::PointingHand)
        .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.edit_settings(
                    move |settings| {
                        settings.terminal.font_size =
                            (settings.terminal.font_size + delta).clamp(8, 32);
                    },
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_animation_picker(
        &self,
        compact_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use oxideterm_gpui_ui::{
            SegmentedControlOptions, segmented_control, segmented_control_item,
        };
        let speeds = animation_options();
        let selected = self.settings_store.settings().appearance.animation_speed;
        let active = speeds
            .iter()
            .position(|speed| *speed == selected)
            .unwrap_or(0);
        let previous = self
            .segmented_control_user_previous_index(ONBOARDING_ANIMATION_MOTION_ID, active)
            .unwrap_or(active);
        let items = speeds
            .iter()
            .enumerate()
            .map(|(index, &speed)| {
                segmented_control_item(
                    &self.tokens,
                    animation_label(speed, &self.i18n),
                    index == active,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        if index != active {
                            this.edit_settings(
                                |settings| settings.appearance.animation_speed = speed,
                                cx,
                            );
                            this.begin_user_segmented_control_transition_from(
                                ONBOARDING_ANIMATION_MOTION_ID,
                                active,
                                index,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
            })
            .collect();
        self.onboarding_section(
            "settings_view.appearance.animation",
            None,
            segmented_control(
                &self.tokens,
                ONBOARDING_ANIMATION_MOTION_ID,
                SegmentedControlOptions::new(active, previous, speeds.len())
                    .compact(compact_width)
                    .has_background_image(self.window_background_preferences().is_some())
                    .user_transition_active(self.segmented_control_user_transition_active(
                        ONBOARDING_ANIMATION_MOTION_ID,
                        active,
                    )),
                items,
            )
            .into_any_element(),
        )
    }

    pub(in crate::workspace) fn onboarding_radius_picker(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected_radius = self.settings_store.settings().appearance.border_radius;
        let slider = self.appearance_slider_control(
            SettingsSlider::OnboardingBorderRadius,
            SelectAnchorId::OnboardingBorderRadiusSlider,
            APPEARANCE_BORDER_RADIUS_MIN,
            APPEARANCE_BORDER_RADIUS_MAX,
            selected_radius as f32,
            cx,
        );
        self.onboarding_section(
            "settings_view.appearance.border_radius",
            None,
            settings_appearance_radius_control(&self.tokens, selected_radius, slider),
        )
    }

    pub(in crate::workspace) fn render_onboarding_launch_stage(
        &self,
        wide: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_mac = cfg!(target_os = "macos");
        let mod_key = if is_mac { "⌘" } else { "Ctrl" };
        let setup_band = div()
            .w_full()
            .grid()
            .grid_cols(if wide { 2 } else { 1 })
            .gap(px(if wide { 28.0 } else { 20.0 }))
            .child(self.render_onboarding_ai_intro(cx))
            .child(self.render_onboarding_cli_companion(window, cx));

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(setup_band)
            .child(self.onboarding_hairline())
            .child(self.render_onboarding_quick_start(wide, window, cx))
            .child(self.onboarding_hairline())
            .child(self.onboarding_shortcut_strip(mod_key, is_mac))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_ai_intro(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                self.onboarding_step_heading(
                    "onboarding.ai_tools_title",
                    "onboarding.ai_tools_desc",
                ),
            )
            .child(self.render_onboarding_ai_setup(cx))
            .child(self.onboarding_capability_list(
                "onboarding.ai_tools_privacy",
                &[
                    (LucideIcon::Key, "ai_tools_privacy_keys"),
                    (LucideIcon::Shield, "ai_tools_privacy_context"),
                ],
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_ai_setup(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.onboarding_toggle_card(
                "onboarding.ai_tools_enable",
                "onboarding.ai_tools_enable_hint",
                self.onboarding.ai_opt_in,
                true,
                |this, cx| {
                    this.onboarding.ai_opt_in = !this.onboarding.ai_opt_in;
                    if !this.onboarding.ai_opt_in {
                        this.onboarding.tool_use_opt_in = false;
                    }
                    cx.notify();
                },
                cx,
            ))
            .child(self.onboarding_toggle_card(
                "onboarding.ai_tools_enable_tools",
                "onboarding.ai_tools_enable_tools_hint",
                self.onboarding.tool_use_opt_in,
                self.onboarding.ai_opt_in,
                |this, cx| {
                    if this.onboarding.ai_opt_in {
                        this.onboarding.tool_use_opt_in = !this.onboarding.tool_use_opt_in;
                        cx.notify();
                    }
                },
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_cli_companion(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cli = self.settings_workspace.read(cx).cli_companion_snapshot();
        let status = cli.status.as_ref();
        let installed = status.is_some_and(|status| status.installed);
        let needs_reinstall = status.is_some_and(|status| status.needs_reinstall);
        let status_label = if cli.loading {
            self.i18n.t("settings_view.general.cli_checking")
        } else if installed && needs_reinstall {
            self.i18n.t("onboarding.cli_step_reinstall_required")
        } else if installed {
            self.i18n.t("onboarding.cli_step_installed")
        } else if !status.is_some_and(|status| status.bundled) && status.is_some() {
            self.i18n.t("onboarding.cli_step_not_bundled")
        } else {
            self.i18n.t("settings_view.general.cli_not_installed")
        };
        let status_color = if installed && !needs_reinstall {
            self.tokens.ui.success
        } else if cli.loading || needs_reinstall {
            self.tokens.ui.warning
        } else {
            self.tokens.ui.text_muted
        };
        let path = status
            .and_then(|status| status.install_path.clone())
            .unwrap_or_default();

        div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                self.onboarding_step_heading(
                    "onboarding.cli_step_title",
                    "onboarding.cli_step_desc",
                ),
            )
            .child(
                div()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | ONBOARDING_CARD_BORDER_ALPHA,
                    ))
                    .bg(self.onboarding_card_bg())
                    .px(px(14.0))
                    .py(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(7.0))
                                    .child(Self::render_lucide_icon(
                                        if installed {
                                            LucideIcon::CheckCircle
                                        } else {
                                            LucideIcon::AlertTriangle
                                        },
                                        15.0,
                                        rgb(status_color),
                                    ))
                                    .child(
                                        div()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(rgb(status_color))
                                            .child(status_label),
                                    ),
                            )
                            .when(!path.is_empty(), |column| {
                                column.child(
                                    div()
                                        .text_size(px(self.tokens.metrics.ui_text_2xs))
                                        .font_family(SharedString::from(
                                            FontFamily::Jetbrains.terminal_family_name(""),
                                        ))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .truncate()
                                        .child(self.onboarding_i18n_with(
                                            "onboarding.cli_step_installed_at",
                                            &[("path", path.clone())],
                                        )),
                                )
                            }),
                    )
                    .child(self.onboarding_button(
                        self.i18n.t("settings_view.general.cli_companion"),
                        Some((LucideIcon::Settings, true)),
                        ButtonVariant::Outline,
                        false,
                        |this, window, cx| this.onboarding_open_cli_settings(window, cx),
                        cx,
                    )),
            )
            .child(self.onboarding_hairline())
            .child(self.onboarding_capability_grid(
                "onboarding.cli_step_capabilities",
                &[
                    (LucideIcon::Server, "cli_step_cap_sessions"),
                    (LucideIcon::Network, "cli_step_cap_forward"),
                    (LucideIcon::Bot, "cli_step_cap_ai"),
                    (LucideIcon::Cpu, "cli_step_cap_status"),
                ],
            ))
            .when(!installed || needs_reinstall, |body| {
                body.child(self.onboarding_quiet_note("onboarding.cli_step_skip_hint", &[]))
            })
            .when(cli.status.is_none(), |body| {
                body.child(self.onboarding_inline_link(
                    LucideIcon::RefreshCw,
                    self.i18n.t("settings_view.help.retry"),
                    |this, cx| this.refresh_cli_companion_status(cx),
                    cx,
                ))
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn render_onboarding_quick_start(
        &self,
        wide: bool,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                self.onboarding_step_heading(
                    "onboarding.quick_start",
                    "onboarding.quick_start_desc",
                ),
            )
            .child(
                div()
                    .grid()
                    // Four primary actions stay readable across all shipped locales in two columns.
                    .grid_cols(if wide { 2 } else { 1 })
                    .gap(px(10.0))
                    .child(self.onboarding_action_card(
                        LucideIcon::Terminal,
                        "onboarding.open_terminal",
                        "onboarding.open_terminal_desc",
                        |this, window, cx| this.onboarding_open_terminal(window, cx),
                        cx,
                    ))
                    .child(self.onboarding_action_card(
                        LucideIcon::Plus,
                        "onboarding.new_connection",
                        "onboarding.new_connection_desc",
                        |this, window, cx| this.onboarding_open_new_connection(window, cx),
                        cx,
                    ))
                    .child(self.onboarding_import_card(cx))
                    .child(self.onboarding_action_card(
                        LucideIcon::AppWindow,
                        "onboarding.import_apps",
                        "onboarding.import_apps_desc",
                        |this, window, cx| this.onboarding_open_connection_importers(window, cx),
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn onboarding_shortcut_strip(
        &self,
        mod_key: &str,
        is_mac: bool,
    ) -> AnyElement {
        let items = [
            ("shortcut_command_palette", format!("{mod_key}K")),
            ("shortcut_new_connection", format!("{mod_key}N")),
            ("shortcut_new_tab", format!("{mod_key}T")),
            ("shortcut_search", format!("{mod_key}F")),
            (
                "shortcut_ai_chat",
                if is_mac { "⌘⇧A" } else { "Ctrl+Shift+A" }.to_string(),
            ),
            (
                "shortcut_split_right",
                if is_mac { "⌘E" } else { "Ctrl+E" }.to_string(),
            ),
            (
                "shortcut_split_down",
                if is_mac { "⌘D" } else { "Ctrl+D" }.to_string(),
            ),
            (
                "shortcut_close_tab",
                if is_mac { "⌘W" } else { "Ctrl+W" }.to_string(),
            ),
        ];
        let label_color = if self.window_background_preferences().is_some() {
            self.tokens.ui.text
        } else {
            self.tokens.ui.text_muted
        };
        let mut strip = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(18.0))
            .gap_y(px(8.0));
        for (desc_key, shortcut) in items {
            strip = strip.child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .rounded(px(self.tokens.radii.xs))
                            .border_1()
                            .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
                            .bg(self.onboarding_sunken_bg())
                            .px(px(5.0))
                            .py(px(1.5))
                            .font_family(SharedString::from(
                                FontFamily::Jetbrains.terminal_family_name(""),
                            ))
                            .text_size(px(self.tokens.metrics.ui_text_2xs))
                            .line_height(px(14.0))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(shortcut),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(label_color))
                            .child(self.i18n.t(&format!("onboarding.{desc_key}"))),
                    ),
            );
        }
        self.onboarding_section("onboarding.shortcuts_title", None, strip.into_any_element())
    }

    pub(in crate::workspace) fn onboarding_import_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (icon, text, disabled) = match self.onboarding.import_state {
            OnboardingImportState::Done => (
                LucideIcon::Check,
                self.onboarding_i18n_with(
                    "onboarding.import_ssh_done",
                    &[("count", self.onboarding.imported_count.to_string())],
                ),
                true,
            ),
            OnboardingImportState::Loading => (
                LucideIcon::LoaderCircle,
                self.i18n.t("onboarding.importing"),
                true,
            ),
            OnboardingImportState::Idle => match self.onboarding.host_count {
                Some(0) => (
                    LucideIcon::Download,
                    self.i18n.t("onboarding.import_ssh_none"),
                    true,
                ),
                Some(count) => (
                    LucideIcon::Download,
                    self.onboarding_i18n_with(
                        "onboarding.import_ssh_desc",
                        &[("count", count.to_string())],
                    ),
                    false,
                ),
                None => (
                    LucideIcon::LoaderCircle,
                    self.i18n.t("onboarding.importing"),
                    true,
                ),
            },
        };
        self.onboarding_action_card_with_detail(
            icon,
            self.i18n.t("onboarding.import_ssh"),
            text,
            disabled,
            |this, _window, cx| this.import_onboarding_ssh_hosts(cx),
            cx,
        )
    }

    pub(in crate::workspace) fn onboarding_action_card(
        &self,
        icon: LucideIcon,
        title_key: &str,
        desc_key: &str,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.onboarding_action_card_with_detail(
            icon,
            self.i18n.t(title_key),
            self.i18n.t(desc_key),
            false,
            action,
            cx,
        )
    }

    pub(in crate::workspace) fn onboarding_action_card_with_detail(
        &self,
        icon: LucideIcon,
        title: String,
        detail: String,
        disabled: bool,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .min_h(px(64.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | ONBOARDING_CARD_BORDER_ALPHA,
            ))
            .bg(self.onboarding_card_bg())
            .px(px(14.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .when(!disabled, |card| {
                card.hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            })
            .opacity(if disabled {
                ONBOARDING_DISABLED_OPACITY
            } else {
                1.0
            })
            .cursor(if disabled {
                CursorStyle::OperationNotAllowed
            } else {
                CursorStyle::PointingHand
            })
            .child(
                div()
                    .size(px(34.0))
                    .flex_none()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | ONBOARDING_HAIRLINE_ALPHA,
                    ))
                    .bg(self.onboarding_sunken_bg())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(if matches!(icon, LucideIcon::LoaderCircle) {
                        self.render_loading_icon(
                            "onboarding-import-loading",
                            16.0,
                            rgb(self.tokens.ui.text_muted),
                        )
                    } else if matches!(icon, LucideIcon::Check) {
                        Self::render_lucide_icon(icon, 16.0, rgb(self.tokens.ui.success))
                    } else {
                        Self::render_lucide_icon(icon, 16.0, rgb(self.tokens.ui.accent))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(detail),
                    ),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ChevronRight,
                15.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if !disabled {
                        action(this, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}
