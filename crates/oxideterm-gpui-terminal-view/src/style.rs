use crate::{
    metrics::TerminalMetrics,
    theme::{TerminalUiTheme, terminal_color_from_hex},
};
use gpui::{
    Font, FontStyle, FontWeight, Hsla, Pixels, Rgba, StrikethroughStyle, TextRun, UnderlineStyle,
    px, rgb, rgba,
};
use oxideterm_terminal_emulator::{TerminalCell, TerminalColor, TerminalUnderlineStyle};

/// Non-GPUI underline styles (`Double`, `Dotted`, `Dashed`) painted with pixel-snapped quads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CustomUnderline {
    pub style: TerminalUnderlineStyle,
    pub color: Hsla,
    pub explicit_color: bool,
}

fn resolved_underline_style(cell: &TerminalCell, link: bool) -> TerminalUnderlineStyle {
    match cell.attrs.underline_style() {
        TerminalUnderlineStyle::None if link => TerminalUnderlineStyle::Single,
        style => style,
    }
}

fn resolved_underline_color(cell: &TerminalCell, color: Hsla, link: bool) -> (Hsla, bool) {
    if link {
        (rgb(0x61afef).into(), true)
    } else if let Some(underline_color) = cell.underline_color() {
        (to_hsla(underline_color), true)
    } else {
        (color, false)
    }
}

pub fn custom_underline_for_cell(
    cell: &TerminalCell,
    color: Hsla,
    link: bool,
) -> Option<CustomUnderline> {
    let style = resolved_underline_style(cell, link);
    match style {
        TerminalUnderlineStyle::Double
        | TerminalUnderlineStyle::Dotted
        | TerminalUnderlineStyle::Dashed => {
            let (ul_color, explicit_color) = resolved_underline_color(cell, color, link);
            Some(CustomUnderline {
                style,
                color: ul_color,
                explicit_color,
            })
        }
        TerminalUnderlineStyle::None
        | TerminalUnderlineStyle::Single
        | TerminalUnderlineStyle::Curly => None,
    }
}

pub fn text_run_for_cell(
    cell: &TerminalCell,
    color: Hsla,
    link: bool,
    font: &Font,
    letter_spacing: Option<Pixels>,
) -> TextRun {
    let text_len = cell.ch.len_utf8() + cell.zerowidth().len();
    let weight = if cell.attrs.bold() {
        FontWeight(font.weight.0.max(FontWeight::BOLD.0))
    } else {
        font.weight
    };
    let style = if cell.attrs.italic() {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    let (ul_color, _) = resolved_underline_color(cell, color, link);
    let underline = match resolved_underline_style(cell, link) {
        TerminalUnderlineStyle::Single => Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(ul_color),
            wavy: false,
        }),
        TerminalUnderlineStyle::Curly => Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(ul_color),
            wavy: true,
        }),
        TerminalUnderlineStyle::None
        | TerminalUnderlineStyle::Double
        | TerminalUnderlineStyle::Dotted
        | TerminalUnderlineStyle::Dashed => None,
    };

    TextRun {
        len: text_len,
        font: Font {
            family: font.family.clone(),
            features: font.features.clone(),
            fallbacks: font.fallbacks.clone(),
            weight,
            style,
        },
        color: if link { rgb(0x61afef).into() } else { color },
        background_color: None,
        underline,
        strikethrough: cell.attrs.strikeout().then_some(StrikethroughStyle {
            thickness: px(1.0),
            color: Some(color),
        }),
        letter_spacing,
    }
}

pub fn marked_text_run(
    text: &str,
    foreground: u32,
    background: u32,
    font: &Font,
    letter_spacing: Option<Pixels>,
) -> TextRun {
    let color = rgb(foreground).into();
    TextRun {
        len: text.len(),
        font: font.clone(),
        color,
        background_color: Some(rgb(background).into()),
        underline: Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        }),
        strikethrough: None,
        letter_spacing,
    }
}

pub fn ghost_text_run(
    text: &str,
    foreground: u32,
    font: &Font,
    letter_spacing: Option<Pixels>,
) -> TextRun {
    TextRun {
        len: text.len(),
        font: font.clone(),
        color: rgba((foreground << 8) | 0x66).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing,
    }
}

pub fn timestamp_text_run(text: &str, header_foreground: u32, font: &Font) -> TextRun {
    let mut font = font.clone();
    font.features = oxideterm_gpui_ui::tabular_nums_features();
    TextRun {
        len: text.len(),
        font,
        color: rgba((header_foreground << 8) | 0xcc).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing: None,
    }
}

pub fn text_run_style_matches(left: &TextRun, right: &TextRun) -> bool {
    left.font == right.font
        && left.color == right.color
        && left.background_color == right.background_color
        && left.underline == right.underline
        && left.strikethrough == right.strikethrough
        && left.letter_spacing == right.letter_spacing
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerlineDirection {
    Right,
    Left,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerlineWeight {
    Filled,
    Thin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerlineShape {
    Triangle,
    HalfCircle,
    SlantedLower,
    SlantedUpper,
    Trapezoid,
    InverseTriangle,
    PixelatedSmall,
    PixelatedBig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PowerlineSeparator {
    pub direction: PowerlineDirection,
    pub weight: PowerlineWeight,
    pub shape: PowerlineShape,
}

impl PowerlineSeparator {
    pub const fn supports_background_cutout(self) -> bool {
        matches!(self.weight, PowerlineWeight::Filled)
            && !matches!(
                self.shape,
                PowerlineShape::PixelatedSmall | PowerlineShape::PixelatedBig
            )
    }
}

pub fn powerline_background_cutout(
    cell: &TerminalCell,
    transparent_background: bool,
    foreground_overridden: bool,
) -> Option<char> {
    if !transparent_background
        || foreground_overridden
        || !cell.style_origin.foreground_is_default_background()
        || !cell.zerowidth().is_empty()
    {
        return None;
    }
    let separator = powerline_separator(cell.ch)?;
    separator.supports_background_cutout().then_some(cell.ch)
}

pub fn powerline_separator(ch: char) -> Option<PowerlineSeparator> {
    let sep = |direction, weight, shape| {
        Some(PowerlineSeparator {
            direction,
            weight,
            shape,
        })
    };
    match ch as u32 {
        0xe0b0 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Filled,
            PowerlineShape::Triangle,
        ),
        0xe0b1 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Thin,
            PowerlineShape::Triangle,
        ),
        0xe0b2 => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Filled,
            PowerlineShape::Triangle,
        ),
        0xe0b3 => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Thin,
            PowerlineShape::Triangle,
        ),
        0xe0b4 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Filled,
            PowerlineShape::HalfCircle,
        ),
        0xe0b5 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Thin,
            PowerlineShape::HalfCircle,
        ),
        0xe0b6 => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Filled,
            PowerlineShape::HalfCircle,
        ),
        0xe0b7 => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Thin,
            PowerlineShape::HalfCircle,
        ),
        0xe0b8 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Filled,
            PowerlineShape::SlantedLower,
        ),
        0xe0b9 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Thin,
            PowerlineShape::SlantedLower,
        ),
        0xe0ba => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Filled,
            PowerlineShape::SlantedLower,
        ),
        0xe0bb => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Thin,
            PowerlineShape::SlantedLower,
        ),
        0xe0bc => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Filled,
            PowerlineShape::SlantedUpper,
        ),
        0xe0bd => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Thin,
            PowerlineShape::SlantedUpper,
        ),
        0xe0be => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Filled,
            PowerlineShape::SlantedUpper,
        ),
        0xe0bf => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Thin,
            PowerlineShape::SlantedUpper,
        ),
        0xe0c4 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Filled,
            PowerlineShape::PixelatedSmall,
        ),
        0xe0c5 => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Filled,
            PowerlineShape::PixelatedSmall,
        ),
        0xe0c6 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Filled,
            PowerlineShape::PixelatedBig,
        ),
        0xe0c7 => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Filled,
            PowerlineShape::PixelatedBig,
        ),
        0xe0d2 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Filled,
            PowerlineShape::Trapezoid,
        ),
        0xe0d4 => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Filled,
            PowerlineShape::Trapezoid,
        ),
        0xe0d6 => sep(
            PowerlineDirection::Right,
            PowerlineWeight::Filled,
            PowerlineShape::InverseTriangle,
        ),
        0xe0d7 => sep(
            PowerlineDirection::Left,
            PowerlineWeight::Filled,
            PowerlineShape::InverseTriangle,
        ),
        _ => None,
    }
}

pub fn to_rgba(color: TerminalColor) -> Rgba {
    Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: 1.0,
    }
}

pub fn to_hsla(color: TerminalColor) -> Hsla {
    to_rgba(color).into()
}

pub fn canvas_text_run_for_cell(
    cell: &TerminalCell,
    color: Hsla,
    link: bool,
    metrics: &TerminalMetrics,
) -> TextRun {
    text_run_for_cell(cell, color, link, &metrics.font, metrics.letter_spacing)
}

pub fn canvas_marked_text_run(
    text: &str,
    theme: &TerminalUiTheme,
    metrics: &TerminalMetrics,
) -> TextRun {
    marked_text_run(
        text,
        theme.foreground,
        theme.background,
        &metrics.font,
        metrics.letter_spacing,
    )
}

pub fn canvas_ghost_text_run(
    text: &str,
    theme: &TerminalUiTheme,
    metrics: &TerminalMetrics,
) -> TextRun {
    ghost_text_run(
        text,
        theme.foreground,
        &metrics.font,
        metrics.letter_spacing,
    )
}

pub fn canvas_timestamp_text_run(
    text: &str,
    theme: &TerminalUiTheme,
    metrics: &TerminalMetrics,
) -> TextRun {
    timestamp_text_run(text, theme.header_foreground, &metrics.font)
}

pub fn terminal_background(theme: &TerminalUiTheme) -> Hsla {
    to_hsla(terminal_color_from_hex(theme.background))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_runs_propagate_metrics_letter_spacing() {
        let font = gpui::font("monospace");
        let spacing = Some(px(1.0));
        let cell = TerminalCell {
            ch: 'a',
            wide: false,
            fg: TerminalColor::rgb(255, 255, 255),
            bg: TerminalColor::rgb(0, 0, 0),
            style_origin: Default::default(),
            attrs: Default::default(),
            extra: None,
            cursor: false,
        };

        assert_eq!(
            text_run_for_cell(&cell, gpui::white(), false, &font, spacing).letter_spacing,
            Some(px(1.0))
        );
        assert_eq!(
            marked_text_run("ab", 0xffffff, 0, &font, spacing).letter_spacing,
            Some(px(1.0))
        );
        assert_eq!(
            ghost_text_run("ls", 0xffffff, &font, spacing).letter_spacing,
            Some(px(1.0))
        );
        assert_eq!(
            timestamp_text_run("12:00", 0xffffff, &font).letter_spacing,
            None
        );

        let mut curly_cell = cell.clone();
        curly_cell.attrs = curly_cell
            .attrs
            .with_underline_style(TerminalUnderlineStyle::Curly);
        let red = TerminalColor::rgb(0xff, 0x55, 0x55);
        curly_cell.set_underline_color(Some(red));
        let curly_run = text_run_for_cell(&curly_cell, gpui::white(), false, &font, spacing);
        assert_eq!(
            curly_run.underline,
            Some(UnderlineStyle {
                thickness: px(1.0),
                color: Some(to_hsla(red)),
                wavy: true,
            })
        );
        assert_eq!(
            custom_underline_for_cell(&curly_cell, gpui::white(), false),
            None
        );

        let mut single_cell = cell.clone();
        single_cell.attrs = single_cell
            .attrs
            .with_underline_style(TerminalUnderlineStyle::Single);
        let single_run = text_run_for_cell(&single_cell, gpui::white(), false, &font, spacing);
        assert!(!text_run_style_matches(&single_run, &curly_run));

        let mut double_cell = cell;
        double_cell.attrs = double_cell
            .attrs
            .with_underline_style(TerminalUnderlineStyle::Double);
        double_cell.set_underline_color(Some(red));
        let double_run = text_run_for_cell(&double_cell, gpui::white(), false, &font, spacing);
        assert_eq!(double_run.underline, None);
        assert_eq!(
            custom_underline_for_cell(&double_cell, gpui::white(), false),
            Some(CustomUnderline {
                style: TerminalUnderlineStyle::Double,
                color: to_hsla(red),
                explicit_color: true,
            })
        );
    }
}
