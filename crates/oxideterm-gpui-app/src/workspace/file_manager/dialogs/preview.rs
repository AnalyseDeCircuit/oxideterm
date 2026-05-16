use super::*;
use gpui::StyledText;

impl WorkspaceApp {
    pub(super) fn render_file_manager_preview_dialog(
        &self,
        entry: LocalFileEntry,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let previewable = sorted_local_files(
            &self.file_manager.files,
            &self.file_manager.filter,
            self.file_manager.sort_field,
            self.file_manager.sort_direction,
        )
        .into_iter()
        .filter(|file| file.file_type != LocalFileType::Directory)
        .collect::<Vec<_>>();
        let current_index = previewable
            .iter()
            .position(|file| file.path == entry.path)
            .unwrap_or(0);
        let can_navigate = previewable.len() > 1;
        let preview_icon = self
            .file_manager
            .preview
            .as_ref()
            .map(preview_icon)
            .unwrap_or(if entry.file_type == LocalFileType::Symlink {
                LucideIcon::Link2
            } else {
                LucideIcon::File
            });
        let show_markdown_toggle = matches!(
            self.file_manager.preview,
            Some(LocalPreview::Markdown { .. })
        );
        let can_copy = self.file_manager.preview.as_ref().is_some_and(|preview| {
            matches!(
                preview,
                LocalPreview::Text { .. } | LocalPreview::Markdown { .. }
            )
        });
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .h(px(48.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .border_b_1()
                    .border_color(file_manager_border(theme.border, has_background))
                    .bg(file_manager_panel_bg(
                        theme.bg_panel,
                        has_background,
                        FILE_MANAGER_PANEL_80_ALPHA,
                    ))
                    .when(can_navigate, |header| {
                        header
                            .child(self.render_file_manager_preview_button(
                                LucideIcon::ChevronLeft,
                                false,
                                cx.listener(|this, _event, _window, cx| {
                                    this.navigate_file_manager_preview(-1, cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(
                                div()
                                    .w(px(48.0))
                                    .text_center()
                                    .text_size(px(FILE_MANAGER_TEXT_XS))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!(
                                        "{} / {}",
                                        current_index + 1,
                                        previewable.len()
                                    )),
                            )
                            .child(self.render_file_manager_preview_button(
                                LucideIcon::ChevronRight,
                                false,
                                cx.listener(|this, _event, _window, cx| {
                                    this.navigate_file_manager_preview(1, cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                    })
                    .child(Self::render_lucide_icon(
                        preview_icon,
                        FILE_MANAGER_ICON_MD,
                        rgb(theme.text_muted),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(FILE_MANAGER_TEXT_SM))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(entry.name.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(FILE_MANAGER_TEXT_XS))
                                    .text_color(rgb(theme.text_muted))
                                    .child(entry.path.clone()),
                            ),
                    )
                    .when(can_copy, |header| {
                        header.child(self.render_file_manager_preview_button(
                            LucideIcon::Copy,
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.copy_file_manager_preview_content(cx);
                                cx.stop_propagation();
                            }),
                        ))
                    })
                    .when(show_markdown_toggle, |header| {
                        header.child(self.render_file_manager_preview_button(
                            if self.file_manager.preview_markdown_source {
                                LucideIcon::Eye
                            } else {
                                LucideIcon::Code2
                            },
                            self.file_manager.preview_markdown_source,
                            cx.listener(|this, _event, _window, cx| {
                                this.file_manager.preview_markdown_source =
                                    !this.file_manager.preview_markdown_source;
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ))
                    })
                    .child(self.render_file_manager_preview_button(
                        LucideIcon::Info,
                        self.file_manager.preview_show_metadata,
                        cx.listener(|this, _event, _window, cx| {
                            this.file_manager.preview_show_metadata =
                                !this.file_manager.preview_show_metadata;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
                    .child(self.render_file_manager_preview_button(
                        LucideIcon::ExternalLink,
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            if let Some(FileManagerDialog::Preview { entry }) =
                                this.file_manager.dialog.clone()
                            {
                                if let Err(error) = open_path_external(&entry.path) {
                                    this.push_file_manager_toast(
                                        this.i18n.t("fileManager.error"),
                                        Some(error),
                                        TerminalNoticeVariant::Error,
                                    );
                                }
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
                    .child(self.render_file_manager_preview_button(
                        LucideIcon::X,
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.close_file_manager_dialog();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scrollbar()
                    .bg(file_manager_bg(self.tokens.ui.bg_sunken, has_background))
                    .child(self.render_file_manager_preview_content(
                        entry.clone(),
                        has_background,
                        cx,
                    )),
            )
            .when(self.file_manager.preview_show_metadata, |dialog| {
                dialog.child(self.render_file_manager_preview_metadata(has_background))
            })
            .child(
                div()
                    .px(px(16.0))
                    .py(px(8.0))
                    .border_t_1()
                    .border_color(file_manager_border(theme.border, has_background))
                    .bg(file_manager_panel_bg(theme.bg_card, has_background, 0xff))
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(if can_navigate {
                        self.i18n.t("fileManager.quickLookHintNav")
                    } else {
                        self.i18n.t("fileManager.quickLookHint")
                    }),
            )
            .into_any_element()
    }

    fn render_file_manager_preview_content(
        &self,
        entry: LocalFileEntry,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.file_manager.preview.as_ref() {
            Some(LocalPreview::Loading) => self.render_file_manager_preview_status(
                LucideIcon::LoaderCircle,
                self.i18n.t("fileManager.loadingMore"),
                None,
                has_background,
                cx,
            ),
            Some(LocalPreview::Text { content, language }) => self
                .render_file_manager_preview_code(
                    content,
                    language.as_deref(),
                    &entry.name,
                    has_background,
                ),
            Some(LocalPreview::Markdown { content })
                if self.file_manager.preview_markdown_source =>
            {
                self.render_file_manager_preview_code(
                    content,
                    Some("markdown"),
                    &entry.name,
                    has_background,
                )
            }
            Some(LocalPreview::Markdown { content }) => {
                self.render_file_manager_preview_markdown(content, cx)
            }
            Some(LocalPreview::Image { path, mime_type }) => self
                .render_file_manager_preview_image(path, mime_type.clone())
                .into_any_element(),
            Some(LocalPreview::Video { path, mime_type }) => {
                self.render_file_manager_preview_video(entry.name, path, mime_type, cx)
            }
            Some(LocalPreview::Audio { path, mime_type }) => {
                self.render_file_manager_preview_audio(entry.name, path, mime_type, cx)
            }
            Some(LocalPreview::Font { path, mime_type }) => {
                self.render_file_manager_preview_font(entry.name, path, mime_type, cx)
            }
            Some(LocalPreview::Pdf { path, mime_type }) => self
                .render_file_manager_preview_pdf(path, mime_type)
                .into_any_element(),
            Some(LocalPreview::Archive { info }) => {
                self.render_file_manager_archive_tree(info, has_background)
            }
            Some(LocalPreview::TooLarge { size }) => self.render_file_manager_preview_status(
                LucideIcon::HelpCircle,
                self.i18n.t("fileManager.fileTooLarge"),
                Some(format!(
                    "{}: {}",
                    self.i18n.t("fileManager.fileSize"),
                    format_file_size(*size)
                )),
                has_background,
                cx,
            ),
            Some(LocalPreview::Unsupported(key)) => self.render_file_manager_preview_status(
                LucideIcon::HelpCircle,
                self.i18n.t(key),
                Some(entry.path),
                has_background,
                cx,
            ),
            Some(LocalPreview::Error(error)) => self.render_file_manager_preview_status(
                LucideIcon::AlertCircle,
                self.i18n.t("fileManager.previewError"),
                Some(error.clone()),
                has_background,
                cx,
            ),
            None => self.render_file_manager_preview_status(
                LucideIcon::HelpCircle,
                self.i18n.t("fileManager.previewError"),
                None,
                has_background,
                cx,
            ),
        }
    }

    fn render_file_manager_preview_image(&self, path: &str, fallback_label: String) -> AnyElement {
        let zoom = self.file_manager.preview_image_zoom.clamp(0.25, 4.0);
        let height = 560.0 * zoom;
        let rotation = self.file_manager.preview_image_rotation.rem_euclid(360);
        let image = if rotation == 0 {
            gpui::img(std::path::PathBuf::from(path))
        } else if let Some(render_image) = rotated_local_preview_image(path, rotation) {
            gpui::img(render_image)
        } else {
            gpui::img(std::path::PathBuf::from(path))
        };
        image
            .w_full()
            .h(px(height))
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                div()
                    .w_full()
                    .h(px(height))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(FILE_MANAGER_TEXT_SM))
                    .child(fallback_label.clone())
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_file_manager_preview_pdf(&self, path: &str, mime_type: &str) -> AnyElement {
        let backend = PdfiumPreviewBackend;
        let zoom = self.file_manager.preview_pdf_zoom.clamp(0.25, 4.0);
        match backend.render_page(&std::path::PathBuf::from(path), 0, (900.0 * zoom) as u32) {
            Ok(bitmap) => {
                if let Some(image) = bitmap.into_render_image() {
                    div()
                        .p(px(16.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            gpui::img(image)
                                .w_full()
                                .h(px(520.0 * zoom))
                                .object_fit(ObjectFit::Contain),
                        )
                        .child(
                            div()
                                .text_size(px(FILE_MANAGER_TEXT_XS))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(format!("PDF · {mime_type} · page 1")),
                        )
                        .into_any_element()
                } else {
                    self.render_file_manager_preview_text_status(
                        "PDFium rendered a page but GPUI could not build a bitmap.",
                    )
                }
            }
            Err(error) => self.render_file_manager_preview_text_status(&format!("{error}")),
        }
    }

    fn render_file_manager_preview_audio(
        &self,
        name: String,
        _path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let snapshot = self.file_manager.preview_audio.snapshot();
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
                    .child(name),
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
                    .child(
                        div()
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg))
                            .text_color(rgb(theme.text))
                            .cursor_pointer()
                            .hover(move |button| button.bg(rgb(theme.bg_hover)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.toggle_file_manager_preview_audio(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(Self::render_lucide_icon(play_icon, 14.0, rgb(theme.text))),
                    )
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
                            .child(format!(
                                "{} / {}",
                                format_file_manager_media_time(position),
                                format_file_manager_media_time(duration)
                            )),
                    )
                    .when(can_seek, |row| {
                        row.child(self.render_file_manager_media_seek_button(
                            "-15s",
                            cx.listener(|this, _event, _window, cx| {
                                let now = this.file_manager.preview_audio.snapshot().position;
                                this.seek_file_manager_preview_audio(
                                    now.saturating_sub(std::time::Duration::from_secs(15)),
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ))
                        .child(
                            self.render_file_manager_media_seek_button(
                                "+15s",
                                cx.listener(|this, _event, _window, cx| {
                                    let snapshot = this.file_manager.preview_audio.snapshot();
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
                            ),
                        )
                    })
                    .when_some(snapshot.error, |row, error| {
                        row.child(
                            div()
                                .text_size(px(FILE_MANAGER_TEXT_XS))
                                .text_color(rgb(FILE_MANAGER_RED))
                                .child(error),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .text_color(rgb(theme.text_muted))
                    .child(mime_type.to_string()),
            )
            .into_any_element()
    }

    fn render_file_manager_preview_video(
        &self,
        name: String,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let snapshot = self.file_manager.preview_video_surface.snapshot();
        let detail = snapshot
            .error
            .unwrap_or_else(|| "Native video playback is initializing.".to_string());
        let fallback = self
            .render_file_manager_native_asset_status_with_external(
                name, path, mime_type, &detail, cx,
            )
            .into_any_element();
        sftp_native_video_element(
            path.to_string(),
            self.file_manager.preview_video_surface.clone(),
            fallback,
        )
        .into_any_element()
    }

    fn render_file_manager_preview_font(
        &self,
        name: String,
        path: &str,
        mime_type: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        if let Some(error) = self.file_manager.preview_font_error.as_ref() {
            return self.render_file_manager_preview_status(
                LucideIcon::FileText,
                self.i18n.t("fileManager.fontLoadError"),
                Some(error.clone()),
                false,
                cx,
            );
        }
        let Some(font_family) = self.file_manager.preview_font_family.clone() else {
            return self.render_file_manager_preview_status(
                LucideIcon::LoaderCircle,
                self.i18n.t("fileManager.loadingFont"),
                Some(path.to_string()),
                false,
                cx,
            );
        };
        let font_size = self.file_manager.preview_font_size;
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
                            this.file_manager.preview_font_size =
                                (this.file_manager.preview_font_size - 4.0).max(8.0);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
                    .child(
                        div()
                            .w(px(52.0))
                            .text_center()
                            .text_size(px(FILE_MANAGER_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{font_size:.0}px")),
                    )
                    .child(self.render_file_manager_font_size_button(
                        "+",
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.file_manager.preview_font_size =
                                (this.file_manager.preview_font_size + 4.0).min(120.0);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
                    .children([16.0, 24.0, 32.0, 48.0, 72.0].into_iter().map(|size| {
                        self.render_file_manager_font_size_button(
                            format!("{size:.0}"),
                            (font_size - size).abs() < f32::EPSILON,
                            cx.listener(move |this, _event, _window, cx| {
                                this.file_manager.preview_font_size = size;
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                    }))
                    .child(
                        div()
                            .ml_auto()
                            .text_size(px(FILE_MANAGER_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("{name} · {mime_type}")),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_y_scrollbar()
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .child(
                        div()
                            .text_size(px(FILE_MANAGER_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(font_family),
                    )
                    .child(
                        div()
                            .font_family(sample_font.clone())
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.3))
                            .text_color(rgb(theme.text))
                            .child("The quick brown fox jumps over the lazy dog."),
                    )
                    .child(
                        div()
                            .font_family(sample_font.clone())
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.3))
                            .text_color(rgb(theme.text))
                            .child("0123456789 !@#$%^&*() []{} <>"),
                    )
                    .child(
                        div()
                            .font_family(sample_font)
                            .text_size(px(font_size))
                            .line_height(px(font_size * 1.3))
                            .text_color(rgb(theme.text))
                            .child("中文字体预览：家里的主机 OxideTerm 终端文件管理器"),
                    ),
            )
            .into_any_element()
    }

    fn render_file_manager_media_seek_button(
        &self,
        label: &'static str,
        on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .px_2()
            .py_1()
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(FILE_MANAGER_TEXT_XS))
            .text_color(rgb(theme.text_muted))
            .cursor_pointer()
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .on_mouse_down(MouseButton::Left, on_click)
            .child(label)
            .into_any_element()
    }

    fn render_file_manager_font_size_button(
        &self,
        label: impl Into<String>,
        active: bool,
        on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(28.0))
            .min_w(px(28.0))
            .px(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(FILE_MANAGER_TEXT_XS))
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgb(theme.bg_panel)
            })
            .hover(move |button| button.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
            .cursor_pointer()
            .child(label.into())
            .on_mouse_down(MouseButton::Left, on_click)
            .into_any_element()
    }

    fn render_file_manager_native_asset_status_with_external(
        &self,
        title: String,
        path: &str,
        mime_type: &str,
        detail: &str,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        let path_for_open = path.to_string();
        div()
            .w_full()
            .min_h(px(520.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .text_size(px(FILE_MANAGER_TEXT_XS))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .text_size(px(FILE_MANAGER_TEXT_SM))
                    .text_color(rgb(theme.text))
                    .child(title),
            )
            .child(mime_type.to_string())
            .child(div().max_w(px(680.0)).child(detail.to_string()))
            .child(
                div()
                    .max_w(px(680.0))
                    .truncate()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .child(path.to_string()),
            )
            .child(
                div()
                    .mt_2()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg))
                    .px_3()
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .text_color(rgb(theme.text))
                    .cursor_pointer()
                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            if let Err(error) = open_path_external(&path_for_open) {
                                this.push_file_manager_toast(
                                    this.i18n.t("fileManager.error"),
                                    Some(error),
                                    TerminalNoticeVariant::Error,
                                );
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(Self::render_lucide_icon(
                        LucideIcon::ExternalLink,
                        FILE_MANAGER_ICON_MD,
                        rgb(theme.text),
                    ))
                    .child(self.i18n.t("fileManager.open")),
            )
    }

    fn render_file_manager_preview_markdown(
        &self,
        content: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let opts = MarkdownOptions::from_theme(&self.tokens);
        div()
            .size_full()
            .p(px(16.0))
            .child(markdown_virtual_with_options(
                cx.entity(),
                "file-manager-preview-markdown-virtual",
                &self.tokens,
                content,
                &opts,
                &self.file_manager.preview_markdown_scroll,
            ))
            .into_any_element()
    }

    fn render_file_manager_preview_code(
        &self,
        content: &str,
        language: Option<&str>,
        filename: &str,
        has_background: bool,
    ) -> AnyElement {
        if content.is_empty() {
            return self
                .render_file_manager_preview_text_status(&self.i18n.t("fileManager.emptyFile"));
        }
        let theme = self.tokens.ui;
        let opts = MarkdownOptions::from_theme(&self.tokens);
        let language = language
            .filter(|language| !language.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| file_manager_preview_language_for_name(filename))
            .to_ascii_lowercase();
        let lines = Arc::new(file_manager_preview_visual_lines(content));
        let row_count = lines.len();
        let list_lines = lines.clone();
        let font_family = settings_mono_font_family(self.settings_store.settings());
        let font_size = self.settings_store.settings().terminal.font_size as f32;
        let row_height = font_size * 1.5;
        let scroll = self.file_manager.preview_code_scroll.clone();
        div()
            .size_full()
            .bg(file_manager_bg(theme.bg_sunken, has_background))
            .child(
                div().size_full().p(px(16.0)).child(
                    uniform_list(
                        "file-manager-preview-code-virtual",
                        row_count,
                        move |range, _window, _cx| {
                            let opts = opts.clone();
                            let language = language.clone();
                            let font_family = font_family.clone();
                            range
                                .map(|index| {
                                    let line = &list_lines[index];
                                    let content: AnyElement = if language != "text"
                                        && language != "plain"
                                        && let Some(runs) = highlight::highlight_code(
                                            &language,
                                            &line.content,
                                            &opts,
                                        ) {
                                        let (text, text_runs) =
                                            highlight::highlighted_runs_to_text_runs(&runs);
                                        StyledText::new(text)
                                            .with_runs(text_runs)
                                            .into_any_element()
                                    } else {
                                        SharedString::from(line.content.clone()).into_any_element()
                                    };
                                    div()
                                        .h(px(row_height))
                                        .w_full()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .font_family(font_family.clone())
                                        .text_size(px(font_size))
                                        .line_height(px(row_height))
                                        .text_color(rgb(theme.text))
                                        .child(
                                            div()
                                                .w(px(48.0))
                                                .flex_none()
                                                .pr(px(12.0))
                                                .text_align(gpui::TextAlign::Right)
                                                .text_color(rgba(
                                                    (theme.text_muted << 8)
                                                        | FILE_MANAGER_PREVIEW_CODE_GUTTER_ALPHA,
                                                ))
                                                .child(
                                                    line.line_number
                                                        .map(|line_number| line_number.to_string())
                                                        .unwrap_or_default(),
                                                ),
                                        )
                                        .child(div().flex_1().min_w(px(0.0)).child(content))
                                        .into_any_element()
                                })
                                .collect::<Vec<_>>()
                        },
                    )
                    .track_scroll(scroll)
                    .size_full()
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
                ),
            )
            .into_any_element()
    }

    fn render_file_manager_archive_tree(
        &self,
        info: &LocalArchiveInfo,
        has_background: bool,
    ) -> AnyElement {
        let saved = if info.total_size > 0 {
            ((1.0 - (info.compressed_size as f64 / info.total_size as f64)) * 100.0).round() as i64
        } else {
            0
        };
        let mut body = div()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .p(px(12.0))
                    .rounded(px(self.tokens.radii.md))
                    .bg(file_manager_panel_bg(
                        self.tokens.ui.bg_panel,
                        has_background,
                        FILE_MANAGER_PANEL_80_ALPHA,
                    ))
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!(
                        "{} {}",
                        info.total_dirs,
                        self.i18n.t("fileManager.folders")
                    ))
                    .child(format!(
                        "{} {}",
                        info.total_files,
                        self.i18n.t("fileManager.files")
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("fileManager.originalSize"),
                        format_file_size(info.total_size)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("fileManager.compressedSize"),
                        format_file_size(info.compressed_size)
                    ))
                    .child(
                        div()
                            .text_color(rgb(FILE_MANAGER_GREEN))
                            .child(format!("{saved}% {}", self.i18n.t("fileManager.saved"))),
                    ),
            )
            .child(self.render_file_manager_archive_header(has_background));
        for (index, entry) in info.entries.iter().enumerate() {
            body = body.child(self.render_file_manager_archive_row(entry, index, has_background));
        }
        body.into_any_element()
    }

    fn render_file_manager_archive_header(&self, has_background: bool) -> AnyElement {
        div()
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .gap(px(8.0))
            .items_center()
            .border_b_1()
            .border_color(file_manager_border(self.tokens.ui.border, has_background))
            .bg(file_manager_panel_bg(
                self.tokens.ui.bg_panel,
                has_background,
                FILE_MANAGER_PANEL_80_ALPHA,
            ))
            .text_size(px(FILE_MANAGER_TEXT_XS))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.i18n.t("fileManager.name")),
            )
            .child(
                div()
                    .w(px(80.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(self.i18n.t("fileManager.size")),
            )
            .child(
                div()
                    .w(px(80.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(self.i18n.t("fileManager.compressed")),
            )
            .child(
                div()
                    .w(px(120.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(self.i18n.t("fileManager.modified")),
            )
            .into_any_element()
    }

    fn render_file_manager_archive_row(
        &self,
        entry: &LocalArchiveEntry,
        index: usize,
        has_background: bool,
    ) -> AnyElement {
        let depth = entry
            .path
            .matches('/')
            .count()
            .saturating_sub(usize::from(entry.is_dir));
        div()
            .min_h(px(28.0))
            .px(px(12.0))
            .flex()
            .gap(px(8.0))
            .items_center()
            .bg(if index % 2 == 0 {
                file_manager_panel_bg(self.tokens.ui.bg_panel, has_background, 0x33)
            } else {
                rgba(0)
            })
            .text_size(px(FILE_MANAGER_TEXT_XS))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .pl(px((depth * 16) as f32))
                    .child(Self::render_lucide_icon(
                        if entry.is_dir {
                            LucideIcon::Folder
                        } else {
                            LucideIcon::File
                        },
                        FILE_MANAGER_ICON_SM,
                        rgb(if entry.is_dir {
                            FILE_MANAGER_ORANGE
                        } else {
                            self.tokens.ui.text_muted
                        }),
                    ))
                    .child(div().truncate().child(entry.name.clone())),
            )
            .child(
                div()
                    .w(px(80.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(if entry.is_dir {
                        "-".to_string()
                    } else {
                        format_file_size(entry.size)
                    }),
            )
            .child(
                div()
                    .w(px(80.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(if entry.is_dir {
                        "-".to_string()
                    } else {
                        format_file_size(entry.compressed_size)
                    }),
            )
            .child(
                div()
                    .w(px(120.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(entry.modified.clone().unwrap_or_else(|| "-".to_string())),
            )
            .into_any_element()
    }

    fn render_file_manager_preview_metadata(&self, has_background: bool) -> AnyElement {
        let Some(metadata) = self.file_manager.preview_metadata.as_ref() else {
            return div().into_any_element();
        };
        let mut grid = div()
            .grid()
            .grid_cols(4)
            .gap_x(px(24.0))
            .gap_y(px(8.0))
            .text_size(px(FILE_MANAGER_TEXT_XS));
        grid = grid.child(self.render_file_manager_metadata_item(
            LucideIcon::HardDrive,
            self.i18n.t("fileManager.size"),
            format_file_size(metadata.size),
            false,
        ));
        grid = grid.child(self.render_file_manager_metadata_item(
            LucideIcon::Clock,
            self.i18n.t("fileManager.modified"),
            self.format_file_manager_quicklook_timestamp(metadata.modified),
            false,
        ));
        if let Some(created) = metadata.created {
            grid = grid.child(self.render_file_manager_metadata_item(
                LucideIcon::Clock,
                self.i18n.t("fileManager.created"),
                self.format_file_manager_quicklook_timestamp(Some(created)),
                false,
            ));
        }
        let permissions = metadata
            .mode
            .map(format_unix_permission_bits)
            .unwrap_or_else(|| {
                if metadata.readonly {
                    self.i18n.t("fileManager.readonly")
                } else {
                    self.i18n.t("fileManager.readwrite")
                }
            });
        grid = grid.child(self.render_file_manager_metadata_item(
            LucideIcon::Shield,
            self.i18n.t("fileManager.permissions"),
            permissions,
            metadata.mode.is_some(),
        ));
        if let Some(mime_type) = metadata.mime_type.as_ref() {
            grid = grid.child(self.render_file_manager_metadata_item(
                LucideIcon::FileText,
                self.i18n.t("fileManager.type"),
                mime_type.clone(),
                false,
            ));
        }
        if metadata.is_symlink {
            grid = grid.child(self.render_file_manager_metadata_item(
                LucideIcon::Link2,
                self.i18n.t("fileManager.symlink"),
                self.i18n.t("fileManager.symlink"),
                false,
            ));
        }
        div()
            .px(px(16.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(file_manager_border(self.tokens.ui.border, has_background))
            .bg(file_manager_panel_bg(
                self.tokens.ui.bg_panel,
                has_background,
                FILE_MANAGER_PANEL_80_ALPHA,
            ))
            .child(grid)
            .into_any_element()
    }

    fn render_file_manager_metadata_item(
        &self,
        icon: LucideIcon,
        label: String,
        value: String,
        mono_value: bool,
    ) -> AnyElement {
        let mut value_el = div()
            .min_w(px(0.0))
            .truncate()
            .text_color(rgb(self.tokens.ui.text))
            .child(value);
        if mono_value {
            value_el =
                value_el.font_family(settings_mono_font_family(self.settings_store.settings()));
        }
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .min_w(px(0.0))
            .child(Self::render_lucide_icon(
                icon,
                FILE_MANAGER_ICON_MD,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(
                div()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("{label}:")),
            )
            .child(value_el)
            .into_any_element()
    }

    fn format_file_manager_quicklook_timestamp(&self, timestamp: Option<i64>) -> String {
        let Some(timestamp) = timestamp.filter(|timestamp| *timestamp > 0) else {
            return "-".to_string();
        };
        let Some(datetime) = chrono::DateTime::from_timestamp(timestamp, 0) else {
            return "-".to_string();
        };
        let datetime = datetime.with_timezone(&chrono::Local);
        match self.i18n.locale() {
            Locale::ZhCn | Locale::ZhTw => datetime.format("%Y年%-m月%-d日").to_string(),
            _ => datetime.format("%b %-d, %Y").to_string(),
        }
    }

    fn render_file_manager_preview_button(
        &self,
        icon: LucideIcon,
        active: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .bg(if active {
                file_manager_hover_bg(theme.bg_hover, true)
            } else {
                rgba(0)
            })
            .hover(move |button| button.bg(file_manager_hover_bg(theme.bg_hover, true)))
            .child(Self::render_lucide_icon(
                icon,
                FILE_MANAGER_ICON_MD,
                rgb(theme.text),
            ))
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_file_manager_preview_status(
        &self,
        icon: LucideIcon,
        title: String,
        description: Option<String>,
        _has_background: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h(px(520.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(Self::render_lucide_icon(
                icon,
                40.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .child(div().text_size(px(FILE_MANAGER_TEXT_SM)).child(title))
            .when_some(description, |el, description| {
                el.child(
                    div()
                        .max_w(px(520.0))
                        .text_center()
                        .text_size(px(FILE_MANAGER_TEXT_XS))
                        .child(description),
                )
            })
            .into_any_element()
    }

    fn render_file_manager_preview_text_status(&self, text: &str) -> AnyElement {
        div()
            .h(px(520.0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(FILE_MANAGER_TEXT_SM))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(text.to_string())
            .into_any_element()
    }
}

fn preview_icon(preview: &LocalPreview) -> LucideIcon {
    match preview {
        LocalPreview::Markdown { .. }
        | LocalPreview::Text {
            language: Some(_), ..
        } => LucideIcon::FileCode,
        LocalPreview::Text { .. } => LucideIcon::FileText,
        LocalPreview::Image { .. } => LucideIcon::FileImage,
        LocalPreview::Pdf { .. } => LucideIcon::FileText,
        LocalPreview::Video { .. } => LucideIcon::FileVideo,
        LocalPreview::Audio { .. } => LucideIcon::FileAudio,
        LocalPreview::Font { .. } => LucideIcon::FileText,
        LocalPreview::Archive { .. } => LucideIcon::FileArchive,
        LocalPreview::TooLarge { .. } | LocalPreview::Unsupported(_) => LucideIcon::HelpCircle,
        LocalPreview::Loading => LucideIcon::LoaderCircle,
        LocalPreview::Error(_) => LucideIcon::AlertCircle,
    }
}

#[derive(Clone, Debug)]
struct FileManagerPreviewVisualLine {
    line_number: Option<usize>,
    content: String,
}

fn file_manager_preview_visual_lines(source: &str) -> Vec<FileManagerPreviewVisualLine> {
    source
        .split('\n')
        .enumerate()
        .flat_map(|(index, line)| {
            wrap_file_manager_virtual_text_line(line, FILE_MANAGER_PREVIEW_CODE_WRAP_COLUMNS)
                .into_iter()
                .enumerate()
                .map(move |(chunk_index, content)| FileManagerPreviewVisualLine {
                    line_number: (chunk_index == 0).then_some(index + 1),
                    content,
                })
        })
        .collect()
}

fn wrap_file_manager_virtual_text_line(line: &str, max_columns: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    // Tauri renders CodeHighlight with CSS `whitespace-pre` and browser
    // scrolling. GPUI preview keeps a fixed row-height virtual list, so long
    // physical lines become stable visual rows instead of oversized elements.
    let max_columns = max_columns.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut width = 0usize;
    for ch in line.chars() {
        if width >= max_columns {
            chunks.push(std::mem::take(&mut current));
            width = 0;
        }
        current.push(ch);
        width += 1;
    }
    chunks.push(current);
    chunks
}

fn format_file_manager_media_time(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn format_unix_permission_bits(mode: u32) -> String {
    let mut output = String::with_capacity(9);
    for bit in [
        0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
    ] {
        output.push(match bit {
            0o400 | 0o040 | 0o004 => {
                if mode & bit != 0 {
                    'r'
                } else {
                    '-'
                }
            }
            0o200 | 0o020 | 0o002 => {
                if mode & bit != 0 {
                    'w'
                } else {
                    '-'
                }
            }
            _ => {
                if mode & bit != 0 {
                    'x'
                } else {
                    '-'
                }
            }
        });
    }
    output
}

fn rotated_local_preview_image(path: &str, rotation: i32) -> Option<std::sync::Arc<RenderImage>> {
    let image = image::open(std::path::PathBuf::from(path)).ok()?;
    let image = match rotation.rem_euclid(360) {
        90 => image.rotate90(),
        180 => image.rotate180(),
        270 => image.rotate270(),
        _ => image,
    };
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut pixels = rgba.into_raw();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(width, height, pixels)?;
    Some(std::sync::Arc::new(RenderImage::new(vec![
        image::Frame::new(buffer),
    ])))
}

fn file_manager_preview_language_for_name(filename: &str) -> String {
    let lower = filename.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        ".bashrc" | ".bash_profile" | ".zshrc" | ".zprofile" | ".profile" | ".env" | ".gitignore"
    ) || lower.ends_with("rc")
    {
        return "bash".to_string();
    }
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "js" => "javascript",
        "jsx" => "jsx",
        "ts" => "typescript",
        "tsx" => "tsx",
        "py" => "python",
        "rs" => "rust",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" | "cxx" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" | "zsh" => "bash",
        "fish" => "fish",
        "ps1" | "psm1" => "powershell",
        "bat" | "cmd" => "batch",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "less" => "less",
        "json" | "json5" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "md" | "mdx" => "markdown",
        "ini" | "editorconfig" | "terminal" => "ini",
        "diff" | "patch" => "diff",
        "log" => "log",
        _ => "plain",
    }
    .to_string()
}
