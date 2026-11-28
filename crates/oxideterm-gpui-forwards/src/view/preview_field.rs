use super::{ForwardsView, Mount, PreviewFocus};
use crate::ForwardInput;
use gpui::{prelude::*, *};
use oxideterm_gpui_editor::line_edit::{FieldEvent, LineField};
use oxideterm_theme::ThemeTokens;

struct Mounted {
    field: Entity<LineField>,
    _events: Subscription,
    _focus: Subscription,
}

#[derive(IntoElement)]
pub(crate) struct PreviewField {
    owner: WeakEntity<ForwardsView>,
    focus: PreviewFocus,
    input: ForwardInput,
    record: Option<String>,
    value: String,
    placeholder: String,
    tokens: ThemeTokens,
    font: SharedString,
}

impl PreviewField {
    pub(crate) fn new(
        view: &ForwardsView,
        focus: PreviewFocus,
        input: ForwardInput,
        placeholder: String,
        cx: &Context<ForwardsView>,
    ) -> Self {
        let state = view.state.read(cx);
        let record = match view.mount {
            Mount::Body => None,
            Mount::Dialogs => state.view.editing_forward.as_ref().map(|r| r.id.clone()),
        };
        Self {
            owner: cx.weak_entity(),
            focus,
            input,
            record,
            value: state.input_value(input).to_owned(),
            placeholder,
            tokens: view.tokens,
            font: view.forward_mono_font(),
        }
    }
}

impl RenderOnce for PreviewField {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        let input = self.input;
        let key = format!(
            "forward-{}-{}-{}",
            self.owner.entity_id(),
            input.anchor_key(),
            self.record.as_deref().unwrap_or("create")
        );
        let mounted = w.use_keyed_state(SharedString::from(key), cx, |w, cx| {
            let field =
                cx.new(|cx| LineField::new(self.tokens, self.placeholder.clone(), false, cx));
            let handle = field.read(cx).focus_handle(cx);
            let focus = self.focus.clone();
            let active = handle.clone();
            let on_focus = cx.on_focus(&handle, w, move |_, _, _| {
                *focus.borrow_mut() = Some(active.clone());
            });
            let owner = self.owner.clone();
            let record = self.record.clone();
            let events = cx.subscribe_in(&field, w, move |_, field, event, _, cx| {
                let _ = owner.update(cx, |v, cx| {
                    let state = v.state.read(cx);
                    let current = match &record {
                        Some(id) => state
                            .view
                            .editing_forward
                            .as_ref()
                            .is_some_and(|r| r.id == *id),
                        None => state.view.show_new_form,
                    };
                    if !current {
                        return;
                    }
                    let tab = v.frame().tab_id;
                    let node = v.frame().node_id.clone();
                    match event {
                        FieldEvent::Changed => {
                            let value = field.read(cx).text().to_owned();
                            v.state.update(cx, |s, cx| {
                                s.set_input_value(input, value);
                                cx.notify();
                            });
                        }
                        FieldEvent::Submit if record.is_some() => {
                            v.submit_forward_edit(tab, node, cx)
                        }
                        FieldEvent::Submit => v.submit_forward_create(tab, node, false, cx),
                        FieldEvent::Cancel if record.is_some() => {
                            v.begin_forward_edit_form_exit(cx);
                        }
                        FieldEvent::Cancel => {
                            v.begin_forward_create_form_exit(cx);
                        }
                    }
                });
            });
            Mounted {
                field,
                _events: events,
                _focus: on_focus,
            }
        });
        let field = mounted.read(cx).field.clone();
        field.update(cx, |f, cx| {
            f.environment(self.tokens, self.placeholder, cx);
            f.sync_value(&self.value, cx);
        });
        div()
            .w_full()
            .font_family(self.font)
            .debug_selector(move || format!("forward-input-{input:?}"))
            .child(field)
    }
}
