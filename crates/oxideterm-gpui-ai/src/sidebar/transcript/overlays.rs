use oxideterm_gpui_ui::{
    context_menu::{ContextMenuActionableStyle, context_menu_action, context_menu_actionable_row},
    modal::rounded_shell_child_radius,
};
pub const AI_CONVERSATION_ROW_HEIGHT: f32 = 46.0; // Tauri ConversationItem: px-3 py-1.5, title + mono meta.
pub const AI_CONVERSATION_EMPTY_HEIGHT: f32 = 52.0; // Tauri empty row p-4 text-center.
pub const AI_CONVERSATION_MAX_HEIGHT: f32 = 256.0; // Tauri max-h-64.
pub const AI_CHAT_PANEL_HEADER_HEIGHT: f32 = 36.0; // Tauri AiChatPanel min-h-[36px].
pub const AI_TOP_FLOATING_INSET_X: f32 = 8.0; // Tauri left-2/right-2 and right-0 within the chat panel.
pub const AI_FLOATING_GAP: f32 = 4.0; // Tauri mt-0.5/mb-1 style popup gap.
pub const AI_CHAT_MENU_WIDTH: f32 = 160.0; // Tauri w-40.
pub const AI_MODEL_SELECTOR_DROPDOWN_WIDTH: f32 = 256.0; // Tauri w-64.
pub const AI_REASONING_MENU_WIDTH: f32 = 220.0; // Compact VS Code-style effort menu.
pub const AI_CONTEXT_POPOVER_WIDTH: f32 = 280.0; // Tauri-sized compact context popover.

struct AiConversationListRow {
    id: Arc<str>,
    title: String,
    cli_origin: bool,
    turn_count: usize,
    updated_at_ms: i64,
    active: bool,
    archived: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiHeaderAction {
    ArchivedConversations,
    NewChat,
    Settings,
}

impl AiAssistantSidebarView {
    fn render_ai_menu_action(
        &self,
        item: Div,
        disabled: bool,
        loading: bool,
        hover_bg: Option<Rgba>,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        // AI safety/chat menus are Radix dropdown-style command rows in Tauri.
        // Keep this as a single WorkspaceApp listener. Passing a cx.listener
        // through another cx.listener re-enters the same entity and trips GPUI's
        // update guard on menu clicks.
        let item = context_menu_actionable_row(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: hover_bg,
                hover_text_color: None,
            },
        );
        context_menu_action(
            item,
            disabled,
            loading,
            cx.listener(move |this, event, window, cx| {
                this.state.update(cx, |ai, _cx| {
                    ai.set_chat_popover_open(AiChatPopover::Menu, false);
                    ai.set_chat_popover_open(AiChatPopover::ConversationList, false);
                    ai.set_chat_popover_open(AiChatPopover::Safety, false);
                });
                listener(this, event, window, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }
}

impl AiAssistantSidebarView {
    pub fn render_ai_conversation_dropdown(
        &self,
        dropdown_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let show_archived = self.state.read(cx).chat_ui().show_archived_conversations;
        let page = self.state.read(cx).conversation_list_page(show_archived);
        let loading = page.loading();
        let failed = page.failed;
        let has_more = page.has_more;
        let count = self
            .state
            .read(cx)
            .conversation_state()
            .conversations
            .iter()
            .filter(|item| item.archived == show_archived)
            .count();
        let dropdown_height = (count as f32 * AI_CONVERSATION_ROW_HEIGHT
            + 36.0
            + if loading || failed || has_more {
                36.0
            } else {
                0.0
            })
        .clamp(
            AI_CONVERSATION_EMPTY_HEIGHT + 36.0,
            AI_CONVERSATION_MAX_HEIGHT,
        );
        let scroll_handle = self.selectable_text_scroll_handle("ai-conversation-dropdown-scroll");
        let mut list = div()
            .id("ai-conversation-dropdown-scroll")
            .w_full()
            .flex()
            .flex_col()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&scroll_handle)
            // Conversation dropdown mirrors a browser popover list: wheel input
            // stays with the overlay and cannot scroll the message/sidebar body.
            .on_scroll(|_, _, cx| cx.stop_propagation());

        let mut conversation_rows = {
            let ai = self.state.read(cx);
            let state = ai.conversation_state();
            state
                .conversations
                .iter()
                .filter(|conversation| conversation.archived == show_archived)
                .map(|conversation| AiConversationListRow {
                    archived: conversation.archived,
                    id: Arc::from(conversation.id.as_str()),
                    title: conversation.title.clone(),
                    cli_origin: conversation.origin == "cli",
                    turn_count: conversation.turn_count,
                    updated_at_ms: conversation.updated_at_ms,
                    active: state.active_conversation_id.as_deref()
                        == Some(conversation.id.as_str()),
                })
                .collect::<Vec<_>>()
        };
        conversation_rows.sort_by(|a, b| {
            b.updated_at_ms
                .cmp(&a.updated_at_ms)
                .then_with(|| b.id.cmp(&a.id))
        });
        list = list.child(
            div()
                .flex_none()
                .h(px(36.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t(if show_archived {
                            "ai.chat.archived_conversations"
                        } else {
                            "ai.chat.conversations"
                        })),
                )
                .child(
                    div()
                        .cursor_pointer()
                        .text_size(px(11.0))
                        .text_color(rgb(self.tokens.ui.accent))
                        .child(self.i18n.t(if show_archived {
                            "ai.chat.show_active"
                        } else {
                            "ai.chat.archived_conversations"
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.state.update(cx, |ai, _| {
                                    ai.show_archived_conversations(!show_archived)
                                });
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                ),
        );
        if conversation_rows.is_empty() && !loading && !failed {
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
                        self.i18n.t(if show_archived {
                            "ai.chat.no_archived_conversations"
                        } else {
                            "ai.chat.no_conversations"
                        }),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            );
        } else {
            let conversation_count = conversation_rows.len();
            for (index, conversation) in conversation_rows.into_iter().enumerate() {
                list = list.child(self.render_ai_conversation_item(
                    conversation,
                    index + 1 == conversation_count && !loading && !failed && !has_more,
                    cx,
                ));
            }
        }
        if loading || failed || has_more {
            list = list.child(
                div()
                    .id("ai-conversation-list-next")
                    .flex_none()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.accent))
                    .child(self.i18n.t(if loading {
                        "ai.chat.context_loading"
                    } else if failed {
                        "ai.chat.list_retry"
                    } else {
                        "ai.chat.list_more"
                    }))
                    .when(!loading, |row| {
                        row.cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.state.update(cx, |ai, _| {
                                    ai.request_conversation_list_page(show_archived)
                                });
                                cx.notify();
                            }))
                    }),
            );
        }
        material_surface(&self.tokens, div(), MaterialRole::Popover)
            .w(px(dropdown_width))
            .h(px(dropdown_height))
            .relative()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .shadow_lg()
            // Keep rounded clipping on a shell separate from the inner scroll
            // owner; setting overflow-hidden on the scroll owner disables it.
            .overflow_hidden()
            .child(list)
            .child(oxideterm_gpui_ui::scroll::vertical_scrollbar_layer(
                "ai-conversation-dropdown-scrollbar",
                &scroll_handle,
            ))
            .into_any_element()
    }

    fn render_ai_conversation_item(
        &self,
        conversation: AiConversationListRow,
        is_last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rename = &self.frame().overlay.rename;
        let (is_renaming, rename_draft, rename_focused) = {
            let ai = self.state.read(cx);
            let chat = ai.chat_ui();
            let is_renaming =
                chat.renaming_conversation_id.as_deref() == Some(conversation.id.as_ref());
            (
                is_renaming,
                is_renaming
                    .then(|| chat.renaming_conversation_draft.clone())
                    .unwrap_or_default(),
                is_renaming && chat.renaming_conversation_focused,
            )
        };
        let title = conversation.title;
        let rename_id = conversation.id.clone();
        let rename_title = title.clone();
        let archive_id = conversation.id.clone();
        let archived = conversation.archived;
        let archive_label = self.i18n.t(if archived {
            "ai.chat.restore_conversation"
        } else {
            "ai.chat.archive_conversation"
        });
        let archive_tokens = self.tokens;
        let delete_id = conversation.id.clone();
        let id = conversation.id;
        let is_active = conversation.active;
        let meta = format!(
            "{} · {}",
            self.ai_conversation_turns_label(conversation.turn_count),
            time_label(
                conversation.updated_at_ms,
                &self.i18n.t("ai.chat.today"),
                &self.i18n.t("ai.chat.yesterday"),
            )
        );
        let title_control = if is_renaming {
            let set_anchor = self.bindings.text_anchor.clone();
            let input = text_input_with_viewport_and_content_align(
                &self.tokens,
                TextInputView {
                    value: &rename_draft,
                    placeholder: String::new(),
                    focused: rename_focused,
                    caret_visible: rename.caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: rename.selected_range.clone(),
                    marked_text: rename.marked.as_deref(),
                },
                TextInputContentAlign::Start,
                &rename.viewport,
                rename.active_offset,
            )
            .h(px(20.0))
            .px(px(4.0))
            .rounded(px(self.tokens.radii.sm))
            .border_color(rgba((self.tokens.ui.accent << 8) | 0x66))
            .bg(rgba((self.tokens.ui.bg << 8) | 0x80))
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::BOLD)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.state.update(cx, |ai, _cx| {
                        ai.focus_conversation_rename();
                        ai.set_model_selector_search_focused(false);
                    });
                    (this.bindings.request)(
                        AiTranscriptRequest::RenamePointer(event.clone()),
                        Some(window),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(
                |this, event: &gpui::MouseMoveEvent, window, cx| {
                    (this.bindings.request)(
                        AiTranscriptRequest::RenameDrag(event.clone()),
                        Some(window),
                        cx,
                    );
                },
            ));
            text_input_anchor_probe(rename.anchor, input, move |anchor, window, cx| {
                set_anchor(anchor, window, cx)
            })
            .into_any_element()
        } else {
            div()
                // The title owns the row's remaining width so truncation
                // preserves text instead of collapsing to an ellipsis.
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(if is_active {
                    rgb(self.tokens.ui.text)
                } else {
                    rgb(self.tokens.ui.text_muted)
                })
                .child(title)
                .into_any_element()
        };
        let rename_tooltip = if is_renaming {
            self.i18n.t("ai.chat.save_conversation_title")
        } else {
            self.i18n.t("ai.chat.rename_conversation")
        };
        let rename_tooltip_label = rename_tooltip;
        let rename_tooltip_tokens = self.tokens;
        let rename_button = div()
            .id(format!("ai-conversation-rename-{id}"))
            .flex_none()
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .text_color(rgb(self.tokens.ui.text_muted))
            .hover(|style| {
                style
                    .bg(rgba((self.tokens.ui.accent << 8) | 0x1a))
                    .text_color(rgb(self.tokens.ui.accent))
            })
            .child(Self::render_lucide_icon(
                if is_renaming {
                    LucideIcon::Check
                } else {
                    LucideIcon::Pencil
                },
                13.0,
                if is_renaming {
                    rgb(self.tokens.ui.accent)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
            .tooltip(move |_window, cx| {
                oxideterm_gpui_ui::tooltip::tooltip_view(
                    rename_tooltip_tokens,
                    rename_tooltip_label.clone(),
                    None,
                    cx,
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    if is_renaming {
                        (this.bindings.request)(AiTranscriptRequest::SaveRename, None, cx);
                    } else {
                        (this.bindings.request)(
                            AiTranscriptRequest::BeginRename(
                                rename_id.to_string(),
                                rename_title.clone(),
                            ),
                            Some(window),
                            cx,
                        );
                    }
                    cx.stop_propagation();
                }),
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
            // Only the bottom row meets the rounded outer shell.
            .when(is_last, |row| {
                row.rounded_b(px(rounded_shell_child_radius(self.tokens.radii.md)))
            })
            .cursor_pointer()
            .when(!is_active, |row| {
                row.hover(|style| style.bg(rgba((self.tokens.ui.bg_panel << 8) | 0x66)))
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .pr(px(8.0))
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .min_w_0()
                            .when(conversation.cli_origin, |row| {
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
                            })
                            .child(title_control),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgba((self.tokens.ui.text_muted << 8) | 0x66))
                            .child(meta),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(rename_button)
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "ai-conversation-archive-{archive_id}"
                            )))
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.md))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgba((self.tokens.ui.accent << 8) | 0x1a)))
                            .child(Self::render_lucide_icon(
                                if archived {
                                    LucideIcon::RotateCcw
                                } else {
                                    LucideIcon::Archive
                                },
                                13.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .tooltip(move |_, cx| {
                                oxideterm_gpui_ui::tooltip::tooltip_view(
                                    archive_tokens,
                                    archive_label.clone(),
                                    None,
                                    cx,
                                )
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.state.update(cx, |ai, _| {
                                        ai.set_conversation_archived(&archive_id, !archived)
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.md))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .hover(|style| {
                                style
                                    .bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                                    .text_color(rgb(self.tokens.ui.error))
                            })
                            .child(Self::render_lucide_icon(
                                LucideIcon::Trash2,
                                13.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    (this.bindings.request)(
                                        AiTranscriptRequest::DeleteConversation(
                                            delete_id.to_string(),
                                        ),
                                        None,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.state.update(cx, |ai, _| {
                        ai.select_conversation(id.to_string());
                        ai.reset_chat_for_conversation_selection();
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    pub fn render_ai_chat_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        material_surface(&self.tokens, div(), MaterialRole::Popover)
            .w(px(AI_CHAT_MENU_WIDTH))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .shadow_lg()
            .child(self.render_ai_chat_menu_item(
                LucideIcon::Archive,
                self.i18n.t("ai.chat.archived_conversations"),
                false,
                AiHeaderAction::ArchivedConversations,
                cx,
            ))
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

    pub fn render_ai_chat_menu_item(
        &self,
        icon: LucideIcon,
        label: String,
        destructive: bool,
        action: AiHeaderAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = div()
            .mx(px(2.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .px(px(10.0))
            .py(px(7.0))
            .text_size(px(12.0))
            .text_color(if destructive {
                rgb(self.tokens.ui.error)
            } else {
                rgb(self.tokens.ui.text_muted)
            })
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                if destructive {
                    rgb(self.tokens.ui.error)
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
                        self.tokens.ui.error
                    } else {
                        self.tokens.ui.text_muted
                    },
                    cx,
                ),
            ));
        // Chat menu actions share the same disabled/loading action guard as
        // file and session context menus.
        self.render_ai_menu_action(
            item,
            false,
            false,
            Some(if destructive {
                rgba((self.tokens.ui.error << 8) | 0x1a)
            } else {
                rgba((self.tokens.ui.border << 8) | 0x1a)
            }),
            move |this, _event, window, cx| match action {
                AiHeaderAction::ArchivedConversations => {
                    this.state.update(cx, |ai, _| {
                        ai.show_archived_conversations(true);
                    });
                    cx.notify();
                }
                AiHeaderAction::Settings => {
                    (this.bindings.request)(AiTranscriptRequest::OpenSettings, Some(window), cx)
                }
                AiHeaderAction::NewChat => {
                    this.state.update(cx, |ai, cx| {
                        ai.open_chat_confirm(crate::state::AiChatConfirmKind::ClearAll, cx);
                    });
                }
            },
            cx,
        )
        .into_any_element()
    }
    pub fn render_ai_safety_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        // Tauri DropdownMenuContent uses w-64 and opens upward from the compact status bar.
        let menu = material_surface(&self.tokens, div(), MaterialRole::Popover)
            .w(px(256.0))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .shadow_lg()
            // Safety mode dropdown follows the same menu wheel boundary as
            // Tauri DropdownMenuContent.
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .py(px(self.tokens.spacing.one))
            .child(
                div()
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.one))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-safety-menu",
                        "title",
                        self.i18n.t("ai.safety_mode.menu_title"),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .child(self.render_ai_safety_menu_item(
                AiSafetyMode::ReadOnly,
                self.i18n.t("ai.safety_mode.read_only_mode"),
                self.i18n.t("ai.safety_mode.read_only_desc"),
                cx,
            ))
            .child(self.render_ai_safety_menu_item(
                AiSafetyMode::Default,
                self.i18n.t("ai.safety_mode.default_mode"),
                self.i18n.t("ai.safety_mode.default_desc"),
                cx,
            ))
            .child(self.render_ai_safety_menu_item(
                AiSafetyMode::Bypass,
                self.i18n.t("ai.safety_mode.bypass_mode"),
                self.i18n.t("ai.safety_mode.bypass_desc"),
                cx,
            ))
            .child(
                div()
                    .my(px(self.tokens.spacing.one))
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x66)),
            )
            .child(
                self.render_ai_menu_action(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(self.tokens.spacing.two))
                        .px(px(self.tokens.spacing.three))
                        .py(px(self.tokens.spacing.two))
                        .text_size(px(12.0))
                        .text_color(rgb(self.tokens.ui.text))
                        .child(
                            div()
                                .flex_none()
                                .w(px(16.0))
                                .h(px(16.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Settings,
                                    14.0,
                                    rgb(self.tokens.ui.text_muted),
                                )),
                        )
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::NonSelectable,
                            "ai-safety-menu",
                            "open-settings",
                            self.i18n.t("ai.safety_mode.open_settings"),
                            self.tokens.ui.text,
                            cx,
                        )),
                    false,
                    false,
                    Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
                    |this, _event, window, cx| {
                        (this.bindings.request)(
                            AiTranscriptRequest::OpenSettings,
                            Some(window),
                            cx,
                        );
                    },
                    cx,
                ),
            );
        menu.into_any_element()
    }

    pub fn render_ai_safety_menu_item(
        &self,
        mode: AiSafetyMode,
        title: String,
        description: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon = match mode {
            AiSafetyMode::Default => LucideIcon::ShieldCheck,
            AiSafetyMode::ReadOnly => LucideIcon::Shield,
            AiSafetyMode::Bypass => LucideIcon::ShieldAlert,
        };
        let bypass = mode == AiSafetyMode::Bypass;
        let title_color = if bypass {
            self.tokens.ui.warning
        } else {
            self.tokens.ui.text
        };
        let mode_key = match mode {
            AiSafetyMode::Default => "default",
            AiSafetyMode::ReadOnly => "read-only",
            AiSafetyMode::Bypass => "bypass",
        };
        let item = div()
            .flex()
            .items_start()
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.two))
            .child(
                div()
                    .flex_none()
                    .w(px(16.0))
                    .h(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    // A fixed icon slot keeps every label aligned even when
                    // individual SVG paths have different optical bounds.
                    .child(Self::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(if bypass {
                            self.tokens.ui.warning
                        } else {
                            self.tokens.ui.accent
                        }),
                    )),
            )
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.spacing.one / 2.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(title_color))
                            // Safety mode rows are menu items; text must bubble mouse-down like Tauri select-none labels.
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "ai-safety-menu-item-title",
                                mode_key,
                                title,
                                title_color,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .line_height(px(15.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "ai-safety-menu-item-description",
                                mode_key,
                                description,
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    ),
            );
        // Safety rows behave as menu actions; disabled/loading semantics stay
        // centralized even though these two actions are currently always enabled.
        self.render_ai_menu_action(
            item,
            false,
            false,
            Some(rgba((self.tokens.ui.bg_hover << 8) | 0x99)),
            move |this, _event, window, cx| {
                match mode {
                    AiSafetyMode::Default => {
                        (this.bindings.request)(AiTranscriptRequest::SafetyDefault, None, cx)
                    }
                    AiSafetyMode::ReadOnly => {
                        (this.bindings.request)(AiTranscriptRequest::SafetyReadOnly, None, cx)
                    }
                    AiSafetyMode::Bypass => {
                        if this.active_ai_safety_mode(cx) != AiSafetyMode::Bypass {
                            // The safety menu is itself a floating overlay.
                            // Open the confirm dialog after this click/update
                            // cycle so GPUI does not re-enter WorkspaceApp while
                            // the old menu frame is still being processed.
                            cx.defer_in(window, |this, _window, cx| {
                                (this.bindings.request)(
                                    AiTranscriptRequest::SafetyConfirm,
                                    None,
                                    cx,
                                );
                            });
                        }
                    }
                }
            },
            cx,
        )
        .into_any_element()
    }

    pub fn render_ai_safety_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "ai-safety-confirm-motion",
            self.state
                .read(cx)
                .chat_ui()
                .safety_confirm_presence
                .phase(),
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-safety-confirm",
                        "title",
                        self.i18n.t("ai.safety_mode.confirm_title"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: Some(
                    div()
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::NonSelectable,
                            "ai-safety-confirm",
                            "description",
                            self.i18n.t("ai.safety_mode.confirm_description"),
                            self.tokens.ui.text_muted,
                            cx,
                        ))
                        .into_any_element(),
                ),
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-safety-confirm",
                        "cancel",
                        self.i18n.t("ai.safety_mode.confirm_cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-safety-confirm",
                        "confirm",
                        self.i18n.t("ai.safety_mode.confirm_enable"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            self.frame().overlay.confirm_focus,
            cx.listener(|this, _event, _window, cx| {
                (this.bindings.request)(AiTranscriptRequest::ConfirmSafety(false), None, cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                (this.bindings.request)(AiTranscriptRequest::ConfirmSafety(true), None, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    pub fn render_ai_context_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(breakdown) = self.frame().overlay.breakdown.as_ref() else {
            return div().into_any_element();
        };
        let total_tokens = breakdown.total;
        let max_tokens = breakdown.max_tokens;
        let percentage = if max_tokens == 0 {
            0.0
        } else {
            ((total_tokens as f32 / max_tokens as f32) * 100.0).min(100.0)
        };
        let usage = AiContextUsage {
            percentage,
            warning: percentage > 70.0,
            danger: percentage > 85.0,
        };
        let prompt_cache_stats = {
            let ai = self.state.read(cx);
            ai.conversation_state()
                .active_conversation_id
                .as_deref()
                .and_then(|id| ai.chat_ui().prompt_cache_stats.get(id))
                .filter(|stats| stats.requests > 0)
                .cloned()
        };
        let popover = ai_context_popover(&self.tokens)
            .child(ai_context_popover_header(
                &self.tokens,
                self.i18n.t("ai.context.breakdown"),
                usage,
                format!(
                    "{} / {} tokens",
                    ai_format_tokens(total_tokens),
                    ai_format_tokens(max_tokens)
                ),
            ))
            .child(
                div()
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x1a)),
            )
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .mb(px(6.0))
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "ai-context-popover-section",
                                "system",
                                self.i18n.t("ai.context.system"),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    )
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.system_instructions"),
                        ai_context_percent(breakdown.system_instructions, max_tokens),
                        cx,
                    ))
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.tool_definitions"),
                        ai_context_percent(breakdown.tool_definitions, max_tokens),
                        cx,
                    ))
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.reserved_output"),
                        ai_context_percent(breakdown.reserved_output, max_tokens),
                        cx,
                    )),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x1a)),
            )
            .child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .child(
                        div()
                            .mb(px(6.0))
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "ai-context-popover-section",
                                "user",
                                self.i18n.t("ai.context.user_context"),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    )
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.messages_label"),
                        ai_context_percent(breakdown.messages, max_tokens),
                        cx,
                    ))
                    .child(self.render_ai_context_breakdown_row(
                        self.i18n.t("ai.context.tool_results"),
                        ai_context_percent(breakdown.tool_results, max_tokens),
                        cx,
                    )),
            )
            .when_some(prompt_cache_stats, |popover, stats| {
                let last_value = format!(
                    "{:.1}% ({} / {})",
                    stats.last_hit_rate(),
                    ai_format_tokens(stats.last_cache_read_tokens.min(usize::MAX as u64) as usize),
                    ai_format_tokens(stats.last_input_tokens.min(usize::MAX as u64) as usize),
                );
                let total_value = format!(
                    "{:.1}% ({} / {})",
                    stats.total_hit_rate(),
                    ai_format_tokens(stats.total_cache_read_tokens.min(usize::MAX as u64) as usize),
                    ai_format_tokens(stats.total_input_tokens.min(usize::MAX as u64) as usize),
                );
                let cache_write_value = (stats.total_cache_write_tokens > 0).then(|| {
                    ai_format_tokens(stats.total_cache_write_tokens.min(usize::MAX as u64) as usize)
                });
                popover
                    .child(
                        div()
                            .border_t_1()
                            .border_color(rgba((self.tokens.ui.border << 8) | 0x1a)),
                    )
                    .child(
                        div()
                            .px(px(12.0))
                            .py(px(8.0))
                            .child(
                                div()
                                    .mb(px(6.0))
                                    .text_size(px(10.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "ai-context-popover-section",
                                        "cache",
                                        self.i18n.t("ai.context.prompt_cache"),
                                        self.tokens.ui.text_muted,
                                        cx,
                                    )),
                            )
                            .child(self.render_ai_context_breakdown_row(
                                self.i18n.t("ai.context.cache_last_request"),
                                last_value,
                                cx,
                            ))
                            .child(self.render_ai_context_breakdown_row(
                                self.i18n.t("ai.context.cache_session_total"),
                                total_value,
                                cx,
                            ))
                            .when_some(cache_write_value, |section, write_tokens| {
                                section.child(self.render_ai_context_breakdown_row(
                                    self.i18n.t("ai.context.cache_write"),
                                    write_tokens,
                                    cx,
                                ))
                            }),
                    )
            })
            .when(
                self.state
                    .read(cx)
                    .conversation_state()
                    .active_conversation()
                    .is_some_and(|conversation| conversation.messages.len() >= 4),
                |popover| {
                    popover
                        .child(
                            div()
                                .border_t_1()
                                .border_color(rgba((self.tokens.ui.border << 8) | 0x1a)),
                        )
                        .child(
                            div()
                                .border_t_1()
                                .border_color(self.workspace_chrome_divider())
                                .px(px(12.0))
                                .py(px(8.0))
                                .child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .gap(px(6.0))
                                        .rounded_none()
                                        .px(px(12.0))
                                        .py(px(6.0))
                                        .text_size(px(11.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(rgb(self.tokens.ui.text))
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(rgba((self.tokens.ui.border << 8) | 0x33))
                                        })
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Archive,
                                            12.0,
                                            rgb(self.tokens.ui.text),
                                        ))
                                        // Popover command label mirrors Tauri select-none button text.
                                        .child(self.render_display_text_with_role(
                                            SelectableTextRole::NonSelectable,
                                            "ai-context-popover-action",
                                            "compress",
                                            self.i18n.t("ai.context.compress_dialog"),
                                            self.tokens.ui.text,
                                            cx,
                                        ))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.state.update(cx, |ai, _cx| {
                                                    ai.set_chat_popover_open(
                                                        AiChatPopover::Context,
                                                        false,
                                                    );
                                                });
                                                (this.bindings.request)(
                                                    AiTranscriptRequest::Compact,
                                                    None,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                        ),
                                ),
                        )
                },
            );
        popover.into_any_element()
    }

    pub fn render_ai_context_breakdown_row(
        &self,
        label: String,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .py(px(2.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "ai-context-breakdown-label",
                        (&label, &value),
                        label.clone(),
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.render_selectable_text_scoped(
                        "ai-context-breakdown-value",
                        (&label, &value),
                        value.clone(),
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub fn render_ai_autocomplete_popup(
        &self,
        items: &[AiAutocompleteCandidate],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_index = self
            .state
            .read(cx)
            .chat_ui()
            .autocomplete_index
            .min(items.len().saturating_sub(1));
        let mut popup = ai_autocomplete_popup(&self.tokens, "ai-chat-autocomplete");
        for (index, item) in items.iter().enumerate() {
            let prefix = match item.kind {
                AiAutocompleteKind::Slash => "/",
                AiAutocompleteKind::Participant => "@",
                AiAutocompleteKind::Reference => "#",
            };
            let candidate = item.clone();
            popup = popup.child(
                ai_autocomplete_item(
                    &self.tokens,
                    prefix,
                    item.name.clone(),
                    if item.description_is_i18n_key {
                        self.i18n.t(&item.description)
                    } else {
                        item.description.clone()
                    },
                    index == active_index,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        (this.bindings.request)(
                            AiTranscriptRequest::Autocomplete(candidate.clone()),
                            None,
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        popup.into_any_element()
    }

    pub fn render_ai_summarize_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "ai-summarize-confirm-motion",
            self.state
                .read(cx)
                .chat_ui()
                .summarize_confirm_presence
                .phase(),
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Default,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-summarize-confirm",
                        "title",
                        self.i18n.t("ai.context.summarize_confirm"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-summarize-confirm",
                        "cancel",
                        self.i18n.t("common.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-summarize-confirm",
                        "confirm",
                        self.i18n.t("ai.context.summarize"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            self.frame().overlay.confirm_focus,
            cx.listener(|this, _event, _window, cx| {
                (this.bindings.request)(AiTranscriptRequest::ConfirmSummary(false), None, cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                (this.bindings.request)(AiTranscriptRequest::ConfirmSummary(true), None, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    pub fn render_ai_clear_all_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(snapshot) = self.state.read(cx).chat_confirm_snapshot() else {
            return div().into_any_element();
        };
        if !matches!(snapshot.kind, crate::state::AiChatConfirmKind::ClearAll) {
            return div().into_any_element();
        }
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "ai-clear-all-confirm-motion",
            snapshot.phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-clear-all-confirm",
                        "title",
                        self.i18n.t("ai.chat.clear_all_confirm"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-clear-all-confirm",
                        "cancel",
                        self.i18n.t("common.actions.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-clear-all-confirm",
                        "confirm",
                        self.i18n.t("common.actions.confirm"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            snapshot.focused_action,
            cx.listener(|this, _event, _window, cx| {
                (this.bindings.request)(AiTranscriptRequest::ConfirmClearAll(false), None, cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                (this.bindings.request)(AiTranscriptRequest::ConfirmClearAll(true), None, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        )
    }

    pub fn render_ai_delete_message_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(snapshot) = self.state.read(cx).chat_confirm_snapshot() else {
            return div().into_any_element();
        };
        if !matches!(
            snapshot.kind,
            crate::state::AiChatConfirmKind::DeleteMessage { .. }
        ) {
            return div().into_any_element();
        }
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "ai-delete-message-confirm-motion",
            snapshot.phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "ai-delete-message-confirm",
                        "title",
                        self.i18n.t("ai.message.delete_confirm"),
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: None,
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-delete-message-confirm",
                        "cancel",
                        self.i18n.t("common.actions.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "ai-delete-message-confirm",
                        "confirm",
                        self.i18n.t("common.actions.confirm"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            snapshot.focused_action,
            cx.listener(|this, _event, _window, cx| {
                (this.bindings.request)(AiTranscriptRequest::ConfirmDeleteMessage(false), None, cx);
                cx.stop_propagation();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                (this.bindings.request)(AiTranscriptRequest::ConfirmDeleteMessage(true), None, cx);
                cx.stop_propagation();
            }),
        )
    }
    pub fn render_ai_reasoning_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (provider, model, selected) = self.frame().overlay.reasoning.as_ref()?;
        let selected = *selected;
        let capability = oxideterm_ai::model_reasoning_capability(&provider.provider_type, model);
        let mut levels = vec![AiReasoningLevel::Auto];
        levels.extend(capability.levels);
        let mut menu = material_surface(&self.tokens, div(), MaterialRole::Popover)
            .w(px(AI_REASONING_MENU_WIDTH))
            .overflow_hidden()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .shadow_lg()
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .py(px(self.tokens.spacing.one))
            .child(
                div()
                    .px(px(self.tokens.spacing.three))
                    .py(px(self.tokens.spacing.one))
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ai.reasoning.title")),
            );
        for level in levels {
            let provider_id = provider.id.clone();
            let provider_type = provider.provider_type.clone();
            let model_for_click = model.clone();
            let label = self.ai_reasoning_level_display(level);
            let is_selected = selected == level;
            menu = menu.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .mx(px(self.tokens.spacing.one))
                    .rounded(px(self.tokens.radii.md))
                    .px(px(self.tokens.spacing.two))
                    .py(px(self.tokens.spacing.one + self.tokens.spacing.one / 2.0))
                    .text_size(px(12.0))
                    .text_color(rgb(self.tokens.ui.text))
                    .bg(if is_selected {
                        rgba((self.tokens.ui.accent << 8) | 0x1a)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                    .child(div().w(px(14.0)).flex_none().when(is_selected, |check| {
                        check.child(Self::render_lucide_icon(
                            LucideIcon::Check,
                            14.0,
                            rgb(self.tokens.ui.accent),
                        ))
                    }))
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            (this.bindings.request)(
                                AiTranscriptRequest::SelectReasoning(
                                    provider_id.clone(),
                                    provider_type.clone(),
                                    model_for_click.clone(),
                                    level,
                                ),
                                None,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        if !capability.known_model {
            menu = menu.child(
                div()
                    .mt(px(self.tokens.spacing.one))
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
                    .px(px(self.tokens.spacing.three))
                    .pt(px(self.tokens.spacing.two))
                    .pb(px(self.tokens.spacing.one))
                    .text_size(px(9.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("ai.reasoning.custom_model_hint")),
            )
        }
        Some(menu.into_any_element())
    }

    fn ai_reasoning_level_display(&self, level: AiReasoningLevel) -> String {
        self.i18n
            .t(&format!("ai.reasoning.level_{}", level.as_str()))
    }
}
