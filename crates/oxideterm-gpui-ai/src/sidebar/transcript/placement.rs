use gpui::{AnchoredPositionMode, Corner, anchored, deferred};
use oxideterm_gpui_ui::modal::{
    TAURI_POPOVER_LAYER_PRIORITY, overlay_content_boundary, popover_backdrop,
};

pub fn has_floating_sidebar_overlay(
    state: &AiWorkspaceEntity,
    has_autocomplete: impl FnOnce() -> bool,
) -> bool {
    let chat = state.chat_ui();
    chat.conversation_list_open
        || chat.menu_open
        || state.model_selector_is_open(crate::state::AiModelSelectorScope::Sidebar)
        || chat.reasoning_menu_open
        || chat.safety_menu_open
        || chat.context_popover_open
        || has_autocomplete()
}

pub fn render_floating_sidebar_overlay(
    state: &AiWorkspaceEntity,
    transcript: &Entity<AiAssistantSidebarView>,
    model_picker: &crate::model_picker::AiModelPickerViews,
    anchors: &HashMap<SelectAnchorId, OverlayAnchor>,
    tokens: &ThemeTokens,
    cx: &App,
    autocomplete: impl FnOnce() -> Vec<AiAutocompleteCandidate>,
    dismiss: impl Fn(&mut Window, &mut App) + Clone + 'static,
) -> Option<AnyElement> {
    use crate::state::AiModelSelectorScope;
    let panel_anchor = anchors.get(&SelectAnchorId::AiPanelRoot).copied()?;
    let panel_left = f32::from(panel_anchor.bounds.left());
    let panel_right = f32::from(panel_anchor.bounds.right());
    let panel_width = f32::from(panel_anchor.bounds.size.width);

    let (corner, anchor_x, anchor_y, popup, dismiss_from_outside) = if state
        .chat_ui()
        .conversation_list_open
    {
        let top = anchors
            .get(&SelectAnchorId::AiConversationList)
            .map(|anchor| f32::from(anchor.bounds.bottom()) + AI_FLOATING_GAP)
            .unwrap_or_else(|| f32::from(panel_anchor.bounds.top()) + AI_CHAT_PANEL_HEADER_HEIGHT);
        let dropdown_width = (panel_width - AI_TOP_FLOATING_INSET_X * 2.0).max(0.0);
        (
            Corner::TopLeft,
            panel_left + AI_TOP_FLOATING_INSET_X,
            top,
            AiSidebarLayer::new(
                transcript,
                AiSidebarLayerKind::Conversations(dropdown_width),
            )
            .into_any_element(),
            true,
        )
    } else if state.chat_ui().menu_open {
        let anchor = anchors.get(&SelectAnchorId::AiChatMenu).copied()?;
        let chat_menu_width = transcript.read(cx).ai_chat_menu_width();
        let left = ai_sidebar_popup_left(
            f32::from(anchor.bounds.right()) - chat_menu_width,
            chat_menu_width,
            panel_left,
            panel_right,
        );
        let top = f32::from(anchor.bounds.bottom()) + AI_FLOATING_GAP / 2.0;
        (
            Corner::TopLeft,
            left,
            top,
            AiSidebarLayer::new(transcript, AiSidebarLayerKind::ChatMenu).into_any_element(),
            true,
        )
    } else if state.model_selector_is_open(AiModelSelectorScope::Sidebar) {
        let anchor = anchors.get(&SelectAnchorId::AiModelSelector)?;
        (
            Corner::BottomLeft,
            ai_sidebar_popup_left(
                f32::from(anchor.bounds.left()),
                AI_MODEL_SELECTOR_DROPDOWN_WIDTH,
                panel_left,
                panel_right,
            ),
            f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
            model_picker.menu.clone().into_any_element(),
            true,
        )
    } else if state.chat_ui().reasoning_menu_open {
        let anchor = anchors.get(&SelectAnchorId::AiReasoningMenu)?;
        (
            Corner::BottomLeft,
            ai_sidebar_popup_left(
                f32::from(anchor.bounds.left()),
                AI_REASONING_MENU_WIDTH,
                panel_left,
                panel_right,
            ),
            f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
            AiSidebarLayer::new(transcript, AiSidebarLayerKind::Reasoning).into_any_element(),
            true,
        )
    } else if state.chat_ui().safety_menu_open {
        let anchor = anchors.get(&SelectAnchorId::AiSafetyMenu)?;
        (
            Corner::BottomLeft,
            ai_sidebar_popup_left(
                f32::from(anchor.bounds.left()),
                AI_MODEL_SELECTOR_DROPDOWN_WIDTH,
                panel_left,
                panel_right,
            ),
            f32::from(anchor.bounds.top()) - AI_FLOATING_GAP,
            AiSidebarLayer::new(transcript, AiSidebarLayerKind::Safety).into_any_element(),
            true,
        )
    } else if state.chat_ui().context_popover_open {
        let anchor = anchors.get(&SelectAnchorId::AiContextPopover)?;
        // Context usage is an informational inspector rather than a menu.
        // Reduced motion keeps only opacity, while Off mounts immediately.
        let popover = oxideterm_gpui_ui::motion::popover_zoom_enter(
            tokens,
            "ai-context-popover-enter",
            div().child(
                AiSidebarLayer::new(transcript, AiSidebarLayerKind::Context).into_any_element(),
            ),
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
    } else if let autocomplete_items = autocomplete()
        && !autocomplete_items.is_empty()
    {
        let anchor = anchors.get(&SelectAnchorId::AiAutocomplete)?;
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
                .child(
                    AiSidebarLayer::new(
                        transcript,
                        AiSidebarLayerKind::Autocomplete(autocomplete_items),
                    )
                    .into_any_element(),
                )
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
    .with_priority(TAURI_POPOVER_LAYER_PRIORITY);
    if !dismiss_from_outside {
        return Some(popup.into_any_element());
    }

    let dismiss_secondary = dismiss.clone();
    Some(
        popover_backdrop()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                dismiss(window, cx);
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, move |_, window, cx| {
                dismiss_secondary(window, cx);
                cx.stop_propagation();
            })
            .child(popup)
            .into_any_element(),
    )
}

fn ai_sidebar_popup_left(desired: f32, popup_width: f32, panel_left: f32, panel_right: f32) -> f32 {
    let min_left = panel_left + AI_TOP_FLOATING_INSET_X;
    let max_left = (panel_right - AI_TOP_FLOATING_INSET_X - popup_width).max(min_left);
    desired.clamp(min_left, max_left)
}
