const BLE_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// Discovery window for saved-profile opens, which start from a cold adapter.
const BLE_SAVED_PROFILE_SCAN_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);
/// A dropped link does not always end the notification stream (the stream can
/// stay open on platforms whose cache still reports the peer), so the worker
/// polls the link itself and closes the session once the peer is gone.
const BLE_LINK_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
const BLE_ATT_HEADER_BYTES: usize = 3;

/// Configuration for a BLE GATT terminal session backed by btleplug.
///
/// The peripheral identity comes from the discovery flow; the service UUID
/// identifies which GATT service carries the notify/write channel used as
/// the terminal byte stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BleSessionConfig {
    /// Adapter to reopen when the session runs without a handed-over
    /// adapter guard. When a guard is supplied the worker uses the guard's
    /// adapter and never reads this value, so resolvers that keep a
    /// supplied guard record a placeholder 0 here.
    pub adapter_index: usize,
    pub device_id: oxideterm_ble::BlePeripheralId,
    pub service_uuid: uuid::Uuid,
    pub device_label: String,
    /// BLE carries the same raw byte stream as a serial console, so both
    /// transports share their send/display options.
    pub runtime_options: SerialRuntimeOptions,
    /// Preferred ATT MTU requested right after the link comes up; `None`
    /// keeps the platform-negotiated default.
    pub mtu: Option<u16>,
    /// When true (default) a `write-without-response` characteristic is
    /// preferred for lower latency. When false the acknowledged WRITE
    /// characteristic wins; devices without one still fall back to
    /// write-without-response because the OS rejects acknowledged writes on
    /// characteristics lacking the WRITE property.
    pub write_without_response: bool,
}

impl BleSessionConfig {
    pub fn endpoint_label(&self) -> String {
        self.device_label.clone()
    }
}

pub struct BleSession {
    audit: Option<oxideterm_audit::AuditContext>,
    recording_sink: Option<oxideterm_audit::RecordingSink>,
    recording_pending: Option<crate::recording_output::RecordingOutput>,
    connect_audit: Option<oxideterm_audit::AuditOperation>,
    connected: bool,
    config: BleSessionConfig,
    term: Arc<FairMutex<Term<LocalEventListener>>>,
    parser: Processor,
    event_rx: LocalEventReceiver,
    worker_rx: crate::backpressure::ByteBoundedReceiver<BleWorkerEvent>,
    pending_events: Vec<TerminalEvent>,
    resize: TerminalResize,
    lifecycle: TerminalLifecycle,
    runtime: Option<Runtime>,
    worker: Option<tokio::task::JoinHandle<()>>,
    command_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    /// Closes the worker outside the bounded data channel: a stalled device
    /// write can fill that channel, and a close silently dropped there would
    /// leak the notify registration and the physical link.
    close_token: tokio_util::sync::CancellationToken,
    title: Option<String>,
    graphics_ingress: GraphicsIngress,
    graphics: TerminalGraphicsState,
    palette: TerminalPalette,
    graphics_alt_screen_active: bool,
    output_queue: VecDeque<crate::backpressure::ByteBoundedItem<BleWorkerEvent>>,
    magic_scan: MagicScanWindow,
    encoding: TerminalEncoding,
    output_decoder: TerminalOutputDecoder,
    output_processor: Option<TerminalOutputProcessor>,
    output_events_enabled: bool,
    trigger_stream: Option<oxideterm_terminal_triggers::TerminalTriggerStream>,
    input_encoder: TerminalInputEncoder,
    encoding_detector: EncodingMismatchDetector,
    modem_consumer: ModemConsumer,
    shell_integration: TerminalShellIntegration,
    serial_console_ingress: SerialConsoleIngress,
    line_ending_ingress: LineEndingIngress,
    runtime_options: SerialRuntimeOptions,
    hexdump_offset: u64,
    /// ATT MTU currently negotiated with the peripheral; updated from worker events.
    negotiated_mtu: Option<u16>,
    /// Frozen at session creation; worker-failure sentences rendered inside
    /// this terminal keep the locale of the moment it was opened.
    i18n: oxideterm_i18n::I18n,
}

#[derive(Debug)]
enum BleWorkerEvent {
    Connected,
    /// ATT MTU negotiated by the OS/device; may arrive late on platforms that
    /// update the value after `subscribe` returns (macOS CoreBluetooth).
    NegotiatedMtu(u16),
    Output(Vec<u8>),
    Failed(String),
    Closed,
}

impl BleSession {
    pub fn new(
        config: BleSessionConfig,
        adapter_guard: Option<oxideterm_ble::BleAdapterGuard>,
        audit: Option<oxideterm_audit::AuditContext>,
        cols: usize,
        rows: usize,
        graphics_options: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback_lines: usize,
        i18n: oxideterm_i18n::I18n,
    ) -> Self {
        let resize = TerminalResize::new(cols, rows, 0, 0);
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        let (listener, event_rx) = local_event_channel();
        let recording_activity = listener.activity_sender();
        let (worker_tx, worker_rx) = crate::backpressure::byte_bounded_channel_with_activity(
            crate::backpressure::TRANSPORT_OUTPUT_BACKLOG_BYTES,
            listener.activity_sender(),
        );
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(256);
        let close_token = tokio_util::sync::CancellationToken::new();
        let term_config = interactive_terminal_config(scrollback_lines);
        let term = Arc::new(FairMutex::new(Term::new(term_config, &size, listener)));
        let connect_audit = Some(oxideterm_audit::AuditOperation::in_context(
            audit.as_ref(),
            oxideterm_audit::AuditCategory::Connection,
            "ble_connect",
            None,
        ));
        let recording_sink = audit
            .as_ref()
            .map(oxideterm_audit::AuditContext::recording_sink);
        if let Some(sink) = &recording_sink {
            sink.set_wake_callback(Arc::new(move || recording_activity.notify()));
        }
        let recording_pending = recording_sink.as_ref().map(|sink| {
            let mut pending = crate::recording_output::RecordingOutput::new(sink.clone());
            pending.resize(resize.cols as u16, resize.rows as u16);
            pending
        });

        let runtime = Runtime::new().ok();
        let mut worker = None;
        if let Some(runtime) = runtime.as_ref() {
            let worker_config = config.clone();
            worker = Some(runtime.spawn(run_ble_worker(
                worker_config,
                adapter_guard,
                command_rx,
                worker_tx,
                close_token.clone(),
                i18n.clone(),
            )));
        } else {
            let _ = worker_tx.send_control(BleWorkerEvent::Failed(
                i18n.t("terminal.ble.runtime_init_failed"),
            ));
        }

        let runtime_options = config.runtime_options;
        Self {
            audit,
            recording_sink,
            recording_pending,
            connect_audit,
            connected: false,
            config,
            term,
            parser: Processor::new(),
            event_rx,
            worker_rx,
            pending_events: Vec::new(),
            resize,
            lifecycle: TerminalLifecycle::Running,
            runtime,
            worker,
            command_tx,
            close_token,
            title: None,
            graphics_ingress: GraphicsIngress::new(graphics_options),
            graphics: TerminalGraphicsState::default(),
            palette: TerminalPalette::default(),
            graphics_alt_screen_active: false,
            output_queue: VecDeque::new(),
            magic_scan: MagicScanWindow::default(),
            encoding,
            output_decoder: TerminalOutputDecoder::new(encoding),
            output_processor: None,
            output_events_enabled: false,
            trigger_stream: None,
            input_encoder: TerminalInputEncoder::new(encoding),
            encoding_detector: EncodingMismatchDetector::new(encoding),
            modem_consumer: ModemConsumer::new(),
            shell_integration: TerminalShellIntegration::default(),
            serial_console_ingress: SerialConsoleIngress::default(),
            line_ending_ingress: LineEndingIngress::default(),
            runtime_options,
            hexdump_offset: 0,
            negotiated_mtu: None,
            i18n,
        }
    }

    fn title_text(&self) -> String {
        format!("BLE {}", self.config.endpoint_label())
    }

    fn audit_exit(&mut self, outcome: oxideterm_audit::AuditOutcome) {
        if let Some(operation) = self.connect_audit.take() {
            operation.finish(
                outcome,
                oxideterm_audit::AuditEvidence::Lifecycle,
                None,
                None,
            );
        } else if self.connected {
            if let Some(context) = &self.audit {
                context.observe(
                    oxideterm_audit::AuditCategory::Connection,
                    "ble_disconnect",
                    None,
                    outcome,
                    oxideterm_audit::AuditEvidence::Lifecycle,
                    oxideterm_audit::AuditAuthorization::NotRequired,
                );
            }
            self.connected = false;
        }
    }

    fn drain_worker_events_with_budget(
        &mut self,
        budget: TerminalDrainBudget,
    ) -> TerminalDrainReport {
        let started = Instant::now();
        let mut report = TerminalDrainReport::default();
        loop {
            if self
                .recording_pending
                .as_mut()
                .is_some_and(|pending| !pending.flush())
            {
                break;
            }

            if budget.time_exhausted(started)
                || report.drained_bytes >= budget.max_bytes
                || report.events_drained >= budget.max_events
            {
                report.budget_exhausted =
                    !self.output_queue.is_empty() || !self.worker_rx.is_empty();
                break;
            }

            let event = match self
                .output_queue
                .pop_front()
                .map(Ok)
                .unwrap_or_else(|| self.worker_rx.try_recv())
            {
                Ok(event) => event,
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    if self.lifecycle.is_running() {
                        if let Some(sink) = &self.recording_sink {
                            sink.interrupt();
                        }
                        self.audit_exit(oxideterm_audit::AuditOutcome::Interrupted);
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                        report.mark_changed();
                    }
                    break;
                }
            };
            if let BleWorkerEvent::Output(bytes) = event.value()
                && report.drained_bytes > 0
                && report.drained_bytes.saturating_add(bytes.len()) > budget.max_bytes
            {
                self.output_queue.push_back(event);
                report.budget_exhausted = true;
                break;
            }

            match event.into_inner() {
                BleWorkerEvent::Connected => {
                    self.connected = true;
                    if let Some(operation) = self.connect_audit.take() {
                        operation.finish(
                            oxideterm_audit::AuditOutcome::Succeeded,
                            oxideterm_audit::AuditEvidence::Protocol,
                            None,
                            None,
                        );
                    }
                    self.title = Some(self.title_text());
                    self.pending_events
                        .push(TerminalEvent::TitleChanged(self.title_text()));
                    report.events_drained += 1;
                    report.mark_changed();
                }
                BleWorkerEvent::NegotiatedMtu(mtu) => {
                    if self.negotiated_mtu != Some(mtu) {
                        self.negotiated_mtu = Some(mtu);
                        report.events_drained += 1;
                        report.mark_changed();
                    }
                }
                BleWorkerEvent::Output(bytes) => {
                    report.events_drained += 1;
                    let processing_started = budget.collect_performance_metrics.then(Instant::now);
                    self.feed_transport_output(&bytes);
                    report.record_data_chunk(
                        bytes.len(),
                        processing_started.map_or(Duration::ZERO, |started| started.elapsed()),
                    );
                    report.mark_changed();
                }
                BleWorkerEvent::Failed(error) => {
                    if let Some(sink) = &self.recording_sink {
                        sink.interrupt();
                    }
                    self.audit_exit(oxideterm_audit::AuditOutcome::Failed);
                    self.lifecycle = TerminalLifecycle::Exited(None);
                    self.feed_utf8_terminal_output(
                        format!(
                            "\r\n{}\r\n",
                            self.i18n
                                .t_with("terminal.ble.connection_failed", &[("error", &error)])
                        )
                        .as_bytes(),
                    );
                    self.pending_events.push(TerminalEvent::ChildExited(None));
                    report.events_drained += 1;
                    report.mark_changed();
                    break;
                }
                BleWorkerEvent::Closed => {
                    if self.lifecycle.is_running() {
                        if let Some(sink) = &self.recording_sink {
                            sink.interrupt();
                        }
                        self.audit_exit(oxideterm_audit::AuditOutcome::Interrupted);
                        self.lifecycle = TerminalLifecycle::Exited(None);
                        self.pending_events.push(TerminalEvent::ChildExited(None));
                        report.mark_changed();
                    }
                    report.events_drained += 1;
                    break;
                }
            }
        }
        report.pending_bytes = self.worker_rx.pending_bytes();
        report.drain_duration = started.elapsed();
        report
    }

    raw_stream_inherent_methods!();
    fn feed_display_output(&mut self, bytes: &[u8], remote_output: bool) {
        // Preserve protocol bytes before optional plugin display transforms.
        let processed_output = self.process_terminal_output(bytes);
        let bytes = processed_output.as_ref();
        let terminal_bytes = self.prepare_display_output(bytes);
        if terminal_bytes.is_empty() {
            return;
        }
        for kind in self.magic_scan.scan(&terminal_bytes) {
            self.pending_events.push(TerminalEvent::MagicDetected(kind));
        }
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
            &terminal_bytes,
            |segment| match segment {
                TerminalGraphicsSegment::Terminal(terminal_bytes) => {
                    if let Some(hint) = self.encoding_detector.observe(&terminal_bytes) {
                        self.pending_events.push(TerminalEvent::EncodingHint(hint));
                    }
                    let decoded = self.output_decoder.decode_to_utf8_bytes(&terminal_bytes);
                    if let Some(stream) = self.trigger_stream.as_mut() {
                        stream.observe_bytes(decoded.as_ref(), |matched| {
                            self.pending_events
                                .push(TerminalEvent::TriggerMatched(matched));
                        });
                    }
                    let record_output = remote_output
                        && (self.output_events_enabled
                            || self
                                .recording_sink
                                .as_ref()
                                .is_some_and(oxideterm_audit::RecordingSink::is_enabled));
                    if record_output {
                        let (_, recordable) = self.shell_integration.advance_with_recording(
                            &mut self.parser,
                            &mut *term,
                            decoded.as_ref(),
                            |event| self.pending_events.push(event),
                        );
                        if !recordable.is_empty() {
                            if let Some(pending) = &mut self.recording_pending {
                                pending.output(&recordable);
                            }
                            if self.output_events_enabled {
                                self.pending_events.push(TerminalEvent::Output(recordable));
                            }
                        }
                    } else {
                        self.shell_integration.advance(
                            &mut self.parser,
                            &mut *term,
                            decoded.as_ref(),
                            |event| self.pending_events.push(event),
                        );
                    }
                    self.graphics.clear_for_alt_screen_transition(
                        &term,
                        &mut self.graphics_alt_screen_active,
                    );
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

    fn process_terminal_output<'a>(&self, bytes: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        apply_terminal_output_processor(&self.output_processor, bytes)
    }

    fn send_data(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.command_tx
            .try_send(bytes)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

impl Drop for BleSession {
    fn drop(&mut self) {
        // Dropping this backend cancels its private runtime and any pending recording.
        self.shutdown();
    }
}

impl TerminalSessionBackend for BleSession {
    raw_stream_backend_methods!();
    fn kind(&self) -> TerminalSessionKind {
        TerminalSessionKind::Ble
    }

    fn ble_connection_status(&self) -> Option<BleConnectionStatus> {
        // The worker's lifecycle owns link loss: a failed or closed worker
        // releases the subscription and the physical link, so the session can
        // never return to a live state. `connected` only separates the
        // pre-connect phase from a subscribed stream.
        Some(if !self.lifecycle.is_running() {
            BleConnectionStatus::Disconnected
        } else if self.connected {
            BleConnectionStatus::Connected
        } else {
            BleConnectionStatus::Connecting
        })
    }

    fn ble_negotiated_mtu(&self) -> Option<u16> {
        self.negotiated_mtu
    }

    fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if self.lifecycle.is_running() && !bytes.is_empty() {
            self.send_data(bytes.to_vec())?;
        }
        Ok(())
    }

    fn set_serial_runtime_options(&mut self, options: SerialRuntimeOptions) -> Result<()> {
        if self.runtime_options.display_mode != options.display_mode
            || self.runtime_options.output_line_ending != options.output_line_ending
        {
            // Presentation changes apply only to future bytes, so no partial stream state may leak.
            self.hexdump_offset = 0;
            self.line_ending_ingress.reset();
            self.serial_console_ingress = SerialConsoleIngress::default();
        }
        self.runtime_options = options;
        Ok(())
    }

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        let grid_changed = self.resize.cols != resize.cols || self.resize.rows != resize.rows;
        self.resize = resize;
        let size = TerminalSize {
            cols: resize.cols,
            rows: resize.rows,
            cell_width: resize.cell_width,
            cell_height: resize.cell_height,
        };
        {
            let mut term = self.term.lock();
            term.resize(size);
            self.shell_integration
                .reflow_command_marks(&term, |event| self.pending_events.push(event));
        }
        if grid_changed {
            if let Some(pending) = &mut self.recording_pending {
                pending.resize(resize.cols as u16, resize.rows as u16);
            }
        }
        Ok(())
    }

    fn shutdown(&mut self) {
        if matches!(self.lifecycle, TerminalLifecycle::Closed) {
            return;
        }
        if let Some(sink) = &self.recording_sink {
            sink.interrupt();
        }
        if let Some(operation) = self.connect_audit.take() {
            operation.finish(
                oxideterm_audit::AuditOutcome::Cancelled,
                oxideterm_audit::AuditEvidence::Lifecycle,
                None,
                None,
            );
        }
        if self.connected {
            if let Some(context) = &self.audit {
                context.observe(
                    oxideterm_audit::AuditCategory::Connection,
                    "ble_disconnect",
                    None,
                    oxideterm_audit::AuditOutcome::CancelRequested,
                    oxideterm_audit::AuditEvidence::Dispatch,
                    oxideterm_audit::AuditAuthorization::NotRequired,
                );
            }
            self.connected = false;
        }
        // Closing bypasses the bounded data channel: a stalled device write
        // can fill it, so the cancel token is the only delivery path that
        // cannot be silently dropped. The worker then performs the
        // unsubscribe/disconnect on its private runtime. Dropping the runtime
        // here would cancel that task before it releases the BLE link and
        // notify registration, so give it a bounded window to finish. The
        // wait runs off the UI thread.
        self.close_token.cancel();
        if let Some(runtime) = self.runtime.take() {
            let worker = self.worker.take();
            std::thread::spawn(move || {
                shutdown_ble_runtime(runtime, worker);
            });
        }
        self.lifecycle = TerminalLifecycle::Closed;
    }
}

fn shutdown_ble_runtime(runtime: Runtime, worker: Option<tokio::task::JoinHandle<()>>) {
    runtime.block_on(async {
        if let Some(mut worker) = worker {
            if tokio::time::timeout(Duration::from_secs(3), &mut worker)
                .await
                .is_err()
            {
                worker.abort();
            }
        }
    });
    runtime.shutdown_timeout(Duration::from_secs(2));
}

/// Selects the notify and write characteristics for the BLE terminal channel.
///
/// Prefers NOTIFY over INDICATE. The write side follows
/// `prefer_write_without_response`: checked (default) picks the
/// `write-without-response` characteristic to minimize round-trip latency,
/// unchecked picks the acknowledged WRITE characteristic. The respective
/// fallback keeps the channel usable on devices that expose only one of the
/// two properties.
fn select_channel_characteristics(
    characteristics: &std::collections::BTreeSet<btleplug::api::Characteristic>,
    prefer_write_without_response: bool,
) -> Option<(
    &btleplug::api::Characteristic,
    &btleplug::api::Characteristic,
    btleplug::api::WriteType,
)> {
    let mut notify_char = None;
    let mut wwr_char = None;
    let mut write_char = None;
    for characteristic in characteristics {
        let props = characteristic.properties;
        if props.contains(btleplug::api::CharPropFlags::NOTIFY) && notify_char.is_none() {
            notify_char = Some(characteristic);
        }
        if props.contains(btleplug::api::CharPropFlags::WRITE_WITHOUT_RESPONSE) {
            wwr_char = Some(characteristic);
        }
        if props.contains(btleplug::api::CharPropFlags::WRITE) && write_char.is_none() {
            write_char = Some(characteristic);
        }
    }
    // The write type follows the characteristic that actually won: the OS
    // rejects acknowledged writes on characteristics lacking the WRITE
    // property and vice versa.
    let (write_char, write_type) = if prefer_write_without_response {
        match wwr_char {
            Some(characteristic) => (
                Some(characteristic),
                btleplug::api::WriteType::WithoutResponse,
            ),
            None => (write_char, btleplug::api::WriteType::WithResponse),
        }
    } else {
        match write_char {
            Some(characteristic) => (Some(characteristic), btleplug::api::WriteType::WithResponse),
            None => (wwr_char, btleplug::api::WriteType::WithoutResponse),
        }
    };
    match (notify_char, write_char) {
        (Some(notify), Some(write)) => Some((notify, write, write_type)),
        _ => None,
    }
}

/// Splits a payload into ATT-sized chunks so a long terminal write does not
/// exceed the negotiated link-layer payload.
async fn write_with_mtu(
    peripheral: &btleplug::platform::Peripheral,
    characteristic: &btleplug::api::Characteristic,
    bytes: &[u8],
    write_type: btleplug::api::WriteType,
    config_mtu: Option<u16>,
) -> Result<(), btleplug::Error> {
    use btleplug::api::Peripheral as _;
    // The OS negotiates the ATT MTU; a configured value only caps it so
    // transfers can be throttled for devices that misbehave at larger sizes.
    let negotiated = peripheral.mtu();
    let mtu = config_mtu
        .map(|configured| configured.min(negotiated))
        .unwrap_or(negotiated) as usize;
    let max_chunk = mtu.saturating_sub(BLE_ATT_HEADER_BYTES).max(1);
    for chunk in bytes.chunks(max_chunk) {
        peripheral.write(characteristic, chunk, write_type).await?;
    }
    Ok(())
}

/// Progress milestones reported while resolving a saved BLE profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleResolveStage {
    OpeningAdapter,
    Scanning,
    Matching,
    VerifyingService,
    Ready,
}

/// Failure reasons reported by the BLE resolver. Variants keep the btleplug
/// error text for logs and diagnostics while letting the UI layer localize
/// the human-facing sentence per variant instead of parsing an English blob.
#[derive(Clone, Debug)]
pub enum BleResolveError {
    Cancelled,
    AdapterEnumeration(String),
    NoAdapters,
    AdapterOpen(String),
    /// The adapter's discovery window could not be read (guard reuse path).
    DeviceEnumeration(String),
    Scan(String),
    /// No in-range device matched the profile clues; the payload is the
    /// identity clue (MAC or advertised name) used for matching.
    NoDeviceMatched {
        identity: String,
    },
    AmbiguousDevice {
        identity: String,
    },
    Connect(String),
    ServiceDiscovery(String),
    /// The matched device does not advertise the profile's target service.
    ServiceMismatch {
        service_uuid: String,
    },
    /// The profile's service UUID does not parse, so the resolve never starts.
    InvalidServiceUuid {
        service_uuid: String,
    },
}

impl std::fmt::Display for BleResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "BLE connection cancelled"),
            Self::AdapterEnumeration(error) => {
                write!(f, "BLE adapter enumeration failed: {error}")
            }
            Self::NoAdapters => write!(f, "no Bluetooth adapters available"),
            Self::AdapterOpen(error) => write!(f, "BLE adapter open failed: {error}"),
            Self::DeviceEnumeration(error) => {
                write!(f, "BLE device enumeration failed: {error}")
            }
            Self::Scan(error) => write!(f, "BLE scan failed: {error}"),
            Self::NoDeviceMatched { identity } => {
                write!(f, "no BLE device in range matched the profile ({identity})")
            }
            Self::AmbiguousDevice { identity } => write!(
                f,
                "multiple BLE devices matched ({identity}); select the device explicitly"
            ),
            Self::Connect(error) => write!(f, "BLE connect failed: {error}"),
            Self::ServiceDiscovery(error) => write!(f, "service discovery failed: {error}"),
            Self::ServiceMismatch { service_uuid } => write!(
                f,
                "device matched but does not advertise the target service {service_uuid}"
            ),
            Self::InvalidServiceUuid { service_uuid } => {
                write!(f, "invalid service UUID: {service_uuid}")
            }
        }
    }
}

/// Closes a link opened by this flow for service inspection or a terminal
/// start that failed. A link that predated the call belongs to another
/// consumer and must survive, otherwise a later session would tear it down.
async fn disconnect_temporary_link(
    peripheral: &btleplug::platform::Peripheral,
    was_connected: bool,
) {
    use btleplug::api::Peripheral as _;
    if !was_connected {
        let _ = tokio::time::timeout(Duration::from_secs(2), peripheral.disconnect()).await;
    }
}

/// Resolves a persisted BLE profile (adapter preference + device identity +
/// service UUID) into a runtime [`BleSessionConfig`] by scanning the radio and
/// matching. Reuses a pre-existing [`oxideterm_ble::BleAdapterGuard`] when
/// supplied so the scan results it produced stay addressable on all three
/// platforms.
///
/// A live scan selection takes precedence. Saved profiles match a public MAC
/// when available, otherwise a unique advertisement name. An unmatched MAC
/// or ambiguous name must not silently connect to a different peripheral.
/// The target service is checked against advertisements or the GATT table.
///
/// Returns the runtime config plus the guard so the caller (typically
/// [`run_ble_worker`]) can stop the scan window once the terminal link is up.
pub async fn ble_resolve_profile_to_session(
    guard: Option<oxideterm_ble::BleAdapterGuard>,
    selected_device: Option<&oxideterm_ble::BlePeripheralId>,
    cancelled: &tokio_util::sync::CancellationToken,
    adapter_mac: Option<&str>,
    device_mac: Option<&str>,
    device_name: Option<&str>,
    service_uuid: &str,
    runtime_options: SerialRuntimeOptions,
    mtu: Option<u16>,
    write_without_response: bool,
    mut on_stage: impl FnMut(BleResolveStage),
) -> Result<(BleSessionConfig, oxideterm_ble::BleAdapterGuard), BleResolveError> {
    use btleplug::api::Peripheral as _;
    use uuid::Uuid;

    on_stage(BleResolveStage::OpeningAdapter);
    let target_service =
        Uuid::parse_str(service_uuid).map_err(|_| BleResolveError::InvalidServiceUuid {
            service_uuid: service_uuid.to_string(),
        })?;

    // Step 0 — adapter selection. Reuse a supplied guard; otherwise pick by
    // stored MAC preference or fall back to the first available adapter.
    let guard_supplied = guard.is_some();
    let (guard, adapter_index) = tokio::select! {
        biased;
        _ = cancelled.cancelled() => return Err(BleResolveError::Cancelled),
        result = async { Ok::<_, BleResolveError>(match guard {
            // A supplied guard wins even when the profile's adapter MAC
            // preference points elsewhere: the scan is already running on
            // the shared adapter, and the worker uses the guard's adapter
            // instead of reopening by index. The 0 is a placeholder that is
            // never read while a guard is handed over.
            Some(existing) => (existing, 0usize),
            None => open_adapter_for_profile(adapter_mac).await?,
        }) } => result?,
    };

    // Step 1 — collect discovered peripherals. A supplied guard already has
    // its discovery window open (the form's scan kept it alive so the selected
    // peripheral stays addressable); re-running `start_scan` there fails on
    // BlueZ with "Operation already in progress", so only a freshly opened
    // adapter performs a real scan window.
    on_stage(BleResolveStage::Scanning);
    let devices = if guard_supplied {
        match tokio::select! {
            biased;
            _ = cancelled.cancelled() => { guard.stop_scan().await; return Err(BleResolveError::Cancelled); }
            result = guard.discovered_devices() => result,
        } {
            Ok(devices) => devices,
            Err(error) => {
                // This attempt owns the discovery window; a failed resolve must
                // not leave the radio scanning behind.
                guard.stop_scan().await;
                return Err(BleResolveError::DeviceEnumeration(error.to_string()));
            }
        }
    } else {
        match tokio::select! {
            biased;
            _ = cancelled.cancelled() => { guard.stop_scan().await; return Err(BleResolveError::Cancelled); }
            result = guard.scan(BLE_SAVED_PROFILE_SCAN_WINDOW) => result,
        } {
            Ok(devices) => devices,
            Err(error) => {
                guard.stop_scan().await;
                return Err(BleResolveError::Scan(error.to_string()));
            }
        }
    };

    // A live scan selection is authoritative; saved profiles must resolve uniquely.
    on_stage(BleResolveStage::Matching);
    let device = match match_ble_device(&devices, selected_device, device_mac, device_name) {
        Ok(device) => device,
        Err(error) => {
            guard.stop_scan().await;
            return Err(error);
        }
    };

    // Step 3 — service UUID verification. Fast path uses the advertised
    // service list (no round-trip); slow path connects and discovers the
    // full GATT table when the advertisement hint is missing or incomplete.
    on_stage(BleResolveStage::VerifyingService);
    let uuid_match = if device.advertised_services.contains(&target_service) {
        true
    } else {
        let (peripheral, was_connected) = match oxideterm_ble::ble_connect_discovered_device(
            guard.adapter(),
            &device.id,
            BLE_CONNECT_TIMEOUT,
            cancelled,
        )
        .await
        {
            Ok(pair) => pair,
            Err(error) => {
                guard.stop_scan().await;
                if matches!(error, oxideterm_ble::BleError::Cancelled) {
                    return Err(BleResolveError::Cancelled);
                }
                return Err(BleResolveError::Connect(error.to_string()));
            }
        };
        let discovered = tokio::select! {
            biased;
            _ = cancelled.cancelled() => Err(BleResolveError::Cancelled),
            result = peripheral.discover_services() => result.map_err(|error| BleResolveError::ServiceDiscovery(error.to_string())),
        };
        let has_target = match discovered {
            Ok(()) => peripheral
                .services()
                .iter()
                .any(|service| service.uuid == target_service),
            Err(error) => {
                disconnect_temporary_link(&peripheral, was_connected).await;
                guard.stop_scan().await;
                return Err(error);
            }
        };
        // The verification link is temporary; a link that predated this check
        // belongs to another consumer and must survive, otherwise the worker
        // that follows would have to reconnect a link it does not own.
        disconnect_temporary_link(&peripheral, was_connected).await;
        has_target
    };

    if !uuid_match {
        let _ = guard.stop_scan().await;
        return Err(BleResolveError::ServiceMismatch {
            service_uuid: service_uuid.to_string(),
        });
    }

    let config = BleSessionConfig {
        adapter_index,
        device_id: device.id.clone(),
        service_uuid: target_service,
        device_label: device
            .name
            .clone()
            .unwrap_or_else(|| device.address.clone()),
        runtime_options,
        mtu,
        write_without_response,
    };
    on_stage(BleResolveStage::Ready);
    Ok((config, guard))
}

fn match_ble_device<'a>(
    devices: &'a [oxideterm_ble::BleDeviceInfo],
    selected: Option<&oxideterm_ble::BlePeripheralId>,
    mac: Option<&str>,
    name: Option<&str>,
) -> Result<&'a oxideterm_ble::BleDeviceInfo, BleResolveError> {
    let mac = mac.filter(|value| {
        value
            .parse::<btleplug::api::BDAddr>()
            .is_ok_and(|address| address != btleplug::api::BDAddr::default())
    });
    let identity = selected
        .map(ToString::to_string)
        .unwrap_or_else(|| mac.or(name).unwrap_or("no identity clue").to_owned());
    let mut matches = devices.iter().filter(|device| {
        if let Some(id) = selected {
            return &device.id == id;
        }
        if let Some(mac) = mac {
            return device
                .persistent_address()
                .is_some_and(|address| address.eq_ignore_ascii_case(mac));
        }
        name.is_some_and(|name| device.name.as_deref() == Some(name))
    });
    let first = matches
        .next()
        .ok_or_else(|| BleResolveError::NoDeviceMatched {
            identity: identity.clone(),
        })?;
    if matches.next().is_some() {
        return Err(BleResolveError::AmbiguousDevice { identity });
    }
    Ok(first)
}

/// Opens a [`oxideterm_ble::BleAdapterGuard`] for the profile, honoring the
/// stored adapter MAC preference. Falls back to the first available adapter
/// when the preference is absent or no adapter matches.
async fn open_adapter_for_profile(
    adapter_mac: Option<&str>,
) -> Result<(oxideterm_ble::BleAdapterGuard, usize), BleResolveError> {
    let infos = oxideterm_ble::ble_list_adapters()
        .await
        .map_err(|error| BleResolveError::AdapterEnumeration(error.to_string()))?;
    if infos.is_empty() {
        return Err(BleResolveError::NoAdapters);
    }

    let index = if let Some(pref) = adapter_mac {
        infos
            .iter()
            .position(|info| {
                info.adapter_mac
                    .as_deref()
                    .map(|mac| mac.eq_ignore_ascii_case(pref))
                    .unwrap_or(false)
            })
            .unwrap_or(0)
    } else {
        0
    };

    let guard = oxideterm_ble::BleAdapterGuard::open(index)
        .await
        .map_err(|error| BleResolveError::AdapterOpen(error.to_string()))?;
    Ok((guard, index))
}

async fn run_ble_worker(
    config: BleSessionConfig,
    mut adapter_guard: Option<oxideterm_ble::BleAdapterGuard>,
    mut command_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    worker_tx: crate::backpressure::ByteBoundedSender<BleWorkerEvent>,
    close_token: tokio_util::sync::CancellationToken,
    i18n: oxideterm_i18n::I18n,
) {
    use btleplug::api::{Manager as _, Peripheral as _};
    use futures_util::StreamExt;

    // Reuse the adapter that produced the scan results when the form handed it
    // over; the device object (Linux) or peripheral cache (macOS/Windows) is
    // still valid against it. Fall back to opening a fresh adapter only when
    // the guard was not provided (e.g. tests or future reconnect paths).
    let adapter = if let Some(guard) = adapter_guard.as_ref() {
        guard.adapter().clone()
    } else {
        let manager = match btleplug::platform::Manager::new().await {
            Ok(manager) => manager,
            Err(error) => {
                let _ = worker_tx.send_control(BleWorkerEvent::Failed(error.to_string()));
                return;
            }
        };
        match manager.adapters().await {
            Ok(adapters) => match adapters.into_iter().nth(config.adapter_index) {
                Some(adapter) => adapter,
                None => {
                    let _ = worker_tx.send_control(BleWorkerEvent::Failed(i18n.t_with(
                        "terminal.ble.adapter_index_out_of_range",
                        &[("n", &config.adapter_index.to_string())],
                    )));
                    return;
                }
            },
            Err(error) => {
                let _ = worker_tx.send_control(BleWorkerEvent::Failed(error.to_string()));
                return;
            }
        }
    };
    // The scan's discovery window stays open through connect: on Linux it keeps
    // the BlueZ object alive, and on macOS/Windows the cache was populated by
    // the scan. Once the link is up the device is no longer temporary, so the
    // guard can be dropped to release the radio.
    let (peripheral, was_connected) = match oxideterm_ble::ble_connect_discovered_device(
        &adapter,
        &config.device_id,
        BLE_CONNECT_TIMEOUT,
        &close_token,
    )
    .await
    {
        Ok(pair) => pair,
        Err(error) => {
            // Connect failed before the link was up; release the discovery
            // window so the radio does not stay active for a dead session.
            if let Some(guard) = adapter_guard.take() {
                guard.stop_scan().await;
            }
            let _ = worker_tx.send_control(BleWorkerEvent::Failed(error.to_string()));
            return;
        }
    };
    let mut subscribed = None;
    let connected_worker = async {
        if let Some(guard) = adapter_guard.as_ref() {
            guard.stop_scan().await;
        }
        adapter_guard = None;
        if let Err(error) = peripheral.discover_services().await {
            // A failed start must not leave the physical link (or the notify
            // registration below) behind: leaked links make every reopened session
            // subscribe to an already-live connection and receive duplicate data.
            let _ = worker_tx.send_control(BleWorkerEvent::Failed(error.to_string()));
            return;
        }
        let services = peripheral.services();
        let Some(service) = services
            .iter()
            .find(|service| service.uuid == config.service_uuid)
        else {
            let _ = worker_tx.send_control(BleWorkerEvent::Failed(i18n.t_with(
                "terminal.ble.service_not_found",
                &[("uuid", &config.service_uuid.to_string())],
            )));
            return;
        };
        let Some((notify_char, write_char, write_type)) =
            select_channel_characteristics(&service.characteristics, config.write_without_response)
        else {
            let _ = worker_tx.send_control(BleWorkerEvent::Failed(
                i18n.t("terminal.ble.no_notify_write_pair"),
            ));
            return;
        };
        let notify_uuid = notify_char.uuid;
        subscribed = Some(notify_char.clone());
        if let Err(error) = peripheral.subscribe(notify_char).await {
            let _ = worker_tx.send_control(BleWorkerEvent::Failed(error.to_string()));
            return;
        }
        let _ = worker_tx.send_control(BleWorkerEvent::Connected);

        let mut notifications = match peripheral.notifications().await {
            Ok(stream) => stream,
            Err(error) => {
                let _ = worker_tx.send_control(BleWorkerEvent::Failed(error.to_string()));
                return;
            }
        };
        let mut link_check = tokio::time::interval(BLE_LINK_CHECK_INTERVAL);
        link_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        link_check.tick().await;
        // Emit the negotiated MTU once after subscribe completes; platforms that
        // update the value later will re-emit it on the first link-check tick.
        let mut last_mtu = peripheral.mtu();
        let _ = worker_tx.send_control(BleWorkerEvent::NegotiatedMtu(last_mtu));
        loop {
            tokio::select! {
                // Close rides the cancel token, not the data channel, so it wins
                // even while every other await point is stalled.
                _ = close_token.cancelled() => {
                    break;
                }
                _ = link_check.tick() => {
                    // `Err` means the platform cannot answer the query; only a
                    // definite `false` proves the peer is gone.
                    if matches!(peripheral.is_connected().await, Ok(false)) {
                        let _ = worker_tx.send_control(BleWorkerEvent::Closed);
                        break;
                    }
                    // Some platforms finalise the ATT MTU after subscribe returns;
                    // surface late updates so the UI reflects the real value.
                    let current_mtu = peripheral.mtu();
                    if current_mtu != last_mtu {
                        last_mtu = current_mtu;
                        let _ = worker_tx.send_control(BleWorkerEvent::NegotiatedMtu(current_mtu));
                    }
                }
                notification = notifications.next() => {
                    match notification {
                        Some(notification) if notification.uuid == notify_uuid => {
                            let data = notification.value;
                            let data_len = data.len();
                            // Output backpressure waits for the UI drain; the
                            // cancel branch keeps shutdown reaching the cleanup
                            // below instead of waiting out the stalled consumer.
                            tokio::select! {
                                _ = close_token.cancelled() => break,
                                sent = worker_tx.send_async(BleWorkerEvent::Output(data), data_len) => {
                                    if sent.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Some(_) => {}
                        None => {
                            let _ = worker_tx.send_control(BleWorkerEvent::Closed);
                            break;
                        }
                    }
                }
                command = command_rx.recv() => {
                    match command {
                        Some(bytes) => {
                            // A stalled device write must not delay the close: the
                            // cancel branch releases the link without waiting out
                            // the blocked transport.
                            tokio::select! {
                                _ = close_token.cancelled() => break,
                                written = write_with_mtu(&peripheral, write_char, &bytes, write_type, config.mtu) => {
                                    if let Err(error) = written {
                                        let _ = worker_tx
                                            .send_control(BleWorkerEvent::Failed(error.to_string()));
                                        break;
                                    }
                                }
                            }
                        }
                        None => {
                            let _ = worker_tx.send_control(BleWorkerEvent::Closed);
                            break;
                        }
                    }
                }
            }
        }
    };
    tokio::select! {
        biased;
        _ = close_token.cancelled() => {},
        _ = connected_worker => {},
    }
    // Every exit path releases the notify registration and the physical link,
    // otherwise the next open reuses a live connection and receives one extra
    // copy of every peer notification per leaked attempt.
    if let Some(characteristic) = subscribed {
        // A stalled unsubscribe must still leave time to disconnect the link.
        let _ = tokio::time::timeout(
            Duration::from_millis(500),
            peripheral.unsubscribe(&characteristic),
        )
        .await;
    }
    disconnect_temporary_link(&peripheral, was_connected).await;
    if let Some(guard) = adapter_guard.take() {
        guard.stop_scan().await;
    }
}

#[cfg(test)]
mod ble_tests {
    use super::*;

    #[test]
    fn shutdown_waits_for_async_worker_cleanup_before_stopping_runtime() {
        let runtime = Runtime::new().unwrap();
        let cancellation = tokio_util::sync::CancellationToken::new();
        let worker_cancel = cancellation.clone();
        let (complete_tx, complete_rx) = std::sync::mpsc::channel();
        let worker = runtime.spawn(async move {
            worker_cancel.cancelled().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            complete_tx.send("cleanup completed").unwrap();
        });
        cancellation.cancel();
        shutdown_ble_runtime(runtime, Some(worker));
        assert_eq!(complete_rx.try_recv().unwrap(), "cleanup completed");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn live_identity_wins_over_placeholder_addresses_and_names_must_be_unique() {
        use btleplug::api::AddressType;
        let mut devices = (1..=2)
            .map(|id| oxideterm_ble::BleDeviceInfo {
                id: uuid::Uuid::from_u128(id).into(),
                name: Some("UART".into()),
                address: "00:00:00:00:00:00".into(),
                rssi: None,
                supports_nus: true,
                advertised_services: vec![oxideterm_ble::NORDIC_UART_SERVICE_UUID],
                address_type: None,
            })
            .collect::<Vec<_>>();
        let selected = devices[1].id.clone();
        assert_eq!(
            match_ble_device(
                &devices,
                Some(&selected),
                Some("00:00:00:00:00:00"),
                Some("UART")
            )
            .unwrap()
            .id,
            selected
        );
        assert!(matches!(
            match_ble_device(&devices, None, Some("00:00:00:00:00:00"), Some("UART")),
            Err(BleResolveError::AmbiguousDevice { .. })
        ));
        assert!(devices[0].persistent_address().is_none());
        devices[0].address = "12:34:56:78:9A:BC".into();
        devices[0].address_type = Some(AddressType::Random);
        assert!(devices[0].persistent_address().is_none());
        devices[0].address_type = Some(AddressType::Public);
        assert_eq!(devices[0].persistent_address(), Some("12:34:56:78:9A:BC"));
        assert_eq!(
            match_ble_device(&devices, None, Some("12:34:56:78:9a:bc"), Some("UART"))
                .unwrap()
                .id,
            devices[0].id
        );
        assert!(matches!(
            match_ble_device(&devices, None, Some("AB:CD:EF:01:23:45"), Some("UART")),
            Err(BleResolveError::NoDeviceMatched { .. })
        ));
    }

    fn characteristic(
        uuid: u128,
        properties: btleplug::api::CharPropFlags,
    ) -> btleplug::api::Characteristic {
        btleplug::api::Characteristic {
            uuid: uuid::Uuid::from_u128(uuid),
            service_uuid: uuid::Uuid::from_u128(0x0002),
            properties,
            descriptors: std::collections::BTreeSet::new(),
        }
    }

    #[test]
    fn select_channel_characteristics_prefers_notify_and_write_without_response() {
        let notify = characteristic(0x0001, btleplug::api::CharPropFlags::NOTIFY);
        let indicate = characteristic(0x0003, btleplug::api::CharPropFlags::INDICATE);
        let write = characteristic(0x0004, btleplug::api::CharPropFlags::WRITE);
        let wwr = characteristic(0x0005, btleplug::api::CharPropFlags::WRITE_WITHOUT_RESPONSE);
        let mut characteristics = std::collections::BTreeSet::new();
        characteristics.insert(indicate.clone());
        characteristics.insert(write.clone());
        characteristics.insert(notify.clone());
        characteristics.insert(wwr.clone());
        let (n, w, wt) = select_channel_characteristics(&characteristics, true).unwrap();
        assert_eq!(n.uuid, notify.uuid);
        assert_eq!(w.uuid, wwr.uuid);
        assert_eq!(wt, btleplug::api::WriteType::WithoutResponse);
    }

    #[test]
    fn select_channel_characteristics_unchecked_prefers_acknowledged_write() {
        let notify = characteristic(0x0001, btleplug::api::CharPropFlags::NOTIFY);
        let write = characteristic(0x0004, btleplug::api::CharPropFlags::WRITE);
        let wwr = characteristic(0x0005, btleplug::api::CharPropFlags::WRITE_WITHOUT_RESPONSE);
        let mut characteristics = std::collections::BTreeSet::new();
        characteristics.insert(write.clone());
        characteristics.insert(notify.clone());
        characteristics.insert(wwr.clone());
        let (n, w, wt) = select_channel_characteristics(&characteristics, false).unwrap();
        assert_eq!(n.uuid, notify.uuid);
        assert_eq!(w.uuid, write.uuid);
        assert_eq!(wt, btleplug::api::WriteType::WithResponse);
    }

    #[test]
    fn select_channel_characteristics_unchecked_falls_back_to_write_without_response() {
        // Devices exposing only a write-without-response characteristic must
        // keep the channel usable when the acknowledged write is requested:
        // the OS rejects WithResponse on characteristics without WRITE.
        let notify = characteristic(0x0001, btleplug::api::CharPropFlags::NOTIFY);
        let wwr = characteristic(0x0005, btleplug::api::CharPropFlags::WRITE_WITHOUT_RESPONSE);
        let mut characteristics = std::collections::BTreeSet::new();
        characteristics.insert(wwr.clone());
        characteristics.insert(notify.clone());
        let (n, w, wt) = select_channel_characteristics(&characteristics, false).unwrap();
        assert_eq!(n.uuid, notify.uuid);
        assert_eq!(w.uuid, wwr.uuid);
        assert_eq!(wt, btleplug::api::WriteType::WithoutResponse);
    }

    #[test]
    fn select_channel_characteristics_rejects_missing_roles() {
        let read_only = characteristic(0x0001, btleplug::api::CharPropFlags::READ);
        let mut characteristics = std::collections::BTreeSet::new();
        characteristics.insert(read_only);
        assert!(select_channel_characteristics(&characteristics, true).is_none());
    }
}
