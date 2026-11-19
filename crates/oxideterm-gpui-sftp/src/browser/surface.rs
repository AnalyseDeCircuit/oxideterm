use super::*;

impl SftpBrowserView {
    pub(super) fn render_sftp_sidebar_surface(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let theme = self.tokens.ui;
        if self.active_sftp_tab {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .px_4()
                .text_size(px(ui.text_xs))
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("sftp.sidebar.active_tab_notice"))
                .into_any_element();
        }
        let Some(node_id) = self.sidebar_target.clone() else {
            return div()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .px_4()
                .text_size(px(ui.text_xs))
                .text_color(rgb(theme.text_muted))
                .child(render_lucide_icon(
                    LucideIcon::FolderOpen,
                    28.0,
                    rgb(theme.text_muted),
                ))
                .child(self.i18n.t("sftp.sidebar.no_follow_target"))
                .child(self.render_sftp_icon_button(
                    LucideIcon::X,
                    self.i18n.t("sftp.preview.close"),
                    cx.listener(|this, _event, _window, cx| {
                        if let Some(node) = this.sidebar_open.clone() {
                            this.send(SftpBrowserRequest::CloseSidebar(node.clone()), None, cx);
                        }
                        cx.stop_propagation();
                    }),
                    cx.entity(),
                ))
                .into_any_element();
        };
        let context_menu_exit_delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.schedule_context_menu_exit(context_menu_exit_delay, cx);
        });
        let snapshot = self.sftp_view().read(cx).surface_render_snapshot();
        let dialog_open = snapshot.dialog_open;
        let context_menu = snapshot.context_menu.clone();
        let node_title = self.remote_title.clone();
        let path = if snapshot.remote.path.is_empty() {
            "/".to_string()
        } else {
            snapshot.remote.path.clone()
        };
        let open_node_id = node_id.clone();
        let close_node_id = node_id.clone();

        let mut root = div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(ui.text_xs))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(node_title),
                            )
                            .child(self.render_sftp_icon_button(
                                if self.embedded_sftp_pinned {
                                    LucideIcon::Pin
                                } else {
                                    LucideIcon::Link2
                                },
                                self.i18n.t(if self.embedded_sftp_pinned {
                                    "sftp.sidebar.unpin"
                                } else {
                                    "sftp.sidebar.pin"
                                }),
                                cx.listener(|this, _event, _window, cx| {
                                    this.send(SftpBrowserRequest::TogglePin, None, cx);
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            ))
                            .child(self.render_sftp_nav_button(
                                SftpPane::Remote,
                                "..",
                                LucideIcon::ArrowUp,
                                "sftp.toolbar.go_up",
                                cx,
                            ))
                            .child(self.render_sftp_tree_mode_button(cx))
                            .child(self.render_sftp_refresh_button(SftpPane::Remote, cx))
                            .child(self.render_sftp_terminal_directory_button(cx))
                            .child(self.render_sftp_sync_terminal_directory_button(cx))
                            .child(self.render_sftp_bookmarks_button(cx))
                            .child(self.render_sftp_icon_button(
                                LucideIcon::Upload,
                                self.i18n.t("sftp.context.upload"),
                                cx.listener(|this, _event, _window, cx| {
                                    this.send(SftpBrowserRequest::BrowseUploads, None, cx);
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            ))
                            .child(self.render_sftp_icon_button(
                                LucideIcon::MoreVertical,
                                self.i18n.t("sftp.sidebar.actions"),
                                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                    this.sftp_view().update(cx, |sftp, cx| {
                                        let selected_file = sftp
                                            .remote_last_selected
                                            .as_ref()
                                            .and_then(|name| {
                                                sftp.remote_files
                                                    .iter()
                                                    .find(|file| &file.name == name)
                                            })
                                            .cloned();
                                        sftp.open_context_menu(
                                            SftpPane::Remote,
                                            selected_file,
                                            f32::from(event.position.x),
                                            f32::from(event.position.y),
                                            cx,
                                        );
                                    });
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            ))
                            .child(self.render_sftp_icon_button(
                                LucideIcon::ExternalLink,
                                self.i18n.t("sftp.sidebar.open_tab"),
                                cx.listener(move |this, _event, _window, cx| {
                                    let current_path =
                                        this.sftp_view().read(cx).remote_path.clone();
                                    this.send(
                                        SftpBrowserRequest::OpenTab(
                                            open_node_id.clone(),
                                            current_path,
                                        ),
                                        None,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            ))
                            // Dismissing the embedded surface leaves the node
                            // and its other consumers connected.
                            .child(self.render_sftp_icon_button(
                                LucideIcon::X,
                                self.i18n.t("sftp.preview.close"),
                                cx.listener(move |this, _event, _window, cx| {
                                    this.send(
                                        SftpBrowserRequest::CloseSidebar(close_node_id.clone()),
                                        None,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                                cx.entity(),
                            )),
                    )
                    .child(self.render_sftp_sidebar_path_bar(
                        &path,
                        &snapshot.remote.path_input,
                        snapshot.remote.path_editing,
                        snapshot.focused_input,
                        cx,
                    )),
            )
            .child(self.render_sftp_filter(
                SftpPane::Remote,
                &snapshot.remote.filter,
                snapshot.focused_input,
                false,
                cx,
            ))
            .child(self.render_sftp_file_list(SftpPane::Remote, false, cx))
            .child(self.render_sftp_sidebar_transfer_queue(&node_id, cx));
        if !dialog_open && let Some(menu) = context_menu {
            // The menu is window-anchored so it can extend beyond the narrow
            // sidebar without being clipped by the sidebar scroll region.
            root = root.child(self.render_sftp_context_menu(menu, window, false, cx));
        }
        if !dialog_open && let Some(menu) = snapshot.bookmark_menu {
            root = root.child(self.render_sftp_bookmark_menu(menu, window, false, cx));
        }
        if !dialog_open
            && snapshot.focused_input == Some(SftpInput::RemotePath)
            && let Some(completion) = self.render_path_completion_overlay(SftpPane::Remote, cx)
        {
            root = root.child(completion);
        }
        root.capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.send(SftpBrowserRequest::Focus, None, cx);
        }))
        .into_any_element()
    }

    fn render_sftp_sidebar_path_bar(
        &self,
        path: &str,
        path_input: &str,
        editing: bool,
        focused_input: Option<SftpInput>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let theme = self.tokens.ui;
        let focused = focused_input == Some(SftpInput::RemotePath);
        let mut path_bar = div()
            .id("sftp-sidebar-path-input")
            .w_full()
            .min_w(px(0.0))
            .h(px(ui.size(24.0)))
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if focused {
                rgb(theme.accent)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg_sunken));

        if editing {
            // The sidebar uses one compact text field instead of the full breadcrumb row.
            path_bar = path_bar
                .child(self.render_sftp_inline_text(
                    SftpInput::RemotePath,
                    Some(SftpPane::Remote),
                    path_input,
                    "sftp.file_list.path_placeholder",
                    focused,
                    cx,
                ))
                .child(
                    div()
                        .flex_none()
                        .size(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.sm))
                        .cursor_pointer()
                        .hover(move |button| button.bg(rgb(theme.bg_hover)))
                        .child(render_lucide_icon(
                            LucideIcon::CornerDownLeft,
                            ui.size(SFTP_ICON_SM),
                            rgb(theme.text),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.enter_path(SftpPane::Remote, cx);
                                cx.stop_propagation();
                            }),
                        ),
                );
        } else {
            path_bar = path_bar
                .cursor(CursorStyle::IBeam)
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(ui.text_small))
                        .text_color(rgb(theme.text_muted))
                        .child(path.to_string()),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        window.focus(&this.focus_handle, cx);
                        this.start_path_edit(SftpPane::Remote, cx);
                        cx.stop_propagation();
                    }),
                );
        }

        path_bar.into_any_element()
    }

    pub(super) fn render_sftp_surface_for_tab(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_background = self.has_background;
        let theme = self.tokens.ui;
        let context_menu_exit_delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.schedule_context_menu_exit(context_menu_exit_delay, cx);
        });
        let SftpSurfaceRenderSnapshot {
            init_error,
            pane_split_ratio,
            active_pane,
            drag_over_pane,
            focused_input,
            dialog_open,
            context_menu,
            bookmark_menu,
            local,
            remote,
        } = self.sftp_view().read(cx).surface_render_snapshot();
        let queue_height = self
            .sftp_view()
            .read(cx)
            .queue_height_for_viewport(window.viewport_size());
        let local_active = active_pane == SftpPane::Local;
        let remote_active = active_pane == SftpPane::Remote;
        let local_drag_over = drag_over_pane == Some(SftpPane::Local);
        let remote_drag_over = drag_over_pane == Some(SftpPane::Remote);
        let remote_title = self.remote_title.clone();
        let pair_primary_title = self.pair_primary_title.clone();
        let surface = self.sftp_view().clone();
        let mut root = div()
            .id("sftp-view")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .p(px(SFTP_ROOT_PADDING))
            .gap(px(SFTP_GAP))
            .bg(sftp_bg(theme.bg, has_background))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    let menu_changed = this
                        .sftp_view()
                        .update(cx, |sftp, _cx| sftp.clear_context_menu_immediately());
                    if menu_changed {
                        // Ordinary pane clicks already repaint through their
                        // own state changes, so skip a no-op background repaint.
                        cx.notify();
                    }
                }),
            )
            .when_some(init_error.as_ref(), |root, error| {
                root.child(self.render_sftp_init_error(error, has_background, cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left_0()
                            .right(relative(1.0 - pane_split_ratio))
                            .pr(px(SFTP_GAP / 2.0))
                            .flex()
                            .child(self.render_sftp_pane(
                                SftpPane::Local,
                                pair_primary_title.map_or_else(
                                    || self.i18n.t("sftp.file_list.local"),
                                    |host| {
                                        self.i18n
                                            .t("sftp.file_list.remote")
                                            .replace("{{host}}", &host)
                                    },
                                ),
                                local,
                                local_active,
                                local_drag_over,
                                focused_input,
                                has_background,
                                window,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(relative(pane_split_ratio))
                            .right_0()
                            .pl(px(SFTP_GAP / 2.0))
                            .flex()
                            .child(
                                self.render_sftp_pane(
                                    SftpPane::Remote,
                                    self.i18n
                                        .t("sftp.file_list.remote")
                                        .replace("{{host}}", &remote_title),
                                    remote,
                                    remote_active,
                                    remote_drag_over,
                                    focused_input,
                                    has_background,
                                    window,
                                    cx,
                                ),
                            ),
                    )
                    .child(
                        div()
                            .id("sftp-pane-resize-handle")
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(relative(pane_split_ratio))
                            .ml(px(-SFTP_PANE_SPLIT_HOTZONE_WIDTH / 2.0))
                            .w(px(SFTP_PANE_SPLIT_HOTZONE_WIDTH))
                            .cursor(CursorStyle::ResizeColumn)
                            // The hotzone covers both pane borders and the gap between them.
                            .occlude()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                    let capture_changed =
                                        this.sftp_view().update(cx, |state, cx| {
                                            if event.click_count >= 2 {
                                                state.reset_pane_split(cx)
                                            } else {
                                                state.start_pane_resize(event.position.x, cx);
                                                true
                                            }
                                        });
                                    if capture_changed {
                                        this.send(
                                            SftpBrowserRequest::ResizeCaptureChanged,
                                            None,
                                            cx,
                                        );
                                    }
                                    window.prevent_default();
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            )
            .child(self.render_sftp_transfer_queue(queue_height, has_background, cx))
            .child(
                div()
                    .id("sftp-queue-resize-handle")
                    .absolute()
                    .left(px(SFTP_ROOT_PADDING))
                    .right(px(SFTP_ROOT_PADDING))
                    .bottom(px(SFTP_ROOT_PADDING + queue_height
                        - (SFTP_QUEUE_SPLIT_HOTZONE_HEIGHT - SFTP_GAP) / 2.0))
                    .h(px(SFTP_QUEUE_SPLIT_HOTZONE_HEIGHT))
                    .cursor(CursorStyle::ResizeRow)
                    // The hotzone spans the file-area border, gap, and queue border.
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            let viewport = window.viewport_size();
                            let capture_changed = this.sftp_view().update(cx, |state, cx| {
                                if event.click_count >= 2 {
                                    state.reset_queue_height(viewport, cx)
                                } else {
                                    state.start_queue_resize(event.position.y, viewport, cx);
                                    true
                                }
                            });
                            if capture_changed {
                                this.send(SftpBrowserRequest::ResizeCaptureChanged, None, cx);
                            }
                            window.prevent_default();
                            cx.stop_propagation();
                        }),
                    ),
            );

        if !dialog_open && let Some(menu) = context_menu {
            root = root.child(self.render_sftp_context_menu(menu, window, has_background, cx));
        }
        if !dialog_open && let Some(menu) = bookmark_menu {
            root = root.child(self.render_sftp_bookmark_menu(menu, window, has_background, cx));
        }
        if !dialog_open {
            let completion_owner = match focused_input {
                Some(SftpInput::LocalPath) => Some(SftpPane::Local),
                Some(SftpInput::RemotePath) => Some(SftpPane::Remote),
                _ => None,
            };
            if let Some(owner) = completion_owner
                && let Some(completion) = self.render_path_completion_overlay(owner, cx)
            {
                root = root.child(completion);
            }
        }

        let root =
            root.capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                this.send(SftpBrowserRequest::Focus, None, cx);
            }));
        div()
            .size_full()
            .child(root)
            .on_children_prepainted(move |bounds, _, cx| {
                let Some(bounds) = bounds.first() else {
                    return;
                };
                surface.update(cx, |sftp, cx| {
                    if sftp.surface_size != Some(bounds.size) {
                        sftp.surface_size = Some(bounds.size);
                        cx.notify();
                    }
                });
            })
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_sftp_pane(
        &self,
        pane: SftpPane,
        title: String,
        snapshot: SftpPaneRenderSnapshot,
        active: bool,
        drag_over: bool,
        focused_input: Option<SftpInput>,
        has_background: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let SftpPaneRenderSnapshot {
            path,
            filter,
            sort_field,
            sort_direction,
            selected_count,
            path_editing,
            path_input,
        } = snapshot;
        let drag_bg = rgba((theme.accent << 8) | SFTP_DRAG_BG_ALPHA);
        let drag_border = rgba((theme.accent << 8) | SFTP_DRAG_RING_ALPHA);
        let transfer_direction = if pane == SftpPane::Local {
            SftpTransferDirection::Upload
        } else {
            SftpTransferDirection::Download
        };

        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .border_1()
            .border_color(if drag_over {
                drag_border
            } else if active {
                rgba((theme.accent << 8) | SFTP_ACTIVE_BORDER_ALPHA)
            } else {
                sftp_border(theme.border, has_background)
            })
            .bg(if drag_over {
                drag_bg
            } else {
                sftp_bg(theme.bg, has_background)
            })
            .drag_over::<gpui::ExternalPaths>(move |style, _paths, _window, _cx| {
                style.bg(drag_bg).border_color(drag_border)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    this.sftp_view().update(cx, |sftp, cx| {
                        if sftp.active_pane != pane {
                            sftp.active_pane = pane;
                            cx.notify();
                        }
                    });
                }),
            )
            .child(self.render_sftp_pane_header(
                pane,
                title,
                &path,
                path_editing,
                &path_input,
                focused_input,
                selected_count,
                transfer_direction,
                active,
                has_background,
                window,
                cx,
            ))
            .child(self.render_sftp_column_header(
                pane,
                sort_field,
                sort_direction,
                has_background,
                cx,
            ))
            .child(self.render_sftp_filter(pane, &filter, focused_input, has_background, cx))
            .child(self.render_sftp_file_list(pane, has_background, cx))
            .into_any_element()
    }

    fn render_sftp_pane_header(
        &self,
        pane: SftpPane,
        title: String,
        path: &str,
        path_editing: bool,
        path_input: &str,
        focused_input: Option<SftpInput>,
        selected_count: usize,
        transfer_direction: SftpTransferDirection,
        active: bool,
        has_background: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let theme = self.tokens.ui;
        let input = if pane == SftpPane::Local {
            SftpInput::LocalPath
        } else {
            SftpInput::RemotePath
        };
        let mut header = div()
            .h(px(ui.size(SFTP_PANE_HEADER_HEIGHT)))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SFTP_PANE_HEADER_GAP))
            .p(px(8.0))
            .border_b_1()
            .border_color(if active {
                rgba((theme.accent << 8) | SFTP_HEADER_ACTIVE_BORDER_ALPHA)
            } else {
                sftp_border(theme.border, has_background)
            })
            .bg(if active {
                rgba((theme.bg_hover << 8) | SFTP_HEADER_ACTIVE_BG_ALPHA)
            } else {
                sftp_panel_bg(theme.bg_panel, has_background, 0xff)
            })
            .child(
                div()
                    .min_w(px(SFTP_PANE_HEADER_TITLE_MIN_WIDTH))
                    .text_size(px(ui.text_xs))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectionRole::RowSafe,
                        "sftp-pane-title",
                        pane as u64,
                        title.to_uppercase(),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(self.render_sftp_path_bar(
                pane,
                input,
                path,
                path_input,
                path_editing,
                focused_input,
                has_background,
                window,
                cx,
            ))
            .child(self.render_sftp_icon_button(
                LucideIcon::Pencil,
                self.i18n.t("sftp.preview.edit"),
                cx.listener(move |this, _event, _window, cx| {
                    this.start_path_edit(pane, cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ));

        if pane == SftpPane::Local && self.sftp_pair_primary_remote_id(cx).is_none() {
            header = header
                .child(self.render_sftp_icon_button(
                    LucideIcon::HardDrive,
                    self.i18n.t("sftp.toolbar.show_drives"),
                    cx.listener(|this, _event, _window, cx| {
                        this.sftp_view().update(cx, |sftp, cx| {
                            sftp.drives_scroll = ScrollHandle::new();
                            sftp.set_dialog(SftpDialog::Drives);
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }),
                    cx.entity(),
                ))
                .child(self.render_sftp_icon_button(
                    LucideIcon::FolderOpen,
                    self.i18n.t("sftp.toolbar.browse_folder"),
                    cx.listener(|this, _event, _window, cx| {
                        this.send(SftpBrowserRequest::BrowseFolder, None, cx);
                        cx.stop_propagation();
                    }),
                    cx.entity(),
                ));
        }

        header = header
            .child(self.render_sftp_nav_button(
                pane,
                "..",
                LucideIcon::ArrowUp,
                "sftp.toolbar.go_up",
                cx,
            ))
            .child(self.render_sftp_nav_button(
                pane,
                "~",
                LucideIcon::Home,
                "sftp.toolbar.home",
                cx,
            ))
            .child(self.render_sftp_refresh_button(pane, cx));

        if pane == SftpPane::Remote
            && matches!(
                self.sftp_view().read(cx).current_remote_id,
                Some(SftpRemoteId::Node(_))
            )
        {
            header = header
                .child(self.render_sftp_terminal_directory_button(cx))
                .child(self.render_sftp_sync_terminal_directory_button(cx));
        }

        if pane == SftpPane::Remote {
            header = header.child(self.render_sftp_bookmarks_button(cx));
        }

        if selected_count > 0 {
            let label = match transfer_direction {
                SftpTransferDirection::Upload => self
                    .i18n
                    .t("sftp.toolbar.upload_count")
                    .replace("{{count}}", &selected_count.to_string()),
                SftpTransferDirection::Download => self
                    .i18n
                    .t("sftp.toolbar.download_count")
                    .replace("{{count}}", &selected_count.to_string()),
            };
            let icon = match transfer_direction {
                SftpTransferDirection::Upload => LucideIcon::Upload,
                SftpTransferDirection::Download => LucideIcon::Download,
            };
            header = header.child(self.render_sftp_transfer_button(
                pane,
                transfer_direction,
                icon,
                label,
                cx,
            ));
        }

        header.into_any_element()
    }

    fn render_sftp_path_bar(
        &self,
        pane: SftpPane,
        input: SftpInput,
        path: &str,
        path_input: &str,
        editing: bool,
        focused_input: Option<SftpInput>,
        has_background: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let theme = self.tokens.ui;
        let focused = focused_input == Some(input);
        let value = if editing { path_input } else { path };
        let path_bar = div()
            .flex_1()
            .min_w(px(0.0))
            .h(px(ui.size(24.0)))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .px(px(SFTP_PATH_BAR_HORIZONTAL_PADDING))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(if focused {
                rgb(theme.accent)
            } else {
                rgb(theme.border)
            })
            .bg(sftp_bg(theme.bg_sunken, has_background))
            .overflow_hidden()
            .cursor_pointer()
            .when(editing, |bar| {
                bar.child(self.render_sftp_inline_text(
                    input,
                    Some(pane),
                    value,
                    "sftp.file_list.path_placeholder",
                    focused,
                    cx,
                ))
                .child(
                    div()
                        .size(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.radii.sm))
                        .hover(move |button| button.bg(rgb(theme.bg_hover)))
                        .child(render_lucide_icon(
                            LucideIcon::CornerDownLeft,
                            ui.size(SFTP_ICON_SM),
                            rgb(theme.text),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.enter_path(pane, cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
            .when(!editing, |bar| {
                bar.child(self.render_sftp_breadcrumb(pane, path, window, cx))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.sftp_view().update(cx, |sftp, cx| {
                        let mut changed = false;
                        if sftp.active_pane != pane {
                            sftp.active_pane = pane;
                            changed = true;
                        }
                        if editing || event.click_count >= 2 {
                            match pane {
                                SftpPane::Local => {
                                    if !sftp.editing_local_path {
                                        sftp.editing_local_path = true;
                                        changed = true;
                                    }
                                    if sftp.local_path_input != sftp.local_path {
                                        sftp.local_path_input.clone_from(&sftp.local_path);
                                        changed = true;
                                    }
                                }
                                SftpPane::Remote => {
                                    if !sftp.editing_remote_path {
                                        sftp.editing_remote_path = true;
                                        changed = true;
                                    }
                                    if sftp.remote_path_input != sftp.remote_path {
                                        sftp.remote_path_input.clone_from(&sftp.remote_path);
                                        changed = true;
                                    }
                                }
                            }
                            changed |= sftp.set_input_focus(Some(input), cx);
                        }
                        if changed {
                            cx.notify();
                        }
                    });
                    cx.stop_propagation();
                }),
            );

        path_bar.into_any_element()
    }

    fn render_sftp_breadcrumb(
        &self,
        pane: SftpPane,
        path: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let theme = self.tokens.ui;
        let pane_is_remote = pane == SftpPane::Remote
            || (pane == SftpPane::Local && self.sftp_pair_primary_remote_id(cx).is_some());
        let segments = sftp_path_segments(path, pane_is_remote);
        let scroll_handle = match pane {
            SftpPane::Local => self.sftp_view().read(cx).local_path_scroll.clone(),
            SftpPane::Remote => self.sftp_view().read(cx).remote_path_scroll.clone(),
        };
        let mut inner = div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SFTP_BREADCRUMB_ROW_GAP));
        for (index, segment) in segments.iter().cloned().enumerate() {
            if index > 0 {
                inner = inner.child(render_lucide_icon(
                    LucideIcon::ChevronRight,
                    ui.size(SFTP_ICON_MD),
                    rgb(theme.text_muted),
                ));
            }
            let is_last = index + 1 == segments.len();
            let full_path = segment.full_path.clone();
            let selection_group_id = selectable_id(
                "sftp-breadcrumb-segment",
                (pane as u64, segment.full_path.as_str()),
            );
            let segment_text_color = if is_last {
                theme.text_heading
            } else {
                theme.text
            };
            inner = inner.child(
                div()
                    .max_w(px(120.0))
                    .h(px(ui.size(20.0)))
                    .px(px(SFTP_BREADCRUMB_SEGMENT_PADDING))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SFTP_BREADCRUMB_CONTENT_GAP))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(if is_last {
                        rgba((theme.bg_hover << 8) | SFTP_BREADCRUMB_ACTIVE_ALPHA)
                    } else {
                        rgba(theme.bg_hover << 8)
                    })
                    .hover(move |crumb| {
                        crumb.bg(rgba((theme.bg_hover << 8) | SFTP_BREADCRUMB_HOVER_ALPHA))
                    })
                    .text_color(if is_last {
                        rgb(theme.text_heading)
                    } else {
                        rgb(theme.text)
                    })
                    .when(index == 0, |item| {
                        item.child(render_lucide_icon(
                            match (pane, segment.root_is_drive) {
                                (SftpPane::Remote, _) => LucideIcon::Server,
                                (SftpPane::Local, true) => LucideIcon::HardDrive,
                                (SftpPane::Local, false) => LucideIcon::Home,
                            },
                            ui.size(SFTP_ICON_MD),
                            rgb(theme.text_muted),
                        ))
                    })
                    .child(div().truncate().child(
                        self.render_row_safe_selectable_display_text_in_group(
                            selection_group_id,
                            "sftp-breadcrumb-cell",
                            ("name", pane as u64, segment.full_path.as_str()),
                            0,
                            segment.name,
                            segment_text_color,
                            None,
                            cx,
                        ),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_path(pane, full_path.clone(), cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }

        div()
            .id(match pane {
                SftpPane::Local => "sftp-local-breadcrumb-scroll",
                SftpPane::Remote => "sftp-remote-breadcrumb-scroll",
            })
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .overflow_hidden()
            .track_scroll(&scroll_handle)
            .text_size(px(ui.text_sm))
            .on_scroll(cx.listener(move |this, event: &ScrollEvent, _window, cx| {
                this.handle_sftp_breadcrumb_scroll(pane, event, cx);
            }))
            .child(
                // Track the direct content row so GPUI measures the real overflow width.
                inner,
            )
            .into_any_element()
    }

    fn render_sftp_column_header(
        &self,
        pane: SftpPane,
        sort_field: SftpSortField,
        sort_direction: SftpSortDirection,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        div()
            .h(px(ui.size(25.0)))
            .flex()
            .flex_row()
            .items_center()
            .px(px(8.0))
            .py(px(4.0))
            .bg(sftp_panel_bg(self.tokens.ui.bg_panel, has_background, 0xff))
            .border_b_1()
            .border_color(sftp_border(self.tokens.ui.border, has_background))
            .text_size(px(ui.text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.render_sftp_sort_header(
                pane,
                SftpSortField::Name,
                sort_field,
                sort_direction,
                self.i18n.t("sftp.file_list.col_name"),
                None,
                cx,
            ))
            .child(self.render_sftp_sort_header(
                pane,
                SftpSortField::Size,
                sort_field,
                sort_direction,
                self.i18n.t("sftp.file_list.col_size"),
                Some(ui.size(SFTP_SIZE_COL)),
                cx,
            ))
            .child(self.render_sftp_sort_header(
                pane,
                SftpSortField::Modified,
                sort_field,
                sort_direction,
                self.i18n.t("sftp.file_list.col_modified"),
                Some(ui.size(SFTP_MODIFIED_COL)),
                cx,
            ))
            .into_any_element()
    }

    fn render_sftp_sort_header(
        &self,
        pane: SftpPane,
        field: SftpSortField,
        active_field: SftpSortField,
        direction: SftpSortDirection,
        label: String,
        width: Option<f32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let theme = self.tokens.ui;
        let field_key = match field {
            SftpSortField::Name => "name",
            SftpSortField::Size => "size",
            SftpSortField::Modified => "modified",
        };
        let selection_group_id = selectable_id("sftp-sort-header", (pane as u64, field_key));
        let header_text_color = if active_field == field {
            theme.accent
        } else {
            theme.text_muted
        };
        div()
            .when_some(width, |header, width| {
                header.w(px(width)).flex_none().justify_end()
            })
            .when(width.is_none(), |header| header.flex_1())
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .text_color(if active_field == field {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |header| header.text_color(rgb(theme.text)))
            .cursor_pointer()
            .child(
                div()
                    .truncate()
                    .child(self.render_row_safe_selectable_display_text_in_group(
                        selection_group_id,
                        "sftp-sort-header-cell",
                        field_key,
                        0,
                        label,
                        header_text_color,
                        None,
                        cx,
                    )),
            )
            .when(active_field == field, |header| {
                let icon = match (field, direction) {
                    (SftpSortField::Name, SftpSortDirection::Asc) => LucideIcon::ArrowUpAZ,
                    (SftpSortField::Name, SftpSortDirection::Desc) => LucideIcon::ArrowDownAZ,
                    _ => LucideIcon::ArrowUpDown,
                };
                header.child(render_lucide_icon(
                    icon,
                    ui.size(SFTP_ICON_SM),
                    rgb(theme.accent),
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.sftp_view().update(cx, |model, cx| {
                        model.toggle_sort(pane, field);
                        cx.notify();
                    });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_sftp_filter(
        &self,
        pane: SftpPane,
        filter: &str,
        focused_input: Option<SftpInput>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let input = if pane == SftpPane::Local {
            SftpInput::LocalFilter
        } else {
            SftpInput::RemoteFilter
        };
        let focused = focused_input == Some(input);
        let theme = self.tokens.ui;
        div()
            .h(px(ui.size(30.0)))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .py(px(4.0))
            .bg(sftp_panel_bg(
                theme.bg_panel,
                has_background,
                SFTP_PANEL_80_ALPHA,
            ))
            .border_b_1()
            .border_color(sftp_border(theme.border, has_background))
            .child(render_lucide_icon(
                LucideIcon::Search,
                ui.size(SFTP_ICON_SM),
                rgb(theme.text_muted),
            ))
            .child(self.render_sftp_inline_text(
                input,
                Some(pane),
                filter,
                "sftp.file_list.filter_placeholder",
                focused,
                cx,
            ))
            .when(!filter.is_empty(), |row| {
                row.child(
                    div()
                        .text_size(px(ui.text_xs))
                        .text_color(rgb(theme.text_muted))
                        .hover(move |x| x.text_color(rgb(theme.text)))
                        .cursor_pointer()
                        .child("×")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.sftp_view().update(cx, |sftp, cx| {
                                    sftp.input_value_mut(input).clear();
                                    cx.notify();
                                });
                                cx.stop_propagation();
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.sftp_view().update(cx, |sftp, cx| {
                        let mut changed = false;
                        if sftp.active_pane != pane {
                            sftp.active_pane = pane;
                            changed = true;
                        }
                        changed |= sftp.set_input_focus(Some(input), cx);
                        if changed {
                            cx.notify();
                        }
                    });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}
