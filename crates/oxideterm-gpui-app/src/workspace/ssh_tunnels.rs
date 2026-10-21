use super::*;
use oxideterm_forwarding::{
    ForwardRule, ForwardStatus, ForwardType, PersistedForward, SshTunnelProfile,
};
use oxideterm_gpui_editor::{EditorPresentation, EditorSettings, TextEditorView};

mod form;

#[derive(Default)]
pub(in crate::workspace) struct SshTunnels {
    pub form: Option<TunnelForm>,
    pub pending_auth: Option<String>,
    pending_starts: HashMap<NodeId, VecDeque<String>>,
    active_start_nodes: HashSet<NodeId>,
    pending_stops: HashMap<String, bool>,
    jobs: HashMap<String, Task<()>>,
}

pub(in crate::workspace) struct TunnelForm {
    saved: Option<PersistedForward>,
    host_id: Option<String>,
    choosing_host: bool,
    name: Entity<TextEditorView>,
    group: Entity<TextEditorView>,
    bind_host: Entity<TextEditorView>,
    bind_port: Entity<TextEditorView>,
    target_host: Entity<TextEditorView>,
    target_port: Entity<TextEditorView>,
    kind: ForwardType,
    startup: bool,
    error: Option<String>,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn saved_ssh_tunnels(&self) -> Vec<PersistedForward> {
        self.forwarding_service
            .registry()
            .list_all_saved_forwards()
            .into_iter()
            .filter(|rule| rule.tunnel.is_some())
            .collect()
    }

    pub(in crate::workspace) fn open_ssh_tunnel_form(
        &mut self,
        id: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prepare_modal_interaction_boundary(cx);
        self.update_connection_form_state(cx, |state| state.form = None);
        let saved = id.and_then(|id| {
            self.saved_ssh_tunnels()
                .into_iter()
                .find(|rule| rule.id == id)
        });
        let profile = saved.as_ref().and_then(|rule| rule.tunnel.as_ref());
        let tokens = self.tokens;
        let mut input = |value: String| {
            cx.new(|cx| {
                let mut editor = TextEditorView::new_verbatim(value, &tokens, cx);
                editor.set_presentation(EditorPresentation::Inline, cx);
                editor.set_settings(
                    EditorSettings {
                        highlight_current_line: false,
                        highlight_special_chars: false,
                        indentation_markers: false,
                        placeholder: None,
                        ..Default::default()
                    },
                    cx,
                );
                editor
            })
        };
        let form = TunnelForm {
            host_id: saved
                .as_ref()
                .and_then(|rule| rule.owner_connection_id.clone()),
            choosing_host: saved.is_none(),
            name: input(profile.map(|p| p.name.clone()).unwrap_or_default()),
            group: input(profile.and_then(|p| p.group.clone()).unwrap_or_default()),
            bind_host: input("127.0.0.1".into()),
            bind_port: input(String::new()),
            target_host: input("127.0.0.1".into()),
            target_port: input(String::new()),
            kind: ForwardType::Local,
            startup: profile.is_some_and(|p| p.connect_on_startup),
            saved,
            error: None,
        };
        window.focus(&form.name.read(cx).focus_handle(cx), cx);
        self.ssh_tunnels.form = Some(form);
        cx.notify();
    }

    pub(in crate::workspace) fn close_ssh_tunnel_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ssh_tunnels.form = None;
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    fn save_ssh_tunnel(&mut self, start: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.ssh_tunnels.form.as_ref() else {
            return;
        };
        let text =
            |input: &Entity<TextEditorView>| input.read(cx).buffer().text().trim().to_string();
        let name = text(&form.name);
        let group = text(&form.group);
        let host = form
            .host_id
            .clone()
            .filter(|id| self.connection_store.get(id).is_some());
        let outcome = (|| -> Result<PersistedForward, String> {
            let host = host.ok_or_else(|| self.i18n.t("ssh_tunnel.choose_host"))?;
            if name.is_empty() || name.contains(['\r', '\n']) || group.contains(['\r', '\n']) {
                return Err(self.i18n.t("ssh_tunnel.name_required"));
            }
            if !group.is_empty() && oxideterm_connections::validate_group_name(&group).is_err() {
                return Err(self.i18n.t("ssh_tunnel.group_invalid"));
            }
            let mut saved = if let Some(saved) = &form.saved {
                // Rule edits remain on the existing forwarding page, with its stopped-rule contract.
                saved.clone()
            } else {
                let port = |input: &Entity<TextEditorView>| {
                    text(input)
                        .parse::<u16>()
                        .ok()
                        .filter(|p| *p > 0)
                        .ok_or_else(|| self.i18n.t("forwards.form.port_invalid"))
                };
                let bind = text(&form.bind_host);
                let target = text(&form.target_host);
                if bind.is_empty()
                    || bind.chars().any(char::is_whitespace)
                    || (form.kind != ForwardType::Dynamic
                        && (target.is_empty() || target.chars().any(char::is_whitespace)))
                {
                    return Err(self.i18n.t("ssh_tunnel.address_required"));
                }
                let rule = match form.kind {
                    ForwardType::Local => ForwardRule::local(
                        bind,
                        port(&form.bind_port)?,
                        target,
                        port(&form.target_port)?,
                    ),
                    ForwardType::Remote => ForwardRule::remote(
                        bind,
                        port(&form.bind_port)?,
                        target,
                        port(&form.target_port)?,
                    ),
                    ForwardType::Dynamic => ForwardRule::dynamic(bind, port(&form.bind_port)?),
                };
                PersistedForward::new(
                    rule.id.clone(),
                    String::new(),
                    Some(host),
                    rule.forward_type,
                    rule,
                    false,
                )
            };
            saved.tunnel = Some(SshTunnelProfile {
                name,
                group: (!group.is_empty()).then_some(group),
                connect_on_startup: form.startup,
            });
            saved.mark_updated();
            let store = self
                .forwarding_service
                .registry()
                .saved_store()
                .ok_or_else(|| self.i18n.t("ssh_tunnel.save_failed"))?;
            if form.saved.is_some() {
                saved = store
                    .update_tunnel_profile(&saved.id, saved.tunnel.take().unwrap())
                    .map_err(|_| self.i18n.t("ssh_tunnel.save_failed"))?;
            } else {
                store
                    .persist_forward(saved.clone())
                    .map_err(|_| self.i18n.t("ssh_tunnel.save_failed"))?;
            }
            Ok(saved)
        })();
        match outcome {
            Ok(saved) => {
                self.ssh_tunnels.form = None;
                self.queue_cloud_sync_dirty_refresh(cx);
                if start {
                    self.open_ssh_tunnel(&saved.id, false, window, cx);
                } else {
                    self.focus_active_pane(window, cx);
                }
            }
            Err(error) => {
                if let Some(form) = self.ssh_tunnels.form.as_mut() {
                    form.error = Some(error);
                }
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn open_ssh_tunnel(
        &mut self,
        id: &str,
        automatic: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ssh_tunnels.jobs.contains_key(id) || self.ssh_tunnels.pending_stops.contains_key(id)
        {
            return;
        }
        let Some(rule) = self
            .saved_ssh_tunnels()
            .into_iter()
            .find(|rule| rule.id == id)
        else {
            return;
        };
        let Some(host_id) = rule.owner_connection_id.as_deref() else {
            return;
        };
        let Some(host) = self.connection_store.get(host_id).cloned() else {
            self.push_command_palette_toast(
                self.i18n.t("ssh_tunnel.title"),
                Some(self.i18n.t("ssh_tunnel.missing_host")),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };
        if let Some(node) = self.indexed_saved_ssh_node_for_connection(host_id, &host)
            && self.node_is_ready_for_forwarding(&node)
        {
            self.queue_ssh_tunnel_start(id, node, window, cx);
            return;
        }
        let config = oxideterm_session_adapter::ssh_config_from_saved_connection(
            &self.connection_store,
            self.settings_store.settings(),
            &host,
        );
        let Some(config) = config else {
            if automatic {
                self.push_command_palette_toast(
                    rule.tunnel.as_ref().unwrap().name.clone(),
                    Some(
                        self.i18n
                            .t("sessionManager.edit_properties.password_required"),
                    ),
                    TerminalNoticeVariant::Warning,
                    cx,
                );
            } else {
                self.ssh_tunnels.pending_auth = Some(id.to_string());
                self.open_saved_connection_prompt(
                    host_id,
                    SavedConnectionPromptAction::Tunnel,
                    None,
                    window,
                    cx,
                );
            }
            return;
        };
        self.connect_ssh_tunnel_with_config(id, config, window, cx);
    }

    pub(in crate::workspace) fn connect_ssh_tunnel_with_config(
        &mut self,
        id: &str,
        config: SshConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rule) = self
            .saved_ssh_tunnels()
            .into_iter()
            .find(|rule| rule.id == id)
        else {
            return;
        };
        let Some(host_id) = rule.owner_connection_id else {
            return;
        };
        let Some(host) = self.connection_store.get(&host_id) else {
            return;
        };
        let title = host.name.clone();
        let node = if config
            .proxy_chain
            .as_ref()
            .is_some_and(|hops| !hops.is_empty())
        {
            match self.expand_saved_connection_tree(&host_id, config, title) {
                Ok(expansion) => expansion.target_node_id,
                Err(error) => {
                    self.push_command_palette_toast(
                        self.i18n.t("ssh_tunnel.title"),
                        Some(error.to_string()),
                        TerminalNoticeVariant::Error,
                        cx,
                    );
                    return;
                }
            }
        } else {
            self.materialize_ssh_root_node(config, title, Some(host_id))
        };
        self.queue_ssh_tunnel_start(id, node, window, cx);
    }

    fn queue_ssh_tunnel_start(
        &mut self,
        id: &str,
        node: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.forwarding_service.set_tunnel_start_requested(id, true);
        self.open_forwards_tab(node.clone(), window, cx);
        let queue = self
            .ssh_tunnels
            .pending_starts
            .entry(node.clone())
            .or_default();
        if !queue.iter().any(|queued| queued == id) {
            queue.push_back(id.to_string());
        }
        if self.node_is_ready_for_forwarding(&node) {
            self.resume_ssh_tunnel_starts(&node, &NodeReadiness::Ready, cx);
        } else if !self.ensure_node_connection_started(&node, cx) {
            self.ssh_tunnels.pending_starts.remove(&node);
        }
    }

    pub(in crate::workspace) fn resume_ssh_tunnel_starts(
        &mut self,
        node: &NodeId,
        readiness: &NodeReadiness,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            readiness,
            NodeReadiness::Error | NodeReadiness::Disconnected
        ) {
            self.ssh_tunnels.pending_starts.remove(node);
            return;
        }
        if *readiness != NodeReadiness::Ready {
            return;
        }
        if self.ssh_tunnels.active_start_nodes.contains(node) {
            return;
        }
        let Some(id) = self
            .ssh_tunnels
            .pending_starts
            .get_mut(node)
            .and_then(VecDeque::pop_front)
        else {
            return;
        };
        let Some(owner) = self
            .ssh_nodes
            .get(node)
            .and_then(|node| node.saved_connection_id.clone())
        else {
            return;
        };
        let service = self.forwarding_service.clone();
        let node = node.clone();
        let worker_node = node.clone();
        let worker_id = id.clone();
        // The workspace owns pending starts; closing the forwarding page cannot cancel them.
        let worker =
            tokio_util::task::AbortOnDropHandle::new(self.forwarding_runtime.spawn(async move {
                service
                    .start_saved_tunnel(worker_node, owner, worker_id)
                    .await
            }));
        self.ssh_tunnels.active_start_nodes.insert(node.clone());
        let task_id = id.clone();
        let task = cx.spawn(async move |weak, cx| {
            let outcome = worker.await;
            let _ = weak.update(cx, |this, cx| {
                this.ssh_tunnels.jobs.remove(&task_id);
                this.ssh_tunnels.active_start_nodes.remove(&node);
                match outcome {
                    Ok(Ok(())) => this.queue_cloud_sync_dirty_refresh(cx),
                    Ok(Err(error)) => this.push_command_palette_toast(
                        this.i18n.t("ssh_tunnel.title"),
                        Some(error),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                    Err(_) => this.push_command_palette_toast(
                        this.i18n.t("ssh_tunnel.title"),
                        Some(this.i18n.t("ssh_tunnel.save_failed")),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
                this.flush_ssh_tunnel_stops(cx);
                if this.node_is_ready_for_forwarding(&node) {
                    this.resume_ssh_tunnel_starts(&node, &NodeReadiness::Ready, cx);
                }
                cx.notify();
            });
        });
        self.ssh_tunnels.jobs.insert(id, task);
    }

    pub(in crate::workspace) fn start_startup_ssh_tunnels(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for saved in self.saved_ssh_tunnels() {
            if saved
                .tunnel
                .as_ref()
                .is_some_and(|profile| profile.connect_on_startup)
            {
                self.open_ssh_tunnel(&saved.id, true, window, cx);
            }
        }
    }

    pub(in crate::workspace) fn ssh_tunnel_status(&self, id: &str) -> ForwardStatus {
        if self.ssh_tunnels.jobs.contains_key(id) {
            return ForwardStatus::Starting;
        }
        if self
            .ssh_tunnels
            .pending_starts
            .values()
            .any(|queue| queue.iter().any(|queued| queued == id))
        {
            return ForwardStatus::Starting;
        }
        self.forwarding_service
            .registry()
            .session_ids()
            .into_iter()
            .filter_map(|session| self.forwarding_service.registry().get(&session))
            .flat_map(|manager| manager.list_forwards())
            .find(|rule| rule.id == id)
            .map_or(ForwardStatus::Stopped, |rule| rule.status)
    }

    pub(in crate::workspace) fn stop_ssh_tunnel(
        &mut self,
        id: &str,
        delete: bool,
        cx: &mut Context<Self>,
    ) {
        self.forwarding_service
            .set_tunnel_start_requested(id, false);
        for queue in self.ssh_tunnels.pending_starts.values_mut() {
            queue.retain(|queued| queued != id);
        }
        let busy = self
            .saved_ssh_tunnels()
            .iter()
            .find(|rule| rule.id == id)
            .and_then(|rule| rule.owner_connection_id.as_ref())
            .and_then(|host| self.saved_ssh_nodes.get(host))
            .and_then(|node| self.forwarding.read(cx).tab_for_node(node))
            .is_some_and(|tab| {
                let _scope = self.enter_forwarding_page(tab, cx);
                self.forwarding.read(cx).operation_pending()
            });
        if busy || self.ssh_tunnels.jobs.contains_key(id) {
            self.ssh_tunnels
                .pending_stops
                .entry(id.to_string())
                .and_modify(|remove| *remove |= delete)
                .or_insert(delete);
            return;
        }
        let service = self.forwarding_service.clone();
        let id = id.to_string();
        let worker_id = id.clone();
        let worker = tokio_util::task::AbortOnDropHandle::new(
            self.forwarding_runtime
                .spawn(async move { service.stop_saved_tunnel(&worker_id, delete).await }),
        );
        let task_id = id.clone();
        let task = cx.spawn(async move |weak, cx| {
            let result = worker.await;
            let _ = weak.update(cx, |this, cx| {
                this.ssh_tunnels.jobs.remove(&task_id);
                match result {
                    Ok(Ok(())) => this.queue_cloud_sync_dirty_refresh(cx),
                    _ => this.push_command_palette_toast(
                        this.i18n.t("ssh_tunnel.title"),
                        Some(this.i18n.t("ssh_tunnel.save_failed")),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
                this.flush_ssh_tunnel_stops(cx);
                cx.notify();
            });
        });
        self.ssh_tunnels.jobs.insert(id, task);
        cx.notify();
    }

    pub(in crate::workspace) fn flush_ssh_tunnel_stops(&mut self, cx: &mut Context<Self>) {
        for (id, delete) in std::mem::take(&mut self.ssh_tunnels.pending_stops) {
            self.stop_ssh_tunnel(&id, delete, cx);
        }
    }

    pub(in crate::workspace) fn move_ssh_tunnels_to_group(
        &self,
        ids: &[String],
        group: Option<&str>,
    ) -> Result<Vec<(String, Option<String>)>, String> {
        let groups = self
            .saved_ssh_tunnels()
            .into_iter()
            .filter(|saved| ids.contains(&saved.id))
            .map(|saved| (saved.id, group.map(str::to_string)))
            .collect::<Vec<_>>();
        self.apply_tunnel_groups(&groups)
    }

    pub(in crate::workspace) fn change_tunnel_group_path(
        &self,
        old: &str,
        new: Option<&str>,
    ) -> Result<Vec<(String, Option<String>)>, String> {
        let groups = self
            .saved_ssh_tunnels()
            .into_iter()
            .filter_map(|saved| {
                let group = saved.tunnel?.group?;
                let suffix = group.strip_prefix(old)?;
                if !suffix.is_empty() && !suffix.starts_with('/') {
                    return None;
                }
                Some((saved.id, new.map(|new| format!("{new}{suffix}"))))
            })
            .collect::<Vec<_>>();
        self.apply_tunnel_groups(&groups)
    }

    pub(in crate::workspace) fn apply_tunnel_groups(
        &self,
        groups: &[(String, Option<String>)],
    ) -> Result<Vec<(String, Option<String>)>, String> {
        if groups.is_empty() {
            return Ok(Vec::new());
        }
        let store = self
            .forwarding_service
            .registry()
            .saved_store()
            .ok_or_else(|| self.i18n.t("ssh_tunnel.save_failed"))?;
        let before = self
            .saved_ssh_tunnels()
            .into_iter()
            .filter(|saved| groups.iter().any(|(id, _)| id == &saved.id))
            .map(|saved| (saved.id, saved.tunnel.and_then(|profile| profile.group)))
            .collect();
        store
            .update_tunnel_groups(groups)
            .map_err(|_| self.i18n.t("ssh_tunnel.save_failed"))?;
        Ok(before)
    }
}
