impl WorkspaceApp {
    fn render_sftp_context_menu(
        &self,
        menu: SftpContextMenu,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let viewport = window.viewport_size();
        let placement = browser_behavior::clamp_context_menu_position(
            menu.x,
            menu.y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            SFTP_CONTEXT_MENU_WIDTH,
            SFTP_CONTEXT_MENU_MAX_HEIGHT,
            8.0,
        );
        let selected_count = self.sftp_selected_names(menu.pane).len();
        let pane_loading = menu.pane == SftpPane::Remote && self.sftp_view.remote_loading;
        let transfer_loading = self.sftp_view.remote_loading;
        let direction = if menu.pane == SftpPane::Local {
            SftpTransferDirection::Upload
        } else {
            SftpTransferDirection::Download
        };
        let transfer_label = if menu.pane == SftpPane::Local {
            self.i18n.t("sftp.context.upload")
        } else {
            self.i18n.t("sftp.context.download")
        };

        let popup = context_menu_event_boundary(div()
            .w(px(SFTP_CONTEXT_MENU_WIDTH))
            .p(px(SFTP_CONTEXT_MENU_PADDING))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(sftp_border(theme.border, has_background))
            .bg(sftp_panel_bg(theme.bg_elevated, has_background, 0xf2))
            .shadow_lg())
            .when(selected_count > 0, |menu_el| {
                menu_el.child(self.render_sftp_context_menu_guarded_item(
                    if menu.pane == SftpPane::Local {
                        LucideIcon::Upload
                    } else {
                        LucideIcon::Download
                    },
                    transfer_label,
                    false,
                    false,
                    transfer_loading,
                    has_background,
                    cx.listener(move |this, _event, _window, _cx| {
                        this.queue_sftp_transfers(menu.pane, direction);
                    }),
                    cx,
                ))
            })
            .when_some(menu.file.clone(), |menu_el, file| {
                if menu.pane != SftpPane::Remote || file.file_type == SftpFileType::Directory {
                    menu_el
                } else {
                    menu_el.child(self.render_sftp_context_menu_guarded_item(
                        LucideIcon::Eye,
                        self.i18n.t("sftp.context.preview"),
                        false,
                        false,
                        pane_loading,
                        has_background,
                        cx.listener({
                            let file = file.clone();
                            move |this, _event, _window, _cx| {
                                this.open_or_preview_sftp_file(menu.pane, &file);
                            }
                        }),
                        cx,
                    ))
                }
            })
            .when(menu.file.is_some() && selected_count == 1, |menu_el| {
                menu_el.child(self.render_sftp_context_menu_guarded_item(
                    LucideIcon::Pencil,
                    self.i18n.t("sftp.context.rename"),
                    false,
                    false,
                    pane_loading,
                    has_background,
                    cx.listener({
                        let file = menu.file.clone();
                        move |this, _event, _window, _cx| {
                            if let Some(file) = file.as_ref() {
                                this.open_sftp_rename_dialog(menu.pane, file.name.clone());
                            }
                        }
                    }),
                    cx,
                ))
            })
            .when_some(menu.file.clone(), |menu_el, file| {
                menu_el.child(self.render_sftp_context_menu_guarded_item(
                    LucideIcon::Copy,
                    self.i18n.t("sftp.context.copy_path"),
                    false,
                    false,
                    pane_loading,
                    has_background,
                    cx.listener(move |this, _event, _window, cx| {
                        let base = match menu.pane {
                            SftpPane::Local => &this.sftp_view.local_path,
                            SftpPane::Remote => &this.sftp_view.remote_path,
                        };
                        cx.write_to_clipboard(ClipboardItem::new_string(join_sftp_path(
                            base, &file.name,
                        )));
                    }),
                    cx,
                ))
            })
            .when(selected_count > 0, |menu_el| {
                menu_el.child(self.render_sftp_context_menu_guarded_item(
                    LucideIcon::Trash2,
                    self.i18n.t("sftp.context.delete"),
                    true,
                    false,
                    pane_loading,
                    has_background,
                    cx.listener(move |this, _event, _window, _cx| {
                        let files = this.sftp_selected_names(menu.pane);
                        this.sftp_view.dialog = Some(SftpDialog::Delete {
                            pane: menu.pane,
                            files,
                        });
                    }),
                    cx,
                ))
            })
            .child(
                div()
                    .h(px(1.0))
                    .my(px(SFTP_CONTEXT_MENU_PADDING))
                    .bg(sftp_border(theme.border, has_background)),
            )
            .child(self.render_sftp_context_menu_guarded_item(
                LucideIcon::FolderOpen,
                self.i18n.t("sftp.context.new_folder"),
                false,
                false,
                pane_loading,
                has_background,
                cx.listener(move |this, _event, _window, _cx| {
                    this.open_sftp_new_folder_dialog(menu.pane);
                }),
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

    fn render_sftp_context_menu_guarded_item(
        &self,
        icon: LucideIcon,
        label: String,
        danger: bool,
        disabled: bool,
        loading: bool,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let color = if danger { SFTP_RED } else { theme.text };
        let item = div()
            .h(px(SFTP_CONTEXT_MENU_ITEM_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .rounded(px(self.tokens.radii.xs))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(color))
            .child(Self::render_lucide_icon(icon, SFTP_ICON_SM, rgb(color)))
            .child(div().truncate().child(label));
        // SFTP remote refresh/transfer can leave a context menu visible while
        // the backing pane is loading. Route those rows through the shared menu
        // guard so the UI cannot dispatch stale actions.
        self.workspace_context_menu_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: Some(sftp_hover_bg(theme.bg_hover, has_background)),
                hover_text_color: None,
            },
            |this| {
                this.sftp_view.context_menu = None;
            },
            move |_this, event, window, cx| listener(event, window, cx),
            cx,
        )
        .into_any_element()
    }
}
