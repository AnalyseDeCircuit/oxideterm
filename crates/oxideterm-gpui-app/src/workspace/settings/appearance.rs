use super::*;

pub(in crate::workspace) const THEME_EDITOR_MODAL_WIDTH: f32 = 672.0; // Tauri ThemeEditorModal max-w-2xl.
pub(in crate::workspace) const THEME_EDITOR_MODAL_MAX_HEIGHT: f32 = 760.0; // Tauri max-h-[85vh] on the default native window.
pub(in crate::workspace) const THEME_EDITOR_HEADER_PADDING_X: f32 = 16.0; // DialogHeader px-4.
pub(in crate::workspace) const THEME_EDITOR_HEADER_PADDING_Y: f32 = 12.0; // DialogHeader py-3.
pub(in crate::workspace) const THEME_EDITOR_BODY_PADDING_X: f32 = 16.0; // Body px-4.
pub(in crate::workspace) const THEME_EDITOR_BODY_PADDING_Y: f32 = 12.0; // Body py-3.
pub(in crate::workspace) const THEME_EDITOR_BODY_GAP: f32 = 16.0; // Tauri space-y-4.
pub(in crate::workspace) const THEME_EDITOR_INPUT_HEIGHT: f32 = 32.0; // Tauri Input h-8.
pub(in crate::workspace) const THEME_EDITOR_DUPLICATE_WIDTH: f32 = 180.0; // Tauri duplicate select w-[180px].

impl WorkspaceApp {
    pub(in crate::workspace) fn settings_appearance_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => self.appearance_theme_card(settings, cx),
            1 => self.appearance_layout_card(settings, cx),
            2 => self.appearance_app_icon_card(settings, cx),
            3 => self.appearance_background_card(settings, cx),
            _ => div().into_any_element(),
        }
    }

    pub(in crate::workspace) fn appearance_theme_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card(
            self.i18n.t("settings_view.appearance.theme"),
            Some(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.appearance_action_button(
                        LucideIcon::Upload,
                        self.i18n.t("settings_view.appearance.theme_import"),
                        cx.listener(|this, _event, _window, cx| {
                            this.import_theme_from_file(cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .when(is_custom_theme_id(&settings.terminal.theme), |actions| {
                        actions.child(self.appearance_action_button(
                            LucideIcon::Pencil,
                            self.i18n.t("settings_view.custom_theme.edit"),
                            cx.listener(|this, _event, _window, cx| {
                                let theme_id =
                                    this.settings_store.settings().terminal.theme.clone();
                                this.open_theme_editor(Some(theme_id), cx);
                                cx.stop_propagation();
                            }),
                        ))
                    })
                    .child(self.appearance_action_button(
                        LucideIcon::Plus,
                        self.i18n.t("settings_view.custom_theme.create"),
                        cx.listener(|this, _event, _window, cx| {
                            this.open_theme_editor(None, cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .into_any_element(),
            ),
            vec![
                self.appearance_row(
                    "settings_view.appearance.color_theme",
                    "settings_view.appearance.color_theme_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceTheme,
                        custom_theme_display_name(settings, &settings.terminal.theme),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                ),
                self.appearance_theme_preview(settings),
            ],
        )
    }

    pub(in crate::workspace) fn appearance_layout_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card(
            self.i18n.t("settings_view.appearance.layout"),
            None,
            vec![
                self.appearance_row(
                    "settings_view.appearance.density",
                    "settings_view.appearance.density_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceDensity,
                        density_label(settings.appearance.ui_density, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.border_radius",
                    "settings_view.appearance.border_radius_hint",
                    self.appearance_radius_control(settings, cx),
                ),
                self.appearance_row(
                    "settings_view.appearance.ui_font",
                    "settings_view.appearance.ui_font_hint",
                    self.appearance_text_input_control(
                        SettingsInput::AppearanceUiFont,
                        settings.appearance.ui_font_family.clone(),
                        self.i18n.t("settings_view.appearance.ui_font_placeholder"),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.animation",
                    "settings_view.appearance.animation_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceAnimation,
                        animation_label(settings.appearance.animation_speed, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.render_profile",
                    "settings_view.appearance.render_profile_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceRenderProfile,
                        render_profile_label(settings.appearance.render_profile, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.frosted_glass",
                    "settings_view.appearance.frosted_glass_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceFrostedGlass,
                        frosted_glass_label(settings.appearance.frosted_glass, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_vibrancy_status(settings),
            ],
        )
    }

    pub(in crate::workspace) fn appearance_app_icon_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card_with_icon(
            LucideIcon::AppWindow,
            self.i18n.t("settings_view.appearance.app_icon"),
            vec![self.appearance_app_icon_row(settings.appearance.app_icon, cx)],
        )
    }

    pub(in crate::workspace) fn appearance_app_icon_row(
        &self,
        selected: AppIconVariant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The icon picker can wrap into multiple rows. Keep its label above the
        // grid so narrow settings panes do not squeeze localized copy vertically.
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.appearance.app_icon_variant")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.appearance.app_icon_variant_hint"),
                            ),
                    ),
            )
            .child(self.appearance_app_icon_picker(selected, cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_app_icon_picker(
        &self,
        selected: AppIconVariant,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut picker = div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_start()
            .gap(px(10.0))
            .min_w(px(0.0))
            .flex_wrap();

        for variant in crate::app_icon::APP_ICON_VARIANTS {
            picker =
                picker.child(self.appearance_app_icon_option(*variant, *variant == selected, cx));
        }

        picker.into_any_element()
    }

    pub(in crate::workspace) fn appearance_app_icon_option(
        &self,
        variant: AppIconVariant,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon_path = crate::app_icon::app_icon_variant_resource_path(variant);
        let border_color = if selected {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.border
        };
        let image = img(icon_path)
            .size(px(42.0))
            .object_fit(ObjectFit::Contain)
            .rounded(px(self.tokens.radii.md));

        div()
            .relative()
            .size(px(58.0))
            .flex_none()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(border_color))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
            .child(image)
            .when(selected, |button| {
                button.child(
                    div()
                        .absolute()
                        .right(px(4.0))
                        .bottom(px(4.0))
                        .size(px(16.0))
                        .rounded_full()
                        .bg(rgb(self.tokens.ui.accent))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Self::render_lucide_icon(
                            LucideIcon::Check,
                            11.0,
                            rgb(self.tokens.ui.bg),
                        )),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.edit_settings(|settings| settings.appearance.app_icon = variant, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_background_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let background_blur = self
            .settings_page
            .background_blur_preview
            .unwrap_or(settings.terminal.background_blur);
        let has_background_image = settings.terminal.background_image.is_some();
        let mut rows = Vec::new();
        if has_background_image {
            // Tauri only shows the master enable checkbox after an image exists.
            // Keep the same conditional layout so the empty gallery card does not
            // reserve controls that the browser version hides.
            rows.push(self.appearance_checkbox_row(
                "settings_view.terminal.bg_enabled",
                "settings_view.terminal.bg_enabled_hint",
                settings.terminal.background_enabled,
                set_terminal_background_enabled,
                cx,
            ));
        }
        rows.push(self.appearance_background_image_slot(settings, cx));
        if has_background_image {
            rows.push(self.appearance_row(
                "settings_view.terminal.bg_scope",
                "settings_view.terminal.bg_scope_hint",
                self.appearance_background_scope_control(settings.terminal.background_scope, cx),
            ));
            rows.extend([
                self.appearance_row(
                    "settings_view.terminal.bg_opacity",
                    "settings_view.terminal.bg_opacity_hint",
                    self.appearance_slider_value_control(
                        SettingsSlider::AppearanceBackgroundOpacity,
                        SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider,
                        3.0,
                        50.0,
                        (settings.terminal.background_opacity * 100.0).round() as f32,
                        "%",
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.terminal.bg_blur",
                    "settings_view.terminal.bg_blur_hint",
                    self.appearance_slider_value_control(
                        SettingsSlider::AppearanceBackgroundBlur,
                        SelectAnchorId::SettingsAppearanceBackgroundBlurSlider,
                        0.0,
                        20.0,
                        background_blur as f32,
                        "px",
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.terminal.bg_fit",
                    "settings_view.terminal.bg_fit_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceBackgroundFit,
                        background_fit_label(settings.terminal.background_fit, &self.i18n),
                        self.tokens.metrics.settings_appearance_fit_select_width,
                        cx,
                    ),
                ),
            ]);
            if settings.terminal.background_scope == BackgroundScope::Content {
                rows.push(self.appearance_background_tabs(settings, cx));
            }
        }
        self.appearance_card_with_icon(
            LucideIcon::Image,
            self.i18n.t("settings_view.terminal.bg_title"),
            rows,
        )
    }

    pub(in crate::workspace) fn appearance_background_scope_control(
        &self,
        selected: BackgroundScope,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut control = div()
            .flex()
            .items_center()
            .p(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_sunken));
        for (scope, label_key) in [
            (
                BackgroundScope::Content,
                "settings_view.terminal.bg_scope_content",
            ),
            (
                BackgroundScope::Window,
                "settings_view.terminal.bg_scope_window",
            ),
        ] {
            let active = scope == selected;
            control = control.child(
                div()
                    .h(px(28.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.sm))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(if active {
                        self.tokens.ui.accent
                    } else {
                        self.tokens.ui.text_muted
                    }))
                    .bg(if active {
                        rgba((self.tokens.ui.accent << 8) | 0x24)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .hover({
                        let hover = self.tokens.ui.bg_hover;
                        move |segment| segment.bg(rgb(hover))
                    })
                    .child(self.i18n.t(label_key))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| settings.terminal.background_scope = scope,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        control.into_any_element()
    }

    pub(in crate::workspace) fn appearance_card(
        &self,
        title: String,
        actions: Option<AnyElement>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(
            settings_appearance_card_header(&self.tokens, title, None, actions),
            rows,
        )
    }

    pub(in crate::workspace) fn appearance_card_with_icon(
        &self,
        icon: LucideIcon,
        title: String,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(
            settings_appearance_card_header(
                &self.tokens,
                title,
                Some(Self::render_lucide_icon(
                    icon,
                    16.0,
                    rgb(self.tokens.ui.text),
                )),
                None,
            ),
            rows,
        )
    }

    pub(in crate::workspace) fn appearance_card_shell(
        &self,
        header: AnyElement,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        settings_appearance_card_shell(
            &self.tokens,
            self.settings_background_active(),
            header,
            rows,
        )
    }

    pub(in crate::workspace) fn appearance_action_button(
        &self,
        icon: LucideIcon,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        // Appearance header actions are Tauri small outline toolbar buttons.
        // Route activation through the workspace Button boundary so browser
        // disabled/loading/focus-visible behavior can stay centralized.
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                height: Some(self.tokens.metrics.settings_appearance_action_height),
                padding_x: Some(10.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                background: Some(rgba(0x00000000)),
                border: Some(rgb(self.tokens.ui.border)),
                text_color: Some(rgb(self.tokens.ui.text)),
                hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    pub(in crate::workspace) fn appearance_row(
        &self,
        label_key: &str,
        hint_key: &str,
        control: AnyElement,
    ) -> AnyElement {
        settings_appearance_row(&self.tokens, &self.i18n, label_key, hint_key, control)
    }

    pub(in crate::workspace) fn appearance_vibrancy_status(
        &self,
        settings: &PersistedSettings,
    ) -> AnyElement {
        let mode = effective_vibrancy_mode(settings, &self.render_policy);
        let mode_label = frosted_glass_label(frosted_glass_mode_from_native(mode), &self.i18n);
        let effective_text = format!(
            "{} {}",
            self.i18n
                .t("settings_view.appearance.frosted_glass_effective_mode"),
            mode_label
        );
        let (status_key, status_color) = if !self.render_policy.allow_vibrancy {
            (
                "settings_view.appearance.frosted_glass_status_profile_disabled",
                self.tokens.ui.warning,
            )
        } else if mode == NativeVibrancyMode::Off {
            (
                "settings_view.appearance.frosted_glass_status_off",
                self.tokens.ui.text_muted,
            )
        } else {
            match self.vibrancy_support {
                VibrancySupport::Supported => (
                    "settings_view.appearance.frosted_glass_status_active",
                    self.tokens.ui.success,
                ),
                VibrancySupport::Fallback { .. } => (
                    "settings_view.appearance.frosted_glass_status_fallback",
                    self.tokens.ui.warning,
                ),
                VibrancySupport::Unsupported { .. } => (
                    "settings_view.appearance.frosted_glass_status_unsupported",
                    self.tokens.ui.error,
                ),
            }
        };
        let blur_key = if self.render_policy.allow_background_blur {
            "settings_view.appearance.frosted_glass_dialog_blur_enabled"
        } else {
            "settings_view.appearance.frosted_glass_dialog_blur_disabled"
        };
        let status_row = |color: u32, label: String| {
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_row()
                .items_start()
                .gap(px(8.0))
                .child(
                    div()
                        .mt(px(5.0))
                        .size(px(6.0))
                        .flex_none()
                        .rounded_full()
                        .bg(rgb(color)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .line_height(px(18.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(label),
                )
                .into_any_element()
        };

        // This status block explains the two independent glass layers. They
        // intentionally follow different render-policy gates so low-power mode
        // can keep cheap system material while disabling expensive realtime blur.
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_sunken))
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(status_row(self.tokens.ui.accent, effective_text))
            .child(status_row(status_color, self.i18n.t(status_key)))
            .child(status_row(self.tokens.ui.text_muted, self.i18n.t(blur_key)))
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_checkbox_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_row(
            label_key,
            hint_key,
            checkbox(&self.tokens, String::new(), checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                    }),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn appearance_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .relative()
            .w(px(width))
            .min_w(px(0.0))
            .child(self.settings_select_control(select_id, value, false, Some(width), cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
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
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w(px(width))
            .cursor(CursorStyle::IBeam)
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
        .into_any_element()
    }

    pub(in crate::workspace) fn appearance_radius_control(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_appearance_radius_control(
            &self.tokens,
            settings.appearance.border_radius,
            self.appearance_slider_control(
                SettingsSlider::AppearanceBorderRadius,
                SelectAnchorId::SettingsAppearanceBorderRadiusSlider,
                APPEARANCE_BORDER_RADIUS_MIN,
                APPEARANCE_BORDER_RADIUS_MAX,
                settings.appearance.border_radius as f32,
                cx,
            ),
        )
    }

    pub(in crate::workspace) fn appearance_slider_value_control(
        &self,
        slider: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        unit: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(self.appearance_slider_control(slider, anchor_id, min, max, value, cx))
            .child(
                div()
                    .w(px(48.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("{}{}", value.round() as i64, unit)),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_slider_control(
        &self,
        slider_id: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity();
        div()
            .w(px(self.tokens.metrics.settings_slider_width))
            .child(select_anchor_probe(
                anchor_id,
                slider(
                    &self.tokens,
                    SliderView {
                        min,
                        max,
                        value,
                        disabled: false,
                    },
                )
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                        this.close_settings_select();
                        this.focused_settings_input = None;
                        this.settings_slider_drag = Some(slider_id);
                        this.apply_settings_slider_from_position(
                            slider_id,
                            f32::from(event.position.x),
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                        this.finish_settings_slider_drag(cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(
                    |this, event: &MouseMoveEvent, _window, cx| {
                        this.update_settings_slider_drag(event, cx);
                    },
                )),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn appearance_theme_preview(
        &self,
        settings: &PersistedSettings,
    ) -> AnyElement {
        settings_appearance_theme_preview(&self.tokens, settings)
    }

    pub(in crate::workspace) fn render_theme_editor_modal(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self.settings_page.theme_editor.as_ref()?;
        let terminal = editor_terminal_theme(&editor.terminal_colors);
        let ui = editor_ui_colors(&editor.ui_colors);
        let title_key = if editor.edit_theme_id.is_some() {
            "settings_view.custom_theme.edit_title"
        } else {
            "settings_view.custom_theme.create_title"
        };
        let save_disabled = editor.name.trim().is_empty();
        let form_visible =
            self.theme_editor_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;

        let dialog = div()
            .w(px(THEME_EDITOR_MODAL_WIDTH))
            .max_h(px(THEME_EDITOR_MODAL_MAX_HEIGHT))
            .rounded(px(self.tokens.radii.md))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            // Header/footer/body each paint to the rounded shell edge. Keeping
            // the shell itself background-free avoids a second corner color
            // when GPUI clips overflow with a rectangular mask.
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .px(px(THEME_EDITOR_HEADER_PADDING_X))
                    .py(px(THEME_EDITOR_HEADER_PADDING_Y))
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    // Tauri's DialogContent clips this painted header through
                    // the modal radius; mirror that edge ownership in GPUI.
                    .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.md)))
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.custom_theme.description")),
                    ),
            )
            .child(
                div()
                    .id("theme-editor-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .selectable_overflow_y_scrollbar(
                        &self.selectable_text_scroll_handle("theme-editor-scroll"),
                    )
                    .px(px(THEME_EDITOR_BODY_PADDING_X))
                    .py(px(THEME_EDITOR_BODY_PADDING_Y))
                    .bg(rgb(self.tokens.ui.bg_elevated))
                    .flex()
                    .flex_col()
                    .gap(px(THEME_EDITOR_BODY_GAP))
                    .child(self.theme_editor_name_duplicate_row(editor, cx))
                    .child(self.theme_editor_preview(editor, terminal, ui))
                    .child(self.theme_editor_section_tabs(editor, cx))
                    .child(self.theme_editor_color_grid(editor, cx)),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(THEME_EDITOR_HEADER_PADDING_X))
                    .py(px(THEME_EDITOR_HEADER_PADDING_Y))
                    .border_t_1()
                    .border_color(rgb(self.tokens.ui.border))
                    // The footer background sits on the shell edge, so it must
                    // own the bottom corners instead of relying on rectangular clipping.
                    .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.md)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .child(if editor.edit_theme_id.is_some() {
                        self.theme_editor_footer_button(
                            LucideIcon::Trash2,
                            self.i18n.t("settings_view.custom_theme.delete"),
                            self.tokens.ui.error,
                            cx.listener(|this, _event, _window, cx| {
                                this.delete_theme_editor_theme(cx);
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                // ThemeEditorModal uses normal shadcn Button
                                // footer actions in Tauri; route clicks through
                                // the shared workspace guard rather than a raw
                                // primitive-level mouse handler.
                                self.workspace_toolbar_action_button(
                                    self.i18n.t("settings_view.custom_theme.cancel"),
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
                                        this.close_theme_editor(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                            .child(self.workspace_toolbar_action_button(
                                self.i18n.t("settings_view.custom_theme.save"),
                                Some(Self::render_lucide_icon(
                                    LucideIcon::Save,
                                    12.0,
                                    rgb(self.tokens.ui.accent_text),
                                )),
                                ToolbarButtonOptions {
                                    button: ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: save_disabled,
                                    },
                                    ..ToolbarButtonOptions::default()
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.save_theme_editor(cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            );

        Some(
            dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        // Tauri ThemeEditorModal passes Dialog onOpenChange
                        // directly through, so overlay close cancels editing.
                        this.close_theme_editor(cx);
                        cx.stop_propagation();
                    }),
                )
                .child(oxideterm_gpui_ui::motion::form_transition(
                    &self.tokens,
                    "theme-editor-form-enter",
                    dialog,
                    form_visible,
                ))
                .when(!form_visible, |backdrop| {
                    backdrop.child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .occlude()
                            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                                cx.stop_propagation();
                            })
                            .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation()),
                    )
                })
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn theme_editor_name_duplicate_row(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        settings_theme_editor_name_duplicate_row(
            self.theme_editor_label("settings_view.custom_theme.name"),
            self.theme_editor_text_input(
                SettingsInput::CustomThemeName,
                editor.name.clone(),
                self.i18n.t("settings_view.custom_theme.name_placeholder"),
                0.0,
                false,
                true,
                cx,
            ),
            editor
                .edit_theme_id
                .is_none()
                .then(|| self.theme_editor_duplicate_row(editor, cx)),
        )
    }

    pub(in crate::workspace) fn theme_editor_duplicate_row(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = if editor.duplicate_theme_touched {
            theme_display_name(&editor.duplicate_theme)
        } else {
            self.i18n.t("settings_view.custom_theme.select_base")
        };
        settings_theme_editor_duplicate_row(
            self.theme_editor_label("settings_view.custom_theme.duplicate_from"),
            self.theme_editor_duplicate_select(value, cx),
        )
    }
    pub(in crate::workspace) fn theme_editor_duplicate_select(
        &self,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::CustomThemeDuplicate;
        self.settings_select_control_with_trigger_style(
            select_id,
            value,
            false,
            Some(THEME_EDITOR_DUPLICATE_WIDTH),
            |trigger| trigger.h(px(THEME_EDITOR_INPUT_HEIGHT)),
            cx,
        )
    }

    pub(in crate::workspace) fn theme_editor_preview(
        &self,
        editor: &ThemeEditorState,
        terminal: TerminalTheme,
        ui: AppUiColors,
    ) -> AnyElement {
        settings_theme_editor_preview(
            &self.tokens,
            &editor.name,
            terminal,
            ui,
            settings_mono_font_family(self.settings_store.settings()),
        )
    }

    pub(in crate::workspace) fn theme_editor_section_tabs(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(self.theme_editor_section_tab(
                ThemeEditorSection::Terminal,
                "settings_view.custom_theme.terminal_colors",
                editor.active_section,
                cx,
            ))
            .child(self.theme_editor_section_tab(
                ThemeEditorSection::Ui,
                "settings_view.custom_theme.ui_colors",
                editor.active_section,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_section_tab(
        &self,
        section: ThemeEditorSection,
        label_key: &str,
        active_section: ThemeEditorSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = section == active_section;
        div()
            .px(px(12.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .bg(rgba(0x00000000))
            .cursor_pointer()
            .hover(|tab| tab.text_color(rgb(self.tokens.ui.text)))
            .child(div().child(self.i18n.t(label_key)))
            .child(div().h(px(2.0)).w_full().bg(if active {
                rgb(self.tokens.ui.accent)
            } else {
                rgba(0x00000000)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(editor) = this.settings_page.theme_editor.as_mut() {
                        editor.active_section = section;
                    }
                    this.close_settings_select();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_color_grid(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if editor.active_section == ThemeEditorSection::Ui {
            return self.theme_editor_ui_color_sections(cx);
        }

        let (fields, colors, section) = match editor.active_section {
            ThemeEditorSection::Terminal => (
                TERMINAL_THEME_COLOR_FIELDS,
                editor.terminal_colors.as_slice(),
                ThemeEditorSection::Terminal,
            ),
            ThemeEditorSection::Ui => unreachable!("UI colors render grouped sections"),
        };
        self.theme_editor_color_grid_for_fields(fields, colors, section, cx)
    }

    pub(in crate::workspace) fn theme_editor_ui_color_sections(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(editor) = self.settings_page.theme_editor.as_ref() else {
            return div().into_any_element();
        };
        let colors = editor.ui_colors.as_slice();

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.custom_theme.ui_colors_hint")),
                    )
                    .child(
                        // Mirrors Tauri ThemeEditorModal's outline Button with
                        // Copy icon, with activation guarded by the shared
                        // workspace Button wrapper.
                        self.workspace_toolbar_action_button(
                            self.i18n.t("settings_view.custom_theme.auto_derive"),
                            Some(Self::render_lucide_icon(
                                LucideIcon::Copy,
                                12.0,
                                rgb(self.tokens.ui.text),
                            )),
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
                                if let Some(editor) = this.settings_page.theme_editor.as_mut() {
                                    let ui = derive_ui_colors_from_terminal(editor_terminal_theme(
                                        &editor.terminal_colors,
                                    ));
                                    editor.ui_colors = app_ui_colors_to_colors(ui);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_background",
                &[0, 1, 2, 3, 4, 5, 6, 7],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_text",
                &[8, 9, 10],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_border",
                &[12, 13, 14],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_accent",
                &[15, 16, 17, 18],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_semantic",
                &[19, 20, 21, 22],
                colors,
                cx,
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_ui_section(
        &self,
        title_key: &str,
        indexes: &[usize],
        colors: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut cells = Vec::new();
        for &index in indexes {
            let Some(field) = UI_THEME_COLOR_FIELDS.get(index) else {
                continue;
            };
            let color = colors
                .get(index)
                .cloned()
                .unwrap_or_else(|| "#000000".to_string());
            cells.push(self.theme_editor_color_cell(
                field,
                color,
                SettingsInput::CustomThemeUiColor(index),
                cx,
            ));
        }

        settings_theme_editor_color_section(&self.tokens, self.i18n.t(title_key), cells)
    }

    pub(in crate::workspace) fn theme_editor_color_grid_for_fields(
        &self,
        fields: &[ThemeColorField],
        colors: &[String],
        section: ThemeEditorSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut cells = Vec::new();
        for (index, field) in fields.iter().enumerate() {
            let color = colors
                .get(index)
                .cloned()
                .unwrap_or_else(|| "#000000".to_string());
            let input = match section {
                ThemeEditorSection::Terminal => SettingsInput::CustomThemeTerminalColor(index),
                ThemeEditorSection::Ui => SettingsInput::CustomThemeUiColor(index),
            };
            cells.push(self.theme_editor_color_cell(field, color, input, cx));
        }
        settings_theme_editor_color_grid(cells)
    }

    pub(in crate::workspace) fn theme_editor_color_cell(
        &self,
        field: &ThemeColorField,
        color: String,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let parsed = parse_color_hex(&color).unwrap_or(0);
        let focused = self.focused_settings_input == Some(input);
        let label = self.i18n.t(&format!(
            "settings_view.custom_theme.colors.{}",
            field.label_key
        ));
        let swatch = settings_theme_editor_color_swatch(&self.tokens, parsed)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            )
            .into_any_element();
        let value_control = if focused {
            self.theme_editor_text_input(
                input,
                color,
                "#RRGGBB".to_string(),
                SETTINGS_THEME_EDITOR_HEX_INPUT_WIDTH,
                true,
                false,
                cx,
            )
        } else {
            settings_theme_editor_color_value(
                &self.tokens,
                color,
                settings_mono_font_family(self.settings_store.settings()),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
        };
        settings_theme_editor_color_cell(&self.tokens, label, swatch, value_control)
    }

    pub(in crate::workspace) fn theme_editor_label(&self, key: &str) -> AnyElement {
        settings_theme_editor_label(&self.tokens, self.i18n.t(key))
    }

    pub(in crate::workspace) fn theme_editor_text_input(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        mono: bool,
        fill: bool,
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
        let control = settings_theme_editor_text_input(
            &self.tokens,
            TextInputView {
                value: display_value,
                placeholder,
                focused,
                caret_visible: self.new_connection_caret_visible,
                secret: false,
                selected_all: false,
                selected_range: self.ime_selected_range_for_target(target),
                marked_text: self.marked_text_for_target(target),
            },
            width,
            mono,
            fill,
            settings_mono_font_family(self.settings_store.settings()),
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
        text_input_anchor_probe(target.anchor_id(), control, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        })
        .into_any_element()
    }

    pub(in crate::workspace) fn theme_editor_footer_button(
        &self,
        icon: LucideIcon,
        label: String,
        color: u32,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        // Theme editor delete uses a color-tinted outline button in Tauri.
        // Keep only the tint local; activation still goes through the shared
        // Button guard used by the rest of the modal footer.
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(icon, 12.0, rgb(color))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_gap: Some(4.0),
                border: Some(rgba((color << 8) | 0x4d)),
                text_color: Some(rgb(color)),
                hover_background: Some(rgba((color << 8) | 0x1a)),
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    pub(in crate::workspace) fn open_theme_editor(
        &mut self,
        edit_theme_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.settings_page
            .open_theme_editor(theme_editor_from_settings(
                self.settings_store.settings(),
                edit_theme_id,
                self.i18n.t("settings_view.custom_theme.new_theme_name"),
            ));
        self.theme_editor_presence.reopen();
        self.close_settings_select();
        self.focused_settings_input = None;
        cx.notify();
    }

    pub(in crate::workspace) fn close_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.close_settings_select();
        self.focused_settings_input = None;
        let Some(generation) = self.theme_editor_presence.begin_exit() else {
            return;
        };
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.finish_theme_editor_exit(generation);
            cx.notify();
            return;
        }
        cx.spawn(async move |weak, cx| {
            gpui::Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this.finish_theme_editor_exit(generation) {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_theme_editor_exit(&mut self, generation: u64) -> bool {
        if !self.theme_editor_presence.finish_exit(generation) {
            return false;
        }
        self.settings_page.close_theme_editor();
        self.theme_editor_presence.reopen();
        true
    }

    pub(in crate::workspace) fn save_theme_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.settings_page.theme_editor.clone() else {
            return;
        };
        let notice_name = editor.name.trim().to_string();
        if notice_name.is_empty() {
            return;
        }
        self.edit_settings(
            move |settings| {
                let _ = save_theme_editor_to_settings(settings, editor);
            },
            cx,
        );
        self.close_theme_editor(cx);
        self.focused_settings_input = None;
        self.send_settings_notice(
            self.i18n
                .t("settings_view.appearance.theme_import_success")
                .replace("{{name}}", &notice_name),
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    pub(in crate::workspace) fn delete_theme_editor_theme(&mut self, cx: &mut Context<Self>) {
        let Some(theme_id) = self
            .settings_page
            .theme_editor
            .as_ref()
            .and_then(|editor| editor.edit_theme_id.clone())
        else {
            return;
        };
        self.edit_settings(
            move |settings| {
                delete_custom_theme_from_settings(settings, &theme_id, "azurite");
            },
            cx,
        );
        self.close_theme_editor(cx);
        self.focused_settings_input = None;
        cx.notify();
    }

    pub(in crate::workspace) fn import_theme_from_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.appearance.theme_import"),
            )),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = fs::read_to_string(&path)
                .map_err(|err| err.to_string())
                .and_then(|contents| import_custom_theme(&contents));
            let _ = weak.update(cx, |this, cx| match result {
                Ok((theme_id, name, value)) => {
                    let selected_theme_id = theme_id.clone();
                    this.edit_settings(
                        move |settings| {
                            settings
                                .custom_themes
                                .insert(theme_id.clone(), value.clone());
                            settings.terminal.theme = selected_theme_id;
                        },
                        cx,
                    );
                    this.send_settings_notice(
                        this.i18n
                            .t("settings_view.appearance.theme_import_success")
                            .replace("{{name}}", &name),
                        TerminalNoticeVariant::Success,
                    );
                }
                Err(error) => {
                    this.send_settings_notice(
                        this.i18n
                            .t("settings_view.appearance.theme_import_error")
                            .replace("{{error}}", &error),
                        TerminalNoticeVariant::Error,
                    );
                }
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn send_settings_notice(
        &self,
        title: String,
        variant: TerminalNoticeVariant,
    ) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }

    pub(in crate::workspace) fn appearance_background_image_slot(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_background_image = settings.terminal.background_image.is_some();
        let pick_label = if has_background_image {
            self.i18n.t("settings_view.terminal.bg_change")
        } else {
            self.i18n.t("settings_view.terminal.bg_add")
        };
        let pick_icon = if has_background_image {
            LucideIcon::Image
        } else {
            LucideIcon::Plus
        };
        let actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .child(self.appearance_action_button(
                pick_icon,
                pick_label,
                cx.listener(|this, _event, _window, cx| {
                    this.pick_background_image(cx);
                    cx.stop_propagation();
                }),
            ))
            .when(has_background_image, |actions| {
                actions.child(
                    settings_background_clear_all_button(
                        &self.tokens,
                        self.i18n.t("settings_view.terminal.bg_clear"),
                        Self::render_lucide_icon(
                            LucideIcon::Trash2,
                            14.0,
                            rgb(self.tokens.ui.error),
                        ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| {
                                    settings.terminal.background_image = None;
                                },
                                cx,
                            );
                        }),
                    ),
                )
            })
            .into_any_element();
        settings_background_gallery(
            &self.tokens,
            self.i18n.t("settings_view.terminal.bg_label"),
            actions,
            self.background_image_slot_content(settings, cx),
        )
    }

    pub(in crate::workspace) fn background_image_slot_content(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(current) = settings.terminal.background_image.as_deref() else {
            return settings_background_empty_hint(
                &self.tokens,
                self.i18n.t("settings_view.terminal.bg_hint"),
            );
        };

        settings_background_thumbnails_layout(self.background_thumbnail(current, true, cx))
    }

    pub(in crate::workspace) fn pick_background_image(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.terminal.bg_add"),
            )),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            if !is_supported_background_image(&path) {
                return;
            }
            let image_path = path.to_string_lossy().to_string();
            let _ = weak.update(cx, |this, cx| {
                this.edit_settings(
                    move |settings| {
                        settings.terminal.background_image = Some(image_path);
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    pub(in crate::workspace) fn background_thumbnail(
        &self,
        image_path: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let image_path = image_path.to_string();
        let fallback_icon_color = self.tokens.ui.text_muted;
        let thumbnail = settings_background_thumbnail_frame(
            &self.tokens,
            &image_path,
            active,
            self.i18n.t("settings_view.terminal.bg_active"),
            move || {
                WorkspaceApp::render_lucide_icon(LucideIcon::Image, 20.0, rgb(fallback_icon_color))
            },
        );
        thumbnail
            .child(
                settings_background_thumbnail_remove_button(
                    &self.tokens,
                    Self::render_lucide_icon(LucideIcon::X, 12.0, rgb(self.tokens.ui.text)),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.edit_settings(
                            |settings| {
                                settings.terminal.background_image = None;
                            },
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let selected_path = image_path.clone();
                    this.edit_settings(
                        move |settings| {
                            settings.terminal.background_image = Some(selected_path);
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
    pub(in crate::workspace) fn appearance_background_tabs(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut pills = Vec::new();
        for (key, label_key, icon) in background_tab_options() {
            let enabled = settings
                .terminal
                .background_enabled_tabs
                .iter()
                .any(|tab| tab == key);
            let key = (*key).to_string();
            pills.push(
                self.background_tab_pill(
                    &key,
                    *label_key,
                    settings_background_tab_lucide(*icon),
                    enabled,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_background_tab(&key, cx);
                    }),
                )
                .into_any_element(),
            );
        }

        settings_background_tabs_section(
            &self.tokens,
            self.i18n.t("settings_view.terminal.bg_tabs"),
            self.i18n.t("settings_view.terminal.bg_tabs_hint"),
            pills,
        )
    }

    pub(in crate::workspace) fn background_tab_pill(
        &self,
        _key: &str,
        label_key: &str,
        icon: LucideIcon,
        enabled: bool,
    ) -> Div {
        let color = if enabled {
            self.tokens.ui.accent
        } else {
            self.tokens.ui.text_muted
        };
        settings_background_tab_pill(
            &self.tokens,
            self.i18n.t(label_key),
            Self::render_lucide_icon(
                icon,
                self.tokens.metrics.settings_background_tab_icon_size,
                rgb(color),
            ),
            enabled,
        )
    }

    pub(in crate::workspace) fn toggle_background_tab(
        &mut self,
        key: &str,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            |settings| {
                if let Some(index) = settings
                    .terminal
                    .background_enabled_tabs
                    .iter()
                    .position(|tab| tab == key)
                {
                    settings.terminal.background_enabled_tabs.remove(index);
                } else {
                    settings
                        .terminal
                        .background_enabled_tabs
                        .push(key.to_string());
                }
            },
            cx,
        );
    }
}
