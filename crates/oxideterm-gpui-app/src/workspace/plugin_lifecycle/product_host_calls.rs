// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Workspace-owned side effects for stable product plugin APIs.

use gpui::{Context, Window};
use oxideterm_quick_commands::{
    QuickCommandConfirmationPolicy, QuickCommandDraft, QuickCommandParameter,
    QuickCommandTargetProtocol,
};
use oxideterm_ssh::NodeId;
use serde::de::DeserializeOwned;
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
                // These effects require a live GPUI Window, so the Entity keeps
                // them until its reliable delivery event reaches the adapter.
                self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.enqueue_product_ui_effect(NativePluginProductUiEffect {
                        plugin_id: plugin_id.to_string(),
                        namespace: namespace.to_string(),
                        method: method.to_string(),
                        args,
                    });
                });
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
    pub(in crate::workspace) fn apply_native_plugin_product_ui_effects(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (effects, backlog_remaining) = self
            .plugin_entity
            .update(cx, |plugins, _cx| plugins.take_product_ui_effects());
        for effect in effects {
            match (effect.namespace.as_str(), effect.method.as_str()) {
                ("connections", "connect") => {
                    if let Some(connection_id) = string_arg(&effect.args, "connectionId") {
                        self.open_saved_connection(connection_id, window, cx);
                    }
                }
                ("connections", "reconnect") => {
                    if let Some(node_id) = string_arg(&effect.args, "nodeId") {
                        self.ensure_node_connection_started(&NodeId::new(node_id.to_string()), cx);
                        cx.notify();
                    }
                }
                ("connections", "disconnect") => {
                    if let Some(node_id) = string_arg(&effect.args, "nodeId") {
                        // User-facing disconnect policy is resolved before NodeRouter cleanup.
                        self.request_disconnect_ssh_node(
                            &NodeId::new(node_id.to_string()),
                            window,
                            cx,
                        );
                    }
                }
                ("quickCommands", "execute") => {
                    if let Some(command_id) = string_arg(&effect.args, "id")
                        && let Some(command) = self
                            .terminal
                            .read(cx)
                            .quick_commands
                            .store
                            .commands
                            .iter()
                            .find(|command| command.id == command_id)
                            .cloned()
                    {
                        self.run_quick_command_model(&command, window, cx);
                    }
                }
                _ => {
                    self.plugin_entity.update(cx, |plugins, _cx| {
                        plugins.registry_mut().record_manager_error(
                            effect.plugin_id,
                            "Unsupported queued product plugin effect".to_string(),
                        );
                    });
                }
            }
        }
        backlog_remaining
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
                // Structured fields are decoded before the queued effect mutates product state.
                let Ok(parameters) =
                    optional_json_arg::<Vec<QuickCommandParameter>>(args, "parameters")
                else {
                    return;
                };
                let Ok(protocols) =
                    optional_json_arg::<Vec<QuickCommandTargetProtocol>>(args, "protocols")
                else {
                    return;
                };
                let Ok(confirmation) =
                    optional_json_arg::<QuickCommandConfirmationPolicy>(args, "confirmation")
                else {
                    return;
                };
                let host_patterns = match optional_json_arg::<Vec<String>>(args, "hostPatterns") {
                    Ok(Some(patterns)) => Some(patterns),
                    Ok(None) => {
                        string_arg(args, "hostPattern").map(|pattern| vec![pattern.to_string()])
                    }
                    Err(()) => return,
                };
                self.terminal.update(cx, |terminal, _cx| {
                    terminal
                        .quick_commands
                        .store
                        .upsert_command(QuickCommandDraft {
                            id: string_arg(args, "id").map(str::to_string),
                            name: name.to_string(),
                            command: command.to_string(),
                            category: string_arg(args, "category").map(str::to_string),
                            description: args
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            parameters,
                            protocols,
                            host_patterns,
                            confirmation,
                        });
                });
            }
            "remove" => {
                if let Some(id) = string_arg(args, "id") {
                    self.terminal.update(cx, |terminal, _cx| {
                        terminal.quick_commands.delete_command(id)
                    });
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
        let surface = self
            .ide_workspace
            .read(cx)
            .surface_for_effect(self.active_tab_id(cx), requested_node_id.as_deref());
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
                    self.select_ai_conversation(id.to_string(), cx);
                    cx.notify();
                }
            }
            "sendMessage" => {
                if let Some(content) = args.get("content").and_then(Value::as_str) {
                    // Plugin text is a sensitive boundary: redact credential-like
                    // material before the existing AI workflow builds model context.
                    let content = Zeroizing::new(content.to_string());
                    let sanitized = oxideterm_ai::sanitize_for_ai(content.as_str());
                    self.ai_entity.update(cx, |ai, _cx| {
                        ai.set_chat_draft(sanitized);
                    });
                    self.send_ai_chat_draft(cx);
                }
            }
            "cancelGeneration" => self.cancel_ai_chat_stream(cx),
            "deleteConversation" => {
                if let Some(id) = string_arg(&args, "conversationId") {
                    self.delete_ai_conversation(id, cx);
                    cx.notify();
                }
            }
            "clearConversations" => {
                self.clear_ai_conversations(cx);
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
                let interval = args.get("intervalMinutes").and_then(Value::as_f64);
                self.cloud_sync.update(cx, |cloud_sync, cx| {
                    let settings = &mut cloud_sync.controller.store.state_mut().settings;
                    settings.auto_upload_enabled = enabled;
                    if let Some(interval) = interval {
                        settings.auto_upload_interval_mins = interval.max(5.0);
                    }
                    cx.notify();
                });
                self.save_cloud_sync_state(cx);
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

fn optional_json_arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<Option<T>, ()> {
    args.get(key)
        .map(|value| serde_json::from_value(value.clone()).map_err(|_| ()))
        .transpose()
}
