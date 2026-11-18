use super::*;

impl FileManagerView {
    pub(super) fn render_file_manager_preview_audio(
        &self,
        name: String,
        _path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let snapshot = self.file_manager.read(cx).preview_audio.snapshot();
        let duration = snapshot.duration.unwrap_or_default();
        let position = snapshot.position.min(duration);
        let progress = if duration.is_zero() {
            0.0
        } else {
            (position.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
        };
        let play_icon = if snapshot.state == AudioPreviewState::Playing {
            LucideIcon::Pause
        } else {
            LucideIcon::Play
        };
        let playback_disabled = snapshot.state == AudioPreviewState::Error;
        let can_seek = snapshot.duration.is_some() && snapshot.state != AudioPreviewState::Error;
        div()
            .w_full()
            .min_h(px(520.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p_4()
            .gap(px(16.0))
            .child(Self::render_lucide_icon(
                LucideIcon::FileAudio,
                56.0,
                rgb(FILE_MANAGER_PURPLE),
            ))
            .child(
                div()
                    .max_w(px(448.0))
                    .truncate()
                    .text_size(px(FILE_MANAGER_TEXT_SM))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "file-preview-audio-name",
                        name.clone(),
                        name,
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(520.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .px_3()
                    .py_2()
                    .child(self.workspace_icon_action_button(
                        play_icon,
                        14.0,
                        rgb(theme.text),
                        IconButtonOptions {
                            disabled: playback_disabled,
                            background: Some(rgb(theme.bg)),
                            border: Some(rgb(theme.border)),
                            hover_background: Some(rgb(theme.bg_hover)),
                            // Local preview audio shares the browser button boundary with
                            // SFTP preview audio; decode errors must not leave a live click target.
                            ..IconButtonOptions::opaque_toolbar(32.0, ButtonRadius::Md)
                        },
                        |this, _event, _window, cx| {
                            this.toggle_file_manager_preview_audio(cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .h(px(6.0))
                            .rounded(px(self.tokens.radii.sm))
                            .overflow_hidden()
                            .bg(rgb(theme.bg_sunken))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(progress))
                                    .rounded(px(self.tokens.radii.sm))
                                    .bg(rgb(theme.accent)),
                            ),
                    )
                    .child(
                        div()
                            .min_w(px(92.0))
                            .text_size(px(FILE_MANAGER_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "file-preview-audio-time",
                                "position",
                                format!(
                                    "{} / {}",
                                    format_file_manager_media_time(position),
                                    format_file_manager_media_time(duration)
                                ),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .when(can_seek, |row| {
                        row.child(self.render_file_manager_media_seek_button(
                            "-15s",
                            cx.listener(|this, _event, _window, cx| {
                                let now =
                                    this.file_manager.read(cx).preview_audio.snapshot().position;
                                this.seek_file_manager_preview_audio(
                                    now.saturating_sub(std::time::Duration::from_secs(15)),
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                            cx,
                        ))
                        .child(
                            self.render_file_manager_media_seek_button(
                                "+15s",
                                cx.listener(|this, _event, _window, cx| {
                                    let snapshot =
                                        this.file_manager.read(cx).preview_audio.snapshot();
                                    let Some(duration) = snapshot.duration else {
                                        return;
                                    };
                                    this.seek_file_manager_preview_audio(
                                        (snapshot.position + std::time::Duration::from_secs(15))
                                            .min(duration),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                                cx,
                            ),
                        )
                    })
                    .when_some(snapshot.error, |row, error| {
                        row.child(
                            div()
                                .text_size(px(FILE_MANAGER_TEXT_XS))
                                .text_color(rgb(FILE_MANAGER_RED))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "file-preview-audio-error",
                                    error.clone(),
                                    error,
                                    FILE_MANAGER_RED,
                                    cx,
                                )),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "file-preview-audio-mime",
                        mime_type,
                        mime_type.to_string(),
                        theme.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_file_manager_preview_video(
        &self,
        name: String,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let video_surface = self.file_manager.read(cx).preview_video_surface.clone();
        let snapshot = (video_surface.snapshot)();
        let detail = snapshot
            .error
            .unwrap_or_else(|| "Native video playback is initializing.".to_string());
        let fallback = self
            .render_file_manager_native_asset_status_with_external(
                name, path, mime_type, &detail, cx,
            )
            .into_any_element();
        (video_surface.render)(path.to_string(), fallback)
    }

    pub(super) fn render_file_manager_preview_font(
        &self,
        name: String,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (font_error, font_family, font_size) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.preview_font_error.clone(),
                file_manager.preview_font_family.clone(),
                file_manager.preview_font_size,
            )
        };
        if let Some(error) = font_error {
            return self.render_file_manager_preview_status(
                LucideIcon::FileText,
                self.i18n.t("fileManager.fontLoadError"),
                Some(error),
                false,
                cx,
            );
        }
        let Some(font_family) = font_family else {
            return self.render_file_manager_preview_status(
                LucideIcon::LoaderCircle,
                self.i18n.t("fileManager.loadingFont"),
                Some(path.to_string()),
                false,
                cx,
            );
        };
        let sample_font = SharedString::from(font_family.clone());
        div()
            .size_full()
            .min_h(px(520.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .bg(rgba((theme.bg_panel << 8) | FILE_MANAGER_PANEL_80_ALPHA))
                    .child(self.render_file_manager_font_size_button(
                        "-",
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.file_manager.update(cx, |file_manager, cx| {
                                file_manager.preview_font_size =
                                    (file_manager.preview_font_size - 4.0).max(8.0);
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }),
                        cx,
                    ))
                    .child(
                        div()
                            .w(px(52.0))
                            .text_center()
                            .text_size(px(FILE_MANAGER_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "file-manager-font-size",
                                &name,
                                format!("{font_size:.0}px"),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(self.render_file_manager_font_size_button(
                        "+",
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.file_manager.update(cx, |file_manager, cx| {
                                file_manager.preview_font_size =
                                    (file_manager.preview_font_size + 4.0).min(120.0);
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }),
                        cx,
                    ))
                    .children([16.0, 24.0, 32.0, 48.0, 72.0].into_iter().map(|size| {
                        self.render_file_manager_font_size_button(
                            format!("{size:.0}"),
                            (font_size - size).abs() < f32::EPSILON,
                            cx.listener(move |this, _event, _window, cx| {
                                this.file_manager.update(cx, |file_manager, cx| {
                                    file_manager.preview_font_size = size;
                                    cx.notify();
                                });
                                cx.stop_propagation();
                            }),
                            cx,
                        )
                    }))
                    .child(
                        div()
                            .ml_auto()
                            .text_size(px(FILE_MANAGER_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "file-manager-font-mime",
                                &name,
                                format!("{name} · {mime_type}"),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .id("file-manager-metadata-scroll")
                    .flex_1()
                    .selectable_overflow_y_scrollbar(
                        &self.file_manager.read(cx).preview_metadata_scroll,
                    )
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .child(
                        div()
                            .text_size(px(FILE_MANAGER_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(self.render_selectable_display_text(
                                "file-manager-font-family",
                                &name,
                                font_family,
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family(sample_font.clone())
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.3))
                            .text_color(rgb(theme.text))
                            .child(self.render_selectable_display_text(
                                "file-manager-font-sample-alpha",
                                &name,
                                "The quick brown fox jumps over the lazy dog.",
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family(sample_font.clone())
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.3))
                            .text_color(rgb(theme.text))
                            .child(self.render_selectable_display_text(
                                "file-manager-font-sample-symbols",
                                &name,
                                "0123456789 !@#$%^&*() []{} <>",
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family(sample_font)
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.3))
                            .text_color(rgb(theme.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "file-preview-font-sample",
                                "zh",
                                "中文字体预览：家里的主机 OxideTerm 终端文件管理器",
                                theme.text,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_file_manager_media_seek_button(
        &self,
        label: &'static str,
        on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.workspace_toolbar_action_button(
            label.to_string(),
            None,
            ToolbarButtonOptions {
                text_color: Some(rgb(theme.text_muted)),
                hover_background: Some(rgb(theme.bg_hover)),
                hover_text_color: Some(rgb(theme.text)),
                // Tauri audio controls are real buttons, so their text never
                // enters document selection; shared toolbar_button preserves
                // that browser button boundary for native preview controls.
                ..ToolbarButtonOptions::compact_text(
                    ButtonVariant::Ghost,
                    ButtonRadius::Sm,
                    24.0,
                    8.0,
                    FILE_MANAGER_TEXT_XS,
                )
            },
            on_click,
        )
        .into_any_element()
    }

    pub(super) fn render_file_manager_font_size_button(
        &self,
        label: impl Into<String>,
        active: bool,
        on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = label.into();
        let text_color = if active { theme.text } else { theme.text_muted };
        self.workspace_toolbar_action_button(
            label,
            None,
            ToolbarButtonOptions {
                background: Some(if active {
                    rgb(theme.bg_hover)
                } else {
                    rgb(theme.bg_panel)
                }),
                text_color: Some(rgb(text_color)),
                hover_background: Some(rgb(theme.bg_hover)),
                hover_text_color: Some(rgb(theme.text)),
                ..ToolbarButtonOptions::compact_text_min_width(
                    ButtonVariant::Secondary,
                    ButtonRadius::Sm,
                    28.0,
                    28.0,
                    8.0,
                    FILE_MANAGER_TEXT_XS,
                )
            },
            on_click,
        )
        .into_any_element()
    }
}
