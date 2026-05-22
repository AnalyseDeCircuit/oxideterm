impl WorkspaceApp {
    fn render_sftp_init_error(
        &self,
        error: &str,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(self.tokens.ui.text))
            .child(self.render_selectable_text_scoped(
                "sftp-init-error",
                (),
                format!("SFTP waiting for connection sync: {error}"),
                self.tokens.ui.text,
                cx,
            ))
            .child(self.render_sftp_text_button(
                "Retry".to_string(),
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.sftp_view.init_error = None;
                    if let Some(tab_id) = this.active_tab_id
                        && let Some(node_id) = this.sftp_tab_nodes.get(&tab_id).cloned()
                    {
                        // Retry mirrors Tauri node_sftp_list_dir: it retries
                        // through the node owner, so a tab with no terminal
                        // pane can rebuild the SSH/SFTP path first.
                        this.ensure_node_connection_started(&node_id);
                        this.sftp_view.remote_load_pending = true;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .into_any_element()
    }

    fn render_sftp_icon_button(
        &self,
        icon: LucideIcon,
        title: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        workspace: gpui::Entity<Self>,
    ) -> AnyElement {
        let title_for_move = title.clone();
        let title_element_id = title.clone();
        let title_request_id = title.clone();
        let tooltip_workspace = workspace.clone();
        let clear_workspace = workspace;
        icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, SFTP_ICON_SM, rgb(self.tokens.ui.text)),
            // Tauri SFTP toolbar buttons render normal opacity; disabled states
            // are handled by callers that omit the action.
            IconButtonOptions::opaque_toolbar(SFTP_TOOL_BUTTON, ButtonRadius::Md),
        )
            .id((gpui::ElementId::from("sftp-icon-button"), title_element_id))
            .flex_none()
            .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                let _ = tooltip_workspace.update(cx, |this, cx| {
                    this.queue_workspace_tooltip(
                        title_request_id.clone(),
                        title_for_move.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                });
            })
            .on_hover(move |hovered: &bool, _window, cx| {
                if !*hovered {
                    let _ = clear_workspace.update(cx, |this, cx| {
                        this.clear_workspace_tooltip(&title, cx);
                    });
                }
            })
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_sftp_nav_button(
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
                this.navigate_sftp_path(pane, target);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        )
    }

    fn render_sftp_refresh_button(&self, pane: SftpPane, cx: &mut Context<Self>) -> AnyElement {
        self.render_sftp_icon_button(
            LucideIcon::RefreshCw,
            self.i18n.t("sftp.toolbar.refresh"),
            cx.listener(move |this, _event, _window, cx| {
                if pane == SftpPane::Remote {
                    this.sftp_view.remote_load_pending = true;
                    this.sftp_view.remote_loading = true;
                }
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        )
    }

    fn render_sftp_transfer_button(
        &self,
        pane: SftpPane,
        direction: SftpTransferDirection,
        icon: LucideIcon,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        toolbar_button(
            &self.tokens,
            label,
            Some(Self::render_lucide_icon(
                icon,
                SFTP_ICON_SM,
                rgb(theme.text),
            )),
            ToolbarButtonOptions {
                icon_gap: Some(4.0),
                text_color: Some(rgb(theme.text)),
                hover_background: Some(rgb(theme.bg_hover)),
                ..ToolbarButtonOptions::compact_text(
                    ButtonVariant::Ghost,
                    ButtonRadius::Sm,
                    24.0,
                    8.0,
                    SFTP_TEXT_XS,
                )
            },
        )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.queue_sftp_transfers(pane, direction);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_sftp_text_button(
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

    fn render_sftp_button_variant(
        &self,
        label: String,
        variant: SftpButtonVariant,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Mirrors the Tauri Button variants used by SFTP dialogs:
        // default = bg-theme-text, secondary = bg-theme-bg-panel, ghost = no border.
        let (bg, border, text, hover_bg, hover_opacity) = match variant {
            SftpButtonVariant::Default => (
                rgb(theme.text),
                rgba((theme.text << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
                rgb(theme.bg),
                rgb(theme.text),
                Some(0.9),
            ),
            SftpButtonVariant::Secondary => (
                rgb(theme.bg_panel),
                rgb(theme.border),
                rgb(theme.text),
                rgb(theme.bg_hover),
                None,
            ),
            SftpButtonVariant::Ghost => (
                rgba((theme.bg << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
                rgba((theme.border << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
                rgb(theme.text),
                rgb(theme.bg_hover),
                None,
            ),
        };
        toolbar_button(
            &self.tokens,
            label,
            None,
            ToolbarButtonOptions {
                background: Some(bg),
                border: Some(border),
                text_color: Some(text),
                hover_background: Some(hover_bg),
                hover_opacity,
                ..ToolbarButtonOptions::compact_text(
                    ButtonVariant::Ghost,
                    ButtonRadius::Md,
                    32.0,
                    12.0,
                    SFTP_TEXT_XS,
                )
            },
        )
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn queue_title(&self, active_count: usize) -> String {
        let mut title = self.i18n.t("sftp.queue.title").to_uppercase();
        if active_count > 0 {
            title.push(' ');
            title.push_str(
                &self
                    .i18n
                    .t("sftp.queue.active_count")
                    .replace("{{count}}", &active_count.to_string()),
            );
        }
        title
    }

    fn transfer_status_text(&self, transfer: &SftpTransferItem) -> String {
        match transfer.state {
            SftpTransferState::Pending => self.i18n.t("sftp.queue.status_waiting"),
            SftpTransferState::Active => format_transfer_speed(transfer.speed),
            SftpTransferState::Paused => self.i18n.t("sftp.queue.status_paused"),
            SftpTransferState::Completed => self.i18n.t("sftp.queue.status_completed"),
            SftpTransferState::Cancelled => self.i18n.t("sftp.queue.status_cancelled"),
            SftpTransferState::Error => transfer
                .error
                .clone()
                .unwrap_or_else(|| self.i18n.t("sftp.queue.status_error")),
        }
    }
}
