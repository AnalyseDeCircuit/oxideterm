impl WorkspaceApp {
    pub(in crate::workspace) fn render_ai_background_tasks(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        const MAX_VISIBLE_TASKS: usize = 3;

        let conversation_id = self
            .ai_entity
            .read(cx)
            .conversation_state()
            .active_conversation_id
            .clone()?;
        let active_tasks = self
            .ai_background_tasks
            .read(cx)
            .snapshots_for_owner(&conversation_id)
            .into_iter()
            .filter(|task| {
                matches!(
                    task.state,
                    oxideterm_ai_tasks::BackgroundTaskState::Queued
                        | oxideterm_ai_tasks::BackgroundTaskState::Running
                        | oxideterm_ai_tasks::BackgroundTaskState::Waiting
                )
            })
            .rev()
            .take(MAX_VISIBLE_TASKS)
            .collect::<Vec<_>>();
        if active_tasks.is_empty() {
            return None;
        }

        let mut rows = div().w_full().flex().flex_col().gap(px(2.0));
        for (task_index, task) in active_tasks.into_iter().enumerate() {
            let cancel_conversation_id = conversation_id.clone();
            let cancel_task_id = task.id.clone();
            let state_label = self.ai_background_task_state_label(task.state);
            let run_label = self
                .i18n
                .t("ai.background_tasks.run_count")
                .replace("{{count}}", &task.run_count.to_string());
            rows = rows.child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(6.0))
                    .rounded(px(5.0))
                    .bg(rgba((self.tokens.ui.border << 8) | 0x0d))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Clock,
                        12.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(task.title),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(10.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(format!("{state_label} · {run_label}")),
                    )
                    .child(
                        div()
                            .id(("ai-background-task-cancel", task_index))
                            .flex_none()
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(|style| {
                                style.bg(rgba((self.tokens.ui.error << 8) | 0x1a))
                            })
                            .child(Self::render_lucide_icon(
                                LucideIcon::X,
                                11.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.cancel_ai_background_task_from_ui(
                                        &cancel_conversation_id,
                                        &cancel_task_id,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            );
        }

        Some(
            div()
                .w_full()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .px(px(10.0))
                .py(px(6.0))
                .border_b_1()
                .border_color(rgba((self.tokens.ui.border << 8) | 0x33))
                .bg(self.context_sidebar_content_background(self.tokens.ui.bg))
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("ai.background_tasks.title")),
                )
                .child(rows)
                .into_any_element(),
        )
    }

    fn ai_background_task_state_label(
        &self,
        state: oxideterm_ai_tasks::BackgroundTaskState,
    ) -> String {
        let key = match state {
            oxideterm_ai_tasks::BackgroundTaskState::Queued => "queued",
            oxideterm_ai_tasks::BackgroundTaskState::Running => "running",
            oxideterm_ai_tasks::BackgroundTaskState::Waiting => "waiting",
            oxideterm_ai_tasks::BackgroundTaskState::Completed => "completed_state",
            oxideterm_ai_tasks::BackgroundTaskState::Failed => "failed_state",
            oxideterm_ai_tasks::BackgroundTaskState::Cancelled => "cancelled",
        };
        self.i18n.t(&format!("ai.background_tasks.{key}"))
    }
}
