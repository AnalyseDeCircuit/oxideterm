use super::*;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use zeroize::Zeroizing;

const CONTROL_BYTES: usize = 1024 * 1024;
const CONTROL_COUNT: usize = 256;
const EVENT_BYTES: usize = 1024 * 1024;

struct Control {
    boundary: u64,
    bytes: usize,
    apply: Box<dyn FnOnce(&mut SshPtyCore) -> Result<()> + Send>,
}

#[derive(Default)]
struct Controls {
    queue: VecDeque<Control>,
    bytes: usize,
}

struct Status {
    title: Option<String>,
    lifecycle: TerminalLifecycle,
    interactive: bool,
    mode: TermMode,
    connection: Option<SshConnectionHandle>,
}

struct Shared {
    core: FairMutex<SshPtyCore>,
    controls: Mutex<Controls>,
    cancelled: AtomicBool,
    finished: AtomicBool,
    failed: AtomicBool,
    output_cancel: Mutex<Option<oxideterm_ssh::SshOutputCancellation>>,
    output_boundary: Mutex<Option<oxideterm_ssh::SshOutputBoundary>>,
    report: Mutex<TerminalDrainReport>,
    events: Mutex<Vec<TerminalEvent>>,
    event_bytes: std::sync::atomic::AtomicUsize,
    status: Mutex<Status>,
    ready_trzsz: Mutex<VecDeque<TrzszTransfer>>,
    wake: crate::activity::TerminalActivitySender,
    ui: crate::activity::TerminalActivitySender,
}

// Completion tasks retain the runtime until their parser thread has joined.
// The UI only removes finished tasks; it never joins an OS thread itself.
struct ParserTask {
    completion: tokio::task::JoinHandle<()>,
    runtime: Arc<Runtime>,
}
static PARSER_TASKS: OnceLock<Mutex<Vec<ParserTask>>> = OnceLock::new();
static STANDALONE_RUNTIME: Mutex<Option<Arc<Runtime>>> = Mutex::new(None);

pub(super) fn standalone_runtime() -> Option<Arc<Runtime>> {
    // Standalone users can share registry consumers without supplying a runtime.
    // That runtime therefore belongs to the process, not to one terminal pane.
    let mut owner = STANDALONE_RUNTIME.lock().expect("standalone SSH runtime");
    if owner.is_none() {
        *owner = Runtime::new().ok().map(Arc::new);
    }
    owner.clone()
}

fn reap_workers() {
    let mut tasks = PARSER_TASKS
        .get_or_init(Default::default)
        .lock()
        .expect("SSH parser tasks");
    let mut i = 0;
    while i < tasks.len() {
        if tasks[i].completion.is_finished() {
            let task = tasks.swap_remove(i);
            // If the application has also relinquished this runtime, all owned
            // parser joins are complete. Shut it down without a UI-thread wait.
            if let Ok(runtime) = Arc::try_unwrap(task.runtime) {
                runtime.shutdown_background();
            }
        } else {
            i += 1;
        }
    }
}

pub struct SshPtySession {
    shared: Arc<Shared>,
    activity: TerminalActivityReceiver,
}

impl SshPtySession {
    pub fn new(
        config: SshSessionConfig,
        cols: usize,
        rows: usize,
        graphics: GraphicsOptions,
        encoding: TerminalEncoding,
        scrollback: usize,
    ) -> Self {
        Self::from_core(SshPtyCore::new(
            config, cols, rows, graphics, encoding, scrollback,
        ))
    }

    fn from_core(core: SshPtyCore) -> Self {
        reap_workers();
        let runtime = core.runtime.clone();
        let (ui, activity) = crate::activity::terminal_activity_channel();
        let wake_rx = core.activity_receiver();
        let status = Status {
            title: core.title(),
            lifecycle: core.lifecycle(),
            interactive: core.is_interactive(),
            mode: core.mode(),
            connection: core.ssh_connection_handle(),
        };
        let shared = Arc::new(Shared {
            wake: core.activity.clone(),
            core: FairMutex::new(core),
            controls: Default::default(),
            cancelled: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            output_cancel: Default::default(),
            output_boundary: Default::default(),
            report: Default::default(),
            events: Default::default(),
            event_bytes: Default::default(),
            status: Mutex::new(status),
            ready_trzsz: Default::default(),
            ui,
        });
        let Some(runtime) = runtime else {
            shared.failed.store(true, Ordering::Release);
            finish_worker(&shared);
            shared.finished.store(true, Ordering::Release);
            return Self { shared, activity };
        };
        let worker_shared = shared.clone();
        let worker_runtime = runtime.handle().clone();
        let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
        match std::thread::Builder::new()
            .name("ssh-terminal-parser".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_worker(&worker_shared, wake_rx, worker_runtime)
                }));
                if result.is_err() {
                    worker_shared.failed.store(true, Ordering::Release);
                }
                finish_worker(&worker_shared);
                worker_shared.finished.store(true, Ordering::Release);
                worker_shared.ui.notify();
                let _ = finished_tx.send(());
            }) {
            Ok(thread) => {
                let completion = runtime.spawn(async move {
                    // Do not occupy a second OS thread while the parser is alive.
                    let _ = finished_rx.await;
                    let joined = tokio::task::spawn_blocking(move || {
                        if thread.join().is_err() {
                            tracing::error!("SSH parser thread panicked during cleanup");
                        }
                    })
                    .await;
                    if joined.is_err() {
                        tracing::error!("SSH parser completion task failed");
                    }
                });
                PARSER_TASKS
                    .get_or_init(Default::default)
                    .lock()
                    .expect("SSH parser tasks")
                    .push(ParserTask {
                        completion,
                        runtime,
                    });
            }
            Err(_) => {
                shared.failed.store(true, Ordering::Release);
                let failed = shared.clone();
                let completion = runtime.spawn_blocking(move || {
                    finish_worker(&failed);
                    failed.finished.store(true, Ordering::Release);
                    failed.ui.notify();
                });
                PARSER_TASKS
                    .get_or_init(Default::default)
                    .lock()
                    .expect("SSH parser tasks")
                    .push(ParserTask {
                        completion,
                        runtime,
                    });
            }
        }
        Self { shared, activity }
    }

    fn enqueue(
        &self,
        bytes: usize,
        apply: impl FnOnce(&mut SshPtyCore) -> Result<()> + Send + 'static,
    ) -> Result<()> {
        if self.shared.cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        if self.shared.finished.load(Ordering::Acquire) {
            bail!("SSH terminal channel closed");
        }
        let mut controls = self.shared.controls.lock().expect("SSH parser controls");
        // A single user paste may already exceed the ordinary queue budget.
        // Retain that one owned request, preserving the existing input contract;
        // additional payloads still see backpressure until it is accepted.
        let oversized_request = bytes > CONTROL_BYTES && controls.bytes == 0;
        if controls.queue.len() >= CONTROL_COUNT
            || (!oversized_request && bytes > CONTROL_BYTES.saturating_sub(controls.bytes))
        {
            drop(controls);
            self.shared.wake.notify();
            bail!("SSH terminal input queue is full");
        }
        controls.bytes += bytes;
        let boundary = self
            .shared
            .output_boundary
            .lock()
            .expect("SSH output boundary")
            .as_ref()
            .map_or(0, |boundary| boundary.capture());
        controls.queue.push_back(Control {
            boundary,
            bytes,
            apply: Box::new(apply),
        });
        drop(controls);
        self.shared.wake.notify();
        Ok(())
    }

    fn enqueue_control(
        &self,
        bytes: usize,
        apply: impl FnOnce(&mut SshPtyCore) -> Result<()> + Send + 'static,
    ) {
        if self.enqueue(bytes, apply).is_err() {
            self.shared.failed.store(true, Ordering::Release);
            self.shared.wake.notify();
        }
    }

    fn cancel(&self) {
        if !self.shared.cancelled.swap(true, Ordering::AcqRel) {
            if let Some(cancel) = self
                .shared
                .output_cancel
                .lock()
                .expect("SSH output cancellation")
                .as_ref()
            {
                cancel.cancel();
            }
            self.shared.wake.notify();
            self.shared.ui.notify();
        }
    }
}

impl Drop for SshPtySession {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn run_worker(shared: &Shared, wake_rx: TerminalActivityReceiver, runtime: tokio::runtime::Handle) {
    let mut control: Option<(Control, u64)> = None;
    let mut writable_task: Option<tokio::task::JoinHandle<()>> = None;
    loop {
        if shared.cancelled.load(Ordering::Acquire) || shared.failed.load(Ordering::Acquire) {
            break;
        }
        let mut progress = false;
        let mut delay = None;
        let mut wait_writer = None;
        {
            let mut core = shared.core.lock();
            if shared.cancelled.load(Ordering::Acquire) {
                break;
            }
            if core.handle.is_none() && control.is_none() {
                // Apply the finite set of controls already accepted before
                // startup, so recording/encoding precede authentication banners.
                let startup = {
                    let mut queue = shared.controls.lock().expect("SSH parser controls");
                    queue.bytes = 0;
                    std::mem::take(&mut queue.queue)
                };
                for command in startup {
                    if shared.cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    if (command.apply)(&mut core).is_err() {
                        shared.failed.store(true, Ordering::Release);
                    }
                    progress = true;
                }
            }
            if core.process_connect_result() {
                progress = true;
                if let Some(handle) = &core.handle {
                    *shared
                        .output_cancel
                        .lock()
                        .expect("SSH output cancellation") =
                        Some(handle.output_rx.cancellation_handle());
                    *shared.output_boundary.lock().expect("SSH output boundary") =
                        Some(handle.output_rx.boundary_handle());
                }
            }
            if control.is_none() {
                let next = {
                    let mut queue = shared.controls.lock().expect("SSH parser controls");
                    let next = queue.queue.pop_front();
                    if let Some(next) = &next {
                        queue.bytes -= next.bytes;
                    }
                    next
                };
                if let Some(next) = next {
                    let boundary = next.boundary;
                    control = Some((next, boundary));
                }
            }
            core.parser_state.flush_trzsz_server_writes();
            core.parser_state.flush_modem_server_writes();
            core.parser_state.flush_tmux_commands();
            if core.parser_state.flush_pending_writes().is_err() && core.lifecycle.is_running() {
                shared.failed.store(true, Ordering::Release);
            }
            let events_full = shared.event_bytes.load(Ordering::Acquire) >= EVENT_BYTES;
            if !core.parser_state.pending_writes.is_empty() {
                wait_writer = core
                    .parser_state
                    .command_tx
                    .clone()
                    .zip(core.runtime.clone());
            } else if !events_full && !core.parser_state.transfer_input_full() {
                if control.as_ref().is_some_and(|(_, boundary)| {
                    core.consumed_sequence >= *boundary || !core.lifecycle.is_running()
                }) {
                    let (command, _) = control.take().unwrap();
                    if (command.apply)(&mut core).is_err() {
                        shared.failed.store(true, Ordering::Release);
                    }
                    progress = true;
                } else {
                    let report = core.read_pending_with_budget(
                        TerminalDrainBudget::new(SSH_OUTPUT_PARSE_SLICE_BYTES, 1)
                            .with_performance_metrics(true),
                    );
                    progress |= report.changed || report.drained_bytes > 0;
                    shared
                        .report
                        .lock()
                        .expect("SSH parser report")
                        .combine(report);
                }
                // Terminal-generated replies must reach the ordered writer before
                // another transport slice can generate dependent side effects.
                while let Ok(event) = core.parser_state.event_rx.try_recv() {
                    progress |= core.parser_state.handle_alacritty_event(event);
                }
                if core
                    .parser_state
                    .pending_events
                    .iter()
                    .any(|event| matches!(event, TerminalEvent::TrzszTransferPrompt { .. }))
                {
                    if let Some(transfer) = core.parser_state.take_trzsz_transfer() {
                        shared
                            .ready_trzsz
                            .lock()
                            .expect("SSH transfer handoff")
                            .push_back(transfer);
                    }
                }
                delay = core.pending_output_flush_delay();
            }
            if progress {
                shared
                    .report
                    .lock()
                    .expect("SSH parser report")
                    .mark_changed();
            }
            publish(shared, &mut core);
        }
        if let Some((sender, runtime)) = wait_writer {
            if writable_task.as_ref().is_none_or(|task| task.is_finished()) {
                let wake = shared.wake.clone();
                writable_task = Some(runtime.spawn(async move {
                    let _ = sender.reserve_owned().await;
                    wake.notify();
                }));
            }
        } else if progress {
            continue;
        }
        // Only idle waiting enters the runtime. Parsing stays on this OS thread,
        // and the same bounded activity channel wakes output, controls and close.
        runtime.block_on(async {
            match delay {
                Some(delay) => {
                    let _ = tokio::time::timeout(delay, wake_rx.notified()).await;
                }
                None => {
                    let _ = wake_rx.notified().await;
                }
            }
        });
    }
    if let Some(task) = writable_task {
        task.abort();
    }
}

fn finish_worker(shared: &Shared) {
    // Release the render lock before waiting for network/startup cleanup. The
    // retained handle keeps the terminal lease alive throughout those waits.
    let (mut handle, connect_task, runtime) = {
        let mut core = shared.core.lock();
        if let Some(handle) = &mut core.handle {
            handle.output_rx.close();
        }
        core.parser_state.interrupt_trzsz_transfer();
        core.parser_state.interrupt_modem_transfer();
        core.output_queue.clear();
        core.output_queue_bytes = 0;
        core.parser_state.retire_transport();
        core.parser_state.pending_events.clear();
        if shared.failed.load(Ordering::Acquire) {
            core.lifecycle = TerminalLifecycle::Exited(None);
            tracing::error!("SSH terminal processing failed");
            core.parser_state
                .pending_events
                .push(TerminalEvent::ProcessingFailed);
        } else {
            core.lifecycle = TerminalLifecycle::Closed;
        }
        let handle = core.handle.take();
        if shared.cancelled.load(Ordering::Acquire) {
            shared.events.lock().expect("SSH parser events").clear();
            shared.event_bytes.store(0, Ordering::Release);
        }
        publish(shared, &mut core);
        (handle, core.connect_task.take(), core.runtime.take())
    };
    shared
        .report
        .lock()
        .expect("SSH parser report")
        .mark_changed();
    shared.ui.notify();
    if let Some(task) = connect_task {
        task.abort();
        if let Some(runtime) = &runtime {
            let _ = runtime.block_on(task);
        }
    }
    {
        let core = shared.core.lock();
        while let Ok(result) = core.connect_rx.try_recv() {
            if let Ok(mut late_handle) = result {
                late_handle.output_rx.close();
            }
        }
    }
    if let Some(handle) = &mut handle {
        handle.output_rx.close();
        if let Some(runtime) = &runtime {
            let sender = handle.command_tx.clone();
            let sent = runtime.block_on(async {
                tokio::time::timeout(
                    Duration::from_secs(5),
                    sender.send(SshTransportCommand::Close),
                )
                .await
            });
            if sent.is_err() {
                tracing::error!("SSH terminal close timed out");
            }
        }
    }
    drop(handle);
    shared
        .controls
        .lock()
        .expect("SSH parser controls")
        .queue
        .clear();
    shared
        .ready_trzsz
        .lock()
        .expect("SSH transfer handoff")
        .clear();
    // Both injected and standalone runtimes have an owner beyond this parser.
    // Release this worker's reference after its transport cleanup completes.
    drop(runtime);
}

fn publish(shared: &Shared, core: &mut SshPtyCore) {
    {
        let mut status = shared.status.lock().expect("SSH parser status");
        if let Some(title) = &core.parser_state.title {
            if status.title.as_ref() != Some(title) {
                status.title = Some(title.clone());
            }
        }
        status.lifecycle = core.lifecycle();
        status.interactive = core.is_interactive();
        status.mode = core.mode();
        if core.handle.is_none() {
            status.connection = None;
        } else if status.connection.is_none() {
            status.connection = core.ssh_connection_handle();
        }
    }
    if !core.parser_state.pending_events.is_empty() {
        let mut events = shared.events.lock().expect("SSH parser events");
        for event in core.take_events() {
            if matches!(event, TerminalEvent::Wakeup)
                && matches!(events.last(), Some(TerminalEvent::Wakeup))
            {
                continue;
            }
            shared
                .event_bytes
                .fetch_add(event_bytes(&event), Ordering::Release);
            events.push(event);
        }
    }
    if shared.report.lock().expect("SSH parser report").changed
        || shared.event_bytes.load(Ordering::Acquire) > 0
    {
        shared.ui.notify();
    }
}

fn event_bytes(event: &TerminalEvent) -> usize {
    let bytes = match event {
        TerminalEvent::Output(bytes) => bytes.capacity(),
        TerminalEvent::TitleChanged(text) | TerminalEvent::ClipboardStore(text) => text.capacity(),
        TerminalEvent::CwdChanged { cwd, host } => {
            cwd.capacity() + host.as_ref().map_or(0, String::capacity)
        }
        TerminalEvent::EditorClipboard(event) => event.text.capacity(),
        TerminalEvent::ShellIntegration(event) => {
            event.sequence.capacity()
                + event.raw.capacity()
                + event.command.as_ref().map_or(0, String::capacity)
        }
        TerminalEvent::CommandMark(
            crate::TerminalCommandMarkEvent::Created(mark)
            | crate::TerminalCommandMarkEvent::Closed(mark),
        ) => mark.command_id.capacity() + mark.command.as_ref().map_or(0, String::capacity),
        TerminalEvent::TriggerMatched(event) => event.retained_bytes(),
        // Prompt text is bounded by its scanner, and the remaining variants
        // carry fixed metadata or shared protocol handles.
        TerminalEvent::PrivilegePrompt(crate::TerminalPrivilegePromptEvent::Visible {
            prompt,
            ..
        }) => match prompt {
            crate::TerminalPrivilegePrompt::Sudo {
                username,
                prompt_text,
            } => prompt_text.capacity() + username.as_ref().map_or(0, String::capacity),
            crate::TerminalPrivilegePrompt::Su {
                target_user,
                prompt_text,
            } => prompt_text.capacity() + target_user.as_ref().map_or(0, String::capacity),
            crate::TerminalPrivilegePrompt::GenericPassword { prompt_text } => {
                prompt_text.capacity()
            }
        },
        _ => 0,
    };
    std::mem::size_of::<TerminalEvent>() + bytes
}

impl TerminalSessionBackend for SshPtySession {
    fn kind(&self) -> TerminalSessionKind {
        self.shared.core.lock().kind()
    }

    fn title(&self) -> Option<String> {
        self.shared
            .status
            .lock()
            .expect("SSH parser status")
            .title
            .clone()
    }

    fn is_interactive(&self) -> bool {
        !self.shared.cancelled.load(Ordering::Acquire)
            && self
                .shared
                .status
                .lock()
                .expect("SSH parser status")
                .interactive
    }

    fn process_info(&self) -> TerminalProcessInfo {
        self.shared.core.lock().process_info()
    }

    fn refresh_process_info(&mut self) {}

    fn pending_output_flush_delay(&self) -> Option<Duration> {
        None
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        let bytes = Zeroizing::new(bytes.to_vec());
        let cost = bytes.len();
        self.enqueue(cost, move |core| core.write_input(&bytes))
    }

    fn write_protocol_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let bytes = Zeroizing::new(bytes.to_vec());
        let cost = bytes.len();
        self.enqueue(cost, move |core| core.write_protocol_bytes(&bytes))
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        let text = Zeroizing::new(text.to_string());
        let cost = text.len();
        self.enqueue(cost, move |core| core.write_text(&text))
    }

    fn paste_text(&mut self, text: &str) -> Result<()> {
        let text = Zeroizing::new(text.to_string());
        let cost = text.len();
        self.enqueue(cost, move |core| core.paste_text(&text))
    }

    fn set_encoding(&mut self, encoding: TerminalEncoding) {
        self.enqueue_control(0, move |core| {
            core.set_encoding(encoding);
            Ok(())
        });
    }

    fn set_output_processor(&mut self, processor: Option<TerminalOutputProcessor>) {
        self.enqueue_control(0, move |core| {
            core.set_output_processor(processor);
            Ok(())
        });
    }

    fn set_output_events_enabled(&mut self, enabled: bool) {
        self.enqueue_control(0, move |core| {
            core.set_output_events_enabled(enabled);
            Ok(())
        });
    }

    fn set_trigger_rules(
        &mut self,
        rules: Option<Arc<oxideterm_terminal_triggers::CompiledTriggerSet>>,
    ) {
        self.enqueue_control(0, move |core| {
            core.set_trigger_rules(rules);
            Ok(())
        });
    }

    fn set_trzsz_policy(&mut self, policy: Option<TrzszTransferPolicy>) {
        self.enqueue_control(0, move |core| {
            core.set_trzsz_policy(policy);
            Ok(())
        });
    }

    fn feed_trzsz_terminal_output(&mut self, bytes: &[u8]) {
        let bytes = Zeroizing::new(bytes.to_vec());
        let cost = bytes.len();
        self.enqueue_control(cost, move |core| {
            core.feed_trzsz_terminal_output(&bytes);
            Ok(())
        });
    }

    fn interrupt_trzsz_transfer(&mut self) {
        self.enqueue_control(0, move |core| {
            core.interrupt_trzsz_transfer();
            Ok(())
        });
    }

    fn finish_trzsz_transfer(&mut self) {
        self.enqueue_control(0, move |core| {
            core.finish_trzsz_transfer();
            Ok(())
        });
    }

    fn interrupt_modem_transfer(&mut self) {
        self.enqueue_control(0, move |core| {
            core.interrupt_modem_transfer();
            Ok(())
        });
    }

    fn finish_modem_transfer(&mut self) {
        self.enqueue_control(0, move |core| {
            core.finish_modem_transfer();
            Ok(())
        });
    }

    fn mode(&self) -> TermMode {
        self.shared.status.lock().expect("SSH parser status").mode
    }

    fn begin_tmux_pane_selection(&mut self, col: usize, row: usize) -> Result<Option<bool>> {
        if self.shared.finished.load(Ordering::Acquire)
            || !self
                .shared
                .core
                .lock()
                .parser_state
                .tmux_display
                .can_select_pane(col, row)
        {
            return Ok(Some(false));
        }
        self.enqueue(0, move |core| {
            let selected = core.select_tmux_pane_at(col, row)?;
            core.parser_state
                .pending_events
                .push(TerminalEvent::TmuxPaneSelected { selected });
            Ok(())
        })?;
        Ok(None)
    }

    fn tmux_local_point(&self, col: usize, row: usize) -> (usize, usize) {
        self.shared.core.lock().tmux_local_point(col, row)
    }

    fn tmux_state(&self) -> Option<crate::TmuxUiState> {
        self.shared.core.lock().tmux_state()
    }

    fn tmux_action(&mut self, action: crate::TmuxAction) -> Result<bool> {
        let cost = match &action {
            crate::TmuxAction::RunCommand(text)
            | crate::TmuxAction::RenameSession { name: text, .. }
            | crate::TmuxAction::RenameWindow { name: text, .. } => text.len(),
            _ => 0,
        };
        if self.shared.core.lock().tmux_state().is_none() {
            return Ok(false);
        }
        let action = Zeroizing::new(action);
        self.enqueue(cost, move |core| {
            core.parser_state.tmux_action_ref(&action).map(|_| ())
        })?;
        Ok(true)
    }

    fn tmux_separator_at(&self, col: usize, row: usize) -> Option<crate::TmuxSeparator> {
        self.shared.core.lock().tmux_separator_at(col, row)
    }

    fn resize_tmux_separator(
        &mut self,
        separator: crate::TmuxSeparator,
        delta: i32,
    ) -> Result<bool> {
        if self.shared.core.lock().tmux_state().is_none() {
            return Ok(false);
        }
        self.enqueue(0, move |core| {
            core.resize_tmux_separator(separator, delta).map(|_| ())
        })?;
        Ok(true)
    }

    fn set_focused(&mut self, focused: bool) -> Result<()> {
        self.enqueue(0, move |core| core.set_focused(focused))
    }

    fn resize_with_cell_size(&mut self, resize: TerminalResize) -> Result<()> {
        self.enqueue(0, move |core| core.resize_with_cell_size(resize))
    }

    fn scroll_lines(&mut self, delta: i32) {
        self.shared.core.lock().scroll_lines(delta)
    }

    fn scroll_lines_snapshot_incremental(
        &mut self,
        delta: i32,
        previous: &TerminalSnapshot,
    ) -> TerminalSnapshot {
        self.shared
            .core
            .lock()
            .scroll_lines_snapshot_incremental(delta, previous)
    }

    fn page_up(&mut self) {
        self.shared.core.lock().page_up()
    }

    fn page_down(&mut self) {
        self.shared.core.lock().page_down()
    }

    fn scroll_to_top(&mut self) {
        self.shared.core.lock().scroll_to_top()
    }

    fn scroll_to_bottom(&mut self) {
        self.shared.core.lock().scroll_to_bottom()
    }

    fn scroll_to_display_offset(&mut self, offset: usize) {
        self.shared.core.lock().scroll_to_display_offset(offset)
    }

    fn search_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
        self.shared.core.lock().search_matches(query)
    }

    fn search_source(&self) -> Option<crate::TerminalSearchSource> {
        self.shared.core.lock().search_source()
    }

    fn set_selection(&self, selection: Option<crate::TerminalSelectionRange>) {
        self.enqueue_control(0, move |core| {
            core.set_selection(selection);
            Ok(())
        });
    }

    fn selection(&self) -> Option<crate::TerminalSelectionRange> {
        self.shared.core.lock().selection()
    }

    fn clear_buffer(&mut self) {
        self.enqueue_control(0, move |core| {
            core.clear_buffer();
            Ok(())
        });
    }

    fn command_output_text(&self, mark: &TerminalCommandMark) -> String {
        self.shared.core.lock().command_output_text(mark)
    }

    fn buffer_text(&self) -> String {
        self.shared.core.lock().buffer_text()
    }

    fn snapshot(&self) -> TerminalSnapshot {
        self.shared.core.lock().snapshot()
    }

    fn snapshot_incremental(&self, previous: &TerminalSnapshot) -> TerminalSnapshot {
        self.shared.core.lock().snapshot_incremental(previous)
    }

    fn snapshot_with_display_offset(&self, display_offset: usize, rows: usize) -> TerminalSnapshot {
        self.shared
            .core
            .lock()
            .snapshot_with_display_offset(display_offset, rows)
    }

    fn terminate_active_task(&mut self) -> Result<()> {
        self.enqueue(0, move |core| core.terminate_active_task())
    }

    fn kill_active_task(&mut self) -> Result<()> {
        self.enqueue(0, move |core| core.kill_active_task())
    }

    fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        self.shared
            .status
            .lock()
            .expect("SSH parser status")
            .connection
            .clone()
    }
    fn lifecycle(&self) -> TerminalLifecycle {
        if self.shared.cancelled.load(Ordering::Acquire) {
            TerminalLifecycle::Closed
        } else {
            self.shared
                .status
                .lock()
                .expect("SSH parser status")
                .lifecycle
                .clone()
        }
    }

    fn read_pending(&mut self) -> bool {
        self.read_pending_with_budget(TerminalDrainBudget::unlimited())
            .changed
    }

    fn read_pending_with_budget(&mut self, _budget: TerminalDrainBudget) -> TerminalDrainReport {
        reap_workers();
        let mut report =
            std::mem::take(&mut *self.shared.report.lock().expect("SSH parser report"));
        report.budget_exhausted = false;
        report
    }

    fn activity_receiver(&self) -> TerminalActivityReceiver {
        self.activity.clone()
    }

    fn take_events(&mut self) -> Vec<TerminalEvent> {
        if self.shared.cancelled.load(Ordering::Acquire) {
            return Vec::new();
        }
        let events = {
            let mut queue = self.shared.events.lock().expect("SSH parser events");
            let events = std::mem::take(&mut *queue);
            self.shared.event_bytes.store(0, Ordering::Release);
            events
        };
        self.shared.wake.notify();
        events
    }

    fn take_trzsz_transfer(&mut self) -> Option<TrzszTransfer> {
        self.shared
            .ready_trzsz
            .lock()
            .expect("SSH transfer handoff")
            .pop_front()
    }

    fn begin_modem_transfer(
        &mut self,
        request: TerminalModemTransferRequest,
    ) -> Result<Option<ModemTransfer>> {
        self.enqueue(0, move |core| {
            let event = match core.start_modem_transfer(request.clone()) {
                Some(transfer) => TerminalEvent::ModemTransferPrompt { request, transfer },
                None => TerminalEvent::ModemTransferStartFailed,
            };
            core.parser_state.pending_events.push(event);
            Ok(())
        })?;
        Ok(None)
    }

    fn try_render_snapshot(
        &self,
        previous: &TerminalSnapshot,
        allow_defer: bool,
    ) -> Option<(
        TerminalSnapshot,
        Option<crate::TerminalSelectionRange>,
        TermMode,
    )> {
        let core = if allow_defer {
            self.shared.core.try_lock_unfair()?
        } else {
            self.shared.core.lock()
        };
        // The parser and its graphics placements cannot advance while these
        // three parts of the render snapshot are captured together.
        Some((
            core.snapshot_incremental(previous),
            core.selection(),
            core.mode(),
        ))
    }

    fn shutdown(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
#[path = "ssh_worker_tests.rs"]
mod tests;
