use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, Entity, MouseButton, Render, Rgba, Window, div, prelude::*, px, rgb,
    rgba, svg,
};
use oxideterm_gpui_ui::icons::LucideIcon;
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use super::{AiBackgroundTaskEntity, AiBackgroundTaskEvent};
use crate::state::AiWorkspaceEntity;

pub struct AiBackgroundTaskFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub background: Rgba,
}

pub struct AiBackgroundTaskView {
    ai_entity: Entity<AiWorkspaceEntity>,
    ai_background_tasks: Entity<AiBackgroundTaskEntity>,
    frame: Rc<dyn Fn(&App) -> Option<AiBackgroundTaskFrame>>,
    tokens: ThemeTokens,
    i18n: I18n,
    background: Rgba,
}

impl AiBackgroundTaskView {
    pub fn new(
        ai_entity: Entity<AiWorkspaceEntity>,
        ai_background_tasks: Entity<AiBackgroundTaskEntity>,
        frame: Rc<dyn Fn(&App) -> Option<AiBackgroundTaskFrame>>,
        initial: AiBackgroundTaskFrame,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(
            &ai_background_tasks,
            |_, _, _: &AiBackgroundTaskEvent, cx| {
                cx.notify();
            },
        )
        .detach();
        Self {
            ai_entity,
            ai_background_tasks,
            frame,
            tokens: initial.tokens,
            i18n: initial.i18n,
            background: initial.background,
        }
    }
}

impl Render for AiBackgroundTaskView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = (self.frame)(cx) else {
            return div().into_any_element();
        };
        self.tokens = frame.tokens;
        self.i18n = frame.i18n;
        self.background = frame.background;
        self.render_tasks(cx)
            .unwrap_or_else(|| div().into_any_element())
    }
}

impl AiBackgroundTaskView {
    fn render_tasks(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
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
                            .hover(|style| style.bg(rgba((self.tokens.ui.error << 8) | 0x1a)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::X,
                                11.0,
                                rgb(self.tokens.ui.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, _window, cx| {
                                    if this
                                        .ai_background_tasks
                                        .read(cx)
                                        .cancel_for_owner(&cancel_conversation_id, &cancel_task_id)
                                    {
                                        cx.notify();
                                    }
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
                .bg(self.background)
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

    fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
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
