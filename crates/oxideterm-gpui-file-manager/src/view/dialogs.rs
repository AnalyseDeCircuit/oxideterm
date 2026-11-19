mod properties;
use super::*;
use oxideterm_gpui_ui::context_menu::{context_menu_content, context_menu_separator};

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
        .on_scroll(|_event, _window, cx| cx.stop_propagation())
        .into_any_element()
}

impl FileManagerView {
    pub(super) fn render_file_manager_context_menu(
        &self,
        menu: FileManagerContextMenu,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (selected_count, menu_loading, has_clipboard) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.selected.len(),
                file_manager.loading
                    || file_manager
                        .operation_progress
                        .as_ref()
                        .is_some_and(|progress| progress.active),
                file_manager.clipboard.is_some(),
            )
        };
        let mut rows: Vec<AnyElement> = Vec::with_capacity(20);
        let mut item_count = 0usize;
        let mut separator_count = 0usize;
        if let Some(file) = menu.file.clone() {
            if file.file_type == LocalFileType::Directory {
                rows.push(self.render_file_manager_context_menu_item(
                    LucideIcon::FolderOpen,
                    self.i18n.t("fileManager.open"),
                    false,
                    has_background,
                    move |this, _event, _window, cx| {
                        this.set_file_manager_path(file.path.clone(), cx);
                    },
                    cx,
                ));
                item_count += 1;
            } else {
                rows.push(self.render_file_manager_context_menu_item(
                    LucideIcon::ExternalLink,
                    self.i18n.t("fileManager.openExternal"),
                    false,
                    has_background,
                    {
                        let file = file.clone();
                        move |this, _event, _window, cx| {
                            if let Err(error) = open_path_external(&file.path) {
                                this.push_file_manager_toast(
                                    this.i18n.t("fileManager.error"),
                                    Some(error),
                                    FileManagerNoticeKind::Error,
                                    cx,
                                );
                            }
                        }
                    },
                    cx,
                ));
                rows.push(self.render_file_manager_context_menu_item(
                    LucideIcon::Eye,
                    self.i18n.t("fileManager.preview"),
                    false,
                    has_background,
                    move |this, _event, _window, cx| {
                        this.open_file_manager_preview(file.clone(), cx);
                    },
                    cx,
                ));
                item_count += 2;
            }
        }
        if menu.file.is_some() {
            rows.push(self.render_file_manager_context_menu_item(
                LucideIcon::FolderOpen,
                self.i18n.t("fileManager.revealInFileManager"),
                false,
                has_background,
                {
                    let file = menu.file.clone();
                    move |this, _event, _window, cx| {
                        if let Some(file) = file.as_ref()
                            && let Err(error) = reveal_path_external(&file.path)
                        {
                            this.push_file_manager_toast(
                                this.i18n.t("fileManager.error"),
                                Some(error),
                                FileManagerNoticeKind::Error,
                                cx,
                            );
                        }
                    }
                },
                cx,
            ));
            item_count += 1;
        }
        if selected_count > 0 {
            rows.push(self.render_file_manager_separator(has_background));
            separator_count += 1;
            rows.push(self.render_file_manager_context_menu_guarded_item(
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
            ));
            rows.push(self.render_file_manager_context_menu_guarded_item(
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
            ));
            rows.push(self.render_file_manager_context_menu_guarded_item(
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
            ));
            rows.push(self.render_file_manager_context_menu_guarded_item(
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
            ));
            item_count += 4;
        }
        if selected_count == 1
            && menu
                .file
                .as_ref()
                .is_some_and(|file| can_extract_archive(&file.name))
        {
            rows.push(self.render_file_manager_context_menu_guarded_item(
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
            ));
            item_count += 1;
        }
        if has_clipboard {
            rows.push(self.render_file_manager_context_menu_guarded_item(
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
            ));
            item_count += 1;
        }
        if selected_count == 1 {
            rows.push(self.render_file_manager_context_menu_guarded_item(
                LucideIcon::Pencil,
                self.i18n.t("fileManager.rename"),
                false,
                false,
                menu_loading,
                has_background,
                {
                    let file = menu.file.clone();
                    move |this, _event, _window, cx| {
                        if let Some(file) = file.as_ref() {
                            this.open_file_manager_rename_dialog(file.name.clone(), cx);
                        }
                    }
                },
                cx,
            ));
            rows.push(self.render_file_manager_context_menu_item(
                LucideIcon::Copy,
                self.i18n.t("fileManager.copyPath"),
                false,
                has_background,
                |this, _event, _window, cx| {
                    this.copy_file_manager_path_to_clipboard(false, cx);
                },
                cx,
            ));
            rows.push(self.render_file_manager_context_menu_item(
                LucideIcon::FileText,
                self.i18n.t("fileManager.copyName"),
                false,
                has_background,
                |this, _event, _window, cx| {
                    this.copy_file_manager_path_to_clipboard(true, cx);
                },
                cx,
            ));
            item_count += 3;
        }
        if selected_count > 0 {
            rows.push(self.render_file_manager_context_menu_item(
                LucideIcon::Info,
                self.i18n.t("fileManager.properties"),
                false,
                has_background,
                {
                    let file = menu.file.clone();
                    move |this, _event, _window, cx| {
                        if let Some(file) = file
                            .clone()
                            .or_else(|| this.single_selected_file_manager_file(cx))
                        {
                            this.open_file_manager_properties(file, cx);
                        }
                    }
                },
                cx,
            ));
            rows.push(self.render_file_manager_context_menu_guarded_item(
                LucideIcon::Trash2,
                self.i18n.t("fileManager.delete"),
                true,
                false,
                menu_loading,
                has_background,
                |this, _event, _window, cx| {
                    this.open_file_manager_delete_dialog(cx);
                },
                cx,
            ));
            item_count += 2;
        }
        rows.push(self.render_file_manager_separator(has_background));
        separator_count += 1;
        rows.push(self.render_file_manager_context_menu_guarded_item(
            LucideIcon::FolderPlus,
            self.i18n.t("fileManager.newFolder"),
            false,
            false,
            menu_loading,
            has_background,
            |this, _event, _window, cx| {
                this.open_file_manager_new_folder_dialog(cx);
            },
            cx,
        ));
        rows.push(self.render_file_manager_context_menu_guarded_item(
            LucideIcon::FilePlus,
            self.i18n.t("fileManager.newFile"),
            false,
            false,
            menu_loading,
            has_background,
            |this, _event, _window, cx| {
                this.open_file_manager_new_file_dialog(cx);
            },
            cx,
        ));
        rows.push(self.render_file_manager_context_menu_item(
            LucideIcon::Check,
            self.i18n.t("fileManager.selectAll"),
            false,
            has_background,
            |this, _event, _window, cx| {
                this.select_all_file_manager_files(cx);
            },
            cx,
        ));
        rows.push(self.render_file_manager_context_menu_item(
            LucideIcon::RefreshCw,
            self.i18n.t("fileManager.refresh"),
            false,
            has_background,
            |this, _event, _window, cx| {
                this.refresh_file_manager_with_drives(cx);
            },
            cx,
        ));
        rows.push(self.render_file_manager_context_menu_item(
            LucideIcon::HardDrive,
            self.i18n.t("fileManager.showDrives"),
            false,
            has_background,
            |this, _event, _window, cx| {
                this.open_file_manager_drives_dialog(cx);
            },
            cx,
        ));
        item_count += 5;

        let viewport = window.viewport_size();
        let viewport_height = f32::from(viewport.height);
        // Clamp only to the usable viewport height instead of a 520px ceiling that clipped the bottom row.
        let max_height =
            (viewport_height - FILE_MANAGER_CONTEXT_MENU_VIEWPORT_MARGIN * 2.0).max(180.0);
        let vertical_chrome = self.tokens.metrics.ui_menu_padding * 2.0 + 2.0;
        let estimated_height = (item_count as f32) * FILE_MANAGER_CONTEXT_MENU_ITEM_HEIGHT
            + (separator_count as f32)
                * oxideterm_gpui_ui::context_menu::context_menu_separator_height_estimate(
                    &self.tokens,
                )
            + vertical_chrome;
        let placement = browser_behavior::clamp_context_menu_position(
            menu.x,
            menu.y,
            f32::from(viewport.width),
            viewport_height,
            FILE_MANAGER_CONTEXT_MENU_WIDTH,
            estimated_height.min(max_height),
            FILE_MANAGER_CONTEXT_MENU_VIEWPORT_MARGIN,
        );
        let inner_max_height = (max_height - vertical_chrome).max(120.0);
        let popup = context_menu_event_boundary(
            context_menu_content(&self.tokens)
                .w(px(FILE_MANAGER_CONTEXT_MENU_WIDTH))
                .min_w_0()
                .max_h(px(max_height))
                .when(has_background, |surface| {
                    surface.border_color(file_manager_border(theme.border, has_background))
                })
                .flex()
                .flex_col()
                .child(
                    div()
                        .w_full()
                        .max_h(px(inner_max_height))
                        .overflow_y_scrollbar()
                        .flex()
                        .flex_col()
                        .children(rows),
                ),
        );

        let menu_visible = self.file_manager.read(cx).context_menu_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        self.workspace_context_menu_backdrop(
            deferred(
                anchored()
                    .anchor(Corner::TopLeft)
                    .position(gpui::point(px(placement.x), px(placement.y)))
                    .position_mode(AnchoredPositionMode::Window)
                    .child(oxideterm_gpui_ui::motion::zoom_fade_y(
                        &self.tokens,
                        "file-manager-context-menu-motion",
                        div().child(overlay_content_boundary(popup)),
                        -4.0,
                        0.95,
                        gpui::point(0.0, 0.0),
                        oxideterm_gpui_ui::motion::MotionDuration::Micro,
                        menu_visible,
                    )),
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
            || self.file_manager.read(cx).context_menu_presence.phase()
                == oxideterm_gpui_ui::motion::ExitPhase::Exiting;
        let color = if danger { FILE_MANAGER_RED } else { theme.text };
        let item =
            div()
                .h(px(FILE_MANAGER_CONTEXT_MENU_ITEM_HEIGHT))
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(self.tokens.metrics.ui_menu_item_padding_x))
                .py(px(5.0))
                .rounded(px(self.tokens.radii.md))
                .text_size(px(FILE_MANAGER_TEXT_XS))
                .text_color(rgb(color))
                .child(Self::render_lucide_icon(
                    icon,
                    FILE_MANAGER_ICON_SM,
                    if danger {
                        rgb(color).into()
                    } else {
                        rgba((color << 8) | 0xd9)
                    },
                ))
                .child(div().flex_1().min_w_0().truncate().child(
                    self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "file-manager-context-menu",
                        label.clone(),
                        label,
                        color,
                        cx,
                    ),
                ));
        let hover_bg = if danger {
            rgba((FILE_MANAGER_RED << 8) | 0x1f)
        } else {
            file_manager_hover_bg(theme.bg_hover, has_background)
        };
        // File manager uses conditional rendering for most unavailable items,
        // but long-running local operations should leave visible menu rows inert
        // like disabled browser/Radix menu items.
        // The shared workspace menu helper owns cx.listener wrapping, so callers
        // pass plain WorkspaceApp closures and avoid nested same-entity updates.
        let listener_with_dismissal =
            move |this: &mut Self,
                  event: &MouseDownEvent,
                  window: &mut Window,
                  cx: &mut Context<Self>| {
                this.dismiss_file_manager_context_menu(cx);
                listener(this, event, window, cx);
            };
        self.workspace_context_menu_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: Some(hover_bg),
                hover_text_color: None,
            },
            |_| {},
            listener_with_dismissal,
            cx,
        )
        .into_any_element()
    }

    fn render_file_manager_separator(&self, _has_background: bool) -> AnyElement {
        context_menu_separator(&self.tokens).into_any_element()
    }

    pub fn render_file_manager_dialog(
        &self,
        window: &mut Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(dialog) = self.file_manager.read(cx).dialog.clone() else {
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
        let card = material_surface(&self.tokens, div(), MaterialRole::Dialog)
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
            .shadow_lg()
            .child(
                div()
                    .h(px(48.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(file_manager_border(self.tokens.ui.border, has_background))
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
                                    this.close_file_manager_dialog(cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .child(body);
        staged_dialog_overlay(
            &self.tokens,
            "file-manager-dialog",
            TauriBackdropRole::Dialog,
            staged_overlay_root().on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_file_manager_dialog(cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            ),
            card,
            8.0,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
            true,
        )
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
        let form_visible = self.file_manager.read(cx).dialog_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        staged_overlay_root()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.begin_file_manager_rich_dialog_exit(cx);
                    cx.stop_propagation();
                }),
            )
            .child(staged_overlay_scrim(
                &self.tokens,
                "file-manager-preview-scrim",
                TauriBackdropRole::QuickLook,
                form_visible,
            ))
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
        details: oxideterm_local_files::LocalFileProperties,
        window: &mut Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let width =
            FILE_MANAGER_DIALOG_WIDTH_SM.min(f32::from(window.viewport_size().width) - 32.0);
        let icon = file_icon_for_entry(&entry);
        let form_visible = self.file_manager.read(cx).dialog_presence.phase()
            == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        staged_overlay_root()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.begin_file_manager_rich_dialog_exit(cx);
                    cx.stop_propagation();
                }),
            )
            .child(staged_overlay_scrim(
                &self.tokens,
                "file-manager-properties-scrim",
                TauriBackdropRole::Dialog,
                form_visible,
            ))
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "file-manager-properties-transition",
                material_surface(&self.tokens, div(), MaterialRole::Dialog)
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
                            .child(icon.render(FILE_MANAGER_ICON_MD, &self.tokens))
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
        let target = FileManagerInput::DialogValue;
        let (dialog, dialog_value, focused) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog.clone(),
                file_manager.dialog_value.clone(),
                file_manager.focused_input == Some(FileManagerInput::DialogValue),
            )
        };
        let placeholder = match dialog {
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
            .child(
                self.text_input_with_workspace_ime(
                    target,
                    self.workspace_text_input(
                        target,
                        TextInputView {
                            value: &dialog_value,
                            placeholder,
                            focused,
                            caret_visible: self.caret_visible,
                            secret: false,
                            selected_all: false,
                            selected_range: self.ime_selected_range_for_target(target, cx),
                            marked_text: self.marked_text_for_target(target, cx),
                        },
                    )
                    .bg(file_manager_bg(self.tokens.ui.bg_sunken, has_background)),
                    |this, cx| {
                        this.file_manager.update(cx, |file_manager, cx| {
                            file_manager.focused_input = Some(FileManagerInput::DialogValue);
                            file_manager.focused_dialog_footer_action = None;
                            cx.notify();
                        });
                    },
                    cx,
                ),
            )
            .child(self.render_file_manager_dialog_buttons(false, cx))
            .into_any_element()
    }

    fn render_file_manager_bookmark_dialog(
        &self,
        path: String,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = FileManagerInput::DialogValue;
        let (dialog_value, focused) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.clone(),
                file_manager.focused_input == Some(FileManagerInput::DialogValue),
            )
        };
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
            .child(
                self.text_input_with_workspace_ime(
                    target,
                    self.workspace_text_input(
                        target,
                        TextInputView {
                            value: &dialog_value,
                            placeholder: self.i18n.t("fileManager.bookmarkName"),
                            focused,
                            caret_visible: self.caret_visible,
                            secret: false,
                            selected_all: false,
                            selected_range: self.ime_selected_range_for_target(target, cx),
                            marked_text: self.marked_text_for_target(target, cx),
                        },
                    )
                    .bg(file_manager_bg(self.tokens.ui.bg_sunken, has_background)),
                    |this, cx| {
                        this.file_manager.update(cx, |file_manager, cx| {
                            file_manager.focused_input = Some(FileManagerInput::DialogValue);
                            cx.notify();
                        });
                    },
                    cx,
                ),
            )
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
                            .font_family(self.mono_font_family.clone())
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
        let primary_disabled = self.file_manager_dialog_primary_disabled(cx);
        let focused_action = self.file_manager.read(cx).focused_dialog_footer_action;
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
                    focused_action,
                    |this, _event, _window, cx| {
                        this.close_file_manager_dialog(cx);
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
                focused_action,
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
        let drives = self.file_manager.read(cx).drives.clone();
        for drive in &drives {
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
                                this.set_file_manager_path(path.clone(), cx);
                                this.close_file_manager_dialog(cx);
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }),
                    ),
            );
        }
        list.into_any_element()
    }
}
