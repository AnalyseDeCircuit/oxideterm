use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::mpsc,
    thread,
};

use gpui::StatefulInteractiveElement;
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

pub(super) struct LauncherState {
    pub(super) core: LauncherRuntimeState,
    pub(super) focused_input: Option<LauncherInput>,
    pub(super) hovered_app_path: Option<String>,
    pub(super) hovered_wsl_distro: Option<String>,
    pub(super) pressed_app_path: Option<String>,
    worker_tx: mpsc::Sender<LauncherWorkerResult>,
    worker_rx: mpsc::Receiver<LauncherWorkerResult>,
}

impl LauncherState {
    pub(super) fn new(enabled: bool) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel();
        Self {
            core: LauncherRuntimeState::new(enabled),
            focused_input: None,
            hovered_app_path: None,
            hovered_wsl_distro: None,
            pressed_app_path: None,
            worker_tx,
            worker_rx,
        }
    }
}

impl WorkspaceApp {
    pub(super) fn open_launcher_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self.tabs.iter().find(|tab| tab.kind == TabKind::Launcher) {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Launcher,
                title: self.i18n.t("launcher.tabTitle"),
                title_source: TabTitleSource::I18nKey("launcher.tabTitle"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.main_window_tabs.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        self.launcher.focused_input = Some(LauncherInput::Search);
        if !launcher_requires_opt_in() || self.launcher.core.enabled {
            self.start_launcher_load_if_needed(false);
        }
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
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
        let enabled = self.launcher.core.enabled;
        let filtered_apps = if enabled {
            self.filtered_launcher_apps()
        } else {
            Vec::new()
        };
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
            .child(self.render_launcher_header(enabled, filtered_apps.len(), cx))
            .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
            .when(self.launcher.core.show_disable_confirm, |surface| {
                surface.child(self.render_launcher_disable_confirm(cx))
            })
            .child(if enabled {
                self.render_launcher_content(filtered_apps, window, cx)
            } else {
                self.render_launcher_consent(has_background, cx)
            })
            .into_any_element()
    }

    fn render_launcher_wsl_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("launcher");
        let filtered_distros = self.launcher.core.filtered_wsl_distros();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_launcher_wsl_header(filtered_distros.len(), cx))
            .child(self.render_launcher_wsl_search(cx))
            .child(self.render_launcher_wsl_content(filtered_distros, cx))
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
                    .opacity(if self.launcher.core.loading {
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
                            if !this.launcher.core.loading {
                                this.refresh_launcher(cx);
                            }
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_launcher_wsl_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = self.launcher.focused_input == Some(LauncherInput::Search);
        let target = WorkspaceImeTarget::Launcher(LauncherInput::Search);
        let marked = self.marked_text_for_target(target);
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
                                value: &self.launcher.core.search_query,
                                placeholder: self.i18n.t("launcher.searchWsl"),
                                focused,
                                caret_visible: self.new_connection_caret_visible,
                                secret: false,
                                selected_all: false,
                                selected_range: self.ime_selected_range_for_target(target),
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
                                this.launcher.focused_input = Some(LauncherInput::Search);
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
                    .child(div().absolute().left(px(10.0)).top(px(9.0)).child(
                        Self::render_lucide_icon(LucideIcon::Search, 14.0, rgb(theme.text_muted)),
                    )),
            )
            .into_any_element()
    }

    fn render_launcher_wsl_content(
        &mut self,
        filtered_distros: Vec<WslDistro>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.launcher.core.loading {
            return self.render_launcher_center_state(
                LucideIcon::LoaderCircle,
                self.i18n.t("launcher.loadingWsl"),
                self.tokens.ui.accent,
                None,
                cx,
            );
        }
        if let Some(error) = self.launcher.core.error.as_ref() {
            return self.render_launcher_center_state(
                LucideIcon::AlertCircle,
                error.clone(),
                LAUNCHER_RED_400,
                Some(self.i18n.t("launcher.retry")),
                cx,
            );
        }
        if filtered_distros.is_empty() {
            let label = if self.launcher.core.search_query.trim().is_empty() {
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

        self.sync_launcher_wsl_list_state(&filtered_distros);
        let state = self.launcher_wsl_list_state.clone();
        let spec = self.launcher_wsl_list_spec();
        let workspace = cx.entity();
        div()
            .id("launcher-wsl-scroll")
            .flex_1()
            .min_h(px(0.0))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| this.render_launcher_wsl_list_item(index, cx))
                },
            ))
            .into_any_element()
    }

    fn sync_launcher_wsl_list_state(&mut self, distros: &[WslDistro]) {
        let signatures = distros
            .iter()
            .map(launcher_wsl_distro_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.launcher_wsl_list_state,
            &mut self.launcher_wsl_list_cache.borrow_mut(),
            "launcher-wsl-distros",
            &signatures,
            self.launcher_wsl_list_spec(),
        );
    }

    fn launcher_wsl_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(LAUNCHER_WSL_LIST_ESTIMATED_HEIGHT),
            LAUNCHER_WSL_LIST_OVERSCAN,
        )
    }

    fn render_launcher_wsl_list_item(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let distros = self.launcher.core.filtered_wsl_distros();
        let total = distros.len();
        let Some(distro) = distros.into_iter().nth(index) else {
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
            .child(self.render_launcher_wsl_row(distro, cx))
            .into_any_element()
    }

    fn render_launcher_wsl_row(&self, distro: WslDistro, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let distro_name = distro.name.clone();
        let hovered = self.launcher.hovered_wsl_distro.as_deref() == Some(distro.name.as_str());
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
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgba((theme.border << 8) | LAUNCHER_WSL_BORDER_ALPHA_30))
            .bg(if hovered {
                rgba((theme.bg_hover << 8) | LAUNCHER_WSL_BG_HOVER_ALPHA_60)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .child(Self::render_lucide_icon(
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
                                    .rounded(px(self.tokens.radii.sm))
                                    .bg(rgba((theme.accent << 8) | LAUNCHER_WSL_ACCENT_ALPHA_20))
                                    .font_family(settings_mono_font_family(
                                        self.settings_store.settings(),
                                    ))
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
            .child(
                div()
                    .opacity(if hovered { 1.0 } else { 0.0 })
                    .child(Self::render_lucide_icon(
                        LucideIcon::ExternalLink,
                        14.0,
                        rgb(theme.text_muted),
                    )),
            )
            .on_mouse_move(cx.listener({
                let distro_name = distro_name.clone();
                move |this, _event: &MouseMoveEvent, _window, cx| {
                    // Pointer movement within the same row should not repaint
                    // the launcher unless the hover target actually changes.
                    if this.launcher.hovered_wsl_distro.as_deref() != Some(distro_name.as_str()) {
                        this.launcher.hovered_wsl_distro = Some(distro_name.clone());
                        cx.notify();
                    }
                }
            }))
            .on_hover(cx.listener({
                let distro_name = distro_name.clone();
                move |this, hovered: &bool, _window, cx| {
                    if !*hovered
                        && this.launcher.hovered_wsl_distro.as_deref() == Some(distro_name.as_str())
                    {
                        this.launcher.hovered_wsl_distro = None;
                        cx.notify();
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let distro_name = distro_name.clone();
                    move |this, _event, _window, cx| {
                        this.launch_wsl(&distro_name, cx);
                    }
                }),
            )
            .into_any_element()
    }

    pub(super) fn poll_launcher_worker_results(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        while let Ok(result) = self.launcher.worker_rx.try_recv() {
            match result {
                LauncherWorkerResult::LoadEntries { generation, result } => {
                    if self.launcher.core.apply_load_result(
                        generation,
                        result,
                        launcher_requires_opt_in(),
                    ) {
                        changed = true;
                    }
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub(super) fn handle_launcher_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.launcher.focused_input != Some(LauncherInput::Search)
            || event.keystroke.modifiers.platform
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.launcher.core.search_query.clear();
                self.launcher.focused_input = None;
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            "backspace" => {
                let changed = self.launcher.core.search_query.pop().is_some()
                    || self.ime_marked_text.take().is_some();
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

    pub(super) fn launcher_input_value(&self, input: LauncherInput) -> &str {
        match input {
            LauncherInput::Search => &self.launcher.core.search_query,
        }
    }

    pub(super) fn launcher_input_value_mut(&mut self, input: LauncherInput) -> &mut String {
        match input {
            LauncherInput::Search => &mut self.launcher.core.search_query,
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
        let focused = self.launcher.focused_input == Some(LauncherInput::Search);
        let target = WorkspaceImeTarget::Launcher(LauncherInput::Search);
        let marked = self.marked_text_for_target(target);
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
                                value: &self.launcher.core.search_query,
                                placeholder: self.i18n.t("launcher.search"),
                                focused,
                                caret_visible: self.new_connection_caret_visible,
                                secret: false,
                                selected_all: false,
                                selected_range: self.ime_selected_range_for_target(target),
                                marked_text: marked,
                            },
                        )
                        .h(px(LAUNCHER_SEARCH_H))
                        .pl(px(36.0))
                        .pr(px(12.0))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                this.launcher.focused_input = Some(LauncherInput::Search);
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
                        self.launcher.core.apps.len(),
                    )),
            )
            .child(self.render_launcher_icon_button(
                LucideIcon::RefreshCw,
                self.i18n.t("launcher.refresh"),
                self.launcher.core.loading,
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
                LauncherHeaderAction::Refresh => this.refresh_launcher(cx),
                LauncherHeaderAction::Disable => {
                    this.launcher.core.show_disable_confirm = true;
                    cx.notify();
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
                        this.launcher.core.show_disable_confirm = false;
                        cx.notify();
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
                        this.disable_launcher(cx);
                    }),
                ),
            )
            .into_any_element()
    }

    fn render_launcher_content(
        &mut self,
        filtered_apps: Vec<LauncherAppEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.launcher.core.loading && self.launcher.core.apps.is_empty() {
            return self.render_launcher_center_state(
                LucideIcon::LoaderCircle,
                self.i18n.t("launcher.scanning"),
                self.tokens.ui.accent,
                None,
                cx,
            );
        }
        if let Some(error) = self.launcher.core.error.as_ref() {
            return self.render_launcher_center_state(
                LucideIcon::AlertCircle,
                error.clone(),
                LAUNCHER_RED_400,
                Some(self.i18n.t("launcher.retry")),
                cx,
            );
        }
        if filtered_apps.is_empty() {
            let label = if self.launcher.core.search_query.trim().is_empty() {
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

        let columns = self.launcher_app_grid_columns(window);
        self.sync_launcher_app_grid_list_state(&filtered_apps, columns);
        let state = self.launcher_app_grid_list_state.clone();
        let spec = self.launcher_app_grid_list_spec();
        let workspace = cx.entity();
        div()
            .id("launcher-apps-scroll")
            .flex_1()
            .min_h(px(0.0))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_launcher_app_grid_row(index, columns, cx)
                    })
                },
            ))
            .into_any_element()
    }

    fn launcher_app_grid_columns(&self, window: &Window) -> usize {
        let settings = self.settings_store.settings();
        let mut available_width = f32::from(window.viewport_size().width);
        if !settings.sidebar_ui.zen_mode {
            available_width -= self.tokens.metrics.activity_bar_width;
            if !self.sidebar_collapsed {
                available_width -= self.sidebar_panel_width();
            }
        }
        if self.context_sidebar_visible() {
            available_width -= self.ai.chat.sidebar_width;
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

    fn sync_launcher_app_grid_list_state(&mut self, apps: &[LauncherAppEntry], columns: usize) {
        let signatures = apps
            .chunks(columns.max(1))
            .enumerate()
            .map(|(row_index, row)| launcher_app_grid_row_signature(row_index, columns, row))
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.launcher_app_grid_list_state,
            &mut self.launcher_app_grid_list_cache.borrow_mut(),
            "launcher-app-grid",
            &signatures,
            self.launcher_app_grid_list_spec(),
        );
    }

    fn launcher_app_grid_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(LAUNCHER_APP_GRID_ESTIMATED_ROW_HEIGHT),
            LAUNCHER_APP_GRID_OVERSCAN,
        )
    }

    fn render_launcher_app_grid_row(
        &self,
        row_index: usize,
        columns: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let apps = self.filtered_launcher_apps();
        let columns = columns.max(1);
        let start = row_index.saturating_mul(columns);
        let Some(row_apps) = apps.get(start..apps.len().min(start + columns)) else {
            return div().into_any_element();
        };
        div()
            .when(row_index == 0, |row| row.pt(px(4.0)))
            .pb(px(LAUNCHER_GRID_GAP_Y))
            .flex()
            .items_start()
            .gap_x(px(LAUNCHER_GRID_GAP_X))
            .children(
                row_apps
                    .iter()
                    .cloned()
                    .map(|app| self.render_launcher_app_icon(app, cx)),
            )
            .into_any_element()
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
                        cx.listener(|this, _event, _window, cx| this.refresh_launcher(cx)),
                    ),
                )
            })
            .into_any_element()
    }

    fn render_launcher_app_icon(
        &self,
        app: LauncherAppEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let app_path = app.path.clone();
        let tooltip_name = app.name.clone();
        let hovered = self.launcher.hovered_app_path.as_deref() == Some(app.path.as_str());
        let pressed = self.launcher.pressed_app_path.as_deref() == Some(app.path.as_str());
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
            .rounded(px(self.tokens.radii.lg))
            .bg(if hovered {
                rgba((0xffffff << 8) | LAUNCHER_WHITE_ALPHA_06)
            } else {
                rgba(0x00000000)
            })
            .cursor_pointer()
            .child(
                div()
                    .size(px(LAUNCHER_ICON_BOX))
                    .rounded(px(self.tokens.radii.lg))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .shadow(vec![gpui::BoxShadow {
                        color: rgba((0x000000 << 8) | 0x33).into(),
                        offset: gpui::point(px(0.0), px(2.0)),
                        blur_radius: px(4.0),
                        spread_radius: px(0.0),
                    }])
                    .child(self.render_launcher_app_icon_image(&app, icon_size)),
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
                let tooltip_name = tooltip_name.clone();
                move |this, event: &MouseMoveEvent, _window, cx| {
                    let tooltip_id = format!("launcher-app-{app_path}");
                    let hover_changed =
                        this.launcher.hovered_app_path.as_deref() != Some(app_path.as_str());
                    if hover_changed {
                        this.launcher.hovered_app_path = Some(app_path.clone());
                    }
                    this.queue_workspace_tooltip(
                        tooltip_id,
                        tooltip_name.clone(),
                        f32::from(event.position.x) + 12.0,
                        f32::from(event.position.y) + 16.0,
                        cx,
                    );
                    // Pending tooltip movement is not visible until the delay
                    // timer fires; visible tooltip movement is repainted by
                    // queue_workspace_tooltip itself.
                    if hover_changed {
                        cx.notify();
                    }
                }
            }))
            .on_hover(cx.listener({
                let app_path = app_path.clone();
                move |this, hovered: &bool, _window, cx| {
                    if !*hovered {
                        let mut changed = false;
                        if this.launcher.hovered_app_path.as_deref() == Some(app_path.as_str()) {
                            this.launcher.hovered_app_path = None;
                            changed = true;
                        }
                        if this.launcher.pressed_app_path.as_deref() == Some(app_path.as_str()) {
                            this.launcher.pressed_app_path = None;
                            changed = true;
                        }
                        let tooltip_changed =
                            this.clear_workspace_tooltip_state(&format!("launcher-app-{app_path}"));
                        // Combine tooltip and tile chrome changes so one leave
                        // event produces at most one repaint.
                        if changed || tooltip_changed {
                            cx.notify();
                        }
                    }
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let app_path = app_path.clone();
                    move |this, _event, _window, cx| {
                        this.launcher.pressed_app_path = Some(app_path.clone());
                        this.launch_app(&app_path, cx);
                    }
                }),
            )
            .into_any_element()
    }

    fn render_launcher_app_icon_image(&self, app: &LauncherAppEntry, icon_size: f32) -> AnyElement {
        let theme = self.tokens.ui;
        let radius = self.tokens.radii.lg;
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

    fn filtered_launcher_apps(&self) -> Vec<LauncherAppEntry> {
        self.launcher.core.filtered_apps()
    }

    fn enable_launcher(&mut self, cx: &mut Context<Self>) {
        self.launcher.core.enable();
        self.settings_store.settings_mut().launcher.enabled = true;
        let _ = self.settings_store.save();
        self.start_launcher_load_if_needed(true);
        cx.notify();
    }

    fn disable_launcher(&mut self, cx: &mut Context<Self>) {
        self.launcher.core.disable();
        self.launcher.focused_input = None;
        self.settings_store.settings_mut().launcher.enabled = false;
        let _ = self.settings_store.save();
        let _ = launcher_core::clear_icon_cache();
        cx.notify();
    }

    fn refresh_launcher(&mut self, cx: &mut Context<Self>) {
        self.launcher.core.clear_for_refresh();
        self.start_launcher_load_if_needed(true);
        cx.notify();
    }

    fn start_launcher_load_if_needed(&mut self, force: bool) {
        let Some(generation) = self
            .launcher
            .core
            .begin_load(force, launcher_requires_opt_in())
        else {
            return;
        };
        let tx = self.launcher.worker_tx.clone();
        thread::Builder::new()
            .name("oxideterm-launcher-scan".to_string())
            .spawn(move || {
                let result = launcher_core::load_entries();
                let _ = tx.send(LauncherWorkerResult::LoadEntries { generation, result });
            })
            .ok();
    }

    fn launch_app(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Err(error) = launcher_core::launch_app(path) {
            self.launcher.core.mark_launch_error(error);
        }
        cx.notify();
    }

    fn launch_wsl(&mut self, distro: &str, cx: &mut Context<Self>) {
        if let Err(error) = launcher_core::launch_wsl(distro) {
            self.launcher.core.mark_launch_error(error);
        }
        cx.notify();
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

fn launcher_app_grid_row_signature(
    row_index: usize,
    columns: usize,
    apps: &[LauncherAppEntry],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    // The virtual row key must change when responsive column count changes, and
    // app metadata changes should remeasure the row without rebuilding the
    // entire launcher grid.
    row_index.hash(&mut hasher);
    columns.hash(&mut hasher);
    for app in apps {
        app.path.hash(&mut hasher);
        app.name.hash(&mut hasher);
        app.icon_path.hash(&mut hasher);
    }
    hasher.finish()
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
