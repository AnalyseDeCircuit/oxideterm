use super::*;

impl SftpBrowserView {
    pub(super) fn render_sftp_init_error(
        &self,
        error: &SftpViewError,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let (label, message) = match error {
            SftpViewError::Operation(message) => ("sftp.errors.connection_sync", message),
            SftpViewError::Directory(message) => ("sftp.errors.directory_load_failed", message),
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((SFTP_YELLOW << 8) | 0x66))
            .bg(rgba((SFTP_YELLOW << 8) | 0x1a))
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(ui.text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .child(self.render_selectable_text_scoped(
                "sftp-init-error",
                (),
                self.i18n.t(label).replace("{{error}}", message),
                self.tokens.ui.text,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.render_sftp_text_button(
                        self.i18n.t("sftp.scp.download_path"),
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.send(SftpBrowserRequest::QuickDownload, None, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
                    .child(self.render_sftp_text_button(
                        self.i18n.t("sftp.preview.retry"),
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.sftp_view().update(cx, |sftp, cx| {
                                sftp.init_error = None;
                                cx.notify();
                            });
                            if let Some(node_id) = this
                                .sftp_view()
                                .read(cx)
                                .current_remote_id
                                .as_ref()
                                .and_then(SftpRemoteId::node_id)
                                .cloned()
                            {
                                // Retry asks the node owner to rebuild SFTP; SCP remains an
                                // explicit compatibility action beside this control.
                                this.send(
                                    SftpBrowserRequest::EnsureConnection(node_id.clone()),
                                    None,
                                    cx,
                                );
                            }
                            this.send(SftpBrowserRequest::LoadRemote, None, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_sftp_icon_button(
        &self,
        icon: LucideIcon,
        title: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        workspace: gpui::Entity<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        self.workspace_tooltip_icon_button(
            icon,
            ui.size(SFTP_ICON_SM),
            rgb(self.tokens.ui.text),
            IconButtonOptions::opaque_toolbar(ui.size(SFTP_TOOL_BUTTON), ButtonRadius::Md),
            title,
            "sftp-icon-button",
            true,
            listener,
            workspace,
        )
    }

    pub(super) fn render_sftp_nav_button(
        &self,
        pane: SftpPane,
        target: &'static str,
        icon: LucideIcon,
        label_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_sftp_icon_button(
            icon,
            self.i18n.t(label_key),
            cx.listener(move |this, _event, _window, cx| {
                this.navigate_path(pane, target, cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_sftp_terminal_directory_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let path = self.terminal_directory.clone();
        let mut title = self.i18n.t("sftp.toolbar.reveal_terminal_directory");
        if let Some(combo) = self.reveal_terminal_key.as_deref() {
            title.push_str(&format!(" · {combo}"));
        }
        title.push('\n');
        title.push_str(
            path.as_deref()
                .unwrap_or(&self.i18n.t("sftp.toolbar.terminal_directory_unavailable")),
        );
        self.workspace_tooltip_icon_button(
            LucideIcon::FolderInput,
            ui.size(SFTP_ICON_SM),
            rgb(self.tokens.ui.text),
            IconButtonOptions {
                disabled: path.is_none(),
                ..IconButtonOptions::opaque_toolbar(ui.size(SFTP_TOOL_BUTTON), ButtonRadius::Md)
            },
            title,
            "sftp-terminal-directory",
            true,
            cx.listener(|this, _event, window, cx| {
                this.send(SftpBrowserRequest::RevealTerminalDirectory, None, cx);
                window.focus(&this.focus_handle, cx);
                cx.stop_propagation();
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_sftp_sync_terminal_directory_button(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let path = self.sftp_terminal_sync_target_path(cx);
        let enabled = self.can_sync_terminal && !path.is_empty();
        let mut title = self.i18n.t("sftp.toolbar.sync_directory_to_terminal");
        title.push('\n');
        if path.is_empty() {
            title.push_str(&self.i18n.t("sftp.toolbar.terminal_directory_unavailable"));
        } else {
            title.push_str(&path);
        }
        self.workspace_tooltip_icon_button(
            LucideIcon::FolderSync,
            ui.size(SFTP_ICON_SM),
            rgb(self.tokens.ui.text),
            IconButtonOptions {
                disabled: !enabled,
                ..IconButtonOptions::opaque_toolbar(ui.size(SFTP_TOOL_BUTTON), ButtonRadius::Md)
            },
            title,
            "sftp-sync-terminal-directory",
            true,
            cx.listener(|this, _event, window, cx| {
                // The sync action focuses the terminal pane itself; focusing the
                // workspace root here would steal that focus back.
                let path = this.sftp_terminal_sync_target_path(cx);
                this.send(
                    SftpBrowserRequest::SyncTerminalDirectory(path),
                    Some(window),
                    cx,
                );
                cx.stop_propagation();
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_sftp_bookmarks_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        // A remote-to-remote pair has no local pane; the saved pair would be
        // ambiguous, so the entry point is disabled with an explanatory tooltip.
        let pair_mode = self.sftp_pair_primary_remote_id(cx).is_some();
        let has_remote = self.sftp_view().read(cx).current_remote_id.is_some();
        let title = if pair_mode {
            self.i18n.t("sftp.bookmarks.unavailable_pair")
        } else {
            self.i18n.t("sftp.bookmarks.title")
        };
        self.workspace_tooltip_icon_button(
            LucideIcon::Star,
            ui.size(SFTP_ICON_SM),
            rgb(self.tokens.ui.text),
            IconButtonOptions {
                disabled: pair_mode || !has_remote,
                ..IconButtonOptions::opaque_toolbar(ui.size(SFTP_TOOL_BUTTON), ButtonRadius::Md)
            },
            title,
            "sftp-bookmarks",
            true,
            cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                this.bookmarks
                    .update(cx, |bookmarks, cx| bookmarks.load(cx));
                let open = this.sftp_view().read(cx).bookmark_menu.is_some();
                let next = (!open).then(|| SftpBookmarkMenu {
                    x: f32::from(event.position.x),
                    y: f32::from(event.position.y),
                });
                this.sftp_view().update(cx, |sftp, cx| {
                    sftp.dismiss_context_menu(cx);
                    sftp.bookmark_menu = next;
                    cx.notify();
                });
                cx.stop_propagation();
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_sftp_tree_mode_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let tree_mode = self.tree_enabled;
        let (icon, label_key) = if tree_mode {
            (LucideIcon::LayoutList, "sftp.toolbar.flat_view")
        } else {
            (LucideIcon::ListTree, "sftp.toolbar.tree_view")
        };
        self.workspace_tooltip_icon_button(
            icon,
            ui.size(SFTP_ICON_SM),
            rgb(self.tokens.ui.text),
            IconButtonOptions::opaque_toolbar(ui.size(SFTP_TOOL_BUTTON), ButtonRadius::Md),
            self.i18n.t(label_key),
            "sftp-tree-mode",
            true,
            cx.listener(|this, _event, _window, cx| {
                // Switching presentation invalidates expanded children; the
                // workspace will rebuild the tree from the current root listing.
                this.sftp_view().update(cx, |sftp, cx| {
                    sftp.clear_sftp_tree_state();
                    // Header transfer actions must not keep operating on a
                    // selection the user made in the other presentation.
                    sftp.remote_selected.clear();
                    sftp.remote_last_selected = None;
                    cx.notify();
                });
                this.send(SftpBrowserRequest::ToggleTreeMode, None, cx);
                cx.stop_propagation();
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_sftp_refresh_button(
        &self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_sftp_icon_button(
            LucideIcon::RefreshCw,
            self.i18n.t("sftp.toolbar.refresh"),
            cx.listener(move |this, _event, _window, cx| {
                match pane {
                    SftpPane::Local => {
                        if this.sftp_pair_primary_remote_id(cx).is_some() {
                            this.send(SftpBrowserRequest::LoadPairPrimary, None, cx);
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        // Local refresh only re-reads the visible directory and
                        // must not disturb the node-owned remote SFTP session.
                        this.sftp_view().update(cx, |sftp, cx| {
                            let result = BrowseAudit::capture()
                                .run(sftp.local_path.clone(), |path| sftp.local_host.list(&path));
                            sftp.local_files = local_files_or_error(&sftp.local_path, result);
                            cx.notify();
                        });
                    }
                    SftpPane::Remote => {
                        this.sftp_view().update(cx, |sftp, cx| {
                            sftp.clear_sftp_tree_state();
                            cx.notify();
                        });
                        this.send(SftpBrowserRequest::LoadRemote, None, cx);
                    }
                }
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        )
    }

    pub(super) fn render_sftp_transfer_button(
        &self,
        pane: SftpPane,
        direction: SftpTransferDirection,
        icon: LucideIcon,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let theme = self.tokens.ui;
        self.workspace_toolbar_action_button(
            label,
            Some(render_lucide_icon(
                icon,
                ui.size(SFTP_ICON_SM),
                rgb(theme.text),
            )),
            ToolbarButtonOptions {
                icon_gap: Some(4.0),
                text_color: Some(rgb(theme.text)),
                hover_background: Some(rgb(theme.bg_hover)),
                ..ToolbarButtonOptions::compact_text(
                    ButtonVariant::Ghost,
                    ButtonRadius::Sm,
                    ui.size(24.0),
                    8.0,
                    ui.text_xs,
                )
            },
            cx.listener(move |this, _event, _window, cx| {
                this.send(SftpBrowserRequest::Transfer(pane, direction), None, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(super) fn render_sftp_text_button(
        &self,
        label: String,
        primary: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let variant = if primary {
            SftpButtonVariant::Default
        } else {
            SftpButtonVariant::Secondary
        };
        self.render_sftp_button_variant(label, variant, listener)
    }

    pub(super) fn render_sftp_button_variant(
        &self,
        label: String,
        variant: SftpButtonVariant,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        crate::style::render_sftp_button_variant(&self.tokens, label, variant, listener)
    }
}
