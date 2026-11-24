use crate::data::{DetectedPort, ForwardRule, ForwardType, ForwardUpdate, NodeId};
use crate::helpers::parse_port;
use crate::{
    ForwardingRuntimeOperation, ForwardsFrame, ForwardsPageState, ForwardsRequest,
    ForwardsViewBindings, style::*,
};
use gpui::{
    AnyElement, App, Context, Entity, IntoElement, MouseButton, MouseDownEvent, Render, Rgba,
    Subscription, Window, div, prelude::*, px, svg,
};
use oxideterm_gpui_ui::{
    button::{IconButtonOptions, ToolbarButtonOptions, icon_button, toolbar_action_button},
    icons::LucideIcon,
};
use oxideterm_i18n::I18n;
use oxideterm_theme::ThemeTokens;
use oxideterm_workspace::TabId;

pub struct ForwardsViews {
    pub state: Entity<ForwardsPageState>,
    body: Entity<ForwardsView>,
    dialogs: Entity<ForwardsView>,
}

impl ForwardsViews {
    pub fn new(
        state: Entity<ForwardsPageState>,
        bindings: ForwardsViewBindings,
        cx: &mut App,
    ) -> Self {
        let body = cx.new(|cx| ForwardsView::new(state.clone(), bindings.clone(), Mount::Body, cx));
        let dialogs = cx.new(|cx| ForwardsView::new(state.clone(), bindings, Mount::Dialogs, cx));
        Self {
            state,
            body,
            dialogs,
        }
    }

    /// A narrow navigator uses the same addresses and status labels as the full rules table.
    pub fn sidebar(&self, rules: &[ForwardRule], tokens: &ThemeTokens, i18n: &I18n) -> AnyElement {
        let theme = tokens.ui;
        div()
            .id("forward-rules-sidebar")
            .size_full()
            .overflow_y_scroll()
            .px_3()
            .pt_1()
            .text_size(px(tokens.metrics.ui_text_sm))
            .when(rules.is_empty(), |d| {
                d.child(
                    div()
                        .py_3()
                        .text_color(gpui::rgb(theme.text_muted))
                        .child(i18n.t("forwards.table.no_forwards")),
                )
            })
            .children(rules.iter().map(|rule| {
                let (local, remote) = crate::helpers::forward_addresses(rule);
                div()
                    .w_full()
                    .py_2()
                    .border_b_1()
                    .border_color(gpui::rgba((theme.border << 8) | 0x66))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(crate::helpers::forward_type_key(rule.forward_type, i18n))
                            .child(
                                div()
                                    .text_size(px(tokens.metrics.ui_text_xs))
                                    .text_color(gpui::rgb(theme.text_muted))
                                    .child(
                                        i18n.t(crate::helpers::forward_status_key(&rule.status)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .truncate()
                            .font_family("Lilex")
                            .text_size(px(tokens.metrics.ui_text_xs))
                            .child(if rule.forward_type == ForwardType::Dynamic {
                                local
                            } else {
                                format!("{local} → {remote}")
                            }),
                    )
            }))
            .into_any_element()
    }

    pub fn body(&self) -> AnyElement {
        self.body.clone().into_any_element()
    }

    pub fn dialogs(&self) -> AnyElement {
        self.dialogs.clone().into_any_element()
    }
}

enum Mount {
    Body,
    Dialogs,
}

pub(crate) struct ForwardsView {
    pub(super) state: Entity<ForwardsPageState>,
    pub(super) bindings: ForwardsViewBindings,
    frame: Option<ForwardsFrame>,
    pub(super) tokens: ThemeTokens,
    pub(super) i18n: I18n,
    mount: Mount,
    _observation: Subscription,
}

impl ForwardsView {
    fn new(
        state: Entity<ForwardsPageState>,
        bindings: ForwardsViewBindings,
        mount: Mount,
        cx: &mut Context<Self>,
    ) -> Self {
        let observation = cx.observe(&state, |_, _, cx| cx.notify());
        Self {
            state,
            bindings,
            frame: None,
            tokens: oxideterm_theme::default_tokens(),
            i18n: I18n::new(oxideterm_i18n::Locale::En),
            mount,
            _observation: observation,
        }
    }

    pub(super) fn frame(&self) -> &ForwardsFrame {
        self.frame
            .as_ref()
            .expect("forwarding frame initialized by Render")
    }

    pub(super) fn request(
        &self,
        request: ForwardsRequest,
        window: Option<&mut Window>,
        cx: &mut App,
    ) {
        (self.bindings.dispatch)(request, window, cx);
    }

    pub(super) fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    pub(super) fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        toolbar_action_button(&self.tokens, label, icon, options, listener)
    }

    pub(super) fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = !(options.disabled || options.loading);
        icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, size, color),
            options,
        )
        .when(enabled, |button| {
            button.on_mouse_down(MouseButton::Left, cx.listener(listener))
        })
        .when(!enabled, |button| {
            button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        })
    }

    pub(super) fn start_forward_operation(
        &self,
        _tab_id: TabId,
        _node_id: NodeId,
        message_key: &'static str,
        sync_saved: bool,
        operation: ForwardingRuntimeOperation,
        cx: &mut Context<Self>,
    ) {
        self.request(
            ForwardsRequest::Operation {
                operation,
                message_key,
                sync_saved,
            },
            None,
            cx,
        );
    }

    fn checked_ports(
        &self,
        kind: ForwardType,
        bind: &str,
        target: &str,
        cx: &mut Context<Self>,
    ) -> Option<(u16, Option<u16>)> {
        let parse = |text: &str| {
            parse_port(text).ok_or(if text.trim().is_empty() {
                "forwards.form.port_required"
            } else {
                "forwards.form.port_invalid"
            })
        };
        let result = parse(bind).and_then(|bind| {
            if kind == ForwardType::Dynamic {
                Ok((bind, None))
            } else {
                parse(target).map(|target| (bind, Some(target)))
            }
        });
        match result {
            Ok(ports) => Some(ports),
            Err(key) => {
                self.state.update(cx, |state, cx| {
                    state.set_error(self.i18n.t(key));
                    cx.notify();
                });
                None
            }
        }
    }

    pub(super) fn submit_forward_create(
        &mut self,
        _tab: TabId,
        _node: NodeId,
        start: bool,
        cx: &mut Context<Self>,
    ) {
        let view = &self.state.read(cx).view;
        let (kind, bind, bind_port, target, target_port, check_health) = (
            view.forward_type,
            view.bind_address.clone(),
            view.bind_port.clone(),
            view.target_host.clone(),
            view.target_port.clone(),
            !view.skip_health_check,
        );
        let Some((bind_port, target_port)) = self.checked_ports(kind, &bind_port, &target_port, cx)
        else {
            return;
        };
        let rule = match kind {
            ForwardType::Local => {
                ForwardRule::local(bind, bind_port, target, target_port.unwrap_or(0))
            }
            ForwardType::Remote => {
                ForwardRule::remote(bind, bind_port, target, target_port.unwrap_or(0))
            }
            ForwardType::Dynamic => ForwardRule {
                target_host: "0.0.0.0".into(),
                ..ForwardRule::dynamic(bind, bind_port)
            },
        };
        self.request(
            ForwardsRequest::Create {
                rule,
                start,
                check_health,
            },
            None,
            cx,
        );
    }

    pub(super) fn submit_forward_edit(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) {
        let view = &self.state.read(cx).view;
        let Some(rule) = view.editing_forward.as_ref() else {
            return;
        };
        let (id, kind, bind, bind_port, target, target_port) = (
            rule.id.clone(),
            rule.forward_type,
            view.edit_bind_address.clone(),
            view.edit_bind_port.clone(),
            view.edit_target_host.clone(),
            view.edit_target_port.clone(),
        );
        let Some((bind_port, target_port)) = self.checked_ports(kind, &bind_port, &target_port, cx)
        else {
            return;
        };
        let update = ForwardUpdate {
            bind_address: Some(bind),
            bind_port: Some(bind_port),
            target_host: (kind != ForwardType::Dynamic).then_some(target),
            target_port,
            ..Default::default()
        };
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.updated",
            true,
            ForwardingRuntimeOperation::Update {
                forward_id: id,
                update,
            },
            cx,
        );
    }

    pub(super) fn create_local_forward_for_detected_port(
        &self,
        tab_id: TabId,
        node_id: NodeId,
        port: DetectedPort,
        cx: &mut Context<Self>,
    ) {
        let mut rule = ForwardRule::local(
            FORWARDS_DEFAULT_BIND_ADDRESS,
            port.port,
            FORWARDS_DEFAULT_TARGET_HOST,
            port.port,
        );
        rule.description = port
            .process_name
            .as_ref()
            .map(|process| format!("{process} ({})", self.i18n.t("forwards.detection.auto")))
            .unwrap_or_else(|| {
                format!(
                    "{} {} ({})",
                    self.i18n.t("forwards.detection.port"),
                    port.port,
                    self.i18n.t("forwards.detection.auto")
                )
            });
        self.dismiss_detected_port(port.port, cx);
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.created",
            true,
            ForwardingRuntimeOperation::Create {
                rule,
                check_health: true,
            },
            cx,
        );
    }

    pub(super) fn dismiss_detected_port(&self, port: u16, cx: &mut Context<Self>) {
        self.request(ForwardsRequest::DismissPort(port), None, cx);
    }

    pub(super) fn open_forward_edit_form(&self, rule: ForwardRule, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            state.open_edit_form(rule);
            cx.notify();
        });
    }
}

impl Render for ForwardsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if matches!(self.mount, Mount::Dialogs) {
            let state = self.state.read(cx);
            if !state.edit_form_open() && !state.delete_confirm_open() {
                return div().into_any_element();
            }
        }
        let Some(frame) = (self.bindings.frame)(cx) else {
            return div().into_any_element();
        };
        self.tokens = frame.tokens;
        self.i18n = frame.i18n.clone();
        let (tab_id, node_id, has_background) =
            (frame.tab_id, frame.node_id.clone(), frame.has_background);
        self.frame = Some(frame);
        match self.mount {
            Mount::Body => self.render_forwards_surface(tab_id, node_id, cx),
            Mount::Dialogs => div()
                .absolute()
                .inset_0()
                .child(self.render_forward_edit_modal(
                    node_id.clone(),
                    tab_id,
                    has_background,
                    window,
                    cx,
                ))
                .child(self.render_forward_delete_confirm(node_id, tab_id, has_background, cx))
                .into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests;
