use super::*;
use crate::workspace::{
    ime::WorkspaceImeTarget, settings_mono_font_family, settings_ui_font_family,
};
use oxideterm_gpui_forwards::{ForwardsFrame, ForwardsRequest};
use oxideterm_gpui_ui::typography::tauri_cjk_ui_font_family;
use std::rc::Rc;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_forwards_tab(
        &mut self,
        node_id: NodeId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let title = format!("{} · {}", self.i18n.t("forwards.table.title"), node_title);
        let tab_id = if let Some(tab_id) = self.forwarding.read(cx).tab_for_node(&node_id) {
            tab_id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::Forwards,
                    title,
                    title_source: TabTitleSource::Static,
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            self.forwarding.update(cx, |forwarding, _cx| {
                forwarding.map_tab_to_node(tab_id, node_id.clone(), _cx);
            });
            tab_id
        };

        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.forwarding
            .update(cx, |state, cx| state.select_page(tab_id, cx));
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.active_ssh_node_id = Some(node_id.clone());
        // Tauri opens the forwarding surface against the selected node but
        // leaves connection establishment to the explicit connect path.
        if let Some(page) = self.forwarding.read(cx).page_state(tab_id) {
            page.update(cx, |page, cx| {
                page.clear_error();
                cx.notify();
            });
        }
        self.start_port_profiler_for_node(node_id, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_forwards_surface(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(tab) = self.active_tab_id(cx) {
            self.render_forwards_surface_for_tab(tab, window, cx)
        } else {
            self.render_empty_workspace(f32::from(window.viewport_size().width), cx)
        }
    }

    pub(in crate::workspace) fn render_forwards_surface_for_tab(
        &mut self,
        tab: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_forwarding_sampling_visibility(cx);
        self.mount_forwarding_page(tab, cx);
        if let Some(views) = self.forwarding.read(cx).views(tab) {
            views.body()
        } else {
            self.render_empty_workspace(f32::from(window.viewport_size().width), cx)
        }
    }

    pub(in crate::workspace) fn render_forwards_dialogs(
        &mut self,
        tab: TabId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.mount_forwarding_page(tab, cx);
        self.forwarding
            .read(cx)
            .views(tab)
            .map(ForwardsViews::dialogs)
    }

    pub(in crate::workspace) fn forwarding_page(
        &self,
        cx: &App,
    ) -> Option<gpui::Entity<ForwardsPageState>> {
        let forwarding = self.forwarding.read(cx);
        forwarding.page_state(forwarding.page_id()?)
    }

    fn mount_forwarding_page(&mut self, tab: TabId, cx: &mut Context<Self>) {
        let forwarding = self.forwarding.read(cx);
        if forwarding.views(tab).is_some() || !forwarding.has_page(tab) {
            return;
        }
        let dispatch = cx.weak_entity();
        let frame = dispatch.clone();
        let ime = dispatch.clone();
        let bindings = ForwardsViewBindings {
            frame: Rc::new(move |cx| frame.upgrade()?.read(cx).forwards_frame(tab, cx)),
            ime: Rc::new(move |input, cx| {
                ime.upgrade()?.read(cx).forwards_ime_frame(tab, input, cx)
            }),
            dispatch: Rc::new(move |request, window, cx| {
                let _ = dispatch.update(cx, |this, cx| {
                    this.dispatch_forwards_request(tab, request, window, cx)
                });
            }),
        };
        self.forwarding.update(cx, |forwarding, cx| {
            forwarding.mount_page(tab, bindings, cx)
        });
    }

    fn forwards_frame(&self, tab: TabId, cx: &App) -> Option<ForwardsFrame> {
        let forwarding = self.forwarding.read(cx);
        let node_id = forwarding.node_for_tab(tab)?;
        let node = self.ssh_nodes.get(&node_id);
        let settings = self.settings_store.settings();
        Some(ForwardsFrame {
            tab_id: tab,
            node_ready: node.is_some_and(|node| node.readiness == NodeReadiness::Ready),
            can_save: node.is_some_and(|node| node.saved_connection_id.is_some()),
            runtime: forwarding.runtime_snapshot(&node_id),
            ports: forwarding
                .port_detection_state(&node_id)
                .unwrap_or_default(),
            node_id,
            tokens: self.tokens,
            i18n: self.i18n.clone(),
            has_background: self.background_surface_active("forwards"),
            ui_font: settings_ui_font_family(&settings.appearance.ui_font_family).into(),
            cjk_font: tauri_cjk_ui_font_family(&settings.appearance.ui_font_family),
            mono_font: settings_mono_font_family(settings),
        })
    }

    fn dispatch_forwards_request(
        &mut self,
        tab: TabId,
        request: ForwardsRequest,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let Some(node) = self.forwarding.read(cx).node_for_tab(tab) else {
            return;
        };
        match request {
            ForwardsRequest::SelectPage => self
                .forwarding
                .update(cx, |forwarding, cx| forwarding.select_page(tab, cx)),
            ForwardsRequest::FocusInput(input, event) => {
                let Some(window) = window else {
                    return;
                };
                self.forwarding
                    .update(cx, |forwarding, cx| forwarding.select_page(tab, cx));
                if let Some(state) = self.forwarding.read(cx).page_state(tab) {
                    state.update(cx, |state, cx| {
                        state.focus_input(input);
                        cx.notify();
                    });
                }
                self.needs_active_pane_focus = false;
                self.ime_marked_text = None;
                window.focus(&self.focus_handle, cx);
                self.begin_ime_selection_from_mouse_down(
                    WorkspaceImeTarget::Forwards(tab, input),
                    &event,
                    window,
                    cx,
                );
            }
            ForwardsRequest::DragInput(event) => {
                if let Some(window) = window {
                    self.update_ime_selection_drag_from_mouse_move(&event, window, cx);
                }
            }
            ForwardsRequest::Refresh => {
                self.forwarding.update(cx, |forwarding, cx| {
                    forwarding.refresh_runtime_snapshot(&node, cx)
                });
            }
            ForwardsRequest::ScanPorts => self.start_port_scan(node, true, cx),
            ForwardsRequest::DismissPort(port) => self.forwarding.update(cx, |forwarding, cx| {
                forwarding.dismiss_detected_port(&node, port, cx)
            }),
            ForwardsRequest::Create {
                rule,
                start,
                check_health,
            } => self.create_forward(tab, node, rule, start, check_health, cx),
            ForwardsRequest::Operation {
                operation,
                message_key,
                sync_saved,
            } => self.start_forward_operation(tab, node, message_key, sync_saved, operation, cx),
        }
    }
}
