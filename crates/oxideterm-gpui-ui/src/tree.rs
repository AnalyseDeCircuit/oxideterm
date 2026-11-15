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
            indent_size: 14.0,
            branch_left: 8.0,
            branch_top: 12.0,
            branch_width: 7.0,
            line_alpha: 0x26,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TreeGuideMetrics {
    pub indent_size: f32,
    pub rail_left: f32,
    pub rail_width: f32,
    pub line_alpha: u32,
    pub end_bottom_inset: f32,
}

impl TreeGuideMetrics {
    /// Aligns each rail dead-center under the parent's `12px` chevron (`px_2(8) + 6 = 14px`)
    /// while leaving clean horizontal air before the `20px * depth` child pill.
    pub fn session_tree() -> Self {
        Self {
            indent_size: 20.0,
            rail_left: 13.5,
            rail_width: 1.0,
            line_alpha: 0x28,
            end_bottom_inset: 4.0,
        }
    }
}

/// Renders an indented tree child with clean vertical guide rails for all active
/// ancestor levels (`0..depth - 1`) and the owning level (`depth - 1`).
pub fn tree_guide_child(
    tokens: &ThemeTokens,
    metrics: TreeGuideMetrics,
    depth: usize,
    ancestor_continues: &[bool],
    line_stops_here: bool,
    child: impl IntoElement,
) -> AnyElement {
    if depth == 0 {
        return child.into_any_element();
    }

    let line_color: Rgba = rgba((tokens.ui.text_muted << 8) | metrics.line_alpha);
    let mut container = div()
        .relative()
        .w_full()
        .pl(px(depth as f32 * metrics.indent_size));

    for level in 0..depth.saturating_sub(1) {
        if ancestor_continues.get(level).copied().unwrap_or(false) {
            let left = level as f32 * metrics.indent_size + metrics.rail_left;
            container = container.child(
                div()
                    .absolute()
                    .left(px(left))
                    .top_0()
                    .bottom_0()
                    .w(px(metrics.rail_width))
                    .bg(line_color),
            );
        }
    }

    let innermost_left = (depth - 1) as f32 * metrics.indent_size + metrics.rail_left;
    let innermost = div()
        .absolute()
        .left(px(innermost_left))
        .top_0()
        .w(px(metrics.rail_width))
        .bg(line_color);
    let innermost = if line_stops_here {
        innermost.bottom(px(metrics.end_bottom_inset))
    } else {
        innermost.bottom_0()
    };

    container.child(innermost).child(child).into_any_element()
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
