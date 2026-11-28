use crate::data::{ForwardRule, ForwardStatus, NodeId};
use crate::*;
use gpui::{
    AppContext, Context, Entity, Render, TestAppContext, VisualTestContext, Window, div,
    prelude::*, px,
};
use oxideterm_i18n::{I18n, Locale};
use oxideterm_workspace::TabId;
use std::{cell::Cell, rc::Rc};

struct TestPage(ForwardsViews);

impl Render for TestPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .child(self.0.body())
            .child(self.0.dialogs())
    }
}

fn open(
    cx: &mut TestAppContext,
    id: u64,
    preview: Option<Rc<Cell<Option<u16>>>>,
) -> (Entity<TestPage>, &mut VisualTestContext) {
    let tab = TabId(id);
    cx.add_window_view(|_, cx| {
        let state = cx.new(|_| ForwardsPageState::new());
        let bindings = ForwardsViewBindings {
            frame: Rc::new(move |_| {
                Some(ForwardsFrame {
                    tab_id: tab,
                    node_id: NodeId::new("forward-view-test"),
                    node_ready: true,
                    can_save: false,
                    runtime: ForwardingRuntimeSnapshot {
                        rules: vec![ForwardRule {
                            status: ForwardStatus::Stopped,
                            ..ForwardRule::local("localhost", 8081, "localhost", 8080)
                        }],
                        ..Default::default()
                    },
                    ports: PortDetectionViewState::default(),
                    tokens: oxideterm_theme::default_tokens(),
                    i18n: I18n::new(Locale::En),
                    has_background: false,
                    ui_font: "sans-serif".into(),
                    cjk_font: "sans-serif".into(),
                    mono_font: "monospace".into(),
                })
            }),
            ime: Rc::new(|_, _| None),
            dispatch: Rc::new(|_, _, _| {}),
        };
        if let Some(submitted) = preview {
            TestPage(ForwardsViews::configuration_preview(
                state,
                move |cx| (bindings.frame)(cx),
                move |req, _, _| {
                    if let ForwardsRequest::Operation {
                        operation: ForwardingRuntimeOperation::Update { update, .. },
                        ..
                    } = req
                    {
                        submitted.set(update.bind_port);
                    }
                },
                cx,
            ))
        } else {
            TestPage(ForwardsViews::new(state, bindings, cx))
        }
    })
}

#[gpui::test]
fn draft_only_rule_edits_selected_port_digits_without_offering_start(cx: &mut TestAppContext) {
    let submitted = Rc::new(Cell::new(None));
    let (_, window) = open(cx, 9, Some(submitted.clone()));
    window.simulate_resize(gpui::size(px(1000.), px(720.)));
    window.update(|w, cx| w.draw(cx).clear(cx));
    assert!(
        window
            .debug_bounds("forward-icon-lucide/play.svg")
            .is_none()
    );
    let edit = window
        .debug_bounds("forward-icon-lucide/pencil.svg")
        .unwrap();
    window.simulate_click(edit.center(), Default::default());
    window.update(|w, cx| w.draw(cx).clear(cx));
    let port = window.debug_bounds("forward-input-EditBindPort").unwrap();
    window.simulate_click(port.center(), Default::default());
    window.simulate_keystrokes("end shift-left shift-left");
    window.simulate_input("24");
    window.simulate_keystrokes("enter");
    assert_eq!(submitted.get(), Some(8024));
}

#[gpui::test]
fn edit_dialog_is_notified_only_in_its_own_window(cx: &mut TestAppContext) {
    let (a, a_window) = open(cx, 1, None);
    let mut a_window = a_window.clone();
    let (b, b_window) = open(cx, 2, None);
    let mut b_window = b_window.clone();
    let (a_state, a_dialogs) = a.read_with(&a_window, |view, _| {
        (view.0.state.clone(), view.0.dialogs.clone())
    });
    let (b_state, b_dialogs) = b.read_with(&b_window, |view, _| {
        (view.0.state.clone(), view.0.dialogs.clone())
    });
    for window in [&mut a_window, &mut b_window] {
        window.simulate_resize(gpui::size(px(1000.), px(720.)));
        window.update(|window, cx| window.draw(cx).clear(cx));
        assert!(window.debug_bounds("forward-edit-scroll").is_none());
    }
    let (a_dirty, b_dirty, root_dirty) = (
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(false)),
    );
    let _observations = cx.update(|cx| {
        let (a_dirty, b_dirty, root_dirty) = (a_dirty.clone(), b_dirty.clone(), root_dirty.clone());
        (
            cx.observe(&a_dialogs, move |_, _| a_dirty.set(true)),
            cx.observe(&b_dialogs, move |_, _| b_dirty.set(true)),
            cx.observe(&a, move |_, _| root_dirty.set(true)),
        )
    });

    let edit = a_window
        .debug_bounds("forward-icon-lucide/pencil.svg")
        .expect("editable forward");
    a_window.simulate_click(edit.center(), gpui::Modifiers::default());
    assert!(a_dirty.get());
    assert!(!b_dirty.get());
    assert!(!root_dirty.get());
    a_window.update(|window, cx| window.draw(cx).clear(cx));
    assert!(a_window.debug_bounds("forward-edit-scroll").is_some());

    b_window.update(|window, cx| window.draw(cx).clear(cx));
    assert!(b_window.debug_bounds("forward-edit-scroll").is_none());
    assert_eq!(
        b_state.read_with(&b_window, |state, _| state
            .input_value(ForwardInput::EditBindPort)
            .to_owned()),
        ""
    );
    assert_eq!(
        a_state.read_with(&b_window, |state, _| state
            .input_value(ForwardInput::EditBindPort)
            .to_owned()),
        "8081"
    );

    // Completing the exit only unmounts the overlay for the initiating tab.
    a_state.update(&mut b_window, |state, cx| {
        state.begin_edit_form_exit(std::time::Duration::ZERO, cx);
    });
    assert!(!a_state.read_with(&b_window, |state, _| state.edit_form_open()));
}
