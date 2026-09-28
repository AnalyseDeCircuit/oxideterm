use super::*;
use gpui::{
    AvailableSpace, IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext, point,
    size,
};

#[path = "../../../oxideterm-terminal/tests/support/ssh_peer.rs"]
mod ssh_peer;

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
    pane.update(cx, |pane, _| {
        pane.snapshot_dirty = true;
        pane.snapshot_deferred_since = None;
    });
    draw(cx);
    let blocked = timed_out.load(std::sync::atomic::Ordering::Acquire);
    let _ = release_tx.send(());
    assert!(
        !blocked,
        "render waited for the parser despite deferring its snapshot"
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
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        draw(cx);
        if pane.read_with(cx, |pane, _| {
            pane.snapshot
                .lines
                .iter()
                .any(|line| line.text().contains("FINAL-AFTER-DEFER"))
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "deferred final output was never painted"
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
    let producer = peer.runtime.spawn(async move {
        for _ in 0..512 {
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
    let mut sent_input = false;
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
        if !sent_input && started.elapsed() >= Duration::from_millis(10) {
            pane.update(cx, |pane, cx| {
                pane.send_command_sender_text_chunk("paint-input", cx);
            });
            sent_input = true;
        }
        cx.draw(
            point(px(0.0), px(0.0)),
            size(
                AvailableSpace::Definite(px(900.0)),
                AvailableSpace::Definite(px(600.0)),
            ),
            |_, _| pane.clone().into_element(),
        );
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
        .block_on(sender.data(channel, b"\x1bP1000p".to_vec()))
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
