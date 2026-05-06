use gpui::{
    AnyElement, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Rgba,
    SharedString, Styled, div, prelude::*, px, rgb, rgba,
};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug)]
pub struct TauriTableColors {
    pub header_border: Rgba,
    pub header_bg: Rgba,
    pub row_border: Rgba,
    pub row_hover_bg: Rgba,
    pub row_selected_bg: Rgba,
}

#[derive(Clone, Copy, Debug)]
pub struct TauriTableMetrics {
    pub header_min_height: f32,
    pub row_min_height: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub header_text_size: f32,
}

impl Default for TauriTableMetrics {
    fn default() -> Self {
        Self {
            header_min_height: 35.0,
            row_min_height: 36.0,
            padding_x: 8.0,
            padding_y: 6.0,
            header_text_size: 12.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TauriTableCellStyle {
    Primary,
    Meta,
    MetaMono,
}

#[derive(Clone, Debug)]
pub struct TauriTableCellOptions {
    pub width: f32,
    pub min_width: f32,
    pub flexible: bool,
    pub padding_left: f32,
    pub primary_text_size: f32,
    pub meta_text_size: f32,
    pub mono_font: Option<SharedString>,
}

pub fn tauri_table_header(
    tokens: &ThemeTokens,
    colors: TauriTableColors,
    metrics: TauriTableMetrics,
) -> Div {
    div()
        .min_h(px(metrics.header_min_height))
        .flex()
        .items_center()
        .px(px(metrics.padding_x))
        .py(px(metrics.padding_y))
        .border_b_1()
        .border_color(colors.header_border)
        .bg(colors.header_bg)
        .text_size(px(metrics.header_text_size))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(tokens.ui.text_muted))
}

pub fn tauri_table_row(
    colors: TauriTableColors,
    metrics: TauriTableMetrics,
    selected: bool,
) -> Div {
    div()
        .relative()
        .min_h(px(metrics.row_min_height))
        .flex()
        .items_center()
        .px(px(metrics.padding_x))
        .py(px(metrics.padding_y))
        .border_b_1()
        .border_color(colors.row_border)
        .bg(if selected {
            colors.row_selected_bg
        } else {
            rgba(0x00000000)
        })
        .hover(move |row| row.bg(colors.row_hover_bg))
}

pub fn tauri_table_checkbox_cell(width: f32, child: impl IntoElement) -> AnyElement {
    div()
        .w(px(width))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(child)
        .into_any_element()
}

pub fn tauri_table_spacer_cell(width: f32) -> AnyElement {
    div().w(px(width)).flex_none().into_any_element()
}

pub fn tauri_table_cell(
    tokens: &ThemeTokens,
    options: &TauriTableCellOptions,
    style: TauriTableCellStyle,
    text: impl Into<String>,
) -> AnyElement {
    let strong = style == TauriTableCellStyle::Primary;
    let cell = div()
        .when(options.flexible, |cell| {
            cell.flex_1().min_w(px(options.min_width))
        })
        .when(!options.flexible, |cell| {
            cell.w(px(options.width)).flex_none()
        })
        .pl(px(options.padding_left))
        .truncate()
        .text_size(px(match style {
            TauriTableCellStyle::Primary => options.primary_text_size,
            TauriTableCellStyle::Meta | TauriTableCellStyle::MetaMono => options.meta_text_size,
        }))
        .font_weight(if strong {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        })
        .text_color(if strong {
            rgb(tokens.ui.text)
        } else {
            rgb(tokens.ui.text_muted)
        })
        .when(style == TauriTableCellStyle::MetaMono, |cell| {
            if let Some(font) = options.mono_font.clone() {
                cell.font_family(font)
            } else {
                cell
            }
        });
    cell.child(text.into()).into_any_element()
}

pub fn tauri_table_sort_header(
    tokens: &ThemeTokens,
    options: &TauriTableCellOptions,
    label: impl Into<String>,
    icon: impl IntoElement,
) -> Div {
    div()
        .when(options.flexible, |cell| {
            cell.flex_1().min_w(px(options.min_width))
        })
        .when(!options.flexible, |cell| {
            cell.w(px(options.width)).flex_none()
        })
        .pl(px(options.padding_left))
        .flex()
        .items_center()
        .gap(px(4.0))
        .cursor_pointer()
        .hover(move |cell| cell.text_color(rgb(tokens.ui.text)))
        .child(div().truncate().child(label.into()))
        .child(icon)
}
