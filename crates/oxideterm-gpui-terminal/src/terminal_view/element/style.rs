use gpui::{
    Font, FontStyle, FontWeight, Hsla, Rgba, StrikethroughStyle, TextRun, UnderlineStyle, px, rgb,
    rgba,
};
use oxideterm_terminal::{TerminalCell, TerminalColor, TerminalUnderlineStyle};

use crate::terminal_ui::*;

/// Non-GPUI underline styles (`Double`, `Dotted`, `Dashed`) painted with pixel-snapped quads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CustomUnderline {
    pub(crate) style: TerminalUnderlineStyle,
    pub(crate) color: Hsla,
    pub(crate) explicit_color: bool,
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

pub(crate) fn custom_underline_for_cell(
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

pub(crate) fn text_run_for_cell(
    cell: &TerminalCell,
    color: Hsla,
    link: bool,
    metrics: &TerminalMetrics,
) -> TextRun {
    let text_len = cell.ch.len_utf8() + cell.zerowidth().len();
    let weight = if cell.attrs.bold() {
        FontWeight(metrics.font.weight.0.max(FontWeight::BOLD.0))
    } else {
        metrics.font.weight
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
            family: metrics.font.family.clone(),
            features: metrics.font.features.clone(),
            fallbacks: metrics.font.fallbacks.clone(),
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
        letter_spacing: metrics.letter_spacing,
    }
}

pub(crate) fn marked_text_run(
    text: &str,
    theme: &TerminalUiTheme,
    metrics: &TerminalMetrics,
) -> TextRun {
    let color = rgb(theme.foreground).into();
    TextRun {
        len: text.len(),
        font: metrics.font.clone(),
        color,
        background_color: Some(rgb(theme.background).into()),
        underline: Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        }),
        strikethrough: None,
        letter_spacing: metrics.letter_spacing,
    }
}

pub(crate) fn ghost_text_run(
    text: &str,
    theme: &TerminalUiTheme,
    metrics: &TerminalMetrics,
) -> TextRun {
    TextRun {
        len: text.len(),
        font: metrics.font.clone(),
        color: rgba((theme.foreground << 8) | 0x66).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing: metrics.letter_spacing,
    }
}

pub(crate) fn timestamp_text_run(
    text: &str,
    theme: &TerminalUiTheme,
    metrics: &TerminalMetrics,
) -> TextRun {
    let mut font = metrics.font.clone();
    font.features = oxideterm_gpui_ui::tabular_nums_features();
    TextRun {
        len: text.len(),
        font,
        color: rgba((theme.header_foreground << 8) | 0xcc).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing: None,
    }
}

pub(crate) fn text_run_style_matches(left: &TextRun, right: &TextRun) -> bool {
    left.font == right.font
        && left.color == right.color
        && left.background_color == right.background_color
        && left.underline == right.underline
        && left.strikethrough == right.strikethrough
        && left.letter_spacing == right.letter_spacing
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PowerlineDirection {
    Right,
    Left,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PowerlineWeight {
    Filled,
    Thin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PowerlineShape {
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
pub(crate) struct PowerlineSeparator {
    pub(crate) direction: PowerlineDirection,
    pub(crate) weight: PowerlineWeight,
    pub(crate) shape: PowerlineShape,
}

impl PowerlineSeparator {
    pub(crate) const fn supports_background_cutout(self) -> bool {
        matches!(self.weight, PowerlineWeight::Filled)
            && !matches!(
                self.shape,
                PowerlineShape::PixelatedSmall | PowerlineShape::PixelatedBig
            )
    }
}

pub(crate) fn powerline_background_cutout(
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

pub(crate) fn powerline_separator(ch: char) -> Option<PowerlineSeparator> {
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

pub(crate) fn to_rgba(color: TerminalColor) -> Rgba {
    Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: 1.0,
    }
}

pub(crate) fn to_hsla(color: TerminalColor) -> Hsla {
    to_rgba(color).into()
}

pub(crate) fn terminal_background(theme: &TerminalUiTheme) -> Hsla {
    to_hsla(terminal_color_from_hex(theme.background))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_runs_propagate_metrics_letter_spacing() {
        let font = terminal_font_with_family_and_cjk(
            TERMINAL_FONT,
            None,
            TERMINAL_FONT_LIGATURES,
            TERMINAL_FONT_WEIGHT,
        );
        let metrics = TerminalMetrics {
            font,
            font_size: px(14.0),
            cell_width: px(11.0),
            letter_spacing: Some(px(1.0)),
            line_height: px(16.0),
            primary_ascent: px(12.0),
            primary_descent: px(4.0),
        };
        let theme = TerminalUiTheme::default();
        let cell = TerminalCell {
            ch: 'a',
            wide: false,
            fg: terminal_color_from_hex(theme.foreground),
            bg: terminal_color_from_hex(theme.background),
            style_origin: Default::default(),
            attrs: Default::default(),
            extra: None,
            cursor: false,
        };

        assert_eq!(
            text_run_for_cell(&cell, gpui::white(), false, &metrics).letter_spacing,
            Some(px(1.0))
        );
        assert_eq!(
            marked_text_run("ab", &theme, &metrics).letter_spacing,
            Some(px(1.0))
        );
        assert_eq!(
            ghost_text_run("ls", &theme, &metrics).letter_spacing,
            Some(px(1.0))
        );
        assert_eq!(
            timestamp_text_run("12:00", &theme, &metrics).letter_spacing,
            None
        );

        let mut curly_cell = cell.clone();
        curly_cell.attrs = curly_cell
            .attrs
            .with_underline_style(TerminalUnderlineStyle::Curly);
        let red = TerminalColor::rgb(0xff, 0x55, 0x55);
        curly_cell.set_underline_color(Some(red));
        let curly_run = text_run_for_cell(&curly_cell, gpui::white(), false, &metrics);
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
        let single_run = text_run_for_cell(&single_cell, gpui::white(), false, &metrics);
        assert!(!text_run_style_matches(&single_run, &curly_run));

        let mut double_cell = cell;
        double_cell.attrs = double_cell
            .attrs
            .with_underline_style(TerminalUnderlineStyle::Double);
        double_cell.set_underline_color(Some(red));
        let double_run = text_run_for_cell(&double_cell, gpui::white(), false, &metrics);
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
