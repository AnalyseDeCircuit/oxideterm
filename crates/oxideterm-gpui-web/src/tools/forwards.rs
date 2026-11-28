use crate::host::Workspace;
use gpui::{prelude::*, *};
use oxideterm_gpui_forwards::{data::*, *};
use oxideterm_workspace::{Tab, TabId, TabKind, TabTitleSource};
use std::{collections::HashMap, time::Duration};

struct Page {
    node: String,
    views: ForwardsViews,
}

/// Drafts belong to their node. Closing a tab drops its form and input focus, not its rules.
#[derive(Default)]
pub(crate) struct DraftForwards {
    pages: HashMap<TabId, Page>,
    rules: HashMap<String, ForwardingRuntimeSnapshot>,
}

impl DraftForwards {
    pub(crate) fn node(&self, tab: TabId) -> Option<&str> {
        self.pages.get(&tab).map(|p| p.node.as_str())
    }
    pub(crate) fn release(&mut self, tab: TabId) {
        self.pages.remove(&tab);
    }
    pub(crate) fn refresh(&self, cx: &mut App) {
        for p in self.pages.values() {
            cx.notify(p.views.state.entity_id());
        }
    }
    pub(crate) fn editing_text(&self, w: &Window) -> bool {
        self.pages.values().any(|p| p.views.editing_text(w))
    }
    pub(crate) fn page(&self, tab: TabId) -> AnyElement {
        div()
            .relative()
            .size_full()
            .when_some(self.pages.get(&tab), |d, p| {
                d.child(p.views.body()).child(p.views.dialogs())
            })
            .into_any_element()
    }
}

impl Workspace {
    pub(crate) fn focused_forward_node(&self, cx: &App) -> Option<String> {
        self.focused_tab()
            .and_then(|tab| self.tools.forwards.node(tab.id).map(str::to_owned))
            .or_else(|| self.focused_file_node(cx))
    }
    pub(crate) fn open_node_forwards(&mut self, node: &str, cx: &mut Context<Self>) {
        if !self.demo || node == "local" || crate::ble::is_ble_node(node) {
            return;
        }
        if let Some(tab) = self
            .tools
            .forwards
            .pages
            .iter()
            .find_map(|(t, p)| (p.node == node).then_some(*t))
        {
            self.activate_page(tab, cx);
            self.changed(cx);
            return;
        }
        let tab = self.layout.alloc_tab_id();
        let owner = cx.weak_entity();
        let handle = owner.clone();
        let state = cx.new(|_| ForwardsPageState::new());
        let views = ForwardsViews::configuration_preview(
            state,
            move |cx| {
                owner
                    .read_with(cx, |h, _| {
                        let node = h.tools.forwards.node(tab)?;
                        Some(ForwardsFrame {
                            tab_id: tab,
                            node_id: NodeId::new(node),
                            node_ready: false,
                            can_save: true,
                            runtime: h
                                .tools
                                .forwards
                                .rules
                                .get(node)
                                .cloned()
                                .unwrap_or_default(),
                            ports: Default::default(),
                            tokens: h.tokens,
                            i18n: h.i18n.clone(),
                            has_background: false,
                            ui_font: h.ui_font.clone(),
                            cjk_font: h.ui_font.clone(),
                            mono_font: h.tokens.metrics.markdown_code_font_family.into(),
                        })
                    })
                    .ok()
                    .flatten()
            },
            move |req, _, cx| {
                let _ = handle.update(cx, |h, cx| h.forward_draft_request(tab, req, cx));
            },
            cx,
        );
        self.tools.forwards.pages.insert(
            tab,
            Page {
                node: node.into(),
                views,
            },
        );
        let label = self
            .catalog_data
            .items
            .iter()
            .find(|r| r.id() == node)
            .map_or(node, |r| r.name.as_str());
        self.layout.insert_and_select_main_tab(Tab {
            id: tab,
            kind: TabKind::Forwards,
            title: format!("{} · {label}", self.i18n.t("sidebar.panels.forwards")),
            title_source: TabTitleSource::Static,
            root_pane: None,
            active_pane_id: None,
        });
        self.sync_active_scope(cx);
        self.changed(cx);
    }

    fn forward_draft_request(&mut self, tab: TabId, req: ForwardsRequest, cx: &mut Context<Self>) {
        let Some(page) = self.tools.forwards.pages.get(&tab) else {
            return;
        };
        if matches!(req, ForwardsRequest::SelectPage) {
            if self.focused_tab().map(|t| t.id) != Some(tab) {
                self.activate_page(tab, cx);
                self.changed(cx);
            }
            return;
        }
        let rules = &mut self
            .tools
            .forwards
            .rules
            .entry(page.node.clone())
            .or_default()
            .rules;
        let result = match req {
            ForwardsRequest::Create {
                mut rule,
                start: false,
                ..
            }
            | ForwardsRequest::Operation {
                operation: ForwardingRuntimeOperation::Save { mut rule },
                ..
            } => {
                rule.status = ForwardStatus::Stopped;
                rules.push(rule);
                Ok(())
            }
            ForwardsRequest::Operation {
                operation: ForwardingRuntimeOperation::Update { forward_id, update },
                ..
            } => {
                if let Some(r) = rules.iter_mut().find(|r| r.id == forward_id) {
                    if let Some(x) = update.forward_type {
                        r.forward_type = x;
                    }
                    if let Some(x) = update.bind_address {
                        r.bind_address = x;
                    }
                    if let Some(x) = update.bind_port {
                        r.bind_port = x;
                    }
                    if let Some(x) = update.target_host {
                        r.target_host = x;
                    }
                    if let Some(x) = update.target_port {
                        r.target_port = x;
                    }
                    if let Some(x) = update.description {
                        r.description = x;
                    }
                    Ok(())
                } else {
                    Err(self.i18n.t("forwards.toast.update_failed"))
                }
            }
            ForwardsRequest::Operation {
                operation: ForwardingRuntimeOperation::Delete { forward_id },
                ..
            } => {
                if let Some(i) = rules.iter().position(|r| r.id == forward_id) {
                    rules.remove(i);
                    Ok(())
                } else {
                    Err(self.i18n.t("forwards.toast.update_failed"))
                }
            }
            ForwardsRequest::FocusInput(..)
            | ForwardsRequest::DragInput(..)
            | ForwardsRequest::SelectPage => return,
            _ => Err(self.i18n.t("forwards.preview_hint")),
        };
        page.views.state.update(cx, |s, cx| {
            s.complete_operation(result, true, Duration::ZERO, cx)
        });
        cx.notify();
    }
}
