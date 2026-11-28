use std::{cell::RefCell, rc::Rc};

use gpui::{
    AppContext, Bounds, ClipboardItem, InputHandler, Modifiers, TestAppContext, point, px, size,
};

use super::{TerminalView, TerminalViewEvent, ime::TerminalViewInputHandler};
use crate::tests::test_metrics;

#[gpui::test]
fn pane_search_navigates_from_scrollback_without_sending_query_or_keys(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, cx| TerminalView::new(Default::default(), test_metrics(), cx));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|window, cx| {
        let writes = sent.clone();
        let sub = cx.subscribe(&view, move |_, event, _| {
            if let TerminalViewEvent::SendBytes(bytes) = event {
                writes.borrow_mut().extend_from_slice(bytes);
            }
        });
        view.update(cx, |v, cx| {
            v.feed_bytes("界foo\r\n".as_bytes(), cx);
            v.feed_bytes(&b"FOO\r\n".repeat(150), cx);
            v.feed_bytes(b"end foo", cx);
            v.open_search(&Default::default(), window, cx);
        });
        sub
    });
    cx.simulate_input("foo");
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        let s = v.search.as_ref().unwrap();
        let coords: Vec<_> = s.matches.iter().map(|m| (m.start_col, m.end_col)).collect();
        assert_eq!(coords, [(2, 5), (4, 7)]);
        assert_eq!(v.snapshot().display_offset as i32, -s.matches[0].line);
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |v, _| assert_eq!(v.snapshot().display_offset, 0));
    cx.simulate_keystrokes("shift-enter escape");
    cx.update(|window, cx| {
        view.read_with(cx, |v, cx| {
            assert!(v.snapshot().display_offset > 0);
            assert!(gpui::Focusable::focus_handle(v, cx).is_focused(window));
        })
    });
    assert_eq!(sent.borrow().as_slice(), b"");
}

#[gpui::test]
fn peer_receives_protocol_replies_in_order_without_local_echo(cx: &mut TestAppContext) {
    let view = cx.new(|cx| TerminalView::new(Default::default(), test_metrics(), cx));
    let events = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|cx| {
        let events = events.clone();
        cx.subscribe(&view, move |_, event, _| {
            events.borrow_mut().push(match event {
                TerminalViewEvent::SendBytes(bytes) => ("write", bytes.clone()),
                TerminalViewEvent::TitleChanged(Some(title)) => {
                    ("title", title.as_bytes().to_vec())
                }
                TerminalViewEvent::TitleChanged(None) => ("reset", Vec::new()),
                TerminalViewEvent::Bell => ("bell", Vec::new()),
                _ => panic!("unexpected host event"),
            });
        })
    });
    view.update(cx, |view, cx| {
        view.feed_bytes(b"\x1b[22;0t\x1b]2;session\x07\x07\xe7", cx);
        view.feed_bytes(b"\x95\x8c\x1b[6", cx);
        view.feed_bytes(b"n\x1b[23;0t", cx);
        view.send_text("next", cx);
        assert_eq!(view.snapshot().lines[0].text().trim_end(), "界");
    });
    assert_eq!(
        *events.borrow(),
        [
            ("title", b"session".to_vec()),
            ("bell", vec![]),
            ("write", b"\x1b[1;3R".to_vec()),
            ("reset", vec![]),
            ("write", b"next".to_vec()),
        ]
    );
}

#[gpui::test]
fn changing_terminal_preferences_trims_primary_history_and_gates_peer_clipboard(
    cx: &mut TestAppContext,
) {
    let view = cx.new(|cx| TerminalView::new(Default::default(), test_metrics(), cx));
    view.update(cx, |view, cx| {
        let mut settings = oxideterm_settings::TerminalSettings::default();
        view.feed_bytes(&b"row\r\n".repeat(180), cx);
        view.feed_bytes(b"\x1b[?1049h", cx);
        settings.scrollback = 100;
        settings.osc52_clipboard = false;
        view.apply_preferences((&settings).into(), cx);
        cx.write_to_clipboard(ClipboardItem::new_string("original".into()));
        view.feed_bytes(b"\x1b]52;c;bmV3\x07\x1b[?1049l", cx);
        assert_eq!(view.snapshot().scrollback_lines, 100);
        assert_eq!(
            cx.read_from_clipboard().and_then(|v| v.text()).as_deref(),
            Some("original")
        );
        settings.osc52_clipboard = true;
        view.apply_preferences((&settings).into(), cx);
        view.feed_bytes(b"\x1b]52;c;bmV3\x07", cx);
        assert_eq!(
            cx.read_from_clipboard().and_then(|v| v.text()).as_deref(),
            Some("new")
        );
    });
}

#[gpui::test]
fn ime_replaces_utf16_preedit_before_commit_and_platform_paste_keeps_boundaries(
    cx: &mut TestAppContext,
) {
    let (view, cx) =
        cx.add_window_view(|_, cx| TerminalView::new(Default::default(), test_metrics(), cx));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let review = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|_, cx| {
        let sent = sent.clone();
        let review = review.clone();
        cx.subscribe(&view, move |_, event, _| {
            if let TerminalViewEvent::SendBytes(bytes) = event {
                sent.borrow_mut().push(bytes.clone());
            } else if let TerminalViewEvent::ReviewPaste(text) = event {
                review.borrow_mut().push(text.to_string());
            }
        })
    });
    view.update(cx, |view, cx| {
        view.feed_bytes(b"\x1b[?1049h\x1b[?25l\x1b[2;3H\x1b[?2004h", cx)
    });
    cx.update(|window, cx| {
        let bounds = Bounds::new(point(px(7.0), px(9.0)), size(px(320.0), px(200.0)));
        let mut handler = TerminalViewInputHandler::new(bounds, view.clone());
        handler.replace_and_mark_text_in_range(None, "a😀z", Some(1..3), window, cx);
        handler.replace_and_mark_text_in_range(Some(1..3), "界", Some(1..1), window, cx);
        assert_eq!(handler.marked_text_range(window, cx), Some(0..3));
        assert_eq!(
            handler.bounds_for_range(2..2, window, cx).unwrap().origin,
            point(px(47.0), px(19.0))
        );
        handler.replace_text_in_range(Some(2..3), "!", window, cx);
        handler.paste(ClipboardItem::new_string("one\r\ntwo".into()), window, cx);
        view.update(cx, |view, cx| {
            let p = oxideterm_settings::TerminalSettings {
                paste_protection: true,
                ..Default::default()
            };
            view.apply_preferences((&p).into(), cx);
        });
        handler.paste(ClipboardItem::new_string("three\rfour".into()), window, cx);
    });
    assert_eq!(
        *sent.borrow(),
        ["a界!".as_bytes(), b"\x1b[200~one\ntwo\x1b[201~"]
    );
    assert_eq!(review.borrow().as_slice(), ["three\rfour"]);
}

#[gpui::test]
fn shift_click_copies_across_pages_without_moving_the_viewport(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, cx| TerminalView::new(Default::default(), test_metrics(), cx));
    cx.simulate_resize(size(px(80.0), px(20.0)));
    cx.update(|window, app| {
        window.draw(app).clear(app);
        window.simulate_next_frame(app);
    });
    view.update(cx, |view, cx| {
        view.feed_bytes(b"aa\r\nbb\r\ncc\r\ndd\r\nee", cx)
    });
    cx.simulate_click(point(px(12.0), px(15.0)), Modifiers::default());
    cx.simulate_keystrokes("shift-home");
    cx.simulate_click(point(px(12.0), px(5.0)), Modifiers::shift());
    view.update(cx, |view, cx| {
        assert_eq!(view.selected_text().as_deref(), Some("a\nbb\ncc\ndd\nee"));
        view.feed_bytes(b"!", cx);
        assert_eq!(view.snapshot().lines[0].text().trim_end(), "aa");
    });
}

#[gpui::test]
fn configured_history_key_replaces_default_without_writing_to_the_peer(cx: &mut TestAppContext) {
    use crate::input::{TerminalKeybindings, TerminalShortcut};
    use gpui::{Focusable, KeyBinding, NoAction};
    let (view, cx) =
        cx.add_window_view(|_, cx| TerminalView::new(Default::default(), test_metrics(), cx));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|w, cx| {
        cx.set_global(TerminalKeybindings {
            bindings: vec![(
                KeyBinding::new("f6", NoAction {}, None),
                TerminalShortcut::Top,
            )],
            normalize: |key| Some(key.clone()),
        });
        let writes = sent.clone();
        let sub = cx.subscribe(&view, move |_, event, _| {
            if let TerminalViewEvent::SendBytes(bytes) = event {
                writes.borrow_mut().extend_from_slice(bytes);
            }
        });
        view.update(cx, |v, cx| {
            w.focus(&v.focus_handle(cx), cx);
            v.feed_bytes(&b"history\r\n".repeat(150), cx);
        });
        sub
    });
    cx.simulate_keystrokes("f6");
    view.read_with(cx, |v, _| {
        assert!(v.snapshot().scrollback_lines > 0);
        assert_eq!(v.snapshot().display_offset, v.snapshot().scrollback_lines);
    });
    assert_eq!(sent.borrow().as_slice(), b"");
    cx.simulate_keystrokes("shift-home");
    assert_eq!(sent.borrow().as_slice(), b"\x1b[1;2H");
    view.read_with(cx, |v, _| assert_eq!(v.snapshot().display_offset, 0));
}
