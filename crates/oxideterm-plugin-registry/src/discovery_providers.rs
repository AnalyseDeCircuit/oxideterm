// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_plugin_manifest::discovery::DiscoveryLaunchKind;
use std::collections::HashSet;

impl NativePluginRegistry {
    pub fn discovery_providers(
        &self,
    ) -> impl Iterator<Item = (&str, &NativePluginDiscoveryProviderDef)> {
        self.plugins()
            .iter()
            .filter(|plugin| native_plugin_process_activatable(plugin))
            .flat_map(|plugin| {
                plugin
                    .manifest
                    .contributes
                    .iter()
                    .flat_map(|c| &c.discovery_providers)
                    .map(|provider| (plugin.manifest.id.as_str(), provider))
            })
    }
}

/// A discovery declaration is independently versioned. Preserve unrelated
/// declarations in the package even if this host cannot decode a newer one.
pub(super) fn parse_manifest(
    text: &str,
    root: &Path,
) -> Result<(NativePluginManifest, Vec<String>), String> {
    let mut json: Value =
        serde_json::from_str(text).map_err(|error| format!("Invalid plugin.json: {error}"))?;
    let Some(entries) = json
        .get_mut("contributes")
        .and_then(Value::as_object_mut)
        .and_then(|c| c.remove("discoveryProviders"))
    else {
        return serde_json::from_str(text)
            .map(|manifest| (manifest, Vec::new()))
            .map_err(|error| format!("Invalid plugin.json: {error}"));
    };
    let mut manifest: NativePluginManifest =
        serde_json::from_value(json).map_err(|error| format!("Invalid plugin.json: {error}"))?;
    let runtime = native_runtime_plan_for_manifest(&manifest)?;
    let capabilities = normalize_native_plugin_capabilities(&manifest.permissions.capabilities)?;
    let process_allowed = matches!(runtime, NativePluginRuntimePlan::Process { .. })
        && manifest
            .engines
            .as_ref()
            .and_then(|e| e.oxideterm.as_ref())
            .is_some()
        && capabilities.iter().any(|cap| cap == "ui.write");
    let entries = entries
        .as_array()
        .ok_or("discoveryProviders must be an array")?;
    let mut ids = HashSet::new();
    let mut duplicates = HashSet::new();
    for id in entries
        .iter()
        .filter_map(|v| v.get("id").and_then(Value::as_str))
    {
        if !ids.insert(id) {
            duplicates.insert(id);
        }
    }
    let mut diagnostics = Vec::new();
    let providers = &mut manifest
        .contributes
        .as_mut()
        .expect("contributes supplied discoveryProviders")
        .discovery_providers;
    for (index, entry) in entries.iter().enumerate() {
        let result: Result<NativePluginDiscoveryProviderDef, String> = (|| {
            if !process_allowed {
                return Err(
                    "Discovery providers require a process runtime, host range and ui.write".into(),
                );
            }
            if entry
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| duplicates.contains(id))
            {
                return Err("Duplicate discovery provider ID".into());
            }
            let provider: NativePluginDiscoveryProviderDef = serde_json::from_value(entry.clone())
                .map_err(|_| "Unsupported discovery provider declaration")?;
            validate_provider(
                &provider,
                capabilities.iter().any(|c| c == "credentials.raw.read"),
            )?;
            if let Some(entry) = &provider.terminal_entry {
                let root = root
                    .canonicalize()
                    .map_err(|_| "Missing discovery plugin package")?;
                let path = root
                    .join(entry)
                    .canonicalize()
                    .map_err(|_| "Missing discovery terminal entry")?;
                if !path.starts_with(root) || !path.is_file() {
                    return Err("Discovery terminal entry must be a file inside the package".into());
                }
            }
            Ok(provider)
        })();
        match result {
            Ok(provider) => providers.push(provider),
            Err(reason) => diagnostics.push(format!("discoveryProviders[{index}]: {reason}")),
        }
    }
    Ok((manifest, diagnostics))
}

fn unique<T: Eq + std::hash::Hash>(values: &[T]) -> bool {
    let mut seen = HashSet::new();
    values.iter().all(|value| seen.insert(value))
}

fn validate_controls(
    controls: &[NativePluginDeclarativeUiControl],
    section: &str,
    fields: &mut HashSet<String>,
    secrets: &mut Vec<(String, Vec<String>)>,
) -> Result<(), String> {
    for control in controls {
        let field = matches!(
            control.kind.as_str(),
            "text" | "number" | "checkbox" | "select" | "textarea" | "password"
        );
        if (!field
            && !matches!(
                control.kind.as_str(),
                "stack" | "row" | "columns" | "markdown"
            ))
            || control.id.as_ref().is_some_and(|id| id.contains('/'))
            || control
                .discovery_use
                .as_deref()
                .is_some_and(|purpose| !field || !matches!(purpose, "catalog" | "open" | "both"))
        {
            return Err("Unsupported discovery form field or use".into());
        }
        if field && let Some(id) = &control.id {
            fields.insert(format!("{section}/{id}"));
        }
        if control.kind == "password" {
            let scope = control
                .secret_scope
                .as_ref()
                .ok_or("Discovery password fields require a secretScope")?;
            secrets.push((
                format!("{section}/{}", control.id.as_deref().unwrap_or_default()),
                scope.clone(),
            ));
        } else if control.secret_scope.is_some() {
            return Err("Only discovery password fields may declare secretScope".into());
        }
        validate_controls(&control.children, section, fields, secrets)?;
    }
    Ok(())
}

fn validate_provider(
    provider: &NativePluginDiscoveryProviderDef,
    can_read_secrets: bool,
) -> Result<(), String> {
    for field in [&provider.id, &provider.title, &provider.icon] {
        validate_manifest_text_field("discoveryProvider", field)?;
    }
    if provider.protocol_version != 1
        || provider.launch_kinds.is_empty()
        || !unique(&provider.launch_kinds)
    {
        return Err("Invalid discovery provider identity, protocol version or launch kinds".into());
    }
    if provider
        .launch_kinds
        .contains(&DiscoveryLaunchKind::Terminal)
        != provider.terminal_entry.is_some()
    {
        return Err("Terminal discovery providers require a package terminalEntry".into());
    }
    if let Some(entry) = &provider.terminal_entry {
        validate_plugin_relative_path(entry)?;
    }
    // Sources without platform-specific settings still use the host's common controls.
    validate_declarative_form(&provider.form, true)?;
    let mut fields = HashSet::new();
    let mut secrets = Vec::new();
    validate_controls(&provider.form.controls, "root", &mut fields, &mut secrets)?;
    for section in &provider.form.sections {
        if section.id.contains('/') {
            return Err("Invalid discovery form section ID".into());
        }
        validate_controls(&section.controls, &section.id, &mut fields, &mut secrets)?;
    }
    if !secrets.is_empty() && !can_read_secrets {
        return Err("Discovery password fields require credentials.raw.read".into());
    }
    for (_, scope) in &secrets {
        if !unique(scope)
            || scope
                .iter()
                .any(|key| !fields.contains(key) || secrets.iter().any(|(secret, _)| secret == key))
        {
            return Err("Discovery secretScope must reference distinct non-secret fields".into());
        }
    }
    let mut action_ids = HashSet::new();
    for action in &provider.source_actions {
        validate_manifest_text_field("sourceActions.id", &action.id)?;
        validate_manifest_text_field("sourceActions.title", &action.title)?;
        if action
            .field
            .as_ref()
            .is_some_and(|key| !fields.contains(key))
        {
            return Err("Unknown discovery source action field".into());
        }
        if action.id.chars().any(char::is_control)
            || action.title.chars().any(char::is_control)
            || !action_ids.insert(&action.id)
        {
            return Err("Invalid or duplicate discovery source action".into());
        }
    }
    Ok(())
}
