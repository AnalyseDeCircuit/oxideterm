// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement, Styled, Window, div, px, svg,
};
use oxideterm_gpui_ui::{ActionSlotRowOptions, action_slot_row};
use oxideterm_theme::ThemeTokens;

use crate::style;

// The document layout owns expansion state so virtualization does not reset it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DisclosureState(Rc<RefCell<HashMap<String, bool>>>);

impl DisclosureState {
    pub(crate) fn open(&self, ids: &[String]) {
        let mut states = self.0.borrow_mut();
        for id in ids {
            states.insert(id.clone(), true);
        }
    }
    pub(crate) fn is_open(&self, id: &str, default: bool) -> bool {
        self.0.borrow().get(id).copied().unwrap_or(default)
    }

    pub(crate) fn toggle(&self, id: &str, default: bool) {
        let next = !self.is_open(id, default);
        self.0.borrow_mut().insert(id.to_owned(), next);
    }
}

#[derive(IntoElement)]
pub(crate) struct HtmlDisclosure {
    pub id: String,
    pub summary: AnyElement,
    pub body: AnyElement,
    pub default_open: bool,
    pub state: DisclosureState,
    pub tokens: ThemeTokens,
}

impl RenderOnce for HtmlDisclosure {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window
            .use_keyed_state(
                gpui::SharedString::from(format!("html-details-state:{}", self.id)),
                cx,
                |_, _| self.state.clone(),
            )
            .read(cx)
            .clone();
        let open = state.is_open(&self.id, self.default_open);
        let focus = window
            .use_keyed_state(
                gpui::SharedString::from(format!("html-details-focus:{}", self.id)),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        let key_focus = focus.clone();
        let click_state = state.clone();
        let click_id = self.id.clone();
        let default = self.default_open;
        let header = action_slot_row(
            &self.tokens,
            ActionSlotRowOptions::new(),
            Some(
                svg()
                    .path(if open {
                        "lucide/chevron-down.svg"
                    } else {
                        "lucide/chevron-right.svg"
                    })
                    .size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(style::muted_color(&self.tokens))
                    .into_any_element(),
            ),
            self.summary,
            Vec::new(),
        )
        .id("html-details-header")
        .track_focus(&focus)
        .cursor_pointer()
        .py(px(self.tokens.spacing.one))
        .hover(|header| header.bg(style::hex_to_hsla(self.tokens.ui.bg_hover)))
        .on_click(move |_, window, cx| {
            window.focus(&focus, cx);
            click_state.toggle(&click_id, default);
            window.refresh();
            cx.stop_propagation();
        })
        .on_key_down(move |event, window, cx| {
            if key_focus.is_focused(window)
                && matches!(event.keystroke.key.as_str(), "enter" | "space")
            {
                state.toggle(&self.id, default);
                window.refresh();
                cx.stop_propagation();
            }
        });
        let mut panel = div().w_full().min_w_0().child(header);
        if open {
            panel = panel.child(div().pt(px(self.tokens.spacing.two)).child(self.body));
        }
        panel
    }
}
