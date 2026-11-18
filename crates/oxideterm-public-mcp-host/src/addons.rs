use super::{PublicMcpBackground, PublicMcpHost, finish_serialized};
use oxideterm_plugin_registry as plugin_host;
use oxideterm_public_mcp::{AddonRef, ClientRef, DomainRequest, PublicToolCall, ToolEnvelope};
use serde::Serialize;
use serde_json::json;
use std::collections::HashSet;
use zeroize::Zeroizing;

pub struct PublicMcpAddonPackage {
    pub checksum: String,
    pub bytes: Zeroizing<Vec<u8>>,
}

#[derive(Serialize)]
pub struct PublicAddonProjection {
    addon_ref: AddonRef,
    id: String,
    name: String,
    version: String,
    description: Option<String>,
    author: Option<String>,
    enabled: bool,
    state: &'static str,
    runtime_kind: &'static str,
    requested_capabilities: Vec<String>,
    capabilities_fingerprint: Option<String>,
    permission_review_required: bool,
    contributions: PublicAddonContributionSummary,
}

#[derive(Serialize)]
struct PublicAddonContributionSummary {
    tabs: usize,
    sidebar_panels: usize,
    activity_bar_items: usize,
    settings: usize,
    terminal_hooks: usize,
    terminal_transports: usize,
    connection_hooks: usize,
    ai_tools: usize,
    api_commands: usize,
    host_monitors: usize,
}

impl PublicMcpHost {
    pub fn handle_public_mcp_addons_list(
        &mut self,
        request: DomainRequest,
        registry: &plugin_host::NativePluginRegistry,
    ) {
        let PublicToolCall::AddonsList(args) = &request.call else {
            return;
        };
        self.sync_addon_refs(&request.client_ref, registry);
        let include_disabled = args.include_disabled.unwrap_or(true);
        let addons = registry
            .plugins()
            .iter()
            .filter(|plugin| include_disabled || plugin.config.enabled)
            .map(|plugin| {
                let addon_ref = self.addon_ref(&request.client_ref, &plugin.manifest.id);
                public_addon_projection(addon_ref, plugin)
            })
            .collect::<Vec<_>>();
        finish_serialized(request, json!({ "addons": addons }));
    }
}

impl PublicMcpBackground {
    pub(crate) fn prepare_addon_package(
        &self,
        request: &DomainRequest,
        args: &oxideterm_public_mcp::calls::AddonsInstallArgs,
    ) -> Result<PublicMcpAddonPackage, String> {
        let checksum = normalized_checksum(&args.checksum);
        let artifact = self
            .state
            .artifacts
            .read_all(
                &request.client_ref,
                &args.artifact_ref,
                plugin_host::NATIVE_PLUGIN_PACKAGE_MAX_BYTES,
            )
            .map_err(|error| error.to_string())?;
        if !addon_package_media_type_is_supported(&artifact.projection.media_type) {
            return Err("The artifact is not a supported addon package".into());
        }
        if artifact.projection.digest != checksum {
            return Err("The addon package checksum does not match the artifact".into());
        }
        Ok(PublicMcpAddonPackage {
            checksum,
            bytes: artifact.bytes,
        })
    }
}
impl PublicMcpHost {
    pub fn sync_addon_refs(
        &mut self,
        client_ref: &ClientRef,
        registry: &plugin_host::NativePluginRegistry,
    ) {
        let ids = registry
            .plugins()
            .iter()
            .map(|plugin| plugin.manifest.id.as_str())
            .collect::<HashSet<_>>();
        let removed_refs = self
            .addon_refs
            .extract_if(|(owner, id), _| owner == client_ref && !ids.contains(id.as_str()))
            .map(|(_, addon_ref)| addon_ref)
            .collect::<HashSet<_>>();
        self.addon_ids
            .retain(|addon_ref, _| !removed_refs.contains(addon_ref));
        for plugin in registry.plugins() {
            let _ = self.addon_ref(client_ref, &plugin.manifest.id);
        }
    }

    pub fn addon_ref(&mut self, client_ref: &ClientRef, plugin_id: &str) -> AddonRef {
        let key = (client_ref.clone(), plugin_id.to_owned());
        let addon_ref = self.addon_refs.entry(key).or_default().clone();
        self.addon_ids
            .entry(addon_ref.clone())
            .or_insert_with(|| (client_ref.clone(), plugin_id.to_owned()));
        addon_ref
    }

    pub fn addon_id(
        &mut self,
        client_ref: &ClientRef,
        addon_ref: &AddonRef,
        registry: &plugin_host::NativePluginRegistry,
    ) -> Option<String> {
        self.sync_addon_refs(client_ref, registry);
        self.addon_ids
            .get(addon_ref)
            .filter(|(owner, _)| owner == client_ref)
            .map(|(_, plugin_id)| plugin_id.clone())
    }

    pub fn remove_addon_ref(&mut self, client_ref: &ClientRef, addon_ref: &AddonRef) {
        let Some((owner, plugin_id)) = self.addon_ids.get(addon_ref) else {
            return;
        };
        if owner != client_ref {
            return;
        }
        let owner = owner.clone();
        let plugin_id = plugin_id.clone();
        self.addon_ids.remove(addon_ref);
        self.addon_refs.remove(&(owner, plugin_id));
    }

    pub fn remove_client_addon_refs(&mut self, client_ref: &ClientRef) {
        let removed_refs = self
            .addon_refs
            .extract_if(|(owner, _), _| owner == client_ref)
            .map(|(_, addon_ref)| addon_ref)
            .collect::<HashSet<_>>();
        self.addon_ids
            .retain(|addon_ref, _| !removed_refs.contains(addon_ref));
    }
}

pub fn public_addon_projection(
    addon_ref: AddonRef,
    plugin: &plugin_host::NativePluginInfo,
) -> PublicAddonProjection {
    let requested_capabilities =
        plugin_host::native_plugin_requested_capabilities(&plugin.manifest, &plugin.runtime_plan)
            .unwrap_or_default();
    let capabilities_fingerprint =
        plugin_host::native_plugin_capabilities_fingerprint(&requested_capabilities).ok();
    PublicAddonProjection {
        addon_ref,
        id: plugin.manifest.id.clone(),
        name: plugin.manifest.name.clone(),
        version: plugin.manifest.version.clone(),
        description: plugin.manifest.description.clone(),
        author: plugin.manifest.author.clone(),
        enabled: plugin.config.enabled,
        state: public_addon_state(plugin.state),
        runtime_kind: plugin_host::native_runtime_kind_label(&plugin.runtime_plan),
        requested_capabilities,
        capabilities_fingerprint,
        permission_review_required: plugin_host::native_plugin_requires_permission_review(
            &plugin.manifest,
            &plugin.runtime_plan,
            &plugin.config,
        ),
        contributions: public_addon_contributions(&plugin.manifest),
    }
}

fn public_addon_contributions(
    manifest: &plugin_host::NativePluginManifest,
) -> PublicAddonContributionSummary {
    let contributes = manifest.contributes.as_ref();
    PublicAddonContributionSummary {
        tabs: contributes
            .and_then(|value| value.tabs.as_ref())
            .map_or(0, Vec::len),
        sidebar_panels: contributes
            .and_then(|value| value.sidebar_panels.as_ref())
            .map_or(0, Vec::len),
        activity_bar_items: contributes
            .and_then(|value| value.activity_bar_items.as_ref())
            .map_or(0, Vec::len),
        settings: contributes
            .and_then(|value| value.settings.as_ref())
            .map_or(0, Vec::len),
        terminal_hooks: usize::from(
            contributes.is_some_and(|value| value.terminal_hooks.is_some()),
        ),
        terminal_transports: contributes
            .and_then(|value| value.terminal_transports.as_ref())
            .map_or(0, Vec::len),
        connection_hooks: contributes
            .and_then(|value| value.connection_hooks.as_ref())
            .map_or(0, Vec::len),
        ai_tools: contributes
            .and_then(|value| value.ai_tools.as_ref())
            .map_or(0, Vec::len),
        api_commands: contributes
            .and_then(|value| value.api_commands.as_ref())
            .map_or(0, Vec::len),
        host_monitors: contributes
            .and_then(|value| value.host_monitors.as_ref())
            .map_or(0, Vec::len),
    }
}

fn public_addon_state(state: plugin_host::NativePluginState) -> &'static str {
    match state {
        plugin_host::NativePluginState::Discovered => "discovered",
        plugin_host::NativePluginState::Disabled => "disabled",
        plugin_host::NativePluginState::UnsupportedLegacyJs => "unsupported_legacy_js",
        plugin_host::NativePluginState::ReadyManifestOnly => "ready_manifest_only",
        plugin_host::NativePluginState::ReadyWasm => "ready_wasm",
        plugin_host::NativePluginState::ReadyProcess => "ready_process",
        plugin_host::NativePluginState::Loading => "loading",
        plugin_host::NativePluginState::Active => "active",
        plugin_host::NativePluginState::Error => "error",
        plugin_host::NativePluginState::AutoDisabled => "auto_disabled",
    }
}

fn normalized_checksum(checksum: &str) -> String {
    checksum
        .strip_prefix("sha256:")
        .unwrap_or(checksum)
        .to_ascii_lowercase()
}

fn addon_package_media_type_is_supported(media_type: &str) -> bool {
    matches!(
        media_type.split(';').next().map(str::trim),
        Some("application/zip" | "application/octet-stream" | "application/x-zip-compressed")
    )
}

fn public_addon_install_error(error: &str) -> &'static str {
    if error.contains("Checksum mismatch") {
        "The addon package checksum verification failed"
    } else if error.contains("Plugin ID mismatch") {
        "The addon package identity does not match the expected identity"
    } else if plugin_host::native_plugin_conflict_id(error).is_some() {
        "The addon is already installed; set replace_existing to replace it"
    } else {
        "The addon package was rejected"
    }
}

impl PublicMcpHost {
    pub fn finish_addon_install(
        &mut self,
        request: DomainRequest,
        expected_identity: &str,
        registry: &plugin_host::NativePluginRegistry,
        result: Result<
            Result<plugin_host::NativePluginUrlInstallResult, String>,
            tokio::sync::oneshot::error::RecvError,
        >,
        cancelled: bool,
    ) {
        let result = match result {
            Ok(result) => result,
            Err(_) => {
                request.finish(ToolEnvelope::failed(
                    "The addon installation worker stopped before completion",
                ));
                return;
            }
        };
        if cancelled {
            if let Err(error) = result {
                drop(Zeroizing::new(error));
            }
            request.finish(ToolEnvelope::failed(
                "The addon authorization changed before installation completed",
            ));
            return;
        }
        let install = match result {
            Ok(install) => install,
            Err(error) => {
                // Package-controlled archive paths do not cross the MCP boundary.
                let error = Zeroizing::new(error);
                request.finish(ToolEnvelope::failed(public_addon_install_error(&error)));
                return;
            }
        };
        let Some(plugin) = registry
            .plugins()
            .iter()
            .find(|plugin| plugin.manifest.id == expected_identity)
        else {
            request.finish(ToolEnvelope::failed(
                "The installed addon could not be rediscovered",
            ));
            return;
        };
        let addon_ref = self.addon_ref(&request.client_ref, &plugin.manifest.id);
        finish_serialized(
            request,
            json!({
                "addon": public_addon_projection(addon_ref, plugin),
                "checksum": install.checksum,
                "replaced_existing": install.replaced_existing,
            }),
        );
    }
}
