use std::{ops::Range, rc::Rc, sync::Arc};

use gpui::{
    AnyElement, App, CursorStyle, Div, ElementId, MouseButton, MouseDownEvent, SharedString,
    Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_connection_monitor::{
    FilesystemCommandCapability, FilesystemEntrySeverity, FilesystemFilter,
    ResourceFilesystemEntry, ResourceFilesystemStatus, filesystem_attention_label_keys,
    filesystem_entry_severity, filesystem_filter_label_key, filesystem_kind_label_key,
    filesystem_percent_severity, filesystem_read_only_label_key, format_bytes,
};
use oxideterm_gpui_ui::{
    button::{IconButtonOptions, icon_button_with_tooltip},
    scroll::ScrollableElement,
    text_input::{
        TextInputAnchor, TextInputAnchorId, TextInputContentAlign, TextInputView,
        TextInputViewport, text_input_anchor_probe, text_input_with_viewport_and_content_align,
    },
    virtual_list::tauri_virtual_list,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::filesystem_state::{FilesystemPanel, filesystem_list_spec};

const FILESYSTEM_TABLE_HEADER_TEXT_SIZE: f32 = 10.0;
const FILESYSTEM_TABLE_COMMAND_TEXT_SIZE: f32 = 12.0;
const FILESYSTEM_TABLE_META_TEXT_SIZE: f32 = 10.0;
const FILESYSTEM_TABLE_VALUE_TEXT_SIZE: f32 = 11.0;
const FILESYSTEM_DETAIL_TEXT_SIZE: f32 = 11.0;
const HOST_FILESYSTEM_TABLE_HEADER_HEIGHT: f32 = 28.0;
const HOST_FILESYSTEM_TABLE_MAIN_ROW_HEIGHT: f32 = 36.0;
const HOST_FILESYSTEM_KIND_COLUMN_WIDTH: f32 = 74.0;
const HOST_FILESYSTEM_USAGE_COLUMN_WIDTH: f32 = 70.0;
const HOST_FILESYSTEM_INODE_COLUMN_WIDTH: f32 = 64.0;
const HOST_FILESYSTEM_FS_COLUMN_WIDTH: f32 = 74.0;
const HOST_FILESYSTEM_RO_COLUMN_WIDTH: f32 = 48.0;
const HOST_FILESYSTEM_SIZE_COLUMN_WIDTH: f32 = 104.0;
const HOST_FILESYSTEM_CONTEXT_COLUMNS_MIN_WIDTH: f32 = 720.0;
const MONITOR_BORDER_ALPHA: u32 = 0x80;
const MONITOR_TINT_ALPHA: u32 = 0x1a;
const MONITOR_EMERALD: u32 = 0x34d399;
const MONITOR_AMBER: u32 = 0xf59e0b;
const MONITOR_RED: u32 = 0xef4444;

/// Window-owned geometry and presentation for the non-secret filesystem search.
pub struct FilesystemSearchFrame {
    pub anchor_id: TextInputAnchorId,
    pub viewport: TextInputViewport,
    pub focused: bool,
    pub caret_visible: bool,
    pub selected_range: Option<Range<usize>>,
    pub active_offset: Option<usize>,
    pub marked_text: Option<String>,
    pub update_anchor: Rc<dyn Fn(TextInputAnchor)>,
}

#[derive(Clone)]
pub enum FilesystemAction {
    FocusSearch(MouseDownEvent),
    SelectFilter(FilesystemFilter),
    ToggleRow(usize),
    Refresh,
    CopyPath(String),
    OpenDiagnostic(String),
}

#[derive(Clone, Copy)]
pub enum FilesystemStatusIcon {
    Drive,
    Warning,
}

type ActionHandler = Rc<dyn Fn(FilesystemAction, &mut Window, &mut App)>;

pub struct FilesystemPanelView<'a> {
    pub state: &'a FilesystemPanel,
    pub connection_id: &'a str,
    pub search: FilesystemSearchFrame,
    pub sidebar_width: f32,
    pub tokens: &'a ThemeTokens,
    pub i18n: &'a I18n,
    pub mono_font_family: SharedString,
    pub render_status: &'a dyn Fn(FilesystemStatusIcon, u32, String, &mut App) -> AnyElement,
    pub on_action: ActionHandler,
}

fn mouse_action(
    handler: &ActionHandler,
    action: FilesystemAction,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    let handler = handler.clone();
    move |_, window, cx| {
        handler(action.clone(), window, cx);
        cx.stop_propagation();
    }
}

impl FilesystemPanelView<'_> {
    pub fn render(self, connection_switcher: AnyElement, cx: &mut App) -> AnyElement {
        let tokens = self.tokens;
        let i18n = self.i18n;
        let selected_id = self.connection_id;
        let rows = self.state.visible_rows(selected_id);
        let status = self
            .state
            .snapshot_for(selected_id)
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();

        div()
            .id("host-filesystems-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(connection_switcher)
                    .child(self.render_search())
                    .child(self.render_host_filesystem_filter_row(tokens, i18n))
                    .child(self.render_host_filesystem_status_row(
                        rows.len(),
                        selected_id.to_owned(),
                        status.clone(),
                        tokens,
                        i18n,
                    )),
            )
            .child(self.render_host_filesystem_list(
                rows,
                self.state.refresh_in_flight(),
                status,
                selected_id,
                self.sidebar_width,
                tokens,
                i18n,
                self.mono_font_family.clone(),
                cx,
            ))
            .into_any_element()
    }

    fn mouse_action(
        &self,
        action: FilesystemAction,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        mouse_action(&self.on_action, action)
    }

    fn render_search(&self) -> AnyElement {
        let ime = &self.search;
        let update_anchor = ime.update_anchor.clone();
        let on_action = self.on_action.clone();
        text_input_anchor_probe(
            ime.anchor_id,
            text_input_with_viewport_and_content_align(
                self.tokens,
                TextInputView {
                    value: self.state.query(),
                    placeholder: self.i18n.t("sidebar.host_filesystems.search_placeholder"),
                    focused: ime.focused,
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
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    on_action(FilesystemAction::FocusSearch(event.clone()), window, cx);
                    cx.stop_propagation();
                },
            ),
            move |anchor, _window, _cx| update_anchor(anchor),
        )
        .into_any_element()
    }

    fn render_host_filesystem_filter_row(&self, tokens: &ThemeTokens, i18n: &I18n) -> AnyElement {
        let mut row = div()
            .id("host-filesystem-filter-scroll")
            .flex()
            .items_center()
            .gap_1()
            .overflow_x_scroll();
        for filter in [
            FilesystemFilter::All,
            FilesystemFilter::Attention,
            FilesystemFilter::Mounts,
            FilesystemFilter::ReadOnly,
            FilesystemFilter::HighUsage,
            FilesystemFilter::InodePressure,
            FilesystemFilter::InodeHotspots,
            FilesystemFilter::LargeItems,
            FilesystemFilter::Blocks,
        ] {
            row = row.child(self.render_host_filesystem_filter_chip(filter, tokens, i18n));
        }
        row.into_any_element()
    }

    fn render_host_filesystem_filter_chip(
        &self,
        filter: FilesystemFilter,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let active = self.state.filter() == filter;
        host_filesystem_filter_chip(active, tokens)
            .child(i18n.t(filesystem_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                self.mouse_action(FilesystemAction::SelectFilter(filter)),
            )
            .into_any_element()
    }
    fn render_host_filesystem_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceFilesystemStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
        let capability_label = match status {
            ResourceFilesystemStatus::Available {
                capability: FilesystemCommandCapability::Full,
                ..
            } => i18n.t("sidebar.host_filesystems.capability.full"),
            ResourceFilesystemStatus::Available {
                capability: FilesystemCommandCapability::Partial,
                ..
            } => i18n.t("sidebar.host_filesystems.capability.partial"),
            _ => i18n.t("sidebar.host_filesystems.capability.unknown"),
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(div().min_w_0().flex_1().truncate().child(format!(
                "{} {} · {}",
                visible_count,
                i18n.t("sidebar.host_filesystems.count_suffix"),
                capability_label
            )))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(icon_button_with_tooltip(
                        tokens,
                        "lucide/terminal.svg",
                        13.0,
                        rgb(theme.text),
                        IconButtonOptions {
                            size: 24.0,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_filesystems.actions.diagnostic"),
                        "host-filesystem-diagnostic",
                        true,
                        self.mouse_action(FilesystemAction::OpenDiagnostic(selected_id)),
                    ))
                    .child(icon_button_with_tooltip(
                        tokens,
                        "lucide/refresh-cw.svg",
                        13.0,
                        rgb(theme.text),
                        IconButtonOptions {
                            size: 24.0,
                            disabled: self.state.refresh_in_flight(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_filesystems.actions.refresh"),
                        "host-filesystem-refresh",
                        true,
                        self.mouse_action(FilesystemAction::Refresh),
                    )),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_filesystem_list(
        &self,
        rows: Vec<ResourceFilesystemEntry>,
        loading: bool,
        status: ResourceFilesystemStatus,
        selected_id: &str,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut App,
    ) -> AnyElement {
        let state = self.state.list_state(&rows, selected_id);
        if loading && rows.is_empty() {
            return (self.render_status)(
                FilesystemStatusIcon::Drive,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_filesystems.loading"),
                cx,
            );
        }
        match status {
            ResourceFilesystemStatus::Unavailable => {
                return (self.render_status)(
                    FilesystemStatusIcon::Drive,
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_filesystems.unavailable"),
                    cx,
                );
            }
            ResourceFilesystemStatus::Error { message } => {
                return (self.render_status)(
                    FilesystemStatusIcon::Warning,
                    MONITOR_RED,
                    host_filesystem_i18n_replace(
                        i18n,
                        "sidebar.host_filesystems.error",
                        &[("error", message)],
                    ),
                    cx,
                );
            }
            ResourceFilesystemStatus::Unknown | ResourceFilesystemStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return (self.render_status)(
                FilesystemStatusIcon::Drive,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_filesystems.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = selected_id.to_owned();
        let actions = self.on_action.clone();
        let expanded_index = self.state.expanded_index();
        let tokens = *tokens;
        let i18n = i18n.clone();
        let show_context_columns = sidebar_width >= HOST_FILESYSTEM_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(Self::render_host_filesystem_table_header(
                show_context_columns,
                &tokens,
                &i18n,
            ))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        filesystem_list_spec(),
                        move |index, _window, _cx| {
                            Self::render_host_filesystem_row(
                                &actions,
                                &selected_id,
                                index,
                                rows.get(index),
                                expanded_index == Some(index),
                                show_context_columns,
                                &tokens,
                                &i18n,
                                mono_font_family.clone(),
                            )
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_host_filesystem_table_header(
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_FILESYSTEM_TABLE_HEADER_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg))
            .text_size(px(FILESYSTEM_TABLE_HEADER_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(i18n.t("sidebar.host_filesystems.columns.path")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_FILESYSTEM_KIND_COLUMN_WIDTH))
                    .truncate()
                    .child(i18n.t("sidebar.host_filesystems.columns.kind")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_FILESYSTEM_USAGE_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_filesystems.columns.usage")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_FILESYSTEM_INODE_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_filesystems.columns.inode")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_FS_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_filesystems.columns.fs")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_SIZE_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .child(i18n.t("sidebar.host_filesystems.columns.size")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_RO_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_filesystems.columns.read_only")),
                    )
            })
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_filesystem_row(
        actions: &ActionHandler,
        connection_id: &str,
        index: usize,
        entry: Option<&ResourceFilesystemEntry>,
        expanded: bool,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let theme = tokens.ui;
        let kind = host_filesystem_kind_display(i18n, &entry.kind);
        let usage = host_filesystem_usage_label(i18n, entry);
        let inode = host_filesystem_percent_dash(&entry.inode_percent);
        let size = host_filesystem_size_label(&entry.size_bytes);
        let read_only = host_filesystem_read_only_display(i18n, entry.read_only);

        div()
            .id((
                ElementId::from((
                    ElementId::from("host-filesystem-row"),
                    connection_id.to_owned(),
                )),
                SharedString::from(entry.id.clone()),
            ))
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(HOST_FILESYSTEM_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Path is the identity column. Keep it first-level flex so
                    // fixed filesystem metadata cannot collapse it during sidebar resize.
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(FILESYSTEM_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(host_filesystem_path_color(entry, theme.text)))
                            .font_family(mono_font_family.clone())
                            .child(host_filesystem_blank_dash(&entry.path)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_KIND_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(FILESYSTEM_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font_family.clone())
                            .child(kind),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_USAGE_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(FILESYSTEM_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_filesystem_percent_color(
                                &entry.used_percent,
                                theme.text_muted,
                            )))
                            .font_family(mono_font_family.clone())
                            .child(usage),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_INODE_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(FILESYSTEM_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_filesystem_percent_color(
                                &entry.inode_percent,
                                theme.text_muted,
                            )))
                            .font_family(mono_font_family.clone())
                            .child(inode),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_FILESYSTEM_FS_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(FILESYSTEM_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font_family.clone())
                                .child(host_filesystem_blank_dash(&entry.fs_type)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_FILESYSTEM_SIZE_COLUMN_WIDTH))
                                .flex()
                                .justify_end()
                                .truncate()
                                .text_size(px(FILESYSTEM_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font_family.clone())
                                .child(size.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_FILESYSTEM_RO_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(FILESYSTEM_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(if entry.read_only {
                                    MONITOR_AMBER
                                } else {
                                    theme.text_muted
                                }))
                                .font_family(mono_font_family.clone())
                                .child(read_only.clone()),
                        )
                    }),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(FILESYSTEM_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font_family.clone())
                            .child(host_filesystem_meta_label(
                                i18n,
                                &entry,
                                show_context_columns,
                            )),
                    )
                    .child(Self::render_host_filesystem_attention_badges(
                        entry, tokens, i18n,
                    ))
                    .child(Self::render_host_filesystem_inline_actions(
                        actions,
                        connection_id,
                        entry,
                        tokens,
                        i18n,
                    )),
            )
            .when(expanded, |row| {
                row.child(Self::render_host_filesystem_detail(
                    entry,
                    tokens,
                    i18n,
                    mono_font_family,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                mouse_action(actions, FilesystemAction::ToggleRow(index)),
            )
            .into_any_element()
    }

    fn render_host_filesystem_inline_actions(
        actions: &ActionHandler,
        connection_id: &str,
        entry: &ResourceFilesystemEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
        let path = entry.path.clone();
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(icon_button_with_tooltip(
                tokens,
                "lucide/copy.svg",
                12.0,
                rgb(theme.text),
                IconButtonOptions {
                    size: 22.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_filesystems.actions.copy_path"),
                "host-filesystem-copy-path",
                true,
                mouse_action(actions, FilesystemAction::CopyPath(path)),
            ))
            .child(icon_button_with_tooltip(
                tokens,
                "lucide/terminal.svg",
                12.0,
                rgb(theme.text),
                IconButtonOptions {
                    size: 22.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..IconButtonOptions::compact(22.0)
                },
                i18n.t("sidebar.host_filesystems.actions.diagnostic"),
                "host-filesystem-row-diagnostic",
                true,
                mouse_action(
                    actions,
                    FilesystemAction::OpenDiagnostic(connection_id.to_owned()),
                ),
            ))
            .into_any_element()
    }

    fn render_host_filesystem_attention_badges(
        entry: &ResourceFilesystemEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let keys = filesystem_attention_label_keys(entry);
        if keys.is_empty() {
            return div().into_any_element();
        }
        let severity = filesystem_entry_severity(entry);
        let color = match severity {
            FilesystemEntrySeverity::Critical => MONITOR_RED,
            FilesystemEntrySeverity::Warning => MONITOR_AMBER,
            FilesystemEntrySeverity::Normal => tokens.ui.text_muted,
        };
        let mut row = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .overflow_hidden();
        for key in keys.into_iter().take(2) {
            row = row.child(
                div()
                    .flex_none()
                    .h(px(20.0))
                    .px_1p5()
                    .flex()
                    .items_center()
                    .rounded(px(10.0))
                    .bg(rgba((color << 8) | MONITOR_TINT_ALPHA))
                    .text_size(px(10.0))
                    .text_color(rgb(color))
                    .child(i18n.t(key)),
            );
        }
        row.into_any_element()
    }

    fn render_host_filesystem_detail(
        entry: &ResourceFilesystemEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .mx_3()
            .mb_2()
            .rounded(px(tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .overflow_x_scrollbar()
            .child(
                div()
                    .p_3()
                    .min_w(px(700.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .font_family(mono_font_family)
                    .text_size(px(FILESYSTEM_DETAIL_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.path"),
                        host_filesystem_blank_dash(&entry.path)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.kind"),
                        host_filesystem_kind_display(i18n, &entry.kind)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.device"),
                        host_filesystem_blank_dash(&entry.device)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.fs"),
                        host_filesystem_blank_dash(&entry.fs_type)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.size"),
                        host_filesystem_size_label(&entry.size_bytes)
                    ))
                    .child(format!(
                        "{}: {} / {}",
                        i18n.t("sidebar.host_filesystems.columns.used_available"),
                        host_filesystem_size_label(&entry.used_bytes),
                        host_filesystem_size_label(&entry.available_bytes)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.usage"),
                        host_filesystem_percent_dash(&entry.used_percent)
                    ))
                    .child(format!(
                        "{}: {} / {} / {}",
                        i18n.t("sidebar.host_filesystems.columns.inode"),
                        host_filesystem_blank_dash(&entry.inode_used),
                        host_filesystem_blank_dash(&entry.inode_available),
                        host_filesystem_percent_dash(&entry.inode_percent)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.read_only"),
                        host_filesystem_read_only_display(i18n, entry.read_only)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.attention"),
                        host_filesystem_attention_summary(i18n, entry)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.source"),
                        host_filesystem_blank_dash(&entry.source)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.detail"),
                        host_filesystem_blank_dash(&entry.detail)
                    ))
                    .child(div().pt_2().whitespace_nowrap().child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.options"),
                        host_filesystem_blank_dash(&entry.options)
                    ))),
            )
            .into_any_element()
    }
}

fn host_filesystem_blank_dash(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        "—".to_string()
    } else {
        trimmed.to_string()
    }
}

fn host_filesystem_kind_display(i18n: &I18n, kind: &str) -> String {
    let key = filesystem_kind_label_key(kind);
    if key == "sidebar.host_filesystems.kinds.unknown" && !kind.trim().is_empty() {
        kind.trim().to_string()
    } else {
        i18n.t(key)
    }
}

fn host_filesystem_read_only_display(i18n: &I18n, read_only: bool) -> String {
    i18n.t(filesystem_read_only_label_key(read_only))
}

fn host_filesystem_usage_label(i18n: &I18n, entry: &ResourceFilesystemEntry) -> String {
    if entry.kind == "mount" {
        return host_filesystem_percent_dash(&entry.used_percent);
    }
    if entry.kind == "inode_dir" {
        return host_filesystem_i18n_replace(
            i18n,
            "sidebar.host_filesystems.values.inode_count",
            &[("count", host_filesystem_blank_dash(&entry.inode_used))],
        );
    }
    if entry.kind == "count_dir" {
        return host_filesystem_i18n_replace(
            i18n,
            "sidebar.host_filesystems.values.file_count",
            &[("count", host_filesystem_blank_dash(&entry.inode_used))],
        );
    }
    host_filesystem_size_label(&entry.size_bytes)
}

fn host_filesystem_i18n_replace(i18n: &I18n, key: &str, replacements: &[(&str, String)]) -> String {
    let mut text = i18n.t(key);
    for (name, value) in replacements {
        text = text.replace(&format!("{{{{{name}}}}}"), value);
    }
    text
}

fn host_filesystem_percent_dash(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('%');
    if trimmed.is_empty() {
        "—".to_string()
    } else {
        format!("{trimmed}%")
    }
}

fn host_filesystem_size_label(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return "—".to_string();
    }
    match trimmed.parse::<u64>() {
        Ok(bytes) => format_bytes(bytes),
        Err(_) => trimmed.to_string(),
    }
}

fn host_filesystem_path_color(entry: &ResourceFilesystemEntry, default_color: u32) -> u32 {
    match filesystem_entry_severity(entry) {
        FilesystemEntrySeverity::Critical => MONITOR_RED,
        FilesystemEntrySeverity::Warning => MONITOR_AMBER,
        FilesystemEntrySeverity::Normal => default_color,
    }
}

fn host_filesystem_percent_color(value: &str, muted_color: u32) -> u32 {
    match filesystem_percent_severity(value) {
        FilesystemEntrySeverity::Critical => MONITOR_RED,
        FilesystemEntrySeverity::Warning => MONITOR_AMBER,
        FilesystemEntrySeverity::Normal if host_filesystem_percent_value(value) > 0 => {
            MONITOR_EMERALD
        }
        FilesystemEntrySeverity::Normal => muted_color,
    }
}

fn host_filesystem_percent_value(value: &str) -> u32 {
    value
        .trim()
        .trim_end_matches('%')
        .split('.')
        .next()
        .unwrap_or_default()
        .parse::<u32>()
        .unwrap_or(0)
}

fn host_filesystem_filter_chip(active: bool, tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    div()
        .flex_none()
        .h(px(tokens.metrics.ui_button_sm_height * 0.75))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.md))
        .cursor_pointer()
        .bg(if active {
            rgb(theme.bg_hover)
        } else {
            rgba(0x00000000)
        })
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(if active {
            rgb(theme.text)
        } else {
            rgb(theme.text_muted)
        })
        .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
}

fn host_filesystem_meta_label(
    i18n: &I18n,
    entry: &ResourceFilesystemEntry,
    show_context_columns: bool,
) -> String {
    if show_context_columns {
        return format!(
            "{} · {}",
            i18n.t("sidebar.host_filesystems.columns.source"),
            host_filesystem_blank_dash(&entry.source)
        );
    }
    let device_or_detail = if !entry.device.trim().is_empty() {
        entry.device.as_str()
    } else if !entry.detail.trim().is_empty() {
        entry.detail.as_str()
    } else {
        entry.source.as_str()
    };
    format!(
        "{} · {}",
        host_filesystem_blank_dash(device_or_detail),
        host_filesystem_blank_dash(&entry.options)
    )
}

fn host_filesystem_attention_summary(i18n: &I18n, entry: &ResourceFilesystemEntry) -> String {
    let labels = filesystem_attention_label_keys(entry)
        .into_iter()
        .map(|key| i18n.t(key))
        .collect::<Vec<_>>();
    if labels.is_empty() {
        "—".to_string()
    } else {
        labels.join(" · ")
    }
}
