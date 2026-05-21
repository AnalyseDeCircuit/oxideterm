const AI_CONVERSATION_ROW_HEIGHT: f32 = 46.0; // Tauri ConversationItem: px-3 py-1.5, title + mono meta.
const AI_CONVERSATION_EMPTY_HEIGHT: f32 = 52.0; // Tauri empty row p-4 text-center.
const AI_CONVERSATION_MAX_HEIGHT: f32 = 256.0; // Tauri max-h-64.
const AI_CHAT_PANEL_HEADER_HEIGHT: f32 = 36.0; // Tauri AiChatPanel min-h-[36px].
const AI_TOP_FLOATING_INSET_X: f32 = 8.0; // Tauri left-2/right-2 and right-0 within the chat panel.
const AI_FLOATING_GAP: f32 = 4.0; // Tauri mt-0.5/mb-1 style popup gap.
const AI_CHAT_MENU_WIDTH: f32 = 160.0; // Tauri w-40.
const AI_MODEL_SELECTOR_DROPDOWN_WIDTH: f32 = 256.0; // Tauri w-64.
const AI_PROFILE_DROPDOWN_WIDTH: f32 = 220.0; // Native profile dropdown width.
const AI_CONTEXT_POPOVER_WIDTH: f32 = 280.0; // Tauri-sized compact context popover.

impl WorkspaceApp {
    pub(in crate::workspace) fn update_ai_sidebar_overlay_for_window_bounds(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let next_size = current_window_size(window);
        let Some(previous_size) = self.ai_overlay_window_size.replace(next_size) else {
            return;
        };
        let dx = next_size.0 - previous_size.0;
        let dy = next_size.1 - previous_size.1;
        if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
            return;
        }
        if !self.has_ai_sidebar_floating_overlay() {
            return;
        }

        self.shift_ai_sidebar_overlay_anchors(dx, dy);
        cx.notify();
    }

    fn shift_ai_sidebar_overlay_anchors(&mut self, dx: f32, dy: f32) {
        for (id, anchor) in &mut self.select_anchors {
            match id {
                SelectAnchorId::AiPanelRoot => {
                    anchor.bounds.origin.x = anchor.bounds.origin.x + px(dx);
                    anchor.bounds.size.height = anchor.bounds.size.height + px(dy);
                }
                SelectAnchorId::AiConversationList | SelectAnchorId::AiChatMenu => {
                    anchor.bounds.origin.x = anchor.bounds.origin.x + px(dx);
                }
                SelectAnchorId::AiModelSelector
                | SelectAnchorId::AiProfileSelector
                | SelectAnchorId::AiSafetyMenu
                | SelectAnchorId::AiContextPopover => {
                    anchor.bounds.origin.x = anchor.bounds.origin.x + px(dx);
                    anchor.bounds.origin.y = anchor.bounds.origin.y + px(dy);
                }
                _ => {}
            }
        }
    }

    pub(in crate::workspace) fn render_ai_sidebar_floating_overlay(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.ai_sidebar_visible() || !self.has_ai_sidebar_floating_overlay() {
            return None;
        }

        let panel_anchor = self.select_anchors.get(&SelectAnchorId::AiPanelRoot).copied()?;
        let panel_left = f32::from(panel_anchor.bounds.left());
        let panel_right = f32::from(panel_anchor.bounds.right());
        let panel_width = f32::from(panel_anchor.bounds.size.width);

        let (corner, anchor_x, anchor_y, popup) = if self.ai_conversation_list_open {
            let top = self
                .select_anchors
                .get(&SelectAnchorId::AiConversationList)
                .map(|anchor| f32::from(anchor.bounds.bottom()) + AI_FLOATING_GAP)
                .unwrap_or_else(|| {
                    f32::from(panel_anchor.bounds.top()) + AI_CHAT_PANEL_HEADER_HEIGHT
                });
            let dropdown_width = (panel_width - AI_TOP_FLOATING_INSET_X * 2.0).max(0.0);
            (
                Corner::TopLeft,
                panel_left + AI_TOP_FLOATING_INSET_X,
                top,
                self.render_ai_conversation_dropdown(dropdown_width, cx),
            )
        } else if self.ai_chat_menu_open {
            let anchor = self.select_anchors.get(&SelectAnchorId::AiChatMenu).copied()?;
            let left = ai_sidebar_popup_left(
                f32::from(anchor.bounds.right()) - AI_CHAT_MENU_WIDTH,
                AI_CHAT_MENU_WIDTH,
                panel_left,
                panel_right,
            );
            let top = f32::from(anchor.bounds.bottom()) + AI_FLOATING_GAP / 2.0;
            (Corner::TopLeft, left, top, self.render_ai_chat_menu(cx))
        } else if self.ai_model_selector_open {
            let anchor = self.select_anchors.get(&SelectAnchorId::AiModelSelector)?;
            (
                Corner::BottomLeft,
                ai_sidebar_popup_left(
                    f32::from(anchor.bounds.left()),
                    AI_MODEL_SELECTOR_DROPDOWN_WIDTH,
                    panel_left,
                    panel_right,
                ),
                f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
                self.render_ai_model_selector_dropdown(
                    &ai_provider_views(&self.settings_store.settings().ai.providers),
                    cx,
                ),
            )
        } else if self.ai_profile_selector_open {
            let anchor = self.select_anchors.get(&SelectAnchorId::AiProfileSelector)?;
            (
                Corner::BottomLeft,
                ai_sidebar_popup_left(
                    f32::from(anchor.bounds.left()),
                    AI_PROFILE_DROPDOWN_WIDTH,
                    panel_left,
                    panel_right,
                ),
                f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
                self.render_ai_profile_dropdown(cx),
            )
        } else if self.ai_safety_menu_open {
            let anchor = self.select_anchors.get(&SelectAnchorId::AiSafetyMenu)?;
            (
                Corner::BottomLeft,
                ai_sidebar_popup_left(
                    f32::from(anchor.bounds.left()),
                    AI_MODEL_SELECTOR_DROPDOWN_WIDTH,
                    panel_left,
                    panel_right,
                ),
                f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
                self.render_ai_safety_menu(cx),
            )
        } else if self.ai_context_popover_open {
            let anchor = self.select_anchors.get(&SelectAnchorId::AiContextPopover)?;
            (
                Corner::BottomLeft,
                ai_sidebar_popup_left(
                    f32::from(anchor.bounds.left()),
                    AI_CONTEXT_POPOVER_WIDTH,
                    panel_left,
                    panel_right,
                ),
                f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
                self.render_ai_context_popover(cx),
            )
        } else {
            return None;
        };

        Some(
            popover_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, window, cx| {
                        this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event, window, cx| {
                        this.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .child(
                    deferred(
                        anchored()
                            .anchor(corner)
                            .position(gpui::point(px(anchor_x), px(anchor_y)))
                            .position_mode(AnchoredPositionMode::Window)
                            .child(overlay_content_boundary(div().child(popup))),
                    )
                    .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY),
                )
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn has_ai_sidebar_floating_overlay(&self) -> bool {
        self.ai_conversation_list_open
            || self.ai_chat_menu_open
            || self.ai_model_selector_open
            || self.ai_profile_selector_open
            || self.ai_safety_menu_open
            || self.ai_context_popover_open
    }

    fn render_ai_conversation_dropdown(&self, dropdown_width: f32, cx: &mut Context<Self>) -> AnyElement {
        let dropdown_height = if self.ai_chat.conversations.is_empty() {
            AI_CONVERSATION_EMPTY_HEIGHT
        } else {
            (self.ai_chat.conversations.len() as f32 * AI_CONVERSATION_ROW_HEIGHT)
                .min(AI_CONVERSATION_MAX_HEIGHT)
        };
        let mut list = div()
            .id("ai-conversation-dropdown-scroll")
            .w(px(dropdown_width))
            .flex()
            .flex_col()
            .h(px(dropdown_height))
            .selectable_overflow_y_scrollbar(
                &self.selectable_text_scroll_handle("ai-conversation-dropdown-scroll"),
            )
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg()
            // Conversation dropdown mirrors a browser popover list: wheel input
            // stays with the overlay and cannot scroll the message/sidebar body.
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation());

        if self.ai_chat.conversations.is_empty() {
            list = list.child(
                div()
                    .p(px(16.0))
                    .text_center()
                    .text_size(px(13.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-conversation-list",
                        "empty",
                        self.i18n.t("ai.chat.no_conversations"),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            );
        } else {
            for conversation in &self.ai_chat.conversations {
                list = list.child(self.render_ai_conversation_item(conversation, cx));
            }
        }
        list.into_any_element()
    }

    fn render_ai_conversation_item(
        &self,
        conversation: &AiConversation,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = conversation.id.clone();
        let delete_id = conversation.id.clone();
        let is_active = self.ai_chat.active_conversation_id.as_deref() == Some(conversation.id.as_str());
        let count = if conversation.messages_loaded {
            conversation.messages.len()
        } else {
            conversation.message_count
        };
        let meta = format!(
            "{} · {}",
            self.ai_messages_count_label(count),
            time_label(conversation.updated_at_ms)
        );
        div()
            .w_full()
            .flex_none()
            .h(px(AI_CONVERSATION_ROW_HEIGHT))
            .flex()
            .items_center()
            .justify_between()
            .px(px(12.0))
            .py(px(6.0))
            .border_l_2()
            .border_color(if is_active {
                rgb(self.tokens.ui.accent)
            } else {
                rgba(0x00000000)
            })
            .bg(if is_active {
                rgba((self.tokens.ui.accent << 8) | 0x0d)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .hover(|style| style.bg(rgba((self.tokens.ui.bg_panel << 8) | 0x66)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .pr(px(8.0))
                    .gap(px(2.0))
                    .child(
                        div().flex().items_center().gap(px(6.0)).min_w_0().when(
                            conversation.origin == "cli",
                            |row| {
                                row.child(
                                    div()
                                        .size(px(16.0))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(self.tokens.radii.md))
                                        .border_1()
                                        .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                                        .text_color(rgba((self.tokens.ui.text_muted << 8) | 0xb3))
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Terminal,
                                            10.0,
                                            rgb(self.tokens.ui.text_muted),
                                        )),
                                )
                            },
                        ).child(
                            div()
                                .truncate()
                                .text_size(px(12.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(if is_active {
                                    rgb(self.tokens.ui.text)
                                } else {
                                    rgb(self.tokens.ui.text_muted)
                                })
                                .child(conversation.title.clone()),
                        ),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .font_family(settings_ui_font_family(""))
                            .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x66))
                            .child(meta),
                    ),
            )
            .child(
                div()
                    .size(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .hover(|style| {
                        style
                            .bg(rgba((0xef4444_u32 << 8) | 0x1a))
                            .text_color(rgb(0xef4444))
                    })
                    .child(Self::render_lucide_icon(
                        LucideIcon::Trash2,
                        13.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.delete_ai_conversation(&delete_id);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.select_ai_conversation(id.clone());
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_ai_chat_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w(px(AI_CHAT_MENU_WIDTH))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .shadow_lg()
            .child(self.render_ai_chat_menu_item(
                LucideIcon::Settings,
                self.i18n.t("ai.chat.settings"),
                false,
                AiHeaderAction::Settings,
                cx,
            ))
            .child(self.render_ai_chat_menu_item(
                LucideIcon::Trash2,
                self.i18n.t("ai.chat.clear_all"),
                true,
                AiHeaderAction::NewChat,
                cx,
            ))
            .into_any_element()
    }

    fn render_ai_chat_menu_item(
        &self,
        icon: LucideIcon,
        label: String,
        destructive: bool,
        action: AiHeaderAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .mx(px(2.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(10.0))
            .py(px(7.0))
            .text_size(px(12.0))
            .text_color(if destructive {
                rgb(0xef4444)
            } else {
                rgb(self.tokens.ui.text_muted)
            })
            .cursor_pointer()
            .hover(|style| {
                style.bg(if destructive {
                    rgba((0xef4444_u32 << 8) | 0x1a)
                } else {
                    rgba((self.tokens.ui.border << 8) | 0x1a)
                })
            })
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                if destructive {
                    rgb(0xef4444)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
            .child(div().truncate().child(
                // Conversation menu rows are commands; text should not intercept row click.
                self.render_display_text_with_role(
                    SelectableTextRole::NonSelectable,
                    "ai-conversation-menu-action",
                    label.clone(),
                    label,
                    if destructive {
                        0xef4444
                    } else {
                        self.tokens.ui.text_muted
                    },
                    cx,
                ),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    match action {
                        AiHeaderAction::Settings => this.open_ai_settings(window, cx),
                        AiHeaderAction::NewChat => {
                            this.ai_clear_all_confirm_open = true;
                            this.reset_standard_confirm_focus();
                            cx.notify();
                        }
                    }
                    this.ai_chat_menu_open = false;
                    this.ai_conversation_list_open = false;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }


}

fn ai_sidebar_popup_left(desired: f32, popup_width: f32, panel_left: f32, panel_right: f32) -> f32 {
    let min_left = panel_left + AI_TOP_FLOATING_INSET_X;
    let max_left = (panel_right - AI_TOP_FLOATING_INSET_X - popup_width).max(min_left);
    desired.clamp(min_left, max_left)
}
