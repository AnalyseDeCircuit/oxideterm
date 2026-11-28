use super::ContextSidebarTool;
use oxideterm_theme::UiMetrics;

pub const TOOLS: [ContextSidebarTool; 13] = [
    ContextSidebarTool::Monitor,
    ContextSidebarTool::Processes,
    ContextSidebarTool::Gpu,
    ContextSidebarTool::Docker,
    ContextSidebarTool::Container,
    ContextSidebarTool::Services,
    ContextSidebarTool::Logs,
    ContextSidebarTool::Ports,
    ContextSidebarTool::Tmux,
    ContextSidebarTool::Filesystems,
    ContextSidebarTool::Schedules,
    ContextSidebarTool::Packages,
    ContextSidebarTool::Redis,
];

pub const MONITORING_TOOL_COUNT: usize = 5;

pub fn visible_tool_count(available_height: f32, row_height: f32, separator_height: f32) -> usize {
    // Reserve More only for overflow, and the second divider only when both groups show.
    (0..=TOOLS.len())
        .rev()
        .find(|&count| {
            let more_height = if count < TOOLS.len() { row_height } else { 0.0 };
            tool_stack_height(count, row_height, separator_height) + more_height <= available_height
        })
        .unwrap_or(0)
}

pub fn tool_stack_height(count: usize, row_height: f32, separator_height: f32) -> f32 {
    let separators = if count > MONITORING_TOOL_COUNT {
        2.0
    } else {
        1.0
    };
    (count + 1) as f32 * row_height + separators * separator_height
}

pub fn overflow_tools(visible: usize) -> Vec<ContextSidebarTool> {
    TOOLS[visible..].to_vec()
}

pub const DIVIDER_MARGIN: f32 = 2.0;

pub fn separator_height(metrics: UiMetrics) -> f32 {
    metrics.divider_height + metrics.activity_icon_gap + 2.0 * DIVIDER_MARGIN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinking_the_rail_moves_only_trailing_tools_into_more() {
        let row_height = 40.0;
        let separator_height = 9.0;
        let stack = |count| tool_stack_height(count, row_height, separator_height);
        // All tools fit exactly without reserving an unnecessary More button.
        let tall = 14.0 * row_height + 2.0 * separator_height;
        assert_eq!(
            visible_tool_count(tall, row_height, separator_height),
            TOOLS.len()
        );
        assert_eq!(overflow_tools(TOOLS.len()), vec![]);
        assert_eq!(
            visible_tool_count(tall + 200.0, row_height, separator_height),
            TOOLS.len()
        );
        // Once the stack no longer fits, More needs a row of its own and only
        // trailing tools move into it.
        let tight = visible_tool_count(tall - 1.0, row_height, separator_height);
        assert_eq!(tight, 11);
        assert_eq!(
            overflow_tools(tight),
            [ContextSidebarTool::Packages, ContextSidebarTool::Redis]
        );
        // Below the group divider only leading monitoring tools stay visible.
        let below_divider = visible_tool_count(
            stack(MONITORING_TOOL_COUNT) - 1.0,
            row_height,
            separator_height,
        );
        assert!(below_divider < MONITORING_TOOL_COUNT);
        assert_eq!(
            overflow_tools(below_divider),
            TOOLS[below_divider..].to_vec()
        );
        assert_eq!(visible_tool_count(80.0, 40.0, 9.0), 0);
    }
}
