use std::{
    hash::{Hash, Hasher},
    time::Duration,
};

use gpui::{
    App, Context, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Task, Timer,
    Window, div, px,
};
use oxideterm_theme::ThemeTokens;

use super::{MermaidRenderRequest, RenderedMermaidImage};
use crate::{
    options::MarkdownOptions,
    render::{MarkdownCodeBlockActions, render_mermaid_body, render_mermaid_header},
    style,
};

#[derive(IntoElement)]
pub(crate) struct MermaidBlock {
    pub source: String,
    pub tokens: ThemeTokens,
    pub options: MarkdownOptions,
    pub actions: Option<MarkdownCodeBlockActions>,
}

struct MermaidRenderState {
    result: Option<Result<RenderedMermaidImage, String>>,
    _task: Option<Task<()>>,
}

impl MermaidRenderState {
    fn new(request: MermaidRenderRequest, cx: &mut Context<Self>) -> Self {
        if let Some(result) = request.cached() {
            return Self {
                result: Some(result),
                _task: None,
            };
        }
        let task = cx.spawn(async move |state, cx| {
            // Replacing a streaming block drops its state and cancels the idle wait.
            Timer::after(Duration::from_millis(150)).await;
            let result = cx
                .background_executor()
                .spawn(async move { request.render() })
                .await;
            let _ = state.update(cx, |state, cx| {
                state.result = Some(result);
                state._task = None;
                cx.notify();
            });
        });
        Self {
            result: None,
            _task: Some(task),
        }
    }
}

impl RenderOnce for MermaidBlock {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let request = MermaidRenderRequest::new(&self.source, &self.tokens, &self.options, 2.0);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        request.hash(&mut hasher);
        let state = window.use_keyed_state(("mermaid-render", hasher.finish()), cx, |_, cx| {
            MermaidRenderState::new(request, cx)
        });
        let result = state.read(cx).result.clone();
        let header = render_mermaid_header(
            &self.source,
            &self.tokens,
            &self.options,
            self.actions.as_ref(),
            result.as_ref().and_then(|result| result.as_ref().ok()),
        );
        let body = if let Some(result) = result {
            render_mermaid_body(&self.source, &self.tokens, &self.options, result)
        } else {
            div()
                .w_full()
                .p(px(self.options.code_block_padding))
                .text_size(style::code_font_size(&self.options))
                .text_color(style::muted_color(&self.tokens))
                .child(SharedString::from(
                    self.options.mermaid_loading_label.clone(),
                ))
                .into_any_element()
        };
        div()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .border_1()
            .border_color(style::code_block_border_color(&self.tokens))
            .bg(style::code_block_bg_color(&self.tokens, &self.options))
            .rounded(px(self.tokens.radii.md))
            .child(header)
            .child(body)
    }
}
