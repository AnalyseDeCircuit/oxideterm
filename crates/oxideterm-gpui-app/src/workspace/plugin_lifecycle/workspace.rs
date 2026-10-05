// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::{App, Context, Window};
use oxideterm_plugin_host_api::workspace::{WorkspaceDestination, WorkspacePage};
use oxideterm_terminal_recording::TerminalRecordingState;
use serde_json::{Value, json};

use super::WorkspaceApp;
use crate::workspace::{TabId, TabKind};

impl WorkspaceApp {
    pub(super) fn native_plugin_workspace_summary(&self, cx: &App) -> Value {
        let host = self.tab_host.read(cx);
        let tabs = host
            .tabs()
            .iter()
            .filter_map(|tab| {
                let kind = match tab.kind {
                    TabKind::Workspace
                    | TabKind::LocalTerminal
                    | TabKind::SshTerminal
                    | TabKind::MoshTerminal => "terminal",
                    TabKind::Ide => "project",
                    TabKind::Sftp | TabKind::FileManager => "files",
                    TabKind::RemoteDesktop => "desktop",
                    TabKind::Forwards => "forwards",
                    _ => return None,
                };
                let mut panes = Vec::new();
                if let Some(root) = &tab.root_pane {
                    root.collect_pane_ids(&mut panes);
                }
                let recordings = panes
                    .iter()
                    .filter_map(|id| host.panes().get(id))
                    .map(|pane| pane.read(cx).recording_status())
                    .filter(|status| status.state != TerminalRecordingState::Idle)
                    .map(|status| {
                        json!({
                            "paused": status.state == TerminalRecordingState::Paused,
                            "elapsedSeconds": status.elapsed.as_secs(),
                        })
                    })
                    .collect::<Vec<_>>();
                Some(json!({
                    "id": tab.id.0.to_string(),
                    "title": oxideterm_audit::redact(&tab.title),
                    "kind": kind,
                    "recordings": recordings,
                    "transferOwners": self.sftp_page_remote_keys(tab.id, cx),
                }))
            })
            .collect::<Vec<_>>();
        let tree = self.native_plugin_session_tree_snapshot_values();
        let nodes = tree
            .iter()
            .map(|node| {
                let state = match node["connectionState"].as_str().unwrap_or("error") {
                    "idle" | "link-down" => "disconnected",
                    state => state,
                };
                let forwards = node["connectionId"]
                    .as_str()
                    .and_then(|id| self.forwarding_service.registry().get(id))
                    .map(|manager| {
                        manager
                            .list_forwards()
                            .iter()
                            .filter(|rule| {
                                matches!(
                                    rule.status,
                                    oxideterm_forwarding::ForwardStatus::Starting
                                        | oxideterm_forwarding::ForwardStatus::Active
                                )
                            })
                            .count()
                    })
                    .unwrap_or(0);
                json!({
                    "id": node["id"],
                    "title": oxideterm_audit::redact(node["label"].as_str().unwrap_or_default()),
                    "state": state,
                    "forwards": forwards,
                })
            })
            .collect::<Vec<_>>();
        let registry = self.plugin_entity.read(cx).registry();
        let issues = registry
            .diagnostics()
            .iter()
            .filter_map(|diagnostic| diagnostic.plugin_id.as_deref())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|id| {
                let name = registry
                    .plugins()
                    .iter()
                    .find(|plugin| plugin.manifest.id == id)
                    .map(|plugin| plugin.manifest.name.as_str())
                    .unwrap_or(id);
                json!({ "id": id, "name": oxideterm_audit::redact(name) })
            })
            .collect::<Vec<_>>();
        json!({ "tabs": tabs, "nodes": nodes, "pluginIssues": issues })
    }

    pub(super) fn open_native_plugin_workspace_destination(
        &mut self,
        plugin_id: &str,
        args: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Ok(destination) = serde_json::from_value::<WorkspaceDestination>(args.clone()) else {
            return;
        };
        match destination {
            WorkspaceDestination::Tab { id } => {
                let Some(tab) = id
                    .parse::<u64>()
                    .ok()
                    .map(TabId)
                    .filter(|id| self.tabs(cx).iter().any(|tab| tab.id == *id))
                else {
                    return;
                };
                if !self.focus_detached_tab_window(tab, cx) {
                    self.set_active_tab(tab, window, cx);
                }
            }
            WorkspaceDestination::Page { page } => match page {
                WorkspacePage::Sessions => self.open_session_manager_tab(window, cx),
                WorkspacePage::Files => self.open_file_manager_tab(window, cx),
                WorkspacePage::Plugins => self.open_plugin_manager_tab(window, cx),
                WorkspacePage::CloudSync => self.open_cloud_sync_tab(window, cx),
                WorkspacePage::Notifications => self.open_notification_center_tab(window, cx),
                WorkspacePage::LocalTerminal => {
                    if let Err(error) = self.create_local_terminal_tab(window, cx) {
                        self.plugin_entity.update(cx, |plugins, _cx| {
                            plugins.registry_mut().record_manager_error(
                                plugin_id.to_string(),
                                oxideterm_audit::redact(&error.to_string()).to_string(),
                            );
                        });
                    }
                }
            },
            WorkspaceDestination::Sftp { node_id } => {
                let node_id = oxideterm_ssh::NodeId::new(node_id);
                if self.node_router.contains_node(&node_id) {
                    self.open_sftp_tab(node_id, window, cx);
                }
            }
            WorkspaceDestination::Forwards { node_id } => {
                let node_id = oxideterm_ssh::NodeId::new(node_id);
                if self.node_router.contains_node(&node_id) {
                    self.open_forwards_tab(node_id, window, cx);
                }
            }
        }
    }
}
