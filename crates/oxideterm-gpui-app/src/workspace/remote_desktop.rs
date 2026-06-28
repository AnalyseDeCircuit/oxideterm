use std::{
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
};

use oxideterm_gpui_remote_desktop::{RemoteDesktopViewState, remote_desktop_surface};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
};
use oxideterm_remote_desktop::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopFakeBackend,
    RemoteDesktopHelperEvent, RemoteDesktopHelperRequest, RemoteDesktopProtocol,
    RemoteDesktopProviderManifest, RemoteDesktopSessionStatus, RemoteDesktopSize,
    builtin_preview_provider_registry, builtin_provider_registry, read_event_line,
    write_request_line,
};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};

use super::*;

const REMOTE_DESKTOP_INITIAL_WIDTH: u32 = 1280;
const REMOTE_DESKTOP_INITIAL_HEIGHT: u32 = 720;

#[derive(Debug)]
pub(super) enum RemoteDesktopWorkerDelivery {
    Event {
        tab_id: TabId,
        event: RemoteDesktopHelperEvent,
    },
    TransportFailed {
        tab_id: TabId,
        message: String,
    },
}

pub(super) struct RemoteDesktopSession {
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    state: RemoteDesktopViewState,
    request_tx: mpsc::Sender<RemoteDesktopHelperRequest>,
}

impl RemoteDesktopSession {
    fn new(
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        request_tx: mpsc::Sender<RemoteDesktopHelperRequest>,
    ) -> Self {
        let state = RemoteDesktopViewState::new(profile.label.clone(), profile.protocol)
            .with_read_only(profile.read_only);
        Self {
            profile,
            provider,
            state,
            request_tx,
        }
    }
}

impl WorkspaceApp {
    pub(super) fn open_remote_desktop_preview_tab(
        &mut self,
        protocol: RemoteDesktopProtocol,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let profile = preview_remote_desktop_profile(protocol);
        let provider = match builtin_preview_provider_registry()
            .ok()
            .and_then(|registry| registry.get_for_protocol(protocol).cloned())
        {
            Some(provider) => provider,
            None => {
                self.push_command_palette_toast(
                    self.i18n.t("remote_desktop.provider_missing"),
                    None,
                    TerminalNoticeVariant::Error,
                );
                return;
            }
        };
        let title = self.remote_desktop_preview_tab_title(protocol);

        self.open_remote_desktop_tab(profile, provider, title, window, cx);
    }

    pub(super) fn open_remote_desktop_connection_tab(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let provider = match builtin_provider_registry()
            .ok()
            .and_then(|registry| registry.get_for_protocol(profile.protocol).cloned())
        {
            Some(provider) => provider,
            None => {
                self.push_command_palette_toast(
                    self.i18n.t("remote_desktop.provider_missing"),
                    None,
                    TerminalNoticeVariant::Error,
                );
                return;
            }
        };
        let title = profile.label.clone();

        self.open_remote_desktop_tab(profile, provider, title, window, cx);
    }

    fn open_remote_desktop_tab(
        &mut self,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = self.alloc_tab_id();
        let request_tx =
            self.spawn_remote_desktop_worker(tab_id, profile.clone(), provider.clone());
        let session = RemoteDesktopSession::new(profile, provider, request_tx);

        self.remote_desktop_sessions.insert(tab_id, session);
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::RemoteDesktop,
            title,
            title_source: TabTitleSource::Static,
            root_pane: None,
            active_pane_id: None,
        });
        self.main_window_tabs.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(super) fn render_remote_desktop_surface(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.remote_desktop_sessions.get(&tab_id) else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("remote_desktop.session_missing"))
                .into_any_element();
        };

        div()
            .size_full()
            .relative()
            .child(remote_desktop_surface(&self.tokens, &session.state))
            .child(self.render_remote_desktop_toolbar(tab_id, cx))
            .into_any_element()
    }

    pub(super) fn poll_remote_desktop_worker_results(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        while let Ok(delivery) = self.remote_desktop_worker_rx.try_recv() {
            match delivery {
                RemoteDesktopWorkerDelivery::Event { tab_id, event } => {
                    if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
                        session.state.apply_event(event);
                        changed = true;
                    }
                }
                RemoteDesktopWorkerDelivery::TransportFailed { tab_id, message } => {
                    if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
                        session
                            .state
                            .apply_event(RemoteDesktopHelperEvent::ConnectionFailure { message });
                        changed = true;
                    }
                }
            }
        }

        if changed {
            cx.notify();
        }
    }

    pub(super) fn close_remote_desktop_tab(&mut self, tab_id: TabId) {
        if let Some(session) = self.remote_desktop_sessions.remove(&tab_id) {
            // The helper owns external resources. Always send a protocol-level
            // close before dropping the channel so real helpers can disconnect.
            let _ = session.request_tx.send(RemoteDesktopHelperRequest::Close);
        }
    }

    fn spawn_remote_desktop_worker(
        &self,
        tab_id: TabId,
        profile: RemoteDesktopConnectionProfile,
        provider: RemoteDesktopProviderManifest,
    ) -> mpsc::Sender<RemoteDesktopHelperRequest> {
        let (request_tx, request_rx) = mpsc::channel();
        let delivery_tx = self.remote_desktop_worker_tx.clone();
        thread::Builder::new()
            .name(format!("remote-desktop-{}", tab_id.0))
            .spawn(move || {
                run_remote_desktop_worker(tab_id, profile, provider, request_rx, delivery_tx);
            })
            .expect("failed to start remote desktop worker");
        request_tx
    }

    fn render_remote_desktop_toolbar(&self, tab_id: TabId, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.remote_desktop_sessions.get(&tab_id) else {
            return div().into_any_element();
        };
        let theme = self.tokens.ui;
        let status = session.state.snapshot().status;
        let reconnect_disabled = matches!(status, RemoteDesktopSessionStatus::Connecting);
        let label = format!(
            "{} · {}:{}",
            session.provider.name, session.profile.endpoint.host, session.profile.endpoint.port
        );

        div()
            .absolute()
            .top(px(14.0))
            .right(px(14.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.two))
            .py(px(self.tokens.spacing.one))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_panel << 8) | 0xdd))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x99))
            .child(
                div()
                    .max_w(px(360.0))
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("remote_desktop.reconnect"),
                None,
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Secondary,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: reconnect_disabled,
                    },
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(move |this, _event, _window, cx| {
                    this.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::Reconnect);
                    cx.notify();
                }),
            ))
            .child(self.workspace_toolbar_action_button(
                self.i18n.t("remote_desktop.disconnect"),
                None,
                ToolbarButtonOptions {
                    button: ButtonOptions {
                        variant: ButtonVariant::Destructive,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                    ..ToolbarButtonOptions::default()
                },
                cx.listener(move |this, _event, _window, cx| {
                    this.send_remote_desktop_request(tab_id, RemoteDesktopHelperRequest::Close);
                    cx.notify();
                }),
            ))
            .into_any_element()
    }

    fn send_remote_desktop_request(&mut self, tab_id: TabId, request: RemoteDesktopHelperRequest) {
        if let Some(session) = self.remote_desktop_sessions.get_mut(&tab_id) {
            if let RemoteDesktopHelperRequest::Resize { size } = request {
                session.state.mark_resize_requested(size);
            }
            let _ = session.request_tx.send(request);
        }
    }

    fn remote_desktop_preview_tab_title(&self, protocol: RemoteDesktopProtocol) -> String {
        match protocol {
            RemoteDesktopProtocol::Rdp => self.i18n.t("remote_desktop.rdp_preview_title"),
            RemoteDesktopProtocol::Vnc => self.i18n.t("remote_desktop.vnc_preview_title"),
        }
    }
}

fn preview_remote_desktop_profile(
    protocol: RemoteDesktopProtocol,
) -> RemoteDesktopConnectionProfile {
    let label = match protocol {
        RemoteDesktopProtocol::Rdp => "RDP Preview",
        RemoteDesktopProtocol::Vnc => "VNC Preview",
    };

    RemoteDesktopConnectionProfile {
        id: format!("preview-{}", protocol.provider_id()),
        label: label.to_string(),
        protocol,
        endpoint: RemoteDesktopEndpoint::for_protocol("preview.local", protocol),
        username: None,
        domain: None,
        credential_ref: None,
        read_only: false,
    }
}

fn run_remote_desktop_worker(
    tab_id: TabId,
    profile: RemoteDesktopConnectionProfile,
    provider: RemoteDesktopProviderManifest,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    if let Ok((mut child, mut stdin)) = spawn_remote_desktop_helper(&provider) {
        let stdout = child.stdout.take();
        let connect = connect_request(&profile);
        if let Err(error) = write_request_line(&mut stdin, &connect) {
            let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::TransportFailed {
                tab_id,
                message: error.to_string(),
            });
            return;
        }
        if let Some(stdout) = stdout {
            let reader_tx = delivery_tx.clone();
            thread::Builder::new()
                .name(format!("remote-desktop-reader-{}", tab_id.0))
                .spawn(move || read_remote_desktop_events(tab_id, stdout, reader_tx))
                .ok();
        }

        run_remote_desktop_writer(tab_id, &mut stdin, request_rx, delivery_tx.clone());
        let exit_code = child.wait().ok().and_then(|status| status.code());
        let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::Event {
            tab_id,
            event: RemoteDesktopHelperEvent::Terminated { exit_code },
        });
        return;
    }

    run_in_process_fake_remote_desktop(tab_id, profile, request_rx, delivery_tx);
}

fn spawn_remote_desktop_helper(
    provider: &RemoteDesktopProviderManifest,
) -> Result<(Child, ChildStdin), std::io::Error> {
    let mut command = Command::new(resolve_remote_desktop_helper_command(
        &provider.entry.command,
    ));
    command
        .args(&provider.entry.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(working_dir) = provider.entry.working_dir.as_ref() {
        command.current_dir(working_dir);
    }
    let mut child = command.spawn()?;
    let stdin = child.stdin.take().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "remote desktop helper stdin is unavailable",
        )
    })?;
    Ok((child, stdin))
}

fn resolve_remote_desktop_helper_command(command: &str) -> PathBuf {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 || command_path.is_absolute() {
        return command_path.to_path_buf();
    }

    for candidate in bundled_remote_desktop_helper_candidates(command) {
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(command)
}

fn bundled_remote_desktop_helper_candidates(command: &str) -> Vec<PathBuf> {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return Vec::new();
    };
    let helper_name = platform_helper_binary_name(command);
    let target = helper_target_label();
    let mut roots = vec![
        exe_dir.join("resources"),
        exe_dir.join("..").join("Resources"),
    ];

    // Development builds keep helper binaries next to the app under target/*.
    roots.push(exe_dir.clone());

    let mut candidates = Vec::new();
    for root in roots {
        candidates.push(root.join("helpers").join(target).join(&helper_name));
        candidates.push(root.join("helpers").join(&helper_name));
        candidates.push(root.join(&helper_name));
    }
    candidates
}

fn platform_helper_binary_name(command: &str) -> String {
    if cfg!(target_os = "windows") && !command.ends_with(".exe") {
        format!("{command}.exe")
    } else {
        command.to_string()
    }
}

fn helper_target_label() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "x86_64") => "macos_x64",
        ("macos", "aarch64") => "macos_arm64",
        ("windows", "x86_64") => "windows_x64",
        ("windows", "aarch64") => "windows_arm64",
        ("linux", "x86_64") => "linux_x64",
        ("linux", "aarch64") => "linux_arm64",
        _ => std::env::consts::ARCH,
    }
}

fn read_remote_desktop_events(
    tab_id: TabId,
    stdout: impl std::io::Read,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        match read_event_line(&mut reader) {
            Ok(Some(event)) => {
                let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::Event { tab_id, event });
            }
            Ok(None) => break,
            Err(error) => {
                let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::TransportFailed {
                    tab_id,
                    message: error.to_string(),
                });
                break;
            }
        }
    }
}

fn run_remote_desktop_writer(
    tab_id: TabId,
    stdin: &mut impl Write,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    for request in request_rx {
        let should_close = matches!(request, RemoteDesktopHelperRequest::Close);
        if let Err(error) = write_request_line(stdin, &request) {
            let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::TransportFailed {
                tab_id,
                message: error.to_string(),
            });
            return;
        }
        if should_close {
            return;
        }
    }
}

fn run_in_process_fake_remote_desktop(
    tab_id: TabId,
    profile: RemoteDesktopConnectionProfile,
    request_rx: mpsc::Receiver<RemoteDesktopHelperRequest>,
    delivery_tx: mpsc::Sender<RemoteDesktopWorkerDelivery>,
) {
    let mut backend = RemoteDesktopFakeBackend::new(profile.protocol);
    for event in backend.handle_request(connect_request(&profile)) {
        let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::Event { tab_id, event });
    }

    for request in request_rx {
        let should_close = matches!(request, RemoteDesktopHelperRequest::Close);
        for event in backend.handle_request(request) {
            let _ = delivery_tx.send(RemoteDesktopWorkerDelivery::Event { tab_id, event });
        }
        if should_close {
            break;
        }
    }
}

fn connect_request(profile: &RemoteDesktopConnectionProfile) -> RemoteDesktopHelperRequest {
    RemoteDesktopHelperRequest::Connect {
        protocol: profile.protocol,
        endpoint: profile.endpoint.clone(),
        username: profile.username.clone(),
        // Credentials are resolved by the eventual provider adapter. The
        // preview path deliberately never fabricates or logs secret material.
        password: None,
        domain: profile.domain.clone(),
        size: RemoteDesktopSize::clamped(
            REMOTE_DESKTOP_INITIAL_WIDTH,
            REMOTE_DESKTOP_INITIAL_HEIGHT,
        ),
        read_only: profile.read_only,
    }
}
