// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Registry state, lifecycle transitions, and host-facing mutations.

use super::*;

/// Official catalog endpoint shipped with OxideTerm builds.
pub const OFFICIAL_NATIVE_PLUGIN_REGISTRY_URL: &str = "https://raw.githubusercontent.com/AnalyseDeCircuit/oxideterm-plugins/main/registry/v1/index.json";
const NATIVE_PLUGIN_REGISTRY_VERSION: u32 = 1;
// The catalog is metadata, not a package transport. Keep malformed endpoints
// from allocating package-sized responses on the application runtime.
const NATIVE_PLUGIN_REGISTRY_MAX_BYTES: u64 = 2 * 1024 * 1024;
const PORTABLE_PLUGIN_PACKAGE_TARGET: &str = "any";

#[derive(Clone, Debug, Default)]
pub struct NativePluginRegistry {
    plugins: Vec<NativePluginInfo>,
    diagnostics: Vec<NativePluginDiagnostic>,
    contributions: NativePluginContributionStore,
    config: NativePluginGlobalConfig,
    config_path: PathBuf,
}

impl NativePluginRegistry {
    pub fn discover(settings_path: &Path) -> Self {
        let plugins_dir = native_plugins_dir(settings_path);
        let config_path = native_plugin_config_path(settings_path);
        let config = load_native_plugin_config(&config_path);
        // Phase 1 owns the native plugin config file. Persist a missing file so
        // later enable/disable/error transitions have a stable location without
        // falling back to ad hoc state.
        if !config_path.exists() {
            let _ = save_native_plugin_config(&config_path, &config);
        }
        // Discovery owns the plugin root contract. Creating it here keeps
        // portable profiles, custom data directories, and older packages
        // consistent before users install or manually copy their first plugin.
        let (plugins, diagnostics) = match fs::create_dir_all(&plugins_dir) {
            Ok(()) => discover_native_plugins_in_dir(&plugins_dir, &config),
            Err(error) => (
                Vec::new(),
                vec![NativePluginDiagnostic {
                    plugin_dir: plugins_dir,
                    plugin_id: None,
                    message: format!("Cannot create plugin directory: {error}"),
                }],
            ),
        };
        let contributions = NativePluginContributionStore::from_plugins(&plugins);
        Self {
            plugins,
            diagnostics,
            contributions,
            config,
            config_path,
        }
    }

    pub fn plugins(&self) -> &[NativePluginInfo] {
        &self.plugins
    }

    pub fn diagnostics(&self) -> &[NativePluginDiagnostic] {
        &self.diagnostics
    }

    pub fn contributions(&self) -> &NativePluginContributionStore {
        &self.contributions
    }

    #[allow(dead_code)]
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    #[allow(dead_code)]
    pub fn configured_plugin_count(&self) -> usize {
        self.config.plugins.len()
    }

    pub fn process_activation_plans(&self) -> Vec<NativePluginProcessActivationPlan> {
        self.plugins
            .iter()
            .filter_map(|plugin| {
                if !matches!(plugin.state, NativePluginState::ReadyProcess) {
                    return None;
                }
                let NativePluginRuntimePlan::Process { entry } = &plugin.runtime_plan else {
                    return None;
                };
                Some(NativePluginProcessActivationPlan {
                    plugin_id: plugin.manifest.id.clone(),
                    manifest: plugin.manifest.clone(),
                    install_dir: plugin.install_dir.clone(),
                    entry: entry.clone(),
                })
            })
            .collect()
    }

    pub fn wasm_activation_plans(&self) -> Vec<NativePluginWasmActivationPlan> {
        self.plugins
            .iter()
            .filter_map(|plugin| {
                if !matches!(plugin.state, NativePluginState::ReadyWasm) {
                    return None;
                }
                let NativePluginRuntimePlan::Wasm { entry } = &plugin.runtime_plan else {
                    return None;
                };
                Some(NativePluginWasmActivationPlan {
                    plugin_id: plugin.manifest.id.clone(),
                    manifest: plugin.manifest.clone(),
                    install_dir: plugin.install_dir.clone(),
                    entry: entry.clone(),
                })
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn install_plugin_package(
        settings_path: &Path,
        expected_id: &str,
        checksum: Option<&str>,
        package_bytes: &[u8],
    ) -> Result<NativePluginManifest, String> {
        Self::install_managed_plugin_package(
            settings_path,
            expected_id,
            checksum,
            package_bytes,
            true,
        )
        .map(|result| result.manifest)
    }

    /// Installs a verified package only after its manifest identity is known.
    pub fn install_managed_plugin_package(
        settings_path: &Path,
        expected_id: &str,
        checksum: Option<&str>,
        package_bytes: &[u8],
        overwrite: bool,
    ) -> Result<NativePluginUrlInstallResult, String> {
        validate_native_plugin_id(expected_id)?;
        install_native_plugin_package_bytes(
            settings_path,
            package_bytes,
            checksum,
            overwrite,
            Some(expected_id),
        )
    }

    #[allow(dead_code)]
    pub fn install_plugin_package_from_bytes(
        settings_path: &Path,
        package_bytes: &[u8],
        checksum: Option<&str>,
        overwrite: bool,
    ) -> Result<NativePluginUrlInstallResult, String> {
        install_native_plugin_package_bytes(settings_path, package_bytes, checksum, overwrite, None)
    }

    #[allow(dead_code)]
    pub async fn fetch_plugin_registry(url: &str) -> Result<NativePluginRegistryIndex, String> {
        validate_native_plugin_package_url(url)?;
        let client = oxideterm_network_proxy::application_http_client()
            .map_err(|error| format!("Failed to create HTTP client: {error}"))?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|error| format!("Failed to fetch registry: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Registry returned HTTP {}",
                response.status().as_u16()
            ));
        }
        if let Some(content_length) = response.content_length()
            && content_length > NATIVE_PLUGIN_REGISTRY_MAX_BYTES
        {
            return Err(format!(
                "Plugin registry too large: {content_length} bytes (max {NATIVE_PLUGIN_REGISTRY_MAX_BYTES} bytes)"
            ));
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| format!("Failed to read registry response: {error}"))?;
        if body.len() as u64 > NATIVE_PLUGIN_REGISTRY_MAX_BYTES {
            return Err(format!(
                "Plugin registry too large: {} bytes (max {NATIVE_PLUGIN_REGISTRY_MAX_BYTES} bytes)",
                body.len()
            ));
        }
        let registry = serde_json::from_slice(&body)
            .map_err(|error| format!("Failed to parse registry index: {error}"))?;
        validate_native_plugin_registry(&registry)?;
        Ok(registry)
    }

    pub async fn fetch_official_plugin_registry() -> Result<NativePluginRegistryIndex, String> {
        Self::fetch_plugin_registry(OFFICIAL_NATIVE_PLUGIN_REGISTRY_URL).await
    }

    /// Resolves one immutable package without exposing platform selection to the UI.
    pub fn resolve_registry_package(
        entry: &NativePluginRegistryEntry,
    ) -> Result<NativePluginRegistryPackage, String> {
        let host_target = native_plugin_host_target().ok_or_else(|| {
            format!(
                "Plugin packages are not available for {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            )
        })?;
        if let Some(package) = entry
            .packages
            .iter()
            .find(|package| package.target == host_target)
            .or_else(|| {
                entry
                    .packages
                    .iter()
                    .find(|package| package.target == PORTABLE_PLUGIN_PACKAGE_TARGET)
            })
        {
            return Ok(package.clone());
        }

        // Version 1 clients still accept the original single-package shape for
        // custom registries created before platform packages were introduced.
        if !entry.download_url.trim().is_empty() {
            return Ok(NativePluginRegistryPackage {
                target: PORTABLE_PLUGIN_PACKAGE_TARGET.to_string(),
                download_url: entry.download_url.clone(),
                checksum: entry.checksum.clone().unwrap_or_default(),
                size: entry.size,
            });
        }
        Err(format!(
            "Plugin \"{}\" has no package for {host_target}",
            entry.id
        ))
    }

    pub fn registry_entry_supports_current_host(entry: &NativePluginRegistryEntry) -> bool {
        Self::resolve_registry_package(entry).is_ok()
    }

    pub fn registry_entry_supports_current_version(entry: &NativePluginRegistryEntry) -> bool {
        let Some(required) = entry.min_oxideterm_version.as_deref() else {
            return true;
        };
        let Ok(required) = semver::Version::parse(required) else {
            return false;
        };
        semver::Version::parse(env!("CARGO_PKG_VERSION")).is_ok_and(|current| current >= required)
    }

    pub fn registry_entry_is_update(
        entry: &NativePluginRegistryEntry,
        installed_version: &str,
    ) -> bool {
        native_plugin_version_is_newer(&entry.version, installed_version)
    }

    #[allow(dead_code)]
    pub async fn install_plugin_package_from_url(
        settings_path: &Path,
        download_url: &str,
        checksum: Option<&str>,
        overwrite: bool,
    ) -> Result<NativePluginUrlInstallResult, String> {
        let bytes = download_native_plugin_package(download_url).await?;
        install_native_plugin_package_bytes(settings_path, &bytes, checksum, overwrite, None)
    }

    /// Installs a marketplace package only after its catalog identity and
    /// required digest have been checked against the package manifest.
    pub async fn install_managed_plugin_package_from_url(
        settings_path: &Path,
        expected_id: &str,
        download_url: &str,
        checksum: &str,
        overwrite: bool,
    ) -> Result<NativePluginUrlInstallResult, String> {
        if checksum.trim().is_empty() {
            return Err("Marketplace plugin package is missing SHA-256".to_string());
        }
        let bytes = download_native_plugin_package(download_url).await?;
        install_native_plugin_package_bytes(
            settings_path,
            &bytes,
            Some(checksum),
            overwrite,
            Some(expected_id),
        )
    }

    #[allow(dead_code)]
    pub fn check_plugin_updates(
        registry: NativePluginRegistryIndex,
        installed: &[NativePluginInstalledInfo],
    ) -> Vec<NativePluginRegistryEntry> {
        let installed_versions = installed
            .iter()
            .map(|plugin| (plugin.id.as_str(), plugin.version.as_str()))
            .collect::<HashMap<_, _>>();
        registry
            .plugins
            .into_iter()
            .filter(|entry| {
                Self::registry_entry_supports_current_host(entry)
                    && Self::registry_entry_supports_current_version(entry)
                    && installed_versions
                        .get(entry.id.as_str())
                        .is_some_and(|version| Self::registry_entry_is_update(entry, version))
            })
            .collect()
    }

    pub fn uninstall_plugin(
        &mut self,
        plugin_id: &str,
        remove_settings: bool,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        let plugin_dir = native_plugins_dir_from_config_path(&self.config_path).join(plugin_id);
        if !plugin_dir.exists() {
            return Err(format!("Plugin \"{plugin_id}\" is not installed"));
        }
        if !plugin_dir.join(PLUGIN_MANIFEST_FILENAME).exists() {
            return Err(format!(
                "Directory \"{plugin_id}\" does not appear to be a valid plugin"
            ));
        }

        fs::remove_dir_all(&plugin_dir)
            .map_err(|error| format!("Failed to remove plugin directory: {error}"))?;
        self.cleanup_runtime_plugin_contributions(plugin_id);
        self.config.plugins.remove(plugin_id);
        if remove_settings {
            self.config.settings.remove(plugin_id);
            self.config.storage.remove(plugin_id);
        }
        save_native_plugin_config(&self.config_path, &self.config)?;
        let settings_path = settings_path_from_native_plugin_config_path(&self.config_path);
        *self = NativePluginRegistry::discover(&settings_path);
        Ok(())
    }

    pub fn mark_runtime_loading(&mut self, plugin_id: &str) -> Result<(), String> {
        self.set_runtime_state(plugin_id, NativePluginState::Loading, None)
    }

    pub fn mark_runtime_active(&mut self, plugin_id: &str) -> Result<(), String> {
        self.set_runtime_state(plugin_id, NativePluginState::Active, None)
    }

    pub fn mark_runtime_error(&mut self, plugin_id: &str, message: String) -> Result<(), String> {
        self.set_runtime_state(plugin_id, NativePluginState::Error, Some(message.clone()))?;
        self.record_manager_error(plugin_id.to_string(), message);
        Ok(())
    }

    fn set_runtime_state(
        &mut self,
        plugin_id: &str,
        state: NativePluginState,
        last_error: Option<String>,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        let plugin = self
            .plugins
            .iter_mut()
            .find(|plugin| plugin.manifest.id == plugin_id)
            .ok_or_else(|| format!("Plugin \"{plugin_id}\" is not discovered"))?;
        // Tauri stores transient plugin lifecycle separately from persisted
        // plugin-config. Native keeps active/loading in memory while persisting
        // runtime errors so Plugin Manager still explains failed activation
        // after restart.
        plugin.state = state;
        if let Some(error) = last_error {
            plugin.config.last_error = Some(error.clone());
            let entry = self
                .config
                .plugins
                .entry(plugin_id.to_string())
                .or_default();
            entry.last_error = Some(error);
            entry.runtime_kind = Some(native_runtime_kind_label(&plugin.runtime_plan).to_string());
            save_native_plugin_config(&self.config_path, &self.config)?;
        } else if matches!(
            state,
            NativePluginState::Active | NativePluginState::Loading
        ) {
            plugin.config.last_error = None;
        }
        Ok(())
    }

    // Phase 3 process/WASM bridges feed dynamic registrations through these
    // entry points once WorkspaceApp owns live runtime supervisors.
    #[allow(dead_code)]
    pub fn apply_runtime_registration(
        &mut self,
        registration: PluginRegistration,
    ) -> Result<(), String> {
        validate_native_plugin_id(&registration.plugin_id)?;
        let plugin = self
            .plugins
            .iter()
            .find(|plugin| plugin.manifest.id == registration.plugin_id)
            .ok_or_else(|| format!("Plugin \"{}\" is not discovered", registration.plugin_id))?;
        let plugin_name = plugin.manifest.name.clone();
        if registration.kind == PluginRegistrationKind::TerminalShortcut {
            return self.contributions.apply_runtime_terminal_shortcut(
                registration,
                plugin_name,
                &plugin.manifest,
            );
        }
        if registration.kind == PluginRegistrationKind::Tab {
            return self.contributions.apply_runtime_tab_view(
                registration,
                plugin_name,
                &plugin.manifest,
            );
        }
        if registration.kind == PluginRegistrationKind::SidebarPanel {
            return self.contributions.apply_runtime_sidebar_panel(
                registration,
                plugin_name,
                &plugin.manifest,
            );
        }
        if registration.kind == PluginRegistrationKind::ActivityBarItem {
            return self.contributions.apply_runtime_activity_bar_item(
                registration,
                plugin_name,
                &plugin.manifest,
            );
        }
        if matches!(
            registration.kind,
            PluginRegistrationKind::TerminalInputInterceptor
                | PluginRegistrationKind::TerminalOutputProcessor
        ) {
            return self.contributions.apply_runtime_terminal_hook(
                registration,
                plugin_name,
                &plugin.manifest,
            );
        }
        self.contributions
            .apply_runtime_registration(registration, plugin_name)
    }

    #[allow(dead_code)]
    pub fn dispose_runtime_registration(&mut self, plugin_id: &str, registration_id: &str) -> bool {
        self.contributions
            .dispose_runtime_registration(plugin_id, registration_id)
    }

    #[allow(dead_code)]
    pub fn cleanup_runtime_plugin_contributions(&mut self, plugin_id: &str) -> usize {
        self.contributions
            .cleanup_runtime_plugin_contributions(plugin_id)
    }

    // Process/WASM runtimes emit protocol messages, while this registry owns
    // the host-visible contribution rows. Keep the bridge explicit so runtime
    // transports cannot mutate UI state outside the same validation path used
    // by manifest-only contributions.
    #[allow(dead_code)]
    pub fn apply_runtime_outbound_message(
        &mut self,
        plugin_id: &str,
        message: &PluginOutboundMessage,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        match message {
            PluginOutboundMessage::RegisterContribution { registration } => {
                if registration.plugin_id != plugin_id {
                    return Err(format!(
                        "Runtime registration plugin id \"{}\" does not match owner \"{}\"",
                        registration.plugin_id, plugin_id
                    ));
                }
                self.apply_runtime_registration(registration.clone())
            }
            PluginOutboundMessage::DisposeContribution { registration_id } => {
                self.dispose_runtime_registration(plugin_id, registration_id);
                Ok(())
            }
            PluginOutboundMessage::RuntimeError { error } => {
                self.record_manager_error(plugin_id.to_string(), error.message.clone());
                Ok(())
            }
            PluginOutboundMessage::Log { level, message } => {
                if matches!(level, PluginRuntimeLogLevel::Error) {
                    self.record_manager_error(plugin_id.to_string(), message.clone());
                }
                Ok(())
            }
            PluginOutboundMessage::RuntimeReady
            | PluginOutboundMessage::ReportProgress { .. }
            | PluginOutboundMessage::EmitEvent { .. }
            | PluginOutboundMessage::CallHostApi { .. } => Ok(()),
        }
    }

    pub fn record_manager_error(&mut self, plugin_id: String, message: String) {
        // Manager-side persistence failures should be visible in the same
        // diagnostics stream as manifest validation failures instead of being
        // lost in stdout/stderr.
        self.diagnostics.push(NativePluginDiagnostic {
            plugin_dir: self.config_path.clone(),
            plugin_id: Some(plugin_id),
            message,
        });
    }

    pub fn set_plugin_enabled(&mut self, plugin_id: &str, enabled: bool) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        let plugin_snapshot = self
            .plugins
            .iter()
            .find(|plugin| plugin.manifest.id == plugin_id)
            .cloned()
            .ok_or_else(|| format!("Plugin \"{plugin_id}\" is not discovered"))?;

        if matches!(
            plugin_snapshot.runtime_plan,
            NativePluginRuntimePlan::UnsupportedLegacyJs { .. }
        ) && enabled
        {
            return Err(
                "Legacy Tauri JavaScript plugins cannot be enabled in native mode".to_string(),
            );
        }

        let entry = self
            .config
            .plugins
            .entry(plugin_id.to_string())
            .or_default();
        entry.enabled = enabled;
        entry.install_path = Some(plugin_snapshot.install_dir.display().to_string());
        let runtime_kind = native_runtime_kind_label(&plugin_snapshot.runtime_plan).to_string();
        entry.runtime_kind = Some(runtime_kind.clone());
        entry.last_loaded_version = Some(plugin_snapshot.manifest.version.clone());

        if enabled {
            // Enabling is the single consent point for the complete current
            // request, including implicit trust in an unsandboxed process.
            entry.approved_capabilities = native_plugin_requested_capabilities(
                &plugin_snapshot.manifest,
                &plugin_snapshot.runtime_plan,
            )?;
            entry.approved_for_version = Some(plugin_snapshot.manifest.version.clone());
            entry.approved_runtime_kind = Some(runtime_kind);
            // Tauri reload clears the disabled/error path before trying to load
            // again. Native Phase 1 has no runtime yet, but the config state must
            // still be ready for Phase 3 activation.
            entry.auto_disabled = false;
            entry.last_error = None;
            entry.error_count = 0;
            entry.error_window_started_at_ms = None;
        }

        save_native_plugin_config(&self.config_path, &self.config)?;
        self.refresh_plugin_state(plugin_id);
        self.contributions = NativePluginContributionStore::from_plugins(&self.plugins);
        Ok(())
    }

    pub fn plugin_setting_value(&self, plugin_id: &str, setting_id: &str) -> Option<Value> {
        validate_native_plugin_id(plugin_id).ok()?;
        let setting = self.find_plugin_setting(plugin_id, setting_id)?;
        Some(
            self.config
                .settings
                .get(plugin_id)
                .and_then(|values| values.get(setting_id))
                .cloned()
                .unwrap_or_else(|| setting.definition.default.clone()),
        )
    }

    // Phase 2 settings controls will call this once the manifest-only settings
    // panel is wired; keeping the typed writer here prevents page-local state
    // from inventing a different persistence path.
    #[allow(dead_code)]
    pub fn set_plugin_setting_value(
        &mut self,
        plugin_id: &str,
        setting_id: &str,
        value: Value,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        let setting = self
            .find_plugin_setting(plugin_id, setting_id)
            .ok_or_else(|| {
                format!("Plugin setting \"{plugin_id}.{setting_id}\" is not declared")
            })?;
        validate_plugin_setting_value(&setting.definition, &value)?;
        self.config
            .settings
            .entry(plugin_id.to_string())
            .or_default()
            .insert(setting_id.to_string(), value);
        save_native_plugin_config(&self.config_path, &self.config)
    }

    #[allow(dead_code)]
    pub fn plugin_storage_value(&self, plugin_id: &str, key: &str) -> Option<Value> {
        validate_native_plugin_id(plugin_id).ok()?;
        validate_plugin_storage_key(key).ok()?;
        self.config
            .storage
            .get(plugin_id)
            .and_then(|values| values.get(key))
            .cloned()
    }

    pub fn set_plugin_storage_value(
        &mut self,
        plugin_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        validate_plugin_storage_key(key)?;
        // Tauri scoped localStorage serializes JSON by plugin id. Native stores
        // the same JSON values under a plugin-owned map and validates the whole
        // plugin bucket before writing so one plugin cannot bloat the shared
        // config file.
        let mut plugin_values = self
            .config
            .storage
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        plugin_values.insert(key.to_string(), value);
        validate_plugin_storage_size(&plugin_values)?;
        self.config
            .storage
            .insert(plugin_id.to_string(), plugin_values);
        save_native_plugin_config(&self.config_path, &self.config)
    }

    pub fn remove_plugin_storage_value(
        &mut self,
        plugin_id: &str,
        key: &str,
    ) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        validate_plugin_storage_key(key)?;
        if let Some(values) = self.config.storage.get_mut(plugin_id) {
            values.remove(key);
            if values.is_empty() {
                self.config.storage.remove(plugin_id);
            }
        }
        save_native_plugin_config(&self.config_path, &self.config)
    }

    #[allow(dead_code)]
    pub fn clear_plugin_storage(&mut self, plugin_id: &str) -> Result<(), String> {
        validate_native_plugin_id(plugin_id)?;
        self.config.storage.remove(plugin_id);
        save_native_plugin_config(&self.config_path, &self.config)
    }

    fn find_plugin_setting(
        &self,
        plugin_id: &str,
        setting_id: &str,
    ) -> Option<&NativePluginSettingContribution> {
        self.contributions
            .settings
            .iter()
            .find(|setting| setting.plugin_id == plugin_id && setting.definition.id == setting_id)
    }

    fn refresh_plugin_state(&mut self, plugin_id: &str) {
        for plugin in &mut self.plugins {
            if plugin.manifest.id == plugin_id {
                let config_entry = self
                    .config
                    .plugins
                    .get(plugin_id)
                    .cloned()
                    .unwrap_or_else(NativePluginConfigEntry::default);
                plugin.state = native_plugin_state_for_manifest(
                    &plugin.manifest,
                    &plugin.runtime_plan,
                    &config_entry,
                );
                plugin.config = config_entry;
                break;
            }
        }
    }
}

async fn download_native_plugin_package(download_url: &str) -> Result<Vec<u8>, String> {
    validate_native_plugin_package_url(download_url)?;
    let client = oxideterm_network_proxy::application_http_client_builder()
        .map_err(|error| format!("Failed to apply application proxy: {error}"))?
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|error| format!("Failed to create HTTP client: {error}"))?;
    let response = client
        .get(download_url)
        .send()
        .await
        .map_err(|error| format!("Failed to download plugin: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Download returned HTTP {}",
            response.status().as_u16()
        ));
    }
    if let Some(content_length) = response.content_length()
        && content_length > PLUGIN_PACKAGE_MAX_BYTES
    {
        return Err(format!(
            "Plugin package too large: {} bytes (max {} bytes)",
            content_length, PLUGIN_PACKAGE_MAX_BYTES
        ));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("Failed to read download body: {error}"))
}

fn validate_native_plugin_registry(registry: &NativePluginRegistryIndex) -> Result<(), String> {
    if registry.version != NATIVE_PLUGIN_REGISTRY_VERSION {
        return Err(format!(
            "Unsupported plugin registry version {}",
            registry.version
        ));
    }
    let mut plugin_ids = std::collections::HashSet::new();
    for entry in &registry.plugins {
        validate_native_plugin_id(&entry.id)
            .map_err(|error| format!("Invalid registry plugin id: {error}"))?;
        if !plugin_ids.insert(entry.id.as_str()) {
            return Err(format!("Duplicate plugin registry id \"{}\"", entry.id));
        }
        if entry.name.trim().is_empty() {
            return Err(format!(
                "Plugin registry entry \"{}\" has no name",
                entry.id
            ));
        }
        semver::Version::parse(&entry.version).map_err(|error| {
            format!(
                "Plugin registry entry \"{}\" has invalid version: {error}",
                entry.id
            )
        })?;
        if let Some(required) = entry.min_oxideterm_version.as_deref() {
            semver::Version::parse(required).map_err(|error| {
                format!(
                    "Plugin registry entry \"{}\" has invalid minimum OxideTerm version: {error}",
                    entry.id
                )
            })?;
        }
        let mut package_targets = std::collections::HashSet::new();
        for package in &entry.packages {
            if !package_targets.insert(package.target.as_str()) {
                return Err(format!(
                    "Plugin registry entry \"{}\" has duplicate target \"{}\"",
                    entry.id, package.target
                ));
            }
            let package_url = reqwest::Url::parse(&package.download_url).map_err(|error| {
                format!(
                    "Plugin registry entry \"{}\" has invalid package URL: {error}",
                    entry.id
                )
            })?;
            if package_url.scheme() != "https" {
                return Err(format!(
                    "Plugin registry entry \"{}\" package URL must use HTTPS",
                    entry.id
                ));
            }
            let checksum = package
                .checksum
                .strip_prefix("sha256:")
                .unwrap_or(&package.checksum);
            if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!(
                    "Plugin registry entry \"{}\" has invalid SHA-256",
                    entry.id
                ));
            }
            if package
                .size
                .is_some_and(|size| size > PLUGIN_PACKAGE_MAX_BYTES)
            {
                return Err(format!(
                    "Plugin registry entry \"{}\" package exceeds the size limit",
                    entry.id
                ));
            }
        }
    }
    Ok(())
}

fn native_plugin_host_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("windows", "aarch64") => Some("aarch64-pc-windows-msvc"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}
