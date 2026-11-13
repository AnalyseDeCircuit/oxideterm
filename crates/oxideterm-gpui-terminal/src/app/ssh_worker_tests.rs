use super::*;
use gpui::{
    AppContext, AvailableSpace, IntoElement, MouseButton, MouseDownEvent, MouseUpEvent,
    TestAppContext, point, size,
};

#[path = "../../../oxideterm-terminal/tests/support/ssh_peer.rs"]
mod ssh_peer;

#[cfg(unix)]
#[test]
fn bash_redraw_over_ssh_preserves_prompt_after_resize_and_scroll() {
    use oxideterm_terminal::{LocalCommand, LocalPtyConfig, LocalPtySession, TerminalEncoding};

    let mut peer = ssh_peer::SshPeer::new();
    let config =
        SshSessionConfig::from(peer.config.take().unwrap()).with_runtime(peer.runtime.clone());
    let mut terminal = TerminalSession::ssh(config, 80, 24);
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut bash = LocalPtySession::spawn_with_config_graphics_and_encoding(
        80,
        24,
        LocalPtyConfig {
            command: Some(LocalCommand {
                program: "/bin/bash".into(),
                args: zeroize::Zeroizing::new(vec![
                    "--noprofile".into(),
                    "--norc".into(),
                    "-i".into(),
                ]),
            }),
            cwd: Some(directory.path().to_path_buf()),
            load_profile: false,
            ..Default::default()
        },
        Default::default(),
        TerminalEncoding::Utf8,
        100,
    )
    .unwrap();
    let output = Arc::new(Mutex::new(Vec::new()));
    let captured = output.clone();
    bash.set_output_processor(Some(Arc::new(move |bytes| {
        captured.lock().extend_from_slice(bytes);
        bytes.to_vec()
    })));
    terminal
        .write_input(b"PS1='ready> '; printf 'row%02d\\n' {1..30}\r")
        .unwrap();
    let activity = terminal.activity_receiver();
    let mut batch = 0;
    let mut previous = terminal.snapshot();
    for (cols, rows, expected) in [
        (80, 24, "row30"),
        (40, 8, "size=8 40"),
        (100, 20, "size=20 100"),
    ] {
        if cols != 80 {
            // Leave a wrapped input line for Readline to redraw on SIGWINCH.
            terminal.write_input(b"printf 'abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz'; printf '\\nsize='; stty size").unwrap();
            let (input, _) = peer.input.recv_timeout(Duration::from_secs(5)).unwrap();
            bash.write_input(&input).unwrap();
            terminal.resize_with_cell_size(cols, rows, 8, 16).unwrap();
            // The SSH startup size can precede this request in the transport queue.
            loop {
                let accepted = peer.resize.recv_timeout(Duration::from_secs(5)).unwrap();
                bash.resize(accepted.0 as usize, accepted.1 as usize)
                    .unwrap();
                if accepted == (cols as u32, rows as u32) {
                    break;
                }
            }
            terminal.write_input(b"\r").unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            for (input, _) in peer.input.try_iter() {
                bash.write_input(&input).unwrap();
            }
            bash.drain_output();
            let mut bytes = std::mem::take(&mut *output.lock());
            if !bytes.is_empty() {
                // A completed SSH send does not mean the parser consumed it. A
                // trailing title marker fences this batch without consuming damage.
                batch += 1;
                let marker = format!("resize-probe-{batch}");
                bytes.extend_from_slice(format!("\x1b]2;{marker}\x07").as_bytes());
                peer.runtime.block_on(sender.data(channel, bytes)).unwrap();
                peer.runtime.block_on(async {
                    tokio::time::timeout(Duration::from_secs(5), async {
                        while terminal.title().as_deref() != Some(marker.as_str()) {
                            assert!(activity.notified().await);
                        }
                    })
                    .await
                    .expect("SSH parser did not consume the output batch");
                });
            }
            let scrolled = terminal.scroll_lines_snapshot_incremental(1, &previous);
            previous = terminal.scroll_lines_snapshot_incremental(-1, &scrolled);
            let full = terminal.snapshot();
            // Readline may still be emitting its resize redraw into the local PTY.
            // Compare after its final prompt reaches the now-quiescent SSH parser.
            let text = terminal.buffer_text();
            if text.lines().any(|line| line.trim_end() == expected)
                && full
                    .lines
                    .get(full.cursor_row)
                    .is_some_and(|line| line.text().trim_end() == "ready>")
            {
                let current = terminal.snapshot_incremental(&previous);
                assert_eq!(
                    current
                        .lines
                        .iter()
                        .map(|row| row.text())
                        .collect::<Vec<_>>(),
                    full.lines.iter().map(|row| row.text()).collect::<Vec<_>>()
                );
                assert_eq!(
                    (current.cursor_row, current.cursor_col),
                    (full.cursor_row, 7)
                );
                assert!(current.cursor_row < rows);
                previous = current;
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Bash did not settle at {cols}x{rows}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    bash.shutdown();
    terminal.shutdown();
    peer.stop();
}

#[gpui::test]
fn accepted_resize_does_not_wait_for_the_ssh_parser(cx: &mut TestAppContext) {
    let mut peer = ssh_peer::SshPeer::new();
    let config =
        SshSessionConfig::from(peer.config.take().unwrap()).with_runtime(peer.runtime.clone());
    let (_, cx) = cx.add_window_view(|_, _| super::tests::TerminalTestRoot);
    let pane = cx.update(|window, cx| {
        cx.new(|cx| {
            TerminalPane::new_ssh_with_preferences(config, Default::default(), window, cx).unwrap()
        })
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let terminal = pane.read_with(cx, |pane, _| pane.terminal.clone());
    let activity = terminal.lock().activity_receiver();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let release_rx = std::sync::Mutex::new(Some(release_rx));
    terminal
        .lock()
        .set_output_processor(Some(Arc::new(move |bytes| {
            if let Some(release) = release_rx.lock().unwrap().take() {
                entered_tx.send(()).unwrap();
                release.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            bytes.to_vec()
        })));
    peer.runtime
        .block_on(sender.data(channel, b"prompt> ".to_vec()))
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    // Completion must precede releasing the parser lock. The timeout only lets a
    // blocking regression unwind rather than hanging the test process.
    let (returned_tx, returned_rx) = std::sync::mpsc::sync_channel(1);
    let release = std::thread::spawn(move || {
        let returned = returned_rx.recv_timeout(Duration::from_secs(2)).is_ok();
        release_tx.send(()).unwrap();
        returned
    });
    let accepted = pane.update(cx, |pane, cx| pane.resize_grid(40, 8, cx));
    let _ = returned_tx.send(());
    assert!(release.join().unwrap(), "resize waited for the parser lock");
    assert_eq!(accepted.unwrap(), (40, 8));
    peer.runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while terminal.lock().snapshot().rows != 8 {
                assert!(activity.notified().await);
            }
        })
        .await
        .unwrap();
    });
    cx.run_until_parked();
    cx.executor().advance_clock(PTY_RESIZE_DEBOUNCE);
    cx.run_until_parked();
    pane.read_with(cx, |pane, _| {
        assert_eq!((pane.snapshot.cols, pane.snapshot.rows), (40, 8));
        assert_eq!(pane.pending_pty_resize, None);
        assert_eq!(pane.snapshot.lines[0].text().trim_end(), "prompt>");
    });
    terminal.lock().shutdown();
    peer.stop();
}

#[gpui::test]
fn viewport_resize_recovers_after_ssh_queue_backpressure(cx: &mut TestAppContext) {
    let mut peer = ssh_peer::SshPeer::new();
    let config =
        SshSessionConfig::from(peer.config.take().unwrap()).with_runtime(peer.runtime.clone());
    let (_, cx) = cx.add_window_view(|_, _| super::tests::TerminalTestRoot);
    let pane = cx.update(|window, cx| {
        cx.new(|cx| {
            TerminalPane::new_ssh_with_preferences(config, Default::default(), window, cx).unwrap()
        })
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let terminal = pane.read_with(cx, |pane, _| pane.terminal.clone());
    let activity = terminal.lock().activity_receiver();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let release_rx = std::sync::Mutex::new(Some(release_rx));
    terminal
        .lock()
        .set_output_processor(Some(Arc::new(move |bytes| {
            if let Some(release) = release_rx.lock().unwrap().take() {
                entered_tx.send(()).unwrap();
                release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            bytes.to_vec()
        })));
    let target = pane.update(cx, |pane, cx| {
        let bounds = Bounds::new(
            point(px(0.0), px(0.0)),
            size(
                px(640.0),
                pane.metrics.line_height * 8.0 + px(TERMINAL_CONTENT_PADDING * 2.0),
            ),
        );
        pane.apply_viewport_bounds(bounds, 1.0, cx);
        pane.pending_pty_resize.unwrap()
    });
    peer.runtime
        .block_on(sender.data(channel, b"output\r\nprompt> ".to_vec()))
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Hold the real parser while filling its bounded control queue. No resize
    // failure hook is needed, and releasing the parser restores the same session.
    let rejected = (0..1024).any(|_| terminal.lock().set_focused(false).is_err());
    let pending_after_failure = pane.update(cx, |pane, cx| {
        pane.flush_pending_pty_resize(pane.pty_resize_generation, cx);
        let error = pane.resize_grid(20, 4, cx).unwrap_err();
        assert_eq!(
            error,
            "The terminal backend rejected the requested grid size."
        );
        pane.pending_pty_resize
    });
    release_tx.send(()).unwrap();
    assert!(
        rejected,
        "the control queue must actually apply backpressure"
    );
    assert_eq!(pending_after_failure, Some(target));
    peer.runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while terminal.lock().set_focused(false).is_err() {
                assert!(activity.notified().await);
            }
        })
        .await
        .unwrap();
    });
    cx.run_until_parked();
    cx.executor().advance_clock(PTY_RESIZE_DEBOUNCE);
    cx.run_until_parked();
    peer.runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while terminal.lock().snapshot().rows != 8 {
                assert!(activity.notified().await);
            }
        })
        .await
        .unwrap();
    });
    cx.executor().advance_clock(PTY_RESIZE_DEBOUNCE);
    cx.run_until_parked();
    pane.read_with(cx, |pane, _| {
        assert_eq!((pane.snapshot.cols, pane.snapshot.rows), (target.0, 8));
        assert_eq!(pane.pending_pty_resize, None);
        assert!(pane.snapshot.cursor_row < 8);
        assert!(
            terminal
                .lock()
                .buffer_text()
                .trim_end()
                .ends_with("prompt>")
        );
    });
    terminal.lock().shutdown();
    peer.stop();
}

#[gpui::test]
fn ssh_search_refreshes_during_output(cx: &mut TestAppContext) {
    let mut peer = ssh_peer::SshPeer::new();
    let config = SshSessionConfig::from(peer.config.take().unwrap())
        .with_runtime(peer.runtime.clone())
        .with_registry(
            peer.registry.clone(),
            oxideterm_ssh::ConnectionConsumer::Terminal("search-output".into()),
        );
    let (pane, cx) = cx.add_window_view(move |window, cx| {
        TerminalPane::new_ssh_with_preferences(
            config,
            TerminalUiPreferences {
                cursor_blink: false,
                ..Default::default()
            },
            window,
            cx,
        )
        .unwrap()
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    pane.update(cx, |pane, cx| {
        pane.tick(cx);
        pane.set_search_query(Some("late-hit".into()), None, cx);
    });
    cx.run_until_parked();
    cx.executor().advance_clock(TERMINAL_SEARCH_DEBOUNCE);
    cx.run_until_parked();
    pane.read_with(cx, |pane, _| {
        assert_eq!(pane.current_search_matches().as_ref(), &[])
    });
    let activity = pane.read_with(cx, |pane, _| pane.terminal.lock().activity_receiver());
    for output in ["late-hit\r\n", "more output\r\n"] {
        peer.runtime
            .block_on(sender.data(channel, output.as_bytes().to_vec()))
            .unwrap();
        peer.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), async {
                while !pane.read_with(cx, |pane, _| {
                    pane.terminal.lock().buffer_text().contains(output.trim())
                }) {
                    assert!(activity.notified().await);
                }
            })
            .await
            .expect("SSH parser did not consume the new output");
        });
        pane.update(cx, |pane, cx| pane.tick(cx));
        cx.run_until_parked();
        cx.executor().advance_clock(TERMINAL_SEARCH_DEBOUNCE / 2);
        cx.run_until_parked();
    }
    pane.read_with(cx, |pane, _| {
        let matches = pane.current_search_matches();
        assert_eq!(
            matches
                .iter()
                .map(|hit| (hit.line, hit.start_col, hit.end_col))
                .collect::<Vec<_>>(),
            vec![(0, 0, 8)]
        );
    });
}

#[gpui::test]
fn exited_ssh_pane_requests_reconnect_only_for_enter_and_resumes_in_place(cx: &mut TestAppContext) {
    let mut peer = ssh_peer::SshPeer::new();
    let config = SshSessionConfig::from(peer.config.take().unwrap())
        .with_runtime(peer.runtime.clone())
        .with_registry(
            peer.registry.clone(),
            oxideterm_ssh::ConnectionConsumer::Terminal("ui-reconnect".into()),
        );
    let (pane, cx) = cx.add_window_view(move |window, cx| {
        TerminalPane::new_ssh_with_preferences(config, TerminalUiPreferences::default(), window, cx)
            .unwrap()
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    peer.runtime
        .block_on(sender.data(channel, b"original output\r\n".to_vec()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !pane.read_with(cx, |pane, _| {
        pane.terminal
            .lock()
            .buffer_text()
            .contains("original output")
    }) {
        assert!(Instant::now() < deadline, "initial output missing");
        std::thread::sleep(Duration::from_millis(2));
    }
    let connection_id = pane.read_with(cx, |pane, _| {
        pane.terminal
            .lock()
            .ssh_connection_handle()
            .unwrap()
            .connection_id()
            .to_string()
    });
    peer.runtime.block_on(sender.close(channel)).unwrap();
    while pane.read_with(cx, |pane, _| pane.lifecycle().is_running()) {
        assert!(Instant::now() < deadline, "shell did not exit");
        std::thread::sleep(Duration::from_millis(2));
    }
    pane.update(cx, |pane, cx| {
        pane.tick(cx);
        assert!(pane.can_reconnect_ssh());
        pane.set_current_working_directory_from_terminal_action("/old-session".into(), cx);
        for (key, expected) in [
            ("shift-enter", None),
            ("enter", Some(TerminalContextAction::Reconnect)),
        ] {
            pane.handle_key(
                &gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                cx,
            );
            assert_eq!(pane.take_context_action_request(), expected, "{key}");
        }
        pane.set_ssh_reconnect_pending(true, cx);
        pane.handle_key(
            &gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("enter").unwrap(),
                is_held: false,
                prefer_character_input: false,
            },
            cx,
        );
        assert_eq!(pane.take_context_action_request(), None);
        // A node failure clears the panes that are still waiting for the queued
        // open, and only those: a reconnect already handed to the shell keeps its
        // StartingShell state instead of flipping back to a usable button.
        assert!(pane.clear_ssh_reconnect_waiting(cx));
        assert!(!pane.ssh_reconnect_pending());
        pane.reconnect_ssh(
            SshSessionConfig::for_existing_connection(connection_id, "fixture", 22, "parser-test")
                .with_runtime(peer.runtime.clone())
                .with_registry(
                    peer.registry.clone(),
                    oxideterm_ssh::ConnectionConsumer::Terminal("ui-reconnect".into()),
                ),
            cx,
        )
        .unwrap();
        assert!(pane.ssh_reconnect_starting());
        assert!(!pane.clear_ssh_reconnect_waiting(cx));
        assert!(pane.ssh_reconnect_starting());
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    peer.runtime
        .block_on(sender.data(channel, b"restored output\r\n".to_vec()))
        .unwrap();
    while !pane.read_with(cx, |pane, _| {
        pane.terminal
            .lock()
            .buffer_text()
            .contains("restored output")
    }) {
        assert!(Instant::now() < deadline, "restored output missing");
        std::thread::sleep(Duration::from_millis(2));
    }
    pane.update(cx, |pane, cx| {
        pane.tick(cx);
        assert!(!pane.can_reconnect_ssh());
        assert!(!pane.ssh_reconnect_pending());
        assert_eq!(pane.current_working_directory(), None);
        assert!(
            pane.terminal
                .lock()
                .buffer_text()
                .contains("original output")
        );
        assert!(pane.send_text("continued input", cx));
    });
    assert_eq!(
        peer.input.recv_timeout(Duration::from_secs(5)).unwrap().0,
        b"continued input"
    );
    pane.update(cx, |pane, _| pane.shutdown());
    peer.stop();
}

#[gpui::test]
fn mouse_clipboard_paste_overrides_tracking_and_preserves_confirmation(cx: &mut TestAppContext) {
    let mut peer = ssh_peer::SshPeer::new();
    let config =
        SshSessionConfig::from(peer.config.take().unwrap()).with_runtime(peer.runtime.clone());
    let (pane, cx) = cx.add_window_view(move |window, cx| {
        TerminalPane::new_ssh_with_preferences(
            config,
            TerminalUiPreferences {
                paste_protection: true,
                right_click_paste: true,
                middle_click_paste: true,
                ..Default::default()
            },
            window,
            cx,
        )
        .unwrap()
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(10)).unwrap();
    peer.runtime
        .block_on(sender.data(channel, b"\x1b[?2004h\x1b[?1003h\x1b[?1006h".to_vec()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !pane.read_with(cx, |pane, _| {
        pane.terminal
            .lock()
            .mode()
            .contains(TermMode::BRACKETED_PASTE | TermMode::MOUSE_MOTION | TermMode::SGR_MOUSE)
    }) {
        assert!(Instant::now() < deadline, "paste mode was not parsed");
        std::thread::sleep(Duration::from_millis(2));
    }
    pane.update(cx, |pane, cx| pane.tick(cx));
    cx.simulate_resize(size(px(900.0), px(600.0)));
    cx.update(|window, cx| {
        pane.update(cx, |pane, cx| {
            pane.apply_viewport_bounds(
                gpui::Bounds::new(point(px(0.0), px(0.0)), window.viewport_size()),
                window.scale_factor(),
                cx,
            );
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (start, end) = pane.read_with(cx, |pane, _| {
        let origin = pane.content_origin();
        let start = point(
            origin.x + px(pane.terminal_content_padding_x() + 0.5 * pane.metrics.cell_width_f32()),
            origin.y + px(TERMINAL_CONTENT_PADDING + 0.5 * pane.metrics.line_height_f32()),
        );
        (
            start,
            start + point(px(pane.metrics.cell_width_f32()), px(0.0)),
        )
    });
    for button in [MouseButton::Right, MouseButton::Middle] {
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                "第一段\r\n\r\n第二段\n第三段\r".into(),
            ))
        });
        cx.simulate_mouse_down(start, button, Default::default());
        cx.simulate_mouse_move(end, button, Default::default());
        // Ownership belongs to the press even if Shift changes before release.
        cx.simulate_mouse_up(
            end,
            button,
            gpui::Modifiers {
                shift: button == MouseButton::Right,
                ..Default::default()
            },
        );
        pane.update(cx, |pane, cx| pane.send_protocol_bytes(b"before-paste", cx));
        let (before_confirmation, _) = peer.input.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(before_confirmation, b"before-paste");

        pane.update(cx, |pane, cx| {
            pane.confirm_pending_paste(cx);
            pane.send_protocol_bytes(b"after-paste", cx);
        });
        let mut received = Vec::new();
        while !received.ends_with(b"after-paste") {
            let (bytes, _) = peer.input.recv_timeout(Duration::from_secs(5)).unwrap();
            received.extend_from_slice(&bytes);
        }
        assert_eq!(
            received,
            "\x1b[200~第一段\n\n第二段\n第三段\n\x1b[201~after-paste".as_bytes()
        );
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    for (button, enabled, expected) in [
        (
            MouseButton::Right,
            false,
            "\x1b[<2;1;1M\x1b[<34;2;1M\x1b[<2;2;1m",
        ),
        (
            MouseButton::Middle,
            false,
            "\x1b[<1;1;1M\x1b[<33;2;1M\x1b[<1;2;1m",
        ),
        (
            MouseButton::Left,
            true,
            "\x1b[<0;1;1M\x1b[<32;2;1M\x1b[<0;2;1m",
        ),
    ] {
        pane.update(cx, |pane, cx| {
            pane.settings.right_click_paste = enabled;
            pane.settings.middle_click_paste = enabled;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_down(start, button, Default::default());
        cx.simulate_mouse_move(end, button, Default::default());
        cx.simulate_mouse_up(end, button, Default::default());
        pane.update(cx, |pane, cx| pane.send_protocol_bytes(b"after-mouse", cx));
        let mut received = Vec::new();
        while !received.ends_with(b"after-mouse") {
            received.extend(peer.input.recv_timeout(Duration::from_secs(5)).unwrap().0);
        }
        assert_eq!(received, format!("{expected}after-mouse").as_bytes());
    }
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_down(
        start,
        MouseButton::Right,
        gpui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    cx.simulate_mouse_up(start, MouseButton::Right, Default::default());
    pane.update(cx, |pane, cx| {
        assert!(pane.context_menu.is_some());
        pane.send_protocol_bytes(b"after-menu", cx);
    });
    assert_eq!(
        peer.input.recv_timeout(Duration::from_secs(5)).unwrap().0,
        b"after-menu"
    );
    pane.update(cx, |pane, _| pane.terminal.lock().shutdown());
}

#[gpui::test]
fn application_scroll_accumulates_small_deltas_until_remote_input(cx: &mut TestAppContext) {
    use gpui::{ScrollDelta, ScrollEvent, TouchPhase};

    let mut peer = ssh_peer::SshPeer::new();
    let config =
        SshSessionConfig::from(peer.config.take().unwrap()).with_runtime(peer.runtime.clone());
    let (pane, cx) = cx.add_window_view(move |window, cx| {
        TerminalPane::new_ssh_with_preferences(
            config,
            TerminalUiPreferences {
                cursor_blink: false,
                ..Default::default()
            },
            window,
            cx,
        )
        .unwrap()
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(10)).unwrap();

    for (sequence, expected_mode, multiplier, expected_input) in [
        (
            b"\x1b[?1049h\x1b[?1000h\x1b[?1006h".as_slice(),
            TermMode::ALT_SCREEN | TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE,
            1.0,
            b"\x1b[<64;1;1M\x1b[<65;1;1M".as_slice(),
        ),
        (
            b"\x1b[?1000l\x1b[?1006l\x1b[?1007h".as_slice(),
            TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL,
            TERMINAL_SCROLL_MULTIPLIER,
            b"\x1bOA\x1bOB".as_slice(),
        ),
    ] {
        peer.runtime
            .block_on(sender.data(channel, sequence.to_vec()))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let ready = pane.read_with(cx, |pane, _| {
                let mode = pane.terminal.lock().mode();
                mode.contains(expected_mode)
                    && (expected_mode.intersects(TermMode::MOUSE_MODE)
                        || !mode.intersects(TermMode::MOUSE_MODE))
            });
            if ready {
                break;
            }
            assert!(Instant::now() < deadline, "application mode was not parsed");
            std::thread::sleep(Duration::from_millis(2));
        }
        pane.update(cx, |pane, cx| {
            pane.clear_smooth_scroll_remainder();
            for (direction, steps) in [(1.0, 3), (-1.0, 4)] {
                for _ in 0..steps {
                    pane.handle_scroll(
                        &ScrollEvent {
                            position: point(px(0.0), px(0.0)),
                            delta: ScrollDelta::Pixels(point(
                                px(0.0),
                                pane.metrics.line_height * (direction * 0.375 / multiplier),
                            )),
                            touch_phase: TouchPhase::Moved,
                            ..Default::default()
                        },
                        cx,
                    );
                }
            }
            // A following write proves the worker drained the scroll input without a timing guess.
            pane.send_protocol_bytes(b"scroll-barrier", cx);
        });
        let mut received = Vec::new();
        while !received.ends_with(b"scroll-barrier") {
            let (bytes, _) = peer.input.recv_timeout(Duration::from_secs(5)).unwrap();
            received.extend_from_slice(&bytes);
        }
        assert_eq!(
            &received[..received.len() - b"scroll-barrier".len()],
            expected_input
        );
    }
    pane.update(cx, |pane, _| pane.terminal.lock().shutdown());
}

#[gpui::test]
fn busy_ssh_parser_does_not_block_drawing_the_previous_frame(cx: &mut TestAppContext) {
    let mut peer = ssh_peer::SshPeer::new();
    let config =
        SshSessionConfig::from(peer.config.take().unwrap()).with_runtime(peer.runtime.clone());
    let (pane, cx) = cx.add_window_view(move |window, cx| {
        TerminalPane::new_ssh_with_preferences(
            config,
            TerminalUiPreferences {
                cursor_blink: false,
                ..Default::default()
            },
            window,
            cx,
        )
        .unwrap()
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(10)).unwrap();
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.draw(
            point(px(0.0), px(0.0)),
            size(
                AvailableSpace::Definite(px(900.0)),
                AvailableSpace::Definite(px(600.0)),
            ),
            |_, _| pane.clone().into_element(),
        );
    };
    draw(cx);
    let previous = pane.read_with(cx, |pane, _| pane.snapshot.clone());
    let (entered_tx, entered_rx) = crossbeam_channel::bounded(1);
    let (release_tx, release_rx) = crossbeam_channel::bounded(1);
    let timed_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_timed_out = timed_out.clone();
    pane.update(cx, |pane, _| {
        pane.terminal
            .lock()
            .set_output_processor(Some(Arc::new(move |bytes| {
                entered_tx.send(()).unwrap();
                // A watchdog releases the worker even when a blocking render regresses.
                if release_rx.recv_timeout(Duration::from_secs(5)).is_err() {
                    worker_timed_out.store(true, std::sync::atomic::Ordering::Release);
                }
                bytes.to_vec()
            })));
    });
    peer.runtime
        .block_on(sender.data(channel, b"FINAL-AFTER-DEFER".to_vec()))
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let palette_started = Instant::now();
    pane.update(cx, |pane, cx| {
        let mut preferences = pane.preferences.clone();
        preferences.theme = TerminalUiTheme::new(0xfdf6e3, 0x657b83, 0x586e75);
        pane.set_preferences(preferences, cx);
        pane.snapshot_deferred_since = None;
    });
    draw(cx);
    eprintln!(
        "SSH_BUSY_PALETTE update_and_draw_ms={:.3}",
        palette_started.elapsed().as_secs_f64() * 1000.0
    );
    let blocked = timed_out.load(std::sync::atomic::Ordering::Acquire);
    let _ = release_tx.send(());
    assert!(
        !blocked,
        "theme update or render waited for the busy parser"
    );
    pane.read_with(cx, |pane, _| {
        assert!(pane.snapshot_dirty);
        assert_eq!(
            pane.snapshot
                .lines
                .iter()
                .map(|row| &row.cells)
                .collect::<Vec<_>>(),
            previous
                .lines
                .iter()
                .map(|row| &row.cells)
                .collect::<Vec<_>>(),
        );
    });
    let activity = pane.read_with(cx, |pane, _| pane.terminal.lock().activity_receiver());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // Bridge the worker's real notification into GPUI's deterministic executor.
        let notified = peer.runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(2), activity.notified())
                .await
                .unwrap_or(false)
        });
        if notified {
            pane.update(cx, |pane, cx| pane.tick(cx));
        }
        draw(cx);
        if pane.read_with(cx, |pane, _| {
            pane.snapshot.lines.iter().any(|line| {
                line.text().contains("FINAL-AFTER-DEFER")
                    && line.cells[0].fg == oxideterm_terminal::TerminalColor::rgb(0x65, 0x7b, 0x83)
                    && line.cells[0].bg == oxideterm_terminal::TerminalColor::rgb(0xfd, 0xf6, 0xe3)
            })
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "deferred final output was never painted with the new palette"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    pane.update(cx, |pane, _| pane.terminal.lock().shutdown());
}

#[gpui::test]
fn ssh_worker_output_reaches_a_painted_pane_without_followup_output(cx: &mut TestAppContext) {
    let mut peer = ssh_peer::SshPeer::new();
    let config = SshSessionConfig::from(peer.config.take().unwrap())
        .with_runtime(peer.runtime.clone())
        .with_registry(
            peer.registry.clone(),
            oxideterm_ssh::ConnectionConsumer::Terminal("paint-test".into()),
        );
    let preferences = TerminalUiPreferences {
        cursor_blink: false,
        ..Default::default()
    };
    let (pane, cx) = cx.add_window_view(move |window, cx| {
        TerminalPane::new_ssh_with_preferences(config, preferences, window, cx).unwrap()
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(10)).unwrap();
    let started = Instant::now();
    let (continue_output_tx, continue_output_rx) = tokio::sync::oneshot::channel();
    let producer = peer.runtime.spawn(async move {
        sender
            .data(channel, "SSH output 中文 🦀\r\n".as_bytes().repeat(128))
            .await
            .unwrap();
        // Hold the final output until the pane has accepted input on the real channel.
        continue_output_rx.await.unwrap();
        for _ in 1..512 {
            sender
                .data(channel, "SSH output 中文 🦀\r\n".as_bytes().repeat(128))
                .await
                .unwrap();
        }
        sender
            .data(channel, b"\r\nPAINTED-FINAL-FRAME\r\n".to_vec())
            .await
            .unwrap();
        Instant::now()
    });
    let activity = pane.read_with(cx, |pane, _| pane.terminal.lock().activity_receiver());
    let mut continue_output = Some(continue_output_tx);
    loop {
        // GPUI's deterministic test harness disables external wakes to its
        // local executor. Bridge the real backend signal into the normal tick.
        let notified = peer.runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(2), activity.notified())
                .await
                .unwrap_or(false)
        });
        if notified {
            pane.update(cx, |pane, cx| pane.tick(cx));
        }
        cx.run_until_parked();
        cx.draw(
            point(px(0.0), px(0.0)),
            size(
                AvailableSpace::Definite(px(900.0)),
                AvailableSpace::Definite(px(600.0)),
            ),
            |_, _| pane.clone().into_element(),
        );
        if continue_output.is_some()
            && pane.read_with(cx, |pane, _| {
                pane.snapshot
                    .lines
                    .iter()
                    .any(|row| row.text().starts_with("SSH output "))
            })
        {
            pane.update(cx, |pane, cx| {
                assert!(pane.send_command_sender_text_chunk("paint-input", cx));
            });
            continue_output.take().unwrap().send(()).unwrap();
        }
        let visible = pane.read_with(cx, |pane, _| {
            pane.snapshot
                .lines
                .iter()
                .any(|row| row.text().trim_end() == "PAINTED-FINAL-FRAME")
        });
        if visible {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the final SSH frame was not presented"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let presented = Instant::now();
    let emitted = peer.runtime.block_on(producer).unwrap();
    let (input, _) = peer.input.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(input, b"paint-input");
    eprintln!(
        "SSH_PAINT producer_ms={:.3} painted_ms={:.3}",
        emitted.duration_since(started).as_secs_f64() * 1000.0,
        presented.duration_since(started).as_secs_f64() * 1000.0
    );
    pane.update(cx, |pane, _| pane.terminal.lock().shutdown());
}

#[gpui::test]
fn tmux_selection_replays_the_first_click_and_release_on_the_new_pane(cx: &mut TestAppContext) {
    let mut peer = ssh_peer::SshPeer::new();
    let config =
        SshSessionConfig::from(peer.config.take().unwrap()).with_runtime(peer.runtime.clone());
    let preferences = TerminalUiPreferences {
        cursor_blink: false,
        ..Default::default()
    };
    let (pane, cx) = cx.add_window_view(move |window, cx| {
        TerminalPane::new_ssh_with_preferences(config, preferences, window, cx).unwrap()
    });
    let (sender, channel) = peer.ready.recv_timeout(Duration::from_secs(10)).unwrap();
    let layout = "80x24,0,0{40x24,0,0,1,39x24,41,0,2}";
    peer.runtime
        .block_on(sender.data(channel, b"\x1bP1000p%begin 0 0 0\n%end 0 0 0\n".to_vec()))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let panes = format!("%1 @1 1 40 24 0 0 {layout}\n%2 @1 0 39 24 0 0 {layout}\n");
    let mut pending_input = Vec::new();
    for (index, (query, body)) in [
        ("refresh-client -C ", ""),
        ("display-message -p '#{version}'", "3.4\n"),
        ("list-sessions -F ", "$1 demo\n"),
        ("list-windows -F ", "@1 0 1 * shell\n"),
        ("list-panes -s -F ", panes.as_str()),
        ("capture-pane -p -e -t ", ""),
        ("capture-pane -p -e -t ", ""),
        (
            "list-panes -s -F '#{pane_id} #{cursor_x} #{cursor_y}'",
            "%1 0 0\n%2 0 0\n",
        ),
        (
            "display-message -p '#{session_id} #{window_id} #{pane_id}'",
            "$1 @1 %1\n",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        // The controller registers replies when queries are written to the transport.
        // A real tmux peer cannot answer queries before receiving them.
        let line_end = loop {
            if let Some(end) = pending_input.iter().position(|byte| *byte == b'\n') {
                break end;
            }
            let (bytes, _) = peer
                .input
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("tmux bootstrap query did not reach the peer");
            pending_input.extend(bytes);
        };
        let command = String::from_utf8(pending_input.drain(..=line_end).collect()).unwrap();
        assert!(
            command.starts_with(query),
            "unexpected tmux query: {command}"
        );
        let number = index + 1;
        let reply = format!("%begin 1 {number} 1\n{body}%end 1 {number} 1\n");
        peer.runtime
            .block_on(sender.data(channel, reply.into_bytes()))
            .unwrap();
    }
    peer.runtime
        .block_on(sender.data(
            channel,
            b"%output %2 \\033[?1000h\\033[?1006hMOUSE-READY\n".to_vec(),
        ))
        .unwrap();
    let activity = pane.read_with(cx, |pane, _| pane.terminal.lock().activity_receiver());
    loop {
        let notified = peer.runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(2), activity.notified())
                .await
                .unwrap_or(false)
        });
        if notified {
            pane.update(cx, |pane, cx| pane.tick(cx));
        }
        cx.draw(
            point(px(0.0), px(0.0)),
            size(
                AvailableSpace::Definite(px(900.0)),
                AvailableSpace::Definite(px(600.0)),
            ),
            |_, _| pane.clone().into_element(),
        );
        if pane.read_with(cx, |pane, _| {
            let terminal = pane.terminal.lock();
            terminal
                .tmux_state()
                .is_some_and(|state| state.ready && state.pane_count == 2)
                // Readiness precedes pane output; wait until mouse modes have been parsed too.
                && terminal
                    .snapshot()
                    .lines
                    .iter()
                    .any(|row| row.text().contains("MOUSE-READY"))
        }) {
            break;
        }
        assert!(Instant::now() < deadline, "tmux bootstrap did not complete");
    }
    while peer.input.try_recv().is_ok() {}
    pane.update(cx, |pane, cx| {
        let origin = pane.content_origin();
        let position = point(
            origin.x + px(pane.terminal_content_padding_x() + 45.5 * pane.metrics.cell_width_f32()),
            origin.y + px(TERMINAL_CONTENT_PADDING + 2.5 * pane.metrics.line_height_f32()),
        );
        pane.handle_mouse_down(
            &MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            },
            cx,
        );
        assert!(pane.tmux_selection_pending);
        pane.handle_mouse_up(
            &MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
            },
            cx,
        );
    });
    let mut commands = Vec::new();
    loop {
        let notified = peer.runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(2), activity.notified())
                .await
                .unwrap_or(false)
        });
        if notified {
            pane.update(cx, |pane, cx| pane.tick(cx));
        }
        while let Ok((bytes, _)) = peer.input.try_recv() {
            commands.extend(bytes);
        }
        if commands.ends_with(b"1b 5b 3c 30 3b 35 3b 33 6d\n") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "tmux mouse replay did not reach the peer"
        );
    }
    let commands = String::from_utf8(commands).unwrap();
    let relevant = commands
        .lines()
        .filter(|line| line.starts_with("select-pane") || line.starts_with("send-keys"))
        .collect::<Vec<_>>();
    assert_eq!(
        relevant,
        [
            "select-pane -t %2",
            "send-keys -H -t %2 1b 5b 3c 30 3b 35 3b 33 4d",
            "send-keys -H -t %2 1b 5b 3c 30 3b 35 3b 33 6d"
        ]
    );
    pane.update(cx, |pane, _| pane.terminal.lock().shutdown());
}
