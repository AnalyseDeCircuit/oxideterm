use gpui::{AnyElement, IntoElement, ParentElement, Rgba, Styled, div, px, rgba};
use oxideterm_theme::ThemeTokens;

#[derive(Clone, Copy, Debug)]
pub struct TreeBranchMetrics {
    pub indent_size: f32,
    pub branch_left: f32,
    pub branch_top: f32,
    pub branch_width: f32,
    pub line_alpha: u32,
}

impl TreeBranchMetrics {
    pub fn tauri_session_tree() -> Self {
        Self {
            indent_size: 16.0,
            branch_left: 10.0,
            branch_top: 14.0,
            branch_width: 8.0,
            line_alpha: 0x26,
        }
    }
}

pub fn tree_child(
    tokens: &ThemeTokens,
    metrics: TreeBranchMetrics,
    depth: usize,
    line_stops_here: bool,
    child: impl IntoElement,
) -> AnyElement {
    let line_color: Rgba = rgba((tokens.ui.text_muted << 8) | metrics.line_alpha);
    let left = (depth.saturating_sub(1) as f32) * metrics.indent_size + metrics.branch_left;
    let branch = div()
        .absolute()
        .left(px(left))
        .top_0()
        .w(px(1.0))
        .bg(line_color);
    let branch = if line_stops_here {
        branch.h(px(metrics.branch_top))
    } else {
        branch.bottom_0()
    };

    div()
        .relative()
        .w_full()
        .pl(px(depth as f32 * metrics.indent_size))
        .child(branch)
        .child(
            div()
                .absolute()
                .left(px(left))
                .top(px(metrics.branch_top))
                .h(px(1.0))
                .w(px(metrics.branch_width))
                .bg(line_color),
        )
        .child(child)
        .into_any_element()
}
