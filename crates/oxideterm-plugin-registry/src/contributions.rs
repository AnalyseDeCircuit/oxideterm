// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Contribution indexing and runtime registration rows.

use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativePluginContributionStore {
    pub tabs: Vec<NativePluginTabContribution>,
    pub sidebar_panels: Vec<NativePluginSidebarContribution>,
    pub settings: Vec<NativePluginSettingContribution>,
    pub ai_tools: Vec<NativePluginAiToolContribution>,
    pub terminal_shortcuts: Vec<NativePluginShortcutContribution>,
    pub terminal_transports: Vec<NativePluginTransportContribution>,
    pub connection_hooks: Vec<NativePluginConnectionHookContribution>,
    pub api_commands: Vec<NativePluginApiCommandContribution>,
    pub host_monitors: Vec<NativePluginHostMonitorContribution>,
    pub runtime_commands: Vec<NativePluginRuntimeCommandContribution>,
    pub runtime_keybindings: Vec<NativePluginRuntimeKeybindingContribution>,
    pub runtime_context_menus: Vec<NativePluginRuntimeContextMenuContribution>,
    pub runtime_status_items: Vec<NativePluginRuntimeStatusItemContribution>,
    pub runtime_tab_views: Vec<NativePluginRuntimeTabViewContribution>,
    pub runtime_sidebar_panels: Vec<NativePluginRuntimeSidebarPanelContribution>,
    pub runtime_event_subscriptions: Vec<NativePluginRuntimeEventSubscriptionContribution>,
    pub runtime_terminal_input_interceptors: Vec<NativePluginRuntimeTerminalHookContribution>,
    pub runtime_terminal_output_processors: Vec<NativePluginRuntimeTerminalHookContribution>,
}

impl NativePluginContributionStore {
    pub(crate) fn from_plugins(plugins: &[NativePluginInfo]) -> Self {
        let mut store = Self::default();
        for plugin in plugins {
            if !native_plugin_contributions_enabled(plugin) {
                continue;
            }
            store.extend_from_plugin(plugin);
        }
        store
    }

    fn extend_from_plugin(&mut self, plugin: &NativePluginInfo) {
        let Some(contributes) = &plugin.manifest.contributes else {
            return;
        };
        let plugin_id = plugin.manifest.id.clone();
        let plugin_name = plugin.manifest.name.clone();

        if let Some(tabs) = &contributes.tabs {
            self.tabs.extend(
                tabs.iter()
                    .cloned()
                    .map(|definition| NativePluginTabContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }),
            );
        }
        if let Some(sidebar_panels) = &contributes.sidebar_panels {
            self.sidebar_panels
                .extend(sidebar_panels.iter().cloned().map(|definition| {
                    NativePluginSidebarContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }
                }));
        }
        if let Some(settings) = &contributes.settings {
            self.settings
                .extend(settings.iter().cloned().map(|definition| {
                    NativePluginSettingContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }
                }));
        }
        if let Some(ai_tools) = &contributes.ai_tools {
            self.ai_tools
                .extend(ai_tools.iter().cloned().map(|definition| {
                    NativePluginAiToolContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }
                }));
        }
        if let Some(hooks) = &contributes.terminal_hooks
            && let Some(shortcuts) = &hooks.shortcuts
        {
            self.terminal_shortcuts
                .extend(shortcuts.iter().cloned().map(|definition| {
                    NativePluginShortcutContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }
                }));
        }
        if let Some(transports) = &contributes.terminal_transports {
            self.terminal_transports
                .extend(transports.iter().cloned().map(|transport| {
                    NativePluginTransportContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        transport,
                    }
                }));
        }
        if let Some(connection_hooks) = &contributes.connection_hooks {
            self.connection_hooks
                .extend(connection_hooks.iter().cloned().map(|hook| {
                    NativePluginConnectionHookContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        hook,
                    }
                }));
        }
        if let Some(api_commands) = &contributes.api_commands {
            self.api_commands
                .extend(api_commands.iter().cloned().map(|command| {
                    NativePluginApiCommandContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        command,
                    }
                }));
        }
        if let Some(host_monitors) = &contributes.host_monitors {
            self.host_monitors
                .extend(host_monitors.iter().cloned().map(|definition| {
                    NativePluginHostMonitorContribution {
                        plugin_id: plugin_id.clone(),
                        plugin_name: plugin_name.clone(),
                        definition,
                    }
                }));
        }
    }

    #[allow(dead_code)]
    pub fn total_count(&self) -> usize {
        self.tabs.len()
            + self.sidebar_panels.len()
            + self.settings.len()
            + self.ai_tools.len()
            + self.terminal_shortcuts.len()
            + self.terminal_transports.len()
            + self.connection_hooks.len()
            + self.api_commands.len()
            + self.host_monitors.len()
            + self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len()
    }

    /// Returns only monitors owned by the requesting plugin.
    pub fn host_monitors_for_plugin(
        &self,
        plugin_id: &str,
    ) -> Vec<NativePluginHostMonitorContribution> {
        self.host_monitors
            .iter()
            .filter(|entry| entry.plugin_id == plugin_id)
            .cloned()
            .collect()
    }

    /// Resolves a monitor inside its declaring plugin namespace.
    pub fn host_monitor(
        &self,
        plugin_id: &str,
        monitor_id: &str,
    ) -> Option<NativePluginHostMonitorContribution> {
        self.host_monitors
            .iter()
            .find(|entry| entry.plugin_id == plugin_id && entry.definition.id == monitor_id)
            .cloned()
    }

    pub fn ai_tool_definitions(&self) -> Vec<oxideterm_ai::AiToolDefinition> {
        self.ai_tools
            .iter()
            .map(|tool| {
                let qualified_name =
                    native_plugin_ai_tool_name(&tool.plugin_id, &tool.definition.name);
                // Phase 2 exposes metadata to the model but keeps execution
                // guarded by the native runtime boundary that starts in Phase 3.
                oxideterm_ai::AiToolDefinition {
                    name: qualified_name,
                    description: format!(
                        "[Plugin: {}] {}",
                        tool.plugin_name, tool.definition.description
                    ),
                    parameters: tool.definition.parameters.clone().unwrap_or_else(
                        || serde_json::json!({ "type": "object", "properties": {} }),
                    ),
                }
            })
            .collect()
    }

    pub fn ai_tool_names(&self) -> Vec<String> {
        self.ai_tools
            .iter()
            .map(|tool| native_plugin_ai_tool_name(&tool.plugin_id, &tool.definition.name))
            .collect()
    }

    pub fn runtime_keybinding_for_normalized_key(
        &self,
        normalized_keybinding: &str,
    ) -> Option<&NativePluginRuntimeKeybindingContribution> {
        // Tauri's plugin store iterates Map values and returns the first
        // normalized match, so earlier plugin registrations keep priority when
        // two plugins claim the same keybinding.
        self.runtime_keybindings
            .iter()
            .find(|entry| entry.normalized_keybinding == normalized_keybinding)
    }

    pub fn runtime_event_subscriptions_for(
        &self,
        event: &str,
    ) -> Vec<NativePluginRuntimeEventSubscriptionContribution> {
        // Event delivery runs outside render, so clone the compact subscription
        // rows here and let WorkspaceApp hand them to the async runtime bridge.
        self.runtime_event_subscriptions
            .iter()
            .filter(|entry| entry.event == event)
            .cloned()
            .collect()
    }

    pub fn tab_contribution(
        &self,
        plugin_id: &str,
        tab_id: &str,
    ) -> Option<NativePluginTabContribution> {
        self.tabs
            .iter()
            .find(|entry| entry.plugin_id == plugin_id && entry.definition.id == tab_id)
            .cloned()
    }

    pub fn runtime_tab_view(
        &self,
        plugin_id: &str,
        tab_id: &str,
    ) -> Option<NativePluginRuntimeTabViewContribution> {
        self.runtime_tab_views
            .iter()
            .find(|entry| entry.plugin_id == plugin_id && entry.tab_id == tab_id)
            .cloned()
    }

    pub fn runtime_sidebar_panels(&self) -> Vec<NativePluginRuntimeSidebarPanelContribution> {
        let mut panels = self.runtime_sidebar_panels.clone();
        // Tauri lets sidebar panel definitions opt into top/bottom groups. Keep
        // that position ordering before title sorting so native panels do not
        // reshuffle every render.
        panels.sort_by(|left, right| {
            native_plugin_sidebar_position_sort_key(&left.position)
                .cmp(&native_plugin_sidebar_position_sort_key(&right.position))
                .then_with(|| left.title.cmp(&right.title))
                .then_with(|| left.panel_id.cmp(&right.panel_id))
        });
        panels
    }

    pub(crate) fn apply_runtime_tab_view(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
        manifest: &NativePluginManifest,
    ) -> Result<(), String> {
        validate_manifest_text_field("runtime.tab.registrationId", &registration.registration_id)?;
        let tab_id = runtime_metadata_string(&registration.metadata, "tabId")
            .or_else(|| runtime_metadata_string(&registration.metadata, "id"))
            .ok_or_else(|| "Runtime tab registration requires metadata.tabId".to_string())?;
        validate_manifest_text_field("runtime.tab.tabId", &tab_id)?;
        let tab_def = manifest_declared_tab(manifest, &tab_id)
            .ok_or_else(|| format!("Tab \"{tab_id}\" not declared in manifest contributes.tabs"))?;
        let schema = runtime_declarative_ui_schema(&registration.metadata)?;
        validate_native_plugin_declarative_ui_schema(&schema)?;

        // Native replaces the Tauri React component with a validated data
        // schema. Re-registering the same id acts as the first patch mechanism
        // and goes through identical validation before replacing host state.
        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        self.runtime_tab_views
            .push(NativePluginRuntimeTabViewContribution {
                plugin_id: registration.plugin_id,
                plugin_name,
                registration_id: registration.registration_id,
                tab_id,
                title: tab_def.title.clone(),
                icon: tab_def.icon.clone(),
                schema,
            });
        Ok(())
    }

    pub(crate) fn apply_runtime_sidebar_panel(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
        manifest: &NativePluginManifest,
    ) -> Result<(), String> {
        validate_manifest_text_field(
            "runtime.sidebarPanel.registrationId",
            &registration.registration_id,
        )?;
        let panel_id = runtime_metadata_string(&registration.metadata, "panelId")
            .or_else(|| runtime_metadata_string(&registration.metadata, "id"))
            .ok_or_else(|| {
                "Runtime sidebar panel registration requires metadata.panelId".to_string()
            })?;
        validate_manifest_text_field("runtime.sidebarPanel.panelId", &panel_id)?;
        let panel_def = manifest_declared_sidebar_panel(manifest, &panel_id).ok_or_else(|| {
            format!(
                "Sidebar panel \"{panel_id}\" not declared in manifest contributes.sidebarPanels"
            )
        })?;
        let schema = runtime_declarative_ui_schema(&registration.metadata)?;
        validate_native_plugin_declarative_ui_schema(&schema)?;

        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        self.runtime_sidebar_panels
            .push(NativePluginRuntimeSidebarPanelContribution {
                plugin_id: registration.plugin_id,
                plugin_name,
                registration_id: registration.registration_id,
                panel_id,
                title: panel_def.title.clone(),
                icon: panel_def.icon.clone(),
                position: panel_def.position.clone(),
                schema,
            });
        Ok(())
    }

    pub(crate) fn apply_runtime_terminal_shortcut(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
        manifest: &NativePluginManifest,
    ) -> Result<(), String> {
        validate_manifest_text_field(
            "runtime.terminalShortcut.registrationId",
            &registration.registration_id,
        )?;
        let command = runtime_metadata_string(&registration.metadata, "command")
            .or_else(|| runtime_metadata_string(&registration.metadata, "id"))
            .unwrap_or_else(|| registration.registration_id.clone());
        validate_manifest_text_field("runtime.terminalShortcut.command", &command)?;
        let declared = manifest
            .contributes
            .as_ref()
            .and_then(|contributes| contributes.terminal_hooks.as_ref())
            .and_then(|hooks| hooks.shortcuts.as_ref())
            .and_then(|shortcuts| shortcuts.iter().find(|shortcut| shortcut.command == command))
            .ok_or_else(|| {
                format!(
                    "Shortcut command \"{command}\" not declared in manifest contributes.terminalHooks.shortcuts"
                )
            })?;
        let normalized_keybinding = normalize_plugin_key_combo(&declared.key).ok_or_else(|| {
            "Runtime terminal shortcut registration has no usable key parts".to_string()
        })?;

        // Tauri registerShortcut stores a key-to-handler map after validating
        // the command against the manifest. Native dispatches the same handler
        // by reusing the runtime command RPC path keyed by the declared command.
        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        self.runtime_keybindings
            .push(NativePluginRuntimeKeybindingContribution {
                plugin_id: registration.plugin_id,
                plugin_name,
                registration_id: registration.registration_id,
                keybinding: declared.key.clone(),
                normalized_keybinding,
                command: command.clone(),
                label: command,
            });
        Ok(())
    }

    pub(crate) fn apply_runtime_terminal_hook(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
        manifest: &NativePluginManifest,
    ) -> Result<(), String> {
        validate_manifest_text_field(
            "runtime.terminalHook.registrationId",
            &registration.registration_id,
        )?;
        let hooks = manifest
            .contributes
            .as_ref()
            .and_then(|contributes| contributes.terminal_hooks.as_ref());
        let declared = match registration.kind {
            PluginRegistrationKind::TerminalInputInterceptor => hooks
                .and_then(|hooks| hooks.input_interceptor)
                .unwrap_or(false),
            PluginRegistrationKind::TerminalOutputProcessor => hooks
                .and_then(|hooks| hooks.output_processor)
                .unwrap_or(false),
            _ => false,
        };
        if !declared {
            let declaration = match registration.kind {
                PluginRegistrationKind::TerminalInputInterceptor => {
                    "inputInterceptor not declared in manifest contributes.terminalHooks"
                }
                PluginRegistrationKind::TerminalOutputProcessor => {
                    "outputProcessor not declared in manifest contributes.terminalHooks"
                }
                _ => "terminal hook not declared in manifest contributes.terminalHooks",
            };
            return Err(declaration.to_string());
        }
        let command = runtime_metadata_string(&registration.metadata, "command")
            .or_else(|| runtime_metadata_string(&registration.metadata, "id"))
            .unwrap_or_else(|| registration.registration_id.clone());
        validate_manifest_text_field("runtime.terminalHook.command", &command)?;
        let contribution = NativePluginRuntimeTerminalHookContribution {
            plugin_id: registration.plugin_id.clone(),
            plugin_name,
            registration_id: registration.registration_id.clone(),
            command,
        };

        // Tauri stores terminal hooks in registration order and removes them
        // through disposables. Native records the same ordered rows here; the
        // terminal I/O pipeline consumes these rows when hooks are executed.
        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        match registration.kind {
            PluginRegistrationKind::TerminalInputInterceptor => {
                self.runtime_terminal_input_interceptors.push(contribution);
            }
            PluginRegistrationKind::TerminalOutputProcessor => {
                self.runtime_terminal_output_processors.push(contribution);
            }
            _ => {}
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn apply_runtime_registration(
        &mut self,
        registration: PluginRegistration,
        plugin_name: String,
    ) -> Result<(), String> {
        validate_manifest_text_field("runtime.registrationId", &registration.registration_id)?;
        // Tauri disposables replace state by key. Native mirrors that by
        // removing an existing registration id before applying the latest
        // runtime payload from process/WASM.
        self.dispose_runtime_registration(&registration.plugin_id, &registration.registration_id);
        match registration.kind {
            PluginRegistrationKind::Command => {
                let command = runtime_metadata_string(&registration.metadata, "id")
                    .or_else(|| runtime_metadata_string(&registration.metadata, "command"))
                    .ok_or_else(|| {
                        "Runtime command registration requires metadata.id".to_string()
                    })?;
                let label = runtime_metadata_string(&registration.metadata, "label")
                    .unwrap_or_else(|| command.clone());
                validate_manifest_text_field("runtime.command.id", &command)?;
                validate_manifest_text_field("runtime.command.label", &label)?;
                self.runtime_commands
                    .push(NativePluginRuntimeCommandContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        command,
                        label,
                        icon: runtime_metadata_string(&registration.metadata, "icon"),
                        shortcut: runtime_metadata_string(&registration.metadata, "shortcut"),
                        section: runtime_metadata_string(&registration.metadata, "section"),
                    });
            }
            PluginRegistrationKind::Keybinding => {
                let keybinding = runtime_metadata_string(&registration.metadata, "keybinding")
                    .or_else(|| runtime_metadata_string(&registration.metadata, "key"))
                    .ok_or_else(|| {
                        "Runtime keybinding registration requires metadata.keybinding".to_string()
                    })?;
                let command = runtime_metadata_string(&registration.metadata, "command")
                    .unwrap_or_else(|| registration.registration_id.clone());
                let label = runtime_metadata_string(&registration.metadata, "label")
                    .unwrap_or_else(|| command.clone());
                validate_manifest_text_field("runtime.keybinding.keybinding", &keybinding)?;
                validate_manifest_text_field("runtime.keybinding.command", &command)?;
                validate_manifest_text_field("runtime.keybinding.label", &label)?;
                let normalized_keybinding =
                    normalize_plugin_key_combo(&keybinding).ok_or_else(|| {
                        "Runtime keybinding registration has no usable key parts".to_string()
                    })?;
                self.runtime_keybindings
                    .push(NativePluginRuntimeKeybindingContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        keybinding,
                        normalized_keybinding,
                        command,
                        label,
                    });
            }
            PluginRegistrationKind::ContextMenu => {
                let target =
                    runtime_metadata_string(&registration.metadata, "target").ok_or_else(|| {
                        "Runtime context menu registration requires metadata.target".to_string()
                    })?;
                validate_one_of(
                    "runtime.contextMenu.target",
                    &target,
                    &["terminal", "sftp", "tab", "sidebar"],
                )?;
                let items = runtime_context_menu_items(&registration.metadata)?;
                self.runtime_context_menus
                    .push(NativePluginRuntimeContextMenuContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        target,
                        items,
                    });
            }
            PluginRegistrationKind::StatusBar => {
                let text =
                    runtime_metadata_string(&registration.metadata, "text").ok_or_else(|| {
                        "Runtime status item registration requires metadata.text".to_string()
                    })?;
                validate_manifest_text_field("runtime.statusBar.text", &text)?;
                let alignment = runtime_metadata_string(&registration.metadata, "alignment")
                    .unwrap_or_else(|| "left".to_string());
                validate_one_of(
                    "runtime.statusBar.alignment",
                    &alignment,
                    &["left", "right"],
                )?;
                self.runtime_status_items
                    .push(NativePluginRuntimeStatusItemContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        text,
                        icon: runtime_metadata_string(&registration.metadata, "icon"),
                        tooltip: runtime_metadata_string(&registration.metadata, "tooltip"),
                        alignment,
                        priority: registration
                            .metadata
                            .get("priority")
                            .and_then(serde_json::Value::as_i64),
                    });
            }
            PluginRegistrationKind::Tab | PluginRegistrationKind::SidebarPanel => {
                return Err(format!(
                    "Runtime registration kind {:?} must be validated against manifest declarations",
                    registration.kind
                ));
            }
            PluginRegistrationKind::EventSubscription => {
                let event =
                    runtime_subscription_event(&registration.metadata, &registration.plugin_id)?;
                let filter = registration
                    .metadata
                    .get("filter")
                    .cloned()
                    .or_else(|| runtime_metadata_node_filter(&registration.metadata));
                self.runtime_event_subscriptions.push(
                    NativePluginRuntimeEventSubscriptionContribution {
                        plugin_id: registration.plugin_id,
                        plugin_name,
                        registration_id: registration.registration_id,
                        event,
                        filter,
                    },
                );
            }
            _ => {
                return Err(format!(
                    "Runtime registration kind {:?} is not a Phase 3 UI contribution",
                    registration.kind
                ));
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn dispose_runtime_registration(
        &mut self,
        plugin_id: &str,
        registration_id: &str,
    ) -> bool {
        let before = self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len();
        self.runtime_commands.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_keybindings.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_context_menus.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_status_items.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_tab_views.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_sidebar_panels.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_event_subscriptions.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_terminal_input_interceptors.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        self.runtime_terminal_output_processors.retain(|entry| {
            !(entry.plugin_id == plugin_id && entry.registration_id == registration_id)
        });
        let after = self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len();
        before != after
    }

    #[allow(dead_code)]
    pub(crate) fn cleanup_runtime_plugin_contributions(&mut self, plugin_id: &str) -> usize {
        let before = self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len();
        self.runtime_commands
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_keybindings
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_context_menus
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_status_items
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_tab_views
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_sidebar_panels
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_event_subscriptions
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_terminal_input_interceptors
            .retain(|entry| entry.plugin_id != plugin_id);
        self.runtime_terminal_output_processors
            .retain(|entry| entry.plugin_id != plugin_id);
        let after = self.runtime_commands.len()
            + self.runtime_keybindings.len()
            + self.runtime_context_menus.len()
            + self.runtime_status_items.len()
            + self.runtime_tab_views.len()
            + self.runtime_sidebar_panels.len()
            + self.runtime_event_subscriptions.len()
            + self.runtime_terminal_input_interceptors.len()
            + self.runtime_terminal_output_processors.len();
        before.saturating_sub(after)
    }
}

fn normalize_plugin_key_combo(keybinding: &str) -> Option<String> {
    let mut parts = keybinding
        .split('+')
        .filter_map(normalize_plugin_key_part)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    parts.sort();
    Some(parts.join("+"))
}

fn normalize_plugin_key_part(part: &str) -> Option<String> {
    let normalized = part.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }
    Some(match normalized.as_str() {
        "cmd" | "command" | "meta" | "super" | "win" | "⌘" => "ctrl".to_string(),
        "control" | "ctrl" | "⌃" => "ctrl".to_string(),
        "option" | "alt" | "⌥" => "alt".to_string(),
        "shift" | "⇧" => "shift".to_string(),
        "escape" | "esc" => "esc".to_string(),
        "spacebar" | "space" | " " => "space".to_string(),
        "left" => "arrowleft".to_string(),
        "right" => "arrowright".to_string(),
        "up" => "arrowup".to_string(),
        "down" => "arrowdown".to_string(),
        key => key.to_string(),
    })
}

fn native_plugin_contributions_enabled(plugin: &NativePluginInfo) -> bool {
    matches!(
        plugin.state,
        NativePluginState::ReadyManifestOnly
            | NativePluginState::ReadyWasm
            | NativePluginState::ReadyProcess
            | NativePluginState::Active
    )
}

pub fn native_plugin_ai_tool_name(plugin_id: &str, tool_name: &str) -> String {
    format!(
        "plugin::{}::{}",
        sanitize_plugin_tool_part(plugin_id),
        sanitize_plugin_tool_part(tool_name)
    )
}

pub fn is_native_plugin_ai_tool_name(tool_name: &str) -> bool {
    tool_name.starts_with("plugin::")
}

fn sanitize_plugin_tool_part(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "unnamed".to_string()
    } else {
        sanitized
    }
}
