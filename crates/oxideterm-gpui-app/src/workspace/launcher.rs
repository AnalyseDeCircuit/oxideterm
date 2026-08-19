use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    thread,
};

use gpui::{EventEmitter, StatefulInteractiveElement};
use oxideterm_editor_core::utf16::replace_utf16;
use oxideterm_gpui_ui::{
    ButtonTone, SurfaceKind, SurfaceOptions, SurfacePadding, TextInputView, button,
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions, button_with,
    },
    semantic_surface, text_input_anchor_probe,
};
use oxideterm_launcher::{
    self as launcher_core, LauncherAppEntry, LauncherLoadResponse, LauncherRuntimeState, WslDistro,
};
use oxideterm_workspace::{Tab, TabKind, TabTitleSource};

use super::ime::WorkspaceImeTarget;
use super::*;

const LAUNCHER_SEARCH_WIDTH: f32 = 360.0; // Keeps application search prominent without crowding page actions.
const LAUNCHER_SEARCH_H: f32 = 32.0; // Matches compact controls used by sibling workspace pages.
const LAUNCHER_TILE_W: f32 = 88.0; // Tauri minmax(88px, 1fr).
const LAUNCHER_TILE_MIN_H: f32 = 100.0; // Tauri containIntrinsicSize 92px 100px.
const LAUNCHER_TILE_PADDING: f32 = 8.0; // Tauri p-2.
const LAUNCHER_ICON_BOX: f32 = 64.0; // Tauri w-16 h-16.
const LAUNCHER_ICON_FALLBACK: f32 = 28.0; // Tauri h-7 w-7.
const LAUNCHER_ICON_PRESSED: f32 = 59.0; // Tauri active:scale-[0.92] on a 64px icon.
const LAUNCHER_APP_NAME_W: f32 = 76.0; // Tauri max-w-[76px].
const LAUNCHER_APP_NAME_SIZE: f32 = 11.0; // Tauri text-[11px].
const LAUNCHER_APP_NAME_LINE_H: f32 = 13.0; // Tauri leading-tight.
const LAUNCHER_APP_NAME_LINES: f32 = 2.0; // Tauri line-clamp-2.
const LAUNCHER_CONSENT_MAX_W: f32 = 384.0; // Tauri max-w-sm.
const LAUNCHER_CONSENT_ICON: f32 = 56.0; // Tauri w-14 h-14.
const LAUNCHER_CONSENT_GAP: f32 = 24.0; // Tauri space-y-6.
const LAUNCHER_CONSENT_DETAIL_GAP: f32 = 10.0; // Tauri gap-2.5.
const LAUNCHER_CONFIRM_PADDING_X: f32 = 12.0; // Tauri px-3.
const LAUNCHER_CONFIRM_PADDING_Y: f32 = 10.0; // Tauri py-2.5.
const LAUNCHER_GRID_GAP_X: f32 = 8.0; // Tauri gap-x-2.
const LAUNCHER_GRID_GAP_Y: f32 = 4.0; // Tauri gap-y-1.
const LAUNCHER_WHITE_ALPHA_06: u32 = 0x0f; // Tauri bg-white/[0.06].
const LAUNCHER_TEXT_MUTED_60_ALPHA: u32 = 0x99; // Tauri text-muted/60.
const LAUNCHER_TEXT_SECONDARY_90_ALPHA: u32 = 0xe6; // Tauri text-secondary/90.
const LAUNCHER_RED_400: u32 = 0xf87171; // Tauri red-400.
const LAUNCHER_RED_500: u32 = 0xef4444; // Tauri red-500.
const LAUNCHER_RED_500_ALPHA_10: u32 = 0x1a; // Tauri red-500/10.
const LAUNCHER_RED_500_ALPHA_20: u32 = 0x33; // Tauri red-500/20.
const LAUNCHER_WSL_HEADER_PADDING_X: f32 = 16.0; // Tauri WSL header px-4.
const LAUNCHER_WSL_HEADER_PADDING_Y: f32 = 12.0; // Tauri WSL header py-3.
const LAUNCHER_WSL_SEARCH_PADDING_Y: f32 = 8.0; // Tauri WSL search py-2.
const LAUNCHER_WSL_CONTENT_PADDING: f32 = 16.0; // Tauri WSL content p-4.
const LAUNCHER_WSL_ROW_PADDING_X: f32 = 16.0; // Tauri WSL row px-4.
const LAUNCHER_WSL_ROW_PADDING_Y: f32 = 12.0; // Tauri WSL row py-3.
const LAUNCHER_WSL_ROW_GAP: f32 = 12.0; // Tauri WSL row/header gap-3.
const LAUNCHER_WSL_BADGE_TEXT_SIZE: f32 = 10.0; // Tauri text-[10px].
const LAUNCHER_WSL_DOT: f32 = 8.0; // Tauri w-2 h-2.
const LAUNCHER_WSL_GREEN_500: u32 = 0x22c55e; // Tauri green-500.
const LAUNCHER_WSL_BORDER_ALPHA_30: u32 = 0x4d; // Tauri border/30.
const LAUNCHER_WSL_BORDER_ALPHA_50: u32 = 0x80; // Tauri border/50.
const LAUNCHER_WSL_BG_HOVER_ALPHA_30: u32 = 0x4d; // Tauri bg-hover/30.
const LAUNCHER_WSL_BG_HOVER_ALPHA_60: u32 = 0x99; // Tauri bg-hover/60.
const LAUNCHER_WSL_ACCENT_ALPHA_20: u32 = 0x33; // Tauri accent/20.
const LAUNCHER_WSL_LIST_INITIAL_ITEM_COUNT: usize = 0;
const LAUNCHER_WSL_LIST_ESTIMATED_HEIGHT: f32 = 56.0;
const LAUNCHER_WSL_LIST_OVERSCAN: usize = 6;
const LAUNCHER_APP_GRID_INITIAL_ROW_COUNT: usize = 0;
const LAUNCHER_APP_GRID_ESTIMATED_ROW_HEIGHT: f32 = 104.0;
const LAUNCHER_APP_GRID_OVERSCAN: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum LauncherInput {
    Search,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LauncherHeaderAction {
    Refresh,
    Disable,
}

impl LauncherInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::Search => 1,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum LauncherWorkerResult {
    LoadEntries {
        generation: u64,
        result: Result<LauncherLoadResponse, String>,
    },
}

/// Owns launcher input, scan state, worker delivery, and scan lifetime.
pub(super) struct LauncherWorkspaceEntity {
    core: LauncherRuntimeState,
    focused_input: Option<LauncherInput>,
    hovered_app_path: Option<String>,
    hovered_wsl_distro: Option<String>,
    pressed_app_path: Option<String>,
    // Stable catalog indices avoid cloning the full result set for each virtual-list frame.
    filtered_app_indices: Vec<usize>,
    filtered_wsl_distro_indices: Vec<usize>,
    wsl_list_state: ListState,
    wsl_list_cache: RefCell<VirtualListSignatureCache>,
    app_grid_list_state: ListState,
    app_grid_list_cache: RefCell<VirtualListSignatureCache>,
    worker_tx: delivery::ActiveDeliverySender<LauncherWorkerResult>,
    worker_rx: std::sync::mpsc::Receiver<LauncherWorkerResult>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum LauncherWorkspaceEvent {
    EnabledChanged(bool),
    // Tooltips are the only row effect that belongs to the global workspace overlay.
    TooltipRequested {
        id: String,
        label: String,
        x: f32,
        y: f32,
    },
    TooltipCleared {
        id: String,
    },
}

impl EventEmitter<LauncherWorkspaceEvent> for LauncherWorkspaceEntity {}

impl LauncherWorkspaceEntity {
    pub(super) fn new(enabled: bool, cx: &mut Context<Self>) -> Self {
        let (worker_tx, worker_rx) = delivery::ActiveDeliverySender::channel();
        let entity = Self {
            core: LauncherRuntimeState::new(enabled),
            focused_input: None,
            hovered_app_path: None,
            hovered_wsl_distro: None,
            pressed_app_path: None,
            filtered_app_indices: Vec::new(),
            filtered_wsl_distro_indices: Vec::new(),
            // Measured list geometry belongs to the surface that filters and renders it.
            wsl_list_state: ListState::new(
                LAUNCHER_WSL_LIST_INITIAL_ITEM_COUNT,
                ListAlignment::Top,
                Self::wsl_list_spec().overdraw(),
            )
            .measure_all(),
            wsl_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            app_grid_list_state: ListState::new(
                LAUNCHER_APP_GRID_INITIAL_ROW_COUNT,
                ListAlignment::Top,
                Self::app_grid_list_spec().overdraw(),
            )
            .measure_all(),
            app_grid_list_cache: RefCell::new(VirtualListSignatureCache::default()),
            worker_tx,
            worker_rx,
        };
        entity.schedule_worker_delivery(cx);
        entity
    }

    pub(super) fn focused_input(&self) -> Option<LauncherInput> {
        self.focused_input
    }

    pub(super) fn focus_search(&mut self, cx: &mut Context<Self>) {
        if self.focused_input != Some(LauncherInput::Search) {
            self.focused_input = Some(LauncherInput::Search);
            cx.notify();
        }
    }

    pub(super) fn clear_input_focus(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(super) fn clear_pressed_app(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.pressed_app_path.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(super) fn input_value(&self, input: LauncherInput) -> &str {
        match input {
            LauncherInput::Search => &self.core.search_query,
        }
    }

    pub(super) fn replace_input(
        &mut self,
        input: LauncherInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.focused_input != Some(input) {
            return false;
        }
        match input {
            LauncherInput::Search => {
                replace_utf16(&mut self.core.search_query, replacement_range, text);
            }
        }
        self.rebuild_filtered_indices();
        cx.notify();
        true
    }

    fn enable(&mut self, cx: &mut Context<Self>) {
        self.core.enable();
        self.start_load_if_needed(true);
        cx.emit(LauncherWorkspaceEvent::EnabledChanged(true));
        cx.notify();
    }

    fn disable(&mut self, cx: &mut Context<Self>) {
        self.core.disable();
        self.focused_input = None;
        self.rebuild_filtered_indices();
        let _ = launcher_core::clear_icon_cache();
        cx.emit(LauncherWorkspaceEvent::EnabledChanged(false));
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.core.clear_for_refresh();
        self.rebuild_filtered_indices();
        self.start_load_if_needed(true);
        cx.notify();
    }

    fn start_load_if_needed(&mut self, force: bool) {
        let Some(generation) = self.core.begin_load(force, launcher_requires_opt_in()) else {
            return;
        };
        let tx = self.worker_tx.clone();
        thread::Builder::new()
            .name("oxideterm-launcher-scan".to_string())
            .spawn(move || {
                let result = launcher_core::load_entries();
                let _ = tx.send(LauncherWorkerResult::LoadEntries { generation, result });
            })
            .ok();
    }

    fn schedule_worker_delivery(&self, cx: &mut Context<Self>) {
        let worker_wake = self.worker_tx.wake();
        let release_wake = worker_wake.clone();
        cx.on_release(move |_, _| {
            // A platform scan may finish after the launcher entity is released.
            release_wake.stop();
        })
        .detach();
        cx.spawn(async move |entity, cx| {
            loop {
                worker_wake.wait().await;
                let should_drain = worker_wake.take();
                let stopped = worker_wake.is_stopped();
                if should_drain {
                    let backlog_remaining = entity
                        .update(cx, |launcher, cx| launcher.drain_worker_results(cx))
                        .unwrap_or(false);
                    if backlog_remaining {
                        // Continue bounded delivery without relying on the workspace heartbeat.
                        worker_wake.mark();
                    }
                }
                if stopped {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain_worker_results(&mut self, cx: &mut Context<Self>) -> bool {
        let result_batch =
            delivery::drain_channel(&self.worker_rx, delivery::USER_ACTION_DELIVERY_BUDGET);
        let mut changed = false;
        for result in result_batch.items {
            match result {
                LauncherWorkerResult::LoadEntries { generation, result } => {
                    let result_applied =
                        self.core
                            .apply_load_result(generation, result, launcher_requires_opt_in());
                    if result_applied {
                        self.rebuild_filtered_indices();
                    }
                    changed |= result_applied;
                }
            }
        }
        if changed {
            cx.notify();
        }
        result_batch.outcome.backlog_remaining
    }

    fn rebuild_filtered_indices(&mut self) {
        self.filtered_app_indices =
            launcher_core::filter_app_indices(&self.core.apps, &self.core.search_query);
        self.filtered_wsl_distro_indices = launcher_core::filter_wsl_distro_indices(
            &self.core.wsl_distros,
            &self.core.search_query,
        );
    }

    fn filtered_app_count(&self) -> usize {
        self.filtered_app_indices.len()
    }

    fn filtered_wsl_distro_count(&self) -> usize {
        self.filtered_wsl_distro_indices.len()
    }

    fn filtered_app_at(&self, filtered_index: usize) -> Option<LauncherAppEntry> {
        self.filtered_app_indices
            .get(filtered_index)
            .and_then(|catalog_index| self.core.apps.get(*catalog_index))
            .cloned()
    }

    fn filtered_wsl_distro_at(&self, filtered_index: usize) -> Option<WslDistro> {
        self.filtered_wsl_distro_indices
            .get(filtered_index)
            .and_then(|catalog_index| self.core.wsl_distros.get(*catalog_index))
            .cloned()
    }

    fn sync_wsl_list_state(&self) {
        let signatures = self
            .filtered_wsl_distro_indices
            .iter()
            .filter_map(|catalog_index| self.core.wsl_distros.get(*catalog_index))
            .map(launcher_wsl_distro_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.wsl_list_state,
            &mut self.wsl_list_cache.borrow_mut(),
            "launcher-wsl-distros",
            &signatures,
            Self::wsl_list_spec(),
        );
    }

    fn sync_app_grid_list_state(&self, columns: usize) {
        let columns = columns.max(1);
        let signatures = self
            .filtered_app_indices
            .chunks(columns)
            .enumerate()
            .map(|(row_index, catalog_indices)| {
                launcher_app_grid_catalog_row_signature(
                    row_index,
                    columns,
                    catalog_indices,
                    &self.core.apps,
                )
            })
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.app_grid_list_state,
            &mut self.app_grid_list_cache.borrow_mut(),
            "launcher-app-grid",
            &signatures,
            Self::app_grid_list_spec(),
        );
    }

    fn wsl_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(LAUNCHER_WSL_LIST_ESTIMATED_HEIGHT),
            LAUNCHER_WSL_LIST_OVERSCAN,
        )
    }

    fn app_grid_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(LAUNCHER_APP_GRID_ESTIMATED_ROW_HEIGHT),
            LAUNCHER_APP_GRID_OVERSCAN,
        )
    }

    fn render_wsl_list_item(
        &mut self,
        index: usize,
        tokens: ThemeTokens,
        mono_font_family: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let total = self.filtered_wsl_distro_count();
        let Some(distro) = self.filtered_wsl_distro_at(index) else {
            return div().into_any_element();
        };
        div()
            .px(px(LAUNCHER_WSL_CONTENT_PADDING))
            .when(index == 0, |item| item.pt(px(LAUNCHER_WSL_CONTENT_PADDING)))
            .pb(px(if index + 1 == total {
                LAUNCHER_WSL_CONTENT_PADDING
            } else {
                8.0
            }))
            .child(self.render_wsl_row(distro, tokens, mono_font_family, cx))
            .into_any_element()
    }

    fn render_wsl_row(
        &mut self,
        distro: WslDistro,
        tokens: ThemeTokens,
        mono_font_family: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let distro_name = distro.name.clone();
        let hovered = self.hovered_wsl_distro.as_deref() == Some(distro.name.as_str());
        div()
            .id((
                "launcher-wsl-distro",
                launcher_element_id_for_path(&distro.name),
            ))
            .flex()
            .items_center()
            .gap(px(LAUNCHER_WSL_ROW_GAP))
            .px(px(LAUNCHER_WSL_ROW_PADDING_X))
            .py(px(LAUNCHER_WSL_ROW_PADDING_Y))
            .rounded(px(tokens.radii.lg))
            .border_1()
            .border_color(rgba((theme.border << 8) | LAUNCHER_WSL_BORDER_ALPHA_30))
            .bg(if hovered {
                rgba((theme.bg_hover << 8) | LAUNCHER_WSL_BG_HOVER_ALPHA_60)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .child(WorkspaceApp::render_lucide_icon(
                LucideIcon::Terminal,
                20.0,
                rgb(theme.accent),
            ))
            .child(
                div().flex_1().min_w(px(0.0)).child(
                    div()
                        .flex()
                        .items_center()
                        .text_size(px(14.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .overflow_hidden()
                        .child(div().truncate().child(distro.name.clone()))
                        .when(distro.is_default, |row| {
                            row.child(
                                div()
                                    .ml(px(8.0))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(tokens.radii.sm))
                                    .bg(rgba((theme.accent << 8) | LAUNCHER_WSL_ACCENT_ALPHA_20))
                                    .font_family(mono_font_family.clone())
                                    .text_size(px(LAUNCHER_WSL_BADGE_TEXT_SIZE))
                                    .text_color(rgb(theme.accent))
                                    .child("DEFAULT"),
                            )
                        }),
                ),
            )
            .child(
                div()
                    .size(px(LAUNCHER_WSL_DOT))
                    .rounded(px(LAUNCHER_WSL_DOT / 2.0))
                    .bg(rgb(if distro.is_running {
                        LAUNCHER_WSL_GREEN_500
                    } else {
                        theme.text_muted
                    })),
            )
            .child(div().opacity(if hovered { 1.0 } else { 0.0 }).child(
                WorkspaceApp::render_lucide_icon(
                    LucideIcon::ExternalLink,
                    14.0,
                    rgb(theme.text_muted),
                ),
            ))
            .on_mouse_move(cx.listener({
                let distro_name = distro_name.clone();
                move |launcher, _event: &MouseMoveEvent, _window, cx| {
                    if launcher.hovered_wsl_distro.as_deref() != Some(distro_name.as_str()) {
                        launcher.hovered_wsl_distro = Some(distro_name.clone());
                        cx.notify();
                    }
                }
            }))
            .on_hover(cx.listener({
                let distro_name = distro_name.clone();
                move |launcher, hovered: &bool, _window, cx| {
                    if !*hovered
                        && launcher.hovered_wsl_distro.as_deref() == Some(distro_name.as_str())
                    {
                        launcher.hovered_wsl_distro = None;
                        cx.notify();
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |launcher, _event, _window, cx| {
                    launcher.launch_wsl(&distro_name, cx);
                }),
            )
            .into_any_element()
    }

    fn render_app_grid_row(
        &mut self,
        row_index: usize,
        columns: usize,
        tokens: ThemeTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let columns = columns.max(1);
        let start = row_index.saturating_mul(columns);
        let end = self.filtered_app_count().min(start.saturating_add(columns));
        let row_apps = (start..end)
            .filter_map(|filtered_index| self.filtered_app_at(filtered_index))
            .collect::<Vec<_>>();
        div()
            .when(row_index == 0, |row| row.pt(px(4.0)))
            .pb(px(LAUNCHER_GRID_GAP_Y))
            .flex()
            .items_start()
            .gap_x(px(LAUNCHER_GRID_GAP_X))
            .children(
                row_apps
                    .into_iter()
                    .map(|app| self.render_app_icon(app, tokens, cx)),
            )
            .into_any_element()
    }

    fn render_app_icon(
        &mut self,
        app: LauncherAppEntry,
        tokens: ThemeTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let app_path = app.path.clone();
        let tooltip_name = app.name.clone();
        let hovered = self.hovered_app_path.as_deref() == Some(app.path.as_str());
        let pressed = self.pressed_app_path.as_deref() == Some(app.path.as_str());
        let icon_size = if pressed {
            LAUNCHER_ICON_PRESSED
        } else {
            LAUNCHER_ICON_BOX
        };
        div()
            .id(("launcher-app", launcher_element_id_for_path(&app.path)))
            .w(px(LAUNCHER_TILE_W))
            .min_h(px(LAUNCHER_TILE_MIN_H))
            .p(px(LAUNCHER_TILE_PADDING))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.0))
            .rounded(px(tokens.radii.lg))
            .bg(if hovered {
                rgba((0xffffff << 8) | LAUNCHER_WHITE_ALPHA_06)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .child(
                div()
                    .size(px(LAUNCHER_ICON_BOX))
                    .rounded(px(tokens.radii.lg))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .shadow(vec![gpui::BoxShadow {
                        inset: false,
                        color: rgba((0x000000 << 8) | 0x33).into(),
                        offset: gpui::point(px(0.0), px(2.0)),
                        blur_radius: px(4.0),
                        spread_radius: px(0.0),
                    }])
                    .child(launcher_app_icon_image(&app, icon_size, tokens)),
            )
            .child(
                div()
                    .max_w(px(LAUNCHER_APP_NAME_W))
                    .h(px(LAUNCHER_APP_NAME_LINE_H * LAUNCHER_APP_NAME_LINES))
                    .overflow_hidden()
                    .text_align(gpui::TextAlign::Center)
                    .text_size(px(LAUNCHER_APP_NAME_SIZE))
                    .line_height(px(LAUNCHER_APP_NAME_LINE_H))
                    .text_color(rgba(
                        (theme.text_secondary << 8) | LAUNCHER_TEXT_SECONDARY_90_ALPHA,
                    ))
                    .child(app.name),
            )
            .on_mouse_move(cx.listener({
                let app_path = app_path.clone();
                move |launcher, event: &MouseMoveEvent, _window, cx| {
                    if launcher.hovered_app_path.as_deref() != Some(app_path.as_str()) {
                        launcher.hovered_app_path = Some(app_path.clone());
                        cx.notify();
                    }
                    cx.emit(LauncherWorkspaceEvent::TooltipRequested {
                        id: format!("launcher-app-{app_path}"),
                        label: tooltip_name.clone(),
                        x: f32::from(event.position.x) + 12.0,
                        y: f32::from(event.position.y) + 16.0,
                    });
                }
            }))
            .on_hover(cx.listener({
                let app_path = app_path.clone();
                move |launcher, hovered: &bool, _window, cx| {
                    if !*hovered {
                        let mut changed = false;
                        if launcher.hovered_app_path.as_deref() == Some(app_path.as_str()) {
                            launcher.hovered_app_path = None;
                            changed = true;
                        }
                        if launcher.pressed_app_path.as_deref() == Some(app_path.as_str()) {
                            launcher.pressed_app_path = None;
                            changed = true;
                        }
                        if changed {
                            cx.notify();
                        }
                        cx.emit(LauncherWorkspaceEvent::TooltipCleared {
                            id: format!("launcher-app-{app_path}"),
                        });
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |launcher, _event, _window, cx| {
                    launcher.select_app_with(&app_path, launcher_core::launch_app, cx);
                }),
            )
            .into_any_element()
    }

    fn select_app_with(
        &mut self,
        path: &str,
        launch: impl FnOnce(&str) -> Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        // Keep selection and launch failure in the same owner before repainting the row.
        self.pressed_app_path = Some(path.to_string());
        if let Err(error) = launch(path) {
            self.core.mark_launch_error(error);
        }
        cx.notify();
    }

    fn launch_wsl(&mut self, distro: &str, cx: &mut Context<Self>) {
        if let Err(error) = launcher_core::launch_wsl(distro) {
            self.core.mark_launch_error(error);
            cx.notify();
        }
    }
}

impl WorkspaceApp {
    pub(super) fn open_launcher_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::Launcher)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Launcher,
                    title: self.i18n.t("launcher.tabTitle"),
                    title_source: TabTitleSource::I18nKey("launcher.tabTitle"),
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            tab_id
        };
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        self.launcher.update(cx, |launcher, cx| {
            launcher.focus_search(cx);
            if !launcher_requires_opt_in() || launcher.core.enabled {
                launcher.start_load_if_needed(false);
                cx.notify();
            }
        });
        window.focus(&self.focus_handle, cx);
        self.reveal_active_tab(window, cx);
        cx.notify();
    }

    pub(super) fn render_launcher_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        if cfg!(target_os = "windows") {
            return self.render_launcher_wsl_surface(cx);
        }
        if cfg!(not(target_os = "macos")) {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("launcher.empty"))
                .into_any_element();
        }

        let has_background = self.launcher_background_active();
        let enabled = self.launcher.read(cx).core.enabled;
        let filtered_app_count = if enabled {
            self.launcher.read(cx).filtered_app_count()
        } else {
            0
        };
        let show_disable_confirm = self.launcher.read(cx).core.show_disable_confirm;
        let page_padding = self.tokens.metrics.settings_content_padding;
        let page_gap = self.tokens.metrics.settings_page_gap;
        div()
            .size_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap(px(page_gap))
            .p(px(page_padding))
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_launcher_header(enabled, filtered_app_count, cx))
            .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
            .when(show_disable_confirm, |surface| {
                surface.child(self.render_launcher_disable_confirm(cx))
            })
            .child(if enabled {
                self.render_launcher_content(window, cx)
            } else {
                self.render_launcher_consent(has_background, cx)
            })
            .into_any_element()
    }

    fn render_launcher_wsl_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("launcher");
        let filtered_distro_count = self.launcher.read(cx).filtered_wsl_distro_count();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_launcher_wsl_header(filtered_distro_count, cx))
            .child(self.render_launcher_wsl_search(cx))
            .child(self.render_launcher_wsl_content(cx))
            .into_any_element()
    }

    fn render_launcher_wsl_header(
        &self,
        filtered_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .gap(px(LAUNCHER_WSL_ROW_GAP))
            .px(px(LAUNCHER_WSL_HEADER_PADDING_X))
            .py(px(LAUNCHER_WSL_HEADER_PADDING_Y))
            .border_b_1()
            .border_color(rgb(theme.border))
            .flex_none()
            .child(Self::render_lucide_icon(
                LucideIcon::Terminal,
                16.0,
                rgb(theme.accent),
            ))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t("launcher.wslTitle")),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{filtered_count} distros")),
            )
            .child(
                div()
                    .id("launcher-wsl-refresh")
                    .size(px(28.0))
                    .rounded(px(self.tokens.radii.sm))
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(if self.launcher.read(cx).core.loading {
                        0.35
                    } else {
                        1.0
                    })
                    .cursor_pointer()
                    .child(Self::render_lucide_icon(
                        LucideIcon::RefreshCw,
                        14.0,
                        rgb(theme.text_muted),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            if !this.launcher.read(cx).core.loading {
                                this.launcher
                                    .update(cx, |launcher, cx| launcher.refresh(cx));
                            }
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_launcher_wsl_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let launcher = self.launcher.read(cx);
        let focused = launcher.focused_input == Some(LauncherInput::Search);
        let target = WorkspaceImeTarget::Launcher(LauncherInput::Search);
        let marked = self.marked_text_for_target(target, cx);
        let workspace = cx.entity();
        div()
            .px(px(LAUNCHER_WSL_HEADER_PADDING_X))
            .py(px(LAUNCHER_WSL_SEARCH_PADDING_Y))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | LAUNCHER_WSL_BORDER_ALPHA_50))
            .flex_none()
            .child(
                div()
                    .relative()
                    .child(text_input_anchor_probe(
                        target.anchor_id(),
                        oxideterm_gpui_ui::text_input(
                            &self.tokens,
                            TextInputView {
                                value: &launcher.core.search_query,
                                placeholder: self.i18n.t("launcher.searchWsl"),
                                focused,
                                caret_visible: self.input_caret.visible(),
                                secret: false,
                                selected_all: false,
                                selected_range: self.ime_selected_range_for_target(target, cx),
                                marked_text: marked,
                            },
                        )
                        .h(px(LAUNCHER_SEARCH_H))
                        .pl(px(32.0))
                        .bg(rgba((theme.bg_hover << 8) | LAUNCHER_WSL_BG_HOVER_ALPHA_30))
                        .border_color(rgba((theme.border << 8) | LAUNCHER_WSL_BORDER_ALPHA_50))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.launcher.update(cx, |launcher, cx| {
                                    launcher.focus_search(cx);
                                });
                                this.show_active_input_caret(cx);
                                window.focus(&this.focus_handle, cx);
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
                    .child(div().absolute().left(px(10.0)).top(px(9.0)).child(
                        Self::render_lucide_icon(LucideIcon::Search, 14.0, rgb(theme.text_muted)),
                    )),
            )
            .into_any_element()
    }

    fn render_launcher_wsl_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let (loading, error, query_empty, filtered_count) = {
            let launcher = self.launcher.read(cx);
            (
                launcher.core.loading,
                launcher.core.error.clone(),
                launcher.core.search_query.trim().is_empty(),
                launcher.filtered_wsl_distro_count(),
            )
        };
        if loading {
            return self.render_launcher_center_state(
                LucideIcon::LoaderCircle,
                self.i18n.t("launcher.loadingWsl"),
                self.tokens.ui.accent,
                None,
                cx,
            );
        }
        if let Some(error) = error {
            return self.render_launcher_center_state(
                LucideIcon::AlertCircle,
                error,
                LAUNCHER_RED_400,
                Some(self.i18n.t("launcher.retry")),
                cx,
            );
        }
        if filtered_count == 0 {
            let label = if query_empty {
                self.i18n.t("launcher.noWsl")
            } else {
                self.i18n.t("launcher.noWslResults")
            };
            return self.render_launcher_center_state(
                LucideIcon::Terminal,
                label,
                self.tokens.ui.text_muted,
                None,
                cx,
            );
        }

        self.launcher
            .update(cx, |launcher, _cx| launcher.sync_wsl_list_state());
        let state = self.launcher.read(cx).wsl_list_state.clone();
        let launcher_entity = self.launcher.clone();
        let tokens = self.tokens;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        div()
            .id("launcher-wsl-scroll")
            .flex_1()
            .min_h(px(0.0))
            .child(tauri_virtual_list(
                state,
                LauncherWorkspaceEntity::wsl_list_spec(),
                move |index, _window, cx| {
                    launcher_entity.update(cx, |launcher, cx| {
                        launcher.render_wsl_list_item(index, tokens, &mono_font_family, cx)
                    })
                },
            ))
            .into_any_element()
    }

    pub(super) fn handle_launcher_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.launcher.read(cx).focused_input != Some(LauncherInput::Search)
            || event.keystroke.modifiers.platform
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.launcher.update(cx, |launcher, cx| {
                    launcher.core.search_query.clear();
                    launcher.focused_input = None;
                    launcher.rebuild_filtered_indices();
                    cx.notify();
                });
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            "backspace" => {
                let query_changed = self.launcher.update(cx, |launcher, cx| {
                    let changed = launcher.core.search_query.pop().is_some();
                    if changed {
                        launcher.rebuild_filtered_indices();
                        cx.notify();
                    }
                    changed
                });
                let changed = query_changed || self.ime_marked_text.take().is_some();
                if changed {
                    // Empty Backspace is only visible if it also clears an IME
                    // composition marker.
                    cx.notify();
                }
                true
            }
            _ => true,
        }
    }

    fn launcher_background_active(&self) -> bool {
        self.background_surface_active("launcher")
    }

    fn render_launcher_consent(&self, has_background: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let content = div()
            .w_full()
            .max_w(px(LAUNCHER_CONSENT_MAX_W))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(LAUNCHER_CONSENT_GAP))
            .text_align(gpui::TextAlign::Center)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(LAUNCHER_CONSENT_ICON))
                    .rounded(px(self.tokens.radii.lg))
                    .bg(rgba((theme.accent << 8) | 0x1a))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Rocket,
                        28.0,
                        rgb(theme.accent),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("launcher.consentTitle")),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(rgb(theme.text_secondary))
                            .child(self.i18n.t("launcher.consentDescription")),
                    ),
            )
            .child(self.render_launcher_consent_details(has_background))
            .child(
                button(
                    &self.tokens,
                    self.i18n.t("launcher.consentEnable"),
                    ButtonTone::Primary,
                )
                .w_full()
                .h(px(32.0))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.enable_launcher(cx);
                    }),
                ),
            );
        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .px(px(32.0))
            .child(content)
            .into_any_element()
    }

    fn render_launcher_consent_details(&self, has_background: bool) -> AnyElement {
        let theme = self.tokens.ui;
        let cache_path = launcher_core::icon_cache_dir()
            .to_string_lossy()
            .into_owned();
        semantic_surface(
            &self.tokens,
            SurfaceOptions::new(SurfaceKind::InsetGroup)
                .padding(SurfacePadding::Normal)
                .has_background_image(has_background),
        )
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .text_align(gpui::TextAlign::Left)
        .child(self.render_launcher_consent_detail(
            LucideIcon::Search,
            self.i18n.t("launcher.consentScan"),
            None,
        ))
        .child(self.render_launcher_consent_detail(
            LucideIcon::HardDrive,
            self.i18n.t("launcher.consentCache"),
            Some(cache_path),
        ))
        .child(self.render_launcher_consent_detail(
            LucideIcon::Shield,
            self.i18n.t("launcher.consentPrivacy"),
            None,
        ))
        .text_color(rgb(theme.text_muted))
        .into_any_element()
    }

    fn render_launcher_consent_detail(
        &self,
        icon: LucideIcon,
        label: String,
        detail: Option<String>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_start()
            .gap(px(LAUNCHER_CONSENT_DETAIL_GAP))
            .child(div().pt(px(2.0)).child(Self::render_lucide_icon(
                icon,
                16.0,
                rgb(theme.text_muted),
            )))
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(rgb(theme.text_muted))
                    .child(label)
                    .when_some(detail, |text, detail| {
                        text.child(
                            div()
                                .mt(px(4.0))
                                .font_family(settings_mono_font_family(
                                    self.settings_store.settings(),
                                ))
                                .text_size(px(10.0))
                                .text_color(rgba(
                                    (theme.text_muted << 8) | LAUNCHER_TEXT_MUTED_60_ALPHA,
                                ))
                                .child(detail),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_launcher_header(
        &self,
        enabled: bool,
        filtered_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let tools = enabled.then(|| self.render_launcher_header_tools(filtered_count, cx));
        div()
            .flex()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap(px(16.0))
            .flex_none()
            .child(
                div()
                    .min_w(px(280.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_2xl))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(self.i18n.t("launcher.title")),
                    )
                    .child(
                        div()
                            .max_w(px(680.0))
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .line_height(px(22.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("launcher.description")),
                    ),
            )
            .when_some(tools, |header, tools| header.child(tools))
            .into_any_element()
    }

    fn render_launcher_header_tools(
        &self,
        filtered_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let launcher = self.launcher.read(cx);
        let focused = launcher.focused_input == Some(LauncherInput::Search);
        let target = WorkspaceImeTarget::Launcher(LauncherInput::Search);
        let marked = self.marked_text_for_target(target, cx);
        let workspace = cx.entity();
        div()
            .flex()
            .items_center()
            .flex_wrap()
            .justify_end()
            .gap(px(8.0))
            .child(
                div()
                    .relative()
                    .w_full()
                    .max_w(px(LAUNCHER_SEARCH_WIDTH))
                    .child(text_input_anchor_probe(
                        target.anchor_id(),
                        oxideterm_gpui_ui::text_input(
                            &self.tokens,
                            TextInputView {
                                value: &launcher.core.search_query,
                                placeholder: self.i18n.t("launcher.search"),
                                focused,
                                caret_visible: self.input_caret.visible(),
                                secret: false,
                                selected_all: false,
                                selected_range: self.ime_selected_range_for_target(target, cx),
                                marked_text: marked,
                            },
                        )
                        .h(px(LAUNCHER_SEARCH_H))
                        .pl(px(36.0))
                        .pr(px(12.0))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.launcher.update(cx, |launcher, cx| {
                                    launcher.focus_search(cx);
                                });
                                this.show_active_input_caret(cx);
                                window.focus(&this.focus_handle, cx);
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
                    .child(div().absolute().left(px(12.0)).top(px(9.0)).child(
                        Self::render_lucide_icon(
                            LucideIcon::Search,
                            14.0,
                            rgba((theme.text_muted << 8) | LAUNCHER_TEXT_MUTED_60_ALPHA),
                        ),
                    )),
            )
            .child(
                div()
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(10.0))
                    .text_color(rgba((theme.text_muted << 8) | 0x80))
                    .child(launcher_core::count_label(
                        filtered_count,
                        launcher.core.apps.len(),
                    )),
            )
            .child(self.render_launcher_icon_button(
                LucideIcon::RefreshCw,
                self.i18n.t("launcher.refresh"),
                launcher.core.loading,
                LauncherHeaderAction::Refresh,
                cx,
            ))
            .child(self.render_launcher_icon_button(
                LucideIcon::Power,
                self.i18n.t("launcher.disable"),
                false,
                LauncherHeaderAction::Disable,
                cx,
            ))
            .into_any_element()
    }

    fn render_launcher_icon_button(
        &self,
        icon: LucideIcon,
        title: String,
        disabled: bool,
        action: LauncherHeaderAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.workspace_tooltip_icon_button(
            icon,
            14.0,
            rgb(theme.text),
            IconButtonOptions {
                size: LAUNCHER_SEARCH_H,
                disabled,
                idle_opacity: 0.5,
                ..IconButtonOptions::compact(LAUNCHER_SEARCH_H)
            },
            title,
            "launcher-icon-button",
            false,
            cx.listener(move |this, _event, _window, cx| match action {
                LauncherHeaderAction::Refresh => {
                    this.launcher
                        .update(cx, |launcher, cx| launcher.refresh(cx));
                }
                LauncherHeaderAction::Disable => {
                    this.launcher.update(cx, |launcher, cx| {
                        launcher.core.show_disable_confirm = true;
                        cx.notify();
                    });
                }
            }),
            cx.entity(),
        )
    }

    fn render_launcher_disable_confirm(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .px(px(LAUNCHER_CONFIRM_PADDING_X))
            .py(px(LAUNCHER_CONFIRM_PADDING_Y))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((LAUNCHER_RED_500 << 8) | LAUNCHER_RED_500_ALPHA_20))
            .bg(rgba((LAUNCHER_RED_500 << 8) | LAUNCHER_RED_500_ALPHA_10))
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.0))
                    .text_color(rgb(LAUNCHER_RED_400))
                    .child(self.i18n.t("launcher.disableConfirm")),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("launcher.disableCancel"),
                    ButtonOptions {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .h(px(24.0))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.launcher.update(cx, |launcher, cx| {
                            launcher.core.show_disable_confirm = false;
                            cx.notify();
                        });
                    }),
                ),
            )
            .child(
                button_with(
                    &self.tokens,
                    self.i18n.t("launcher.disableAction"),
                    ButtonOptions {
                        variant: ButtonVariant::Destructive,
                        size: ButtonSize::Sm,
                        radius: ButtonRadius::Md,
                        disabled: false,
                    },
                )
                .h(px(24.0))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.launcher
                            .update(cx, |launcher, cx| launcher.disable(cx));
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_launcher_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (initial_scan, error, query_empty, filtered_count) = {
            let launcher = self.launcher.read(cx);
            (
                launcher.core.loading && launcher.core.apps.is_empty(),
                launcher.core.error.clone(),
                launcher.core.search_query.trim().is_empty(),
                launcher.filtered_app_count(),
            )
        };
        if initial_scan {
            return self.render_launcher_center_state(
                LucideIcon::LoaderCircle,
                self.i18n.t("launcher.scanning"),
                self.tokens.ui.accent,
                None,
                cx,
            );
        }
        if let Some(error) = error {
            return self.render_launcher_center_state(
                LucideIcon::AlertCircle,
                error,
                LAUNCHER_RED_400,
                Some(self.i18n.t("launcher.retry")),
                cx,
            );
        }
        if filtered_count == 0 {
            let label = if query_empty {
                self.i18n.t("launcher.empty")
            } else {
                self.i18n.t("launcher.noResults")
            };
            return self.render_launcher_center_state(
                LucideIcon::Search,
                label,
                self.tokens.ui.text_muted,
                None,
                cx,
            );
        }

        let columns = self.launcher_app_grid_columns(window, cx);
        self.launcher.update(cx, |launcher, _cx| {
            launcher.sync_app_grid_list_state(columns)
        });
        let state = self.launcher.read(cx).app_grid_list_state.clone();
        let launcher_entity = self.launcher.clone();
        let tokens = self.tokens;
        div()
            .id("launcher-apps-scroll")
            .flex_1()
            .min_h(px(0.0))
            .child(tauri_virtual_list(
                state,
                LauncherWorkspaceEntity::app_grid_list_spec(),
                move |index, _window, cx| {
                    launcher_entity.update(cx, |launcher, cx| {
                        launcher.render_app_grid_row(index, columns, tokens, cx)
                    })
                },
            ))
            .into_any_element()
    }

    fn launcher_app_grid_columns(&self, window: &Window, cx: &App) -> usize {
        let settings = self.settings_store.settings();
        let mut available_width = f32::from(window.viewport_size().width);
        if !settings.sidebar_ui.zen_mode {
            available_width -= self.tokens.metrics.activity_bar_width;
            if !self.sidebar_collapsed {
                available_width -= self.sidebar_panel_width();
            }
        }
        if self.context_sidebar_visible() {
            available_width -= self.ai_entity.read(cx).chat_ui().sidebar_width;
        }
        let page_padding = self.tokens.metrics.settings_content_padding;
        let grid_width = (available_width - page_padding * 2.0).max(LAUNCHER_TILE_W);
        // The Tauri launcher is a wrapping icon grid. Native virtualizes one
        // horizontal grid row at a time, so columns derive from the same tile
        // width/gap constants instead of hard-coding an app count.
        ((grid_width + LAUNCHER_GRID_GAP_X) / (LAUNCHER_TILE_W + LAUNCHER_GRID_GAP_X))
            .floor()
            .max(1.0) as usize
    }

    fn render_launcher_center_state(
        &self,
        icon: LucideIcon,
        label: String,
        icon_color: u32,
        action: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .px(px(32.0))
            .child(if matches!(icon, LucideIcon::LoaderCircle) {
                self.render_loading_icon(
                    "launcher-center-loading",
                    if action.is_some() { 32.0 } else { 24.0 },
                    rgba((icon_color << 8) | 0xcc),
                )
            } else {
                Self::render_lucide_icon(
                    icon,
                    if action.is_some() { 32.0 } else { 24.0 },
                    rgba((icon_color << 8) | 0xcc),
                )
            })
            .child(
                div()
                    .text_size(px(14.0))
                    .text_align(gpui::TextAlign::Center)
                    .text_color(rgba((icon_color << 8) | 0xcc))
                    .child(label),
            )
            .when_some(action, |state, label| {
                state.child(
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
                            this.launcher
                                .update(cx, |launcher, cx| launcher.refresh(cx));
                        }),
                    ),
                )
            })
            .into_any_element()
    }

    fn enable_launcher(&mut self, cx: &mut Context<Self>) {
        self.launcher.update(cx, |launcher, cx| launcher.enable(cx));
    }
}

fn launcher_requires_opt_in() -> bool {
    cfg!(target_os = "macos")
}

fn launcher_element_id_for_path(path: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn launcher_wsl_distro_signature(distro: &WslDistro) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Distro name is the row identity; default/running badges affect visible row
    // content, so include them when deciding whether GPUI should remeasure.
    distro.name.hash(&mut hasher);
    distro.is_default.hash(&mut hasher);
    distro.is_running.hash(&mut hasher);
    hasher.finish()
}

fn launcher_app_grid_catalog_row_signature(
    row_index: usize,
    columns: usize,
    catalog_indices: &[usize],
    apps: &[LauncherAppEntry],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    // The virtual row key must change when responsive column count changes, and
    // app metadata changes should remeasure the row without rebuilding the
    // entire launcher grid.
    row_index.hash(&mut hasher);
    columns.hash(&mut hasher);
    for catalog_index in catalog_indices {
        if let Some(app) = apps.get(*catalog_index) {
            app.path.hash(&mut hasher);
            app.name.hash(&mut hasher);
            app.icon_path.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn launcher_app_icon_image(
    app: &LauncherAppEntry,
    icon_size: f32,
    tokens: ThemeTokens,
) -> AnyElement {
    let theme = tokens.ui;
    let radius = tokens.radii.lg;
    if let Some(icon_path) = app.icon_path.as_ref() {
        gpui::img(PathBuf::from(icon_path))
            .size(px(icon_size))
            .object_fit(ObjectFit::Contain)
            .with_fallback(move || {
                launcher_app_icon_fallback(theme.bg_panel, theme.text_muted, radius, icon_size)
            })
            .into_any_element()
    } else {
        launcher_app_icon_fallback(theme.bg_panel, theme.text_muted, radius, icon_size)
    }
}

fn launcher_app_icon_fallback(
    bg_panel: u32,
    text_muted: u32,
    radius: f32,
    icon_size: f32,
) -> AnyElement {
    div()
        .size(px(icon_size))
        .rounded(px(radius))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(bg_panel))
        .child(
            svg()
                .path(LucideIcon::AppWindow.path())
                .size(px(LAUNCHER_ICON_FALLBACK))
                .text_color(rgb(text_muted)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use gpui::TestAppContext;

    use super::*;

    #[gpui::test]
    fn scan_state_and_delivery_are_launcher_entity_owned(cx: &mut TestAppContext) {
        let launcher = cx.new(|cx| LauncherWorkspaceEntity::new(true, cx));
        let (generation, sender) = launcher.update(cx, |launcher, _cx| {
            let generation = launcher
                .core
                .begin_load(true, true)
                .expect("scan generation");
            (generation, launcher.worker_tx.clone())
        });
        sender
            .send(LauncherWorkerResult::LoadEntries {
                generation,
                result: Ok(LauncherLoadResponse {
                    apps: vec![LauncherAppEntry {
                        name: "Terminal".to_string(),
                        path: "/Applications/Terminal.app".to_string(),
                        bundle_id: None,
                        icon_path: None,
                    }],
                    icon_dir: None,
                    wsl_distros: Vec::new(),
                }),
            })
            .expect("launcher delivery");

        // Delivery completes while no launcher page or WorkspaceApp is mounted.
        cx.run_until_parked();

        launcher.read_with(cx, |launcher, _cx| {
            assert!(!launcher.core.loading);
            assert_eq!(launcher.core.apps.len(), 1);
            assert_eq!(launcher.core.apps[0].name, "Terminal");
            assert_eq!(launcher.filtered_app_count(), 1);
        });
    }

    #[gpui::test]
    fn app_selection_action_is_applied_by_launcher_entity(cx: &mut TestAppContext) {
        let launcher = cx.new(|cx| LauncherWorkspaceEntity::new(true, cx));
        let launched_path = Arc::new(Mutex::new(None));
        let observed_path = launched_path.clone();

        launcher.update(cx, |launcher, cx| {
            launcher.select_app_with(
                "/Applications/Terminal.app",
                move |path| {
                    *observed_path.lock().expect("selection capture") = Some(path.to_string());
                    Ok(())
                },
                cx,
            );
        });

        launcher.read_with(cx, |launcher, _cx| {
            assert_eq!(
                launcher.pressed_app_path.as_deref(),
                Some("/Applications/Terminal.app")
            );
        });
        assert_eq!(
            launched_path.lock().expect("selection result").as_deref(),
            Some("/Applications/Terminal.app")
        );
    }
}
