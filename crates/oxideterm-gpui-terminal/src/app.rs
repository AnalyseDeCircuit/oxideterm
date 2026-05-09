use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Result;
use gpui::{
    Bounds, ClipboardItem, Context, FocusHandle, PathPromptOptions, Pixels, SharedString,
    Subscription, Timer, Window, px,
};
use oxideterm_ssh::SshConnectionHandle;
use oxideterm_terminal::{
    GraphicsOptions, LocalPtyConfig, SshSessionConfig, TermMode, TerminalDrainBudget,
    TerminalEvent, TerminalLifecycle, TerminalSession, TerminalSnapshot, TrzszTransferDirection,
    TrzszTransferSelection,
};
use oxideterm_trzsz::TrzszState;
use parking_lot::Mutex;

use crate::background_cache::BackgroundImageRenderCache;
use crate::terminal_ui::*;
use crate::terminal_view::*;

mod image_cache;
mod ime;
mod interactions;
mod render;
mod scrollbar;

use crate::trzsz_worker::{
    TrzszPromptRequest, TrzszPromptSelection, TrzszWorkerEvent, TrzszWorkerJob,
    run_trzsz_worker_job,
};
use image_cache::ImageRenderCache;
pub(crate) use image_cache::TerminalRenderedImage;
pub(crate) use ime::TerminalInputHandler;
use scrollbar::{ScrollbarDrag, ScrollbarGeometry};

pub type SharedTerminalSession = Arc<Mutex<TerminalSession>>;

pub struct TerminalPane {
    terminal: Arc<Mutex<TerminalSession>>,
    focus_handle: FocusHandle,
    preferences: TerminalUiPreferences,
    settings: TerminalUiSettings,
    theme: TerminalUiTheme,
    snapshot: TerminalSnapshot,
    metrics: TerminalMetrics,
    selection: Option<TerminalSelection>,
    pending_paste: Option<String>,
    marked_text: Option<String>,
    search_query: Option<String>,
    selected_search_match: Option<usize>,
    hovered_link: Option<TerminalLinkRange>,
    selecting: bool,
    last_mouse_report_point: Option<TerminalPoint>,
    title: SharedString,
    bell_flash: bool,
    terminal_exited: bool,
    scroll_px: Pixels,
    scrollbar_drag: Option<ScrollbarDrag>,
    copy_on_select_generation: u64,
    focused: bool,
    cursor_visible: bool,
    cursor_blink_terminal_enabled: bool,
    last_cursor_blink: Instant,
    last_terminal_input: Instant,
    last_drain_budget_exhausted: bool,
    image_cache: ImageRenderCache,
    background_image_cache: BackgroundImageRenderCache,
    bounds: Option<Bounds<Pixels>>,
    last_pty_resize: Option<(usize, usize, u16, u16)>,
    pending_pty_resize: Option<(usize, usize, u16, u16)>,
    pty_resize_generation: u64,
    trzsz_state: Arc<TrzszState>,
    trzsz_owner_id: String,
    trzsz_prompt_active: bool,
    trzsz_connection_lost: bool,
    _subscriptions: Vec<Subscription>,
}

const PTY_RESIZE_DEBOUNCE: Duration = Duration::from_millis(100);
static NEXT_TRZSZ_OWNER_ID: AtomicU64 = AtomicU64::new(1);

impl TerminalPane {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Result<Self> {
        Self::new_with_preferences(TerminalUiPreferences::default(), window, cx)
    }

    pub fn new_with_preferences(
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let terminal = Arc::new(Mutex::new(
            TerminalSession::local_with_graphics_and_encoding(
                DEFAULT_COLS,
                DEFAULT_ROWS,
                graphics_options_from_preferences(&preferences),
                preferences.terminal_encoding,
                preferences.scrollback_lines,
            )?,
        ));
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn new_local_with_config_and_preferences(
        config: LocalPtyConfig,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let terminal = Arc::new(Mutex::new(
            TerminalSession::local_with_config_graphics_and_encoding(
                DEFAULT_COLS,
                DEFAULT_ROWS,
                config,
                graphics_options_from_preferences(&preferences),
                preferences.terminal_encoding,
                preferences.scrollback_lines,
            )?,
        ));
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn new_ssh(
        config: SshSessionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        Self::new_ssh_with_preferences(config, TerminalUiPreferences::default(), window, cx)
    }

    pub fn new_ssh_with_preferences(
        config: SshSessionConfig,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let terminal = Self::ssh_shared_session(config, &preferences);
        Self::from_session(terminal, preferences, window, cx)
    }

    pub fn ssh_shared_session(
        config: SshSessionConfig,
        preferences: &TerminalUiPreferences,
    ) -> SharedTerminalSession {
        Arc::new(Mutex::new(TerminalSession::ssh_with_graphics_and_encoding(
            config,
            DEFAULT_COLS,
            DEFAULT_ROWS,
            graphics_options_from_preferences(preferences),
            preferences.terminal_encoding,
            preferences.scrollback_lines,
        )))
    }

    pub fn from_shared_session(
        terminal: SharedTerminalSession,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        Self::from_session(terminal, preferences, window, cx)
    }

    fn from_session(
        terminal: SharedTerminalSession,
        preferences: TerminalUiPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let snapshot = terminal.lock().snapshot();
        let focus_handle = cx.focus_handle();
        let metrics = TerminalMetrics::measure_with_preferences(window, &preferences);
        window.focus(&focus_handle);
        terminal.lock().set_focused(true)?;
        let trzsz_owner_id = format!(
            "gpui-terminal-{}",
            NEXT_TRZSZ_OWNER_ID.fetch_add(1, Ordering::Relaxed)
        );

        let focus_in = cx.on_focus_in(&focus_handle, window, |this, _window, cx| {
            this.handle_focus_change(true, cx);
        });
        let focus_out = cx.on_focus_out(&focus_handle, window, |this, _event, _window, cx| {
            this.handle_focus_change(false, cx);
        });

        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_millis(16)).await;
                if weak
                    .update(cx, |this, cx| {
                        this.tick(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        Ok(Self {
            terminal,
            focus_handle,
            preferences: preferences.clone(),
            settings: TerminalUiSettings::from_preferences(&preferences),
            theme: preferences.theme.clone(),
            snapshot,
            metrics,
            selection: None,
            pending_paste: None,
            marked_text: None,
            search_query: None,
            selected_search_match: None,
            hovered_link: None,
            selecting: false,
            last_mouse_report_point: None,
            title: SharedString::from("OxideTerm"),
            bell_flash: false,
            terminal_exited: false,
            scroll_px: px(0.0),
            scrollbar_drag: None,
            copy_on_select_generation: 0,
            focused: true,
            cursor_visible: true,
            cursor_blink_terminal_enabled: false,
            last_cursor_blink: Instant::now(),
            last_terminal_input: Instant::now(),
            last_drain_budget_exhausted: false,
            image_cache: {
                let mut cache = ImageRenderCache::default();
                cache.set_byte_limit(preferences.render_policy.image_cache_bytes);
                cache
            },
            background_image_cache: {
                let mut cache = BackgroundImageRenderCache::default();
                cache.set_byte_limit(preferences.render_policy.image_cache_bytes);
                cache
            },
            bounds: None,
            last_pty_resize: None,
            pending_pty_resize: None,
            pty_resize_generation: 0,
            trzsz_state: TrzszState::new(),
            trzsz_owner_id,
            trzsz_prompt_active: false,
            trzsz_connection_lost: false,
            _subscriptions: vec![focus_in, focus_out],
        })
    }

    pub fn title(&self) -> SharedString {
        self.title.clone()
    }

    pub fn set_preferences(&mut self, preferences: TerminalUiPreferences, cx: &mut Context<Self>) {
        if self.preferences.terminal_encoding != preferences.terminal_encoding {
            self.terminal
                .lock()
                .set_encoding(preferences.terminal_encoding);
        }
        if self.preferences.trzsz_policy != preferences.trzsz_policy {
            self.terminal
                .lock()
                .set_trzsz_policy(preferences.trzsz_policy.clone());
        }
        self.settings = TerminalUiSettings::from_preferences(&preferences);
        self.theme = preferences.theme.clone();
        self.image_cache
            .set_byte_limit(preferences.render_policy.image_cache_bytes);
        self.background_image_cache
            .set_byte_limit(preferences.render_policy.image_cache_bytes);
        self.preferences = preferences;
        self.last_pty_resize = None;
        self.pending_pty_resize = None;
        self.reset_cursor_blink();
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus_handle);
    }

    pub fn shutdown(&mut self) {
        self.terminal.lock().shutdown();
    }

    pub fn lifecycle(&self) -> TerminalLifecycle {
        self.terminal.lock().lifecycle()
    }

    pub fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        self.terminal.lock().ssh_connection_handle()
    }

    pub fn set_search_query(
        &mut self,
        query: Option<String>,
        selected_match: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.search_query = query;
        self.selected_search_match = selected_match;
        if self.search_query.is_some() {
            self.scroll_to_selected_search_match(cx);
        }
        cx.notify();
    }

    pub fn select_next_search_result(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.select_next_search_match(forward, cx);
    }

    pub fn copy_to_clipboard(&mut self, cx: &mut Context<Self>) {
        self.copy_from_platform_shortcut(cx);
    }

    pub fn has_selection(&self) -> bool {
        self.selection
            .is_some_and(|selection| !selection.is_empty())
    }

    pub fn paste_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.terminal_accepts_input() && self.terminal.lock().paste_text(text).is_ok() {
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
        }
    }

    pub fn send_command_line(&mut self, command: &str, cx: &mut Context<Self>) {
        if command.trim().is_empty() {
            return;
        }
        let mut input = command.to_string();
        input.push('\r');
        self.send_text(&input, cx);
    }

    pub fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        if !self.terminal_accepts_input() {
            return;
        }
        if self.settings.paste_protection && paste_needs_confirmation(&text) {
            self.pending_paste = Some(text);
            cx.notify();
            return;
        }
        self.paste_text(&text, cx);
    }

    pub(crate) fn confirm_pending_paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.pending_paste.take() else {
            return;
        };
        self.paste_text(&text, cx);
        cx.notify();
    }

    pub(crate) fn cancel_pending_paste(&mut self, cx: &mut Context<Self>) {
        if self.pending_paste.take().is_some() {
            cx.notify();
        }
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let budget = self.next_drain_budget();
        let (report, events) = {
            let mut terminal = self.terminal.lock();
            terminal.refresh_process_info();
            let report = terminal.read_pending_with_budget(budget);
            (report, terminal.take_events())
        };
        self.last_drain_budget_exhausted = report.budget_exhausted;

        for event in events {
            self.handle_terminal_event(event, cx);
        }

        if report.changed {
            self.snapshot = self.terminal.lock().snapshot();
            cx.notify();
        }

        self.update_cursor_blink(cx);
    }

    fn next_drain_budget(&self) -> TerminalDrainBudget {
        let drain = self.preferences.render_policy.drain;
        if self.last_drain_budget_exhausted {
            TerminalDrainBudget::new(drain.throughput_bytes, drain.max_events)
        } else if self.last_terminal_input.elapsed() <= Duration::from_millis(220) {
            TerminalDrainBudget::new(drain.interactive_bytes, drain.max_events)
        } else {
            TerminalDrainBudget::new(drain.normal_bytes, drain.max_events)
        }
    }

    fn handle_terminal_event(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        match event {
            TerminalEvent::TitleChanged(title) => {
                self.title = title.into();
                cx.notify();
            }
            TerminalEvent::TitleReset => {
                self.title = SharedString::from("OxideTerm");
                cx.notify();
            }
            TerminalEvent::Bell => {
                self.bell_flash = true;
                cx.notify();
                cx.spawn(async move |weak, cx| {
                    Timer::after(Duration::from_millis(180)).await;
                    let _ = weak.update(cx, |this, cx| {
                        this.bell_flash = false;
                        cx.notify();
                    });
                })
                .detach();
            }
            TerminalEvent::Wakeup => {
                cx.notify();
            }
            TerminalEvent::BlinkChanged(blinking) => {
                self.cursor_blink_terminal_enabled = blinking;
                self.reset_cursor_blink();
                cx.notify();
            }
            TerminalEvent::ChildExited(code) => {
                self.notify_trzsz_connection_lost_if_active();
                self.terminal_exited = true;
                self.title = match code {
                    Some(code) => format!("Process exited ({code})").into(),
                    None => "Process exited".into(),
                };
                cx.notify();
            }
            TerminalEvent::MagicDetected(kind) => {
                let _ = kind;
            }
            TerminalEvent::TrzszTransferPrompt {
                direction,
                selection,
                remote_is_windows,
            } => {
                self.handle_trzsz_transfer_prompt(
                    TrzszPromptRequest {
                        direction,
                        selection,
                        remote_is_windows,
                    },
                    cx,
                );
            }
            TerminalEvent::EncodingHint(hint) => {
                let _ = hint;
            }
            TerminalEvent::ClipboardStore(text) => {
                if self.settings.osc52_clipboard {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            TerminalEvent::ClipboardLoad(formatter) => {
                if self.settings.osc52_clipboard
                    && let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
                {
                    let response = formatter(&text);
                    self.send_protocol_bytes(response.as_bytes(), cx);
                }
            }
        }
    }

    fn handle_trzsz_transfer_prompt(
        &mut self,
        request: TrzszPromptRequest,
        cx: &mut Context<Self>,
    ) {
        if self.trzsz_prompt_active {
            return;
        }
        // Match Tauri's controller boundary: once the magic key is accepted,
        // the protocol owner moves to a transfer worker while PTY output keeps
        // flowing into the same buffer through the terminal-side input handle.
        let Some(transfer) = self.terminal.lock().take_trzsz_transfer() else {
            return;
        };

        self.trzsz_prompt_active = true;
        self.trzsz_connection_lost = false;
        self.emit_trzsz_prompt_notice(&request);
        let receiver = match request.direction {
            TrzszTransferDirection::Upload => {
                let directory = request.selection == TrzszTransferSelection::Directory;
                cx.prompt_for_paths(PathPromptOptions {
                    files: !directory,
                    directories: directory,
                    multiple: true,
                    prompt: Some(SharedString::from(if directory {
                        self.preferences
                            .trzsz_labels
                            .select_upload_directory_title
                            .clone()
                    } else {
                        self.preferences
                            .trzsz_labels
                            .select_upload_files_title
                            .clone()
                    })),
                })
            }
            TrzszTransferDirection::Download => cx.prompt_for_paths(PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some(SharedString::from(
                    self.preferences
                        .trzsz_labels
                        .select_download_directory_title
                        .clone(),
                )),
            }),
        };

        let state = self.trzsz_state.clone();
        let owner_id = self.trzsz_owner_id.clone();
        let policy = self.preferences.trzsz_policy.clone().unwrap_or_default();
        let terminal_columns = self.snapshot.cols;
        cx.spawn(async move |weak, cx| {
            let selection = match receiver.await {
                Ok(Ok(Some(paths))) => match request.direction {
                    TrzszTransferDirection::Upload => TrzszPromptSelection::Upload(
                        paths
                            .into_iter()
                            .map(|path| path.to_string_lossy().to_string())
                            .collect(),
                    ),
                    TrzszTransferDirection::Download => paths
                        .into_iter()
                        .next()
                        .map(|path| {
                            TrzszPromptSelection::DownloadRoot(path.to_string_lossy().to_string())
                        })
                        .unwrap_or(TrzszPromptSelection::Cancelled),
                },
                _ => TrzszPromptSelection::Cancelled,
            };
            let (result_tx, result_rx) = std::sync::mpsc::channel();
            let (event_tx, event_rx) = std::sync::mpsc::channel();
            // The worker blocks on trzsz protocol reads, so it must never run
            // while holding the terminal session lock. The terminal tick keeps
            // draining PTY output and flushing worker writes back to SSH.
            std::thread::spawn(move || {
                let result = run_trzsz_worker_job(TrzszWorkerJob {
                    transfer,
                    request,
                    selection,
                    owner_id,
                    state,
                    policy,
                    event_tx,
                    terminal_columns,
                })
                .map_err(|error| error.to_string());
                let _ = result_tx.send(result);
            });

            loop {
                while let Ok(event) = event_rx.try_recv() {
                    let _ = weak.update(cx, |this, cx| {
                        this.handle_trzsz_worker_event(event, cx);
                    });
                }
                match result_rx.try_recv() {
                    Ok(result) => {
                        let _ = weak.update(cx, |this, cx| {
                            while let Ok(event) = event_rx.try_recv() {
                                this.handle_trzsz_worker_event(event, cx);
                            }
                            if !this.trzsz_connection_lost {
                                this.terminal.lock().finish_trzsz_transfer();
                            }
                            this.trzsz_prompt_active = false;
                            this.trzsz_connection_lost = false;
                            let _ = result;
                            cx.notify();
                        });
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        let _ = weak.update(cx, |this, cx| {
                            while let Ok(event) = event_rx.try_recv() {
                                this.handle_trzsz_worker_event(event, cx);
                            }
                            if !this.trzsz_connection_lost {
                                this.terminal.lock().finish_trzsz_transfer();
                            }
                            this.trzsz_prompt_active = false;
                            this.trzsz_connection_lost = false;
                            cx.notify();
                        });
                        break;
                    }
                }
            }
        })
        .detach();
    }

    fn handle_trzsz_worker_event(&mut self, event: TrzszWorkerEvent, cx: &mut Context<Self>) {
        match event {
            TrzszWorkerEvent::TerminalOutput(bytes) => {
                // Tauri writes TextProgressBar VT output into the local terminal
                // renderer. Sending it back to the remote PTY would corrupt the
                // trzsz protocol stream, so native has a dedicated local feed.
                self.terminal.lock().feed_trzsz_terminal_output(&bytes);
                self.snapshot = self.terminal.lock().snapshot();
                cx.notify();
            }
            TrzszWorkerEvent::Completed => {
                if self.trzsz_connection_lost {
                    return;
                }
                self.emit_trzsz_notice(
                    self.preferences.trzsz_labels.completed_title.clone(),
                    Some(self.preferences.trzsz_labels.completed_description.clone()),
                    TerminalNoticeVariant::Success,
                );
            }
            TrzszWorkerEvent::Cancelled => {
                if self.trzsz_connection_lost {
                    return;
                }
                self.emit_trzsz_notice(
                    self.preferences.trzsz_labels.cancelled_title.clone(),
                    Some(self.preferences.trzsz_labels.cancelled_description.clone()),
                    TerminalNoticeVariant::Warning,
                );
            }
            TrzszWorkerEvent::PartialCleanup => {
                self.emit_trzsz_notice(
                    self.preferences.trzsz_labels.partial_cleanup_title.clone(),
                    Some(
                        self.preferences
                            .trzsz_labels
                            .partial_cleanup_description
                            .clone(),
                    ),
                    TerminalNoticeVariant::Warning,
                );
            }
            TrzszWorkerEvent::Failed {
                code,
                detail,
                message,
                ..
            } => {
                if self.trzsz_connection_lost {
                    return;
                }
                let (title, description, variant) =
                    self.trzsz_failure_notice(&code, detail.as_deref(), &message);
                self.emit_trzsz_notice(title, Some(description), variant);
            }
        }
    }

    fn notify_trzsz_connection_lost_if_active(&mut self) {
        if !self.trzsz_prompt_active || self.trzsz_connection_lost {
            return;
        }

        self.trzsz_connection_lost = true;
        // Mirrors TerminalView.disposeTrzszController({ notifyConnectionLost: true }):
        // emit one connection-lost toast, then stop the protocol buffer so the
        // transfer worker is unblocked instead of waiting for more PTY data.
        self.terminal.lock().interrupt_trzsz_transfer();
        self.emit_trzsz_notice(
            self.preferences.trzsz_labels.connection_lost_title.clone(),
            Some(
                self.preferences
                    .trzsz_labels
                    .connection_lost_description
                    .clone(),
            ),
            TerminalNoticeVariant::Warning,
        );
    }

    fn emit_trzsz_prompt_notice(&self, request: &TrzszPromptRequest) {
        let labels = &self.preferences.trzsz_labels;
        let (title, description) = match request.direction {
            TrzszTransferDirection::Upload
                if request.selection == TrzszTransferSelection::Directory =>
            {
                (
                    labels.select_upload_directory_title.clone(),
                    labels.select_upload_directory_description.clone(),
                )
            }
            TrzszTransferDirection::Upload => (
                labels.select_upload_files_title.clone(),
                labels.select_upload_files_description.clone(),
            ),
            TrzszTransferDirection::Download => (
                labels.select_download_directory_title.clone(),
                labels.select_download_directory_description.clone(),
            ),
        };
        self.emit_trzsz_notice(title, Some(description), TerminalNoticeVariant::Default);
    }

    fn emit_trzsz_notice(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
    ) {
        if let Some(sink) = &self.preferences.notice_sink {
            sink(TerminalNotice {
                title,
                description,
                status_text: None,
                progress: None,
                variant,
            });
        }
    }

    fn trzsz_failure_notice(
        &self,
        code: &str,
        detail: Option<&str>,
        fallback: &str,
    ) -> (String, String, TerminalNoticeVariant) {
        let labels = &self.preferences.trzsz_labels;
        match code {
            "invalid_api_version" | "root_mismatch" | "root_not_prepared" => (
                labels.version_mismatch_title.clone(),
                labels.version_mismatch_description.clone(),
                TerminalNoticeVariant::Error,
            ),
            "invalid_path" | "unauthorized_path" | "reserved_name" => (
                labels.path_invalid_title.clone(),
                labels.path_invalid_description.clone(),
                TerminalNoticeVariant::Error,
            ),
            "symlink_not_allowed" => (
                labels.symlink_not_supported_title.clone(),
                labels.symlink_not_supported_description.clone(),
                TerminalNoticeVariant::Error,
            ),
            "already_exists" => (
                labels.conflict_detected_title.clone(),
                labels.conflict_detected_description.clone(),
                TerminalNoticeVariant::Warning,
            ),
            "directory_not_allowed" => (
                labels.directory_not_allowed_title.clone(),
                labels.directory_not_allowed_description.clone(),
                TerminalNoticeVariant::Warning,
            ),
            "max_file_count_exceeded" => (
                labels.max_file_count_title.clone(),
                format_count_limit_message(&labels.max_file_count_description, detail),
                TerminalNoticeVariant::Warning,
            ),
            "max_total_bytes_exceeded" => (
                labels.max_total_bytes_title.clone(),
                format_byte_limit_message(&labels.max_total_bytes_description, detail),
                TerminalNoticeVariant::Warning,
            ),
            _ => (
                labels.failed_title.clone(),
                if fallback.is_empty() {
                    labels.failed_description.clone()
                } else {
                    fallback.to_string()
                },
                TerminalNoticeVariant::Error,
            ),
        }
    }

    fn handle_focus_change(&mut self, focused: bool, cx: &mut Context<Self>) {
        self.focused = focused;
        let _ = self.terminal.lock().set_focused(focused);
        self.reset_cursor_blink();
        cx.notify();
    }

    fn send_protocol_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if !self.terminal_accepts_input() {
            return;
        }

        if self.terminal.lock().write_protocol_bytes(bytes).is_ok() {
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
        }
    }

    fn send_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.terminal_accepts_input() {
            return;
        }

        if self.terminal.lock().write_text(text).is_ok() {
            self.last_terminal_input = Instant::now();
            self.reset_cursor_blink();
            cx.notify();
        }
    }

    fn terminal_accepts_input(&self) -> bool {
        !self.terminal_exited && self.terminal.lock().is_interactive()
    }

    fn commit_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.marked_text = None;
        self.send_text(text, cx);
    }

    fn set_marked_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.marked_text = (!text.is_empty()).then(|| text.to_string());
        cx.notify();
    }

    fn clear_marked_text(&mut self, cx: &mut Context<Self>) {
        if self.marked_text.take().is_some() {
            cx.notify();
        }
    }

    fn marked_text_range(&self) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn should_blink_cursor(&self) -> bool {
        let alt_screen = self.terminal.lock().mode().contains(TermMode::ALT_SCREEN);
        should_blink_cursor_for_mode(
            self.settings.blink_mode,
            self.focused,
            self.cursor_blink_terminal_enabled,
            alt_screen,
            self.preferences.cursor_shape,
        )
    }

    fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.last_cursor_blink = Instant::now();
    }

    fn update_cursor_blink(&mut self, cx: &mut Context<Self>) {
        if !self.should_blink_cursor() {
            if !self.cursor_visible {
                self.cursor_visible = true;
                cx.notify();
            }
            self.last_cursor_blink = Instant::now();
            return;
        }

        if self.last_cursor_blink.elapsed() >= CURSOR_BLINK_INTERVAL {
            self.cursor_visible = !self.cursor_visible;
            self.last_cursor_blink = Instant::now();
            cx.notify();
        }
    }

    pub fn apply_viewport_bounds(
        &mut self,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        cx: &mut Context<Self>,
    ) {
        self.bounds = Some(bounds);
        let cell_width = self.metrics.cell_width_f32();
        let line_height = self.metrics.line_height_f32();
        let width =
            (f32::from(bounds.size.width) - TERMINAL_CONTENT_PADDING * 2.0).max(cell_width * 2.0);
        let height =
            (f32::from(bounds.size.height) - TERMINAL_CONTENT_PADDING * 2.0).max(line_height * 2.0);
        let cols = whole_cells_in_span(width, cell_width).max(2);
        let rows = whole_cells_in_span(height, line_height).max(2);
        let cell_width_px = (cell_width * scale_factor).ceil().max(1.0) as u16;
        let cell_height_px = (line_height * scale_factor).ceil().max(1.0) as u16;
        let resize = (cols, rows, cell_width_px, cell_height_px);

        if self.last_pty_resize == Some(resize) || self.pending_pty_resize == Some(resize) {
            return;
        }

        self.pending_pty_resize = Some(resize);
        self.pty_resize_generation = self.pty_resize_generation.wrapping_add(1);
        let generation = self.pty_resize_generation;
        cx.spawn(async move |weak, cx| {
            Timer::after(PTY_RESIZE_DEBOUNCE).await;
            let _ = weak.update(cx, |view, cx| {
                view.flush_pending_pty_resize(generation, cx);
            });
        })
        .detach();
    }

    fn flush_pending_pty_resize(&mut self, generation: u64, cx: &mut Context<Self>) {
        if generation != self.pty_resize_generation {
            return;
        }
        let Some((cols, rows, cell_width_px, cell_height_px)) = self.pending_pty_resize.take()
        else {
            return;
        };
        let resize = (cols, rows, cell_width_px, cell_height_px);
        if self.last_pty_resize == Some(resize) {
            return;
        }

        let resized = {
            let mut terminal = self.terminal.lock();
            terminal
                .resize_with_cell_size(cols, rows, cell_width_px, cell_height_px)
                .is_ok()
        };
        if resized {
            self.last_pty_resize = Some(resize);
            self.snapshot = self.terminal.lock().snapshot();
            cx.notify();
        }
    }

    fn content_origin(&self) -> gpui::Point<Pixels> {
        self.bounds
            .map(|bounds| bounds.origin)
            .unwrap_or_else(|| gpui::point(px(0.0), px(0.0)))
    }
}

pub fn paste_needs_confirmation(text: &str) -> bool {
    const PASTE_LINE_THRESHOLD: usize = 1;
    const PASTE_CHAR_THRESHOLD: usize = 50;

    text.contains('\n')
        && (text.split('\n').count() > PASTE_LINE_THRESHOLD || text.len() > PASTE_CHAR_THRESHOLD)
}

fn graphics_options_from_preferences(preferences: &TerminalUiPreferences) -> GraphicsOptions {
    let graphics = preferences.render_policy.terminal_graphics;
    let storage_limit_mb = graphics.storage_limit_bytes.div_ceil(1024 * 1024);
    GraphicsOptions {
        enabled: true,
        sixel: true,
        iterm2_inline: true,
        kitty: true,
        pixel_limit: graphics.pixel_limit.min(u32::MAX as usize) as u32,
        storage_limit_mb: storage_limit_mb.min(u32::MAX as usize) as u32,
        show_placeholder: graphics.show_placeholders,
    }
}

fn format_count_limit_message(template: &str, detail: Option<&str>) -> String {
    let selected = detail_value(detail, "selected").unwrap_or_else(|| "0".to_string());
    let max = detail_value(detail, "max").unwrap_or_else(|| "0".to_string());
    template
        .replace("{{selected}}", &selected)
        .replace("{{max}}", &max)
}

fn format_byte_limit_message(template: &str, detail: Option<&str>) -> String {
    let selected = detail_value(detail, "selected")
        .and_then(|value| value.parse::<u64>().ok())
        .map(format_binary_size)
        .unwrap_or_else(|| "0 B".to_string());
    let max = detail_value(detail, "max")
        .and_then(|value| value.parse::<u64>().ok())
        .map(format_binary_size)
        .unwrap_or_else(|| "0 B".to_string());
    template
        .replace("{{selected}}", &selected)
        .replace("{{max}}", &max)
}

fn detail_value(detail: Option<&str>, key: &str) -> Option<String> {
    detail?
        .split(',')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| (name == key).then(|| value.trim().to_string()))
}

fn format_binary_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn whole_cells_in_span(span: f32, cell_span: f32) -> usize {
    let cells = span / cell_span;
    let nearest_integer = cells.round();
    if (cells - nearest_integer).abs() <= 0.0001 {
        nearest_integer.max(0.0) as usize
    } else {
        cells.floor().max(0.0) as usize
    }
}
