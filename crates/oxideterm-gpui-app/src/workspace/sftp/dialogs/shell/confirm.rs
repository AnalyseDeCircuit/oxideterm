use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn render_sftp_editor_close_confirm_dialog(
        &self,
        name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let dialog_visible =
            self.sftp_view.dialog_presence.phase() == oxideterm_gpui_ui::motion::ExitPhase::Visible;
        let backdrop_name = name.clone();
        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    // Tauri IDE/SFTP save-confirm dialogs close through
                    // onOpenChange(false) -> cancel, never discard.
                    this.cancel_sftp_editor_close_confirm(backdrop_name.clone());
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::form_transition(
                &self.tokens,
                "sftp-editor-close-confirm-presence",
                div()
                    .w(px(SFTP_DIALOG_WIDTH_SM))
                    .max_w(relative(0.9))
                    .overflow_hidden()
                    .rounded(px(self.tokens.radii.lg))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | SFTP_DIALOG_BORDER_SUBTLE_ALPHA))
                    .bg(rgb(theme.bg_elevated))
                    .shadow(vec![gpui::BoxShadow {
                        color: gpui::Hsla::from(rgba(SFTP_DIALOG_SHADOW_ALPHA)),
                        offset: gpui::point(px(0.0), px(16.0)),
                        blur_radius: px(32.0),
                        spread_radius: px(0.0),
                    }])
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(24.0))
                            .pt(px(24.0))
                            .pb(px(16.0))
                            .child(
                                div()
                                    .w(px(48.0))
                                    .h(px(48.0))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .border_1()
                                    .border_color(rgba(
                                        (theme.accent << 8) | SFTP_CONFIRM_ICON_RING_ALPHA,
                                    ))
                                    .bg(rgba((theme.accent << 8) | SFTP_CONFIRM_ICON_BG_ALPHA))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::HelpCircle,
                                        24.0,
                                        rgb(theme.accent),
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text))
                                    .text_center()
                                    .line_height(px(20.0))
                                    .child(self.i18n.t("sftp.preview.unsaved_changes_confirm")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .border_t_1()
                            .border_color(rgba((theme.border << 8) | SFTP_DIALOG_DIVIDER_ALPHA))
                            .child(
                                div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .border_r_1()
                                    .border_color(rgba(
                                        (theme.border << 8) | SFTP_DIALOG_DIVIDER_ALPHA,
                                    ))
                                    .text_center()
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text_muted))
                                    .hover(move |button| {
                                        button.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text))
                                    })
                                    .cursor_pointer()
                                    .child(self.i18n.t("sftp.dialogs.cancel"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _event, _window, cx| {
                                            this.cancel_sftp_editor_close_confirm(name.clone());
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .py(px(10.0))
                                    .text_center()
                                    .text_size(px(SFTP_TEXT_SM))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.accent))
                                    .hover(move |button| {
                                        button.bg(rgba(
                                            (theme.accent << 8) | SFTP_CONFIRM_ACTION_HOVER_ALPHA,
                                        ))
                                    })
                                    .cursor_pointer()
                                    .child(self.i18n.t("sftp.preview.confirm"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.discard_sftp_editor_changes();
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    ),
                            ),
                    ),
                dialog_visible,
            ))
            .when(!dialog_visible, |backdrop| {
                backdrop.child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .left_0()
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, |_event, _window, cx| {
                            cx.stop_propagation();
                        })
                        .on_scroll_wheel(|_event, _window, cx| cx.stop_propagation()),
                )
            })
            .into_any_element()
    }
}
