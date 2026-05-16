use gpui::{
    Font, FontStyle, FontWeight, Hsla, Rgba, StrikethroughStyle, TextRun, UnderlineStyle, px, rgb,
    rgba,
};
use oxideterm_terminal::{TerminalCell, TerminalColor};

use crate::terminal_ui::*;

pub(crate) fn text_run_for_cell(
    cell: &TerminalCell,
    color: Hsla,
    link: bool,
    metrics: &TerminalMetrics,
) -> TextRun {
    let weight = if cell.attrs.bold {
        FontWeight::BOLD
    } else {
        FontWeight::default()
    };
    let style = if cell.attrs.italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };

    TextRun {
        len: cell.ch.len_utf8(),
        font: Font {
            family: metrics.font.family.clone(),
            features: metrics.font.features.clone(),
            fallbacks: metrics.font.fallbacks.clone(),
            weight,
            style,
        },
        color: if link { rgb(0x61afef).into() } else { color },
        background_color: None,
        underline: (cell.attrs.underline || link).then_some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(if link { rgb(0x61afef).into() } else { color }),
            wavy: false,
        }),
        strikethrough: cell.attrs.strikeout.then_some(StrikethroughStyle {
            thickness: px(1.0),
            color: Some(color),
        }),
    }
}

pub(crate) fn marked_text_run(text: &str, metrics: &TerminalMetrics) -> TextRun {
    let color = rgb(0xe6e8eb).into();
    TextRun {
        len: text.len(),
        font: metrics.font.clone(),
        color,
        background_color: Some(rgba(0x528bff33).into()),
        underline: Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        }),
        strikethrough: None,
    }
}

pub(crate) fn text_run_style_matches(left: &TextRun, right: &TextRun) -> bool {
    fn comparable_style(run: &TextRun) -> (&Font, Hsla, Option<Hsla>, bool, bool) {
        (
            &run.font,
            run.color,
            run.background_color,
            run.underline.is_some(),
            run.strikethrough.is_some(),
        )
    }

    comparable_style(left) == comparable_style(right)
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PowerlineSeparator {
    pub(crate) direction: PowerlineDirection,
    pub(crate) weight: PowerlineWeight,
    pub(crate) shape: PowerlineShape,
}

pub(crate) fn powerline_separator(ch: char) -> Option<PowerlineSeparator> {
    match ch as u32 {
        0xe0b0 => Some(PowerlineSeparator {
            direction: PowerlineDirection::Right,
            weight: PowerlineWeight::Filled,
            shape: PowerlineShape::Triangle,
        }),
        0xe0b1 => Some(PowerlineSeparator {
            direction: PowerlineDirection::Right,
            weight: PowerlineWeight::Thin,
            shape: PowerlineShape::Triangle,
        }),
        0xe0b2 => Some(PowerlineSeparator {
            direction: PowerlineDirection::Left,
            weight: PowerlineWeight::Filled,
            shape: PowerlineShape::Triangle,
        }),
        0xe0b3 => Some(PowerlineSeparator {
            direction: PowerlineDirection::Left,
            weight: PowerlineWeight::Thin,
            shape: PowerlineShape::Triangle,
        }),
        0xe0b4 => Some(PowerlineSeparator {
            direction: PowerlineDirection::Right,
            weight: PowerlineWeight::Filled,
            shape: PowerlineShape::HalfCircle,
        }),
        0xe0b5 => Some(PowerlineSeparator {
            direction: PowerlineDirection::Right,
            weight: PowerlineWeight::Thin,
            shape: PowerlineShape::HalfCircle,
        }),
        0xe0b6 => Some(PowerlineSeparator {
            direction: PowerlineDirection::Left,
            weight: PowerlineWeight::Filled,
            shape: PowerlineShape::HalfCircle,
        }),
        0xe0b7 => Some(PowerlineSeparator {
            direction: PowerlineDirection::Left,
            weight: PowerlineWeight::Thin,
            shape: PowerlineShape::HalfCircle,
        }),
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

pub(crate) fn terminal_background() -> Hsla {
    rgb(OXIDETERM_TERMINAL_BACKGROUND).into()
}
