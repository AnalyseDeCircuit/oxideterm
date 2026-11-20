use super::*;

use gpui::Div;

pub(super) fn monitor_center_state(
    app: &WorkspaceApp,
    icon: LucideIcon,
    color: u32,
    label: String,
    cx: &mut Context<WorkspaceApp>,
) -> AnyElement {
    let selectable_text = app.host_tools_selectable_text(cx);
    host_tools_center_state(icon.path(), color, label, &selectable_text, cx)
}

impl WorkspaceApp {
    /// Renders a multiline editable buffer line by line so the shared IME
    /// mapping (which is line-based) and pointer selection stay aligned.
    pub(in crate::workspace) fn render_settings_multiline_textarea_lines(
        &self,
        mut textarea: Div,
        target: WorkspaceImeTarget,
        value: &str,
        placeholder: bool,
        line_height: f32,
        cx: &App,
    ) -> Div {
        textarea = textarea
            .w_auto()
            .min_w_full()
            .flex_shrink_0()
            .whitespace_nowrap();
        let selection = self.ime_selected_range_for_target(target, cx);
        let theme = self.tokens.ui;
        for (line_range, line_text) in
            oxideterm_settings_model::settings_multiline_line_ranges(value)
        {
            let (selection_range, caret_offset) =
                oxideterm_settings_model::settings_multiline_line_selection(
                    selection.as_ref(),
                    &line_range,
                );
            let mut line = div()
                .h(px(line_height))
                .min_h(px(line_height))
                .flex_shrink_0()
                .whitespace_nowrap();
            if placeholder {
                line = line
                    .text_color(rgb(theme.text_muted))
                    .child(line_text.as_str().to_string());
            } else {
                line = line.child(oxideterm_gpui_ui::text_input::text_input_value_segments(
                    &self.tokens,
                    &line_text,
                    false,
                    selection_range,
                    caret_offset,
                    self.input_caret.visible(),
                ));
            }
            textarea = textarea.child(line);
        }
        textarea
    }
}
