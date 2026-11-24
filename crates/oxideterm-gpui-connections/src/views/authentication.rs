use std::rc::Rc;

use gpui::{App, Context, Entity, Render, Window, div, prelude::*};
use oxideterm_gpui_ui::motion::ExitPhase;
use oxideterm_ssh::HostKeyStatus;

use super::{
    ConnectionsInput, ConnectionsViewBindings, ConnectionsWindowRequest, HostKeyDecision,
    HostKeyFrame, HostKeyPresentation, connection_text_input,
    dialog::DialogRenderer,
    keyboard_interactive::{KeyboardAction, KeyboardStatus},
};
use crate::{
    entity::{AuthenticationChanged, ConnectionFlowEntity, HostKeyDialogSnapshot},
    prompts::NativeSshPromptSender,
};

impl From<HostKeyDialogSnapshot> for HostKeyFrame {
    fn from(snapshot: HostKeyDialogSnapshot) -> Self {
        let status = match snapshot.status {
            HostKeyStatus::Unknown {
                fingerprint,
                key_type,
            } => HostKeyPresentation::Unknown {
                fingerprint,
                key_type,
            },
            HostKeyStatus::Changed {
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            } => HostKeyPresentation::Changed {
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            },
            HostKeyStatus::Error { message } => HostKeyPresentation::Error { message },
            HostKeyStatus::Verified => HostKeyPresentation::Verified,
        };
        Self {
            host: snapshot.host,
            port: snapshot.port,
            visible: snapshot.visible,
            status,
        }
    }
}

pub(super) struct AuthenticationView {
    flow: Entity<ConnectionFlowEntity>,
    bindings: ConnectionsViewBindings,
    _authentication_changed: gpui::Subscription,
}

impl AuthenticationView {
    pub(super) fn new(
        flow: Entity<ConnectionFlowEntity>,
        bindings: ConnectionsViewBindings,
        cx: &mut Context<Self>,
    ) -> Self {
        let authentication_changed =
            cx.subscribe(&flow, |_, _, _: &AuthenticationChanged, cx| cx.notify());
        Self {
            flow,
            bindings,
            _authentication_changed: authentication_changed,
        }
    }
}

impl Render for AuthenticationView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let Some(environment) = (self.bindings.environment)(cx) else {
            return div();
        };
        let host_key = self
            .flow
            .read(cx)
            .host_key_dialog_snapshot()
            .map(|snapshot| {
                let dispatch = self.bindings.dispatch.clone();
                let renderer = DialogRenderer {
                    tokens: &environment.tokens,
                    i18n: &environment.i18n,
                    dispatch: Rc::new(move |decision, window, cx| {
                        let request = match decision {
                            HostKeyDecision::Cancel => ConnectionsWindowRequest::CancelHostKey,
                            HostKeyDecision::AcceptOnce => {
                                ConnectionsWindowRequest::AcceptHostKey { persist: false }
                            }
                            HostKeyDecision::AcceptAndPersist => {
                                ConnectionsWindowRequest::AcceptHostKey { persist: true }
                            }
                        };
                        dispatch(request, Some(window), cx);
                    }),
                };
                renderer.render_host_key_dialog(
                    &snapshot.into(),
                    &self.bindings.selectable_text,
                    cx,
                )
            });
        let authentication = self
            .flow
            .read(cx)
            .keyboard_interactive_challenge()
            .map(|challenge| {
                let flow = self.flow.downgrade();
                let dispatch = self.bindings.dispatch.clone();
                let renderer = DialogRenderer {
                    tokens: &environment.tokens,
                    i18n: &environment.i18n,
                    dispatch: Rc::new(move |action, window, cx: &mut App| match action {
                        KeyboardAction::Cancel => dispatch(
                            ConnectionsWindowRequest::CancelAuthentication,
                            Some(window),
                            cx,
                        ),
                        KeyboardAction::Submit => dispatch(
                            ConnectionsWindowRequest::SubmitAuthentication,
                            Some(window),
                            cx,
                        ),
                        KeyboardAction::ToggleRemember => {
                            let _ = flow
                                .update(cx, |flow, cx| flow.toggle_prompt_password_remember(cx));
                        }
                    }),
                };
                let status = KeyboardStatus {
                    title: &challenge.request.name,
                    instructions: &challenge.request.instructions,
                    visible: challenge.presence.phase() == ExitPhase::Visible,
                    focused_prompt: challenge.focused_prompt,
                    remember: challenge
                        .response_tx
                        .as_ref()
                        .is_some_and(NativeSshPromptSender::can_remember)
                        .then_some(challenge.remember_password),
                    external_wait: challenge.external_wait,
                    timed_out: challenge.timed_out(),
                    seconds_remaining: challenge.seconds_left(),
                    filled: challenge.all_responses_filled(),
                };
                let prompts =
                    challenge
                        .request
                        .prompts
                        .iter()
                        .enumerate()
                        .map(|(index, prompt)| {
                            (
                                prompt.prompt.as_str(),
                                prompt.echo,
                                challenge
                                    .responses
                                    .get(index)
                                    .map(String::as_str)
                                    .unwrap_or_default(),
                            )
                        });
                renderer.render_keyboard_dialog(status, prompts, |index, view| {
                    let target = ConnectionsInput::KeyboardInteractive(index);
                    let frame = (self.bindings.ime)(target, cx)?;
                    Some(connection_text_input(
                        &environment.tokens,
                        &self.bindings.dispatch,
                        target,
                        frame,
                        view,
                    ))
                })
            });
        div()
            .absolute()
            .inset_0()
            .children(host_key)
            .children(authentication)
    }
}
