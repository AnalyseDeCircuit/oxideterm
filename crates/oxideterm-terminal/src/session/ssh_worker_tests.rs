use super::*;
use oxideterm_modem_transfer::ModemIo;
use russh::{ChannelId, server};
#[path = "../../tests/support/ssh_peer.rs"]
mod ssh_peer;

struct Fixture<T: TerminalSessionBackend = SshPtySession> {
    runtime: Arc<Runtime>,
    server: ssh_peer::SshPeer,
    peer: server::Handle,
    channel: ChannelId,
    input: crossbeam_channel::Receiver<(Vec<u8>, Instant)>,
    terminal: T,
    wait_shutdown: fn(&T),
    registry: SshConnectionRegistry,
}
impl<T: TerminalSessionBackend> Fixture<T> {
    fn with_backend(create: impl FnOnce(SshSessionConfig) -> T, wait_shutdown: fn(&T)) -> Self {
        let mut server = ssh_peer::SshPeer::new();
        let runtime = server.runtime.clone();
        let registry = server.registry.clone();
        let input = server.input.clone();
        let config = server.config.take().unwrap();
        let mut terminal = create(
            SshSessionConfig::from(config)
                .with_runtime(runtime.clone())
                .with_registry(
                    registry.clone(),
                    ConnectionConsumer::Terminal("fixture".into()),
                )
                .with_trzsz_policy(Some(TrzszTransferPolicy::default())),
        );
        let (peer, channel) = server
            .ready
            .recv_timeout(Duration::from_secs(10))
            .expect("SSH shell startup");
        wait_until(|| {
            terminal.read_pending();
            terminal.is_interactive()
        });
        Self {
            runtime,
            server,
            peer,
            channel,
            input,
            terminal,
            registry,
            wait_shutdown,
        }
    }
    fn send(&self, bytes: &[u8]) {
        self.runtime
            .block_on(self.peer.data(self.channel, bytes.to_vec()))
            .unwrap();
    }
}

impl Fixture {
    fn new() -> Self {
        Self::with_backend(
            |config| {
                SshPtySession::new(
                    config,
                    80,
                    24,
                    Default::default(),
                    TerminalEncoding::Utf8,
                    1000,
                )
            },
            |terminal| wait_until(|| terminal.shared.finished.load(Ordering::Acquire)),
        )
    }
    fn barrier(&self) {
        let done = Arc::new(AtomicBool::new(false));
        let applied = done.clone();
        self.terminal
            .enqueue(0, move |_| {
                applied.store(true, Ordering::Release);
                Ok(())
            })
            .unwrap();
        wait_until(|| done.load(Ordering::Acquire));
    }
}
impl<T: TerminalSessionBackend> Drop for Fixture<T> {
    fn drop(&mut self) {
        self.terminal.shutdown();
        (self.wait_shutdown)(&self.terminal);
        self.server.stop();
        reap_workers();
    }
}
fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "SSH worker condition timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn ssh_parses_and_sends_replies_without_ui_drains() {
    let mut fixture = Fixture::new();
    fixture.send(b"abc\x1b[6n");
    assert_eq!(
        fixture
            .input
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .0,
        b"\x1b[1;4R"
    );
    assert!(fixture.terminal.buffer_text().contains("abc"));
    fixture.terminal.write_text("input").unwrap();
    assert_eq!(
        fixture
            .input
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .0,
        b"input"
    );
    fixture.send("\r\n中文 🦀 final\r\n".as_bytes());
    wait_until(|| fixture.terminal.buffer_text().contains("中文 🦀 final"));
    assert!(fixture.terminal.read_pending());
}

#[test]
fn ssh_control_boundary_preserves_recording_encoding_resize_and_clear_order() {
    let mut fixture = Fixture::new();
    fixture.terminal.set_output_events_enabled(true);
    fixture.barrier();
    fixture.terminal.take_events();
    let before = "before 中文\r\n".repeat(200);
    {
        let shared = fixture.terminal.shared.clone();
        let core = shared.core.lock();
        let start = core.handle.as_ref().unwrap().output_rx.published_sequence();
        fixture.send(before.as_bytes());
        wait_until(|| core.handle.as_ref().unwrap().output_rx.published_sequence() > start);
        fixture.terminal.set_output_events_enabled(false);
        fixture.terminal.set_encoding(TerminalEncoding::Gbk);
        fixture
            .terminal
            .resize_with_cell_size(TerminalResize::new(100, 30, 8, 16))
            .unwrap();
        fixture.terminal.clear_buffer();
        let boundary = core.handle.as_ref().unwrap().output_rx.published_sequence();
        fixture.send(b"late-before-clear\r\n");
        wait_until(|| core.handle.as_ref().unwrap().output_rx.published_sequence() > boundary);
    }
    fixture.barrier();
    let recorded: Vec<u8> = fixture
        .terminal
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            TerminalEvent::Output(bytes) => Some(bytes),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(recorded, before.as_bytes());
    fixture.send(b"\xc4\xe3\xba\xc3 after\r\n");
    wait_until(|| fixture.terminal.buffer_text().contains("你好 after"));
    let snapshot = fixture.terminal.snapshot();
    assert_eq!((snapshot.cols, snapshot.rows), (100, 30));
    assert!(!fixture.terminal.buffer_text().contains("before 中文"));
    assert!(fixture.terminal.buffer_text().contains("late-before-clear"));
    assert!(
        !fixture
            .terminal
            .take_events()
            .iter()
            .any(|event| matches!(event, TerminalEvent::Output(_)))
    );
}

#[test]
fn ssh_stalled_ui_bounds_events_and_close_unblocks_the_producer() {
    let mut fixture = Fixture::new();
    fixture.terminal.set_output_events_enabled(true);
    fixture.barrier();
    fixture.terminal.take_events();
    let peer = fixture.peer.clone();
    let channel = fixture.channel;
    let producer = fixture.runtime.spawn(async move {
        for _ in 0..8192 {
            if peer.data(channel, vec![b'x'; 4096]).await.is_err() {
                break;
            }
        }
    });
    wait_until(|| fixture.terminal.shared.event_bytes.load(Ordering::Acquire) >= EVENT_BYTES);
    let (bytes, sequence) = {
        let core = fixture.terminal.shared.core.lock();
        (
            fixture.terminal.shared.event_bytes.load(Ordering::Acquire),
            core.consumed_sequence,
        )
    };
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        fixture.terminal.shared.core.lock().consumed_sequence,
        sequence
    );
    assert!(
        bytes < EVENT_BYTES + 16 * 1024,
        "one bounded parse turn may cross the delivery watermark"
    );
    let started = Instant::now();
    fixture.terminal.shutdown();
    assert!(started.elapsed() < Duration::from_millis(100));
    wait_until(|| fixture.terminal.shared.finished.load(Ordering::Acquire));
    assert!(fixture.terminal.take_events().is_empty());
    producer.abort();
}

#[test]
fn ssh_busy_snapshot_retries_after_the_final_output_without_another_packet() {
    let mut fixture = Fixture::new();
    let previous = fixture.terminal.snapshot();
    let (entered_tx, entered_rx) = crossbeam_channel::bounded(1);
    let (release_tx, release_rx) = crossbeam_channel::bounded(1);
    fixture
        .terminal
        .set_output_processor(Some(Arc::new(move |bytes| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            bytes.to_vec()
        })));
    fixture.barrier();
    fixture.send(b"last frame\r\n");
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        fixture
            .terminal
            .try_render_snapshot(&previous, true)
            .is_none()
    );
    release_tx.send(()).unwrap();
    wait_until(|| fixture.terminal.buffer_text().contains("last frame"));
    let (snapshot, _, _) = fixture
        .terminal
        .try_render_snapshot(&previous, false)
        .unwrap();
    assert_eq!(snapshot.lines[0].text().trim_end(), "last frame");
}

#[test]
fn closing_ssh_parser_leaves_an_existing_forward_consumer_usable() {
    for injected_runtime in [true, false] {
        let mut fixture = Fixture::with_backend(
            |mut config| {
                if !injected_runtime {
                    config.runtime = None;
                }
                SshPtySession::new(
                    config,
                    80,
                    24,
                    Default::default(),
                    TerminalEncoding::Utf8,
                    1000,
                )
            },
            |terminal| wait_until(|| terminal.shared.finished.load(Ordering::Acquire)),
        );
        let connection = fixture.terminal.ssh_connection_handle().unwrap();
        let consumer = ConnectionConsumer::PortForward("fixture-forward".into());
        let retained = fixture
            .registry
            .acquire_consumer_for_connection(connection.connection_id(), consumer.clone())
            .unwrap();
        let mut stream = fixture
            .runtime
            .block_on(retained.open_direct_tcpip("echo.test", 9000, "127.0.0.1", 0))
            .unwrap();
        fixture.terminal.shutdown();
        wait_until(|| fixture.terminal.shared.finished.load(Ordering::Acquire));
        fixture.runtime.block_on(async {
            stream.write_all(b"forward survives").await.unwrap();
            let mut reply = [0; 16];
            tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut reply))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(&reply, b"forward survives");
            drop(stream);
        });
        let info = fixture
            .registry
            .list()
            .into_iter()
            .find(|info| info.connection_id == connection.connection_id())
            .unwrap();
        assert_eq!(info.consumers, vec![consumer.clone()]);
        fixture
            .registry
            .release(connection.connection_id(), &consumer);
    }
}

#[test]
fn ssh_controls_make_progress_during_continuous_output() {
    let mut fixture = Fixture::new();
    let stop = Arc::new(AtomicBool::new(false));
    let producer_stop = stop.clone();
    let peer = fixture.peer.clone();
    let channel = fixture.channel;
    let producer = fixture.runtime.spawn(async move {
        while !producer_stop.load(Ordering::Acquire) {
            if peer
                .data(channel, b"continuous output\r\n".repeat(200))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    wait_until(|| fixture.terminal.buffer_text().contains("continuous output"));
    fixture.terminal.write_text("interactive-input").unwrap();
    assert_eq!(
        fixture
            .input
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .0,
        b"interactive-input"
    );
    assert!(!producer.is_finished());
    stop.store(true, Ordering::Release);
    producer.abort();
}

#[test]
fn ssh_trzsz_transfer_writes_wake_the_parser_without_ui_polling() {
    let mut fixture = Fixture::new();
    fixture
        .terminal
        .set_output_processor(Some(Arc::new(|bytes| bytes.to_ascii_uppercase())));
    fixture.terminal.set_output_events_enabled(true);
    fixture.barrier();
    fixture.terminal.take_events();
    fixture.send(b"::TRZSZ:TRANSFER:R:1.1.6:9\n");
    let mut prompt = false;
    wait_until(|| {
        prompt |= fixture
            .terminal
            .take_events()
            .iter()
            .any(|event| matches!(event, TerminalEvent::TrzszTransferPrompt { .. }));
        prompt
    });
    let mut transfer = fixture.terminal.take_trzsz_transfer().unwrap();
    transfer.send_action(true, false).unwrap();
    let action = fixture
        .input
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .0;
    assert!(action.starts_with(b"#ACT:"));
    fixture.send(b"#CFG:eJyrVkrKzEssqlSySkvMKU7VUUrJLEpNLsmHi9QCANctDJE=\n");
    let cancel = transfer.input_handle();
    let (done_tx, done_rx) = crossbeam_channel::bounded(1);
    let reader = std::thread::spawn(move || {
        let config = transfer.recv_config();
        done_tx.send((transfer, config)).unwrap();
    });
    let received = done_rx.recv_timeout(Duration::from_secs(5));
    if received.is_err() {
        cancel.stop_transferring();
    }
    reader.join().unwrap();
    let (mut transfer, config) = received.unwrap();
    let config = config.unwrap();
    assert_eq!(config["binary"].as_bool(), Some(false));
    assert_eq!(config["directory"].as_bool(), Some(false));
    assert!(
        !fixture
            .terminal
            .take_events()
            .iter()
            .any(|event| matches!(event, TerminalEvent::Output(_)))
    );
    transfer.stop_transferring();
    fixture.terminal.interrupt_trzsz_transfer();
    fixture.barrier();
    fixture.send(b"\r\nafter transfer\r\n");
    wait_until(|| fixture.terminal.buffer_text().contains("AFTER TRANSFER"));
}

#[test]
#[ignore = "manual release-profile SSH throughput and UI-drain benchmark"]
fn ssh_background_performance() {
    let mode = std::env::var("OXIDETERM_SSH_BENCH_MODE").unwrap_or_else(|_| "worker".into());
    match mode.as_str() {
        "worker" => measure_backend("worker", Fixture::new),
        "sync" => measure_backend("sync", || {
            Fixture::with_backend(
                |config| {
                    SshPtyCore::new(
                        config,
                        80,
                        24,
                        Default::default(),
                        TerminalEncoding::Utf8,
                        1000,
                    )
                },
                |_| {},
            )
        }),
        _ => panic!("benchmark mode must be worker or sync"),
    }
}

fn measure_backend<T: BenchmarkClient>(mode: &str, create: impl Fn() -> Fixture<T>) {
    for workload in ["plain", "ansi", "unicode", "long-csi"] {
        let pattern: &[u8] = match workload {
            "plain" => {
                b"SSH terminal throughput benchmark 0123456789 abcdefghijklmnopqrstuvwxyz\r\n"
            }
            "ansi" => b"\x1b[38;5;42mSSH colored output\x1b[0m cargo check\r\n",
            "unicode" => "SSH 中文输出 e\u{301} 🦀 0123456789\r\n".as_bytes(),
            _ => b"\x1b[1;2;3;4;5;7;8;9;22;23;24;25;27;28;29;38;5;42mX\x1b[0m",
        };
        let mut payload = Vec::with_capacity(16 * 1024 * 1024 + 128);
        while payload.len() < 16 * 1024 * 1024 {
            payload.extend_from_slice(pattern);
        }
        payload.extend_from_slice(b"\r\nBENCHMARK-END\r\n");
        for run in 0..4 {
            let mut fixture = create();
            fixture.terminal.take_events();
            let initial = fixture.terminal.snapshot();
            let mut snapshot = initial;
            let peer = fixture.peer.clone();
            let channel = fixture.channel;
            let corpus = payload.clone();
            let (done_tx, done_rx) = crossbeam_channel::bounded(1);
            let start = Instant::now();
            let cpu_start = cpu_seconds();
            let producer = fixture.runtime.spawn(async move {
                for chunk in corpus.chunks(16 * 1024) {
                    peer.data(channel, chunk.to_vec()).await.unwrap();
                }
                done_tx.send(Instant::now()).unwrap();
            });
            let mut producer_done = None;
            let mut parser_done = None;
            let mut frame_costs = Vec::new();
            let mut snapshot_costs = Vec::new();
            let mut presented_end = false;
            let mut input_latencies = Vec::new();
            let mut input_started = None;
            let mut last_input = start;
            let mut deferred_since: Option<Instant> = None;
            let mut exhausted = false;
            let mut last_frame = start;
            let activity = fixture.terminal.activity_receiver();
            loop {
                let tick = Instant::now();
                let budget = if exhausted {
                    TerminalDrainBudget::throughput()
                } else if tick.duration_since(last_input) < Duration::from_millis(220) {
                    TerminalDrainBudget::interactive()
                } else {
                    TerminalDrainBudget::normal()
                };
                let report = fixture
                    .terminal
                    .read_pending_with_budget(budget.with_max_duration(Duration::from_millis(2)));
                exhausted = report.budget_exhausted;
                fixture.terminal.take_events();
                fixture.terminal.mode();
                if tick.duration_since(last_input) >= Duration::from_millis(5)
                    && input_started.is_none()
                {
                    input_started = Some(Instant::now());
                    fixture.terminal.write_text("ping").unwrap();
                    last_input = tick;
                }
                if let Ok((_, received)) = fixture.input.try_recv() {
                    if let Some(sent) = input_started.take() {
                        input_latencies.push(received.duration_since(sent).as_secs_f64() * 1000.0);
                    }
                }
                if tick.duration_since(last_frame) >= Duration::from_micros(8333) {
                    let snapshot_started = Instant::now();
                    let allow_defer = deferred_since
                        .is_none_or(|since| since.elapsed() < Duration::from_millis(32));
                    if let Some((next, _, _)) =
                        fixture.terminal.try_render_snapshot(&snapshot, allow_defer)
                    {
                        snapshot = next;
                        snapshot_costs.push(snapshot_started.elapsed().as_secs_f64() * 1000.0);
                        presented_end = snapshot
                            .lines
                            .iter()
                            .any(|row| row.text().contains("BENCHMARK-END"));
                        deferred_since = None;
                    } else {
                        deferred_since.get_or_insert(tick);
                    }
                    last_frame = tick;
                }
                frame_costs.push(tick.elapsed().as_secs_f64() * 1000.0);
                if producer_done.is_none() {
                    producer_done = done_rx.try_recv().ok();
                }
                if presented_end {
                    parser_done = Some(Instant::now());
                }
                if let Some(parsed) = parser_done {
                    let emitted = producer_done
                        .or_else(|| done_rx.recv_timeout(Duration::from_secs(1)).ok())
                        .unwrap();
                    frame_costs.sort_by(f64::total_cmp);
                    snapshot_costs.sort_by(f64::total_cmp);
                    input_latencies.sort_by(f64::total_cmp);
                    let p95 = |samples: &[f64]| {
                        if samples.is_empty() {
                            0.0
                        } else {
                            samples[(samples.len() * 95 / 100).min(samples.len() - 1)]
                        }
                    };
                    eprintln!(
                        "SSH_BENCH mode={mode} workload={workload} run={run} bytes={} producer_ms={:.3} parser_ms={:.3} snapshot_ready_ms={:.3} ui_p95_ms={:.3} snapshot_p95_ms={:.3} input_p95_ms={:.3} input_samples={} cpu_s={:.3} peak_rss_bytes={}",
                        payload.len(),
                        emitted.duration_since(start).as_secs_f64() * 1000.0,
                        fixture
                            .terminal
                            .parse_completed()
                            .duration_since(start)
                            .as_secs_f64()
                            * 1000.0,
                        parsed.duration_since(start).as_secs_f64() * 1000.0,
                        p95(&frame_costs),
                        p95(&snapshot_costs),
                        p95(&input_latencies),
                        input_latencies.len(),
                        cpu_seconds() - cpu_start,
                        peak_rss_bytes()
                    );
                    break;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(30),
                    "SSH benchmark stalled"
                );
                if exhausted {
                    std::thread::sleep(Duration::from_millis(1));
                } else {
                    let wait = Duration::from_micros(8333).saturating_sub(last_frame.elapsed());
                    fixture.runtime.block_on(async {
                        let _ = tokio::time::timeout(wait, activity.notified()).await;
                    });
                }
            }
            fixture.runtime.block_on(producer).unwrap();
        }
    }
}

#[cfg(unix)]
fn process_usage() -> libc::rusage {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    assert_eq!(
        unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) },
        0
    );
    unsafe { usage.assume_init() }
}
#[cfg(unix)]
fn cpu_seconds() -> f64 {
    let usage = process_usage();
    (usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) as f64
        + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as f64 / 1_000_000.0
}
#[cfg(unix)]
fn peak_rss_bytes() -> u64 {
    let rss = process_usage().ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        rss
    } else {
        rss * 1024
    }
}
#[cfg(not(unix))]
fn cpu_seconds() -> f64 {
    f64::NAN
}
#[cfg(not(unix))]
fn peak_rss_bytes() -> u64 {
    0
}

trait BenchmarkClient: TerminalSessionBackend {
    fn parse_completed(&self) -> Instant;
}
impl BenchmarkClient for SshPtySession {
    fn parse_completed(&self) -> Instant {
        self.shared.core.lock().last_parse_completed.unwrap()
    }
}
impl BenchmarkClient for SshPtyCore {
    fn parse_completed(&self) -> Instant {
        self.last_parse_completed.unwrap()
    }
}

#[test]
fn ssh_close_during_authentication_releases_the_startup_consumer() {
    let mut peer = ssh_peer::SshPeer::with_auth_delay(Duration::from_secs(1));
    let config = SshSessionConfig::from(peer.config.take().unwrap())
        .with_runtime(peer.runtime.clone())
        .with_registry(
            peer.registry.clone(),
            ConnectionConsumer::Terminal("cancel-startup".into()),
        );
    let mut terminal = SshPtySession::new(
        config,
        80,
        24,
        Default::default(),
        TerminalEncoding::Utf8,
        100,
    );
    wait_until(|| {
        peer.registry
            .list()
            .iter()
            .any(|info| !info.consumers.is_empty())
    });
    terminal.shutdown();
    wait_until(|| terminal.shared.finished.load(Ordering::Acquire));
    assert!(
        peer.registry
            .list()
            .iter()
            .all(|info| info.consumers.is_empty())
    );
    assert!(terminal.take_events().is_empty());
    assert!(peer.ready.try_recv().is_err());
    reap_workers();
}

#[test]
fn ssh_deferred_shell_starts_after_layout_and_closes_without_stale_events() {
    let mut peer = ssh_peer::SshPeer::new();
    let mut config = peer.config.take().unwrap();
    config.post_connect_command = Some("after-layout".into());
    let config = SshSessionConfig::from(config)
        .with_runtime(peer.runtime.clone())
        .with_deferred_pty(true);
    let mut terminal = SshPtySession::new(
        config,
        80,
        24,
        Default::default(),
        TerminalEncoding::Utf8,
        100,
    );
    wait_until(|| terminal.is_interactive());
    assert!(peer.ready.recv_timeout(Duration::from_millis(30)).is_err());
    terminal
        .resize_with_cell_size(TerminalResize::new(100, 30, 8, 16))
        .unwrap();
    peer.ready.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        peer.input.recv_timeout(Duration::from_secs(5)).unwrap().0,
        b"after-layout\r"
    );
    terminal.shutdown();
    wait_until(|| terminal.shared.finished.load(Ordering::Acquire));
    assert!(terminal.take_events().is_empty());
    reap_workers();
}

#[test]
fn ssh_single_large_input_is_delivered_without_truncation() {
    let mut fixture = Fixture::new();
    fixture.barrier();
    let input = "large-input-".repeat(100_000);
    fixture.terminal.write_text(&input).unwrap();
    let mut received = Vec::new();
    while received.len() < input.len() {
        received.extend(
            fixture
                .input
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .0,
        );
    }
    assert_eq!(received, input.as_bytes());
    assert!(fixture.terminal.is_interactive());
}

#[test]
fn retired_ssh_worker_cannot_release_or_publish_into_its_replacement() {
    let mut fixture = Fixture::new();
    let connection = fixture.terminal.ssh_connection_handle().unwrap();
    let keeper = ConnectionConsumer::NodeRouter("fixture-node".into());
    fixture
        .registry
        .acquire_consumer_for_connection(connection.connection_id(), keeper.clone())
        .unwrap();
    fixture.terminal.shutdown();
    let replacement_consumer = ConnectionConsumer::Terminal("replacement".into());
    let config = SshSessionConfig::for_existing_connection(
        connection.connection_id(),
        "127.0.0.1",
        22,
        "parser-test",
    )
    .with_registry(fixture.registry.clone(), replacement_consumer.clone())
    .with_runtime(fixture.runtime.clone());
    let mut replacement = SshPtySession::new(
        config,
        80,
        24,
        Default::default(),
        TerminalEncoding::Utf8,
        100,
    );
    let (peer, channel) = fixture
        .server
        .ready
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    wait_until(|| replacement.is_interactive());
    fixture
        .runtime
        .block_on(peer.data(channel, b"replacement-frame\r\n".to_vec()))
        .unwrap();
    wait_until(|| replacement.buffer_text().contains("replacement-frame"));
    wait_until(|| fixture.terminal.shared.finished.load(Ordering::Acquire));
    assert!(fixture.terminal.take_events().is_empty());
    let info = fixture
        .registry
        .list()
        .into_iter()
        .find(|info| info.connection_id == connection.connection_id())
        .unwrap();
    assert_eq!(info.consumers, vec![keeper.clone(), replacement_consumer]);
    replacement.shutdown();
    wait_until(|| replacement.shared.finished.load(Ordering::Acquire));
    fixture
        .registry
        .release(connection.connection_id(), &keeper);
}

#[test]
fn closing_ssh_stops_a_trzsz_reader_waiting_for_the_peer() {
    let mut fixture = Fixture::new();
    fixture.send(b"::TRZSZ:TRANSFER:R:1.1.6:9\n");
    let mut transfer = None;
    wait_until(|| {
        transfer = fixture.terminal.take_trzsz_transfer();
        transfer.is_some()
    });
    let mut transfer = transfer.unwrap();
    let cancel = transfer.input_handle();
    let (done_tx, done_rx) = crossbeam_channel::bounded(1);
    let reader = std::thread::spawn(move || {
        done_tx.send(transfer.recv_config()).unwrap();
    });
    fixture.terminal.shutdown();
    let result = done_rx.recv_timeout(Duration::from_secs(5));
    if result.is_err() {
        cancel.stop_transferring();
    }
    reader.join().unwrap();
    assert!(
        matches!(result.unwrap(), Err(oxideterm_trzsz::TrzszError::InvalidState(reason)) if reason == "Stopped")
    );
    wait_until(|| fixture.terminal.shared.finished.load(Ordering::Acquire));
}

#[test]
fn manual_modem_transfer_handoff_preserves_binary_data_and_wakes_writes() {
    let mut fixture = Fixture::new();
    fixture
        .terminal
        .begin_modem_transfer(TerminalModemTransferRequest {
            protocol: oxideterm_modem_transfer::DetectedModemProtocol::Xmodem,
            direction: oxideterm_modem_transfer::ModemTransferDirection::Upload,
        })
        .unwrap();
    let mut transfer = None;
    wait_until(|| {
        for event in fixture.terminal.take_events() {
            if let TerminalEvent::ModemTransferPrompt {
                transfer: ready, ..
            } = event
            {
                transfer = Some(ready);
            }
        }
        transfer.is_some()
    });
    let mut transfer = transfer.unwrap();
    ModemIo::write_all(&mut transfer, b"modem-write").unwrap();
    assert_eq!(
        fixture
            .input
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .0,
        b"modem-write"
    );
    let mut binary: Vec<u8> = (0..=255).collect();
    binary.extend_from_slice(&[0xe4, 0xbd]);
    fixture.send(&binary);
    let mut received = Vec::new();
    wait_until(|| {
        received.extend(transfer.drain_remote_output());
        received.len() >= binary.len()
    });
    assert_eq!(received, binary);
    fixture.terminal.shutdown();
    wait_until(|| fixture.terminal.shared.finished.load(Ordering::Acquire));
    assert!(matches!(
        transfer.read_byte(Duration::from_millis(10)),
        Err(oxideterm_modem_transfer::ModemTransferError::Cancelled)
    ));
}

#[test]
fn ssh_parser_failure_is_reported_once_without_falling_back_to_ui_parsing() {
    let mut fixture = Fixture::new();
    fixture
        .terminal
        .set_output_processor(Some(Arc::new(|_| panic!("fixture processor failure"))));
    fixture.barrier();
    fixture.terminal.take_events();
    fixture.send(b"trigger failure");
    wait_until(|| fixture.terminal.shared.finished.load(Ordering::Acquire));
    let events = fixture.terminal.take_events();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, TerminalEvent::ProcessingFailed))
            .count(),
        1
    );
    assert!(!fixture.terminal.is_interactive());
    assert!(fixture.terminal.take_events().is_empty());
    assert!(!fixture.terminal.buffer_text().contains("trigger failure"));
}

#[test]
fn ssh_packet_splits_preserve_grid_modes_selection_graphics_and_private_recording() {
    let prefix = "line0\r\n\x1b[31mred\x1b[0m 中文 e\u{301}\r\n\x1b[?1049hhidden\x1b[?1049l";
    let image =
        "\x1b7\x1b[6;41H\x1b_Gq=2,a=T,i=7,z=-1,C=1,f=24,s=2,v=2,m=0;AAAA/////wAAAP8A\x1b\\\x1b8";
    let private =
        "\x1b]7719;v=3;kind=editor-clipboard;app=vim;op=copy;data=%73%65%63%72%65%74\x1b\\";
    let suffix = "\x1b[?2004hend\x1b[6n";
    let transcript = format!("{prefix}{image}{private}{suffix}").into_bytes();
    let expected_recording = format!("{prefix}\x1b7\x1b[6;41H\x1b8{suffix}").into_bytes();
    let selection = Some(crate::TerminalSelectionRange {
        start_line: 1,
        start_col: 0,
        end_line: 1,
        end_col: 2,
        is_block: false,
    });
    let mut baseline = SshPtyCore::new_disconnected_for_test(
        SshSessionConfig::new("localhost", 22, "fixture"),
        80,
        24,
        Default::default(),
        TerminalEncoding::Utf8,
        1000,
    );
    baseline
        .resize_with_cell_size(TerminalResize::new(80, 24, 10, 20))
        .unwrap();
    baseline.set_output_events_enabled(true);
    baseline.take_events();
    baseline.parser_state.feed_transport_output(&transcript);
    baseline.read_pending();
    baseline.set_selection(selection);
    let expected = baseline.snapshot();
    assert_eq!(
        baseline.buffer_text().lines().take(3).collect::<Vec<_>>(),
        ["line0", "red 中文 e\u{301}", "end"]
    );
    assert_eq!((expected.cursor_row, expected.cursor_col), (2, 3));
    assert!(baseline.mode().contains(TermMode::BRACKETED_PASTE));
    assert_eq!(expected.images.len(), 1);
    let image = &expected.images[0];
    assert_eq!(
        (image.row, image.col, image.pixel_width, image.pixel_height),
        (5, 40, 2, 2)
    );
    assert_eq!(
        &*image.data.as_ref().unwrap().rgba,
        &[
            0, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 255, 0, 255, 0, 255
        ]
    );
    for chunk_size in [1, 7, transcript.len()] {
        let mut fixture = Fixture::new();
        fixture
            .terminal
            .resize_with_cell_size(TerminalResize::new(80, 24, 10, 20))
            .unwrap();
        fixture.terminal.set_output_events_enabled(true);
        fixture.barrier();
        fixture.terminal.take_events();
        for chunk in transcript.chunks(chunk_size) {
            fixture.send(chunk);
            if chunk_size < transcript.len() {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert_eq!(
            fixture
                .input
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .0,
            b"\x1b[3;4R"
        );
        fixture.terminal.set_selection(selection);
        fixture.barrier();
        let actual = fixture.terminal.snapshot();
        assert_eq!(
            actual
                .lines
                .iter()
                .map(|row| &row.cells)
                .collect::<Vec<_>>(),
            expected
                .lines
                .iter()
                .map(|row| &row.cells)
                .collect::<Vec<_>>(),
            "chunk size {chunk_size}"
        );
        assert_eq!(actual.images, expected.images);
        assert_eq!(fixture.terminal.mode(), baseline.mode());
        assert_eq!(fixture.terminal.selection(), selection);
        let mut recording = Vec::new();
        let mut clipboard = None;
        for event in fixture.terminal.take_events() {
            match event {
                TerminalEvent::Output(bytes) => recording.extend(bytes),
                TerminalEvent::EditorClipboard(event) => clipboard = Some(event.text),
                _ => {}
            }
        }
        assert_eq!(recording, expected_recording);
        assert_eq!(clipboard.as_deref().map(String::as_str), Some("secret"));
    }
}

#[test]
fn ssh_trzsz_backpressure_stops_at_the_transfer_buffer_and_cancel_wakes_it() {
    let mut fixture = Fixture::new();
    fixture.send(b"::TRZSZ:TRANSFER:R:1.1.6:9\n");
    let mut transfer = None;
    wait_until(|| {
        transfer = fixture.terminal.take_trzsz_transfer();
        transfer.is_some()
    });
    let mut transfer = transfer.unwrap();
    let input = transfer.input_handle();
    let peer = fixture.peer.clone();
    let channel = fixture.channel;
    let producer = fixture.runtime.spawn(async move {
        for _ in 0..1024 {
            if peer.data(channel, vec![b'x'; 4096]).await.is_err() {
                break;
            }
        }
    });
    let limit = oxideterm_trzsz::MAX_TRANSFER_CHUNK_SIZE * 2;
    wait_until(|| input.buffered_bytes() >= limit);
    let queued = input.buffered_bytes();
    assert!(queued <= limit + SSH_OUTPUT_PARSE_SLICE_BYTES);
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(input.buffered_bytes(), queued);
    transfer.stop_transferring();
    fixture.terminal.interrupt_trzsz_transfer();
    fixture.barrier();
    assert_eq!(input.buffered_bytes(), 0);
    fixture.terminal.shutdown();
    wait_until(|| fixture.terminal.shared.finished.load(Ordering::Acquire));
    producer.abort();
}
