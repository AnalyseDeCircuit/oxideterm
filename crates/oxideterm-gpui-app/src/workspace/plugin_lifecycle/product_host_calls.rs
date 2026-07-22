// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Workspace-owned side effects for stable product plugin APIs.

use gpui::{Context, Window};
use oxideterm_quick_commands::QuickCommandDraft;
use oxideterm_ssh::NodeId;
use serde_json::Value;
use zeroize::Zeroizing;

use super::{NativePluginProductUiEffect, WorkspaceApp};

impl WorkspaceApp {
    pub(super) fn handle_native_plugin_product_host_call(
        &mut self,
        plugin_id: &str,
        namespace: &str,
        method: &str,
        args: Value,
        cx: &mut Context<Self>,
    ) -> bool {
        match (namespace, method) {
            ("connections", "connect" | "reconnect" | "disconnect")
            | ("quickCommands", "execute") => {
                // These effects require the current GPUI Window and are consumed
                // at the beginning of the next workspace render pass.
                self.native_plugin_runtime.product_ui_effects.push_back(
                    NativePluginProductUiEffect {
                        plugin_id: plugin_id.to_string(),
                        namespace: namespace.to_string(),
                        method: method.to_string(),
                        args,
                    },
                );
                cx.notify();
            }
            ("notifications", method) => {
                self.apply_native_plugin_notification_effect(method, &args, cx)
            }
            ("quickCommands", method) => {
                self.apply_native_plugin_quick_command_effect(method, &args, cx)
            }
            ("theme", "setActive") => self.apply_native_plugin_theme_effect(&args, cx),
            ("ide", method) => self.apply_native_plugin_ide_effect(method, args, cx),
            ("ai", method) => self.apply_native_plugin_ai_effect(method, args, cx),
            ("cloudSync", method) => self.apply_native_plugin_cloud_sync_effect(method, &args, cx),
            _ => return false,
        }
        true
    }

    /// Consumes only effects that need a live window, preserving their product owners.
    pub(in crate::workspace) fn poll_native_plugin_product_ui_effects(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        while let Some(effect) = self.native_plugin_runtime.product_ui_effects.pop_front() {
            match (effect.namespace.as_str(), effect.method.as_str()) {
                ("connections", "connect") => {
                    if let Some(connection_id) = string_arg(&effect.args, "connectionId") {
                        self.open_saved_connection(connection_id, window, cx);
                    }
                }
                ("connections", "reconnect") => {
                    if let Some(node_id) = string_arg(&effect.args, "nodeId") {
                        self.ensure_node_connection_started(&NodeId::new(node_id.to_string()));
                        cx.notify();
                    }
                }
                ("connections", "disconnect") => {
                    if let Some(node_id) = string_arg(&effect.args, "nodeId") {
                        // Shared-node disconnect preserves the product's normal
                        // cascade confirmation before NodeRouter cleanup runs.
                        self.request_disconnect_ssh_node(&NodeId::new(node_id.to_string()), cx);
                    }
                }
                ("quickCommands", "execute") => {
                    if let Some(command_id) = string_arg(&effect.args, "id")
                        && let Some(command) = self
                            .quick_commands
                            .commands
                            .iter()
                            .find(|command| command.id == command_id)
                            .map(|command| command.command.clone())
                    {
                        self.run_quick_command(&command, window, cx);
                    }
                }
                _ => self.native_plugin_runtime.registry.record_manager_error(
                    effect.plugin_id,
                    "Unsupported queued product plugin effect".to_string(),
                ),
            }
        }
    }

    fn apply_native_plugin_notification_effect(
        &mut self,
        method: &str,
        args: &Value,
        cx: &mut Context<Self>,
    ) {
        let notifications = &mut self.notification_center.notifications;
        match method {
            "markRead" => {
                if let Some(id) = args.get("id").and_then(Value::as_u64) {
                    notifications.mark_read(id);
                }
            }
            "markAllRead" => notifications.mark_all_read(),
            "setDnd" => {
                if let Some(enabled) = args.get("enabled").and_then(Value::as_bool)
                    && notifications.dnd_enabled != enabled
                {
                    notifications.toggle_dnd();
                }
            }
            "remove" => {
                if let Some(id) = args.get("id").and_then(Value::as_u64) {
                    notifications.remove(id);
                }
            }
            "clear" => notifications.clear(),
            _ => return,
        }
        cx.notify();
    }

    fn apply_native_plugin_quick_command_effect(
        &mut self,
        method: &str,
        args: &Value,
        cx: &mut Context<Self>,
    ) {
        match method {
            "upsert" => {
                let Some(name) = string_arg(args, "name") else {
                    return;
                };
                let Some(command) = string_arg(args, "command") else {
                    return;
                };
                self.quick_commands.upsert_command(QuickCommandDraft {
                    id: string_arg(args, "id").map(str::to_string),
                    name: name.to_string(),
                    command: command.to_string(),
                    category: string_arg(args, "category").unwrap_or("custom").to_string(),
                    description: string_arg(args, "description")
                        .unwrap_or_default()
                        .to_string(),
                    host_pattern: string_arg(args, "hostPattern")
                        .unwrap_or_default()
                        .to_string(),
                });
            }
            "remove" => {
                if let Some(id) = string_arg(args, "id") {
                    self.quick_commands.delete_command(id);
                }
            }
            _ => return,
        }
        cx.notify();
    }

    fn apply_native_plugin_theme_effect(&mut self, args: &Value, cx: &mut Context<Self>) {
        let Some(theme_id) = string_arg(args, "themeId") else {
            return;
        };
        let valid = oxideterm_theme::BUILT_IN_THEMES
            .iter()
            .any(|theme| theme.id == theme_id)
            || self
                .settings_store
                .settings()
                .custom_themes
                .contains_key(theme_id);
        if !valid {
            return;
        }
        let theme_id = theme_id.to_string();
        self.edit_settings(|settings| settings.terminal.theme = theme_id, cx);
    }

    fn apply_native_plugin_ide_effect(
        &mut self,
        method: &str,
        args: Value,
        cx: &mut Context<Self>,
    ) {
        let requested_node_id = string_arg(&args, "nodeId").map(str::to_string);
        let surface = requested_node_id
            .as_deref()
            .and_then(|node_id| {
                self.ide_tab_surfaces.values().find_map(|surface| {
                    surface
                        .read(cx)
                        .plugin_snapshot()
                        .filter(|snapshot| snapshot.project.node_id == node_id)
                        .map(|_| surface.clone())
                })
            })
            .or_else(|| self.active_ide_surface())
            .or_else(|| self.ide_tab_surfaces.values().next().cloned());
        let Some(surface) = surface else {
            return;
        };
        surface.update(cx, |surface, cx| match method {
            "openFile" => string_arg(&args, "path")
                .is_some_and(|path| surface.plugin_open_remote_file(path.to_string(), cx)),
            "replaceActiveText" => args
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| surface.plugin_replace_active_text(text.to_string(), cx)),
            "insertActiveText" => args
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| surface.plugin_insert_active_text(text.to_string(), cx)),
            "saveActive" => surface.plugin_save_active(cx),
            "closeFile" => string_arg(&args, "path")
                .is_some_and(|path| surface.plugin_close_remote_file(path, cx)),
            "refreshProject" => {
                surface.refresh_project_tree_root(cx);
                true
            }
            _ => false,
        });
    }

    fn apply_native_plugin_ai_effect(&mut self, method: &str, args: Value, cx: &mut Context<Self>) {
        match method {
            "createConversation" => {
                self.create_ai_sidebar_conversation(
                    string_arg(&args, "title").map(str::to_string),
                    cx,
                );
            }
            "selectConversation" => {
                if let Some(id) = string_arg(&args, "conversationId") {
                    self.select_ai_conversation(id.to_string());
                    cx.notify();
                }
            }
            "sendMessage" => {
                if let Some(content) = args.get("content").and_then(Value::as_str) {
                    // Plugin text is a sensitive boundary: redact credential-like
                    // material before the existing AI workflow builds model context.
                    let content = Zeroizing::new(content.to_string());
                    self.ai.chat.draft = oxideterm_ai::sanitize_for_ai(content.as_str());
                    self.send_ai_chat_draft(cx);
                }
            }
            "cancelGeneration" => self.cancel_ai_chat_stream(cx),
            "deleteConversation" => {
                if let Some(id) = string_arg(&args, "conversationId") {
                    self.delete_ai_conversation(id);
                    cx.notify();
                }
            }
            "clearConversations" => {
                self.clear_ai_conversations();
                cx.notify();
            }
            _ => {}
        }
    }

    fn apply_native_plugin_cloud_sync_effect(
        &mut self,
        method: &str,
        args: &Value,
        cx: &mut Context<Self>,
    ) {
        match method {
            "check" => self.start_cloud_sync_check_with_options(false, cx),
            "upload" => self.start_cloud_sync_upload_with_options(
                args.get("force").and_then(Value::as_bool).unwrap_or(false),
                false,
                false,
                cx,
            ),
            "pullPreview" => self.start_cloud_sync_pull_preview_with_options(false, cx),
            "applyPreview" => self.start_cloud_sync_apply_preview(cx),
            "setAutoUpload" => {
                let Some(enabled) = args.get("enabled").and_then(Value::as_bool) else {
                    return;
                };
                let state = self.cloud_sync.controller.store.state_mut();
                state.settings.auto_upload_enabled = enabled;
                if let Some(interval) = args.get("intervalMinutes").and_then(Value::as_f64) {
                    state.settings.auto_upload_interval_mins = interval.max(5.0);
                }
                self.save_cloud_sync_state();
                self.reschedule_cloud_sync_auto_upload(cx);
                cx.notify();
            }
            _ => {}
        }
    }
}

fn string_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
