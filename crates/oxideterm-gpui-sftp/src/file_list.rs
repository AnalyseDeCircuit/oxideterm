// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::HashSet, path::PathBuf, rc::Rc};

#[cfg(not(target_family = "wasm"))]
use gpui::ExternalPaths;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, MouseButton, MouseDownEvent, MouseMoveEvent,
    Rgba, Subscription, UniformListScrollHandle, Window, div, prelude::*, px, rgb, rgba, svg,
};
use oxideterm_gpui_ui::{
    icons::LucideIcon,
    motion::animated_spinner,
    virtual_list::{TauriVirtualListSpec, tauri_virtual_uniform_list},
};
use oxideterm_i18n::I18n;
use oxideterm_sftp::{
    SftpFileEntry, SftpFileType,
    browser_files::{SftpSortDirection, SftpSortField, sftp_sorted_filtered_indices},
    format_file_size,
};
use oxideterm_theme::ThemeTokens;

use crate::{
    style::{SftpUiMetrics, sftp_hover_bg},
    tree::{SftpTreeData, SftpTreeRow},
};

const SFTP_ICON_SM: f32 = 12.0; // Tauri h-3 w-3
const SFTP_ICON_MD: f32 = 14.0; // Tauri h-3.5 w-3.5
const SFTP_ROW_HEIGHT: f32 = 25.0; // Tauri px-2 py-1 text-xs
const SFTP_VIRTUAL_OVERSCAN: usize = 15; // Keep SFTP file panes aligned with FileList virtual overdraw.
const SFTP_SIZE_COL: f32 = 80.0; // Tauri w-20
const SFTP_MODIFIED_COL: f32 = 96.0; // Tauri w-24
const SFTP_DRAG_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10
const SFTP_SELECTED_BG_ALPHA: u32 = 0x33; // Tauri bg-theme-accent/20

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SftpPane {
    Local,
    Remote,
}

pub fn sftp_file_list_virtual_spec(row_height: f32) -> TauriVirtualListSpec {
    // Tauri SFTP FileList uses the same row estimate and overscan for rendering
    // and keyboard reveal. Keep them as one named native spec so scrollIntoView
    // parity does not split from virtualized row rendering.
    TauriVirtualListSpec::new(px(row_height), SFTP_VIRTUAL_OVERSCAN)
}

pub struct SftpFileListData<'a> {
    pub files: &'a [SftpFileEntry],
    pub filter: &'a str,
    pub sort_field: SftpSortField,
    pub sort_direction: SftpSortDirection,
    pub selected: &'a HashSet<String>,
    pub scroll: &'a UniformListScrollHandle,
    pub compact: bool,
    pub loading: bool,
    pub drag_over: bool,
    pub tree: SftpTreeData<'a>,
    pub tree_selected: Option<&'a str>,
}

pub struct SftpFileListFrame {
    pub pane: SftpPane,
    pub tree_enabled: bool,
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub focus: FocusHandle,
    pub has_background: bool,
    pub selectable_text: Rc<dyn Fn(&str, u64, String, u32) -> AnyElement>,
}

#[derive(Clone)]
pub enum SftpFileListRequest {
    DragMoved {
        pane: SftpPane,
        x: f32,
        y: f32,
    },
    DragEnded(SftpPane),
    ExternalDrop(Vec<PathBuf>),
    #[cfg(target_family = "wasm")]
    BrowserDrop(gpui::BrowserFileDrop),
    ClearMenu,
    BlankPressed {
        pane: SftpPane,
        tree: bool,
    },
    Menu {
        pane: SftpPane,
        file: Option<SftpFileEntry>,
        tree: bool,
        event: MouseDownEvent,
    },
    RowPressed {
        pane: SftpPane,
        file: SftpFileEntry,
        event: MouseDownEvent,
    },
    TreeToggle(String),
    TreePressed {
        file: SftpFileEntry,
        activate: bool,
    },
}

type ListReader = Rc<dyn for<'a> Fn(SftpPane, &'a App) -> Option<SftpFileListData<'a>>>;
type ListDispatch = Rc<dyn Fn(&SftpFileListRequest, &mut Window, &mut App)>;

pub struct SftpFileListView {
    frame: SftpFileListFrame,
    source: ListReader,
    dispatch: ListDispatch,
    _observation: Subscription,
}

impl SftpFileListView {
    pub fn new<T: 'static>(
        frame: SftpFileListFrame,
        source: &Entity<T>,
        read: for<'a> fn(&'a T, SftpPane) -> SftpFileListData<'a>,
        dispatch: ListDispatch,
        cx: &mut Context<Self>,
    ) -> Self {
        let weak_source = source.downgrade();
        Self {
            frame,
            source: Rc::new(move |pane, cx| {
                weak_source
                    .upgrade()
                    .map(|source| read(source.read(cx), pane))
            }),
            dispatch,
            _observation: cx.observe(source, |_, _, cx| cx.notify()),
        }
    }

    pub fn configure(&mut self, frame: SftpFileListFrame, cx: &mut Context<Self>) {
        self.frame = frame;
        cx.notify();
    }

    fn sftp_ui_metrics(&self) -> SftpUiMetrics {
        SftpUiMetrics::from_tokens(&self.frame.tokens)
    }

    fn request<E: 'static, F: Fn(&E) -> SftpFileListRequest + 'static>(
        &self,
        make: F,
    ) -> impl Fn(&E, &mut Window, &mut App) + use<E, F> {
        let dispatch = self.dispatch.clone();
        move |event, window, cx| dispatch(&make(event), window, cx)
    }

    fn render_sftp_empty_file_list(
        &self,
        list: gpui::Stateful<gpui::Div>,
        pane: SftpPane,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let theme = self.frame.tokens.ui;
        list.child(
            div()
                .w_full()
                .py(px(48.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .text_size(px(ui.text_xs))
                .text_color(rgb(theme.text_muted))
                .child(div().mb(px(8.0)).opacity(0.4).child(render_lucide_icon(
                    LucideIcon::FolderOpen,
                    32.0,
                    rgb(theme.text_muted),
                )))
                .child((self.frame.selectable_text)(
                    "sftp-file-list-empty",
                    pane as u64,
                    self.frame.i18n.t("sftp.file_list.empty"),
                    theme.text_muted,
                )),
        )
        .into_any_element()
    }

    fn render_sftp_tree_rows(
        &self,
        list: gpui::Stateful<gpui::Div>,
        rows: Vec<SftpTreeRow>,
        has_background: bool,
        scroll_handle: UniformListScrollHandle,
    ) -> AnyElement {
        let ui = self.sftp_ui_metrics();
        let tokens = self.frame.tokens;
        let theme = tokens.ui;
        let focus = self.frame.focus.clone();
        let source = self.source.clone();
        let dispatch = self.dispatch.clone();
        let rows = std::sync::Arc::new(rows);
        let row_count = rows.len();
        list.child(tauri_virtual_uniform_list(
            ("sftp-file-list-virtual", SftpPane::Remote as u64),
            row_count,
            scroll_handle,
            sftp_file_list_virtual_spec(ui.size(SFTP_ROW_HEIGHT)),
            move |range, _window, _cx| {
                let focus = focus.clone();
                range
                    .map(|index| {
                        let row = rows[index].clone();
                        let file = row.entry.clone();
                        let is_directory = file.file_type == SftpFileType::Directory;
                        let is_selected = source(SftpPane::Remote, _cx)
                            .and_then(|data| data.tree_selected)
                            == Some(file.path.as_str());
                        let display_name = if let Some(target) = file.symlink_target.as_ref() {
                            format!(
                                "{} -> {}",
                                oxideterm_sftp::display_remote_path(&file.name),
                                oxideterm_sftp::display_remote_path(target)
                            )
                        } else {
                            oxideterm_sftp::display_remote_path(&file.name).into_owned()
                        };
                        let chevron = if is_directory {
                            let path = file.path.clone();
                            let icon = if row.loading {
                                LucideIcon::LoaderCircle
                            } else if row.expanded {
                                LucideIcon::ChevronDown
                            } else {
                                LucideIcon::ChevronRight
                            };
                            let toggle = dispatch.clone();
                            let chevron_focus = focus.clone();
                            div()
                                .flex_none()
                                .size(px(14.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                                    window.focus(&chevron_focus, cx);
                                    toggle(
                                        &SftpFileListRequest::TreeToggle(path.clone()),
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                })
                                .child(render_lucide_icon(
                                    icon,
                                    ui.size(SFTP_ICON_SM),
                                    rgb(theme.text_muted),
                                ))
                                .into_any_element()
                        } else {
                            div().w(px(14.0)).flex_none().into_any_element()
                        };
                        let row_file = file.clone();
                        let row_dispatch = dispatch.clone();
                        let row_focus = focus.clone();
                        let menu_file = file.clone();
                        let menu_dispatch = dispatch.clone();
                        let menu_focus = focus.clone();
                        div()
                            .w_full()
                            .h(px(ui.size(SFTP_ROW_HEIGHT)))
                            .flex()
                            .flex_row()
                            .items_center()
                            .px(px(8.0))
                            .py(px(4.0))
                            .border_b_1()
                            .border_color(rgba(theme.border << 8))
                            .text_size(px(ui.text_xs))
                            .text_color(if is_selected {
                                rgb(theme.accent)
                            } else {
                                rgb(theme.text)
                            })
                            .bg(if is_selected {
                                rgba((theme.accent << 8) | SFTP_SELECTED_BG_ALPHA)
                            } else {
                                rgba(theme.bg << 8)
                            })
                            .hover(move |row| row.bg(sftp_hover_bg(theme.bg_hover, has_background)))
                            .cursor_pointer()
                            .child(div().w(px(12.0 * row.depth as f32)).flex_none())
                            .child(chevron)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child({
                                        let icon = if is_directory {
                                            oxideterm_gpui_ui::file_icons::folder_icon(
                                                &file.name, false,
                                            )
                                        } else {
                                            oxideterm_gpui_ui::file_icons::file_icon(&file.name)
                                        };
                                        icon.with_symlink(file.is_symlink)
                                            .render(ui.size(SFTP_ICON_MD), &tokens)
                                    })
                                    .child(div().truncate().child(display_name)),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                move |event: &MouseDownEvent, window, cx| {
                                    window.focus(&row_focus, cx);
                                    row_dispatch(
                                        &SftpFileListRequest::TreePressed {
                                            file: row_file.clone(),
                                            activate: event.click_count >= 2,
                                        },
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                move |event: &MouseDownEvent, window, cx| {
                                    window.focus(&menu_focus, cx);
                                    menu_dispatch(
                                        &SftpFileListRequest::Menu {
                                            pane: SftpPane::Remote,
                                            file: Some(menu_file.clone()),
                                            tree: true,
                                            event: event.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                },
                            )
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            },
        ))
        .into_any_element()
    }
}

impl Render for SftpFileListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let SftpFileListFrame {
            pane,
            has_background,
            ..
        } = self.frame;
        let Some(data) = (self.source)(pane, cx) else {
            return div().into_any_element();
        };
        let ui = self.sftp_ui_metrics();
        let tokens = self.frame.tokens;
        let theme = tokens.ui;
        let compact = data.compact;
        let tree_mode = pane == SftpPane::Remote
            && self.frame.tree_enabled
            && compact
            && data.filter.trim().is_empty();
        let list = div()
            .id(("sftp-file-list-scroll", pane as u64))
            .flex_1()
            .min_h(px(0.0))
            .bg(if data.drag_over {
                rgba((theme.accent << 8) | SFTP_DRAG_BG_ALPHA)
            } else if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .on_mouse_move(self.request(move |event: &MouseMoveEvent| {
                SftpFileListRequest::DragMoved {
                    pane,
                    x: f32::from(event.position.x),
                    y: f32::from(event.position.y),
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                self.request(move |_| SftpFileListRequest::DragEnded(pane)),
            )
            .when(pane == SftpPane::Remote, |list| {
                #[cfg(target_family = "wasm")]
                {
                    list.can_drop(|drag, _, _| drag.is::<gpui::BrowserFileDrop>())
                        .drag_over::<gpui::BrowserFileDrop>(move |row, _, _, _| {
                            row.bg(rgba((theme.accent << 8) | SFTP_DRAG_BG_ALPHA))
                        })
                        .on_drop(self.request(|files: &gpui::BrowserFileDrop| {
                            SftpFileListRequest::BrowserDrop(files.clone())
                        }))
                }
                #[cfg(not(target_family = "wasm"))]
                list.can_drop(|drag, _window, _cx| drag.is::<ExternalPaths>())
                    .on_drop(self.request(|paths: &ExternalPaths| {
                        SftpFileListRequest::ExternalDrop(paths.paths().to_vec())
                    }))
            })
            .on_scroll(self.request(|_| SftpFileListRequest::ClearMenu))
            .on_mouse_down(
                MouseButton::Left,
                self.request(move |_| SftpFileListRequest::BlankPressed {
                    pane,
                    tree: tree_mode,
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                self.request(move |event: &MouseDownEvent| SftpFileListRequest::Menu {
                    pane,
                    tree: tree_mode,
                    file: None,
                    event: event.clone(),
                }),
            );

        if data.loading {
            return list
                .child(
                    div()
                        .w_full()
                        .py(px(48.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(8.0))
                        .text_size(px(ui.text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(animated_spinner(
                            &tokens,
                            ("sftp-file-list-loading", pane as usize),
                            svg()
                                .path(LucideIcon::LoaderCircle.path())
                                .size(px(20.0))
                                .text_color(rgb(theme.text_muted)),
                        ))
                        .child((self.frame.selectable_text)(
                            "sftp-file-list-loading",
                            pane as u64,
                            self.frame.i18n.t("sftp.file_list.loading"),
                            theme.text_muted,
                        )),
                )
                .into_any_element();
        }

        if tree_mode {
            let rows = data.tree.rows();
            if rows.is_empty() {
                return self.render_sftp_empty_file_list(list, pane);
            }
            return self.render_sftp_tree_rows(list, rows, has_background, data.scroll.clone());
        }

        let visible_indices = sftp_sorted_filtered_indices(
            data.files,
            data.filter,
            data.sort_field,
            data.sort_direction,
        );
        if visible_indices.is_empty() {
            return self.render_sftp_empty_file_list(list, pane);
        }

        let focus = self.frame.focus.clone();
        let source = self.source.clone();
        let dispatch = self.dispatch.clone();
        let visible_indices = std::sync::Arc::new(visible_indices);
        let scroll_handle = data.scroll.clone();
        let row_count = visible_indices.len();

        list.child(tauri_virtual_uniform_list(
            ("sftp-file-list-virtual", pane as u64),
            row_count,
            scroll_handle,
            sftp_file_list_virtual_spec(ui.size(SFTP_ROW_HEIGHT)),
            move |range, _window, _cx| {
                let focus = focus.clone();
                range
                    .map(|index| {
                        let source_index = visible_indices[index];
                        let (file, is_selected) = {
                            let Some(data) = source(pane, _cx) else {
                                return div().into_any_element();
                            };
                            let Some(file) = data.files.get(source_index).cloned() else {
                                return div().into_any_element();
                            };
                            let is_selected = data.selected.contains(&file.name);
                            (file, is_selected)
                        };
                        let row_file = file.clone();
                        let context_file = file.clone();
                        let display_name = if let Some(target) = file.symlink_target.as_ref() {
                            format!(
                                "{} -> {}",
                                oxideterm_sftp::display_remote_path(&file.name),
                                oxideterm_sftp::display_remote_path(target)
                            )
                        } else {
                            oxideterm_sftp::display_remote_path(&file.name).into_owned()
                        };
                        let size_text =
                            if file.file_type == SftpFileType::Directory || !file.size_known {
                                "-".to_string()
                            } else {
                                format_file_size(file.size)
                            };
                        div()
                            .w_full()
                            .h(px(ui.size(SFTP_ROW_HEIGHT)))
                            .flex()
                            .flex_row()
                            .items_center()
                            .px(px(8.0))
                            .py(px(4.0))
                            .border_b_1()
                            .border_color(rgba(theme.border << 8))
                            .text_size(px(ui.text_xs))
                            .text_color(if is_selected {
                                rgb(theme.accent)
                            } else {
                                rgb(theme.text)
                            })
                            .bg(if is_selected {
                                rgba((theme.accent << 8) | SFTP_SELECTED_BG_ALPHA)
                            } else {
                                rgba(theme.bg << 8)
                            })
                            .hover(move |row| row.bg(sftp_hover_bg(theme.bg_hover, has_background)))
                            .cursor_pointer()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child({
                                        let icon = if file.file_type == SftpFileType::Directory {
                                            oxideterm_gpui_ui::file_icons::folder_icon(
                                                &file.name, false,
                                            )
                                        } else {
                                            oxideterm_gpui_ui::file_icons::file_icon(&file.name)
                                        };
                                        icon.with_symlink(file.is_symlink)
                                            .render(ui.size(SFTP_ICON_MD), &tokens)
                                    })
                                    // Tauri file rows are select-none. Plain
                                    // display text also prevents retaining the
                                    // root selectable-text adapter here.
                                    .child(div().truncate().child(display_name)),
                            )
                            .when(!compact, |row| {
                                row.child(
                                    div()
                                        .w(px(ui.size(SFTP_SIZE_COL)))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(size_text),
                                )
                            })
                            .when(!compact, |row| {
                                let modified = local_modified_time(file.modified);
                                let date = modified.map_or_else(
                                    || "-".to_owned(),
                                    |time| time.format("%Y/%-m/%-d").to_string(),
                                );
                                row.child(
                                    div()
                                        .w(px(ui.size(SFTP_MODIFIED_COL)))
                                        .flex_none()
                                        .text_align(gpui::TextAlign::Right)
                                        .text_color(rgb(theme.text_muted))
                                        .child(date)
                                        .id(format!("sftp-mtime-{}:{}", pane as u64, file.path))
                                        .when_some(modified, |cell, time| {
                                            cell.tooltip(move |_, cx| {
                                                oxideterm_gpui_ui::tooltip::tooltip_view(
                                                    tokens,
                                                    time.format("%Y/%-m/%-d %H:%M:%S").to_string(),
                                                    None,
                                                    cx,
                                                )
                                            })
                                        }),
                                )
                            })
                            .on_mouse_down(MouseButton::Left, {
                                let focus = focus.clone();
                                let dispatch = dispatch.clone();
                                move |event: &MouseDownEvent, window, cx| {
                                    window.focus(&focus, cx);
                                    dispatch(
                                        &SftpFileListRequest::RowPressed {
                                            pane,
                                            file: row_file.clone(),
                                            event: event.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            })
                            .on_mouse_down(MouseButton::Right, {
                                let focus = focus.clone();
                                let dispatch = dispatch.clone();
                                move |event: &MouseDownEvent, window, cx| {
                                    window.focus(&focus, cx);
                                    dispatch(
                                        &SftpFileListRequest::Menu {
                                            pane,
                                            file: Some(context_file.clone()),
                                            tree: false,
                                            event: event.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            })
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            },
        ))
        .into_any_element()
    }
}

fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(color)
        .into_any_element()
}

fn local_modified_time(modified: Option<i64>) -> Option<chrono::DateTime<chrono::Local>> {
    let timestamp = modified.filter(|value| *value > 0)?;
    Some(chrono::DateTime::from_timestamp(timestamp, 0)?.with_timezone(&chrono::Local))
}
