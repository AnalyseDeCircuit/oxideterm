use super::*;
use oxideterm_gpui_connections::session_manager::SessionManagerOpenTarget;
use oxideterm_gpui_editor::line_edit::{FieldEvent, LineField};
use oxideterm_gpui_settings_view::knowledge_page::dialog::{
    KnowledgeDialogAction as Action, KnowledgeDialogContent as Content,
};
use oxideterm_gpui_ui::{ConfirmDialogAction, select::*};
use oxideterm_settings_model::{KnowledgeDeleteTarget, SettingsInput};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Collection,
    Document,
    Delete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Name,
    Choice,
    Cancel,
    Confirm,
}

pub(super) struct Dialog {
    kind: Kind,
    field: Option<Entity<LineField>>,
    _events: Option<Subscription>,
    focus: Control,
    previous_focus: Option<FocusHandle>,
    highlighted: Option<usize>,
    scroll: ScrollHandle,
}

impl WebKnowledge {
    pub(crate) fn dialog_open(&self) -> bool {
        self.dialog.is_some()
    }
}

impl Workspace {
    pub(super) fn open_knowledge_dialog(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.knowledge.dialog.is_some() {
            return;
        }
        let page = self.knowledge.page.read(cx);
        let kind = if page.knowledge_create_dialog_open() {
            Kind::Collection
        } else if page.knowledge_document_dialog_owned_by(w.window_handle().window_id()) {
            Kind::Document
        } else if page.knowledge_delete_confirm().is_some() {
            Kind::Delete
        } else {
            return;
        };
        let previous_focus = w.focused(cx);
        let (field, events) = if kind == Kind::Delete {
            w.focus(&self.focus, cx);
            (None, None)
        } else {
            let placeholder = self.i18n.t(if kind == Kind::Document {
                "settings_view.knowledge.new_document_title_placeholder"
            } else {
                "settings_view.knowledge.collection_name_placeholder"
            });
            let field = cx.new(|cx| LineField::new(self.tokens, placeholder, false, cx));
            let events = cx.subscribe_in(&field, w, |h, _, event, w, cx| match event {
                FieldEvent::Changed => cx.notify(),
                FieldEvent::Submit => h.finish_knowledge_dialog(true, w, cx),
                FieldEvent::Cancel => h.finish_knowledge_dialog(false, w, cx),
            });
            w.focus(&field.read(cx).focus_handle(cx), cx);
            (Some(field), Some(events))
        };
        self.knowledge.active_input = None;
        self.knowledge.dialog = Some(Dialog {
            kind,
            field,
            _events: events,
            focus: if kind == Kind::Delete {
                Control::Cancel
            } else {
                Control::Name
            },
            previous_focus,
            highlighted: None,
            scroll: ScrollHandle::new(),
        });
        cx.notify();
    }

    fn knowledge_choices(&self, kind: Kind) -> Vec<(Option<String>, String)> {
        if kind == Kind::Document {
            [
                ("markdown", "settings_view.knowledge.format_markdown"),
                ("plaintext", "settings_view.knowledge.format_plain_text"),
            ]
            .into_iter()
            .map(|(id, key)| (Some(id.to_owned()), self.i18n.t(key)))
            .collect()
        } else {
            std::iter::once((None, self.i18n.t("settings_view.knowledge.scope_global")))
                .chain(
                    self.catalog_data
                        .items
                        .iter()
                        .filter_map(|r| match &r.target {
                            SessionManagerOpenTarget::Connection(id) => {
                                Some((Some(id.clone()), r.name.clone()))
                            }
                            _ => None,
                        }),
                )
                .collect()
        }
    }

    fn set_knowledge_choice(&mut self, value: Option<String>, cx: &mut Context<Self>) {
        let Some(d) = &mut self.knowledge.dialog else {
            return;
        };
        d.highlighted = None;
        self.knowledge.page.update(cx, |page, _| match d.kind {
            Kind::Document => {
                if let Some(v) = value {
                    page.set_knowledge_document_format(v);
                }
            }
            Kind::Collection => page.set_knowledge_collection_connection_id(value),
            Kind::Delete => {}
        });
        cx.notify();
    }

    pub(crate) fn render_knowledge_dialog(
        &self,
        w: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let d = self.knowledge.dialog.as_ref()?;
        let page = self.knowledge.page.read(cx);
        let (content, fields) = if d.kind == Kind::Delete {
            (Content::Delete(page.knowledge_delete_confirm()?), None)
        } else {
            let field = d.field.as_ref()?;
            let name = field.read(cx).text();
            let choices = self.knowledge_choices(d.kind);
            let current = if d.kind == Kind::Document {
                Some(page.knowledge_new_document_format())
            } else {
                page.knowledge_new_collection_connection_id()
            };
            let selected = choices
                .iter()
                .position(|(v, _)| v.as_deref() == current)
                .unwrap_or(0);
            let label = choices[selected].1.clone();
            let focused = d.focus == Control::Choice && self.focus.is_focused(w);
            let trigger = select_trigger_with_focus_visible(
                &self.tokens,
                label.clone(),
                false,
                false,
                focused,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |h, _, w, cx| {
                    if let Some(d) = &mut h.knowledge.dialog {
                        d.focus = Control::Choice;
                        d.highlighted = d.highlighted.is_none().then_some(selected);
                        d.scroll.scroll_to_item(selected);
                        w.focus(&h.focus, cx);
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            );
            let choice = div()
                .w_full()
                .child(trigger)
                .when_some(d.highlighted, |c, index| {
                    let popup = select_overlay_popup_with_max_height(&self.tokens, 320., 180.)
                        .track_scroll(&d.scroll)
                        .children(choices.into_iter().enumerate().map(|(i, (value, label))| {
                            select_option_action(
                                select_option_highlighted(
                                    &self.tokens,
                                    label,
                                    selected == i,
                                    i == index,
                                ),
                                false,
                                false,
                                cx.listener(move |h, _, _, cx| {
                                    h.set_knowledge_choice(value.clone(), cx);
                                    cx.stop_propagation();
                                }),
                            )
                        }));
                    c.child(deferred(
                        anchored().snap_to_window_with_margin(px(8.)).child(popup),
                    ))
                });
            let content = if d.kind == Kind::Document {
                Content::Document {
                    title: name,
                    format_label: label,
                    error: page.knowledge_new_document_error(),
                    phase: page.knowledge_document_presence().phase(),
                }
            } else {
                Content::Collection {
                    name,
                    scope_label: label,
                    phase: page.knowledge_create_presence().phase(),
                }
            };
            (
                content,
                Some((field.clone().into_any_element(), choice.into_any_element())),
            )
        };
        let owner = cx.weak_entity();
        let window = w.window_handle();
        let dispatch = Rc::new(move |action, cx: &mut App| {
            let owner = owner.clone();
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, w, cx| {
                    let _ = owner.update(cx, |h, cx| {
                        h.finish_knowledge_dialog(matches!(action, Action::Confirm), w, cx)
                    });
                });
            });
        });
        let focus = match d.focus {
            Control::Cancel => Some(ConfirmDialogAction::Cancel),
            Control::Confirm => Some(ConfirmDialogAction::Confirm),
            _ => None,
        };
        Some(self.settings.read(cx).knowledge_dialog(
            &self.tokens,
            content,
            fields,
            focus,
            dispatch,
            cx,
        ))
    }

    fn finish_knowledge_dialog(&mut self, commit: bool, w: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = &self.knowledge.dialog else {
            return;
        };
        let kind = d.kind;
        let mut created = None;
        let mut deleted = None;
        let done = self.knowledge.page.update(cx, |page, cx| {
            if kind == Kind::Delete {
                let Some(target) = page.take_knowledge_delete_confirm() else {
                    return true;
                };
                if commit {
                    let message = self.i18n.t("settings_view.knowledge.error_delete");
                    let ok = match target.target {
                        KnowledgeDeleteTarget::Document => {
                            page.delete_knowledge_document(&target.id, message)
                        }
                        KnowledgeDeleteTarget::Collection => {
                            page.delete_knowledge_collection(&target.id, message)
                        }
                    };
                    if ok {
                        deleted = Some(target);
                    }
                }
            } else {
                let document = kind == Kind::Document;
                if commit {
                    let name = d.field.as_ref().unwrap().read(cx).text().to_owned();
                    if name.trim().is_empty() {
                        return false;
                    }
                    let input = if document {
                        SettingsInput::KnowledgeDocumentTitle
                    } else {
                        SettingsInput::KnowledgeCollectionName
                    };
                    page.focus_settings_input(input, cx);
                    let len = page
                        .settings_input_value(input)
                        .unwrap_or_default()
                        .encode_utf16()
                        .count();
                    page.replace_settings_input(input, Some(0..len), &name, cx);
                    if document {
                        created = page.create_blank_knowledge_document(
                            self.i18n.t("settings_view.knowledge.error_create_document"),
                        );
                        if created.is_none() {
                            return false;
                        }
                    } else if !page.create_knowledge_collection(
                        self.i18n
                            .t("settings_view.knowledge.error_create_collection"),
                    ) {
                        return false;
                    }
                }
                if document {
                    page.close_knowledge_document_dialog(std::time::Duration::ZERO, cx);
                } else {
                    page.close_knowledge_create_dialog(std::time::Duration::ZERO, cx);
                }
            }
            true
        });
        if !done {
            cx.notify();
            return;
        }
        if let Some(focus) = self.knowledge.dialog.take().and_then(|d| d.previous_focus) {
            w.focus(&focus, cx);
        } else {
            w.focus(&self.focus, cx);
        }
        let views = &self.knowledge.views;
        if let Some((doc, open)) = created {
            let id = doc.id.clone();
            views
                .state
                .update(cx, |s, _| s.insert_created_document(doc));
            if open {
                views
                    .view
                    .update(cx, |v, cx| v.select_knowledge_document(id, cx));
            }
        }
        if let Some(target) = deleted {
            views.state.update(cx, |s, cx| match target.target {
                KnowledgeDeleteTarget::Collection => s.remove_collection(&target.id, cx),
                KnowledgeDeleteTarget::Document => s.remove_document(&target.id),
            });
        }
        if commit {
            views.refresh(cx);
        }
        cx.notify();
    }

    pub(crate) fn knowledge_dialog_key(
        &mut self,
        e: &KeyDownEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(d) = &self.knowledge.dialog else {
            return;
        };
        let field_focused = d
            .field
            .as_ref()
            .is_some_and(|v| v.read(cx).focus_handle(cx).is_focused(w));
        let focus = if field_focused {
            Control::Name
        } else {
            d.focus
        };
        let key = e.keystroke.key.as_str();
        if field_focused && key != "tab" {
            return;
        }
        if key == "tab" {
            let controls: &[Control] = if d.kind == Kind::Delete {
                &[Control::Cancel, Control::Confirm]
            } else {
                &[
                    Control::Name,
                    Control::Choice,
                    Control::Cancel,
                    Control::Confirm,
                ]
            };
            let i = controls.iter().position(|c| *c == focus).unwrap_or(0);
            let next = controls[(i + if e.keystroke.modifiers.shift {
                controls.len() - 1
            } else {
                1
            }) % controls.len()];
            let d = self.knowledge.dialog.as_mut().unwrap();
            d.highlighted = None;
            d.focus = next;
            if next == Control::Name {
                w.focus(&d.field.as_ref().unwrap().read(cx).focus_handle(cx), cx);
            } else {
                w.focus(&self.focus, cx);
            }
        } else if key == "escape" && d.highlighted.is_none() {
            self.finish_knowledge_dialog(false, w, cx);
        } else if focus == Control::Choice {
            let choices = self.knowledge_choices(d.kind);
            let page = self.knowledge.page.read(cx);
            let selected = if d.kind == Kind::Document {
                Some(page.knowledge_new_document_format())
            } else {
                page.knowledge_new_collection_connection_id()
            };
            let current = choices
                .iter()
                .position(|(v, _)| v.as_deref() == selected)
                .unwrap_or(0);
            let index = d.highlighted.unwrap_or(current).min(choices.len() - 1);
            if matches!(key, "enter" | "space" | " ") && d.highlighted.is_some() {
                self.set_knowledge_choice(choices[index].0.clone(), cx);
            } else {
                let d = self.knowledge.dialog.as_mut().unwrap();
                d.highlighted = match key {
                    "escape" => None,
                    "enter" | "space" | " " => Some(current),
                    "up" => Some(browser_select_next_index(
                        index,
                        choices.len(),
                        BrowserSelectKeyDirection::Previous,
                    )),
                    "down" => Some(browser_select_next_index(
                        index,
                        choices.len(),
                        BrowserSelectKeyDirection::Next,
                    )),
                    "home" => Some(0),
                    "end" => Some(choices.len() - 1),
                    _ => return,
                };
                if let Some(i) = d.highlighted {
                    d.scroll.scroll_to_item(i);
                }
            }
        } else if matches!(key, "enter" | "space" | " ") {
            self.finish_knowledge_dialog(focus == Control::Confirm, w, cx);
        } else {
            return;
        }
        w.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
}
