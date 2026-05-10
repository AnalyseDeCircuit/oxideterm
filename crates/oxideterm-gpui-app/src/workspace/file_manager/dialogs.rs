use super::*;

mod preview;

impl WorkspaceApp {
    pub(super) fn render_file_manager_context_menu(
        &self,
        menu: FileManagerContextMenu,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let viewport = window.viewport_size();
        let max_height = (f32::from(viewport.height) * 0.8)
            .min(FILE_MANAGER_CONTEXT_MENU_MAX_HEIGHT)
            .max(180.0);
        let x = menu
            .x
            .min(f32::from(viewport.width) - FILE_MANAGER_CONTEXT_MENU_WIDTH - 8.0)
            .max(8.0);
        let y = menu
            .y
            .min(f32::from(viewport.height) - max_height - 8.0)
            .max(8.0);
        let selected_count = self.file_manager.selected.len();

        let popup = div()
            .w(px(FILE_MANAGER_CONTEXT_MENU_WIDTH))
            .max_h(px(max_height))
            .overflow_hidden()
            .p(px(FILE_MANAGER_CONTEXT_MENU_PADDING))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(file_manager_border(theme.border, has_background))
            .bg(file_manager_panel_bg(
                theme.bg_elevated,
                has_background,
                0xf2,
            ))
            .shadow_lg()
            .when_some(menu.file.clone(), |menu_el, file| {
                if file.file_type == LocalFileType::Directory {
                    menu_el.child(self.render_file_manager_context_menu_item(
                        LucideIcon::FolderOpen,
                        self.i18n.t("fileManager.open"),
                        false,
                        has_background,
                        cx.listener({
                            let file = file.clone();
                            move |this, _event, _window, cx| {
                                this.set_file_manager_path(file.path.clone());
                                this.file_manager.context_menu = None;
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    ))
                } else {
                    menu_el
                        .child(self.render_file_manager_context_menu_item(
                            LucideIcon::ExternalLink,
                            self.i18n.t("fileManager.openExternal"),
                            false,
                            has_background,
                            cx.listener({
                                let file = file.clone();
                                move |this, _event, _window, cx| {
                                    if let Err(error) = open_path_external(&file.path) {
                                        this.push_file_manager_toast(
                                            this.i18n.t("fileManager.error"),
                                            Some(error),
                                            TerminalNoticeVariant::Error,
                                        );
                                    }
                                    this.file_manager.context_menu = None;
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }),
                        ))
                        .child(self.render_file_manager_context_menu_item(
                            LucideIcon::Eye,
                            self.i18n.t("fileManager.preview"),
                            false,
                            has_background,
                            cx.listener({
                                let file = file.clone();
                                move |this, _event, _window, cx| {
                                    this.open_file_manager_preview(file.clone(), cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            }),
                        ))
                }
            })
            .when(menu.file.is_some(), |menu_el| {
                menu_el.child(self.render_file_manager_context_menu_item(
                    LucideIcon::FolderOpen,
                    self.i18n.t("fileManager.revealInFileManager"),
                    false,
                    has_background,
                    cx.listener({
                        let file = menu.file.clone();
                        move |this, _event, _window, cx| {
                            if let Some(file) = file.as_ref()
                                && let Err(error) = reveal_path_external(&file.path)
                            {
                                this.push_file_manager_toast(
                                    this.i18n.t("fileManager.error"),
                                    Some(error),
                                    TerminalNoticeVariant::Error,
                                );
                            }
                            this.file_manager.context_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }),
                ))
            })
            .when(selected_count > 0, |menu_el| {
                menu_el
                    .child(self.render_file_manager_separator(has_background))
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::Copy,
                        self.i18n.t("fileManager.copy"),
                        false,
                        has_background,
                        cx.listener(|this, _event, _window, cx| {
                            this.copy_file_manager_selection(false, cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::Pencil,
                        self.i18n.t("fileManager.cut"),
                        false,
                        has_background,
                        cx.listener(|this, _event, _window, cx| {
                            this.copy_file_manager_selection(true, cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::Copy,
                        self.i18n.t("fileManager.duplicate"),
                        false,
                        has_background,
                        cx.listener(|this, _event, _window, cx| {
                            this.duplicate_file_manager_selection(cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::FileArchive,
                        self.i18n.t("fileManager.compress"),
                        false,
                        has_background,
                        cx.listener(|this, _event, _window, cx| {
                            this.compress_file_manager_selection(cx);
                            cx.stop_propagation();
                        }),
                    ))
            })
            .when(
                selected_count == 1
                    && menu
                        .file
                        .as_ref()
                        .is_some_and(|file| can_extract_archive(&file.name)),
                |menu_el| {
                    menu_el.child(self.render_file_manager_context_menu_item(
                        LucideIcon::FileArchive,
                        self.i18n.t("fileManager.extract"),
                        false,
                        has_background,
                        cx.listener(|this, _event, _window, cx| {
                            this.extract_selected_file_manager_archive(cx);
                            cx.stop_propagation();
                        }),
                    ))
                },
            )
            .when(self.file_manager.clipboard.is_some(), |menu_el| {
                menu_el.child(self.render_file_manager_context_menu_item(
                    LucideIcon::Download,
                    self.i18n.t("fileManager.paste"),
                    false,
                    has_background,
                    cx.listener(|this, _event, _window, cx| {
                        this.paste_file_manager_clipboard(cx);
                        cx.stop_propagation();
                    }),
                ))
            })
            .when(selected_count == 1, |menu_el| {
                menu_el
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::Pencil,
                        self.i18n.t("fileManager.rename"),
                        false,
                        has_background,
                        cx.listener({
                            let file = menu.file.clone();
                            move |this, _event, _window, cx| {
                                if let Some(file) = file.as_ref() {
                                    this.open_file_manager_rename_dialog(file.name.clone());
                                }
                                this.file_manager.context_menu = None;
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    ))
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::Copy,
                        self.i18n.t("fileManager.copyPath"),
                        false,
                        has_background,
                        cx.listener(|this, _event, _window, cx| {
                            this.copy_file_manager_path_to_clipboard(false, cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::FileText,
                        self.i18n.t("fileManager.copyName"),
                        false,
                        has_background,
                        cx.listener(|this, _event, _window, cx| {
                            this.copy_file_manager_path_to_clipboard(true, cx);
                            cx.stop_propagation();
                        }),
                    ))
            })
            .when(selected_count > 0, |menu_el| {
                menu_el
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::Info,
                        self.i18n.t("fileManager.properties"),
                        false,
                        has_background,
                        cx.listener({
                            let file = menu.file.clone();
                            move |this, _event, _window, cx| {
                                if let Some(file) = file
                                    .clone()
                                    .or_else(|| this.single_selected_file_manager_file())
                                {
                                    this.open_file_manager_properties(file);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    ))
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::Trash2,
                        self.i18n.t("fileManager.delete"),
                        true,
                        has_background,
                        cx.listener(|this, _event, _window, cx| {
                            this.open_file_manager_delete_dialog();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
            })
            .child(self.render_file_manager_separator(has_background))
            .child(self.render_file_manager_context_menu_item(
                LucideIcon::FolderPlus,
                self.i18n.t("fileManager.newFolder"),
                false,
                has_background,
                cx.listener(|this, _event, _window, cx| {
                    this.open_file_manager_new_folder_dialog();
                    this.file_manager.context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(self.render_file_manager_context_menu_item(
                LucideIcon::FilePlus,
                self.i18n.t("fileManager.newFile"),
                false,
                has_background,
                cx.listener(|this, _event, _window, cx| {
                    this.open_file_manager_new_file_dialog();
                    this.file_manager.context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(self.render_file_manager_context_menu_item(
                LucideIcon::Check,
                self.i18n.t("fileManager.selectAll"),
                false,
                has_background,
                cx.listener(|this, _event, _window, cx| {
                    this.select_all_file_manager_files();
                    this.file_manager.context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .child(self.render_file_manager_context_menu_item(
                LucideIcon::RefreshCw,
                self.i18n.t("fileManager.refresh"),
                false,
                has_background,
                cx.listener(|this, _event, _window, cx| {
                    this.refresh_file_manager();
                    this.file_manager.context_menu = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event, _window, cx| {
                    cx.stop_propagation();
                }),
            )
            .into_any_element();

        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(x), px(y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(popup),
        )
        .with_priority(100)
        .into_any_element()
    }

    fn render_file_manager_context_menu_item(
        &self,
        icon: LucideIcon,
        label: String,
        danger: bool,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let color = if danger { FILE_MANAGER_RED } else { theme.text };
        div()
            .h(px(FILE_MANAGER_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(self.tokens.radii.xs))
            .text_size(px(FILE_MANAGER_TEXT_XS))
            .text_color(rgb(color))
            .cursor_pointer()
            .hover(move |item| item.bg(file_manager_hover_bg(theme.bg_hover, has_background)))
            .child(Self::render_lucide_icon(
                icon,
                FILE_MANAGER_ICON_SM,
                rgb(color),
            ))
            .child(div().truncate().child(label))
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_file_manager_separator(&self, has_background: bool) -> AnyElement {
        div()
            .h(px(1.0))
            .my(px(FILE_MANAGER_CONTEXT_MENU_PADDING))
            .bg(file_manager_border(self.tokens.ui.border, has_background))
            .into_any_element()
    }

    pub(super) fn render_file_manager_dialog(
        &self,
        window: &mut Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(dialog) = self.file_manager.dialog.clone() else {
            return div().into_any_element();
        };
        let title = match &dialog {
            FileManagerDialog::Drives => self.i18n.t("fileManager.selectDrive"),
            FileManagerDialog::NewFolder => self.i18n.t("fileManager.newFolder"),
            FileManagerDialog::NewFile => self.i18n.t("fileManager.newFile"),
            FileManagerDialog::Rename { .. } => self.i18n.t("fileManager.rename"),
            FileManagerDialog::Delete { .. } => self.i18n.t("fileManager.confirmDelete"),
            FileManagerDialog::EditBookmark { .. } => self.i18n.t("fileManager.editBookmark"),
            FileManagerDialog::Properties { .. } => self.i18n.t("fileManager.propTitleGetInfo"),
            FileManagerDialog::Preview { entry } => entry.name.clone(),
        };
        let body = match dialog {
            FileManagerDialog::Drives => self.render_file_manager_drives_dialog(has_background, cx),
            FileManagerDialog::NewFolder
            | FileManagerDialog::NewFile
            | FileManagerDialog::Rename { .. } => {
                self.render_file_manager_name_dialog(has_background, cx)
            }
            FileManagerDialog::EditBookmark { path, .. } => {
                self.render_file_manager_bookmark_dialog(path, has_background, cx)
            }
            FileManagerDialog::Delete { files } => {
                self.render_file_manager_delete_dialog(files, has_background, cx)
            }
            FileManagerDialog::Properties { entry, details } => {
                self.render_file_manager_properties_dialog(entry, details, has_background)
            }
            FileManagerDialog::Preview { entry } => {
                self.render_file_manager_preview_dialog(entry, has_background, cx)
            }
        };
        let width = match self.file_manager.dialog {
            Some(FileManagerDialog::Preview { .. }) => 1000.0,
            Some(FileManagerDialog::Properties { .. }) => FILE_MANAGER_DIALOG_WIDTH_LG,
            _ => FILE_MANAGER_DIALOG_WIDTH_SM,
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(
                (self.tokens.ui.bg << 8) | FILE_MANAGER_DIALOG_OVERLAY_ALPHA,
            ))
            .child(
                div()
                    .w(px(width.min(f32::from(window.viewport_size().width) - 32.0)))
                    .max_h(px(
                        (f32::from(window.viewport_size().height) * 0.86).max(240.0)
                    ))
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | FILE_MANAGER_DIALOG_BORDER_ALPHA,
                    ))
                    .bg(file_manager_panel_bg(
                        self.tokens.ui.bg_elevated,
                        has_background,
                        0xf2,
                    ))
                    .shadow_lg()
                    .child(
                        div()
                            .h(px(48.0))
                            .px(px(16.0))
                            .flex()
                            .items_center()
                            .border_b_1()
                            .border_color(file_manager_border(
                                self.tokens.ui.border,
                                has_background,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .text_size(px(FILE_MANAGER_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .size(px(28.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(self.tokens.radii.sm))
                                    .cursor_pointer()
                                    .hover({
                                        let theme = self.tokens.ui;
                                        move |button| button.bg(rgb(theme.bg_hover))
                                    })
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::X,
                                        FILE_MANAGER_ICON_MD,
                                        rgb(self.tokens.ui.text_muted),
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.close_file_manager_dialog();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    fn render_file_manager_name_dialog(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = WorkspaceImeTarget::FileManager(FileManagerInput::DialogValue);
        let workspace = cx.entity();
        let placeholder = match self.file_manager.dialog {
            Some(FileManagerDialog::NewFolder) => self.i18n.t("fileManager.folderName"),
            Some(FileManagerDialog::NewFile) => self.i18n.t("fileManager.fileName"),
            Some(FileManagerDialog::EditBookmark { .. }) => self.i18n.t("fileManager.bookmarkName"),
            _ => self.i18n.t("fileManager.newName"),
        };
        div()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(text_input_anchor_probe(
                target.anchor_id(),
                text_input(
                    &self.tokens,
                    TextInputView {
                        value: &self.file_manager.dialog_value,
                        placeholder,
                        focused: self.file_manager.focused_input
                            == Some(FileManagerInput::DialogValue),
                        caret_visible: self.new_connection_caret_visible,
                        secret: false,
                        selected_all: false,
                        marked_text: self.marked_text_for_target(target),
                    },
                )
                .bg(file_manager_bg(self.tokens.ui.bg_sunken, has_background))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        window.focus(&this.focus_handle);
                        this.file_manager.focused_input = Some(FileManagerInput::DialogValue);
                        this.ime_marked_text = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            ))
            .child(self.render_file_manager_dialog_buttons(false, cx))
            .into_any_element()
    }

    fn render_file_manager_bookmark_dialog(
        &self,
        path: String,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = WorkspaceImeTarget::FileManager(FileManagerInput::DialogValue);
        let workspace = cx.entity();
        div()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("fileManager.editBookmarkDesc")),
            )
            .child(text_input_anchor_probe(
                target.anchor_id(),
                text_input(
                    &self.tokens,
                    TextInputView {
                        value: &self.file_manager.dialog_value,
                        placeholder: self.i18n.t("fileManager.bookmarkName"),
                        focused: self.file_manager.focused_input
                            == Some(FileManagerInput::DialogValue),
                        caret_visible: self.new_connection_caret_visible,
                        secret: false,
                        selected_all: false,
                        marked_text: self.marked_text_for_target(target),
                    },
                )
                .bg(file_manager_bg(self.tokens.ui.bg_sunken, has_background))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        window.focus(&this.focus_handle);
                        this.file_manager.focused_input = Some(FileManagerInput::DialogValue);
                        this.ime_marked_text = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .child(
                        div()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("fileManager.bookmarkPath")),
                    )
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(self.tokens.radii.sm))
                            .bg(file_manager_bg(self.tokens.ui.bg_sunken, has_background))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .truncate()
                            .child(path),
                    ),
            )
            .child(self.render_file_manager_dialog_buttons(false, cx))
            .into_any_element()
    }

    fn render_file_manager_delete_dialog(
        &self,
        files: Vec<String>,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .text_size(px(FILE_MANAGER_TEXT_SM))
            .child(
                self.i18n
                    .t("fileManager.confirmDeleteDesc")
                    .replace("{{count}}", &files.len().to_string()),
            )
            .child(self.render_file_manager_dialog_buttons(true, cx))
            .into_any_element()
    }

    fn render_file_manager_dialog_buttons(
        &self,
        danger: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(
                div()
                    .h(px(32.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .cursor_pointer()
                    .child(self.i18n.t("common.cancel"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.close_file_manager_dialog();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .h(px(32.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .rounded(px(self.tokens.radii.sm))
                    .bg(if danger {
                        rgb(FILE_MANAGER_RED)
                    } else {
                        rgb(theme.accent)
                    })
                    .text_color(rgb(theme.accent_text))
                    .cursor_pointer()
                    .child(if danger {
                        self.i18n.t("fileManager.delete")
                    } else {
                        self.i18n.t("fileManager.go")
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.accept_file_manager_dialog(cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_file_manager_drives_dialog(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().p(px(12.0)).flex().flex_col().gap(px(8.0));
        for drive in local_drives() {
            list = list.child(
                div()
                    .p(px(10.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(file_manager_border(self.tokens.ui.border, has_background))
                    .bg(file_manager_bg(self.tokens.ui.bg, has_background))
                    .hover({
                        let theme = self.tokens.ui;
                        move |row| row.bg(file_manager_hover_bg(theme.bg_hover, has_background))
                    })
                    .cursor_pointer()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::HardDrive,
                                16.0,
                                rgb(self.tokens.ui.accent),
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(drive.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(FILE_MANAGER_TEXT_XS))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(drive.drive_type.clone()),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(FILE_MANAGER_TEXT_XS))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(format!(
                                "{} · {} {} / {}",
                                drive.path,
                                self.i18n.t("fileManager.available"),
                                format_file_size(drive.available_space),
                                format_file_size(drive.total_space),
                            )),
                    )
                    .when(drive.read_only, |row| {
                        row.child(
                            div()
                                .mt(px(4.0))
                                .text_size(px(FILE_MANAGER_TEXT_XS))
                                .text_color(rgb(FILE_MANAGER_ORANGE))
                                .child(self.i18n.t("fileManager.readOnly")),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let path = drive.path.clone();
                            move |this, _event, _window, cx| {
                                this.set_file_manager_path(path.clone());
                                this.close_file_manager_dialog();
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    ),
            );
        }
        list.into_any_element()
    }

    fn render_file_manager_properties_dialog(
        &self,
        entry: LocalFileEntry,
        details: FileManagerProperties,
        _has_background: bool,
    ) -> AnyElement {
        let mut rows = vec![
            (self.i18n.t("fileManager.name"), entry.name),
            (
                self.i18n.t("fileManager.propKind"),
                self.i18n.t(&details.kind_label),
            ),
            (self.i18n.t("fileManager.propLocation"), details.location),
            (
                self.i18n.t("fileManager.size"),
                format_file_size(details.total_size.unwrap_or(details.size)),
            ),
            (
                self.i18n.t("fileManager.modified"),
                format_modified(details.modified),
            ),
            (
                self.i18n.t("fileManager.propAccessed"),
                format_modified(details.accessed),
            ),
            (
                self.i18n.t("fileManager.propAccess"),
                if details.readonly {
                    self.i18n.t("fileManager.readonly")
                } else {
                    self.i18n.t("fileManager.readwrite")
                },
            ),
        ];
        if let Some(target) = details.symlink_target {
            rows.push((self.i18n.t("fileManager.symlink"), target));
        }
        if let (Some(files), Some(dirs)) = (details.dir_files, details.dir_dirs) {
            rows.push((
                self.i18n.t("fileManager.propContents"),
                self.i18n
                    .t("fileManager.propDirSummary")
                    .replace("{{files}}", &files.to_string())
                    .replace("{{dirs}}", &dirs.to_string()),
            ));
        }
        div()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .children(rows.into_iter().map(|(label, value)| {
                div()
                    .flex()
                    .gap(px(12.0))
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .child(
                        div()
                            .w(px(120.0))
                            .flex_none()
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(label),
                    )
                    .child(div().flex_1().min_w(px(0.0)).truncate().child(value))
            }))
            .into_any_element()
    }
}
