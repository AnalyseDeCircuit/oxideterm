use gpui::{AnyElement, App, Div, IntoElement, MouseButton, prelude::*};
use oxideterm_gpui_ui::form_field;
use oxideterm_gpui_ui::select::{BrowserFocusOrigin, select_anchor_probe};
use oxideterm_theme::ThemeTokens;

#[cfg(not(target_family = "wasm"))]
use crate::views::ConnectionsViews;
use crate::views::{FormAction, FormDispatch, form::FormOwner};
use crate::{ConnectionFormState, NewConnectionSelect};

use super::select_anchor_id;

#[cfg(not(target_family = "wasm"))]
impl ConnectionsViews {
    pub fn group_select(
        &self,
        tokens: &ThemeTokens,
        i18n: &oxideterm_i18n::I18n,
        label: String,
        group: &str,
        cx: &App,
    ) -> AnyElement {
        group_select(
            &FormOwner::Desktop(self.flow.clone()),
            &super::super::desktop::form_dispatch(&self.dispatch),
            tokens,
            i18n,
            label,
            group,
            cx,
        )
    }

    pub fn labelled_select(
        &self,
        tokens: &ThemeTokens,
        select: NewConnectionSelect,
        label: String,
        value: String,
        placeholder: bool,
        disabled: bool,
        cx: &App,
    ) -> AnyElement {
        labelled_select(
            &FormOwner::Desktop(self.flow.clone()),
            &super::super::desktop::form_dispatch(&self.dispatch),
            tokens,
            select,
            label,
            value,
            placeholder,
            disabled,
            cx,
        )
    }

    pub fn select_trigger(
        &self,
        tokens: &ThemeTokens,
        select: NewConnectionSelect,
        value: String,
        placeholder: bool,
        disabled: bool,
        cx: &App,
    ) -> Div {
        select_trigger(
            &self.flow.read(cx).form,
            tokens,
            select,
            value,
            placeholder,
            disabled,
        )
    }

    pub fn track_select_anchor(
        &self,
        select: NewConnectionSelect,
        trigger: impl IntoElement,
        cx: &App,
    ) -> AnyElement {
        track_select_anchor(&FormOwner::Desktop(self.flow.clone()), select, trigger, cx)
    }
}

pub(in crate::views) fn group_select(
    flow: &FormOwner,
    dispatch: &FormDispatch,
    tokens: &ThemeTokens,
    i18n: &oxideterm_i18n::I18n,
    label: String,
    group: &str,
    cx: &App,
) -> AnyElement {
    let value = if crate::form_state::connection_group_is_ungrouped(group, i18n) {
        i18n.t("ssh.form.ungrouped")
    } else {
        group.trim().to_owned()
    };
    labelled_select(
        flow,
        dispatch,
        tokens,
        NewConnectionSelect::Group,
        label,
        value,
        false,
        false,
        cx,
    )
}

pub(in crate::views) fn labelled_select(
    flow: &FormOwner,
    dispatch: &FormDispatch,
    tokens: &ThemeTokens,
    select: NewConnectionSelect,
    label: String,
    value: String,
    placeholder: bool,
    disabled: bool,
    cx: &App,
) -> AnyElement {
    let dispatch = dispatch.clone();
    let owner = flow.clone();
    let trigger = select_trigger(flow.state(cx), tokens, select, value, placeholder, disabled)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if !disabled {
                owner.update(cx, |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.field_focused = false;
                        form.selected_field = None;
                    }
                    if state.open_select == Some(select) {
                        state.close_select();
                    } else {
                        state.open_select = Some(select);
                        state.select_focus_origin = Some(BrowserFocusOrigin::Pointer);
                    }
                });
                dispatch(FormAction::FocusFormSelect, Some(window), cx);
            }
            cx.stop_propagation();
        });
    form_field(
        tokens,
        label,
        track_select_anchor(flow, select, trigger, cx),
    )
}

pub(in crate::views) fn select_trigger(
    state: &ConnectionFormState,
    tokens: &ThemeTokens,
    select: NewConnectionSelect,
    value: String,
    placeholder: bool,
    disabled: bool,
) -> Div {
    select.trigger(
        tokens,
        state.open_select,
        state.select_focus_origin,
        value,
        placeholder,
        disabled,
    )
}

pub(in crate::views) fn track_select_anchor(
    flow_entity: &FormOwner,
    select: NewConnectionSelect,
    trigger: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let anchors = flow_entity.anchors(cx);
    let notify_on_change = flow_entity.state(cx).open_select == Some(select);
    let entity = flow_entity.clone();
    select_anchor_probe(select_anchor_id(select), trigger, move |anchor, _, cx| {
        // An open menu needs a repaint after scrolling changes its anchor.
        if anchors.update(anchor) && notify_on_change {
            entity.update(cx, |_| ());
        }
    })
    .into_any_element()
}
