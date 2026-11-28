use super::*;
use oxideterm_gpui_ai::state::{
    AiChatConfirmEffect, AiChatConfirmKeyAction, AiChatConfirmOwnerKind,
};
use oxideterm_gpui_editor::line_edit::{FieldEvent, LineField};
use oxideterm_gpui_ui::select::{OverlayAnchor, SelectAnchorId};
use std::collections::HashMap;

pub(super) type Anchors = Rc<RefCell<HashMap<SelectAnchorId, OverlayAnchor>>>;

pub(super) fn anchor_writer(
    anchors: &Anchors,
    state: &Entity<AiWorkspaceEntity>,
) -> Rc<dyn Fn(OverlayAnchor, &mut Window, &mut App)> {
    let anchors = anchors.clone();
    let state = state.clone();
    Rc::new(move |anchor, w, cx| {
        let old = anchors.borrow_mut().insert(anchor.id.clone(), anchor);
        if old.is_some_and(|a| a.bounds != anchor.bounds)
            && has_floating_sidebar_overlay(state.read(cx), || false)
        {
            w.refresh();
        }
    })
}

pub(crate) struct ChatLayers {
    state: Entity<AiWorkspaceEntity>,
    transcript: Entity<AiAssistantSidebarView>,
    models: AiModelPickerViews,
    anchors: Anchors,
    style: Rc<RefCell<(ThemeTokens, I18n, SharedString)>>,
}

impl ChatLayers {
    pub(super) fn new(
        state: Entity<AiWorkspaceEntity>,
        transcript: Entity<AiAssistantSidebarView>,
        models: AiModelPickerViews,
        anchors: Anchors,
        style: Rc<RefCell<(ThemeTokens, I18n, SharedString)>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut mounted = false;
        cx.observe(&state, move |_, state, cx| {
            let state = state.read(cx);
            let visible = state.chat_confirm_owner_snapshot().is_some()
                || has_floating_sidebar_overlay(state, || false);
            if mounted || visible {
                cx.notify();
            }
            mounted = visible;
        })
        .detach();
        Self {
            state,
            transcript,
            models,
            anchors,
            style,
        }
    }
}

impl Render for ChatLayers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        if let Some(confirm) = state.chat_confirm_owner_snapshot() {
            let layer = match confirm.kind {
                AiChatConfirmOwnerKind::ClearAll => AiSidebarLayerKind::ClearAllConfirm,
                AiChatConfirmOwnerKind::DeleteMessage => AiSidebarLayerKind::DeleteMessageConfirm,
            };
            return AiSidebarLayer::new(&self.transcript, layer).into_any_element();
        }
        let target = self.state.clone();
        render_floating_sidebar_overlay(
            state,
            &self.transcript,
            &self.models,
            &self.anchors.borrow(),
            &self.style.borrow().0,
            cx,
            Vec::new,
            move |_, cx| {
                target.update(cx, |s, cx| {
                    s.close_chat_popovers();
                    s.dismiss_model_selector();
                    s.clear_conversation_rename();
                    cx.notify();
                });
            },
        )
        .unwrap_or_else(|| div().into_any_element())
    }
}

impl Assistant {
    pub(crate) fn layers(&self) -> AnyElement {
        self.layers.clone().into_any_element()
    }

    pub(crate) fn overlay_key(
        &mut self,
        e: &KeyDownEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.state.read(cx).chat_confirm_owner_snapshot().is_some() {
            let action = self.state.update(cx, |s, cx| {
                s.handle_chat_confirm_key(
                    &e.keystroke.key,
                    e.keystroke.modifiers.shift,
                    e.keystroke.modifiers.platform || e.keystroke.modifiers.control,
                    cx,
                )
            });
            match action {
                Some(AiChatConfirmKeyAction::Cancel) => self.confirm_chat(false, cx),
                Some(AiChatConfirmKeyAction::Confirm) => self.confirm_chat(true, cx),
                _ => {}
            }
            w.prevent_default();
            cx.stop_propagation();
            return true;
        }
        if has_floating_sidebar_overlay(self.state.read(cx), || false) {
            if self.rename.focus_handle(cx).is_focused(w) {
                return true;
            }
            if e.keystroke.key == "escape" {
                self.state.update(cx, |s, cx| {
                    s.dismiss_model_selector();
                    s.close_chat_popovers();
                    cx.notify();
                });
                w.prevent_default();
                cx.stop_propagation();
            }
            return true;
        }
        false
    }

    pub(super) fn confirm_chat(&mut self, confirmed: bool, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.env.borrow().0,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        self.state.update(cx, |s, cx| {
            match s.begin_chat_confirm_exit(confirmed, delay, cx).1 {
                Some(AiChatConfirmEffect::DeleteMessage { message_id }) => {
                    s.delete_ai_message(&message_id, cx)
                }
                Some(AiChatConfirmEffect::ClearAll) => {
                    s.cancel_chat_stream();
                    s.reset_conversation_lists_after_clear();
                    for id in self
                        .history
                        .ids()
                        .into_iter()
                        .chain(
                            s.conversation_state()
                                .conversations
                                .iter()
                                .map(|c| c.id.clone()),
                        )
                        .collect::<std::collections::HashSet<_>>()
                    {
                        s.cancel_chat_stream_for(&id);
                        s.delete_conversation(&id);
                    }
                    s.clear_conversations();
                    s.clear_chat_expansions();
                    s.reset_chat_for_new_conversation();
                    s.close_chat_popovers();
                    s.dismiss_model_selector();
                    s.persist_chat_state();
                    cx.notify();
                }
                None => {}
            }
        });
    }

    pub(super) fn rename_field(
        state: &Entity<AiWorkspaceEntity>,
        tokens: ThemeTokens,
        cx: &mut Context<Self>,
    ) -> Entity<LineField> {
        let field = cx.new(|cx| LineField::new(tokens, String::new(), false, cx));
        let state = state.clone();
        cx.subscribe(&field, move |_, field, event, cx| {
            state.update(cx, |s, cx| {
                if s.chat_ui().renaming_conversation_id.is_none() {
                    return;
                }
                match event {
                    FieldEvent::Changed => {
                        let end = s
                            .chat_ui()
                            .renaming_conversation_draft
                            .encode_utf16()
                            .count();
                        s.replace_conversation_rename(Some(0..end), field.read(cx).text());
                        cx.notify();
                    }
                    FieldEvent::Submit => {
                        s.save_conversation_rename(cx);
                    }
                    FieldEvent::Cancel => {
                        s.clear_conversation_rename();
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        field
    }
}
