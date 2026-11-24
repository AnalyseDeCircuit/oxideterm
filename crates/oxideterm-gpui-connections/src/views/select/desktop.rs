use gpui::{AnyElement, App, Div, Entity, IntoElement, MouseButton, prelude::*};
use oxideterm_gpui_ui::form_field;
use oxideterm_gpui_ui::select::{BrowserFocusOrigin, select_anchor_probe};
use oxideterm_theme::ThemeTokens;

use crate::views::{ConnectionsDispatch, ConnectionsViews, ConnectionsWindowRequest};
use crate::{ConnectionFormState, NewConnectionSelect, entity::ConnectionFlowEntity};

use super::select_anchor_id;

impl ConnectionsViews {
    pub fn group_select(
        &self,
        tokens: &ThemeTokens,
        i18n: &oxideterm_i18n::I18n,
        label: String,
        group: &str,
        cx: &App,
    ) -> AnyElement {
        group_select(&self.flow, &self.dispatch, tokens, i18n, label, group, cx)
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
            &self.flow,
            &self.dispatch,
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
        track_select_anchor(&self.flow, select, trigger, cx)
    }
}

pub(in crate::views) fn group_select(
    flow: &Entity<ConnectionFlowEntity>,
    dispatch: &ConnectionsDispatch,
    tokens: &ThemeTokens,
    i18n: &oxideterm_i18n::I18n,
    label: String,
    group: &str,
    cx: &App,
) -> AnyElement {
    let value = if crate::conversion::connection_group_is_ungrouped(group, i18n) {
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
    flow: &Entity<ConnectionFlowEntity>,
    dispatch: &ConnectionsDispatch,
    tokens: &ThemeTokens,
    select: NewConnectionSelect,
    label: String,
    value: String,
    placeholder: bool,
    disabled: bool,
    cx: &App,
) -> AnyElement {
    let dispatch = dispatch.clone();
    let owner = flow.downgrade();
    let trigger = select_trigger(
        &flow.read(cx).form,
        tokens,
        select,
        value,
        placeholder,
        disabled,
    )
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        if !disabled {
            let _ = owner.update(cx, |flow, cx| {
                if let Some(form) = flow.form.form.as_mut() {
                    form.field_focused = false;
                    form.selected_field = None;
                }
                if flow.form.open_select == Some(select) {
                    flow.form.close_select();
                } else {
                    flow.form.open_select = Some(select);
                    flow.form.select_focus_origin = Some(BrowserFocusOrigin::Pointer);
                }
                cx.notify();
            });
            dispatch(ConnectionsWindowRequest::FocusFormSelect, Some(window), cx);
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
    flow_entity: &gpui::Entity<ConnectionFlowEntity>,
    select: NewConnectionSelect,
    trigger: impl IntoElement,
    cx: &App,
) -> AnyElement {
    let flow = flow_entity.read(cx);
    let anchors = flow.select_anchor_store();
    let notify_on_change = flow.form.open_select == Some(select);
    let entity = flow_entity.downgrade();
    select_anchor_probe(select_anchor_id(select), trigger, move |anchor, _, cx| {
        // An open menu needs a repaint after scrolling changes its anchor.
        if anchors.update(anchor) && notify_on_change {
            let _ = entity.update(cx, |_, cx| cx.notify());
        }
    })
    .into_any_element()
}
