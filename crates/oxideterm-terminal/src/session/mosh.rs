// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use alacritty_terminal::term::cell::{Cell as AlacrittyCell, Flags};
use fernomade_predict::{
    PredictionAction, PredictionContext, PredictionDisplay, PredictionOverlay,
    PredictionReconciliation,
};

const MOSH_COMMAND_CHANNEL_CAPACITY: usize = 256;

pub struct MoshTerminalSession {
    title: String,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: LocalEventReceiver,
    worker_rx: crate::backpressure::ByteBoundedReceiver<MoshTerminalWorkerEvent>,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    command_tx: tokio::sync::mpsc::Sender<MoshTerminalCommand>,
    connection_status: MoshConnectionStatus,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
    graphics_alt_screen_active: bool,
    output_queue: VecDeque<crate::backpressure::ByteBoundedItem<MoshTerminalWorkerEvent>>,
    output_processor: Option<TerminalOutputProcessor>,
    output_events_enabled: bool,
    trigger_stream: Option<oxideterm_terminal_triggers::TerminalTriggerStream>,
    shell_integration: TerminalShellIntegration,
    predictor: PredictionOverlay,
    next_prediction_id: u64,
    prediction_started_at: Instant,
}

enum MoshTerminalCommand {
    Data {
        prediction_id: u64,
        bytes: Vec<u8>,
    },
    Resize { columns: u16, rows: u16 },
    Close,
}

impl fmt::Debug for MoshTerminalCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data {
                prediction_id,
                bytes,
            } => formatter
                .debug_struct("Data")
                .field("prediction_id", prediction_id)
                .field("bytes", &bytes.len())
                .finish(),
            Self::Resize { columns, rows } => formatter
                .debug_struct("Resize")
                .field("columns", columns)
                .field("rows", rows)
                .finish(),
            Self::Close => formatter.write_str("Close"),
        }
    }
}

#[derive(Debug)]
enum MoshTerminalWorkerEvent {
    Connected,
    ConnectionState(MoshConnectionStatus),
    Output(Vec<u8>),
    RemoteResize { columns: u16, rows: u16 },
    RoundTripEstimate(u16),
    PredictionAcknowledged(u64),
    Failed(String),
    Closed,
}

impl MoshTerminalSession {
    pub fn new(
        config: MoshTerminalConfig,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        scrollback_lines: usize,
    ) -> Self {
        let resize = TerminalResize::new(cols, rows, 0, 0);
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        let (listener, event_rx) = local_event_channel();
        let (worker_tx, worker_rx) = crate::backpressure::byte_bounded_channel_with_activity(
            crate::backpressure::TRANSPORT_OUTPUT_BACKLOG_BYTES,
            listener.activity_sender(),
        );
        let (command_tx, command_rx) =
            tokio::sync::mpsc::channel(MOSH_COMMAND_CHANNEL_CAPACITY);
        let term = Arc::new(FairMutex::new(Term::new(
            interactive_terminal_config(scrollback_lines),
            &size,
            listener,
        )));
        let title = config.title.clone();
        let prediction_display = match config.prediction {
            MoshPredictionDisplay::Adaptive => PredictionDisplay::Adaptive,
            MoshPredictionDisplay::Always => PredictionDisplay::Always,
            MoshPredictionDisplay::Never => PredictionDisplay::Never,
        };
        config.task_runtime.spawn(run_mosh_terminal_worker(
            config.bootstrap,
            config.bootstrap_context,
            resize,
            command_rx,
            worker_tx,
        ));

        Self {
            title,
            term,
            parser: Processor::new(),
            event_rx,
            worker_rx,
            pending_events: Vec::new(),
            resize,
            lifecycle: TerminalLifecycle::Running,
            command_tx,
            connection_status: MoshConnectionStatus::Connecting,
            graphics_ingress: GraphicsIngress::new(graphics_options),
            graphics: TerminalGraphicsState::default(),
            graphics_alt_screen_active: false,
            output_queue: VecDeque::new(),
            output_processor: None,
            output_events_enabled: false,
            trigger_stream: None,
            shell_integration: TerminalShellIntegration::default(),
            predictor: PredictionOverlay::new(prediction_display, false),
            next_prediction_id: 0,
            prediction_started_at: Instant::now(),
        }
    }

    fn send_command(&self, command: MoshTerminalCommand) -> Result<()> {
        self.command_tx
            .try_send(command)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    fn drain_worker_events_with_budget(&mut self, budget: TerminalDrainBudget) -> TerminalDrainReport {
        let started = Instant::now();
        let mut report = TerminalDrainReport::default();
        if self
            .predictor
            .expire_stale(self.prediction_elapsed_milliseconds())
        {
            self.reconcile_prediction();
            report.mark_changed();
        }
        loop {
            if budget.time_exhausted(started)
                || report.drained_bytes >= budget.max_bytes
                || report.events_drained >= budget.max_events
            {
                report.budget_exhausted =
                    !self.output_queue.is_empty() || !self.worker_rx.is_empty();
                break;
            }
            if let Some(event) = self.output_queue.pop_front() {
                let MoshTerminalWorkerEvent::Output(bytes) = event.into_inner() else {
                    unreachable!("only Mosh output enters the local drain queue");
                };
                report.events_drained += 1;
                let processing_started = budget.collect_performance_metrics.then(Instant::now);
                self.feed_transport_output(&bytes);
                report.record_data_chunk(
                    bytes.len(),
                    processing_started.map_or(Duration::ZERO, |started| started.elapsed()),
                );
                report.mark_changed();
                continue;
            }

            let event = match self.worker_rx.try_recv() {
                Ok(event) => event,
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    if self.lifecycle.is_running() {
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                        report.mark_changed();
                    }
                    break;
                }
            };
            if let MoshTerminalWorkerEvent::Output(bytes) = event.value()
                && report.drained_bytes > 0
                && report.drained_bytes.saturating_add(bytes.len()) > budget.max_bytes
            {
                self.output_queue.push_back(event);
                report.budget_exhausted = true;
                break;
            }

            match event.into_inner() {
                MoshTerminalWorkerEvent::Connected => {
                    self.pending_events
                        .push(TerminalEvent::TitleChanged(self.title.clone()));
                    report.mark_changed();
                }
                MoshTerminalWorkerEvent::ConnectionState(status) => {
                    self.connection_status = status;
                    self.pending_events.push(TerminalEvent::Wakeup);
                    report.mark_changed();
                }
                MoshTerminalWorkerEvent::Output(bytes) => {
                    let processing_started = budget.collect_performance_metrics.then(Instant::now);
                    self.reconcile_prediction();
                    self.feed_transport_output(&bytes);
                    report.record_data_chunk(
                        bytes.len(),
                        processing_started.map_or(Duration::ZERO, |started| started.elapsed()),
                    );
                    report.mark_changed();
                }
                MoshTerminalWorkerEvent::RemoteResize { columns, rows } => {
                    self.reconcile_prediction();
                    self.apply_remote_resize(columns, rows);
                    report.mark_changed();
                }
                MoshTerminalWorkerEvent::RoundTripEstimate(milliseconds) => {
                    self.predictor
                        .set_round_trip_milliseconds(Some(milliseconds));
                }
                MoshTerminalWorkerEvent::PredictionAcknowledged(frame_id) => {
                    self.predictor.acknowledge(frame_id);
                }
                MoshTerminalWorkerEvent::Failed(error) => {
                    self.reconcile_prediction();
                    self.lifecycle = TerminalLifecycle::Exited(None);
                    self.feed_transport_output(
                        format!("\r\nMosh connection failed: {error}\r\n").as_bytes(),
                    );
                    self.pending_events.push(TerminalEvent::ChildExited(None));
                    report.mark_changed();
                    break;
                }
                MoshTerminalWorkerEvent::Closed => {
                    self.reconcile_prediction();
                    if self.lifecycle.is_running() {
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                        report.mark_changed();
                    }
                    break;
                }
            }
            report.events_drained += 1;
        }
        report.pending_bytes = self.worker_rx.pending_bytes();
        report.drain_duration = started.elapsed();
        report
    }

    fn apply_remote_resize(&mut self, columns: u16, rows: u16) {
        if columns < 2 || rows < 2 {
            return;
        }
        self.resize.cols = usize::from(columns);
        self.resize.rows = usize::from(rows);
        self.term.lock().resize(TerminalSize {
            cols: self.resize.cols,
            rows: self.resize.rows,
            cell_width: self.resize.cell_width,
            cell_height: self.resize.cell_height,
        });
    }

    fn feed_transport_output(&mut self, bytes: &[u8]) {
        let processed = apply_terminal_output_processor(&self.output_processor, bytes);
        let bytes = processed.as_ref();
        let mut term = self.term.lock();
        let size = TerminalSize {
            cols: self.resize.cols,
            rows: self.resize.rows,
            cell_width: self.resize.cell_width,
            cell_height: self.resize.cell_height,
        };
        let cursor = Cell::new(graphics_cursor_from_term(&term, size));
        let mut protocol_responses = Vec::new();
        self.graphics_ingress.advance_ordered(
            bytes,
            |segment| match segment {
                TerminalGraphicsSegment::Terminal(terminal_bytes) => {
                    if let Some(stream) = self.trigger_stream.as_mut() {
                        stream.observe_bytes(&terminal_bytes, |matched| {
                            self.pending_events
                                .push(TerminalEvent::TriggerMatched(matched));
                        });
                    }
                    if self.output_events_enabled {
                        let (_, recordable) = self.shell_integration.advance_with_recording(
                            &mut self.parser,
                            &mut *term,
                            &terminal_bytes,
                            |event| self.pending_events.push(event),
                        );
                        if !recordable.is_empty() {
                            self.pending_events.push(TerminalEvent::Output(recordable));
                        }
                    } else {
                        self.shell_integration.advance(
                            &mut self.parser,
                            &mut *term,
                            &terminal_bytes,
                            |event| self.pending_events.push(event),
                        );
                    }
                    self.graphics
                        .clear_for_alt_screen_transition(&term, &mut self.graphics_alt_screen_active);
                    cursor.set(graphics_cursor_from_term(&term, size));
                }
                TerminalGraphicsSegment::Event(event) => {
                    if let Some(response) = self.graphics.handle_event(event) {
                        protocol_responses.push(response);
                    }
                }
            },
            || cursor.get(),
        );
        drop(term);
        for response in protocol_responses {
            let _ = self.write_protocol_bytes(&response);
        }
    }

    fn prediction_elapsed_milliseconds(&self) -> u64 {
        u64::try_from(self.prediction_started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn prediction_context(&self) -> PredictionContext {
        let term = self.term.lock();
        let cursor = &term.grid().cursor;
        let row = u16::try_from(cursor.point.line.0.max(0)).unwrap_or(u16::MAX);
        let column = u16::try_from(cursor.point.column.0).unwrap_or(u16::MAX);
        let columns = u16::try_from(self.resize.cols).unwrap_or(u16::MAX);
        let attributes = prediction_attributes(&cursor.template);
        let mut cursor_state = format!(
            "\u{1b}[{};{}H",
            row.saturating_add(1),
            column.saturating_add(1)
        )
        .into_bytes();
        cursor_state.extend_from_slice(&attributes);
        PredictionContext {
            row,
            column,
            columns,
            attributes,
            cursor_state,
        }
    }

    fn feed_prediction_overlay(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut *self.term.lock(), bytes);
        self.pending_events.push(TerminalEvent::Wakeup);
    }

    fn reconcile_prediction(&mut self) {
        match self.predictor.take_reconciliation() {
            PredictionReconciliation::None => {}
            PredictionReconciliation::Local(bytes) => self.feed_prediction_overlay(&bytes),
            PredictionReconciliation::Redraw => {
                // Every displayed OxideTerm prediction is anchored with a cursor context.
                debug_assert!(false, "contextual Mosh prediction unexpectedly required redraw");
            }
        }
    }

    fn queue_user_input(&mut self, bytes: &[u8], action: PredictionAction) -> Result<()> {
        if !self.lifecycle.is_running() || bytes.is_empty() {
            return Ok(());
        }
        let prediction_id = self.next_prediction_id;
        self.send_command(MoshTerminalCommand::Data {
            prediction_id,
            bytes: bytes.to_vec(),
        })?;
        self.next_prediction_id = self.next_prediction_id.saturating_add(1);
        let context = self.prediction_context();
        if let Some(overlay) = self.predictor.offer_for_frame_with_context_at(
            prediction_id,
            action,
            Some(&context),
            self.prediction_elapsed_milliseconds(),
        ) {
            self.feed_prediction_overlay(&overlay);
        }
        Ok(())
    }

    fn handle_alacritty_event(&mut self, event: AlacEvent) -> bool {
        match event {
            AlacEvent::Title(title) => {
                self.title = title.clone();
                self.pending_events.push(TerminalEvent::TitleChanged(title));
                false
            }
            AlacEvent::ResetTitle => {
                self.pending_events
                    .push(TerminalEvent::TitleChanged(self.title.clone()));
                false
            }
            AlacEvent::Bell => {
                self.pending_events.push(TerminalEvent::Bell);
                false
            }
            AlacEvent::Wakeup | AlacEvent::MouseCursorDirty => {
                self.pending_events.push(TerminalEvent::Wakeup);
                true
            }
            AlacEvent::CursorBlinkingChange => {
                let blinking = self.term.lock().cursor_style().blinking;
                self.pending_events
                    .push(TerminalEvent::BlinkChanged(blinking));
                true
            }
            AlacEvent::PtyWrite(text) => {
                let _ = self.write_protocol_bytes(text.as_bytes());
                false
            }
            AlacEvent::ClipboardStore(_, text) => {
                self.pending_events.push(TerminalEvent::ClipboardStore(text));
                false
            }
            AlacEvent::ClipboardLoad(_, formatter) => {
                self.pending_events.push(TerminalEvent::ClipboardLoad(formatter));
                false
            }
            AlacEvent::ColorRequest(_, _)
            | AlacEvent::TextAreaSizeRequest(_)
            | AlacEvent::ChildExit(_)
            | AlacEvent::Exit => false,
        }
    }
}

impl TerminalSessionBackend for MoshTerminalSession {
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::Mosh
    }

    fn title(&self) -> Option<String> {
        Some(self.title.clone())
    }

    fn lifecycle(&self) -> TerminalLifecycle {
        self.lifecycle.clone()
    }

    fn process_info(&self) -> TerminalProcessInfo {
        TerminalProcessInfo::default()
    }

    fn refresh_process_info(&mut self) {}

    fn read_pending(&mut self) -> bool {
        self.read_pending_with_budget(TerminalDrainBudget::unlimited())
            .changed
    }

    fn read_pending_with_budget(&mut self, budget: TerminalDrainBudget) -> TerminalDrainReport {
        let started = Instant::now();
        let mut report = self.drain_worker_events_with_budget(budget);
        while report.events_drained < budget.max_events && !budget.time_exhausted(started) {
            let Ok(event) = self.event_rx.try_recv() else {
                break;
            };
            report.events_drained += 1;
            if self.handle_alacritty_event(event) {
                report.mark_changed();
            }
        }
        report.drain_duration = started.elapsed();
        report
    }

    fn activity_receiver(&self) -> TerminalActivityReceiver {
        self.event_rx.activity_receiver()
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.pending_events)
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        self.queue_user_input(bytes, prediction_action_from_input(bytes))
    }

    fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.queue_user_input(bytes, PredictionAction::Barrier)
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        self.queue_user_input(text.as_bytes(), prediction_action_from_input(text.as_bytes()))
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        let bytes = if self.mode().contains(TermMode::BRACKETED_PASTE) {
            [b"\x1b[200~".as_slice(), text.as_bytes(), b"\x1b[201~".as_slice()].concat()
        } else {
            text.as_bytes().to_vec()
        };
        self.queue_user_input(&bytes, PredictionAction::Barrier)
    }

    fn set_encoding(&mut self, _encoding: TerminalEncoding) {
        // Mosh terminal state is defined as UTF-8 and cannot switch encodings.
    }

    fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        self.output_processor = processor;
    }

    fn set_output_events_enabled(&mut self, enabled: bool) {
        self.output_events_enabled = enabled;
    }

    fn set_trigger_rules(
        &mut self,
        rules: Option<Arc<oxideterm_terminal_triggers::CompiledTriggerSet>>,
    ) {
        self.trigger_stream = rules.map(oxideterm_terminal_triggers::TerminalTriggerStream::new);
    }

    fn mosh_connection_status(&self) -> Option<MoshConnectionStatus> {
        Some(self.connection_status)
    }

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn set_focused(&mut self, focused: bool) -> Result<()> {
        let should_report = {
            let mut term = self.term.lock();
            term.is_focused = focused;
            term.mode().contains(TermMode::FOCUS_IN_OUT)
        };
        if let Some(report) = focus_report_sequence(should_report, focused) {
            self.write_protocol_bytes(report)?;
        }
        Ok(())
    }

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        self.reconcile_prediction();
        let grid_changed = self.resize.cols != resize.cols || self.resize.rows != resize.rows;
        if grid_changed {
            self.shell_integration
                .reset_command_marks_for_grid_reflow(|event| self.pending_events.push(event));
        }
        self.resize = resize;
        self.term.lock().resize(TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        });
        let _ = self.send_command(MoshTerminalCommand::Resize {
            columns: u16::try_from(resize.cols).unwrap_or(u16::MAX),
            rows: u16::try_from(resize.rows).unwrap_or(u16::MAX),
        });
        Ok(())
    }

    fn scroll_lines(&mut self, delta: i32) {
        if delta != 0 {
            self.term.lock().scroll_display(Scroll::Delta(delta));
        }
    }

    fn scroll_lines_snapshot_incremental(
        &mut self,
        delta: i32,
        previous: &TerminalSnapshot,
    ) -> TerminalSnapshot {
        let mut term = self.term.lock();
        scroll_snapshot_from_term(
            &mut term,
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
            delta,
            previous,
        )
    }

    fn page_up(&mut self) { self.term.lock().scroll_display(Scroll::PageUp); }
    fn page_down(&mut self) { self.term.lock().scroll_display(Scroll::PageDown); }
    fn scroll_to_top(&mut self) { self.term.lock().scroll_display(Scroll::Top); }
    fn scroll_to_bottom(&mut self) { self.term.lock().scroll_display(Scroll::Bottom); }

    fn scroll_to_display_offset(&mut self, offset: usize) {
        let mut term = self.term.lock();
        let maximum = term.total_lines().saturating_sub(term.screen_lines());
        let current = term.grid().display_offset();
        let delta = offset.min(maximum) as i32 - current as i32;
        if delta != 0 {
            term.scroll_display(Scroll::Delta(delta));
        }
    }

    fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
        search_matches_from_term(&self.term.lock(), self.resize.cols, query)
    }

    fn search_source(&self) -> Option<crate::TerminalSearchSource> {
        Some(crate::TerminalSearchSource::new(self.term.clone(), self.resize.cols))
    }

    fn clear_buffer(&mut self) {
        clear_terminal_buffer(&mut self.term.lock());
        self.graphics.clear();
    }

    fn buffer_text(&self) -> String {
        terminal_buffer_text_from_term(&self.term.lock(), self.resize.cols)
    }

    fn command_output_text(&self, mark: &TerminalCommandMark) -> String {
        command_output_text_from_term(&self.term.lock(), mark)
    }

    fn snapshot(&self) -> TerminalSnapshot {
        snapshot_from_term(
            &self.term.lock(),
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
        )
    }

    fn snapshot_incremental(&self, previous: &TerminalSnapshot) -> TerminalSnapshot {
        incremental_snapshot_from_term(
            &mut self.term.lock(),
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
            previous,
        )
    }

    fn snapshot_with_display_offset(&self, display_offset: usize, rows: usize) -> TerminalSnapshot {
        snapshot_from_term_with_display_offset(
            &self.term.lock(),
            TerminalSize {
                cols: self.resize.cols,
                rows: self.resize.rows,
                cell_width: self.resize.cell_width,
                cell_height: self.resize.cell_height,
            },
            &self.graphics,
            display_offset,
            rows,
        )
    }

    fn terminate_active_task(&mut self) -> Result<()> { self.write_protocol_bytes(b"\x03") }
    fn kill_active_task(&mut self) -> Result<()> { self.write_protocol_bytes(b"\x03") }

    fn shutdown(&mut self) {
        if matches!(self.lifecycle, TerminalLifecycle::Closed) {
            return;
        }
        let _ = self.send_command(MoshTerminalCommand::Close);
        self.lifecycle = TerminalLifecycle::Closed;
    }
}

async fn run_mosh_terminal_worker(
    mut bootstrap: oxideterm_mosh::MoshBootstrapConfig,
    bootstrap_context: oxideterm_mosh::MoshBootstrapContext,
    initial_resize: TerminalResize,
    mut command_rx: tokio::sync::mpsc::Receiver<MoshTerminalCommand>,
    worker_tx: crate::backpressure::ByteBoundedSender<MoshTerminalWorkerEvent>,
) {
    bootstrap.terminal_columns = u16::try_from(initial_resize.cols).unwrap_or(u16::MAX);
    bootstrap.terminal_rows = u16::try_from(initial_resize.rows).unwrap_or(u16::MAX);
    let bootstrap = match oxideterm_mosh::bootstrap_mosh(bootstrap, bootstrap_context).await {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            let _ = worker_tx.send_control(MoshTerminalWorkerEvent::Failed(error.to_string()));
            return;
        }
    };
    let session_config = oxideterm_mosh::MoshSessionConfig {
        remote_host: bootstrap.remote_host,
        remote_port: bootstrap.remote_port,
        ip_family: bootstrap.ip_family,
        columns: u16::try_from(initial_resize.cols).unwrap_or(u16::MAX),
        rows: u16::try_from(initial_resize.rows).unwrap_or(u16::MAX),
        key: bootstrap.key,
    };
    let (mut client, owner) = match oxideterm_mosh::start_mosh_session(session_config).await {
        Ok(session) => session,
        Err(error) => {
            let _ = worker_tx.send_control(MoshTerminalWorkerEvent::Failed(error.to_string()));
            return;
        }
    };
    let _ = worker_tx.send_control(MoshTerminalWorkerEvent::Connected);
    let mut owner = Some(owner);

    loop {
        tokio::select! {
            biased;
            command = command_rx.recv() => {
                match command {
                    Some(MoshTerminalCommand::Data { prediction_id, bytes }) => {
                        if client.send_input_for_prediction(prediction_id, bytes).await.is_err() {
                            let _ = worker_tx.send_control(MoshTerminalWorkerEvent::Closed);
                            return;
                        }
                    }
                    Some(MoshTerminalCommand::Resize { columns, rows }) => {
                        let _ = client.resize(columns, rows).await;
                    }
                    Some(MoshTerminalCommand::Close) => {
                        if let Some(owner) = owner.take() {
                            owner.shutdown().await;
                        }
                        let _ = worker_tx.send_control(MoshTerminalWorkerEvent::Closed);
                        return;
                    }
                    None => return,
                }
            }
            event = client.next_event() => {
                let Some(event) = event else {
                    let _ = worker_tx.send_control(MoshTerminalWorkerEvent::Closed);
                    return;
                };
                match event {
                    oxideterm_mosh::MoshSessionEvent::Output(bytes) => {
                        let byte_len = bytes.len();
                        if worker_tx.send_async(MoshTerminalWorkerEvent::Output(bytes), byte_len).await.is_err() {
                            return;
                        }
                    }
                    oxideterm_mosh::MoshSessionEvent::RemoteResize { columns, rows } => {
                        let _ = worker_tx.send_control(MoshTerminalWorkerEvent::RemoteResize { columns, rows });
                    }
                    oxideterm_mosh::MoshSessionEvent::ConnectionStateChanged(state) => {
                        let status = match state {
                            oxideterm_mosh::MoshConnectionState::Connecting => MoshConnectionStatus::Connecting,
                            oxideterm_mosh::MoshConnectionState::Connected => MoshConnectionStatus::Connected,
                            oxideterm_mosh::MoshConnectionState::Interrupted => MoshConnectionStatus::Interrupted,
                        };
                        let _ = worker_tx.send_control(MoshTerminalWorkerEvent::ConnectionState(status));
                    }
                    oxideterm_mosh::MoshSessionEvent::Closed(_) => {
                        owner.take();
                        let _ = worker_tx.send_control(MoshTerminalWorkerEvent::Closed);
                        return;
                    }
                    oxideterm_mosh::MoshSessionEvent::Failed(error) => {
                        let _ = worker_tx.send_control(MoshTerminalWorkerEvent::Failed(error));
                        return;
                    }
                    oxideterm_mosh::MoshSessionEvent::RoundTripEstimate(milliseconds) => {
                        let _ = worker_tx.send_control(MoshTerminalWorkerEvent::RoundTripEstimate(milliseconds));
                    }
                    oxideterm_mosh::MoshSessionEvent::PredictionAcknowledged(frame_id) => {
                        let _ = worker_tx.send_control(MoshTerminalWorkerEvent::PredictionAcknowledged(frame_id));
                    }
                    oxideterm_mosh::MoshSessionEvent::RemoteStateAdvanced(_) => {}
                }
            }
        }
    }
}

fn prediction_action_from_input(bytes: &[u8]) -> PredictionAction {
    match bytes {
        [0x08] | [0x7f] => PredictionAction::Backspace,
        b"\x1b[D" | b"\x1bOD" => PredictionAction::Left,
        b"\x1b[C" | b"\x1bOC" => PredictionAction::Right,
        _ => {
            let Ok(text) = std::str::from_utf8(bytes) else {
                return PredictionAction::Barrier;
            };
            let mut characters = text.chars();
            let Some(character) = characters.next() else {
                return PredictionAction::Barrier;
            };
            if characters.next().is_some() || character.is_control() {
                PredictionAction::Barrier
            } else if character.is_ascii() {
                PredictionAction::PrintableAscii(character as u8)
            } else {
                PredictionAction::PrintableUtf8(character)
            }
        }
    }
}

fn prediction_attributes(cell: &AlacrittyCell) -> Vec<u8> {
    let mut codes = vec!["0".to_string()];
    if cell.flags.contains(Flags::BOLD) {
        codes.push("1".to_string());
    }
    if cell.flags.contains(Flags::DIM) {
        codes.push("2".to_string());
    }
    if cell.flags.contains(Flags::ITALIC) {
        codes.push("3".to_string());
    }
    if cell.flags.intersects(Flags::ALL_UNDERLINES) {
        codes.push("4".to_string());
    }
    if cell.flags.contains(Flags::INVERSE) {
        codes.push("7".to_string());
    }
    if cell.flags.contains(Flags::HIDDEN) {
        codes.push("8".to_string());
    }
    if cell.flags.contains(Flags::STRIKEOUT) {
        codes.push("9".to_string());
    }
    let foreground = crate::color::color_to_rgb(cell.fg);
    let background = crate::color::color_to_rgb(cell.bg);
    codes.push(format!(
        "38;2;{};{};{}",
        foreground.r, foreground.g, foreground.b
    ));
    codes.push(format!(
        "48;2;{};{};{}",
        background.r, background.g, background.b
    ));
    format!("\u{1b}[{}m", codes.join(";")).into_bytes()
}
