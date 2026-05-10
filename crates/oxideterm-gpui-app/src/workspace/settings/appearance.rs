impl WorkspaceApp {
    fn settings_appearance(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.appearance_theme_card(settings, cx),
            self.appearance_layout_card(settings, cx),
            self.appearance_background_card(settings, cx),
        ]
    }

    fn appearance_theme_card(
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
                    ))
                    .child(self.appearance_action_button(
                        LucideIcon::Plus,
                        self.i18n.t("settings_view.custom_theme.create"),
                    ))
                    .into_any_element(),
            ),
            vec![
                self.appearance_row(
                    "settings_view.appearance.color_theme",
                    "settings_view.appearance.color_theme_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceTheme,
                        theme_display_name(&settings.terminal.theme),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                ),
                self.appearance_theme_preview(settings),
            ],
        )
    }

    fn appearance_layout_card(
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
            ],
        )
    }

    fn appearance_background_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let background_blur = self
            .background_blur_preview
            .unwrap_or(settings.terminal.background_blur);
        self.appearance_card_with_icon(
            LucideIcon::Image,
            self.i18n.t("settings_view.terminal.bg_title"),
            vec![
                self.appearance_checkbox_row(
                    "settings_view.terminal.bg_enabled",
                    "settings_view.terminal.bg_enabled_hint",
                    settings.terminal.background_enabled,
                    set_terminal_background_enabled,
                    cx,
                ),
                self.appearance_background_gallery(settings, cx),
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
                self.appearance_background_tabs(settings, cx),
            ],
        )
    }

    fn appearance_card(
        &self,
        title: String,
        actions: Option<AnyElement>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(self.appearance_card_title(title, None))
                .when_some(actions, |header, actions| header.child(actions))
                .into_any_element(),
            rows,
        )
    }

    fn appearance_card_with_icon(
        &self,
        icon: LucideIcon,
        title: String,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(self.appearance_card_title(title, Some(icon)), rows)
    }

    fn appearance_card_shell(&self, header: AnyElement, rows: Vec<AnyElement>) -> AnyElement {
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
            .child(header)
            .children(rows)
            .into_any_element()
    }

    fn appearance_card_title(&self, title: String, icon: Option<LucideIcon>) -> AnyElement {
        let mut title_el = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text));
        if let Some(icon) = icon {
            title_el = title_el.child(Self::render_lucide_icon(
                icon,
                16.0,
                rgb(self.tokens.ui.text),
            ));
        }
        title_el.child(title.to_uppercase()).into_any_element()
    }

    fn appearance_action_button(&self, icon: LucideIcon, label: String) -> Div {
        div()
            .h(px(self.tokens.metrics.settings_appearance_action_height))
            .px(px(10.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgba(0x00000000))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            ))
            .child(label)
    }

    fn appearance_row(&self, label_key: &str, hint_key: &str, control: AnyElement) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
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
            .child(control)
            .into_any_element()
    }

    fn appearance_checkbox_row(
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

    fn appearance_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, value, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        div()
            .relative()
            .w(px(width))
            .min_w(px(0.0))
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn appearance_text_input_control(
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
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w(px(width))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
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

    fn appearance_radius_control(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .size(px(28.0))
                    .rounded(px(settings.appearance.border_radius as f32))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_secondary)),
            )
            .child(self.appearance_slider_control(
                SettingsSlider::AppearanceBorderRadius,
                SelectAnchorId::SettingsAppearanceBorderRadiusSlider,
                0.0,
                24.0,
                settings.appearance.border_radius as f32,
                cx,
            ))
            .child(
                div()
                    .w(px(48.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("{}px", settings.appearance.border_radius)),
            )
            .into_any_element()
    }

    fn appearance_slider_value_control(
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

    fn appearance_slider_control(
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
                        this.open_settings_select = None;
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

    fn appearance_theme_preview(&self, settings: &PersistedSettings) -> AnyElement {
        let terminal = self.tokens.terminal;
        div()
            .w_full()
            .mt(px(self.tokens.metrics.settings_font_preview_margin_top))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(terminal.background))
            .p(px(self.tokens.metrics.settings_theme_preview_padding))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.settings_theme_preview_dot_gap))
                    .child(self.preview_dot(terminal.red))
                    .child(self.preview_dot(terminal.yellow))
                    .child(self.preview_dot(terminal.green)),
            )
            .child(
                div()
                    .font_family(
                        settings
                            .terminal
                            .font_family
                            .terminal_family_name(&settings.terminal.custom_font_family),
                    )
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(self.tokens.metrics.settings_theme_preview_line_height))
                    .text_color(rgb(terminal.foreground))
                    .flex()
                    .flex_col()
                    .child("$ echo \"Hello World\"")
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(6.0))
                            .child(div().text_color(rgb(terminal.blue)).child("~"))
                            .child(div().text_color(rgb(terminal.magenta)).child("git"))
                            .child(div().text_color(rgb(terminal.blue)).child("status")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.0))
                            .child(">")
                            .child(div().w(px(9.0)).h(px(18.0)).bg(rgb(terminal.cursor))),
                    ),
            )
            .into_any_element()
    }

    fn preview_dot(&self, color: u32) -> AnyElement {
        div()
            .size(px(self.tokens.metrics.settings_theme_preview_dot_size))
            .rounded_full()
            .bg(rgb(color))
            .into_any_element()
    }

    fn appearance_background_gallery(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.terminal.bg_gallery")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                self.appearance_action_button(
                                    LucideIcon::Plus,
                                    self.i18n.t("settings_view.terminal.bg_add"),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.pick_background_image(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                            .when(settings.terminal.background_image.is_some(), |actions| {
                                actions.child(
                                    div()
                                        .h(px(self
                                            .tokens
                                            .metrics
                                            .settings_appearance_action_height))
                                        .px(px(10.0))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(6.0))
                                        .rounded(px(self.tokens.radii.md))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(self.tokens.ui.error))
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(rgba((self.tokens.ui.error << 8) | 0x14))
                                        })
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Trash2,
                                            14.0,
                                            rgb(self.tokens.ui.error),
                                        ))
                                        .child(self.i18n.t("settings_view.terminal.bg_clear_all"))
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
                            }),
                    ),
            )
            .child(self.background_thumbnails(settings, cx))
            .into_any_element()
    }

    fn background_thumbnails(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(current) = settings.terminal.background_image.as_deref() else {
            return div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.terminal.bg_hint"))
                .into_any_element();
        };

        div()
            .w_full()
            .grid()
            .grid_cols(4)
            .gap(px(8.0))
            .child(self.background_thumbnail(current, true, cx))
            .into_any_element()
    }

    fn pick_background_image(&mut self, cx: &mut Context<Self>) {
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

    fn background_thumbnail(
        &self,
        image_path: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let image_path = image_path.to_string();
        let image_source = std::path::PathBuf::from(&image_path);
        let fallback_label = std::path::Path::new(&image_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&image_path)
            .to_string();
        let fallback_text_size = self.tokens.metrics.ui_text_xs;
        let fallback_text_color = self.tokens.ui.text_muted;
        let fallback_icon_color = self.tokens.ui.text_muted;
        let fallback_bg = self.tokens.ui.bg_sunken;
        let image = gpui::img(image_source)
            .w_full()
            .h_full()
            .object_fit(ObjectFit::Cover)
            .with_fallback(move || {
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .bg(rgb(fallback_bg))
                    .child(WorkspaceApp::render_lucide_icon(
                        LucideIcon::Image,
                        20.0,
                        rgb(fallback_icon_color),
                    ))
                    .child(
                        div()
                            .max_w_full()
                            .px(px(8.0))
                            .text_size(px(fallback_text_size))
                            .text_color(rgb(fallback_text_color))
                            .truncate()
                            .child(fallback_label.clone()),
                    )
                    .into_any_element()
            });

        div()
            .relative()
            .h(px(self.tokens.metrics.settings_background_thumb_height))
            .rounded(px(self.tokens.radii.md))
            .overflow_hidden()
            .border_2()
            .border_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.border
            }))
            .cursor_pointer()
            .child(image)
            .when(active, |thumb| {
                thumb.child(
                    div()
                        .absolute()
                        .top(px(8.0))
                        .left(px(8.0))
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgb(self.tokens.ui.accent))
                        .px(px(self.tokens.metrics.settings_background_badge_padding_x))
                        .py(px(self.tokens.metrics.settings_background_badge_padding_y))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.accent_text))
                        .child(self.i18n.t("settings_view.terminal.bg_active")),
                )
            })
            .child(
                div()
                    .absolute()
                    .top(px(6.0))
                    .right(px(6.0))
                    .p(px(3.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba(0x00000099))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::X,
                        12.0,
                        rgb(self.tokens.ui.text),
                    ))
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
    fn appearance_background_tabs(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut grid = div().w_full().grid().grid_cols(3).gap(px(10.0));
        for (key, label_key, icon) in background_tab_options() {
            let enabled = settings
                .terminal
                .background_enabled_tabs
                .iter()
                .any(|tab| tab == key);
            let key = (*key).to_string();
            grid = grid.child(
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
                    ),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.terminal.bg_tabs")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.terminal.bg_tabs_hint")),
                    ),
            )
            .child(grid)
            .into_any_element()
    }

    fn background_tab_pill(
        &self,
        _key: &str,
        label_key: &str,
        icon: LucideIcon,
        enabled: bool,
    ) -> Div {
        div()
            .h(px(40.0))
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(if enabled {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.border
            }))
            .bg(if enabled {
                rgba((self.tokens.ui.accent << 8) | 0x1a)
            } else {
                rgba(0x00000000)
            })
            .px(px(14.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(if enabled {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                icon,
                self.tokens.metrics.settings_background_tab_icon_size,
                rgb(if enabled {
                    self.tokens.ui.accent
                } else {
                    self.tokens.ui.text_muted
                }),
            ))
            .child(div().truncate().child(self.i18n.t(label_key)))
    }

    fn toggle_background_tab(&mut self, key: &str, cx: &mut Context<Self>) {
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
