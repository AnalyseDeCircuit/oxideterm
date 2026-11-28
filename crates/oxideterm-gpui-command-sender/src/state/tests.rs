use gpui::TestAppContext;
use oxideterm_theme::default_tokens;

use super::*;

fn test_context_menu_labels() -> EditorContextMenuLabels {
    EditorContextMenuLabels {
        copy: "Copy".to_string(),
        cut: "Cut".to_string(),
        paste: "Paste".to_string(),
        select_all: "Select all".to_string(),
    }
}

#[gpui::test]
fn expanded_panel_starts_the_task_selected_in_the_strip(cx: &mut TestAppContext) {
    use crate::{ExpandedSender, SenderAction, SenderPanelFrame};
    use gpui::{IntoElement, ParentElement, Render, Styled, Window, div, px, rgb};
    struct Panel(
        Entity<TerminalCommandSenderEntity>,
        Rc<Cell<Option<TerminalCommandSenderId>>>,
    );
    impl Render for Panel {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let selection = self.1.clone();
            let frame = SenderPanelFrame {
                tokens: default_tokens(),
                mono_font: "monospace".into(),
                divider: rgb(0xaaaaaa),
                has_background: false,
                current: "test".into(),
                targets: vec![],
                groups: vec![],
            };
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(ExpandedSender::new(
                    self.0.clone(),
                    frame,
                    oxideterm_i18n::I18n::default(),
                    move |a, _, _| {
                        if let SenderAction::Start(id) = a {
                            selection.set(Some(id));
                        }
                    },
                ))
        }
    }
    let start = Rc::new(Cell::new(None));
    let model = cx.new(|cx| {
        TerminalCommandSenderEntity::new(
            default_tokens(),
            "Input".into(),
            "Commands".into(),
            test_context_menu_labels(),
            cx,
        )
    });
    let first = model.update(cx, |m, cx| {
        let first = m.active_document_id();
        m.set_expanded(true, cx);
        m.add_document(cx);
        first
    });
    let (_, w) = cx.add_window_view(|_, _| Panel(model.clone(), start.clone()));
    w.simulate_resize(gpui::size(px(900.), px(450.)));
    w.update(|w, app| w.draw(app).clear(app));
    let tab = w.debug_bounds("sender-task-1").unwrap();
    w.simulate_click(tab.center(), Default::default());
    w.update(|w, app| w.draw(app).clear(app));
    let run = w.debug_bounds("sender-run-action").unwrap();
    assert!(run.top() > tab.bottom());
    assert_eq!(start.get(), None);
    w.simulate_click(run.center(), Default::default());
    assert_eq!(start.get(), Some(first));
}

#[gpui::test]
fn selected_target_receives_raw_frames_until_the_original_task_stops(cx: &mut TestAppContext) {
    let sender = cx.new(|cx| {
        TerminalCommandSenderEntity::new(
            default_tokens(),
            "Command".into(),
            "Commands".into(),
            test_context_menu_labels(),
            cx,
        )
    });
    let writes = Rc::new(std::cell::RefCell::new(Vec::new()));
    let candidates = [PaneId(8), PaneId(9)].map(|pane_id| {
        let writes = writes.clone();
        TerminalCommandSenderTarget {
            pane_id,
            write: Rc::new(move |frame, _| {
                let TerminalSenderFrame::RawBytes(bytes) = frame else {
                    panic!("expected raw bytes")
                };
                writes.borrow_mut().push((pane_id, bytes.to_vec()));
                Some(true)
            }),
        }
    });
    let id = sender.update(cx, |s, cx| {
        let id = s.replace_active_text("0d 00\nff".into(), cx);
        s.set_target_scope(id, TerminalCommandSenderTargetScope::Selected, cx);
        s.toggle_selected_target(id, PaneId(9), cx);
        s.set_input_mode(id, TerminalSenderInputMode::Hex, cx);
        assert!(writes.borrow().is_empty());
        assert!(s.start(id, Some(PaneId(8)), candidates.into(), cx));
        s.add_document(cx);
        id
    });
    cx.run_until_parked();
    assert_eq!(&*writes.borrow(), &[(PaneId(9), vec![13, 0])]);
    sender.update(cx, |s, cx| assert!(s.stop(id, cx)));
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    assert_eq!(&*writes.borrow(), &[(PaneId(9), vec![13, 0])]);
}

#[test]
fn sender_height_reserves_terminal_space_in_the_measured_surface() {
    for (available, requested, delta, expected) in [
        (800.0, 280.0, 0.0, 280.0),
        (800.0, 280.0, -1000.0, 520.0),
        (400.0, 280.0, 0.0, 220.0),
        (200.0, 280.0, 0.0, 20.0),
        (120.0, 280.0, 0.0, 0.0),
        (0.0, 280.0, 0.0, 0.0),
    ] {
        assert_eq!(
            adjusted_sender_panel_height(requested, delta, available),
            expected
        );
    }
    for available in [180.0, 200.0, 400.0] {
        assert_eq!(
            TerminalCommandSenderLayout::Compact.panel_height(280.0, available),
            oxideterm_gpui_ui::WORKSPACE_STATUS_BAR_HEIGHT,
            "compact input must remain stable while the adjacent dock adjusts its height",
        );
    }
    assert_eq!(
        TerminalCommandSenderLayout::Compact.panel_height(280.0, 12.0),
        12.0
    );
    assert_eq!(
        TerminalCommandSenderLayout::Hidden.panel_height(280.0, 400.0),
        0.0
    );
}

#[test]
fn plan_errors_map_without_retaining_input_details() {
    assert_eq!(
        plan_failure(TerminalSenderPlanError::InvalidHexDigit),
        TerminalCommandSenderFailure::InvalidHex
    );
    assert_eq!(
        plan_failure(TerminalSenderPlanError::EmptyInput),
        TerminalCommandSenderFailure::EmptyInput
    );
}

#[gpui::test]
fn compact_and_expanded_inputs_sync_only_when_layout_changes(cx: &mut TestAppContext) {
    let sender = cx.new(|cx| {
        TerminalCommandSenderEntity::new(
            default_tokens(),
            "Command".to_string(),
            "Commands".to_string(),
            test_context_menu_labels(),
            cx,
        )
    });
    sender.update(cx, |sender, cx| {
        sender.replace_active_compact_text("echo compact".to_string(), cx);
        sender.set_expanded(true, cx);
    });
    let editor = sender
        .read_with(cx, |sender, _cx| {
            sender
                .active_document_snapshot()
                .map(|snapshot| snapshot.editor)
        })
        .expect("active sender editor");
    assert_eq!(
        editor.read_with(cx, |editor, _cx| editor.buffer().text()),
        "echo compact"
    );

    editor.update(cx, |editor, cx| {
        editor.replace_text_external("echo expanded".to_string(), cx);
    });
    sender.update(cx, |sender, cx| sender.set_expanded(false, cx));
    sender.read_with(cx, |sender, _cx| {
        assert_eq!(sender.active_compact_draft(), Some("echo expanded"));
    });
}

#[gpui::test]
fn multiline_text_auto_expands_compact_sender_and_preserves_lines(cx: &mut TestAppContext) {
    let sender = cx.new(|cx| {
        TerminalCommandSenderEntity::new(
            default_tokens(),
            "Command".to_string(),
            "Commands".to_string(),
            test_context_menu_labels(),
            cx,
        )
    });

    sender.update(cx, |sender, cx| {
        sender.replace_active_text("echo single".to_string(), cx);
        assert!(!sender.is_expanded());
        assert_eq!(sender.active_compact_draft(), Some("echo single"));
    });

    let multiline = "vllm serve /models/qwen \\\n  --host 0.0.0.0 \\\n  --port 1234";
    sender.update(cx, |sender, cx| {
        sender.replace_active_text(multiline.to_string(), cx);
        assert!(sender.is_expanded());
    });
    let editor = sender
        .read_with(cx, |sender, _cx| {
            sender
                .active_document_snapshot()
                .map(|snapshot| snapshot.editor)
        })
        .expect("active sender editor");
    assert_eq!(
        editor.read_with(cx, |editor, _cx| editor.buffer().text()),
        multiline
    );

    sender.update(cx, |sender, cx| {
        sender.set_expanded(false, cx);
        sender.replace_active_text("docker run".to_string(), cx);
        assert!(!sender.is_expanded());
        sender.append_active_text("--rm \\\n  alpine", true, cx);
        assert!(sender.is_expanded());
    });
    assert_eq!(
        editor.read_with(cx, |editor, _cx| editor.buffer().text()),
        "docker run --rm \\\n  alpine"
    );
}

#[gpui::test]
fn compact_suggestion_selection_starts_at_the_directional_edge(cx: &mut TestAppContext) {
    let sender = cx.new(|cx| {
        TerminalCommandSenderEntity::new(
            default_tokens(),
            "Command".to_string(),
            "Commands".to_string(),
            test_context_menu_labels(),
            cx,
        )
    });

    sender.update(cx, |sender, cx| {
        let mode = CompactSuggestionMode::History;
        assert_eq!(
            sender.move_compact_suggestion_selection(3, true, mode, cx),
            Some(0)
        );
        assert_eq!(
            sender.move_compact_suggestion_selection(3, true, mode, cx),
            Some(1)
        );
        assert_eq!(
            sender.move_compact_suggestion_selection(3, false, mode, cx),
            Some(0)
        );
        sender.dismiss_compact_suggestions();
        assert_eq!(
            sender.move_compact_suggestion_selection(3, false, mode, cx),
            Some(2)
        );
    });
}
