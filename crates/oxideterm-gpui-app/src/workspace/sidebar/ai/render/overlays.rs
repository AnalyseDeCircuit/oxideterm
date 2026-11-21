use oxideterm_gpui_ai::sidebar::transcript::{
    has_floating_sidebar_overlay, render_floating_sidebar_overlay,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn update_ai_sidebar_overlay_for_window_bounds(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let next_size = current_window_size(window);
        let previous_size = self
            .ai
            .state
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
        if !self.ai_sidebar_visible() {
            return None;
        }
        let owner = cx.weak_entity();
        render_floating_sidebar_overlay(
            self.ai.state.read(cx),
            &self.ai.transcript,
            &self.ai.model_picker,
            &self.select_anchors,
            &self.tokens,
            cx,
            || self.ai_chat_autocomplete_items(cx),
            move |window, cx| {
                let _ = owner.update(cx, |app, cx| {
                    app.dismiss_transient_workspace_overlays_from_outside_pointer(window, cx);
                });
            },
        )
    }

    pub(in crate::workspace) fn has_ai_sidebar_floating_overlay(&self, cx: &App) -> bool {
        has_floating_sidebar_overlay(self.ai.state.read(cx), || {
            !self.ai_chat_autocomplete_items(cx).is_empty()
        })
    }
}
