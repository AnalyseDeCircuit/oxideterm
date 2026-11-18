use super::*;

impl FileManagerView {
    pub(super) fn render_file_manager_properties_dialog(
        &self,
        entry: LocalFileEntry,
        details: FileManagerProperties,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_dir = entry.file_type == LocalFileType::Directory;
        let file_type = if is_dir || entry.file_type == LocalFileType::Symlink {
            self.i18n.t(match entry.file_type {
                LocalFileType::Directory => "fileManager.propTypeFolder",
                LocalFileType::Symlink => "fileManager.propTypeSymlink",
                LocalFileType::File => "fileManager.propTypeFile",
            })
        } else {
            details.mime_type.clone().unwrap_or_else(|| {
                self.i18n.t(match entry.file_type {
                    LocalFileType::Directory => "fileManager.propTypeFolder",
                    LocalFileType::Symlink => "fileManager.propTypeSymlink",
                    LocalFileType::File => "fileManager.propTypeFile",
                })
            })
        };
        let mut body = div()
            .px(px(16.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(2.0));

        body = body
            .child(self.render_file_manager_property_row_text(
                self.i18n.t("fileManager.propKind"),
                file_type,
                false,
                cx,
            ))
            .child(self.render_file_manager_property_row_value(
                self.i18n.t("fileManager.size"),
                self.render_file_manager_property_size(details.size, cx),
                cx,
            ))
            .child(self.render_file_manager_property_row_text(
                self.i18n.t("fileManager.propLocation"),
                details.location.clone(),
                false,
                cx,
            ))
            .child(self.render_file_manager_property_separator(has_background));

        if let Some(created) = details.created {
            body = body.child(self.render_file_manager_property_row_text(
                self.i18n.t("fileManager.created"),
                format_full_timestamp(Some(created)),
                false,
                cx,
            ));
        }
        body = body.child(self.render_file_manager_property_row_text(
            self.i18n.t("fileManager.modified"),
            format_full_timestamp(details.modified),
            false,
            cx,
        ));
        if let Some(accessed) = details.accessed {
            body = body.child(self.render_file_manager_property_row_text(
                self.i18n.t("fileManager.propAccessed"),
                format_full_timestamp(Some(accessed)),
                false,
                cx,
            ));
        }

        body = body
            .child(self.render_file_manager_property_separator(has_background))
            .child(if let Some(mode) = details.mode {
                self.render_file_manager_property_row_value(
                    self.i18n.t("fileManager.permissions"),
                    self.render_file_manager_property_permissions(mode, cx),
                    cx,
                )
            } else {
                self.render_file_manager_property_row_text(
                    self.i18n.t("fileManager.propAccess"),
                    if details.readonly {
                        self.i18n.t("fileManager.readonly")
                    } else {
                        self.i18n.t("fileManager.readwrite")
                    },
                    false,
                    cx,
                )
            });

        if details.is_symlink {
            body = body.child(self.render_file_manager_property_row_text(
                self.i18n.t("fileManager.symlink"),
                self.i18n.t("fileManager.propYes"),
                false,
                cx,
            ));
        }
        if !is_dir && let Some(mime_type) = details.mime_type.clone() {
            body = body.child(self.render_file_manager_property_row_text(
                self.i18n.t("fileManager.mimeType"),
                mime_type,
                true,
                cx,
            ));
        }

        if is_dir {
            body = body.child(self.render_file_manager_property_separator(has_background));
            if let (Some(files), Some(dirs)) = (details.dir_files, details.dir_dirs) {
                body = body.child(
                    self.render_file_manager_property_row_text(
                        self.i18n.t("fileManager.propContents"),
                        self.i18n
                            .t("fileManager.propDirSummary")
                            .replace("{{files}}", &files.to_string())
                            .replace("{{dirs}}", &dirs.to_string()),
                        false,
                        cx,
                    ),
                );
            }
            if let Some(total_size) = details.total_size {
                body = body.child(self.render_file_manager_property_row_value(
                    self.i18n.t("fileManager.propTotalSize"),
                    self.render_file_manager_property_size(total_size, cx),
                    cx,
                ));
            }
        } else {
            body = body.child(self.render_file_manager_property_separator(has_background));
            if let Some(checksum) = self.file_manager.read(cx).properties_checksum.clone() {
                body = body
                    .child(self.render_file_manager_property_row_text(
                        "MD5",
                        checksum.md5,
                        true,
                        cx,
                    ))
                    .child(self.render_file_manager_property_row_text(
                        "SHA-256",
                        checksum.sha256,
                        true,
                        cx,
                    ));
            } else {
                body = body.child(self.render_file_manager_checksum_row(cx));
            }
        }

        body.into_any_element()
    }

    pub(super) fn render_file_manager_property_row_text(
        &self,
        label: impl Into<String>,
        value: impl Into<String>,
        mono: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = label.into();
        let value = value.into();
        let mut value_el = div()
            .flex_1()
            .min_w(px(0.0))
            .text_color(rgb(self.tokens.ui.text))
            .child(self.render_selectable_display_text(
                "file-manager-property-value",
                (&label, mono),
                value,
                self.tokens.ui.text,
                cx,
            ));
        if mono {
            value_el = value_el.font_family(self.mono_font_family.clone());
        }
        self.render_file_manager_property_row_value(label, value_el.into_any_element(), cx)
    }

    pub(super) fn render_file_manager_property_row_value(
        &self,
        label: impl Into<String>,
        value: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = label.into();
        div()
            .flex()
            .items_start()
            .gap(px(12.0))
            .py(px(6.0))
            .text_size(px(FILE_MANAGER_TEXT_XS))
            .child(
                div()
                    .min_w(px(104.0))
                    .max_w(px(128.0))
                    .flex_none()
                    .text_align(gpui::TextAlign::Right)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_display_text(
                        "file-manager-property-label",
                        &label,
                        label.clone(),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .child(value)
            .into_any_element()
    }

    pub(super) fn render_file_manager_property_separator(
        &self,
        has_background: bool,
    ) -> AnyElement {
        div()
            .h(px(1.0))
            .my(px(6.0))
            .bg(file_manager_border(self.tokens.ui.border, has_background))
            .into_any_element()
    }

    pub(super) fn render_file_manager_property_size(
        &self,
        size: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut value = div()
            .flex()
            .items_baseline()
            .gap(px(4.0))
            .flex_wrap()
            .text_color(rgb(self.tokens.ui.text))
            .child(self.render_selectable_text_scoped(
                "file-manager-property-size",
                size,
                format_file_size(size),
                self.tokens.ui.text,
                cx,
            ));
        if size >= 1024 {
            let bytes = format!(
                "({} {})",
                format_number_with_separators(size),
                self.i18n.t("fileManager.propBytes")
            );
            value = value.child(div().text_color(rgb(self.tokens.ui.text_muted)).child(
                self.render_selectable_text_scoped(
                    "file-manager-property-size-bytes",
                    size,
                    bytes,
                    self.tokens.ui.text_muted,
                    cx,
                ),
            ));
        }
        value.into_any_element()
    }

    pub(super) fn render_file_manager_property_permissions(
        &self,
        mode: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let perms = format_permission_bits(mode);
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(1.0))
            .font_family(self.mono_font_family.clone())
            .text_color(rgb(self.tokens.ui.text));
        for (index, ch) in perms.chars().enumerate() {
            let color = match ch {
                'r' => 0x34d399,
                'w' => 0xfbbf24,
                'x' => 0x38bdf8,
                _ => self.tokens.ui.text_muted,
            };
            row = row.child(div().text_color(rgb(color)).child(
                self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "file-manager-permission-char",
                    index,
                    ch.to_string(),
                    color,
                    cx,
                ),
            ));
        }
        row.child(
            div()
                .ml(px(6.0))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "file-manager-permission-mode",
                    mode,
                    format!("({:04o})", mode & 0o777),
                    self.tokens.ui.text_muted,
                    cx,
                )),
        )
        .into_any_element()
    }

    pub(super) fn render_file_manager_checksum_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let loading = self.file_manager.read(cx).properties_checksum_loading;
        let theme = self.tokens.ui;
        self.render_file_manager_property_row_value(
            self.i18n.t("fileManager.propChecksum"),
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_color(rgb(FILE_MANAGER_BLUE))
                .cursor_pointer()
                .opacity(if loading { 0.5 } else { 1.0 })
                .child(if loading {
                    self.render_loading_icon(
                        "file-manager-checksum-loading",
                        FILE_MANAGER_ICON_SM,
                        rgb(FILE_MANAGER_BLUE),
                    )
                } else {
                    Self::render_lucide_icon(
                        LucideIcon::Hash,
                        FILE_MANAGER_ICON_SM,
                        rgb(FILE_MANAGER_BLUE),
                    )
                })
                .child(if loading {
                    self.i18n.t("fileManager.propCalculating")
                } else {
                    self.i18n.t("fileManager.propCalcChecksum")
                })
                .hover(move |row| row.text_color(rgb(theme.accent_hover)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.calculate_file_manager_properties_checksum(cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
            cx,
        )
    }
}

fn format_number_with_separators(value: u64) -> String {
    let raw = value.to_string();
    let mut out = String::with_capacity(raw.len() + raw.len() / 3);
    for (index, ch) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn format_full_timestamp(timestamp: Option<i64>) -> String {
    let Some(timestamp) = timestamp.filter(|timestamp| *timestamp > 0) else {
        return "-".to_string();
    };
    let Some(datetime) = chrono::DateTime::from_timestamp(timestamp, 0) else {
        return "-".to_string();
    };
    datetime
        .with_timezone(&chrono::Local)
        .format("%Y/%-m/%-d %H:%M:%S")
        .to_string()
}

fn format_permission_bits(mode: u32) -> String {
    let bits = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    bits.iter()
        .map(|(bit, ch)| if mode & bit != 0 { *ch } else { '-' })
        .collect()
}
