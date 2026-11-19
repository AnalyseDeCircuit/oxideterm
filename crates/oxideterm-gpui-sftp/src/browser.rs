// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::{layout::SFTP_ROOT_PADDING, style::*};
use gpui::{
    AnchoredPositionMode, AnyElement, Bounds, ClipboardItem, Corner, CursorStyle, FocusHandle,
    MouseButton, MouseDownEvent, MouseMoveEvent, Rgba, ScrollEvent, SharedString, WeakEntity,
    Window, anchored, deferred, div, relative, rgb, rgba, svg,
};
use oxideterm_gpui_ui::breadcrumb_scroll::scroll_breadcrumb_by_wheel;
use oxideterm_gpui_ui::{
    button::{
        ButtonRadius, ButtonVariant, IconButtonOptions, ToolbarButtonOptions, icon_button,
        toolbar_action_button,
    },
    context_menu::{
        ContextMenuActionableStyle, context_menu_action, context_menu_actionable_row,
        context_menu_backdrop, context_menu_event_boundary,
    },
    icons::LucideIcon,
    modal::overlay_content_boundary,
    text_input::{
        TextInputContentAlign, TextInputView, text_input_anchor_probe,
        text_input_with_viewport_and_content_align,
    },
};
use oxideterm_i18n::I18n;
use oxideterm_sftp::{SftpPathBookmark, browser_files::sftp_path_segments};
use oxideterm_theme::ThemeTokens;
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
};

mod controls;
mod menus;
mod surface;

const SFTP_GAP: f32 = 8.0; // Tauri gap-2
const SFTP_PANE_SPLIT_HOTZONE_WIDTH: f32 = 14.0;
const SFTP_QUEUE_SPLIT_HOTZONE_HEIGHT: f32 = 14.0;
const SFTP_PANE_HEADER_HEIGHT: f32 = 40.0; // Tauri h-10
const SFTP_PANE_HEADER_GAP: f32 = 6.0;
const SFTP_PANE_HEADER_TITLE_MIN_WIDTH: f32 = 32.0;
const SFTP_PATH_BAR_HORIZONTAL_PADDING: f32 = 4.0;
const SFTP_BREADCRUMB_ROW_GAP: f32 = 1.0;
const SFTP_BREADCRUMB_SEGMENT_PADDING: f32 = 3.0;
const SFTP_BREADCRUMB_CONTENT_GAP: f32 = 2.0;
const SFTP_ICON_SM: f32 = 12.0; // Tauri h-3 w-3
const SFTP_ICON_MD: f32 = 14.0; // Tauri h-3.5 w-3.5
const SFTP_TOOL_BUTTON: f32 = 24.0; // Tauri h-6 w-6
const SFTP_SIZE_COL: f32 = 80.0; // Tauri w-20
const SFTP_MODIFIED_COL: f32 = 96.0; // Tauri w-24
const SFTP_PANEL_80_ALPHA: u32 = 0xcc; // Tauri bg-theme-bg-panel/80
const SFTP_ACTIVE_BORDER_ALPHA: u32 = 0x80; // Tauri border-oxide-accent/50
const SFTP_HEADER_ACTIVE_BG_ALPHA: u32 = 0x80; // Tauri bg-theme-bg-hover/50
const SFTP_HEADER_ACTIVE_BORDER_ALPHA: u32 = 0x4d; // Tauri border-oxide-accent/30
const SFTP_DRAG_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10
const SFTP_DRAG_RING_ALPHA: u32 = 0x4d; // Tauri ring-oxide-accent/30
const SFTP_BREADCRUMB_ACTIVE_ALPHA: u32 = 0x4d; // Tauri bg-theme-bg-hover/30
const SFTP_BREADCRUMB_HOVER_ALPHA: u32 = 0x80; // Tauri hover:bg-theme-bg-hover/50
const SFTP_YELLOW: u32 = 0xeab308; // Tauri text-yellow-500
const SFTP_RED: u32 = 0xf87171; // Tauri text-red-400
const SFTP_CONTEXT_MENU_WIDTH: f32 = 180.0; // Tauri min-w-[180px]
const SFTP_CONTEXT_MENU_MAX_HEIGHT: f32 = 288.0; // 8 items + separators, clamped like fixed portal menu
const SFTP_CONTEXT_MENU_PADDING: f32 = 4.0; // Tauri py-1
const SFTP_CONTEXT_MENU_ITEM_HEIGHT: f32 = 30.0; // Tauri px-3 py-1.5 text-xs

pub enum SftpBrowserRequest {
    Focus,
    CloseSidebar(NodeId),
    OpenTab(NodeId, String),
    TogglePin,
    BrowseUploads,
    BrowseFolder,
    ResizeCaptureChanged,
    RevealTerminalDirectory,
    SyncTerminalDirectory(String),
    ToggleTreeMode,
    EnsureConnection(NodeId),
    QuickDownload,
    LoadPairPrimary,
    LoadRemote,
    Transfer(SftpPane, SftpTransferDirection),
    TreeDownload(SftpFileEntry),
    Preview(SftpPane, SftpFileEntry),
    Extract(SftpFileEntry),
    SaveBookmark,
    RemoveBookmark(String),
    DismissOutside,
    InputPressed(SftpInput, MouseDownEvent),
    InputDragged(MouseMoveEvent),
    TransferFiles(SftpPane, SftpTransferDirection, Vec<String>),
    ExternalDrop(Vec<std::path::PathBuf>),
}

#[derive(Clone, Copy)]
pub enum SelectionRole {
    Document,
    RowSafe,
    NonSelectable,
}

type Dispatch = Rc<dyn Fn(SftpBrowserRequest, Option<&mut Window>, &mut App)>;
type Selectable = Rc<dyn Fn(SelectionRole, Option<u64>, u64, usize, String, u32) -> AnyElement>;

pub struct SftpBrowserFrame {
    pub sidebar: bool,
    pub sidebar_target: Option<NodeId>,
    pub sidebar_open: Option<NodeId>,
    pub active_sftp_tab: bool,
    pub embedded_sftp_pinned: bool,
    pub has_background: bool,
    pub remote_title: String,
    pub pair_primary_title: Option<String>,
    pub terminal_directory: Option<String>,
    pub reveal_terminal_key: Option<String>,
    pub can_sync_terminal: bool,
    pub tree_enabled: bool,
    pub bookmarks: Entity<SftpBookmarks>,
    pub bookmark_remote_key: Option<String>,
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub focus_handle: FocusHandle,
    pub mono_font: SharedString,
    pub ime: HashMap<SftpInput, SftpDialogIme>,
    pub completion_anchor: Rc<dyn Fn(SftpInput) -> Option<Bounds<Pixels>>>,
    pub selectable: Selectable,
    pub dispatch: Dispatch,
}

pub struct SftpBrowserView {
    source: WeakEntity<SftpWorkspaceEntity>,
    frame: SftpBrowserFrame,
    _observe: Subscription,
    _bookmarks: Subscription,
}

impl std::ops::Deref for SftpBrowserView {
    type Target = SftpBrowserFrame;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl SftpBrowserView {
    pub fn mount(
        source: &Entity<SftpWorkspaceEntity>,
        frame: SftpBrowserFrame,
        cx: &mut App,
    ) -> AnyElement {
        if let Some(view) = source.read(cx).feature_views.browser.clone() {
            view.update(cx, |view, cx| view.configure(frame, cx));
            return view.into_any_element();
        }
        let view = cx.new(|cx| Self::new(source, frame, cx));
        source.update(cx, |state, _| {
            state.feature_views.browser = Some(view.clone())
        });
        view.into_any_element()
    }

    fn new(
        source: &Entity<SftpWorkspaceEntity>,
        frame: SftpBrowserFrame,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            source: source.downgrade(),
            _bookmarks: cx.observe(&frame.bookmarks, |_, _, cx| cx.notify()),
            frame,
            _observe: cx.observe(source, |_, _, cx| cx.notify()),
        }
    }

    fn configure(&mut self, frame: SftpBrowserFrame, cx: &mut Context<Self>) {
        self.frame = frame;
        cx.notify();
    }

    pub(crate) fn refresh_ime(
        &mut self,
        input: SftpInput,
        ime: SftpDialogIme,
        cx: &mut Context<Self>,
    ) {
        self.frame.ime.insert(input, ime);
        cx.notify();
    }

    fn sftp_view(&self) -> Entity<SftpWorkspaceEntity> {
        self.source
            .upgrade()
            .expect("a mounted browser has a SFTP owner")
    }

    fn send(&self, request: SftpBrowserRequest, window: Option<&mut Window>, cx: &mut App) {
        (self.dispatch)(request, window, cx);
    }

    fn sftp_ui_metrics(&self) -> SftpUiMetrics {
        SftpUiMetrics::from_tokens(&self.tokens)
    }

    fn sftp_terminal_sync_target_path(&self, cx: &App) -> String {
        self.sftp_view()
            .read(cx)
            .terminal_sync_path(self.tree_enabled)
    }

    fn sftp_pair_primary_remote_id(&self, cx: &App) -> Option<SftpRemoteId> {
        self.sftp_view().read(cx).pair_primary_remote_id.clone()
    }

    fn sftp_selected_names(&self, pane: SftpPane, cx: &App) -> Vec<String> {
        let state = self.sftp_view().read(cx);
        if pane == SftpPane::Remote
            && self.tree_enabled
            && self.sidebar
            && state.remote_filter.trim().is_empty()
        {
            return state
                .selected_tree_file()
                .map(|file| vec![file.name])
                .unwrap_or_default();
        }
        state.selected_names(pane)
    }

    fn workspace_tooltip_icon_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        tooltip: String,
        prefix: &'static str,
        flex_none: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _view: Entity<Self>,
    ) -> AnyElement {
        let actionable = !(options.disabled || options.loading);
        let show = self.source.clone();
        let clear = self.source.clone();
        let label = tooltip.clone();
        icon_button(&self.tokens, render_lucide_icon(icon, size, color), options)
            .id((gpui::ElementId::from(prefix), tooltip.clone()))
            .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                let _ = show.update(cx, |_, cx| {
                    cx.emit(SftpWorkspaceEvent::TooltipRequested {
                        id: label.clone(),
                        label: label.clone(),
                        x: f32::from(event.position.x) + 12.0,
                        y: f32::from(event.position.y) + 16.0,
                    })
                });
            })
            .on_hover(move |hovered: &bool, _, cx| {
                if !*hovered {
                    let _ = clear.update(cx, |_, cx| {
                        cx.emit(SftpWorkspaceEvent::TooltipCleared {
                            id: tooltip.clone(),
                        })
                    });
                }
            })
            .when(actionable, |button| {
                button.on_mouse_down(MouseButton::Left, listener)
            })
            .when(!actionable, |button| {
                button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            })
            .when(flex_none, |button| button.flex_none())
            .into_any_element()
    }

    fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        toolbar_action_button(&self.tokens, label, icon, options, listener)
    }

    fn workspace_context_menu_backdrop(
        &self,
        menu: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        context_menu_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.send(SftpBrowserRequest::DismissOutside, Some(window), cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| {
                    this.send(SftpBrowserRequest::DismissOutside, Some(window), cx);
                    cx.stop_propagation();
                }),
            )
            .child(menu)
    }

    fn workspace_context_menu_persistent_styled_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        style: ContextMenuActionableStyle,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        context_menu_action(
            context_menu_actionable_row(item, disabled, loading, style),
            disabled,
            loading,
            cx.listener(move |this, event, window, cx| {
                listener(this, event, window, cx);
                cx.stop_propagation();
            }),
        )
    }

    fn workspace_context_menu_styled_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        style: ContextMenuActionableStyle,
        _close: impl Fn(&mut Self) + 'static,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_context_menu_persistent_styled_action(
            item, disabled, loading, style, listener, cx,
        )
    }

    fn apply_path_load(&self, load: SftpPathLoad, cx: &mut App) {
        match load {
            SftpPathLoad::Local => (),
            SftpPathLoad::Remote => self.send(SftpBrowserRequest::LoadRemote, None, cx),
            SftpPathLoad::PairPrimary => self.send(SftpBrowserRequest::LoadPairPrimary, None, cx),
        }
    }

    fn set_path(&self, pane: SftpPane, path: String, cx: &mut App) {
        let load = self
            .sftp_view()
            .update(cx, |state, cx| state.set_path(pane, path, cx));
        self.apply_path_load(load, cx);
    }

    fn enter_path(&self, pane: SftpPane, cx: &mut App) {
        let path = self.sftp_view().read(cx).committed_path_input(pane);
        if !path.is_empty() {
            self.set_path(pane, path, cx);
        }
    }

    fn start_path_edit(&self, pane: SftpPane, cx: &mut App) {
        self.sftp_view().update(cx, |state, cx| {
            state.start_path_edit(pane, cx);
            cx.notify();
        });
    }

    fn navigate_path(&self, pane: SftpPane, target: &str, cx: &mut App) {
        let path = self.sftp_view().read(cx).navigation_target(pane, target);
        self.set_path(pane, path, cx);
    }

    fn render_path_completion_overlay(
        &self,
        pane: SftpPane,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.sftp_view().read(cx);
        let (input, completion, id) = match pane {
            SftpPane::Local => (
                SftpInput::LocalPath,
                &state.local_path_completion,
                "sftp-local-path-completion",
            ),
            SftpPane::Remote => (
                SftpInput::RemotePath,
                &state.remote_path_completion,
                "sftp-remote-path-completion",
            ),
        };
        if !completion.is_visible() {
            return None;
        }
        let bounds = (self.completion_anchor)(input)?;
        let browser = cx.weak_entity();
        Some(oxideterm_gpui_file_manager::render_completion_popup(
            id,
            &self.tokens,
            bounds,
            completion.selected_index(),
            completion.scroll_handle.clone(),
            completion.suggestions(),
            move |index, cx| {
                let _ = browser.update(cx, |view, cx| {
                    let load = view
                        .sftp_view()
                        .update(cx, |state, cx| state.accept_completion(pane, index, cx));
                    if let Some(load) = load {
                        view.apply_path_load(load, cx);
                    }
                });
            },
        ))
    }

    fn render_sftp_inline_text(
        &self,
        input: SftpInput,
        pane: Option<SftpPane>,
        value: &str,
        placeholder_key: &'static str,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ime = &self.ime[&input];
        let control = text_input_with_viewport_and_content_align(
            &self.tokens,
            TextInputView {
                value,
                placeholder: self.i18n.t(placeholder_key),
                focused,
                caret_visible: ime.caret_visible,
                secret: false,
                selected_all: false,
                selected_range: ime.selected_range.clone(),
                marked_text: ime.marked_text.as_deref(),
            },
            TextInputContentAlign::Start,
            &ime.viewport,
            ime.active_offset,
        )
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .px(px(0.0))
        .border_0()
        .bg(rgba(0x00000000))
        .text_size(px(self.sftp_ui_metrics().text_xs))
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                this.sftp_view().update(cx, |state, cx| {
                    if let Some(pane) = pane {
                        state.active_pane = pane;
                    }
                    state.set_input_focus(Some(input), cx);
                    cx.notify();
                });
                this.send(
                    SftpBrowserRequest::InputPressed(input, event.clone()),
                    Some(window),
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
            this.send(
                SftpBrowserRequest::InputDragged(event.clone()),
                Some(window),
                cx,
            );
        }));
        let update = ime.update_anchor.clone();
        text_input_anchor_probe(ime.anchor_id, control, move |anchor, _, _| update(anchor))
            .into_any_element()
    }

    fn render_selectable_text_scoped(
        &self,
        scope: &str,
        key: impl Hash,
        text: impl Into<SharedString>,
        color: u32,
        _cx: &mut App,
    ) -> AnyElement {
        let text = text.into().to_string();
        (self.selectable)(
            SelectionRole::Document,
            None,
            selectable_id(scope, (key, &text)),
            0,
            text,
            color,
        )
    }

    fn render_display_text_with_role(
        &self,
        role: SelectionRole,
        scope: &str,
        key: impl Hash,
        text: String,
        color: u32,
        _cx: &mut App,
    ) -> AnyElement {
        (self.selectable)(
            role,
            None,
            selectable_id(scope, (key, text.as_str())),
            0,
            text,
            color,
        )
    }

    fn render_row_safe_selectable_display_text_in_group(
        &self,
        group: u64,
        scope: &str,
        key: impl Hash,
        order: usize,
        text: String,
        color: u32,
        _font_family: Option<SharedString>,
        _cx: &mut App,
    ) -> AnyElement {
        (self.selectable)(
            SelectionRole::RowSafe,
            Some(group),
            selectable_id(scope, (key, text.as_str())),
            order,
            text,
            color,
        )
    }

    fn handle_sftp_breadcrumb_scroll(
        &mut self,
        pane: SftpPane,
        event: &ScrollEvent,
        cx: &mut Context<Self>,
    ) {
        let ui = self.sftp_ui_metrics();
        let scroll_handle = match pane {
            SftpPane::Local => self.sftp_view().read(cx).local_path_scroll.clone(),
            SftpPane::Remote => self.sftp_view().read(cx).remote_path_scroll.clone(),
        };
        if let Some(changed) =
            scroll_breadcrumb_by_wheel(&scroll_handle, event, px(ui.size(SFTP_PANE_HEADER_HEIGHT)))
        {
            cx.stop_propagation();
            if changed {
                cx.notify();
            }
        }
    }

    fn render_sftp_file_list(
        &self,
        pane: SftpPane,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Entity<SftpFileListView> {
        let source = self.sftp_view().clone();
        source.update(cx, |sftp, _| {
            sftp.file_row_height = self.sftp_ui_metrics().size(SFTP_ROW_HEIGHT);
        });
        let selectable = self.selectable.clone();
        let frame = SftpFileListFrame {
            pane,
            tree_enabled: self.tree_enabled,
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            focus: self.focus_handle.clone(),
            has_background,
            selectable_text: Rc::new(move |scope, key, text, color| {
                let id = selectable_id(scope, ((key, text.as_str()), text.as_str()));
                selectable(SelectionRole::Document, None, id, 0, text, color)
            }),
        };
        let render_state = match pane {
            SftpPane::Local => &source.read(cx).feature_views.local_list,
            SftpPane::Remote => &source.read(cx).feature_views.remote_list,
        };
        if let Some(view) = render_state.clone() {
            view.update(cx, |view, cx| view.configure(frame, cx));
            return view;
        }
        let browser = cx.weak_entity();
        let dispatch = move |request: &SftpFileListRequest, window: &mut Window, cx: &mut App| {
            let _ = browser.update(cx, |view, cx| {
                view.handle_sftp_file_list_request(request, window, cx)
            });
        };
        let view = cx.new(|cx| {
            SftpFileListView::new(
                frame,
                &source,
                SftpWorkspaceEntity::browser_list_data,
                Rc::new(dispatch),
                cx,
            )
        });
        source.update(cx, |sftp, _| match pane {
            SftpPane::Local => sftp.feature_views.local_list = Some(view.clone()),
            SftpPane::Remote => sftp.feature_views.remote_list = Some(view.clone()),
        });
        view
    }

    fn handle_sftp_file_list_request(
        &mut self,
        request: &SftpFileListRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            SftpFileListRequest::DragMoved { pane, x, y } => {
                self.sftp_view().update(cx, |state, cx| {
                    if state.update_drag(*pane, *x, *y) {
                        cx.notify();
                    }
                });
            }
            SftpFileListRequest::DragEnded(pane) => {
                let transfer = self
                    .sftp_view()
                    .update(cx, |state, cx| state.finish_file_drag(*pane, cx));
                if let Some((source, direction, names)) = transfer {
                    self.send(
                        SftpBrowserRequest::TransferFiles(source, direction, names),
                        None,
                        cx,
                    );
                }
            }
            SftpFileListRequest::ExternalDrop(paths) => {
                self.send(SftpBrowserRequest::ExternalDrop(paths.clone()), None, cx);
                self.sftp_view().update(cx, |sftp, cx| {
                    sftp.drag_over_pane = None;
                    cx.notify();
                });
                cx.stop_propagation();
            }
            SftpFileListRequest::ClearMenu => {
                self.sftp_view().update(cx, |sftp, cx| {
                    if sftp.clear_context_menu_immediately() {
                        cx.notify();
                    }
                });
            }
            SftpFileListRequest::BlankPressed { pane, tree } => {
                window.focus(&self.focus_handle, cx);
                let menu_changed = self
                    .sftp_view()
                    .update(cx, |sftp, cx| sftp.dismiss_context_menu(cx));
                self.sftp_view().update(cx, |state, cx| {
                    if state.cancel_drag_capture() {
                        cx.notify();
                    }
                });
                let selection_changed = self.sftp_view().update(cx, |state, cx| {
                    let changed = state.clear_selection(*pane);
                    if changed {
                        cx.notify();
                    }
                    changed
                });
                let tree_selection_changed = *tree
                    && self.sftp_view().update(cx, |sftp, cx| {
                        let changed = sftp.tree_selected.take().is_some();
                        if changed {
                            cx.notify();
                        }
                        changed
                    });
                if menu_changed || selection_changed || tree_selection_changed {
                    cx.notify();
                }
            }
            SftpFileListRequest::Menu {
                pane,
                file,
                tree,
                event,
            } => {
                window.focus(&self.focus_handle, cx);
                let x = f32::from(event.position.x);
                let y = f32::from(event.position.y);
                self.sftp_view().update(cx, |sftp, cx| {
                    if *tree {
                        if let Some(file) = file {
                            sftp.open_tree_context_menu(file.clone(), x, y, cx);
                            return;
                        }
                        sftp.tree_selected = None;
                    }
                    if let Some(file) = file {
                        let selected = match pane {
                            SftpPane::Local => &sftp.local_selected,
                            SftpPane::Remote => &sftp.remote_selected,
                        };
                        if !selected.contains(&file.name) {
                            sftp.select_file(*pane, file.name.clone(), event.modifiers);
                        }
                    }
                    sftp.open_context_menu(*pane, file.clone(), x, y, cx);
                });
                cx.stop_propagation();
                if file.is_none() {
                    cx.notify();
                }
            }
            SftpFileListRequest::RowPressed { pane, file, event } => {
                self.sftp_view().update(cx, |sftp, cx| {
                    if event.click_count >= 2 {
                        sftp.activate_file(*pane, file.clone(), cx);
                    } else {
                        sftp.select_file(*pane, file.name.clone(), event.modifiers);
                        sftp.start_drag_candidate(
                            *pane,
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                        );
                    }
                    cx.notify();
                });
            }
            SftpFileListRequest::TreeToggle(path) => {
                self.sftp_view().update(cx, |sftp, cx| {
                    sftp.set_input_focus(None, cx);
                    sftp.select_tree_file(path.clone());
                    sftp.toggle_tree_directory(path.clone(), cx);
                    cx.notify();
                });
            }
            SftpFileListRequest::TreePressed { file, activate } => {
                self.sftp_view().update(cx, |sftp, cx| {
                    sftp.set_input_focus(None, cx);
                    sftp.select_tree_file(file.path.clone());
                    if *activate {
                        if file.file_type == SftpFileType::Directory {
                            sftp.toggle_tree_directory(file.path.clone(), cx);
                        } else {
                            sftp.activate_file(SftpPane::Remote, file.clone(), cx);
                        }
                    } else {
                        sftp.dismiss_context_menu(cx);
                    }
                    cx.notify();
                });
            }
        }
    }

    fn render_sftp_sidebar_transfer_queue(
        &self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> Entity<SftpQueueView<SftpRemoteId>> {
        self.mount_sftp_queue(
            QueuePresentation::Sidebar(SftpRemoteId::Node(node_id.clone())),
            cx,
        )
    }

    fn render_sftp_transfer_queue(
        &self,
        height: f32,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Entity<SftpQueueView<SftpRemoteId>> {
        self.mount_sftp_queue(
            QueuePresentation::Expanded {
                height,
                has_background,
            },
            cx,
        )
    }

    fn mount_sftp_queue(
        &self,
        presentation: QueuePresentation<SftpRemoteId>,
        cx: &mut Context<Self>,
    ) -> Entity<SftpQueueView<SftpRemoteId>> {
        let selectable = self.selectable.clone();
        let frame = SftpQueueFrame {
            presentation,
            tokens: self.tokens.clone(),
            i18n: self.i18n.clone(),
            mono_font: self.mono_font.clone(),
            selectable_text: Rc::new(move |scope, key, text, color| {
                let id = match key {
                    QueueTextKey::Count(count) => {
                        selectable_id(scope, ((count, text.as_str()), text.as_str()))
                    }
                    QueueTextKey::Label(label) => {
                        selectable_id(scope, ((label, text.as_str()), text.as_str()))
                    }
                };
                selectable(SelectionRole::Document, None, id, 0, text, color)
            }),
        };
        let source = self.sftp_view().clone();
        if let Some(view) = source.read(cx).feature_views.queue.clone() {
            view.update(cx, |view, cx| view.configure(frame, cx));
            return view;
        }

        let weak_source = source.downgrade();
        let view = cx.new(|cx| {
            SftpQueueView::new(
                frame,
                &source,
                |sftp| SftpQueueData {
                    transfers: &sftp.transfers,
                    incomplete_transfers: &sftp.incomplete_transfers,
                    incomplete_load_inflight: sftp.incomplete_load_inflight,
                    show_incomplete: sftp.show_incomplete,
                },
                Rc::new(move |request, cx| {
                    let _ = weak_source.update(cx, |sftp, cx| {
                        let event = match request {
                            SftpQueueRequest::SetState { id, state } => {
                                SftpWorkspaceEvent::TransferStateRequested { id, state }
                            }
                            SftpQueueRequest::CancelOrRemove { id } => {
                                SftpWorkspaceEvent::CancelOrRemoveTransferRequested { id }
                            }
                            SftpQueueRequest::ResumeIncomplete { transfer_id } => {
                                SftpWorkspaceEvent::ResumeIncompleteTransferRequested {
                                    transfer_id,
                                }
                            }
                            SftpQueueRequest::DiscardIncomplete { transfer_id } => {
                                SftpWorkspaceEvent::DiscardIncompleteTransferRequested {
                                    transfer_id,
                                }
                            }
                            SftpQueueRequest::Tooltip { id, label, x, y } => {
                                SftpWorkspaceEvent::TooltipRequested { id, label, x, y }
                            }
                            SftpQueueRequest::ClearTooltip { id } => {
                                SftpWorkspaceEvent::TooltipCleared { id }
                            }
                            SftpQueueRequest::ToggleIncomplete => {
                                sftp.show_incomplete = !sftp.show_incomplete;
                                sftp.notify_transfer_queue(cx);
                                return;
                            }
                            SftpQueueRequest::ClearCompleted => {
                                sftp.transfers
                                    .retain(|item| item.state != SftpTransferState::Completed);
                                sftp.notify_transfer_queue(cx);
                                return;
                            }
                            SftpQueueRequest::RevealLocalPath { path } => {
                                cx.reveal_path(Path::new(&path));
                                return;
                            }
                        };
                        cx.emit(event);
                    });
                }),
                cx,
            )
        });
        source.update(cx, |sftp, _| sftp.feature_views.queue = Some(view.clone()));
        view
    }
}

impl Render for SftpBrowserView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.source.upgrade().is_none() {
            return div().size(px(0.0)).into_any_element();
        }
        if self.sidebar {
            self.render_sftp_sidebar_surface(window, cx)
        } else {
            self.render_sftp_surface_for_tab(window, cx)
        }
    }
}

fn selectable_id(scope: &str, key: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    scope.hash(&mut hasher);
    key.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(color)
        .into_any_element()
}
