use oxideterm_gpui_ai::sidebar::transcript::{AiSidebarLayer, AiSidebarLayerKind, AI_CHAT_MENU_WIDTH, AI_CHAT_PANEL_HEADER_HEIGHT, AI_CONTEXT_POPOVER_WIDTH, AI_FLOATING_GAP, AI_MODEL_SELECTOR_DROPDOWN_WIDTH, AI_REASONING_MENU_WIDTH, AI_TOP_FLOATING_INSET_X};
impl WorkspaceApp {
    pub(in crate::workspace) fn update_ai_sidebar_overlay_for_window_bounds(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let next_size = current_window_size(window);
        let previous_size = self
            .ai.state
            .update(cx, |ai, _cx| ai.replace_overlay_window_size(next_size));
        let Some(previous_size) = previous_size else {
            return;
        };
        let dx = next_size.0 - previous_size.0;
        let dy = next_size.1 - previous_size.1;
        if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
            return;
        }
        if !self.has_ai_sidebar_floating_overlay(cx) {
            return;
        }

        self.shift_ai_sidebar_overlay_anchors(dx, dy);
        cx.notify();
    }

    pub(in crate::workspace) fn shift_ai_sidebar_overlay_anchors(&mut self, dx: f32, dy: f32) {
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
                | SelectAnchorId::AiReasoningMenu
                | SelectAnchorId::AiSafetyMenu
                | SelectAnchorId::AiContextPopover
                | SelectAnchorId::AiAutocomplete => {
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
        if !self.ai_sidebar_visible() || !self.has_ai_sidebar_floating_overlay(cx) {
            return None;
        }

        let panel_anchor = self
            .select_anchors
            .get(&SelectAnchorId::AiPanelRoot)
            .copied()?;
        let panel_left = f32::from(panel_anchor.bounds.left());
        let panel_right = f32::from(panel_anchor.bounds.right());
        let panel_width = f32::from(panel_anchor.bounds.size.width);

        let (corner, anchor_x, anchor_y, popup, dismiss_from_outside) =
            if self.ai.state.read(cx).chat_ui().conversation_list_open {
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
                    AiSidebarLayer::new(&self.ai.transcript, AiSidebarLayerKind::Conversations(dropdown_width)).into_any_element(),
                    true,
                )
            } else if self.ai.state.read(cx).chat_ui().menu_open {
                let anchor = self
                    .select_anchors
                    .get(&SelectAnchorId::AiChatMenu)
                    .copied()?;
                let left = ai_sidebar_popup_left(
                    f32::from(anchor.bounds.right()) - AI_CHAT_MENU_WIDTH,
                    AI_CHAT_MENU_WIDTH,
                    panel_left,
                    panel_right,
                );
                let top = f32::from(anchor.bounds.bottom()) + AI_FLOATING_GAP / 2.0;
                (
                    Corner::TopLeft,
                    left,
                    top,
                    AiSidebarLayer::new(&self.ai.transcript, AiSidebarLayerKind::ChatMenu).into_any_element(),
                    true,
                )
            } else if self
                .ai.state
                .read(cx)
                .model_selector_is_open(AiModelSelectorScope::Sidebar)
            {
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
                    self.ai.model_picker.menu.clone().into_any_element(),
                    true,
                )
            } else if self.ai.state.read(cx).chat_ui().reasoning_menu_open {
                let anchor = self.select_anchors.get(&SelectAnchorId::AiReasoningMenu)?;
                (
                    Corner::BottomLeft,
                    ai_sidebar_popup_left(
                        f32::from(anchor.bounds.left()),
                        AI_REASONING_MENU_WIDTH,
                        panel_left,
                        panel_right,
                    ),
                    f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
                    AiSidebarLayer::new(&self.ai.transcript, AiSidebarLayerKind::Reasoning).into_any_element(),
                    true,
                )
            } else if self.ai.state.read(cx).chat_ui().safety_menu_open {
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
                    AiSidebarLayer::new(&self.ai.transcript, AiSidebarLayerKind::Safety).into_any_element(),
                    true,
                )
            } else if self.ai.state.read(cx).chat_ui().context_popover_open {
                let anchor = self.select_anchors.get(&SelectAnchorId::AiContextPopover)?;
                // Context usage is an informational inspector rather than a menu.
                // Reduced motion keeps only opacity, while Off mounts immediately.
                let popover = oxideterm_gpui_ui::motion::popover_zoom_enter(
                    &self.tokens,
                    "ai-context-popover-enter",
                    div().child(AiSidebarLayer::new(&self.ai.transcript, AiSidebarLayerKind::Context).into_any_element()),
                    6.0,
                    gpui::point(0.5, 1.0),
                );
                (
                    Corner::BottomLeft,
                    ai_sidebar_popup_left(
                        f32::from(anchor.bounds.left()),
                        AI_CONTEXT_POPOVER_WIDTH,
                        panel_left,
                        panel_right,
                    ),
                    f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
                    popover,
                    true,
                )
            } else if let autocomplete_items = self.ai_chat_autocomplete_items(cx)
                && !autocomplete_items.is_empty()
            {
                let anchor = self.select_anchors.get(&SelectAnchorId::AiAutocomplete)?;
                let popup_width = f32::from(anchor.bounds.size.width)
                    .min((panel_width - AI_TOP_FLOATING_INSET_X * 2.0).max(1.0))
                    .max(1.0);
                (
                    Corner::BottomLeft,
                    ai_sidebar_popup_left(
                        f32::from(anchor.bounds.left()),
                        popup_width,
                        panel_left,
                        panel_right,
                    ),
                    f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
                    div()
                        .w(px(popup_width))
                        .child(AiSidebarLayer::new(&self.ai.transcript, AiSidebarLayerKind::Autocomplete(autocomplete_items)).into_any_element())
                        .into_any_element(),
                    false,
                )
            } else {
                return None;
            };

        let popup = deferred(
            anchored()
                .anchor(corner)
                .position(gpui::point(px(anchor_x), px(anchor_y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(overlay_content_boundary(div().child(popup))),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY);
        if !dismiss_from_outside {
            return Some(popup.into_any_element());
        }

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
                .child(popup)
                .into_any_element(),
        )
    }

    pub(in crate::workspace) fn has_ai_sidebar_floating_overlay(&self, cx: &App) -> bool {
        self.ai.state.read(cx).chat_ui().conversation_list_open
            || self.ai.state.read(cx).chat_ui().menu_open
            || self
                .ai.state
                .read(cx)
                .model_selector_is_open(AiModelSelectorScope::Sidebar)
            || self.ai.state.read(cx).chat_ui().reasoning_menu_open
            || self.ai.state.read(cx).chat_ui().safety_menu_open
            || self.ai.state.read(cx).chat_ui().context_popover_open
            || !self.ai_chat_autocomplete_items(cx).is_empty()
    }

}

pub(in crate::workspace) fn ai_sidebar_popup_left(
    desired: f32,
    popup_width: f32,
    panel_left: f32,
    panel_right: f32,
) -> f32 {
    let min_left = panel_left + AI_TOP_FLOATING_INSET_X;
    let max_left = (panel_right - AI_TOP_FLOATING_INSET_X - popup_width).max(min_left);
    desired.clamp(min_left, max_left)
}
