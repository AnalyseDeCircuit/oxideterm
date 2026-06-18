// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only
//
// This module adapts the PTY event-loop structure used by alacritty_terminal
// (Apache-2.0 OR MIT) so OxideTerm can intercept graphics protocols between
// PTY reads and the ANSI parser. The graphics interception, event routing, and
// public integration points are OxideTerm-specific.

use std::{
    borrow::Cow,
    cell::Cell,
    collections::VecDeque,
    fmt::{self, Display, Formatter},
    io::{self, ErrorKind, Read, Write},
    num::NonZeroUsize,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::JoinHandle,
    time::Instant,
};

use alacritty_terminal::{
    event::{Event, EventListener, Notify, OnResize, WindowSize},
    sync::FairMutex,
    term::{Term, TermMode},
    tty::{self, EventedPty, EventedReadWrite},
    vte::ansi,
};
use crossbeam_channel::Sender as CrossbeamSender;
use oxideterm_modem_transfer::{ModemConsumer, ModemConsumerEvent, ModemTransfer};
use oxideterm_terminal_encoding::{
    EncodingMismatchDetector, TerminalEncoding, TerminalOutputDecoder,
};
use oxideterm_terminal_graphics::{
    GraphicsIngress, GraphicsOptions, TerminalGraphicsEvent, TerminalGraphicsSegment,
};
use polling::{Event as PollingEvent, Events, PollMode, Poller};

use crate::{
    TerminalEvent, TerminalOutputProcessor, TerminalSize,
    backpressure::{
        LOCAL_MAX_LOCKED_PARSE_BYTES, LOCAL_PTY_READ_BUFFER_BYTES, MagicScanWindow,
        TerminalMagicKind, Utf8ResidualGuard,
    },
    graphics_cursor_from_term,
    shell_integration::TerminalShellIntegration,
};
#[cfg(windows)]
const PTY_READ_WRITE_TOKEN: usize = 2;
#[cfg(not(windows))]
const PTY_READ_WRITE_TOKEN: usize = 0;
const PTY_CHILD_EVENT_TOKEN: usize = 1;

pub(crate) enum LocalGraphicsMsg {
    Input(Cow<'static, [u8]>),
    Shutdown,
    Resize(WindowSize),
    SetEncoding(TerminalEncoding),
    SetOutputProcessor(Option<TerminalOutputProcessor>),
    SetOutputEventsEnabled(bool),
    StartModemTransfer {
        request: crate::TerminalModemTransferRequest,
        response_tx: Sender<Option<ModemTransfer>>,
    },
    FinishModemTransfer,
    InterruptModemTransfer,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LocalPtyReadReport {
    pub(crate) raw_bytes: usize,
    pub(crate) parsed_bytes: usize,
    pub(crate) budget_exhausted: bool,
}

pub(crate) struct LocalGraphicsEventLoop<U: EventListener> {
    poll: Arc<Poller>,
    pty: tty::Pty,
    rx: PeekableReceiver<LocalGraphicsMsg>,
    tx: Sender<LocalGraphicsMsg>,
    terminal: Arc<FairMutex<Term<U>>>,
    event_proxy: U,
    drain_on_exit: bool,
    graphics_tx: CrossbeamSender<TerminalGraphicsEvent>,
    magic_tx: CrossbeamSender<TerminalMagicKind>,
    event_tx: CrossbeamSender<TerminalEvent>,
    stats_tx: CrossbeamSender<LocalPtyReadReport>,
    size: TerminalSize,
    graphics_options: GraphicsOptions,
    encoding: TerminalEncoding,
}

impl<U> LocalGraphicsEventLoop<U>
where
    U: EventListener + Send + 'static,
{
    pub(crate) fn new(
        terminal: Arc<FairMutex<Term<U>>>,
        event_proxy: U,
        pty: tty::Pty,
        drain_on_exit: bool,
        graphics_tx: CrossbeamSender<TerminalGraphicsEvent>,
        magic_tx: CrossbeamSender<TerminalMagicKind>,
        terminal_event_tx: CrossbeamSender<TerminalEvent>,
        stats_tx: CrossbeamSender<LocalPtyReadReport>,
        size: TerminalSize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
    ) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        Ok(Self {
            poll: Poller::new()?.into(),
            pty,
            rx: PeekableReceiver::new(rx),
            tx,
            terminal,
            event_proxy,
            drain_on_exit,
            graphics_tx,
            magic_tx,
            event_tx: terminal_event_tx,
            stats_tx,
            size,
            graphics_options,
            encoding,
        })
    }

    fn advance_guarded_bytes(
        state: &mut LocalGraphicsState,
        terminal: &mut Term<U>,
        bytes: &[u8],
        size: TerminalSize,
        magic_tx: &CrossbeamSender<TerminalMagicKind>,
        event_tx: &CrossbeamSender<TerminalEvent>,
        graphics_tx: &CrossbeamSender<TerminalGraphicsEvent>,
    ) -> (usize, bool) {
        for kind in state.magic_scan.scan(bytes) {
            let _ = magic_tx.send(kind);
        }

        let cursor = Cell::new(graphics_cursor_from_term(terminal, size));
        let mut parsed_bytes = 0usize;
        let mut graphics_changed = false;
        let mut priority_writes = Vec::new();
        {
            let graphics = &mut state.graphics;
            let parser = &mut state.parser;
            let encoding_detector = &mut state.encoding_detector;
            let output_decoder = &mut state.output_decoder;
            let output_events_enabled = state.output_events_enabled;
            let shell_integration = &mut state.shell_integration;
            let alt_screen_active = &mut state.alt_screen_active;
            graphics.advance_ordered(
                bytes,
                |segment| match segment {
                    TerminalGraphicsSegment::Terminal(terminal_bytes) => {
                        if let Some(hint) = encoding_detector.observe(&terminal_bytes) {
                            let _ = event_tx.send(TerminalEvent::EncodingHint(hint));
                        }
                        let decoded = output_decoder.decode_to_utf8_bytes(&terminal_bytes);
                        parsed_bytes += terminal_bytes.len();
                        if output_events_enabled && !decoded.is_empty() {
                            // Tauri feeds decoded display text into TerminalRecorder after xterm
                            // receives it. Keep the native recorder on the same side of encoding
                            // detection instead of recording raw PTY bytes.
                            let _ = event_tx.send(TerminalEvent::Output(decoded.as_ref().to_vec()));
                        }
                        shell_integration.advance(parser, terminal, decoded.as_ref(), |event| {
                            let _ = event_tx.send(event);
                        });
                        if let Some(event) =
                            LocalGraphicsState::alt_screen_clear_event(alt_screen_active, terminal)
                        {
                            graphics_changed = true;
                            let _ = graphics_tx.send(event);
                        }
                        cursor.set(graphics_cursor_from_term(terminal, size));
                    }
                    TerminalGraphicsSegment::Event(TerminalGraphicsEvent::Respond(bytes)) => {
                        priority_writes.push(Cow::Owned(bytes));
                    }
                    TerminalGraphicsSegment::Event(event) => {
                        graphics_changed = true;
                        let _ = graphics_tx.send(event);
                    }
                },
                || cursor.get(),
            );
        }

        for bytes in priority_writes {
            state.push_priority_write(bytes);
        }

        (parsed_bytes, graphics_changed)
    }

    fn advance_plain_output(
        state: &mut LocalGraphicsState,
        terminal: &mut Term<U>,
        bytes: &[u8],
        size: TerminalSize,
        magic_tx: &CrossbeamSender<TerminalMagicKind>,
        event_tx: &CrossbeamSender<TerminalEvent>,
        graphics_tx: &CrossbeamSender<TerminalGraphicsEvent>,
    ) -> (usize, bool) {
        state
            .utf8_guard
            .push(bytes)
            .map(|guarded| {
                Self::advance_guarded_bytes(
                    state,
                    terminal,
                    &guarded,
                    size,
                    magic_tx,
                    event_tx,
                    graphics_tx,
                )
            })
            .unwrap_or_default()
    }

    fn advance_processed_output(
        state: &mut LocalGraphicsState,
        terminal: &mut Term<U>,
        bytes: &[u8],
        size: TerminalSize,
        magic_tx: &CrossbeamSender<TerminalMagicKind>,
        event_tx: &CrossbeamSender<TerminalEvent>,
        graphics_tx: &CrossbeamSender<TerminalGraphicsEvent>,
    ) -> (usize, bool) {
        let events = state.modem_consumer.process_server_output(bytes);
        Self::handle_modem_consumer_events(
            state,
            terminal,
            events,
            size,
            magic_tx,
            event_tx,
            graphics_tx,
        )
    }

    fn handle_modem_consumer_events(
        state: &mut LocalGraphicsState,
        terminal: &mut Term<U>,
        events: Vec<ModemConsumerEvent>,
        size: TerminalSize,
        magic_tx: &CrossbeamSender<TerminalMagicKind>,
        event_tx: &CrossbeamSender<TerminalEvent>,
        graphics_tx: &CrossbeamSender<TerminalGraphicsEvent>,
    ) -> (usize, bool) {
        let mut parsed_bytes = 0usize;
        let mut graphics_changed = false;
        for event in events {
            match event {
                ModemConsumerEvent::WriteTerminal(bytes) => {
                    let (bytes, changed) = Self::advance_plain_output(
                        state,
                        terminal,
                        &bytes,
                        size,
                        magic_tx,
                        event_tx,
                        graphics_tx,
                    );
                    parsed_bytes += bytes;
                    graphics_changed |= changed;
                }
                ModemConsumerEvent::SendServer(bytes) => {
                    state.push_priority_write(Cow::Owned(bytes));
                }
                ModemConsumerEvent::TransferStarted(request) => {
                    if let Some(transfer) = state.modem_consumer.active_transfer().cloned() {
                        let _ =
                            event_tx.send(TerminalEvent::ModemTransferPrompt { request, transfer });
                    }
                }
                ModemConsumerEvent::TransferDataQueued => {}
                ModemConsumerEvent::TransferCancelRequested => {}
            }
        }
        (parsed_bytes, graphics_changed)
    }

    pub(crate) fn channel(&self) -> LocalGraphicsEventLoopSender {
        LocalGraphicsEventLoopSender {
            sender: self.tx.clone(),
            poller: self.poll.clone(),
        }
    }

    fn drain_recv_channel(&mut self, state: &mut LocalGraphicsState) -> bool {
        while let Some(msg) = self.rx.recv() {
            match msg {
                LocalGraphicsMsg::Input(input) => state.write_list.push_back(input),
                LocalGraphicsMsg::Resize(window_size) => {
                    self.size = TerminalSize {
                        cols: window_size.num_cols as usize,
                        rows: window_size.num_lines as usize,
                        cell_width: window_size.cell_width,
                        cell_height: window_size.cell_height,
                    };
                    self.pty.on_resize(window_size);
                }
                LocalGraphicsMsg::SetEncoding(encoding) => {
                    state.set_encoding(encoding);
                }
                LocalGraphicsMsg::SetOutputProcessor(processor) => {
                    state.output_processor = processor;
                    state.utf8_guard = Utf8ResidualGuard::default();
                }
                LocalGraphicsMsg::SetOutputEventsEnabled(enabled) => {
                    state.output_events_enabled = enabled;
                }
                LocalGraphicsMsg::StartModemTransfer {
                    request,
                    response_tx,
                } => {
                    let _ = response_tx.send(state.modem_consumer.start_manual_transfer(request));
                }
                LocalGraphicsMsg::FinishModemTransfer => {
                    state.modem_consumer.finish_transfer();
                }
                LocalGraphicsMsg::InterruptModemTransfer => {
                    state.modem_consumer.interrupt_transfer();
                }
                LocalGraphicsMsg::Shutdown => return false,
            }
        }

        true
    }

    fn pty_read(
        &mut self,
        state: &mut LocalGraphicsState,
        buf: &mut [u8],
    ) -> io::Result<LocalPtyReadReport> {
        let mut unprocessed = 0;
        let mut processed = 0;
        let mut raw_bytes = 0;
        let mut graphics_changed = false;
        let mut budget_exhausted = false;
        let terminal_lease = self.terminal.lease();
        let mut terminal = None;

        loop {
            match self.pty.reader().read(&mut buf[unprocessed..]) {
                Ok(0) if unprocessed == 0 => break,
                Ok(got) => {
                    unprocessed += got;
                    raw_bytes += got;
                }
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted | ErrorKind::WouldBlock if unprocessed == 0 => break,
                    ErrorKind::Interrupted | ErrorKind::WouldBlock => {}
                    _ => return Err(err),
                },
            }

            let terminal = match &mut terminal {
                Some(terminal) => terminal,
                None => terminal.insert(match self.terminal.try_lock_unfair() {
                    None if unprocessed >= LOCAL_PTY_READ_BUFFER_BYTES => {
                        self.terminal.lock_unfair()
                    }
                    None => continue,
                    Some(terminal) => terminal,
                }),
            };

            let processed_output = state.process_output(&buf[..unprocessed]);
            let (parsed_bytes, changed) = Self::advance_processed_output(
                state,
                terminal,
                processed_output.as_ref(),
                self.size,
                &self.magic_tx,
                &self.event_tx,
                &self.graphics_tx,
            );
            graphics_changed |= changed;

            processed += parsed_bytes;
            unprocessed = 0;

            if processed >= LOCAL_MAX_LOCKED_PARSE_BYTES {
                budget_exhausted = true;
                break;
            }
        }

        drop(terminal);
        drop(terminal_lease);

        if state.needs_write() {
            self.pty_write(state)?;
        }

        if graphics_changed || (state.parser.sync_bytes_count() < processed && processed > 0) {
            self.event_proxy.send_event(Event::Wakeup);
        }

        Ok(LocalPtyReadReport {
            raw_bytes,
            parsed_bytes: processed,
            budget_exhausted,
        })
    }

    fn flush_utf8_residual(
        &mut self,
        state: &mut LocalGraphicsState,
    ) -> io::Result<LocalPtyReadReport> {
        let Some(residual) = state.utf8_guard.flush() else {
            return Ok(LocalPtyReadReport::default());
        };
        let raw_bytes = residual.len();

        let size = self.size;
        let magic_tx = self.magic_tx.clone();
        let graphics_tx = self.graphics_tx.clone();
        let event_tx = self.event_tx.clone();
        let mut terminal = self.terminal.lock_unfair();
        let (processed, graphics_changed) = Self::advance_guarded_bytes(
            state,
            &mut *terminal,
            &residual,
            size,
            &magic_tx,
            &event_tx,
            &graphics_tx,
        );
        drop(terminal);

        if state.needs_write() {
            self.pty_write(state)?;
        }

        if graphics_changed || processed > 0 {
            self.event_proxy.send_event(Event::Wakeup);
        }

        Ok(LocalPtyReadReport {
            raw_bytes,
            parsed_bytes: processed,
            budget_exhausted: false,
        })
    }

    fn pty_write(&mut self, state: &mut LocalGraphicsState) -> io::Result<()> {
        state.ensure_next();

        'write_many: while let Some(mut current) = state.take_current() {
            'write_one: loop {
                match self.pty.writer().write(current.remaining_bytes()) {
                    Ok(0) => {
                        state.set_current(Some(current));
                        break 'write_many;
                    }
                    Ok(n) => {
                        current.advance(n);
                        if current.finished() {
                            state.goto_next();
                            break 'write_one;
                        }
                    }
                    Err(err) => {
                        state.set_current(Some(current));
                        match err.kind() {
                            ErrorKind::Interrupted | ErrorKind::WouldBlock => break 'write_many,
                            _ => return Err(err),
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn flush_modem_server_writes(&mut self, state: &mut LocalGraphicsState) -> bool {
        let mut queued = false;
        for bytes in state.modem_consumer.take_server_writes() {
            state.push_priority_write(Cow::Owned(bytes));
            queued = true;
        }
        queued
    }

    pub(crate) fn spawn(mut self) -> JoinHandle<()> {
        std::thread::Builder::new()
            .name("OxideTerm PTY graphics reader".to_string())
            .spawn(move || {
                let modem_poller = self.poll.clone();
                let mut state = LocalGraphicsState::new(
                    self.graphics_options.clone(),
                    self.encoding,
                    Arc::new(move || {
                        // Modem workers run outside this event loop, so wake the
                        // poller when they enqueue protocol bytes for the PTY.
                        let _ = modem_poller.notify();
                    }),
                );
                let mut buf = [0u8; LOCAL_PTY_READ_BUFFER_BYTES];
                let poll_opts = PollMode::Level;
                let mut interest = PollingEvent::readable(0);

                if let Err(error) = unsafe { self.pty.register(&self.poll, interest, poll_opts) } {
                    tracing::error!(%error, "local graphics event loop registration failed");
                    return;
                }

                let mut events = Events::with_capacity(NonZeroUsize::new(1024).unwrap());

                'event_loop: loop {
                    let handler = state.parser.sync_timeout();
                    let timeout = handler
                        .sync_timeout()
                        .map(|deadline| deadline.saturating_duration_since(Instant::now()));

                    events.clear();
                    if let Err(error) = self.poll.wait(&mut events, timeout) {
                        match error.kind() {
                            ErrorKind::Interrupted => continue,
                            _ => {
                                tracing::error!(%error, "local graphics event loop poll failed");
                                break 'event_loop;
                            }
                        }
                    }

                    let modem_writes_queued = self.flush_modem_server_writes(&mut state);
                    if events.is_empty() && self.rx.peek().is_none() && !modem_writes_queued {
                        state.parser.stop_sync(&mut *self.terminal.lock());
                        self.event_proxy.send_event(Event::Wakeup);
                        continue;
                    }

                    if !self.drain_recv_channel(&mut state) {
                        break;
                    }

                    for event in events.iter() {
                        match event.key {
                            PTY_CHILD_EVENT_TOKEN => {
                                if let Some(tty::ChildEvent::Exited(status)) =
                                    self.pty.next_child_event()
                                {
                                    if let Some(status) = status {
                                        self.event_proxy.send_event(Event::ChildExit(status));
                                    }
                                    if self.drain_on_exit {
                                        if let Ok(report) = self.pty_read(&mut state, &mut buf) {
                                            self.send_read_report(report);
                                        }
                                    }
                                    if let Ok(report) = self.flush_utf8_residual(&mut state) {
                                        self.send_read_report(report);
                                    }
                                    self.terminal.lock().exit();
                                    self.event_proxy.send_event(Event::Wakeup);
                                    break 'event_loop;
                                }
                            }
                            PTY_READ_WRITE_TOKEN => {
                                if event.is_interrupt() {
                                    continue;
                                }

                                if event.readable {
                                    match self.pty_read(&mut state, &mut buf) {
                                        Ok(report) => self.send_read_report(report),
                                        Err(error) => {
                                            #[cfg(target_os = "linux")]
                                            if error.raw_os_error() == Some(libc::EIO) {
                                                continue;
                                            }

                                            tracing::error!(
                                                %error,
                                                "local graphics event loop PTY read failed"
                                            );
                                            break 'event_loop;
                                        }
                                    }
                                }

                                if event.writable
                                    && let Err(error) = self.pty_write(&mut state)
                                {
                                    tracing::error!(
                                        %error,
                                        "local graphics event loop PTY write failed"
                                    );
                                    break 'event_loop;
                                }
                            }
                            _ => {}
                        }
                    }

                    if modem_writes_queued && let Err(error) = self.pty_write(&mut state) {
                        tracing::error!(
                            %error,
                            "local graphics event loop modem PTY write failed"
                        );
                        break 'event_loop;
                    }

                    let needs_write = state.needs_write();
                    if needs_write != interest.writable {
                        interest.writable = needs_write;
                        if let Err(error) = self.pty.reregister(&self.poll, interest, poll_opts) {
                            tracing::error!(
                                %error,
                                "local graphics event loop PTY reregister failed"
                            );
                            break 'event_loop;
                        }
                    }
                }

                let _ = self.pty.deregister(&self.poll);
            })
            .expect("failed to spawn local graphics event loop")
    }

    fn send_read_report(&self, report: LocalPtyReadReport) {
        if report.raw_bytes > 0 || report.parsed_bytes > 0 || report.budget_exhausted {
            let _ = self.stats_tx.send(report);
        }
    }
}

struct Writing {
    source: Cow<'static, [u8]>,
    written: usize,
}

pub(crate) struct LocalGraphicsNotifier(pub(crate) LocalGraphicsEventLoopSender);

impl Notify for LocalGraphicsNotifier {
    fn notify<B>(&self, bytes: B)
    where
        B: Into<Cow<'static, [u8]>>,
    {
        let bytes = bytes.into();
        if !bytes.is_empty() {
            let _ = self.0.send(LocalGraphicsMsg::Input(bytes));
        }
    }
}

impl OnResize for LocalGraphicsNotifier {
    fn on_resize(&mut self, window_size: WindowSize) {
        let _ = self.0.send(LocalGraphicsMsg::Resize(window_size));
    }
}

#[derive(Debug)]
pub(crate) enum EventLoopSendError {
    Io(io::Error),
    Send(mpsc::SendError<LocalGraphicsMsg>),
}

impl Display for EventLoopSendError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(f),
            Self::Send(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for EventLoopSendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => error.source(),
            Self::Send(error) => error.source(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct LocalGraphicsEventLoopSender {
    sender: Sender<LocalGraphicsMsg>,
    poller: Arc<Poller>,
}

impl LocalGraphicsEventLoopSender {
    pub(crate) fn send(&self, msg: LocalGraphicsMsg) -> Result<(), EventLoopSendError> {
        self.sender.send(msg).map_err(EventLoopSendError::Send)?;
        self.poller.notify().map_err(EventLoopSendError::Io)
    }
}

struct LocalGraphicsState {
    write_list: VecDeque<Cow<'static, [u8]>>,
    writing: Option<Writing>,
    parser: ansi::Processor,
    graphics: GraphicsIngress,
    utf8_guard: Utf8ResidualGuard,
    magic_scan: MagicScanWindow,
    output_processor: Option<TerminalOutputProcessor>,
    output_events_enabled: bool,
    output_decoder: TerminalOutputDecoder,
    encoding_detector: EncodingMismatchDetector,
    shell_integration: TerminalShellIntegration,
    modem_consumer: ModemConsumer,
    alt_screen_active: bool,
}

impl LocalGraphicsState {
    fn new(
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        modem_wake: Arc<dyn Fn() + Send + Sync + 'static>,
    ) -> Self {
        Self {
            write_list: VecDeque::new(),
            writing: None,
            parser: ansi::Processor::new(),
            graphics: GraphicsIngress::new(graphics_options),
            utf8_guard: Utf8ResidualGuard::default(),
            magic_scan: MagicScanWindow::default(),
            output_processor: None,
            output_events_enabled: false,
            output_decoder: TerminalOutputDecoder::new(encoding),
            encoding_detector: EncodingMismatchDetector::new(encoding),
            shell_integration: TerminalShellIntegration::default(),
            modem_consumer: ModemConsumer::with_wake(modem_wake),
            alt_screen_active: false,
        }
    }

    fn set_encoding(&mut self, encoding: TerminalEncoding) {
        self.output_decoder.set_encoding(encoding);
        self.output_decoder.reset();
        self.encoding_detector.set_encoding(encoding);
    }

    fn process_output<'a>(&self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        let Some(processor) = &self.output_processor else {
            return Cow::Borrowed(bytes);
        };
        // Output processors sit before UTF-8 buffering and ANSI parsing so
        // transformed bytes, including suppression, are what the terminal sees.
        Cow::Owned(processor(bytes))
    }

    fn ensure_next(&mut self) {
        if self.writing.is_none() {
            self.goto_next();
        }
    }

    fn goto_next(&mut self) {
        self.writing = self.write_list.pop_front().map(Writing::new);
    }

    fn take_current(&mut self) -> Option<Writing> {
        self.writing.take()
    }

    fn needs_write(&self) -> bool {
        self.writing.is_some() || !self.write_list.is_empty()
    }

    fn set_current(&mut self, next: Option<Writing>) {
        self.writing = next;
    }

    fn push_priority_write(&mut self, bytes: Cow<'static, [u8]>) {
        if bytes.is_empty() {
            return;
        }

        self.write_list.push_front(bytes);
        self.ensure_next();
    }

    fn alt_screen_clear_event<U: EventListener>(
        alt_screen_active: &mut bool,
        terminal: &Term<U>,
    ) -> Option<TerminalGraphicsEvent> {
        let next_active = terminal.mode().contains(TermMode::ALT_SCREEN);
        if next_active == *alt_screen_active {
            return None;
        }

        *alt_screen_active = next_active;
        // Local PTY graphics events are applied on the UI/session side, so emit a
        // protocol-level clear at the same ordered point as the screen switch.
        Some(TerminalGraphicsEvent::Delete { id: None })
    }
}

impl Writing {
    fn new(source: Cow<'static, [u8]>) -> Self {
        Self { source, written: 0 }
    }

    fn advance(&mut self, amount: usize) {
        self.written += amount;
    }

    fn remaining_bytes(&self) -> &[u8] {
        &self.source[self.written..]
    }

    fn finished(&self) -> bool {
        self.written >= self.source.len()
    }
}

struct PeekableReceiver<T> {
    rx: Receiver<T>,
    peeked: Option<T>,
}

impl<T> PeekableReceiver<T> {
    fn new(rx: Receiver<T>) -> Self {
        Self { rx, peeked: None }
    }

    fn peek(&mut self) -> Option<&T> {
        if self.peeked.is_none() {
            self.peeked = self.rx.try_recv().ok();
        }
        self.peeked.as_ref()
    }

    fn recv(&mut self) -> Option<T> {
        if self.peeked.is_some() {
            self.peeked.take()
        } else {
            match self.rx.try_recv() {
                Err(TryRecvError::Disconnected) => panic!("local graphics event loop closed"),
                result => result.ok(),
            }
        }
    }
}
