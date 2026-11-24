use super::metrics::*;
use gpui::px;

pub(crate) fn test_metrics() -> TerminalMetrics {
    TerminalMetrics {
        font: terminal_font_with_family_and_cjk(
            TERMINAL_FONT,
            None,
            TERMINAL_FONT_LIGATURES,
            TERMINAL_FONT_WEIGHT,
        ),
        font_size: px(14.0),
        cell_width: px(8.0),
        letter_spacing: None,
        line_height: px(10.0),
        primary_ascent: px(7.5),
        primary_descent: px(2.5),
    }
}
