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
    term::Term,
    tty::{self, EventedPty, EventedReadWrite},
    vte::ansi,
};
use crossbeam_channel::Sender as CrossbeamSender;
use oxideterm_terminal_graphics::{GraphicsIngress, GraphicsOptions, TerminalGraphicsEvent};
use polling::{Event as PollingEvent, Events, PollMode, Poller};

use crate::{TerminalSize, graphics_cursor_from_term};

const READ_BUFFER_SIZE: usize = 0x10_0000;
const MAX_LOCKED_READ: usize = u16::MAX as usize;
#[cfg(windows)]
const PTY_READ_WRITE_TOKEN: usize = 2;
#[cfg(not(windows))]
const PTY_READ_WRITE_TOKEN: usize = 0;
const PTY_CHILD_EVENT_TOKEN: usize = 1;

#[derive(Debug)]
pub(crate) enum LocalGraphicsMsg {
    Input(Cow<'static, [u8]>),
    Shutdown,
    Resize(WindowSize),
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
    size: TerminalSize,
    graphics_options: GraphicsOptions,
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
        size: TerminalSize,
        graphics_options: GraphicsOptions,
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
            size,
            graphics_options,
        })
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
                LocalGraphicsMsg::Shutdown => return false,
            }
        }

        true
    }

    fn pty_read(&mut self, state: &mut LocalGraphicsState, buf: &mut [u8]) -> io::Result<()> {
        let mut unprocessed = 0;
        let mut processed = 0;
        let mut graphics_changed = false;
        let terminal_lease = self.terminal.lease();
        let mut terminal = None;

        loop {
            match self.pty.reader().read(&mut buf[unprocessed..]) {
                Ok(0) if unprocessed == 0 => break,
                Ok(got) => unprocessed += got,
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted | ErrorKind::WouldBlock if unprocessed == 0 => break,
                    ErrorKind::Interrupted | ErrorKind::WouldBlock => {}
                    _ => return Err(err),
                },
            }

            let terminal = match &mut terminal {
                Some(terminal) => terminal,
                None => terminal.insert(match self.terminal.try_lock_unfair() {
                    None if unprocessed >= READ_BUFFER_SIZE => self.terminal.lock_unfair(),
                    None => continue,
                    Some(terminal) => terminal,
                }),
            };

            let cursor = Cell::new(graphics_cursor_from_term(&**terminal, self.size));
            let mut parsed_bytes = 0usize;
            let events = state.graphics.advance_with(
                &buf[..unprocessed],
                |terminal_bytes| {
                    parsed_bytes += terminal_bytes.len();
                    state.parser.advance(&mut **terminal, terminal_bytes);
                    cursor.set(graphics_cursor_from_term(&**terminal, self.size));
                },
                || cursor.get(),
            );

            if !events.is_empty() {
                for event in events {
                    match event {
                        TerminalGraphicsEvent::Respond(bytes) => {
                            state.push_priority_write(Cow::Owned(bytes));
                        }
                        event => {
                            graphics_changed = true;
                            let _ = self.graphics_tx.send(event);
                        }
                    }
                }
            }

            processed += parsed_bytes;
            unprocessed = 0;

            if processed >= MAX_LOCKED_READ {
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

        Ok(())
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

    pub(crate) fn spawn(mut self) -> JoinHandle<()> {
        std::thread::Builder::new()
            .name("OxideTerm PTY graphics reader".to_string())
            .spawn(move || {
                let mut state = LocalGraphicsState::new(self.graphics_options.clone());
                let mut buf = [0u8; READ_BUFFER_SIZE];
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

                    if events.is_empty() && self.rx.peek().is_none() {
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
                                        let _ = self.pty_read(&mut state, &mut buf);
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

                                if event.readable
                                    && let Err(error) = self.pty_read(&mut state, &mut buf)
                                {
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
}

impl LocalGraphicsState {
    fn new(graphics_options: GraphicsOptions) -> Self {
        Self {
            write_list: VecDeque::new(),
            writing: None,
            parser: ansi::Processor::new(),
            graphics: GraphicsIngress::new(graphics_options),
        }
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
