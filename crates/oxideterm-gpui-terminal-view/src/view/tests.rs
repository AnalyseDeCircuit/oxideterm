use std::{cell::RefCell, rc::Rc};

use gpui::{
    AppContext, Bounds, ClipboardItem, InputHandler, Modifiers, TestAppContext, point, px, size,
};

use super::{TerminalView, TerminalViewEvent, ime::TerminalViewInputHandler};
use crate::tests::test_metrics;

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
fn ime_replaces_utf16_preedit_before_commit_and_platform_paste_keeps_boundaries(
    cx: &mut TestAppContext,
) {
    let (view, cx) =
        cx.add_window_view(|_, cx| TerminalView::new(Default::default(), test_metrics(), cx));
    let sent = Rc::new(RefCell::new(Vec::new()));
    let _subscription = cx.update(|_, cx| {
        let sent = sent.clone();
        cx.subscribe(&view, move |_, event, _| {
            if let TerminalViewEvent::SendBytes(bytes) = event {
                sent.borrow_mut().push(bytes.clone());
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
    });
    assert_eq!(
        *sent.borrow(),
        ["a界!".as_bytes(), b"\x1b[200~one\ntwo\x1b[201~"]
    );
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
