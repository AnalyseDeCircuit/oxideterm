use super::*;
use gpui::{AppContext, Entity, IntoElement, RenderOnce, Subscription, WeakEntity, Window};
use oxideterm_gpui_editor::line_edit::{FieldEvent, LineField};

// Numeric parameter indices are reused by the manager and by execution prompts.
// Scope the editor and late blur commits to the draft that created the field.
fn field_id(state: &TerminalQuickCommandsState, input: QuickCommandInput, secret: bool) -> u64 {
    let record = if let Some(run) = &state.pending_execution {
        ("execute", Some(run.command.id.as_str()))
    } else if let Some(draft) = &state.store.command_editor {
        ("command", draft.id.as_deref())
    } else if let Some(draft) = &state.store.category_editor {
        ("category", draft.id.as_deref())
    } else {
        ("search", None)
    };
    selection_id("quick-field", (record, input, secret))
}

struct Mounted {
    field: Entity<LineField>,
    requested_focus: bool,
    _events: Subscription,
    _focus: Subscription,
}

#[derive(IntoElement)]
pub(super) struct QuickField {
    owner: WeakEntity<QuickCommandsView>,
    access: QuickCommandsAccess,
    tokens: oxideterm_theme::ThemeTokens,
    input: QuickCommandInput,
    value: Zeroizing<String>,
    placeholder: String,
    focused: bool,
    secret: bool,
    mono_font: Option<SharedString>,
}

impl QuickField {
    pub(super) fn new(
        view: &QuickCommandsView,
        input: QuickCommandInput,
        value: &str,
        focused: bool,
        placeholder: String,
        secret: bool,
        cx: &Context<QuickCommandsView>,
    ) -> Self {
        Self {
            owner: cx.weak_entity(),
            access: view.state.clone(),
            tokens: view.tokens,
            input,
            value: Zeroizing::new(value.to_owned()),
            placeholder,
            focused,
            secret,
            mono_font: quick_command_input_uses_monospace(input).then(|| view.mono_font.clone()),
        }
    }
}

impl RenderOnce for QuickField {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let input = self.input;
        let secret = self.secret;
        let id = field_id(self.access.read(cx), input, secret);
        let mounted = w.use_keyed_state(("quick-field", id), cx, |w, cx| {
            let field = cx.new(|cx| {
                let f = LineField::new(self.tokens, self.placeholder.clone(), false, cx);
                if secret { f.masked() } else { f }
            });
            let owner = self.owner.clone();
            let focus = field.read(cx).focus_handle(cx);
            let on_focus = cx.on_focus(&focus, w, move |_, _, cx| {
                let _ = owner.update(cx, |v, cx| {
                    v.state.update(cx, |s, _| s.set_focused_input(input));
                    cx.notify();
                });
            });
            let owner = self.owner.clone();
            let events = cx.subscribe_in(&field, w, move |_, field, event, w, cx| {
                let _ = owner.update(cx, |v, cx| {
                    if field_id(v.state.read(cx), input, secret) != id {
                        return;
                    }
                    match event {
                        FieldEvent::Changed => {
                            v.state.update(cx, |state, cx| {
                                state.commit_field(input, field.read(cx).text())
                            });
                        }
                        FieldEvent::Cancel => {
                            v.state.update(cx, |s, _| s.blur_input());
                            w.focus(&v.focus, cx);
                            if !v.manager && input == QuickCommandInput::Search {
                                v.close_terminal_quick_commands_panel(cx);
                                v.dispatch(QuickCommandsRequest::FocusPane, Some(w), cx);
                            }
                        }
                        FieldEvent::Submit
                            if matches!(
                                input,
                                QuickCommandInput::CategoryName | QuickCommandInput::CategoryColor
                            ) =>
                        {
                            v.save_quick_command_category_editor(cx)
                        }
                        FieldEvent::Submit if v.state.read(cx).store.command_editor.is_some() => {
                            v.save_quick_command_editor(cx)
                        }
                        FieldEvent::Submit => {}
                    }
                    cx.notify();
                });
            });
            Mounted {
                field,
                requested_focus: false,
                _events: events,
                _focus: on_focus,
            }
        });
        let field = mounted.update(cx, |mounted, cx| {
            mounted.field.update(cx, |f, cx| {
                f.environment(self.tokens, self.placeholder, cx);
                f.sync_value(&self.value, cx);
            });
            if self.focused && !mounted.requested_focus {
                w.focus(&mounted.field.read(cx).focus_handle(cx), cx);
            }
            mounted.requested_focus = self.focused;
            mounted.field.clone()
        });
        div()
            .h(px(32.))
            .w_full()
            .when_some(self.mono_font, |e, font| e.font_family(font))
            .child(field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Render, TestAppContext};
    use std::rc::Rc;

    struct Manager(Entity<QuickCommandsView>);
    impl Render for Manager {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0.clone()
        }
    }

    #[gpui::test]
    fn manager_replaces_selected_text_in_the_focused_draft(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let state = cx.new(|_| {
            let mut s = TerminalQuickCommandsState::load(&dir.path().join("settings.json"));
            s.open_manager();
            s.start_category_create();
            s.store.query = "filter".into();
            s.store.category_editor.as_mut().unwrap().name = "A🚀B".into();
            s
        });
        let (_, window) = cx.add_window_view(|_, cx| {
            let mut tokens = oxideterm_theme::default_tokens();
            tokens.apply_motion(oxideterm_theme::UiMotionProfile::Off);
            let frame = QuickCommandsFrame {
                tokens,
                i18n: I18n::default(),
                ui_font: "sans-serif".into(),
                mono_font: "monospace".into(),
                enabled: true,
                focus: cx.focus_handle(),
                has_background: false,
                caret_visible: true,
                target: None,
                execution_targets: vec![],
            };
            let current = frame.clone();
            Manager(
                QuickCommandsViews::with_local_inputs(
                    QuickCommandsAccess::new(state.downgrade(), |s| s, |s| s),
                    Rc::new(move |_| Some(current.clone())),
                    Rc::new(|_, _, _| {}),
                    Rc::new(|text, _| div().child(text.text).into_any_element()),
                    frame,
                    cx,
                )
                .manager,
            )
        });
        window.update(|w, app| w.draw(app).clear(app));
        window.simulate_keystrokes("left shift-left");
        window.simulate_input("界");
        state.read_with(window, |s, _| {
            assert_eq!(s.input_value(QuickCommandInput::CategoryName), Some("A界B"));
            assert_eq!(s.store.query, "filter");
        });
    }
}
