use super::*;

mod preview;

fn file_manager_dialog_exit_shield() -> AnyElement {
    // Rich preview payloads remain mounted only for painting during exit.
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
        .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation())
        .into_any_element()
}

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
        let placement = browser_behavior::clamp_context_menu_position(
            menu.x,
            menu.y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            FILE_MANAGER_CONTEXT_MENU_WIDTH,
            max_height,
            8.0,
        );
        let selected_count = self.file_manager.selected.len();
        let menu_loading = self.file_manager.loading
            || self
                .file_manager
                .operation_progress
                .as_ref()
                .is_some_and(|progress| progress.active);
        let popup = context_menu_event_boundary(
            div()
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
                .shadow_lg(),
        )
        .when_some(menu.file.clone(), |menu_el, file| {
            if file.file_type == LocalFileType::Directory {
                menu_el.child(self.render_file_manager_context_menu_item(
                    LucideIcon::FolderOpen,
                    self.i18n.t("fileManager.open"),
                    false,
                    has_background,
                    {
                        let file = file.clone();
                        move |this, _event, _window, _cx| {
                            this.set_file_manager_path(file.path.clone());
                        }
                    },
                    cx,
                ))
            } else {
                menu_el
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::ExternalLink,
                        self.i18n.t("fileManager.openExternal"),
                        false,
                        has_background,
                        {
                            let file = file.clone();
                            move |this, _event, _window, _cx| {
                                if let Err(error) = open_path_external(&file.path) {
                                    this.push_file_manager_toast(
                                        this.i18n.t("fileManager.error"),
                                        Some(error),
                                        TerminalNoticeVariant::Error,
                                    );
                                }
                            }
                        },
                        cx,
                    ))
                    .child(self.render_file_manager_context_menu_item(
                        LucideIcon::Eye,
                        self.i18n.t("fileManager.preview"),
                        false,
                        has_background,
                        {
                            let file = file.clone();
                            move |this, _event, _window, cx| {
                                this.open_file_manager_preview(file.clone(), cx);
                            }
                        },
                        cx,
                    ))
            }
        })
        .when(menu.file.is_some(), |menu_el| {
            menu_el.child(self.render_file_manager_context_menu_item(
                LucideIcon::FolderOpen,
                self.i18n.t("fileManager.revealInFileManager"),
                false,
                has_background,
                {
                    let file = menu.file.clone();
                    move |this, _event, _window, _cx| {
                        if let Some(file) = file.as_ref()
                            && let Err(error) = reveal_path_external(&file.path)
                        {
                            this.push_file_manager_toast(
                                this.i18n.t("fileManager.error"),
                                Some(error),
                                TerminalNoticeVariant::Error,
                            );
                        }
                    }
                },
                cx,
            ))
        })
        .when(selected_count > 0, |menu_el| {
            menu_el
                .child(self.render_file_manager_separator(has_background))
                .child(self.render_file_manager_context_menu_guarded_item(
                    LucideIcon::Copy,
                    self.i18n.t("fileManager.copy"),
                    false,
                    false,
                    menu_loading,
                    has_background,
                    |this, _event, _window, cx| {
                        this.copy_file_manager_selection(false, cx);
                    },
                    cx,
                ))
                .child(self.render_file_manager_context_menu_guarded_item(
                    LucideIcon::Pencil,
                    self.i18n.t("fileManager.cut"),
                    false,
                    false,
                    menu_loading,
                    has_background,
                    |this, _event, _window, cx| {
                        this.copy_file_manager_selection(true, cx);
                    },
                    cx,
                ))
                .child(self.render_file_manager_context_menu_guarded_item(
                    LucideIcon::Copy,
                    self.i18n.t("fileManager.duplicate"),
                    false,
                    false,
                    menu_loading,
                    has_background,
                    |this, _event, _window, cx| {
                        this.duplicate_file_manager_selection(cx);
                    },
                    cx,
                ))
                .child(self.render_file_manager_context_menu_guarded_item(
                    LucideIcon::FileArchive,
                    self.i18n.t("fileManager.compress"),
                    false,
                    false,
                    menu_loading,
                    has_background,
                    |this, _event, _window, cx| {
                        this.compress_file_manager_selection(cx);
                    },
                    cx,
                ))
        })
        .when(
            selected_count == 1
                && menu
                    .file
                    .as_ref()
                    .is_some_and(|file| can_extract_archive(&file.name)),
            |menu_el| {
                menu_el.child(self.render_file_manager_context_menu_guarded_item(
                    LucideIcon::FolderArchive,
                    self.i18n.t("fileManager.extract"),
                    false,
                    false,
                    menu_loading,
                    has_background,
                    |this, _event, _window, cx| {
                        this.extract_selected_file_manager_archive(cx);
                    },
                    cx,
                ))
            },
        )
        .when(self.file_manager.clipboard.is_some(), |menu_el| {
            menu_el.child(self.render_file_manager_context_menu_guarded_item(
                LucideIcon::Download,
                self.i18n.t("fileManager.paste"),
                false,
                false,
                menu_loading,
                has_background,
                |this, _event, _window, cx| {
                    this.paste_file_manager_clipboard(cx);
                },
                cx,
            ))
        })
        .when(selected_count == 1, |menu_el| {
            menu_el
                .child(self.render_file_manager_context_menu_guarded_item(
                    LucideIcon::Pencil,
                    self.i18n.t("fileManager.rename"),
                    false,
                    false,
                    menu_loading,
                    has_background,
                    {
                        let file = menu.file.clone();
                        move |this, _event, _window, _cx| {
                            if let Some(file) = file.as_ref() {
                                this.open_file_manager_rename_dialog(file.name.clone());
                            }
                        }
                    },
                    cx,
                ))
                .child(self.render_file_manager_context_menu_item(
                    LucideIcon::Copy,
                    self.i18n.t("fileManager.copyPath"),
                    false,
                    has_background,
                    |this, _event, _window, cx| {
                        this.copy_file_manager_path_to_clipboard(false, cx);
                    },
                    cx,
                ))
                .child(self.render_file_manager_context_menu_item(
                    LucideIcon::FileText,
                    self.i18n.t("fileManager.copyName"),
                    false,
                    has_background,
                    |this, _event, _window, cx| {
                        this.copy_file_manager_path_to_clipboard(true, cx);
                    },
                    cx,
                ))
        })
        .when(selected_count > 0, |menu_el| {
            menu_el
                .child(self.render_file_manager_context_menu_item(
                    LucideIcon::Info,
                    self.i18n.t("fileManager.properties"),
                    false,
                    has_background,
                    {
                        let file = menu.file.clone();
                        move |this, _event, _window, _cx| {
                            if let Some(file) = file
                                .clone()
                                .or_else(|| this.single_selected_file_manager_file())
                            {
                                this.open_file_manager_properties(file);
                            }
                        }
                    },
                    cx,
                ))
                .child(self.render_file_manager_context_menu_guarded_item(
                    LucideIcon::Trash2,
                    self.i18n.t("fileManager.delete"),
                    true,
                    false,
                    menu_loading,
                    has_background,
                    |this, _event, _window, _cx| {
                        this.open_file_manager_delete_dialog();
                    },
                    cx,
                ))
        })
        .child(self.render_file_manager_separator(has_background))
        .child(self.render_file_manager_context_menu_guarded_item(
            LucideIcon::FolderPlus,
            self.i18n.t("fileManager.newFolder"),
            false,
            false,
            menu_loading,
            has_background,
            |this, _event, _window, _cx| {
                this.open_file_manager_new_folder_dialog();
            },
            cx,
        ))
        .child(self.render_file_manager_context_menu_guarded_item(
            LucideIcon::FilePlus,
            self.i18n.t("fileManager.newFile"),
            false,
            false,
            menu_loading,
            has_background,
            |this, _event, _window, _cx| {
                this.open_file_manager_new_file_dialog();
            },
            cx,
        ))
        .child(self.render_file_manager_context_menu_item(
            LucideIcon::Check,
            self.i18n.t("fileManager.selectAll"),
            false,
            has_background,
            |this, _event, _window, _cx| {
                this.select_all_file_manager_files();
            },
            cx,
        ))
        .child(self.render_file_manager_context_menu_item(
            LucideIcon::RefreshCw,
            self.i18n.t("fileManager.refresh"),
            false,
            has_background,
            |this, _event, _window, _cx| {
                this.refresh_file_manager();
            },
            cx,
        ));

        self.workspace_context_menu_backdrop(
            deferred(
                anchored()
                    .anchor(Corner::TopLeft)
                    .position(gpui::point(px(placement.x), px(placement.y)))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(overlay_content_boundary(popup)),
            )
            .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY),
            cx,
        )
        .into_any_element()
    }

    fn render_file_manager_context_menu_item(
        &self,
        icon: LucideIcon,
        label: String,
        danger: bool,
        has_background: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_file_manager_context_menu_guarded_item(
            icon,
            label,
            danger,
            false,
            false,
            has_background,
            listener,
            cx,
        )
    }

    fn render_file_manager_context_menu_guarded_item(
        &self,
        icon: LucideIcon,
        label: String,
        danger: bool,
        disabled: bool,
        loading: bool,
        has_background: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let disabled = disabled
            || self.file_manager.context_menu_presence.phase()
                == oxideterm_gpui_ui::motion::ExitPhase::Exiting;
        let color = if danger { FILE_MANAGER_RED } else { theme.text };
        let item = div()
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
            .child(Self::render_lucide_icon(
                icon,
                FILE_MANAGER_ICON_SM,
                rgb(color),
            ))
            .child(div().truncate().child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "file-manager-context-menu",
                label.clone(),
                label,
                color,
                cx,
            )));
        // File manager uses conditional rendering for most unavailable items,
        // but long-running local operations should leave visible menu rows inert
        // like disabled browser/Radix menu items.
        // The shared workspace menu helper owns cx.listener wrapping, so callers
        // pass plain WorkspaceApp closures and avoid nested same-entity updates.
        self.workspace_context_menu_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: Some(file_manager_hover_bg(theme.bg_hover, has_background)),
                hover_text_color: None,
            },
            |this| {
                this.dismiss_file_manager_context_menu();
            },
            listener,
            cx,
        )
        .into_any_element()
    }

    fn render_file_manager_separator(&self, has_background: bool) -> AnyElement {
        div()
            .h(px(1.0))
            .my(px(FILE_MANAGER_CONTEXT_MENU_PADDING))
            .bg(file_manager_border(self.tokens.ui.border, has_background))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_file_manager_dialog(
        &self,
        window: &mut Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(dialog) = self.file_manager.dialog.clone() else {
            return div().into_any_element();
        };
        if let FileManagerDialog::Preview { entry } = &dialog {
            return self.render_file_manager_preview_modal(
                entry.clone(),
                window,
                has_background,
                cx,
            );
        }
        if let FileManagerDialog::Properties { entry, details } = &dialog {
            return self.render_file_manager_properties_modal(
                entry.clone(),
                details.clone(),
                window,
                has_background,
                cx,
            );
        }
        let title = match &dialog {
            FileManagerDialog::Drives => self.i18n.t("fileManager.selectDrive"),
            FileManagerDialog::NewFolder => self.i18n.t("fileManager.newFolder"),
            FileManagerDialog::NewFile => self.i18n.t("fileManager.newFile"),
            FileManagerDialog::Rename { .. } => self.i18n.t("fileManager.rename"),
            FileManagerDialog::Delete { .. } => self.i18n.t("fileManager.confirmDelete"),
            FileManagerDialog::EditBookmark { .. } => self.i18n.t("fileManager.editBookmark"),
            FileManagerDialog::Properties { .. } => self.i18n.t("fileManager.propTitleGetInfo"),
            FileManagerDialog::Preview { .. } => {
                unreachable!("preview uses dedicated QuickLook modal")
            }
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
                self.render_file_manager_properties_dialog(entry, details, has_background, cx)
            }
            FileManagerDialog::Preview { .. } => {
                unreachable!("preview uses dedicated QuickLook modal")
            }
        };
        let width = FILE_MANAGER_DIALOG_WIDTH_SM;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_file_manager_dialog();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .w(px(width.min(f32::from(window.viewport_size().width) - 32.0)))
                    .max_h(px(
                        (f32::from(window.viewport_size().height) * 0.86).max(240.0)
                    ))
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    // Tauri DialogContent clips all child paint to the rounded
                    // shell. Keep the local file-manager modal on the same
                    // contract so body/footer backgrounds cannot leak.
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | FILE_MANAGER_DIALOG_BORDER_ALPHA,
                    ))
                    // Tauri DialogContent keeps inside clicks from becoming
                    // backdrop outside-click dismissals.
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
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
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "file-manager-dialog-title",
                                        title.clone(),
                                        title,
                                        self.tokens.ui.text,
                                        cx,
                                    )),
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

    fn render_file_manager_preview_modal(
        &self,
        entry: LocalFileEntry,
        window: &mut Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let viewport_width = f32::from(viewport.width);
        let viewport_height = f32::from(viewport.height);
        // Tauri QuickLook uses width/height min(90vw/vh, fixed cap), with 95vw/vh
        // min/max guards so very small windows still leave a little backdrop visible.
        let max_width = viewport_width * 0.95;
        let max_height = viewport_height * 0.95;
        let min_width = FILE_MANAGER_QUICKLOOK_MIN_WIDTH.min(max_width);
        let min_height = FILE_MANAGER_QUICKLOOK_MIN_HEIGHT.min(max_height);
        let width = (viewport_width * 0.9)
            .min(FILE_MANAGER_QUICKLOOK_WIDTH)
            .max(min_width)
            .min(max_width);
        let height = (viewport_height * 0.9)
            .min(FILE_MANAGER_QUICKLOOK_HEIGHT)
            .max(min_height)
            .min(max_height);
        let form_visible = self.file_manager.dialog_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        quicklook_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.begin_file_manager_rich_dialog_exit(cx);
                    cx.stop_propagation();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "file-manager-preview-transition",
                div()
                    .w(px(width))
                    .h(px(height))
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    .overflow_hidden()
                    // Tauri QuickLook is a single rounded border-box, but GPUI's
                    // overflow mask is rectangular. Keep this outer shell
                    // background-free so edge children own the visible color at
                    // every corner instead of exposing a second alpha layer.
                    .border_1()
                    .border_color(rgba(
                        (self.tokens.ui.border << 8) | FILE_MANAGER_DIALOG_BORDER_ALPHA,
                    ))
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(self.render_file_manager_preview_dialog(
                        entry,
                        rounded_shell_child_radius(self.tokens.radii.lg),
                        has_background,
                        window,
                        cx,
                    )),
                form_visible,
            ))
            .when(!form_visible, |backdrop| {
                backdrop.child(file_manager_dialog_exit_shield())
            })
            .into_any_element()
    }

    fn render_file_manager_properties_modal(
        &self,
        entry: LocalFileEntry,
        details: FileManagerProperties,
        window: &mut Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let width =
            FILE_MANAGER_DIALOG_WIDTH_SM.min(f32::from(window.viewport_size().width) - 32.0);
        let (icon, icon_color) = file_icon_for_entry(&entry);
        let icon_color = if icon_color == 0 {
            theme.text_muted
        } else {
            icon_color
        };
        let form_visible = self.file_manager.dialog_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.begin_file_manager_rich_dialog_exit(cx);
                    cx.stop_propagation();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "file-manager-properties-transition",
                div()
                    .w(px(width.max(280.0)))
                    .max_h(px(
                        (f32::from(window.viewport_size().height) * 0.86).max(240.0)
                    ))
                    .flex()
                    .flex_col()
                    .rounded(px(self.tokens.radii.lg))
                    // Tauri property dialogs inherit DialogContent
                    // overflow-hidden; GPUI needs the same explicit clipping
                    // plus footer corner ownership below.
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgba((theme.border << 8) | FILE_MANAGER_DIALOG_BORDER_ALPHA))
                    // Mirrors browser DialogContent bubbling: property fields
                    // stay interactive while the backdrop remains dismissible.
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .bg(file_manager_panel_bg(
                        theme.bg_elevated,
                        has_background,
                        0xf2,
                    ))
                    .shadow_lg()
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(12.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .border_b_1()
                            .border_color(file_manager_border(theme.border, has_background))
                            .child(Self::render_lucide_icon(
                                icon,
                                FILE_MANAGER_ICON_MD,
                                rgb(icon_color),
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(FILE_MANAGER_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text))
                                    .child(self.render_selectable_display_text(
                                        "file-manager-dialog-title",
                                        entry.path.as_str(),
                                        entry.name.clone(),
                                        theme.text,
                                        cx,
                                    )),
                            )
                            .child(self.workspace_icon_action_button(
                                LucideIcon::X,
                                FILE_MANAGER_ICON_MD,
                                rgb(theme.text_muted),
                                IconButtonOptions {
                                    hover_background: Some(rgb(theme.bg_hover)),
                                    // Properties dialog close is an icon-only shadcn button in Tauri.
                                    // Route it through the shared primitive so disabled/focus behavior
                                    // stays consistent with other modal actions.
                                    ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                                },
                                |this, _event, _window, cx| {
                                    this.begin_file_manager_rich_dialog_exit(cx);
                                    cx.stop_propagation();
                                },
                                cx,
                            )),
                    )
                    .child(self.render_file_manager_properties_dialog(
                        entry,
                        details,
                        has_background,
                        cx,
                    ))
                    .child(
                        div()
                            .px(px(16.0))
                            .py(px(10.0))
                            .border_t_1()
                            .border_color(file_manager_border(theme.border, has_background))
                            .bg(file_manager_panel_bg(theme.bg_panel, has_background, 0xff))
                            .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                            .flex()
                            .justify_end()
                            .child(self.workspace_toolbar_action_button(
                                "OK".to_string(),
                                None,
                                ToolbarButtonOptions {
                                    background: Some(file_manager_hover_bg(
                                        theme.bg_hover,
                                        has_background,
                                    )),
                                    text_color: Some(rgb(theme.text)),
                                    hover_background: Some(rgb(theme.text_muted)),
                                    // The OK footer action is a button boundary, not selectable text.
                                    ..ToolbarButtonOptions::compact_text(
                                        ButtonVariant::Secondary,
                                        ButtonRadius::Sm,
                                        28.0,
                                        12.0,
                                        FILE_MANAGER_TEXT_XS,
                                    )
                                },
                                cx.listener(|this, _event, _window, cx| {
                                    this.begin_file_manager_rich_dialog_exit(cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
                form_visible,
            ))
            .when(!form_visible, |backdrop| {
                backdrop.child(file_manager_dialog_exit_shield())
            })
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
                        selected_range: self.ime_selected_range_for_target(target),
                        marked_text: self.marked_text_for_target(target),
                    },
                )
                .bg(file_manager_bg(self.tokens.ui.bg_sunken, has_background))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        window.focus(&this.focus_handle);
                        this.file_manager.focused_input = Some(FileManagerInput::DialogValue);
                        this.file_manager.focused_dialog_footer_action = None;
                        this.ime_marked_text = None;
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
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "file-manager-bookmark-dialog",
                        "description",
                        self.i18n.t("fileManager.editBookmarkDesc"),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
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
                        selected_range: self.ime_selected_range_for_target(target),
                        marked_text: self.marked_text_for_target(target),
                    },
                )
                .bg(file_manager_bg(self.tokens.ui.bg_sunken, has_background))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        window.focus(&this.focus_handle);
                        this.file_manager.focused_input = Some(FileManagerInput::DialogValue);
                        this.ime_marked_text = None;
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
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .text_size(px(FILE_MANAGER_TEXT_XS))
                    .child(div().text_color(rgb(self.tokens.ui.text_muted)).child(
                        self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "file-manager-bookmark-dialog",
                            "path-label",
                            self.i18n.t("fileManager.bookmarkPath"),
                            self.tokens.ui.text_muted,
                            cx,
                        ),
                    ))
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(self.tokens.radii.sm))
                            .bg(file_manager_bg(self.tokens.ui.bg_sunken, has_background))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .truncate()
                            .child(self.render_selectable_display_text(
                                "file-manager-bookmark-path",
                                "edit-bookmark",
                                path,
                                self.tokens.ui.text,
                                cx,
                            )),
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
        let primary_disabled = self.file_manager_dialog_primary_disabled();
        div()
            .flex()
            .justify_end()
            .gap(px(8.0))
            .child(
                // Tauri LocalFileManager uses DialogFooter + shadcn Button
                // for create/rename/delete prompts. Keep native prompt
                // buttons on the same shared button primitive as other modal
                // footers instead of hand-drawing div chrome here.
                self.workspace_confirm_footer_action_button(
                    self.i18n.t("common.actions.cancel"),
                    ButtonVariant::Ghost,
                    ConfirmDialogAction::Cancel,
                    false,
                    self.file_manager.focused_dialog_footer_action,
                    |this, _event, _window, cx| {
                        this.close_file_manager_dialog();
                        cx.stop_propagation();
                        cx.notify();
                    },
                    cx,
                ),
            )
            .child(self.workspace_confirm_footer_action_button(
                if danger {
                    self.i18n.t("fileManager.delete")
                } else {
                    self.i18n.t("fileManager.go")
                },
                if danger {
                    ButtonVariant::Destructive
                } else {
                    ButtonVariant::Default
                },
                ConfirmDialogAction::Confirm,
                primary_disabled,
                self.file_manager.focused_dialog_footer_action,
                |this, _event, _window, cx| {
                    this.accept_file_manager_dialog(cx);
                    cx.stop_propagation();
                },
                cx,
            ))
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
                                rgb(self.tokens.ui.text_secondary),
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
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "file-manager-drive-meta",
                                drive.path.as_str(),
                                format!(
                                    "{} · {} {} / {}",
                                    drive.path,
                                    self.i18n.t("fileManager.available"),
                                    format_file_size(drive.available_space),
                                    format_file_size(drive.total_space),
                                ),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    )
                    .when(drive.read_only, |row| {
                        row.child(
                            div()
                                .mt(px(4.0))
                                .text_size(px(FILE_MANAGER_TEXT_XS))
                                .text_color(rgb(FILE_MANAGER_ORANGE))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "file-manager-drive-meta",
                                    (drive.path.as_str(), "read-only"),
                                    self.i18n.t("fileManager.readOnly"),
                                    FILE_MANAGER_ORANGE,
                                    cx,
                                )),
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
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_dir = entry.file_type == LocalFileType::Directory;
        let file_type = if is_dir || entry.file_type == LocalFileType::Symlink {
            self.i18n.t(&details.kind_label)
        } else {
            details
                .mime_type
                .clone()
                .unwrap_or_else(|| self.i18n.t(&details.kind_label))
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
            if let Some(checksum) = self.file_manager.properties_checksum.clone() {
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

    fn render_file_manager_property_row_text(
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
                value.clone(),
                self.tokens.ui.text,
                cx,
            ));
        if mono {
            value_el =
                value_el.font_family(settings_mono_font_family(self.settings_store.settings()));
        }
        self.render_file_manager_property_row_value(label, value_el.into_any_element(), cx)
    }

    fn render_file_manager_property_row_value(
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

    fn render_file_manager_property_separator(&self, has_background: bool) -> AnyElement {
        div()
            .h(px(1.0))
            .my(px(6.0))
            .bg(file_manager_border(self.tokens.ui.border, has_background))
            .into_any_element()
    }

    fn render_file_manager_property_size(&self, size: u64, cx: &mut Context<Self>) -> AnyElement {
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

    fn render_file_manager_property_permissions(
        &self,
        mode: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let perms = format_permission_bits(mode);
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(1.0))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
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

    fn render_file_manager_checksum_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let loading = self.file_manager.properties_checksum_loading;
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
