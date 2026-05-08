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
            .child(format!("SFTP waiting for connection sync: {error}"))
            .child(self.render_sftp_text_button(
                "Retry".to_string(),
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.sftp_view.init_error = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ))
            .into_any_element()
    }

    fn render_sftp_icon_button(
        &self,
        icon: LucideIcon,
        _title: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size(px(SFTP_TOOL_BUTTON))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .text_color(rgb(theme.text))
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                icon,
                SFTP_ICON_SM,
                rgb(theme.text),
            ))
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
        div()
            .h(px(24.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(SFTP_TEXT_XS))
            .text_color(rgb(theme.text))
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                icon,
                SFTP_ICON_SM,
                rgb(theme.text),
            ))
            .child(label)
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
        let (bg, border, text) = match variant {
            SftpButtonVariant::Default => (
                rgb(theme.text),
                rgba((theme.text << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
                rgb(theme.bg),
            ),
            SftpButtonVariant::Secondary => {
                (rgb(theme.bg_panel), rgb(theme.border), rgb(theme.text))
            }
            SftpButtonVariant::Ghost => (
                rgba((theme.bg << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
                rgba((theme.border << 8) | SFTP_BUTTON_TRANSPARENT_ALPHA),
                rgb(theme.text),
            ),
        };
        div()
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_color(text)
            .text_size(px(SFTP_TEXT_XS))
            .font_weight(gpui::FontWeight::MEDIUM)
            .hover(move |button| {
                match variant {
                    SftpButtonVariant::Default => button.opacity(0.9),
                    SftpButtonVariant::Secondary | SftpButtonVariant::Ghost => {
                        button.bg(rgb(theme.bg_hover))
                    }
                }
            })
            .cursor_pointer()
            .child(label)
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
