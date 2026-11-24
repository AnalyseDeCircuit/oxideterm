use std::rc::Rc;

use gpui::{App, AppContext, Context, Entity};
use oxideterm_gpui_ai::{
    sidebar::{AiChatHeaderFrame, AiChatHeaderRequest, AiChatHeaderView},
    state::{AiChatPopover, AiWorkspaceEntity},
};
use oxideterm_gpui_ui::{select::OverlayAnchor, tauri_ui_font_family};

use super::super::WorkspaceApp;

impl WorkspaceApp {
    pub(in crate::workspace) fn mount_ai_chat_header(
        state: &Entity<AiWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> Entity<AiChatHeaderView> {
        let owner = cx.weak_entity();
        let request_owner = owner.clone();
        let anchor_owner = owner.clone();
        cx.new(|_| {
            AiChatHeaderView::new(
                state.clone(),
                Rc::new(move |cx: &App| {
                    let owner = owner.upgrade()?;
                    let app = owner.read(cx);
                    Some(AiChatHeaderFrame {
                        tokens: app.tokens,
                        i18n: app.i18n.clone(),
                        background: app
                            .context_sidebar_content_background(app.tokens.ui.shell_bg()),
                        font: tauri_ui_font_family(
                            &app.settings_store.settings().appearance.ui_font_family,
                        ),
                    })
                }),
                Rc::new(move |request, window, cx| {
                    let _ = request_owner.update(cx, |app, cx| match request {
                        AiChatHeaderRequest::OpenPopover(popover, open) => {
                            app.close_ai_sidebar_popovers(cx);
                            app.ai
                                .state
                                .update(cx, |ai, _| ai.set_chat_popover_open(popover, open));
                            if popover == AiChatPopover::Menu {
                                window.focus(&app.focus_handle, cx);
                            }
                            cx.notify();
                        }
                        AiChatHeaderRequest::NewConversation => {
                            app.create_ai_sidebar_conversation(None, cx);
                        }
                    });
                }),
                Rc::new(move |anchor: OverlayAnchor, window, cx| {
                    let anchor_owner = anchor_owner.clone();
                    window.defer(cx, move |_, cx| {
                        let _ =
                            anchor_owner.update(cx, |app, cx| app.update_select_anchor(anchor, cx));
                    });
                }),
            )
        })
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn mount_ai_editors(
        state: &Entity<AiWorkspaceEntity>,
        cx: &mut Context<Self>,
    ) -> (
        Entity<oxideterm_gpui_ai::sidebar::editor::AiEditorView>,
        Entity<oxideterm_gpui_ai::sidebar::editor::AiEditorView>,
    ) {
        use super::super::ime::WorkspaceImeTarget;
        use oxideterm_gpui_ai::sidebar::editor::{
            AiEditorBindings, AiEditorFrame, AiEditorInput, AiEditorKind, AiEditorView,
        };
        let frame_owner = cx.weak_entity();
        let input_owner = frame_owner.clone();
        let anchor_owner = frame_owner.clone();
        let target = |kind| match kind {
            AiEditorKind::Composer => WorkspaceImeTarget::AiChatInput,
            AiEditorKind::MessageEdit => WorkspaceImeTarget::AiMessageEdit,
        };
        let bindings = AiEditorBindings {
            frame: Rc::new(move |kind, cx| {
                let owner = frame_owner.upgrade()?;
                let app = owner.read(cx);
                let settings = app.settings_store.settings();
                let target = target(kind);
                Some(AiEditorFrame {
                    tokens: app.tokens,
                    i18n: app.i18n.clone(),
                    font: tauri_ui_font_family(&settings.appearance.ui_font_family),
                    width: app.ai_editor_text_width(target),
                    anchor: target.anchor_id(),
                    model_selected: AiEditorView::model_selected(&settings.ai),
                    marked_range: app.ime_marked_virtual_range_for_target(target, cx),
                    selected_range: app.ime_selected_range_for_target(target, cx),
                    active_offset: app.ime_active_offset_for_target(target, cx),
                    composed_text: app.ime_text_with_marked_text_for_target(target, cx),
                    caret_visible: app.input_caret.visible(),
                })
            }),
            input: Rc::new(move |kind, input, window, cx| {
                let _ = input_owner.update(cx, |app, cx| match input {
                    AiEditorInput::PointerDown(event) => {
                        app.ime_marked_text = None;
                        window.focus(&app.focus_handle, cx);
                        app.begin_ime_selection_from_mouse_down(target(kind), &event, window, cx);
                    }
                    AiEditorInput::Drag(event) => {
                        app.update_ime_selection_drag_from_mouse_move(&event, window, cx)
                    }
                });
            }),
            anchor: Rc::new(move |anchor, window, cx| {
                let owner = anchor_owner.clone();
                window.defer(cx, move |_, cx| {
                    let _ = owner.update(cx, |app, cx| app.update_text_input_anchor(anchor, cx));
                });
            }),
        };
        AiEditorView::mount_pair(state, bindings, cx)
    }
}
