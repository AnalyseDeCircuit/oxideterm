use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
};

use gpui::RenderImage;
use oxideterm_gpui_ui::{
    TextInputView,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions, button_with,
    },
    text_input_anchor_probe,
};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};
use oxideterm_wsl_graphics::{
    GraphicsSessionMode, WSL_GRAPHICS_UNAVAILABLE, WslDistro, WslGraphicsError, WslGraphicsSession,
    WslgStatus, wsl,
};

use super::graphics_vnc::{
    GraphicsVncFrame, GraphicsVncInput, GraphicsVncWorkerEvent, SharedGraphicsVncGeometry,
    graphics_vnc_canvas, graphics_vnc_keysyms, run_graphics_vnc_worker, vnc_button_mask,
    vnc_scroll_masks,
};
use super::ime::WorkspaceImeTarget;
use super::*;

const GRAPHICS_SELECTOR_MAX_W: f32 = 384.0; // Tauri max-w-sm.
const GRAPHICS_SELECTOR_PADDING_X: f32 = 24.0; // Tauri px-6.
const GRAPHICS_SELECTOR_GAP: f32 = 16.0; // Tauri gap-4.
const GRAPHICS_WARNING_PADDING_X: f32 = 12.0; // Tauri px-3.
const GRAPHICS_WARNING_PADDING_Y: f32 = 10.0; // Tauri py-2.5.
const GRAPHICS_DISTRO_ROW_PADDING_X: f32 = 16.0; // Tauri px-4.
const GRAPHICS_DISTRO_ROW_PADDING_Y: f32 = 12.0; // Tauri py-3.
const GRAPHICS_DISTRO_ROW_GAP: f32 = 12.0; // Tauri gap-3.
const GRAPHICS_BADGE_TEXT_SIZE: f32 = 11.0; // Tauri text-xs.
const GRAPHICS_DOT_SIZE: f32 = 6.0; // Tauri w-1.5 h-1.5.
const GRAPHICS_INPUT_H: f32 = 36.0; // Tauri shadcn Input default h-9.
const GRAPHICS_TOOLBAR_TOP: f32 = 16.0; // Tauri top-4.
const GRAPHICS_TOOLBAR_PADDING_X: f32 = 12.0; // Tauri px-3.
const GRAPHICS_TOOLBAR_PADDING_Y: f32 = 8.0; // Tauri py-2.
const GRAPHICS_TOOLBAR_GAP: f32 = 8.0; // Tauri gap-2.
const GRAPHICS_STATUS_OVERLAY_BOTTOM: f32 = 16.0; // Tauri bottom-4.
const GRAPHICS_STATUS_OVERLAY_PADDING_X: f32 = 16.0; // Tauri px-4.
const GRAPHICS_STATUS_OVERLAY_PADDING_Y: f32 = 12.0; // Tauri py-3.
const GRAPHICS_COMMON_APP_COL_GAP: f32 = 8.0; // Tauri gap-2.
const GRAPHICS_AMBER_500: u32 = 0xf59e0b; // Tauri amber-500.
const GRAPHICS_GREEN_500: u32 = 0x22c55e; // Tauri green-500.
const GRAPHICS_RED_400: u32 = 0xf87171; // Tauri red-400.
const GRAPHICS_RED_500: u32 = 0xef4444; // Tauri destructive.
const GRAPHICS_ALPHA_10: u32 = 0x1a; // Tailwind /10.
const GRAPHICS_ALPHA_20: u32 = 0x33; // Tailwind /20.
const GRAPHICS_ALPHA_50: u32 = 0x80; // Tailwind /50.
const GRAPHICS_ALPHA_90: u32 = 0xe6; // Tailwind /90.

const COMMON_APPS: &[(&str, &str)] = &[
    ("gedit", "gedit"),
    ("Firefox", "firefox"),
    ("Nautilus", "nautilus"),
    ("VS Code", "code"),
    ("xterm", "xterm"),
    ("GIMP", "gimp"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum GraphicsInput {
    AppCommand,
}

impl GraphicsInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::AppCommand => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GraphicsStatus {
    Idle,
    Starting,
    Active,
    Disconnected,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GraphicsLaunchMode {
    Desktop,
    App,
}

#[derive(Clone, Debug)]
pub(super) enum GraphicsWorkerResult {
    ListSessions {
        result: Result<Vec<WslGraphicsSession>, String>,
    },
    LoadDistros {
        generation: u64,
        result: Result<Vec<WslDistro>, String>,
    },
    DetectWslg {
        generation: u64,
        distro: String,
        result: Result<WslgStatus, String>,
    },
    Start {
        generation: u64,
        result: Result<WslGraphicsSession, String>,
    },
    StartApp {
        generation: u64,
        result: Result<WslGraphicsSession, String>,
    },
    Stop {
        generation: u64,
        session_id: String,
        result: Result<(), String>,
    },
    Reconnect {
        generation: u64,
        result: Result<WslGraphicsSession, String>,
    },
    VncEvent(GraphicsVncWorkerEvent),
}

pub(super) struct GraphicsState {
    distros: Vec<WslDistro>,
    sessions: Vec<WslGraphicsSession>,
    wslg_statuses: HashMap<String, WslgStatus>,
    selected_distro: Option<String>,
    launch_mode: GraphicsLaunchMode,
    app_command: String,
    status: GraphicsStatus,
    error: Option<String>,
    loading: bool,
    generation: u64,
    pub(super) focused_input: Option<GraphicsInput>,
    session: Option<WslGraphicsSession>,
    vnc_session_id: Option<String>,
    vnc_input: Option<tokio::sync::mpsc::UnboundedSender<GraphicsVncInput>>,
    vnc_stop: Option<tokio::sync::oneshot::Sender<()>>,
    vnc_frame: Option<GraphicsVncFrame>,
    vnc_render_image: Option<Arc<RenderImage>>,
    vnc_retired_images: Vec<Arc<RenderImage>>,
    vnc_geometry: SharedGraphicsVncGeometry,
    vnc_button_mask: u8,
    worker_tx: mpsc::Sender<GraphicsWorkerResult>,
    worker_rx: mpsc::Receiver<GraphicsWorkerResult>,
}

impl GraphicsState {
    pub(super) fn new() -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        Self {
            distros: Vec::new(),
            sessions: Vec::new(),
            wslg_statuses: HashMap::new(),
            selected_distro: None,
            launch_mode: GraphicsLaunchMode::Desktop,
            app_command: String::new(),
            status: GraphicsStatus::Idle,
            error: None,
            loading: false,
            generation: 0,
            focused_input: None,
            session: None,
            vnc_session_id: None,
            vnc_input: None,
            vnc_stop: None,
            vnc_frame: None,
            vnc_render_image: None,
            vnc_retired_images: Vec::new(),
            vnc_geometry: SharedGraphicsVncGeometry::default(),
            vnc_button_mask: 0,
            worker_tx,
            worker_rx,
        }
    }
}

impl WorkspaceApp {
    pub(super) fn open_graphics_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self.tabs.iter().find(|tab| tab.kind == TabKind::Graphics) {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Graphics,
                title: self.i18n.t("graphics.tab_title"),
                title_source: TabTitleSource::I18nKey("graphics.tab_title"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.main_window_tabs.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        self.start_graphics_load_if_needed(false);
        self.load_graphics_sessions();
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(super) fn render_graphics_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.graphics.session.is_some() || self.graphics.status != GraphicsStatus::Idle {
            return self.render_graphics_active_surface(window, cx);
        }
        self.render_graphics_distro_selector(cx)
    }

    pub(super) fn poll_graphics_worker_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        while let Ok(result) = self.graphics.worker_rx.try_recv() {
            match result {
                GraphicsWorkerResult::ListSessions { result } => {
                    if let Ok(mut sessions) = result {
                        sessions.sort_by(|left, right| {
                            left.distro
                                .cmp(&right.distro)
                                .then_with(|| left.desktop_name.cmp(&right.desktop_name))
                                .then_with(|| left.id.cmp(&right.id))
                        });
                        self.graphics.sessions = sessions;
                        changed = true;
                    }
                }
                GraphicsWorkerResult::LoadDistros { generation, result } => {
                    if generation != self.graphics.generation {
                        continue;
                    }
                    self.graphics.loading = false;
                    match result {
                        Ok(distros) => {
                            self.graphics.error = None;
                            self.graphics.distros = distros;
                            if self.graphics.selected_distro.is_none() {
                                self.graphics.selected_distro = self
                                    .graphics
                                    .distros
                                    .iter()
                                    .find(|distro| distro.is_default)
                                    .or_else(|| self.graphics.distros.first())
                                    .map(|distro| distro.name.clone());
                            }
                            self.start_graphics_wslg_detection(generation);
                            self.load_graphics_sessions();
                        }
                        Err(error) => {
                            self.graphics.error = Some(normalize_graphics_error(error));
                            self.graphics.distros.clear();
                        }
                    }
                    changed = true;
                }
                GraphicsWorkerResult::DetectWslg {
                    generation,
                    distro,
                    result,
                } => {
                    if generation != self.graphics.generation {
                        continue;
                    }
                    if let Ok(status) = result {
                        self.graphics.wslg_statuses.insert(distro, status);
                        changed = true;
                    }
                }
                GraphicsWorkerResult::Start { generation, result }
                | GraphicsWorkerResult::StartApp { generation, result } => {
                    if generation != self.graphics.generation {
                        continue;
                    }
                    match result {
                        Ok(session) => {
                            self.reset_graphics_vnc_viewer(true);
                            self.graphics.error = None;
                            self.graphics.status = GraphicsStatus::Starting;
                            self.graphics.session = Some(session);
                            self.load_graphics_sessions();
                        }
                        Err(error) => {
                            self.reset_graphics_vnc_viewer(true);
                            self.graphics.status = GraphicsStatus::Error;
                            self.graphics.error = Some(normalize_graphics_error(error));
                            self.graphics.session = None;
                        }
                    }
                    changed = true;
                }
                GraphicsWorkerResult::Stop {
                    generation,
                    session_id,
                    result,
                } => {
                    if generation != self.graphics.generation {
                        continue;
                    }
                    if self
                        .graphics
                        .session
                        .as_ref()
                        .is_none_or(|session| session.id == session_id)
                    {
                        self.graphics.session = None;
                        self.reset_graphics_vnc_viewer(true);
                        self.graphics.status = GraphicsStatus::Idle;
                        self.load_graphics_sessions();
                    }
                    if let Err(error) = result {
                        self.graphics.error = Some(normalize_graphics_error(error));
                    }
                    changed = true;
                }
                GraphicsWorkerResult::Reconnect { generation, result } => {
                    if generation != self.graphics.generation {
                        continue;
                    }
                    match result {
                        Ok(session) => {
                            self.reset_graphics_vnc_viewer(true);
                            self.graphics.error = None;
                            self.graphics.status = GraphicsStatus::Starting;
                            self.graphics.session = Some(session);
                            self.load_graphics_sessions();
                        }
                        Err(error) => {
                            self.reset_graphics_vnc_viewer(true);
                            self.graphics.status = GraphicsStatus::Error;
                            self.graphics.error = Some(normalize_graphics_error(error));
                        }
                    }
                    changed = true;
                }
                GraphicsWorkerResult::VncEvent(event) => {
                    changed |= self.apply_graphics_vnc_event(event);
                }
            }
        }
        self.drop_graphics_vnc_retired_images(window, cx);
        if changed {
            self.ensure_graphics_vnc_worker();
            cx.notify();
        }
    }

    pub(super) fn handle_graphics_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.graphics.session.is_some()
            && self.graphics.focused_input.is_none()
            && let Some(keysym) =
                graphics_vnc_keysyms(&event.keystroke.key, event.keystroke.key_char.as_deref())
        {
            // VNC needs explicit press/release. GPUI routes graphics input
            // through KeyDown here, so send a short key tap for now.
            self.send_graphics_vnc_input(GraphicsVncInput::Key { keysym, down: true });
            self.send_graphics_vnc_input(GraphicsVncInput::Key {
                keysym,
                down: false,
            });
            cx.notify();
            return true;
        }
        if self.graphics.focused_input != Some(GraphicsInput::AppCommand)
            || event.keystroke.modifiers.platform
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "enter" => {
                self.start_graphics_app();
                cx.notify();
                true
            }
            "escape" => {
                self.graphics.focused_input = None;
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            "backspace" => {
                let changed = self.graphics.app_command.pop().is_some()
                    || self.ime_marked_text.take().is_some();
                if changed {
                    // Empty Backspace should not repaint unless it clears IME
                    // composition state.
                    cx.notify();
                }
                true
            }
            _ => true,
        }
    }

    pub(super) fn graphics_input_value(&self, input: GraphicsInput) -> &str {
        match input {
            GraphicsInput::AppCommand => &self.graphics.app_command,
        }
    }

    pub(super) fn graphics_input_value_mut(&mut self, input: GraphicsInput) -> &mut String {
        match input {
            GraphicsInput::AppCommand => &mut self.graphics.app_command,
        }
    }

    pub(super) fn shutdown_graphics_session(&mut self) {
        let Some(session_id) = self
            .graphics
            .session
            .as_ref()
            .map(|session| session.id.clone())
        else {
            self.reset_graphics_vnc_viewer(true);
            return;
        };
        self.graphics.generation = self.graphics.generation.saturating_add(1);
        let generation = self.graphics.generation;
        self.reset_graphics_vnc_viewer(true);
        let tx = self.graphics.worker_tx.clone();
        let backend = self.wsl_graphics.clone();
        self.forwarding_runtime.spawn(async move {
            let result = backend
                .stop(&session_id)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(GraphicsWorkerResult::Stop {
                generation,
                session_id,
                result,
            });
        });
    }

    fn render_graphics_distro_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        if self.graphics.loading {
            return self.render_graphics_center_state(
                LucideIcon::LoaderCircle,
                self.i18n.t("graphics.loading_distros"),
                theme.accent,
                None,
                cx,
            );
        }

        if let Some(error) = self.graphics.error.as_ref()
            && error == WSL_GRAPHICS_UNAVAILABLE
        {
            return self.render_graphics_not_available(cx);
        }

        if self.graphics.distros.is_empty() && self.graphics.error.is_none() {
            return self.render_graphics_center_state(
                LucideIcon::Monitor,
                self.i18n.t("graphics.no_distros"),
                theme.text_muted,
                None,
                cx,
            );
        }

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .px(px(GRAPHICS_SELECTOR_PADDING_X))
            .bg(rgb(theme.bg))
            .child(
                div()
                    .w_full()
                    .max_w(px(GRAPHICS_SELECTOR_MAX_W))
                    .flex()
                    .flex_col()
                    .gap(px(GRAPHICS_SELECTOR_GAP))
                    .child(self.render_graphics_mode_tabs(cx))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t(match self.graphics.launch_mode {
                                GraphicsLaunchMode::Desktop => "graphics.select_distro",
                                GraphicsLaunchMode::App => "graphics.app_select_distro",
                            })),
                    )
                    .when_some(self.graphics.error.as_ref(), |panel, error| {
                        panel.child(self.render_graphics_error_box(error))
                    })
                    .child(self.render_graphics_launch_mode(cx))
                    .when(!self.graphics.sessions.is_empty(), |panel| {
                        panel.child(self.render_graphics_session_list(cx))
                    }),
            )
            .into_any_element()
    }

    fn render_graphics_mode_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let desktop_active = self.graphics.launch_mode == GraphicsLaunchMode::Desktop;
        let app_active = self.graphics.launch_mode == GraphicsLaunchMode::App;
        div()
            .grid()
            .grid_cols(2)
            .gap(px(2.0))
            .p(px(2.0))
            .rounded(px(self.tokens.radii.md))
            .bg(rgb(theme.bg_panel))
            .child(self.render_graphics_mode_tab(
                "graphics.desktop_mode",
                desktop_active,
                GraphicsLaunchMode::Desktop,
                cx,
            ))
            .child(self.render_graphics_mode_tab(
                "graphics.app_mode",
                app_active,
                GraphicsLaunchMode::App,
                cx,
            ))
            .into_any_element()
    }

    fn render_graphics_mode_tab(
        &self,
        label_key: &'static str,
        active: bool,
        mode: GraphicsLaunchMode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(if active { theme.text } else { theme.text_muted }))
            .bg(if active {
                rgb(theme.bg)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .child(self.i18n.t(label_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.graphics.launch_mode = mode;
                    this.graphics.focused_input =
                        (mode == GraphicsLaunchMode::App).then_some(GraphicsInput::AppCommand);
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_graphics_launch_mode(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.graphics.launch_mode {
            GraphicsLaunchMode::Desktop => self.render_graphics_desktop_mode(cx),
            GraphicsLaunchMode::App => self.render_graphics_app_mode(cx),
        }
    }

    fn render_graphics_desktop_mode(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_graphics_warning(self.i18n.t("graphics.desktop_experimental"), None))
            .children(
                self.graphics
                    .distros
                    .iter()
                    .cloned()
                    .map(|distro| self.render_graphics_distro_row(distro, cx)),
            )
            .into_any_element()
    }

    fn render_graphics_app_mode(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let selected_status = self
            .graphics
            .selected_distro
            .as_ref()
            .and_then(|name| self.graphics.wslg_statuses.get(name));
        let can_start = self.graphics.selected_distro.is_some()
            && !self.graphics.app_command.trim().is_empty()
            && self.graphics.status != GraphicsStatus::Starting;
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.render_graphics_warning(
                self.i18n.t("graphics.desktop_experimental"),
                Some(self.i18n.t("graphics.app_experimental_note")),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.render_graphics_label("graphics.app_distro_label"))
                    .child(self.render_graphics_app_distro_selector(cx))
                    .when_some(selected_status, |field, status| {
                        field.child(self.render_graphics_wslg_badge(Some(status)))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.render_graphics_label("graphics.app_command_label"))
                    .child(self.render_graphics_app_command_input(cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("graphics.app_common_apps")),
                    )
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap(px(GRAPHICS_COMMON_APP_COL_GAP))
                            .children(COMMON_APPS.iter().map(|(label, command)| {
                                self.render_graphics_common_app_button(label, command, cx)
                            })),
                    ),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("graphics.start_app"),
                    ButtonOptions {
                        variant: ButtonVariant::Default,
                        size: ButtonSize::Default,
                        radius: ButtonRadius::Md,
                        disabled: !can_start,
                    },
                )
                .w_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        if this.graphics.selected_distro.is_some()
                            && !this.graphics.app_command.trim().is_empty()
                        {
                            this.start_graphics_app();
                            cx.notify();
                        }
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_graphics_session_list(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("graphics.tab_title")),
            )
            .children(
                self.graphics
                    .sessions
                    .iter()
                    .cloned()
                    .map(|session| self.render_graphics_session_row(session, cx)),
            )
            .into_any_element()
    }

    fn render_graphics_session_row(
        &self,
        session: WslGraphicsSession,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let reconnect_session_id = session.id.clone();
        let stop_session_id = session.id.clone();
        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(graphics_session_title(&session)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text_muted))
                            .child(session.distro.clone()),
                    ),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("graphics.reconnect"),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.reconnect_graphics_session_id(reconnect_session_id.clone());
                        cx.notify();
                    }),
                ),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("graphics.stop"),
                    ButtonOptions {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.stop_graphics_session_id(stop_session_id.clone());
                        cx.notify();
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_graphics_label(&self, key: &'static str) -> AnyElement {
        div()
            .text_size(px(13.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(key))
            .into_any_element()
    }

    fn render_graphics_distro_row(&self, distro: WslDistro, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let status = self.graphics.wslg_statuses.get(&distro.name);
        let distro_name = distro.name.clone();
        div()
            .flex()
            .items_center()
            .gap(px(GRAPHICS_DISTRO_ROW_GAP))
            .px(px(GRAPHICS_DISTRO_ROW_PADDING_X))
            .py(px(GRAPHICS_DISTRO_ROW_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba(0x00000000))
            .cursor_pointer()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(div().truncate().child(distro.name.clone()))
                            .when(distro.is_default, |name_row| {
                                name_row.child(self.render_graphics_default_badge())
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(if distro.is_running {
                                self.i18n.t("graphics.distro_running")
                            } else {
                                self.i18n.t("graphics.distro_stopped")
                            })
                            .child(self.render_graphics_wslg_badge(status)),
                    ),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ChevronRight,
                16.0,
                rgb(theme.text_muted),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.start_graphics_desktop(distro_name.clone());
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_graphics_app_distro_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .children(self.graphics.distros.iter().cloned().map(|distro| {
                let selected = self.graphics.selected_distro.as_deref() == Some(&distro.name);
                let distro_name = distro.name.clone();
                div()
                    .h(px(34.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgb(if selected { theme.accent } else { theme.border }))
                    .bg(if selected {
                        rgba((theme.accent << 8) | GRAPHICS_ALPHA_10)
                    } else {
                        rgb(theme.bg)
                    })
                    .text_size(px(13.0))
                    .text_color(rgb(theme.text))
                    .cursor_pointer()
                    .child(div().flex_1().truncate().child(format!(
                        "{}{}{}",
                        distro.name,
                        if distro.is_default { " (Default)" } else { "" },
                        if distro.is_running {
                            String::new()
                        } else {
                            format!(" - {}", self.i18n.t("graphics.distro_stopped"))
                        }
                    )))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Check,
                        14.0,
                        rgb(if selected { theme.accent } else { theme.bg }),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.graphics.selected_distro = Some(distro_name.clone());
                            cx.notify();
                        }),
                    )
            }))
            .into_any_element()
    }

    fn render_graphics_app_command_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = self.graphics.focused_input == Some(GraphicsInput::AppCommand);
        let target = WorkspaceImeTarget::Graphics(GraphicsInput::AppCommand);
        let marked = self.marked_text_for_target(target);
        let workspace = cx.entity();
        div()
            .relative()
            .child(text_input_anchor_probe(
                target.anchor_id(),
                oxideterm_gpui_ui::text_input(
                    &self.tokens,
                    TextInputView {
                        value: &self.graphics.app_command,
                        placeholder: self.i18n.t("graphics.app_command_placeholder"),
                        focused,
                        caret_visible: self.new_connection_caret_visible,
                        secret: false,
                        selected_all: false,
                        selected_range: self.ime_selected_range_for_target(target),
                        marked_text: marked,
                    },
                )
                .h(px(GRAPHICS_INPUT_H))
                .bg(rgb(theme.bg))
                .border_color(rgb(theme.border))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        this.graphics.focused_input = Some(GraphicsInput::AppCommand);
                        this.new_connection_caret_visible = true;
                        window.focus(&this.focus_handle);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    }),
                )
                .on_mouse_move(cx.listener(
                    |this, event: &gpui::MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    },
                )),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn render_graphics_common_app_button(
        &self,
        label: &str,
        command: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let command = command.to_string();
        button_with(
            &self.tokens,
            label.to_string(),
            ButtonOptions {
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled: false,
            },
        )
        .justify_start()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                this.graphics.app_command = command.clone();
                this.graphics.focused_input = Some(GraphicsInput::AppCommand);
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_graphics_active_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.ensure_graphics_vnc_worker();
        self.drop_graphics_vnc_retired_images(window, cx);
        let frame = self.graphics.vnc_frame.clone();
        let render_image = self.graphics.vnc_render_image.clone();
        let geometry = self.graphics.vnc_geometry.clone();
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(0x000000))
            .child(
                div()
                    .size_full()
                    .child(graphics_vnc_canvas(frame, render_image, geometry, 0x000000))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.send_graphics_vnc_pointer(
                                event.position,
                                Some((MouseButton::Left, true)),
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.send_graphics_vnc_pointer(
                                event.position,
                                Some((MouseButton::Right, true)),
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.send_graphics_vnc_pointer(
                                event.position,
                                Some((MouseButton::Middle, true)),
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                            this.send_graphics_vnc_pointer(
                                event.position,
                                Some((MouseButton::Left, false)),
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                            this.send_graphics_vnc_pointer(
                                event.position,
                                Some((MouseButton::Right, false)),
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                            this.send_graphics_vnc_pointer(
                                event.position,
                                Some((MouseButton::Middle, false)),
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.send_graphics_vnc_pointer(event.position, None);
                        cx.stop_propagation();
                    }))
                    .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                        this.send_graphics_vnc_scroll(event.position, &event.delta);
                        cx.stop_propagation();
                    })),
            )
            .child(self.render_graphics_toolbar(window, cx))
            .when(
                self.graphics.status != GraphicsStatus::Active
                    || self.graphics.error.is_some()
                    || self.graphics.session.is_none(),
                |surface| surface.child(self.render_graphics_status_overlay(cx)),
            )
            .child(
                div()
                    .absolute()
                    .right(px(16.0))
                    .bottom(px(16.0))
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba((theme.bg_panel << 8) | GRAPHICS_ALPHA_90))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | GRAPHICS_ALPHA_50))
                    .text_size(px(11.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.graphics_canvas_diagnostics_text()),
            )
            .into_any_element()
    }

    fn render_graphics_toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let session = self.graphics.session.as_ref();
        div()
            .absolute()
            .top(px(GRAPHICS_TOOLBAR_TOP))
            .left(px(GRAPHICS_TOOLBAR_TOP))
            .right(px(GRAPHICS_TOOLBAR_TOP))
            .flex()
            .items_center()
            .gap(px(GRAPHICS_TOOLBAR_GAP))
            .px(px(GRAPHICS_TOOLBAR_PADDING_X))
            .py(px(GRAPHICS_TOOLBAR_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_panel << 8) | GRAPHICS_ALPHA_90))
            .border_1()
            .border_color(rgba((theme.border << 8) | GRAPHICS_ALPHA_50))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .truncate()
                            .child(session.map(graphics_session_title).unwrap_or_default()),
                    )
                    .when_some(session, |meta, session| {
                        meta.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(px(11.0))
                                .text_color(rgb(theme.text_muted))
                                .child(session.distro.clone())
                                .when(
                                    matches!(session.mode, GraphicsSessionMode::App { .. }),
                                    |row| row.child(self.i18n.t("graphics.app_mode")),
                                )
                                .child(self.i18n.t("graphics.desktop_experimental")),
                        )
                    }),
            )
            .child(self.render_graphics_toolbar_button(
                "graphics.reconnect",
                ButtonVariant::Secondary,
                self.graphics.session.is_none(),
                GraphicsToolbarAction::Reconnect,
                window,
                cx,
            ))
            .child(self.render_graphics_toolbar_button(
                "graphics.fullscreen",
                ButtonVariant::Secondary,
                false,
                GraphicsToolbarAction::Fullscreen,
                window,
                cx,
            ))
            .child(self.render_graphics_toolbar_button(
                "graphics.stop",
                ButtonVariant::Destructive,
                self.graphics.session.is_none(),
                GraphicsToolbarAction::Stop,
                window,
                cx,
            ))
            .into_any_element()
    }

    fn render_graphics_toolbar_button(
        &self,
        label_key: &'static str,
        variant: ButtonVariant,
        disabled: bool,
        action: GraphicsToolbarAction,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Graphics toolbar buttons are shadcn-style Tauri actions; route
        // disabled activation through the same workspace guard as other toolbars.
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                match action {
                    GraphicsToolbarAction::Reconnect => {
                        if this.graphics.session.is_some() {
                            this.reconnect_graphics_session();
                        }
                    }
                    GraphicsToolbarAction::Fullscreen => window.toggle_fullscreen(),
                    GraphicsToolbarAction::Stop => this.stop_graphics_session(),
                }
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_graphics_status_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let (icon, text, color) = match self.graphics.status {
            GraphicsStatus::Starting => (
                LucideIcon::LoaderCircle,
                self.i18n.t("graphics.starting"),
                theme.accent,
            ),
            GraphicsStatus::Disconnected => (
                LucideIcon::WifiOff,
                self.i18n.t("graphics.disconnected"),
                GRAPHICS_AMBER_500,
            ),
            GraphicsStatus::Error => (
                LucideIcon::AlertCircle,
                self.graphics
                    .error
                    .clone()
                    .unwrap_or_else(|| self.i18n.t("graphics.error")),
                GRAPHICS_RED_400,
            ),
            _ => (
                LucideIcon::LoaderCircle,
                self.i18n.t("graphics.starting"),
                theme.accent,
            ),
        };
        div()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(GRAPHICS_STATUS_OVERLAY_BOTTOM))
            .flex()
            .justify_center()
            .child(
                div()
                    .max_w(px(520.0))
                    .px(px(GRAPHICS_STATUS_OVERLAY_PADDING_X))
                    .py(px(GRAPHICS_STATUS_OVERLAY_PADDING_Y))
                    .rounded(px(self.tokens.radii.md))
                    .bg(rgba((theme.bg_panel << 8) | GRAPHICS_ALPHA_90))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | GRAPHICS_ALPHA_50))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(Self::render_lucide_icon(icon, 16.0, rgb(color)))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(theme.text))
                            .child(text),
                    )
                    .when(
                        matches!(
                            self.graphics.status,
                            GraphicsStatus::Disconnected | GraphicsStatus::Error
                        ) && self.graphics.session.is_some(),
                        |overlay| {
                            overlay.child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("graphics.reconnect"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.reconnect_graphics_session();
                                        cx.notify();
                                    }),
                                ),
                            )
                        },
                    ),
            )
            .into_any_element()
    }

    fn render_graphics_center_state(
        &self,
        icon: LucideIcon,
        label: String,
        color: u32,
        action: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(self.tokens.ui.bg))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.0))
                    .text_align(gpui::TextAlign::Center)
                    .child(Self::render_lucide_icon(icon, 28.0, rgb(color)))
                    .child(
                        div()
                            .max_w(px(GRAPHICS_SELECTOR_MAX_W))
                            .text_size(px(14.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(label),
                    )
                    .when_some(action, |panel, label| {
                        panel.child(
                            button_with(
                                &self.tokens,
                                label,
                                ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: false,
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    this.start_graphics_load_if_needed(true);
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_graphics_not_available(&self, _cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .px(px(GRAPHICS_SELECTOR_PADDING_X))
            .bg(rgb(theme.bg))
            .child(
                div()
                    .w_full()
                    .max_w(px(GRAPHICS_SELECTOR_MAX_W))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_20))
                    .bg(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_10))
                    .p(px(16.0))
                    .flex()
                    .gap(px(12.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::AlertTriangle,
                        20.0,
                        rgb(GRAPHICS_AMBER_500),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("graphics.not_available")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("graphics.no_distros")),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_graphics_error_box(&self, error: &str) -> AnyElement {
        div()
            .px(px(GRAPHICS_WARNING_PADDING_X))
            .py(px(GRAPHICS_WARNING_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((GRAPHICS_RED_500 << 8) | GRAPHICS_ALPHA_10))
            .text_color(rgb(GRAPHICS_RED_400))
            .text_size(px(13.0))
            .child(error.to_string())
            .into_any_element()
    }

    fn render_graphics_warning(&self, strong: String, detail: Option<String>) -> AnyElement {
        div()
            .px(px(GRAPHICS_WARNING_PADDING_X))
            .py(px(GRAPHICS_WARNING_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_10))
            .border_1()
            .border_color(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_20))
            .text_size(px(12.0))
            .text_color(rgb(GRAPHICS_AMBER_500))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(strong))
                    .when_some(detail, |line, detail| {
                        line.child(div().ml(px(4.0)).child(detail))
                    }),
            )
            .into_any_element()
    }

    fn render_graphics_default_badge(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .ml(px(8.0))
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .bg(rgba((theme.accent << 8) | GRAPHICS_ALPHA_10))
            .text_size(px(11.0))
            .text_color(rgb(theme.accent))
            .child("Default")
            .into_any_element()
    }

    fn render_graphics_wslg_badge(&self, status: Option<&WslgStatus>) -> AnyElement {
        let Some(status) = status else {
            return div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(self.tokens.radii.sm))
                .bg(rgba((self.tokens.ui.text_muted << 8) | GRAPHICS_ALPHA_10))
                .text_size(px(GRAPHICS_BADGE_TEXT_SIZE))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child("WSLg N/A")
                .into_any_element();
        };
        if !status.available {
            return div()
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(self.tokens.radii.sm))
                .bg(rgba((self.tokens.ui.text_muted << 8) | GRAPHICS_ALPHA_10))
                .text_size(px(GRAPHICS_BADGE_TEXT_SIZE))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child("WSLg N/A")
                .into_any_element();
        }
        let label = match (status.wayland, status.x11) {
            (true, true) => "Wayland + X11",
            (true, false) => "Wayland",
            (false, true) => "X11",
            (false, false) => "WSLg",
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba((GRAPHICS_GREEN_500 << 8) | GRAPHICS_ALPHA_10))
                    .text_size(px(GRAPHICS_BADGE_TEXT_SIZE))
                    .text_color(rgb(GRAPHICS_GREEN_500))
                    .child(
                        div()
                            .size(px(GRAPHICS_DOT_SIZE))
                            .rounded_full()
                            .bg(rgb(GRAPHICS_GREEN_500)),
                    )
                    .child(label.to_string()),
            )
            .when(!status.has_openbox, |row| {
                row.child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgba((GRAPHICS_AMBER_500 << 8) | GRAPHICS_ALPHA_10))
                        .text_size(px(GRAPHICS_BADGE_TEXT_SIZE))
                        .text_color(rgb(GRAPHICS_AMBER_500))
                        .child(self.i18n.t("graphics.openbox_missing")),
                )
            })
            .into_any_element()
    }

    fn start_graphics_load_if_needed(&mut self, force: bool) {
        if self.graphics.loading || (!force && !self.graphics.distros.is_empty()) {
            return;
        }
        self.graphics.generation = self.graphics.generation.saturating_add(1);
        let generation = self.graphics.generation;
        self.graphics.loading = true;
        self.graphics.error = None;
        let tx = self.graphics.worker_tx.clone();
        self.forwarding_runtime.spawn(async move {
            let result = wsl::list_distros().map_err(|error| error.to_string());
            let _ = tx.send(GraphicsWorkerResult::LoadDistros { generation, result });
        });
    }

    fn load_graphics_sessions(&self) {
        let tx = self.graphics.worker_tx.clone();
        let backend = self.wsl_graphics.clone();
        self.forwarding_runtime.spawn(async move {
            let result = Ok::<_, String>(backend.list_sessions().await);
            let _ = tx.send(GraphicsWorkerResult::ListSessions { result });
        });
    }

    fn start_graphics_wslg_detection(&self, generation: u64) {
        for distro in self
            .graphics
            .distros
            .iter()
            .filter(|distro| distro.is_running)
        {
            let tx = self.graphics.worker_tx.clone();
            let backend = self.wsl_graphics.clone();
            let distro_name = distro.name.clone();
            self.forwarding_runtime.spawn(async move {
                let result = backend
                    .detect_wslg(&distro_name)
                    .await
                    .map_err(|error| error.to_string());
                let _ = tx.send(GraphicsWorkerResult::DetectWslg {
                    generation,
                    distro: distro_name,
                    result,
                });
            });
        }
    }

    fn start_graphics_desktop(&mut self, distro: String) {
        if self.graphics.status == GraphicsStatus::Starting {
            return;
        }
        self.graphics.generation = self.graphics.generation.saturating_add(1);
        let generation = self.graphics.generation;
        self.graphics.status = GraphicsStatus::Starting;
        self.graphics.error = None;
        self.graphics.session = None;
        self.reset_graphics_vnc_viewer(true);
        let tx = self.graphics.worker_tx.clone();
        let backend = self.wsl_graphics.clone();
        self.forwarding_runtime.spawn(async move {
            let result = backend
                .start_desktop(distro)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(GraphicsWorkerResult::Start { generation, result });
        });
    }

    fn start_graphics_app(&mut self) {
        if self.graphics.status == GraphicsStatus::Starting {
            return;
        }
        let Some(distro) = self.graphics.selected_distro.clone() else {
            return;
        };
        let argv = split_graphics_app_command(&self.graphics.app_command);
        if argv.is_empty() {
            return;
        }
        self.graphics.generation = self.graphics.generation.saturating_add(1);
        let generation = self.graphics.generation;
        self.graphics.status = GraphicsStatus::Starting;
        self.graphics.error = None;
        self.graphics.session = None;
        self.reset_graphics_vnc_viewer(true);
        let tx = self.graphics.worker_tx.clone();
        let backend = self.wsl_graphics.clone();
        self.forwarding_runtime.spawn(async move {
            let result = backend
                .start_app(distro, argv, None, None)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(GraphicsWorkerResult::StartApp { generation, result });
        });
    }

    fn stop_graphics_session(&mut self) {
        if self.graphics.session.is_some() {
            self.shutdown_graphics_session();
        } else {
            self.graphics.status = GraphicsStatus::Idle;
            self.graphics.error = None;
            self.reset_graphics_vnc_viewer(true);
        }
    }

    fn stop_graphics_session_id(&mut self, session_id: String) {
        if self
            .graphics
            .session
            .as_ref()
            .is_some_and(|session| session.id == session_id)
        {
            self.shutdown_graphics_session();
            return;
        }
        let tx = self.graphics.worker_tx.clone();
        let backend = self.wsl_graphics.clone();
        self.forwarding_runtime.spawn(async move {
            let result = backend
                .stop(&session_id)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(GraphicsWorkerResult::ListSessions {
                result: result.map(|_| Vec::new()),
            });
            let sessions = backend.list_sessions().await;
            let _ = tx.send(GraphicsWorkerResult::ListSessions {
                result: Ok(sessions),
            });
        });
    }

    fn reconnect_graphics_session(&mut self) {
        let Some(session_id) = self
            .graphics
            .session
            .as_ref()
            .map(|session| session.id.clone())
        else {
            return;
        };
        self.graphics.generation = self.graphics.generation.saturating_add(1);
        let generation = self.graphics.generation;
        self.graphics.status = GraphicsStatus::Starting;
        self.graphics.error = None;
        self.reset_graphics_vnc_viewer(true);
        let tx = self.graphics.worker_tx.clone();
        let backend = self.wsl_graphics.clone();
        self.forwarding_runtime.spawn(async move {
            let result = backend
                .reconnect(&session_id)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(GraphicsWorkerResult::Reconnect { generation, result });
        });
    }

    fn reconnect_graphics_session_id(&mut self, session_id: String) {
        self.graphics.generation = self.graphics.generation.saturating_add(1);
        let generation = self.graphics.generation;
        self.graphics.status = GraphicsStatus::Starting;
        self.graphics.error = None;
        self.reset_graphics_vnc_viewer(true);
        let tx = self.graphics.worker_tx.clone();
        let backend = self.wsl_graphics.clone();
        self.forwarding_runtime.spawn(async move {
            let result = backend
                .reconnect(&session_id)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(GraphicsWorkerResult::Reconnect { generation, result });
        });
    }

    fn apply_graphics_vnc_event(&mut self, event: GraphicsVncWorkerEvent) -> bool {
        match event {
            GraphicsVncWorkerEvent::Connected { session_id } => {
                if !self.graphics_session_matches(&session_id) {
                    return false;
                }
                self.graphics.status = GraphicsStatus::Active;
                self.graphics.error = None;
                true
            }
            GraphicsVncWorkerEvent::Frame { session_id, frame } => {
                if !self.graphics_session_matches(&session_id) {
                    return false;
                }
                if let Some(render_image) = frame.render_image() {
                    let old_image = self.graphics.vnc_render_image.replace(render_image);
                    self.retire_graphics_vnc_image(old_image);
                }
                self.graphics.vnc_frame = Some(frame);
                self.graphics.status = GraphicsStatus::Active;
                self.graphics.error = None;
                true
            }
            GraphicsVncWorkerEvent::Disconnected { session_id, reason } => {
                if !self.graphics_session_matches(&session_id) {
                    return false;
                }
                self.graphics.vnc_session_id = None;
                self.graphics.vnc_input = None;
                self.graphics.vnc_stop = None;
                self.graphics.vnc_button_mask = 0;
                let old_image = self.graphics.vnc_render_image.take();
                self.retire_graphics_vnc_image(old_image);
                if let Some(reason) = reason {
                    self.graphics.status = GraphicsStatus::Error;
                    self.graphics.error = Some(normalize_graphics_error(reason));
                } else {
                    self.graphics.status = GraphicsStatus::Disconnected;
                }
                true
            }
        }
    }

    fn retire_graphics_vnc_image(&mut self, image: Option<Arc<RenderImage>>) {
        if let Some(image) = image {
            self.graphics.vnc_retired_images.push(image);
        }
    }

    fn drop_graphics_vnc_retired_images(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for image in std::mem::take(&mut self.graphics.vnc_retired_images) {
            // The WSL graphics VNC preview replaces full-frame RenderImage
            // values frequently; dropping the Arc alone does not evict GPUI's
            // sprite atlas entry.
            cx.drop_image(image, Some(window));
        }
    }

    fn graphics_session_matches(&self, session_id: &str) -> bool {
        self.graphics
            .session
            .as_ref()
            .is_some_and(|session| session.id == session_id)
    }

    fn reset_graphics_vnc_viewer(&mut self, clear_frame: bool) {
        // The native VNC worker is a viewer concern: the WSL graphics crate owns
        // VNC/server processes, while GPUI owns the client connection and input
        // routing. Always stop the viewer before switching sessions.
        if let Some(stop) = self.graphics.vnc_stop.take() {
            let _ = stop.send(());
        }
        self.graphics.vnc_session_id = None;
        self.graphics.vnc_input = None;
        self.graphics.vnc_button_mask = 0;
        self.graphics.vnc_geometry.clear();
        if clear_frame {
            self.graphics.vnc_frame = None;
            let image = self.graphics.vnc_render_image.take();
            self.retire_graphics_vnc_image(image);
        }
    }

    fn ensure_graphics_vnc_worker(&mut self) {
        let Some(session) = self.graphics.session.clone() else {
            self.reset_graphics_vnc_viewer(true);
            return;
        };
        if self.graphics.vnc_session_id.as_deref() == Some(session.id.as_str())
            && self.graphics.vnc_input.is_some()
        {
            return;
        }

        self.reset_graphics_vnc_viewer(true);
        let (input_tx, input_rx) = tokio::sync::mpsc::unbounded_channel();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let event_tx = self.graphics.worker_tx.clone();
        let session_id = session.id.clone();
        let vnc_port = session.vnc_port;
        self.graphics.vnc_session_id = Some(session_id.clone());
        self.graphics.vnc_input = Some(input_tx);
        self.graphics.vnc_stop = Some(stop_tx);
        self.forwarding_runtime.spawn(async move {
            run_graphics_vnc_worker(session_id, vnc_port, input_rx, stop_rx, move |event| {
                let _ = event_tx.send(GraphicsWorkerResult::VncEvent(event));
            })
            .await;
        });
    }

    fn send_graphics_vnc_input(&mut self, input: GraphicsVncInput) -> bool {
        self.graphics
            .vnc_input
            .as_ref()
            .is_some_and(|sender| sender.send(input).is_ok())
    }

    fn send_graphics_vnc_pointer(
        &mut self,
        position: Point<Pixels>,
        button_update: Option<(MouseButton, bool)>,
    ) -> bool {
        let Some((x, y)) = self.graphics.vnc_geometry.pointer(position) else {
            return false;
        };
        if let Some((button, pressed)) = button_update {
            let mask = vnc_button_mask(button);
            if pressed {
                self.graphics.vnc_button_mask |= mask;
            } else {
                self.graphics.vnc_button_mask &= !mask;
            }
        }
        self.send_graphics_vnc_input(GraphicsVncInput::Pointer {
            x,
            y,
            buttons: self.graphics.vnc_button_mask,
        })
    }

    fn send_graphics_vnc_scroll(&mut self, position: Point<Pixels>, delta: &gpui::ScrollDelta) {
        let Some((x, y)) = self.graphics.vnc_geometry.pointer(position) else {
            return;
        };
        for mask in vnc_scroll_masks(delta) {
            let _ = self.send_graphics_vnc_input(GraphicsVncInput::Pointer {
                x,
                y,
                buttons: self.graphics.vnc_button_mask | mask,
            });
            let _ = self.send_graphics_vnc_input(GraphicsVncInput::Pointer {
                x,
                y,
                buttons: self.graphics.vnc_button_mask,
            });
        }
    }

    fn graphics_canvas_diagnostics_text(&self) -> String {
        let backend = if self.detected_graphics.driver_name.is_empty() {
            format!("{:?}", self.detected_graphics.kind)
        } else {
            self.detected_graphics.driver_name.clone()
        };
        format!("{}: {backend}", self.i18n.t("graphics.gpu_canvas_backend"))
    }
}

#[derive(Clone, Copy)]
enum GraphicsToolbarAction {
    Reconnect,
    Fullscreen,
    Stop,
}

fn graphics_session_title(session: &WslGraphicsSession) -> String {
    match &session.mode {
        GraphicsSessionMode::Desktop => session.desktop_name.clone(),
        GraphicsSessionMode::App { title, argv } => title
            .clone()
            .or_else(|| argv.first().cloned())
            .unwrap_or_else(|| session.desktop_name.clone()),
    }
}

fn split_graphics_app_command(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_graphics_error(error: String) -> String {
    if error.contains("only available on Windows")
        || error == WslGraphicsError::UnsupportedPlatform.to_string()
    {
        WSL_GRAPHICS_UNAVAILABLE.to_string()
    } else {
        error
    }
}
