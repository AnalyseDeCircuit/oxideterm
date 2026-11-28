use super::*;
impl ExpandedView {
    fn terminal_command_action_button(
        &self,
        icon: LucideIcon,
        color: Rgba,
        disabled: bool,
        background: Option<Rgba>,
        id: &'static str,
        title: String,
        click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use oxideterm_gpui_ui::button::{IconButtonOptions, icon_button_with_tooltip};
        icon_button_with_tooltip(
            &self.frame.tokens,
            icon.path(),
            14.,
            color,
            IconButtonOptions {
                disabled,
                background,
                hover_background: Some(rgb(self.frame.tokens.ui.bg_hover)),
                ..IconButtonOptions::opaque_toolbar(24., ButtonRadius::Md)
            },
            title,
            id,
            false,
            cx.listener(click),
        )
    }
    pub(super) fn render_terminal_command_sender_tabs(
        &self,
        documents: &[TerminalCommandSenderDocumentSnapshot],
        active_id: TerminalCommandSenderId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.frame.tokens.ui;
        let mut tab_row = div().h_full().flex_none().flex().flex_row().items_stretch();
        for (index, document) in documents.iter().enumerate() {
            let sender_id = document.id;
            let active = sender_id == active_id;
            let label = format!("{} {}", self.i18n.t("terminal.sender.task"), sender_id.0);
            let status_icon = match document.status {
                TerminalCommandSenderStatus::Running => LucideIcon::LoaderCircle,
                TerminalCommandSenderStatus::Completed => LucideIcon::Check,
                TerminalCommandSenderStatus::Stopped => LucideIcon::Square,
                TerminalCommandSenderStatus::Failed => LucideIcon::AlertCircle,
                TerminalCommandSenderStatus::Idle => LucideIcon::FileText,
            };
            let icon_color = if active {
                rgb(theme.accent)
            } else {
                rgb(theme.text_muted)
            };
            let text_color = if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            };
            let left_padding = if index == 0 {
                TERMINAL_SENDER_PANEL_PADDING
            } else {
                10.0
            };
            tab_row = tab_row.child(
                div()
                    .debug_selector(move || format!("sender-task-{}", sender_id.0))
                    .flex_none()
                    .h_full()
                    .max_w(px(180.0))
                    .pl(px(left_padding))
                    .pr(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .border_b_2()
                    .border_color(if active {
                        rgb(theme.accent)
                    } else {
                        rgba(0x00000000)
                    })
                    .bg(rgba(0x00000000))
                    .cursor_pointer()
                    .text_size(px(self.frame.tokens.metrics.ui_text_xs))
                    .font_weight(if active {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .text_color(text_color)
                    .hover(move |tab| tab.bg(rgb(theme.bg_hover)).text_color(rgb(theme.text)))
                    .child(Self::render_lucide_icon(status_icon, 12.0, icon_color))
                    .child(div().truncate().child(label))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.terminal_command_sender.update(cx, |sender, cx| {
                                sender.set_active_document(sender_id, cx);
                            });
                            (this.dispatch)(SenderAction::FocusEditor(sender_id), window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        // Keep the add-task action right beside the final tab instead of isolating it on the far right.
        tab_row = tab_row.child(div().flex_none().pl(px(4.0)).flex().items_center().child(
            self.terminal_command_action_button(
                LucideIcon::Plus,
                rgb(theme.text_muted),
                false,
                None,
                "terminal-command-sender-add",
                self.i18n.t("terminal.sender.add_task"),
                |this, _event, window, cx| {
                    let sender_id = this
                        .terminal_command_sender
                        .update(cx, |sender, cx| sender.add_document(cx));
                    (this.dispatch)(SenderAction::FocusEditor(sender_id), window, cx);
                    cx.stop_propagation();
                },
                cx,
            ),
        ));
        // Scrollable moves the viewport's own style onto an outer wrapper.
        // Keep the tab flex row as a child so its horizontal layout survives
        // that transfer and additional tasks cannot stack behind the editor.
        let tab_list = div()
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .overflow_x_scrollbar()
            .child(tab_row);

        let task_actions = div()
            .flex_none()
            .pr(px(TERMINAL_SENDER_PANEL_PADDING))
            .flex()
            .items_center()
            .gap(px(2.0))
            .when(documents.len() > 1, |actions| {
                actions.child(self.terminal_command_action_button(
                    LucideIcon::X,
                    rgb(theme.text_muted),
                    false,
                    None,
                    "terminal-command-sender-remove",
                    self.i18n.t("terminal.sender.remove_task"),
                    move |this, _event, window, cx| {
                        this.terminal_command_sender.update(cx, |sender, cx| {
                            sender.remove_document(active_id, cx);
                        });
                        let next_id = this.terminal_command_sender.read(cx).active_document_id();
                        (this.dispatch)(SenderAction::FocusEditor(next_id), window, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
            })
            .child(self.terminal_command_action_button(
                LucideIcon::ChevronDown,
                rgb(theme.text_muted),
                false,
                None,
                "terminal-command-sender-collapse",
                self.i18n.t("terminal.sender.collapse"),
                |this, _event, window, cx| {
                    this.terminal_command_sender
                        .update(cx, |sender, cx| sender.toggle_expanded(cx));
                    (this.dispatch)(SenderAction::FocusCompact, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ));

        div()
            .h(px(32.0))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(self.frame.divider)
            .when(!self.frame.has_background, |strip| strip.bg(rgb(theme.bg)))
            .child(tab_list)
            .child(task_actions)
            .into_any_element()
    }
}
