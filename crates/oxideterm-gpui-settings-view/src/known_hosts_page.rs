use std::rc::Rc;

use gpui::{
    AnyElement, App, ClipboardItem, FontWeight, MouseButton, ScrollHandle, div, prelude::*, px, rgb,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    scroll::ScrollableElement,
    text_input::TextInputContentAlign,
};

use crate::{
    SettingsInput,
    components::{SettingsCalloutTone, SettingsControls},
    surface::nav::render_icon,
};

/// The host and key together identify a deletion, even after filtering or reordering rows.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PortableKnownHostEntry {
    pub host: String,
    pub port: Option<u16>,
    pub key_type: String,
    pub fingerprint: String,
    /// Source line when the host can distinguish repeated keys in a file.
    pub line_number: Option<usize>,
    pub can_remove: bool,
}

#[derive(Default)]
pub struct PortableKnownHostsSnapshot {
    pub entries: Vec<PortableKnownHostEntry>,
    pub source: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Clone)]
pub(crate) enum KnownHostsRequest {
    Refresh,
    Select(Option<PortableKnownHostEntry>),
    Delete(PortableKnownHostEntry),
}

impl SettingsControls<'_> {
    fn known_hosts_button(
        &self,
        label: &'static str,
        icon: &'static str,
        danger: bool,
        disabled: bool,
        action: impl Fn(&mut App) + 'static,
    ) -> AnyElement {
        toolbar_button(
            self.tokens,
            self.i18n.t(label),
            Some(render_icon(icon, 14.0, rgb(self.tokens.ui.text))),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if danger {
                        ButtonVariant::Destructive
                    } else {
                        ButtonVariant::Outline
                    },
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                font_size: Some(self.tokens.metrics.ui_text_sm),
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            if !disabled {
                action(cx);
            }
            cx.stop_propagation();
        })
        .debug_selector(|| label.to_string())
        .into_any_element()
    }

    pub(crate) fn known_hosts_list(
        &self,
        snapshot: &PortableKnownHostsSnapshot,
        query_value: &str,
        pending_delete: Option<&PortableKnownHostEntry>,
        scroll: &ScrollHandle,
        can_manage: bool,
        open_file: Option<(bool, Rc<dyn Fn(&mut App)>)>,
        dispatch: Rc<dyn Fn(KnownHostsRequest, &mut App)>,
        cx: &App,
    ) -> AnyElement {
        let busy = snapshot.loading;
        let query = query_value.to_lowercase();
        let mut entries = snapshot
            .entries
            .iter()
            .filter(|entry| {
                query.is_empty()
                    || entry.host.to_lowercase().contains(&query)
                    || entry.fingerprint.to_lowercase().contains(&query)
                    || entry.key_type.to_lowercase().contains(&query)
                    || entry
                        .port
                        .is_some_and(|port| port.to_string().contains(&query))
            })
            .peekable();
        let action = |request: KnownHostsRequest| {
            let dispatch = dispatch.clone();
            move |cx: &mut App| dispatch(request.clone(), cx)
        };
        let mut toolbar = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .child(div().flex_1().min_w(px(180.0)).child(self.field(
                SettingsInput::KnownHostsSearch,
                query_value,
                self.i18n.t("settings_view.known_hosts.search"),
                None,
                TextInputContentAlign::Start,
                false,
                cx,
            )))
            .child(self.known_hosts_button(
                "settings_view.known_hosts.refresh",
                "lucide/refresh-cw.svg",
                false,
                busy || !can_manage,
                action(KnownHostsRequest::Refresh),
            ));
        if let Some((exists, open)) = open_file {
            toolbar = toolbar.child(self.known_hosts_button(
                "settings_view.known_hosts.open_file",
                "lucide/external-link.svg",
                false,
                !exists,
                move |cx| open(cx),
            ));
        }
        let mut body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(toolbar);
        if let Some(path) = &snapshot.source {
            body = body.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        self.i18n
                            .t("settings_view.known_hosts.source")
                            .replace("{{path}}", path),
                    ),
            );
        }
        if let Some(error) = &snapshot.error {
            body = body.child(
                self.known_hosts_status(
                    self.i18n
                        .t("settings_view.known_hosts.error")
                        .replace("{{error}}", error),
                ),
            );
        }
        if busy {
            body = body
                .child(self.known_hosts_status(self.i18n.t("settings_view.known_hosts.loading")));
        }
        if entries.peek().is_none() && !busy && snapshot.error.is_none() {
            body = body.child(self.known_hosts_status(self.i18n.t(if query.is_empty() {
                "settings_view.known_hosts.empty"
            } else {
                "settings_view.known_hosts.no_matches"
            })));
        }
        let mut list = div().w_full().min_w_0().flex().flex_col();
        for (index, entry) in entries.enumerate() {
            if index != 0 {
                list = list.child(self.card_separator());
            }
            let endpoint = match entry.port {
                Some(22) | None => entry.host.clone(),
                Some(port) => format!("[{}]:{port}", entry.host),
            };
            let fingerprint = entry.fingerprint.clone();
            let mut key = format!("{} · {}", entry.key_type, fingerprint);
            if let Some(line) = entry.line_number {
                key.push_str(&format!(" · #{line}"));
            }
            let mut row = div()
                .id(("known-host-row", index))
                .debug_selector(|| format!("known-host-{endpoint}"))
                .w_full()
                .min_w_0()
                .py(px(12.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(endpoint),
                )
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .child(key),
                );
            let mut actions = div().flex().flex_wrap().items_center().gap(px(8.0)).child(
                self.known_hosts_button(
                    "settings_view.known_hosts.copy",
                    "lucide/copy.svg",
                    false,
                    false,
                    move |cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(fingerprint.clone()));
                    },
                ),
            );
            if pending_delete == Some(entry) {
                row = row.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.warning))
                        .child(self.i18n.t("settings_view.known_hosts.confirm_delete")),
                );
                actions = actions
                    .child(self.known_hosts_button(
                        "settings_view.known_hosts.cancel",
                        "lucide/x.svg",
                        false,
                        busy || !can_manage,
                        action(KnownHostsRequest::Select(None)),
                    ))
                    .child(self.known_hosts_button(
                        "settings_view.known_hosts.confirm",
                        "lucide/trash-2.svg",
                        true,
                        busy || !can_manage,
                        action(KnownHostsRequest::Delete(entry.clone())),
                    ));
            } else if entry.can_remove {
                actions = actions.child(self.known_hosts_button(
                    "settings_view.known_hosts.delete",
                    "lucide/trash-2.svg",
                    false,
                    busy || !can_manage,
                    action(KnownHostsRequest::Select(Some(entry.clone()))),
                ));
            } else {
                actions = actions.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.i18n.t("settings_view.known_hosts.read_only")),
                );
            }
            list = list.child(row.child(actions));
        }
        body = body.child(
            div()
                .id("known-hosts-list")
                .relative()
                .w_full()
                .max_h(px(360.0))
                .child(
                    div()
                        .id("known-hosts-scroll")
                        .w_full()
                        .max_h(px(360.0))
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .on_scroll(|_, _, cx| cx.stop_propagation())
                        .child(list),
                )
                .vertical_scrollbar(scroll),
        );
        self.settings_card(
            "settings_view.known_hosts.title",
            "settings_view.known_hosts.description",
            vec![body.into_any_element()],
        )
    }

    fn known_hosts_status(&self, status: String) -> AnyElement {
        self.settings_callout(
            SettingsCalloutTone::Info,
            None,
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .child(status)
                .into_any_element(),
        )
    }
}
