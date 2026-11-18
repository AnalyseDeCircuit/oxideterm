use std::rc::Rc;

use gpui::{
    App, Context, Entity, MouseButton, Render, Rgba, SharedString, StyledText, TextRun, Window,
    div, font, prelude::*, px, rgb, rgba,
};
use oxideterm_gpui_ui::{
    button::{ButtonRadius, IconButtonOptions, icon_button_with_tooltip},
    icons::LucideIcon,
    select::{OverlayAnchor, SelectAnchorId, select_anchor_probe},
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;

use crate::state::{AiChatPopover, AiWorkspaceEntity};

pub struct AiChatHeaderFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub background: Rgba,
    pub font: SharedString,
}

pub enum AiChatHeaderRequest {
    OpenPopover(AiChatPopover, bool),
    NewConversation,
}

pub struct AiChatHeaderView {
    state: Entity<AiWorkspaceEntity>,
    frame: Rc<dyn Fn(&App) -> Option<AiChatHeaderFrame>>,
    request: Rc<dyn Fn(AiChatHeaderRequest, &mut Window, &mut App)>,
    anchor: Rc<dyn Fn(OverlayAnchor, &mut Window, &mut App)>,
}

impl AiChatHeaderView {
    pub fn new(
        state: Entity<AiWorkspaceEntity>,
        frame: Rc<dyn Fn(&App) -> Option<AiChatHeaderFrame>>,
        request: Rc<dyn Fn(AiChatHeaderRequest, &mut Window, &mut App)>,
        anchor: Rc<dyn Fn(OverlayAnchor, &mut Window, &mut App)>,
    ) -> Self {
        Self {
            state,
            frame,
            request,
            anchor,
        }
    }
}

impl Render for AiChatHeaderView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = (self.frame)(cx) else {
            return div().into_any_element();
        };
        let ai = self.state.read(cx);
        let title = ai
            .conversation_state()
            .active_conversation()
            .map(|chat| chat.title.clone());
        let new_disabled = ai.chat_initialization_error().is_some();
        let tokens = frame.tokens;
        let border = tokens.ui.border;
        let icon_options = |disabled| IconButtonOptions {
            disabled,
            hover_background: Some(rgba((border << 8) | 0x1a)),
            ..IconButtonOptions::opaque_toolbar(22.0, ButtonRadius::Md)
        };

        let new = icon_button_with_tooltip(
            &tokens,
            LucideIcon::Plus.path(),
            14.0,
            rgb(tokens.ui.text_muted),
            icon_options(new_disabled),
            frame.i18n.t("ai.chat.new_chat_tooltip"),
            "ai-sidebar-header-button",
            false,
            cx.listener(|this, _, window, cx| {
                (this.request)(AiChatHeaderRequest::NewConversation, window, cx);
                cx.stop_propagation();
            }),
        );
        let more = icon_button_with_tooltip(
            &tokens,
            LucideIcon::MoreVertical.path(),
            14.0,
            rgb(tokens.ui.text_muted),
            icon_options(false),
            frame.i18n.t("ai.chat.more_options"),
            "ai-sidebar-header-button",
            false,
            cx.listener(|this, _, window, cx| {
                let open = !this.state.read(cx).chat_ui().menu_open;
                (this.request)(
                    AiChatHeaderRequest::OpenPopover(AiChatPopover::Menu, open),
                    window,
                    cx,
                );
                cx.stop_propagation();
            }),
        );
        let more_anchor = self.anchor.clone();
        div()
            .w_full()
            .min_w_0()
            .min_h(px(36.0))
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(rgba((border << 8) | 0x4d))
            .bg(frame.background)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(tokens.ui.text_muted))
                            .child(label(
                                frame.i18n.t("ai.chat.header").to_uppercase(),
                                frame.font.clone(),
                                rgb(tokens.ui.text_muted),
                            )),
                    )
                    .when_some(title, |row, title| {
                        let title_anchor = self.anchor.clone();
                        row.child(
                            div()
                                .flex_none()
                                .text_color(rgba((border << 8) | 0x66))
                                .child("·"),
                        )
                        .child(select_anchor_probe(
                            SelectAnchorId::AiConversationList,
                            div()
                                .flex_1()
                                .min_w_0()
                                .cursor_pointer()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(rgba((tokens.ui.text_muted << 8) | 0x99))
                                .hover(|style| style.text_color(rgb(tokens.ui.text)))
                                .child(label(
                                    title,
                                    frame.font.clone(),
                                    rgba((tokens.ui.text_muted << 8) | 0x99),
                                ))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        let open =
                                            !this.state.read(cx).chat_ui().conversation_list_open;
                                        (this.request)(
                                            AiChatHeaderRequest::OpenPopover(
                                                AiChatPopover::ConversationList,
                                                open,
                                            ),
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    }),
                                ),
                            move |anchor, window, cx| title_anchor(anchor, window, cx),
                        ))
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(new)
                    .child(select_anchor_probe(
                        SelectAnchorId::AiChatMenu,
                        more,
                        move |anchor, window, cx| more_anchor(anchor, window, cx),
                    )),
            )
            .into_any_element()
    }
}

fn label(text: String, family: SharedString, color: Rgba) -> StyledText {
    let len = text.len();
    StyledText::new(text).with_runs(vec![TextRun {
        len,
        font: font(family),
        color: color.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
        letter_spacing: None,
    }])
}
