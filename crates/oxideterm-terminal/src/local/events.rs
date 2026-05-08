#[derive(Clone)]
struct LocalEventListener {
    tx: Sender<AlacEvent>,
}

impl EventListener for LocalEventListener {
    fn send_event(&self, event: AlacEvent) {
        let _ = self.tx.send(event);
    }
}

#[derive(Clone)]
pub enum TerminalEvent {
    TitleChanged(String),
    TitleReset,
    Bell,
    Wakeup,
    BlinkChanged(bool),
    ChildExited(Option<i32>),
    MagicDetected(TerminalMagicKind),
    TrzszTransferPrompt {
        direction: TrzszTransferDirection,
        selection: TrzszTransferSelection,
        remote_is_windows: bool,
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
