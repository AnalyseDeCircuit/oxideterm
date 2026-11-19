use gpui::{Context, KeyDownEvent, Window};
use oxideterm_ssh::{
    KeyboardInteractivePromptRequest, KeyboardInteractiveResponses, SshPromptError,
};
use tokio::sync::oneshot;

use crate::workspace::WorkspaceApp;
use crate::workspace::ime::keystroke_uses_text_edit_modifier;
use crate::workspace::new_connection::entity::{
    KeyboardInteractiveKeyAction, KeyboardInteractiveSubmitResult,
};

pub(in crate::workspace) use oxideterm_gpui_connections::prompts::NativeSshPromptSender;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_keyboard_interactive_challenge(
        &mut self,
        request: KeyboardInteractivePromptRequest,
        response_tx: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let opened = self.connections.flow.update(cx, |connection_flow, cx| {
            connection_flow.open_keyboard_interactive_challenge(request, response_tx, cx)
        });
        if !opened {
            return;
        }
        self.prepare_modal_interaction_boundary(cx);
        self.show_active_input_caret(cx);
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn handle_keyboard_interactive_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let action = self.connections.flow.update(cx, |connection_flow, cx| {
            connection_flow.handle_keyboard_interactive_key(
                key,
                modifiers.shift,
                keystroke_uses_text_edit_modifier(&event.keystroke),
                cx,
            )
        });
        match action {
            KeyboardInteractiveKeyAction::NotHandled => false,
            KeyboardInteractiveKeyAction::Paste => {
                self.paste_into_keyboard_interactive_field(cx);
                true
            }
            KeyboardInteractiveKeyAction::Cancel => {
                self.cancel_keyboard_interactive_challenge(cx);
                true
            }
            KeyboardInteractiveKeyAction::Submit => {
                self.submit_keyboard_interactive_challenge(window, cx);
                true
            }
            KeyboardInteractiveKeyAction::Handled => {
                self.show_active_input_caret(cx);
                true
            }
        }
    }

    fn paste_into_keyboard_interactive_field(&mut self, cx: &mut Context<Self>) {
        if self
            .connections
            .flow
            .update(cx, |flow, cx| flow.paste_interactive_clipboard(cx))
        {
            self.show_active_input_caret(cx);
        }
    }

    pub(in crate::workspace) fn submit_keyboard_interactive_challenge(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self.connections.flow.update(cx, |connection_flow, cx| {
            connection_flow.submit_keyboard_interactive_challenge(cx)
        });
        if matches!(result, KeyboardInteractiveSubmitResult::Submitted)
            && self.connection_form_state(cx).form.is_none()
        {
            self.focus_active_pane(window, cx);
        }
    }

    pub(in crate::workspace) fn cancel_keyboard_interactive_challenge(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if self.connections.flow.update(cx, |flow, cx| {
            flow.dismiss_keyboard_interactive_challenge(delay, &self.i18n, cx)
        }) {
            cx.notify();
        }
    }
}
