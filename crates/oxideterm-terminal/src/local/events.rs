#[derive(Clone)]
struct LocalEventListener {
    tx: Sender<AlacEvent>,
    protocol_tx: Option<Sender<String>>,
    wakeup_pending: Arc<std::sync::atomic::AtomicBool>,
    activity: crate::activity::TerminalActivitySender,
}

struct LocalEventReceiver {
    rx: Receiver<AlacEvent>,
    wakeup_pending: Arc<std::sync::atomic::AtomicBool>,
    activity: TerminalActivityReceiver,
}

fn local_event_channel() -> (LocalEventListener, LocalEventReceiver) {
    let (tx, rx) = unbounded();
    let wakeup_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (activity_tx, activity_rx) = crate::activity::terminal_activity_channel();
    (
        LocalEventListener {
            tx,
            protocol_tx: None,
            wakeup_pending: wakeup_pending.clone(),
            activity: activity_tx,
        },
        LocalEventReceiver {
            rx,
            wakeup_pending,
            activity: activity_rx,
        },
    )
}

impl LocalEventListener {
    fn activity_sender(&self) -> crate::activity::TerminalActivitySender {
        self.activity.clone()
    }
}

impl LocalEventReceiver {
    fn try_recv(&self) -> std::result::Result<AlacEvent, crossbeam_channel::TryRecvError> {
        let event = self.rx.try_recv()?;
        if matches!(event, AlacEvent::Wakeup) {
            // Clear before handling so concurrent output can enqueue the next redraw.
            self.wakeup_pending
                .store(false, std::sync::atomic::Ordering::Release);
        }
        Ok(event)
    }

    fn is_empty(&self) -> bool {
        self.rx.is_empty()
    }

    fn activity_receiver(&self) -> TerminalActivityReceiver {
        self.activity.clone()
    }
}

impl EventListener for LocalEventListener {
    fn send_event(&self, event: AlacEvent) {
        if let AlacEvent::PtyWrite(text) = &event
            && let Some(tx) = &self.protocol_tx
        {
            // ConPTY waits for cursor reports after resize. Reply on the PTY
            // worker even when the pane is hidden or the UI thread is busy.
            let _ = tx.send(text.clone());
            return;
        }
        let is_wakeup = matches!(event, AlacEvent::Wakeup);
        if is_wakeup
            && self
                .wakeup_pending
                .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            return;
        }

        if self.tx.send(event).is_err() && is_wakeup {
            // A disconnected receiver must not leave cloned listeners permanently muted.
            self.wakeup_pending
                .store(false, std::sync::atomic::Ordering::Release);
        } else {
            // Terminal protocol events are consumed by the pane owner, so wake it after publish.
            self.activity.notify();
        }
    }
}

#[derive(Clone)]
pub enum TerminalEvent {
    Output(Vec<u8>),
    TriggerMatched(oxideterm_terminal_triggers::TriggerMatched),
    PrivilegePrompt(TerminalPrivilegePromptEvent),
    TitleChanged(String),
    TitleReset,
    Bell,
    Wakeup,
    BlinkChanged(bool),
    ChildExited(Option<i32>),
    SessionRestarted,
    // Failed startup must remain visible instead of triggering normal exit auto-close.
    StartupFailed,
    X11ForwardingStatus(oxideterm_ssh::X11ForwardingStatus),
    ProcessingFailed,
    TmuxPaneSelected {
        selected: bool,
    },
    MagicDetected(TerminalMagicKind),
    TrzszTransferPrompt {
        direction: TrzszTransferDirection,
        selection: TrzszTransferSelection,
        remote_is_windows: bool,
    },
    ModemTransferStartFailed,
    ModemTransferPrompt {
        request: ModemTransferRequest,
        transfer: ModemTransfer,
    },
    ShellIntegration(ShellIntegrationEvent),
    EditorIntegration(TerminalEditorIntegrationEvent),
    EditorClipboard(TerminalEditorClipboardEvent),
    CommandMark(TerminalCommandMarkEvent),
    CwdChanged {
        cwd: String,
        host: Option<String>,
    },
    EncodingHint(EncodingHint),
    ClipboardStore(String),
    ClipboardLoad(Arc<dyn Fn(&str) -> String + Sync + Send + 'static>),
}

#[derive(Clone, Copy, Debug)]
struct TerminalSize {
    cols: usize,
    rows: usize,
    cell_width: u16,
    cell_height: u16,
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

fn window_size(size: TerminalSize) -> WindowSize {
    WindowSize {
        num_lines: size.rows as u16,
        num_cols: size.cols as u16,
        cell_width: size.cell_width,
        cell_height: size.cell_height,
    }
}
