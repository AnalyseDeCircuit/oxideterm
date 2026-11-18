impl WorkspaceApp {
    pub(in crate::workspace) fn ai_tool_indicator_info(&self, cx: &App) -> (bool, usize) {
        let tool_use = &self.settings_store.settings().ai.tool_use;
        let enabled = tool_use.enabled;
        let tool_policy = ai_tool_use_policy_from_settings(tool_use);
        let active_tool_count = ai_active_tool_count(
            enabled,
            self.settings_store.settings().ai.skills.enabled,
            self.ai_web_provider_id().is_ok(),
            &tool_policy,
            self.ai.state.read(cx).mcp_registry(),
        );
        (enabled, active_tool_count)
    }

    pub(in crate::workspace) fn ai_editor_text_width(
        &self,
        target: WorkspaceImeTarget,
        cx: &App,
    ) -> f32 {
        let sidebar_width = self.ai.state.read(cx).chat_ui().sidebar_width;
        let text_width = if target == WorkspaceImeTarget::AiMessageEdit {
            self.text_input_anchors
                .bounds(target.anchor_id())
                .map(|bounds| f32::from(bounds.size.width))
                .unwrap_or(sidebar_width - AI_INPUT_SOFT_WRAP_CHROME_PX)
        } else {
            sidebar_width - AI_INPUT_SOFT_WRAP_CHROME_PX
        };
        text_width.max(1.0)
    }

    pub(in crate::workspace) fn ai_editor_visual_lines<'a>(
        &self,
        target: WorkspaceImeTarget,
        text: &'a str,
        cx: &App,
    ) -> Vec<AiInputVisualLine<'a>> {
        let width = self.ai_editor_text_width(target, cx);
        if target == WorkspaceImeTarget::AiMessageEdit {
            ai_input_measured_visual_lines(
                text,
                (width - self.tokens.metrics.form_caret_width).max(1.0),
                gpui::font(oxideterm_gpui_ui::tauri_ui_font_family(
                    &self.settings_store.settings().appearance.ui_font_family,
                )),
                &gpui::WindowTextSystem::new(cx.text_system().clone()),
            )
        } else {
            ai_input_visual_lines(
                text,
                ai_input_soft_wrap_columns(width + AI_INPUT_SOFT_WRAP_CHROME_PX),
            )
        }
    }

    pub(in crate::workspace) fn ai_context_token_breakdown(&self, cx: &App) -> AiContextTokenBreakdown {
        self.ai_conversation_prompt(cx).ai_context_token_breakdown(self.resolve_ai_stream_config(cx).ok().as_ref())
    }

    pub(in crate::workspace) fn ai_chat_autocomplete_items(
        &self,
        cx: &App,
    ) -> Vec<AiAutocompleteCandidate> {
        let settings = &self.settings_store.settings().ai;
        self.ai
            .state
            .read(cx)
            .chat_autocomplete_items(settings.active_backend, || {
                if settings.skills.enabled {
                    self.ai
                        .skills.read()
                        .catalog()
                        .into_iter()
                        .map(|skill| (skill.id, skill.description))
                        .collect()
                } else {
                    Vec::new()
                }
            })
    }

    pub(in crate::workspace) fn apply_ai_chat_autocomplete_candidate(
        &mut self,
        candidate: &AiAutocompleteCandidate,
        cx: &mut Context<Self>,
    ) {
        self.ai.state.update(cx, |ai, cx| {
            ai.apply_chat_autocomplete(candidate);
            cx.notify();
        });
        self.ime_marked_text = None;
    }
}

use oxideterm_gpui_ai::sidebar::editor::{
    AI_INPUT_SOFT_WRAP_CHROME_PX, ai_input_measured_visual_lines, ai_input_soft_wrap_columns,
};
pub(in crate::workspace) use oxideterm_gpui_ai::sidebar::editor::{
    AiInputVisualLine, ai_input_line_index_for_offset, ai_input_visual_lines,
};

impl WorkspaceApp {
    pub(in crate::workspace) fn ai_context_message_usage_counts(
        &self,
        cx: &App,
    ) -> Option<(usize, usize)> {
        self.ai_conversation_prompt(cx).context_usage_counts(|| self.resolve_ai_stream_config(cx).ok())
    }
}
