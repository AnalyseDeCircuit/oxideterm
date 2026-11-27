//! Window-local command picker. Native providers retain their executable payloads.
pub mod model;
mod session;
mod state;
use gpui::{
    AnyElement, App, Context, CursorStyle, Entity, MouseButton, MouseDownEvent, Rgba, SharedString,
    Window, div, prelude::*, px, rgb, rgba, svg,
};
use model::*;
use oxideterm_gpui_ui::{
    icons::LucideIcon,
    modal::{
        TauriBackdropRole, dialog_content, overlay_content_boundary, rounded_shell_child_radius,
        staged_overlay_root, staged_overlay_scrim,
    },
    motion::{ExitPhase, MotionDuration},
    virtual_list::*,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::CommandPaletteMode as PaletteMode;
pub use session::*;
pub use state::*;
use std::{collections::HashSet, rc::Rc, sync::Arc};

#[derive(Clone)]
pub enum ShellPaletteRequest {
    Execute(String),
    Close,
    QueryChanged(String),
    Select(usize),
}

pub fn key_request<A>(
    stroke: &gpui::Keystroke,
    query: &str,
    selected: Option<usize>,
    items: impl FnOnce() -> Vec<RankedItem<A>>,
) -> Option<ShellPaletteRequest> {
    use ShellPaletteRequest::*;
    let step = match stroke.key.as_str() {
        "escape" if !stroke.modifiers.platform => return Some(Close),
        "enter" if !stroke.modifiers.platform => {
            let items = items();
            return resolve_command_palette_selected_index(&items, selected)
                .map(|i| Execute(items[i].item.id.clone()));
        }
        "arrowdown" | "down" => PaletteStep::Forward(1),
        "arrowup" | "up" => PaletteStep::Backward(1),
        "pagedown" => PaletteStep::Forward(8),
        "pageup" => PaletteStep::Backward(8),
        "home" => PaletteStep::First,
        "end" => PaletteStep::Last,
        "backspace" if !stroke.modifiers.platform => {
            let mut query = query.to_owned();
            query.pop()?;
            return Some(QueryChanged(query));
        }
        _ => {
            let text = stroke.key_char.as_deref()?;
            return (!stroke.modifiers.platform
                && !stroke.modifiers.control
                && !text.chars().any(char::is_control))
            .then(|| QueryChanged(format!("{query}{text}")));
        }
    };
    let items = items();
    let index = match selected {
        Some(index) => next_enabled_palette_index(&items, index, step),
        None if matches!(step, PaletteStep::Last | PaletteStep::Backward(_)) => {
            items.iter().rposition(|row| !row.item.disabled)
        }
        None => items.iter().position(|row| !row.item.disabled),
    };
    index.map(Select)
}

pub fn query_input(
    tokens: &ThemeTokens,
    i18n: &I18n,
    palette: &CommandPaletteView,
    viewport: &oxideterm_gpui_ui::text_input::TextInputViewport,
    caret_visible: bool,
    selected_range: Option<std::ops::Range<usize>>,
    active_offset: Option<usize>,
    marked_text: Option<&str>,
) -> gpui::Div {
    let placeholder = i18n.t(match palette.content {
        PaletteContent::TabScopes => "sidebar.tab_scope.search",
        PaletteContent::QuickBarCategories => "terminal.quick_commands.search_groups",
        PaletteContent::BackgroundTasks => "sidebar.background_tasks.search",
        PaletteContent::Commands => command_palette_placeholder_key(palette.mode),
    });

    oxideterm_gpui_ui::text_input::text_input_with_viewport(
        tokens,
        oxideterm_gpui_ui::text_input::TextInputView {
            value: &palette.raw_query,
            placeholder,
            focused: true,
            caret_visible,
            secret: false,
            selected_all: false,
            selected_range,
            marked_text,
        },
        viewport,
        active_offset,
    )
    .flex_1()
    .min_w_0()
    .border_0()
    .bg(rgba(0))
    .px(px(0.))
    .text_size(px(tokens.metrics.ui_text_sm))
    .line_height(px(20.))
}
pub type PaletteDispatch = Rc<dyn Fn(ShellPaletteRequest, &mut Window, &mut App)>;
pub struct PaletteFrame<A> {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub mono_font: SharedString,
    pub palette: CommandPaletteView,
    pub ranked_items: Vec<RankedItem<A>>,
    pub selected: Option<usize>,
    pub preview_selected: bool,
    pub titlebar_height: f32,
    pub scope_counts: Option<String>,
    /// Platform text field including selection, marked text and caret hit testing.
    pub input: AnyElement,
}
type FrameSource<A> = dyn Fn(&mut Window, &mut App) -> Option<PaletteFrame<A>>;
pub struct ShellPalette<A: 'static> {
    frame: Box<FrameSource<A>>,
    request: PaletteDispatch,
}
impl<A: 'static> ShellPalette<A> {
    pub fn new<P: PaletteProvider>(
        model: &Entity<PaletteModel<P>>,
        frame: impl Fn(&mut Window, &mut App) -> Option<PaletteFrame<A>> + 'static,
        request: PaletteDispatch,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(model, |_, _, cx| cx.notify()).detach();
        Self {
            frame: Box::new(frame),
            request,
        }
    }
}
impl<A: Clone + 'static> Render for ShellPalette<A> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = (self.frame)(window, cx) else {
            return div().into_any_element();
        };
        let PaletteFrame {
            tokens,
            i18n,
            mono_font,
            palette,
            ranked_items,
            selected,
            preview_selected,
            titlebar_height,
            scope_counts,
            input,
        } = frame;
        let selected_index = resolve_command_palette_selected_index(&ranked_items, selected);
        let preview = selected_index
            .filter(|_| preview_selected)
            .map(|i| command_palette_connection_preview(&tokens, &ranked_items[i].item));
        Rc::new(PaletteStyle {
            tokens,
            i18n,
            mono_font,
            request: self.request.clone(),
            query: palette.raw_query.clone(),
            content: palette.content,
        })
        .panel(
            window,
            palette,
            ranked_items,
            selected_index,
            preview,
            titlebar_height,
            scope_counts,
            input,
        )
    }
}
struct PaletteStyle {
    tokens: ThemeTokens,
    i18n: I18n,
    mono_font: SharedString,
    request: PaletteDispatch,
    query: String,
    content: PaletteContent,
}
fn icon_element(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(color)
        .into_any_element()
}
impl PaletteStyle {
    fn listener(
        &self,
        event: ShellPaletteRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let request = self.request.clone();
        move |_, window, cx| {
            request(event.clone(), window, cx);
            cx.stop_propagation();
        }
    }
    fn panel<A: Clone + 'static>(
        self: Rc<Self>,
        window: &Window,
        palette: CommandPaletteView,
        ranked_items: Vec<RankedItem<A>>,
        selected_index: Option<usize>,
        preview: Option<gpui::Stateful<gpui::Div>>,
        titlebar_height: f32,
        scope_counts: Option<String>,
        input: AnyElement,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let compact = palette.content != PaletteContent::Commands;
        let palette_top = if compact {
            titlebar_height + 4.0
        } else {
            f32::from(viewport.height) * COMMAND_PALETTE_TOP_RATIO
        };
        let preview_height = if preview.is_some() {
            COMMAND_PALETTE_CONNECTION_PREVIEW_HEIGHT
        } else {
            0.0
        };
        let list_max_height = command_palette_list_max_height(
            f32::from(viewport.height),
            palette_top,
            preview_height,
            compact,
        );
        let mode = palette.mode;
        let rows = Arc::new(command_palette_virtual_rows(ranked_items));
        let row_count = rows.len();
        let rows_height =
            (row_count as f32 * COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT).min(list_max_height);
        let virtual_rows = rows;
        let style = self.clone();
        let divider_color = rgba((self.tokens.ui.border << 8) | 0x59);
        let footer_bg = rgba((self.tokens.ui.bg_panel << 8) | 0x26);
        let footer_left = self.i18n.t(if compact {
            "sidebar.tab_scope.hint"
        } else {
            "command_palette.mode_hint"
        });
        let footer_right = if compact {
            scope_counts
        } else {
            Some(self.i18n.t("command_palette.footer_hint"))
        };

        let panel = dialog_content(&self.tokens)
            .w(px(if compact {
                compact_palette_panel_width(f32::from(viewport.width))
            } else {
                command_palette_panel_width(f32::from(viewport.width))
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(COMMAND_PALETTE_INPUT_HEIGHT))
                            .px(px(12.0))
                            .flex()
                            .items_center()
                            .border_b_1()
                            .border_color(divider_color)
                            // The search row is the first painted child inside
                            // the rounded command palette shell; match the
                            // shell's inner curve instead of the outer border.
                            .rounded_t(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                            .when(mode != PaletteMode::All, |row| {
                                row.child(self.render_command_palette_mode_badge(mode))
                            })
                            .child(self.render_command_palette_icon_slot(
                                LucideIcon::Search,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .child(div().ml(px(8.0)).flex_1().min_w_0().child(input)),
                    )
                    .child(
                        div().py(px(COMMAND_PALETTE_LIST_PADDING_Y)).child(
                            div()
                                .relative()
                                .id("command-palette-scroll")
                                .w_full()
                                .h(px(rows_height))
                                .max_h(px(list_max_height))
                                .child(tauri_virtual_uniform_list(
                                    "command-palette-virtual-list",
                                    row_count,
                                    palette.scroll_handle.clone(),
                                    TauriVirtualListSpec::new(
                                        px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT),
                                        COMMAND_PALETTE_VIRTUAL_OVERSCAN,
                                    ),
                                    move |range, _window, _cx| {
                                        range
                                            .map(|row_index| {
                                                let row = virtual_rows[row_index].clone();
                                                style.render_command_palette_virtual_row(
                                                    row,
                                                    selected_index,
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                    },
                                )),
                        ),
                    )
                    .when_some(preview, |root, preview| root.child(preview))
                    .when_some(palette.error, |root, error| {
                        root.child(
                            div()
                                .border_t_1()
                                .border_color(divider_color)
                                .px(px(12.0))
                                .py(px(8.0))
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.error))
                                .child(error),
                        )
                    })
                    .child(
                        div()
                            .border_t_1()
                            .border_color(divider_color)
                            .bg(footer_bg)
                            .rounded_b(px(rounded_shell_child_radius(self.tokens.radii.lg)))
                            .px(px(12.0))
                            .py(px(6.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .text_size(px(self.tokens.metrics.ui_text_2xs))
                            .line_height(px(16.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(footer_left)
                            .when_some(footer_right, |footer, right_label| {
                                footer.child(
                                    div()
                                        .flex_none()
                                        .when(compact, |el| el.font_family(self.mono_font.clone()))
                                        .child(right_label),
                                )
                            }),
                    ),
            );

        let visible = palette.phase == ExitPhase::Visible;
        staged_overlay_root()
            .items_start()
            .justify_center()
            .on_mouse_down(MouseButton::Left, self.listener(ShellPaletteRequest::Close))
            .on_mouse_down(
                MouseButton::Right,
                self.listener(ShellPaletteRequest::Close),
            )
            .child(staged_overlay_scrim(
                &self.tokens,
                ("command-palette-scrim", palette.motion_generation),
                TauriBackdropRole::CommandPalette,
                visible,
            ))
            .child(oxideterm_gpui_ui::motion::zoom_fade_y(
                &self.tokens,
                ("command-palette-panel-motion", palette.motion_generation),
                div()
                    .mt(px(palette_top))
                    .child(overlay_content_boundary(panel)),
                -6.0,
                0.95,
                gpui::point(0.5, 0.0),
                MotionDuration::Control,
                visible,
            ))
            .into_any_element()
    }
    fn render_command_palette_mode_badge(&self, mode: PaletteMode) -> AnyElement {
        div()
            .mr(px(8.0))
            .rounded(px(self.tokens.radii.xs))
            .bg(rgba(
                (self.tokens.ui.accent << 8) | COMMAND_PALETTE_MODE_BADGE_ALPHA,
            ))
            .px(px(6.0))
            .py(px(2.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_family(self.mono_font.clone())
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(self.tokens.ui.accent))
            .child(match mode {
                PaletteMode::All => "",
                PaletteMode::Commands => ">",
                PaletteMode::Sessions => "@",
                PaletteMode::Connections => "#",
            })
            .into_any_element()
    }

    fn render_command_palette_section_heading(
        &self,
        section: PaletteSection,
        show_separator: bool,
    ) -> AnyElement {
        let key = if self.content == PaletteContent::QuickBarCategories {
            "terminal.quick_commands.groups"
        } else if self.content == PaletteContent::TabScopes {
            if section == PaletteSection::Connections {
                "sidebar.tab_scope.connections"
            } else {
                "sidebar.tab_scope.categories"
            }
        } else {
            section_label_key(section)
        };
        let label = self.i18n.t(key);
        div()
            .w_full()
            .h(px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT))
            .px(px(14.0))
            .when(show_separator, |heading| {
                heading
                    .border_t_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x40))
            })
            .flex()
            .items_center()
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .line_height(px(16.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(label)
            .into_any_element()
    }

    fn render_shortcut_keycap(&self, shortcut: String) -> AnyElement {
        div()
            .flex_none()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x80))
            .bg(rgb(self.tokens.ui.bg_panel))
            .px(px(6.0))
            .py(px(2.0))
            .font_family(self.mono_font.clone())
            .text_size(px(self.tokens.metrics.ui_text_2xs))
            .line_height(px(14.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(shortcut)
            .into_any_element()
    }

    fn render_command_palette_row<A>(
        &self,
        ranked: &RankedItem<A>,
        index: usize,
        selected_index: Option<usize>,
    ) -> AnyElement {
        let disabled = ranked.item.disabled;
        let selected = !disabled && selected_index == Some(index);
        let id = ranked.item.id.clone();
        let mut label_row = div().flex().flex_1().items_center().min_w_0().child(
            self.render_highlighted_palette_text(
                &ranked.item.label,
                &ranked.highlights,
                selected,
                disabled,
                false,
            ),
        );
        if let Some(detail) = ranked.item.detail.as_ref() {
            label_row = label_row.child(
                div()
                    .ml(px(8.0))
                    .min_w_0()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(16.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_highlighted_palette_text(
                        detail,
                        &palette_detail_highlights(detail, &self.query),
                        false,
                        disabled,
                        true,
                    )),
            );
        }
        let hover_bg = self.tokens.ui.bg_hover;
        let has_trailing = ranked.item.count.is_some()
            || ranked.item.active
            || disabled
            || ranked.item.shortcut.is_some();

        let row_pill = div()
            .id(("command-palette-row", index))
            .w_full()
            .h_full()
            .px(px(8.0))
            .rounded(px(self.tokens.radii.sm))
            .flex()
            .items_center()
            .gap(px(COMMAND_PALETTE_ITEM_GAP))
            .bg(if selected {
                rgba((self.tokens.ui.accent << 8) | COMMAND_PALETTE_SELECTED_ALPHA)
            } else {
                rgba(0x00000000)
            })
            .text_color(if selected {
                rgb(self.tokens.ui.accent)
            } else if disabled {
                rgb(self.tokens.ui.text_muted)
            } else {
                rgb(self.tokens.ui.text)
            })
            .when(!selected && !disabled, |row| {
                row.hover(move |row| row.bg(rgb(hover_bg)))
            })
            .cursor(if disabled {
                CursorStyle::Arrow
            } else {
                CursorStyle::PointingHand
            })
            .when(!disabled, |row| {
                row.on_mouse_move({
                    let request = self.request.clone();
                    move |_: &gpui::MouseMoveEvent, window, cx| {
                        request(ShellPaletteRequest::Select(index), window, cx)
                    }
                })
                .on_mouse_down(
                    MouseButton::Left,
                    self.listener(ShellPaletteRequest::Execute(id)),
                )
            })
            .child(self.render_command_palette_icon_slot(
                ranked.item.icon,
                if selected {
                    rgb(self.tokens.ui.accent)
                } else {
                    rgb(self.tokens.ui.text_muted)
                },
            ))
            .child(label_row)
            .when(has_trailing, |row| {
                let mut trailing = div()
                    .ml_auto()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0));
                if let Some(count) = ranked.item.count.as_ref() {
                    trailing = trailing.child(
                        div()
                            .font_family(self.mono_font.clone())
                            .text_size(px(self.tokens.metrics.ui_text_2xs))
                            .text_color(rgb(if ranked.item.active {
                                self.tokens.ui.accent
                            } else {
                                self.tokens.ui.text_muted
                            }))
                            .child(count.clone()),
                    );
                }
                if ranked.item.active {
                    trailing = trailing.child(icon_element(
                        LucideIcon::Check,
                        13.0,
                        rgb(self.tokens.ui.accent),
                    ));
                }
                if disabled {
                    trailing = trailing.child(
                        div()
                            .rounded(px(self.tokens.radii.xs))
                            .bg(rgb(self.tokens.ui.bg_panel))
                            .px(px(6.0))
                            .py(px(2.0))
                            .text_size(px(self.tokens.metrics.ui_text_2xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("common.disabled")),
                    );
                }
                if let Some(shortcut) = ranked.item.shortcut.as_ref() {
                    trailing = trailing.child(self.render_shortcut_keycap(shortcut.clone()));
                }
                row.child(trailing)
            });

        div()
            .w_full()
            .h(px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT))
            .px(px(6.0))
            .child(row_pill)
            .into_any_element()
    }

    fn render_command_palette_virtual_row<A>(
        &self,
        row: CommandPaletteVirtualRow<A>,
        selected_index: Option<usize>,
    ) -> AnyElement {
        match row {
            CommandPaletteVirtualRow::Heading {
                section,
                show_separator,
            } => self.render_command_palette_section_heading(section, show_separator),
            CommandPaletteVirtualRow::Item { ranked, item_index } => {
                self.render_command_palette_row(&ranked, item_index, selected_index)
            }
            CommandPaletteVirtualRow::Empty => div()
                .w_full()
                .h(px(COMMAND_PALETTE_VIRTUAL_ROW_HEIGHT))
                .px(px(16.0))
                .flex()
                .items_center()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("command_palette.no_results"))
                .into_any_element(),
        }
    }

    fn render_command_palette_icon_slot(&self, icon: LucideIcon, color: Rgba) -> AnyElement {
        div()
            .w(px(COMMAND_PALETTE_ICON_SLOT))
            .h(px(COMMAND_PALETTE_ICON_SLOT))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(icon_element(icon, COMMAND_PALETTE_ICON_SLOT, color))
            .into_any_element()
    }

    fn render_highlighted_palette_text(
        &self,
        text: &str,
        highlights: &[usize],
        selected: bool,
        disabled: bool,
        muted: bool,
    ) -> AnyElement {
        let highlight_set = highlights.iter().copied().collect::<HashSet<_>>();
        let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
        // Search returns character indices; styled text uses UTF-8 byte ranges.
        for (index, (offset, ch)) in text.char_indices().enumerate() {
            if highlight_set.contains(&index) {
                let end = offset + ch.len_utf8();
                if let Some(previous) = ranges.last_mut()
                    && previous.end == offset
                {
                    previous.end = end;
                } else {
                    ranges.push(offset..end);
                }
            }
        }
        let highlighted = gpui::StyledText::new(text.to_owned()).with_highlights(
            ranges.into_iter().map(|range| {
                (
                    range,
                    gpui::HighlightStyle {
                        color: Some(rgb(self.tokens.ui.accent).into()),
                        font_weight: Some(gpui::FontWeight::SEMIBOLD),
                        ..Default::default()
                    },
                )
            }),
        );
        div()
            .min_w_0()
            .truncate()
            .text_size(px(if muted {
                self.tokens.metrics.ui_text_xs
            } else {
                self.tokens.metrics.ui_text_sm
            }))
            .line_height(px(if muted { 16.0 } else { 20.0 }))
            .text_color(rgb(if selected {
                self.tokens.ui.accent
            } else if muted || disabled {
                self.tokens.ui.text_muted
            } else {
                self.tokens.ui.text
            }))
            .child(highlighted)
            .into_any_element()
    }
}
