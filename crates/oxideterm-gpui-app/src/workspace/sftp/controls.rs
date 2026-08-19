use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn render_sftp_init_error(
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
            .child(
                self.render_selectable_text_scoped(
                    "sftp-init-error",
                    (),
                    self.i18n
                        .t("sftp.errors.connection_sync")
                        .replace("{{error}}", error),
                    self.tokens.ui.text,
                    cx,
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.render_sftp_text_button(
                        self.i18n.t("sftp.scp.download_path"),
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.queue_quick_scp_download(cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
                    .child(self.render_sftp_text_button(
                        self.i18n.t("sftp.preview.retry"),
                        false,
                        cx.listener(|this, _event, _window, cx| {
                            this.sftp_view.update(cx, |sftp, cx| {
                                sftp.init_error = None;
                                cx.notify();
                            });
                            if let Some(node_id) = this.visible_sftp_node_id(cx) {
                                // Retry asks the node owner to rebuild SFTP; SCP remains an
                                // explicit compatibility action beside this control.
                                this.ensure_node_connection_started(&node_id, cx);
                                this.request_sftp_remote_load(cx);
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )),
            )
            .into_any_element()
    }

    pub(in crate::workspace::sftp) fn render_sftp_icon_button(
        &self,
        icon: LucideIcon,
        title: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        workspace: gpui::Entity<Self>,
    ) -> AnyElement {
        self.workspace_tooltip_icon_button(
            icon,
            SFTP_ICON_SM,
            rgb(self.tokens.ui.text),
            IconButtonOptions::opaque_toolbar(SFTP_TOOL_BUTTON, ButtonRadius::Md),
            title,
            "sftp-icon-button",
            true,
            listener,
            workspace,
        )
    }

    pub(in crate::workspace::sftp) fn render_sftp_nav_button(
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
                this.navigate_sftp_path(pane, target, cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        )
    }

    pub(in crate::workspace::sftp) fn render_sftp_refresh_button(
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
                            this.request_sftp_pair_primary_load(cx);
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        // Local refresh only re-reads the visible directory and
                        // must not disturb the node-owned remote SFTP session.
                        let path = this.sftp_view.read(cx).local_path.clone();
                        let files = refreshed_local_files(&path);
                        this.sftp_view.update(cx, |sftp, cx| {
                            sftp.local_files = files;
                            cx.notify();
                        });
                    }
                    SftpPane::Remote => this.request_sftp_remote_load(cx),
                }
                cx.stop_propagation();
                cx.notify();
            }),
            cx.entity(),
        )
    }

    pub(in crate::workspace::sftp) fn render_sftp_transfer_button(
        &self,
        pane: SftpPane,
        direction: SftpTransferDirection,
        icon: LucideIcon,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.workspace_toolbar_action_button(
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
            cx.listener(move |this, _event, _window, cx| {
                this.queue_sftp_transfers(pane, direction, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .into_any_element()
    }

    pub(in crate::workspace::sftp) fn render_sftp_text_button(
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

    pub(in crate::workspace::sftp) fn render_sftp_button_variant(
        &self,
        label: String,
        variant: SftpButtonVariant,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Mirrors the Tauri Button variants used by SFTP dialogs and keeps
        // irreversible conflict actions visually distinct from the safe default.
        let (bg, border, text, hover_bg, hover_opacity) = match variant {
            SftpButtonVariant::Default => (
                rgb(theme.text),
                rgba((theme.text << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
                rgb(theme.bg),
                rgb(theme.text),
                Some(0.9),
            ),
            SftpButtonVariant::Destructive => (
                rgba((theme.error << 8) | SFTP_DESTRUCTIVE_BG_ALPHA),
                rgba((theme.error << 8) | SFTP_DESTRUCTIVE_BORDER_ALPHA),
                rgb(SFTP_DESTRUCTIVE_TEXT),
                rgb(theme.error),
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
        self.workspace_toolbar_action_button(
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
            listener,
        )
        .into_any_element()
    }

    pub(in crate::workspace::sftp) fn queue_title(&self, active_count: usize) -> String {
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
}
