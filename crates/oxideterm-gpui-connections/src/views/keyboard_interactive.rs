use std::{rc::Rc, time::Duration};

use gpui::{
    AnyElement, App, BackgroundExecutor, Context, FocusHandle, Focusable, Instant, KeyDownEvent,
    MouseButton, Render, Task, Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_ui::{
    TextInputView,
    button::ButtonVariant,
    form_field,
    modal::{modal_container, rounded_shell_child_radius},
};
use zeroize::Zeroizing;

use super::{ConnectionsEnvironment, dialog::DialogRenderer};
mod input;
use input::PromptDraft;

pub struct KeyboardInteractivePrompt {
    pub label: String,
    pub echo: bool,
}

pub struct KeyboardInteractiveFrame {
    pub title: String,
    pub instructions: String,
    pub prompts: Vec<KeyboardInteractivePrompt>,
    pub remember_eligible: bool,
    pub external_wait: bool,
    /// Remaining lifetime at mount, supplied by the challenge owner.
    pub seconds_remaining: u64,
}

pub enum KeyboardInteractiveDecision {
    /// Answers in prompt order and the user's choice to remember an eligible password.
    Submit(Vec<Zeroizing<String>>, bool),
    /// Dismissed by the user or expired before the answers were delivered.
    Cancel,
}

/// A single challenge owns its drafts and ticking task until delivery or unmount.
/// The recipient removes the mounted dialog after the callback.
pub struct PortableKeyboardInteractiveDialog {
    environment: ConnectionsEnvironment,
    frame: KeyboardInteractiveFrame,
    drafts: Vec<PromptDraft>,
    focused: usize,
    focus: FocusHandle,
    remember: bool,
    clock: BackgroundExecutor,
    opened_at: Instant,
    timer: Option<Task<()>>,
    respond: Option<Box<dyn FnOnce(KeyboardInteractiveDecision, &mut App)>>,
}

impl PortableKeyboardInteractiveDialog {
    pub fn new(
        environment: ConnectionsEnvironment,
        frame: KeyboardInteractiveFrame,
        respond: impl FnOnce(KeyboardInteractiveDecision, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let clock = cx.background_executor().clone();
        let opened_at = clock.now();
        let lifetime = Duration::from_secs(frame.seconds_remaining);
        let timer = cx.spawn(async move |owner, cx| {
            loop {
                let elapsed = cx
                    .background_executor()
                    .now()
                    .saturating_duration_since(opened_at);
                let remaining = lifetime.saturating_sub(elapsed);
                if remaining.is_zero() {
                    let _ = owner.update(cx, |view: &mut Self, cx| {
                        view.act(KeyboardAction::Cancel, cx)
                    });
                    break;
                }
                cx.background_executor()
                    .timer(remaining.min(Duration::from_secs(1)))
                    .await;
                if owner.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            drafts: frame
                .prompts
                .iter()
                .map(|_| PromptDraft::default())
                .collect(),
            environment,
            frame,
            focused: 0,
            focus,
            remember: false,
            clock,
            opened_at,
            timer: Some(timer),
            respond: Some(Box::new(respond)),
        }
    }

    pub fn set_external_wait(&mut self, waiting: bool, cx: &mut Context<Self>) {
        if waiting && let Some(draft) = self.drafts.get_mut(self.focused) {
            draft.commit_marked();
        }
        self.frame.external_wait = waiting;
        cx.notify();
    }

    fn remaining(&self) -> Duration {
        Duration::from_secs(self.frame.seconds_remaining)
            .saturating_sub(self.clock.now().saturating_duration_since(self.opened_at))
    }

    fn editing(&self) -> bool {
        self.respond.is_some() && !self.frame.external_wait && !self.remaining().is_zero()
    }

    fn filled(&self) -> bool {
        !self.frame.external_wait
            && self
                .drafts
                .iter()
                .all(|d| !d.value.is_empty() && d.marked.is_none())
    }

    fn act(&mut self, action: KeyboardAction, cx: &mut Context<Self>) {
        let decision = match action {
            KeyboardAction::Cancel => KeyboardInteractiveDecision::Cancel,
            KeyboardAction::Submit if self.editing() && self.filled() => {
                let answers = self
                    .drafts
                    .iter_mut()
                    .map(|draft| std::mem::take(&mut draft.value))
                    .collect();
                KeyboardInteractiveDecision::Submit(answers, self.remember)
            }
            KeyboardAction::ToggleRemember if self.editing() && self.frame.remember_eligible => {
                self.remember = !self.remember;
                cx.notify();
                return;
            }
            _ => return,
        };
        if let Some(respond) = self.respond.take() {
            // Keep the field geometry for the exit animation, not its answers or text layouts.
            for draft in &mut self.drafts {
                *draft = PromptDraft::default();
            }
            self.timer = None;
            cx.notify();
            cx.defer(move |cx| respond(decision, cx));
        }
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if self
            .drafts
            .get(self.focused)
            .is_some_and(|d| d.marked.is_some())
        {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        match key {
            "escape" => self.act(KeyboardAction::Cancel, cx),
            "enter" => self.act(KeyboardAction::Submit, cx),
            "tab" if !self.drafts.is_empty() => {
                self.focused = if modifiers.shift {
                    self.focused.saturating_sub(1)
                } else {
                    (self.focused + 1).min(self.drafts.len() - 1)
                };
                window.notify_text_input_changed(&self.focus);
                cx.notify();
            }
            _ => {
                if !self.editing() {
                    return;
                }
                let Some(draft) = self.drafts.get_mut(self.focused) else {
                    return;
                };
                if !draft.edit_key(key, modifiers) {
                    return;
                }
                window.notify_text_input_changed(&self.focus);
                cx.notify();
            }
        }
        window.prevent_default();
        cx.stop_propagation();
    }
}

impl Focusable for PortableKeyboardInteractiveDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PortableKeyboardInteractiveDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.weak_entity();
        let remaining = self.remaining();
        let renderer = DialogRenderer {
            tokens: &self.environment.tokens,
            i18n: &self.environment.i18n,
            dispatch: Rc::new(move |action, _, cx| {
                let _ = owner.update(cx, |view, cx| view.act(action, cx));
            }),
        };
        let status = KeyboardStatus {
            title: &self.frame.title,
            instructions: &self.frame.instructions,
            focused_prompt: self.focused,
            visible: self.respond.is_some(),
            remember: self.frame.remember_eligible.then_some(self.remember),
            external_wait: self.frame.external_wait,
            seconds_remaining: remaining.as_secs(),
            timed_out: remaining.is_zero(),
            filled: self.respond.is_some() && self.filled(),
        };
        let prompts = self
            .frame
            .prompts
            .iter()
            .zip(&self.drafts)
            .map(|(prompt, draft)| (prompt.label.as_str(), prompt.echo, draft.value.as_str()));
        let dialog = renderer.render_keyboard_dialog(status, prompts, |index, view| {
            let draft = &self.drafts[index];
            let focused = view.focused && self.editing();
            let input = crate::input::paint_input(
                renderer.tokens,
                TextInputView {
                    focused,
                    caret_visible: focused,
                    selected_range: focused.then(|| draft.selection()),
                    marked_text: draft.marked.as_ref().map(|(text, _)| text.as_str()),
                    ..view
                },
                &draft.viewport,
                Some(draft.caret),
            )
            .when(focused, |input| {
                let focus = self.focus.clone();
                let owner = cx.entity();
                input.relative().child(
                    gpui::canvas(
                        |_, _, _| (),
                        move |bounds, _, window, cx| {
                            window.handle_input(
                                &focus,
                                gpui::ElementInputHandler::new(bounds, owner),
                                cx,
                            );
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
            });
            Some(
                input
                    .id(("kbi-prompt", index))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                            if view.editing() {
                                if let Some(previous) = view.drafts.get_mut(view.focused) {
                                    previous.commit_marked();
                                }
                                view.focused = index;
                                window.focus(&view.focus, cx);
                                view.drafts[index].select_at(event, view.frame.prompts[index].echo);
                                window.notify_text_input_changed(&view.focus);
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(
                        move |view, event: &gpui::MouseMoveEvent, window, cx| {
                            if event.dragging() && view.editing() && view.focused == index {
                                let draft = &mut view.drafts[index];
                                if let Some(offset) =
                                    draft.offset(event.position, view.frame.prompts[index].echo)
                                {
                                    draft.caret = offset;
                                    window.notify_text_selection_changed(&view.focus);
                                    cx.notify();
                                }
                            }
                        },
                    ))
                    .into_any_element(),
            )
        });
        div()
            .absolute()
            .inset_0()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(dialog)
    }
}

#[derive(Clone, Copy)]
pub(super) enum KeyboardAction {
    Cancel,
    Submit,
    ToggleRemember,
}

/// Per-render metadata only. Answers are borrowed directly from their owner for painting.
pub(super) struct KeyboardStatus<'a> {
    pub title: &'a str,
    pub instructions: &'a str,
    pub visible: bool,
    pub focused_prompt: usize,
    pub remember: Option<bool>,
    pub external_wait: bool,
    pub seconds_remaining: u64,
    pub timed_out: bool,
    pub filled: bool,
}

impl DialogRenderer<'_, KeyboardAction> {
    pub(super) fn render_keyboard_dialog<'a>(
        &self,
        status: KeyboardStatus<'_>,
        prompts: impl IntoIterator<Item = (&'a str, bool, &'a str)>,
        mut render_input: impl FnMut(usize, TextInputView<'_>) -> Option<AnyElement>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let title = if status.title.trim().is_empty() {
            self.i18n.t("ssh.kbi.title")
        } else {
            status.title.to_owned()
        };
        let timed_out = status.timed_out;
        let seconds_left = status.seconds_remaining;
        let can_submit = !timed_out && status.filled;
        let external_wait = status.external_wait;

        let mut prompt_list = div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.modal_section_gap));
        for (index, (label, echo, value)) in prompts.into_iter().enumerate() {
            let Some(input) = render_input(
                index,
                TextInputView {
                    value,
                    placeholder: String::new(),
                    focused: status.focused_prompt == index,
                    caret_visible: false,
                    secret: !echo,
                    selected_all: false,
                    selected_range: None,
                    marked_text: None,
                },
            ) else {
                continue;
            };
            prompt_list = prompt_list.child(form_field(
                self.tokens,
                if label == "ssh.form.password" {
                    self.i18n.t("ssh.form.password")
                } else {
                    label.to_owned()
                },
                input,
            ));
        }

        if let Some(remember) = status.remember {
            prompt_list = prompt_list.child(
                oxideterm_gpui_ui::checkbox_with(
                    self.tokens,
                    "ssh-prompt-save-password",
                    self.i18n.t("ssh.kbi.save_password_after_login"),
                    remember,
                    oxideterm_gpui_ui::CheckboxOptions {
                        disabled: timed_out,
                        ..Default::default()
                    },
                )
                .id("ssh-prompt-save-password")
                .on_mouse_down(
                    MouseButton::Left,
                    self.on_action(KeyboardAction::ToggleRemember),
                ),
            );
        }

        // Authentication fields redraw while waiting and typing. Avoid sampling
        // the entire window for blur on each of those frames.
        oxideterm_gpui_ui::modal::staged_overlay_root()
            .on_mouse_down(MouseButton::Left, self.on_action(KeyboardAction::Cancel))
            .child(oxideterm_gpui_ui::modal::staged_overlay_scrim(
                self.tokens,
                "keyboard-interactive-dialog-scrim",
                oxideterm_gpui_ui::modal::TauriBackdropRole::Dialog,
                true,
            ))
            .child(oxideterm_gpui_ui::motion::form_transition(
                self.tokens,
                "keyboard-interactive-dialog-transition",
                modal_container(self.tokens)
                    .bg(rgb(theme.bg_elevated))
                    .w(px(self.tokens.metrics.modal_width))
                    .rounded(px(self.tokens.radii.md))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(theme.border))
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .px(px(self.tokens.metrics.modal_header_padding_x))
                            .py(px(self.tokens.metrics.modal_header_padding_y))
                            .bg(rgb(theme.bg_panel))
                            // Browser DialogContent clips the painted header
                            // into the shell radius; keep native KBI prompts
                            // from exposing square top-corner pixels.
                            .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.md)))
                            .border_b_1()
                            .border_color(rgb(theme.border))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.modal_title_font_size))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(title),
                            )
                            .when(!status.instructions.trim().is_empty(), |header| {
                                header.child(
                                    div()
                                        .mt_1()
                                        .text_size(px(self
                                            .tokens
                                            .metrics
                                            .modal_description_font_size))
                                        .text_color(rgb(theme.text_muted))
                                        .child(status.instructions.to_owned()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .p(px(self.tokens.metrics.modal_body_padding))
                            .flex()
                            .flex_col()
                            .gap(px(self.tokens.metrics.modal_body_gap))
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.sm))
                                    .border_1()
                                    .border_color(if seconds_left <= 15 {
                                        rgba((theme.error << 8) | 0x80)
                                    } else {
                                        rgb(theme.border)
                                    })
                                    .bg(if seconds_left <= 15 {
                                        rgba((theme.error << 8) | 0x18)
                                    } else {
                                        rgba((theme.bg_hover << 8) | 0x80)
                                    })
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .text_size(px(self.tokens.metrics.form_text_font_size))
                                    .text_color(if seconds_left <= 15 {
                                        rgb(theme.error)
                                    } else {
                                        rgb(theme.text_muted)
                                    })
                                    .child(if timed_out {
                                        self.i18n.t("modals.kbi.timeout")
                                    } else {
                                        self.i18n
                                            .t("modals.kbi.time_remaining")
                                            .replace("{{seconds}}", &seconds_left.to_string())
                                    }),
                            )
                            .child(prompt_list),
                    )
                    .child(
                        div()
                            .h(px(self.tokens.metrics.modal_footer_height))
                            .px(px(self.tokens.metrics.modal_footer_padding_x))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .border_t_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_panel))
                            // Footer chrome is flush with the dialog bottom,
                            // so it owns the inner bottom corners too.
                            .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.md)))
                            .child(self.button(
                                self.i18n.t("ssh.form.cancel"),
                                ButtonVariant::Ghost,
                                false,
                                KeyboardAction::Cancel,
                            ))
                            .when(!external_wait, |footer| {
                                footer.child(self.button(
                                    self.i18n.t("ssh.kbi.continue"),
                                    ButtonVariant::Default,
                                    !can_submit,
                                    KeyboardAction::Submit,
                                ))
                            }),
                    ),
                status.visible,
            ))
            .into_any_element()
    }
}
