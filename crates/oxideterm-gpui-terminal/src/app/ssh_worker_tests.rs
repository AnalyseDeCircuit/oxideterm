use super::*;
use gpui::{
    AvailableSpace, IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext, point,
    size,
};

#[path = "../../../oxideterm-terminal/tests/support/ssh_peer.rs"]
mod ssh_peer;

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
    let bootstrap = format!(
        "\x1bP1000p%begin 1 1 1\n%end 1 1 1\n%begin 1 2 1\n3.4\n%end 1 2 1\n\
         %begin 1 3 1\n$1 demo\n%end 1 3 1\n%begin 1 4 1\n@1 0 1 * shell\n%end 1 4 1\n\
         %begin 1 5 1\n%1 @1 1 40 24 0 0 {layout}\n%2 @1 0 39 24 0 0 {layout}\n%end 1 5 1\n\
         %begin 1 6 1\n%end 1 6 1\n%begin 1 7 1\n%end 1 7 1\n\
         %begin 1 8 1\n%1 0 0\n%2 0 0\n%end 1 8 1\n%begin 1 9 1\n$1 @1 %1\n%end 1 9 1\n\
         %output %2 \\033[?1000h\\033[?1006h\n"
    );
    peer.runtime
        .block_on(sender.data(channel, bootstrap.into_bytes()))
        .unwrap();
    let activity = pane.read_with(cx, |pane, _| pane.terminal.lock().activity_receiver());
    let deadline = Instant::now() + Duration::from_secs(10);
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
            pane.terminal
                .lock()
                .tmux_state()
                .is_some_and(|state| state.ready && state.pane_count == 2)
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
