// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashSet;

use oxideterm_connections::oxide_file::{
    EncryptedPluginSetting, ImportResultEnvelope, OxideForwardRecord,
};
use oxideterm_forwarding::OwnedForwardImportRecord;
use oxideterm_i18n::I18n;
use oxideterm_quick_commands::{QuickCommandImportResult, QuickCommandImportStrategy};
use oxideterm_settings::{PersistedSettings, merge_oxide_settings_snapshot};
use zeroize::Zeroizing;

use super::{OxideClientStateImportOptions, OxideClientStateImportResult};

/// Application-owned stores and runtime notifications used after the archive commits.
pub trait OxideImportHost {
    fn import_forwards(
        &mut self,
        records: &[OwnedForwardImportRecord],
        replace_owners: &HashSet<String>,
        merge_owners: &HashSet<String>,
    ) -> Result<usize, String>;
    fn import_quick_commands(
        &mut self,
        snapshot: &str,
        strategy: QuickCommandImportStrategy,
    ) -> QuickCommandImportResult;
    fn import_plugin_settings(
        &mut self,
        entries: &[EncryptedPluginSetting],
    ) -> Result<usize, String>;
    fn current_settings(&self) -> &PersistedSettings;
    fn import_settings(&mut self, settings: PersistedSettings) -> Result<(), ()>;
    fn store_ai_provider_key(&mut self, id: &str, key: Zeroizing<String>) -> Result<(), String>;
    fn show_import_status(&mut self, status: String);
}

pub struct OxideSidecarOptions<'a> {
    pub forwards: bool,
    pub quick_commands: Option<QuickCommandImportStrategy>,
    pub plugin_settings: bool,
    pub selected_plugin_ids: Option<&'a HashSet<String>>,
    pub app_settings: bool,
    pub selected_app_settings_sections: Option<&'a HashSet<String>>,
    pub portable_secrets: bool,
}

impl<'a> OxideSidecarOptions<'a> {
    pub fn for_dialog(options: &'a OxideClientStateImportOptions) -> Self {
        Self {
            // The archive worker already honors the two sidecar flags before delivery.
            forwards: true,
            portable_secrets: true,
            quick_commands: options
                .import_quick_commands
                .then_some(options.quick_command_strategy),
            plugin_settings: options.import_plugin_settings,
            selected_plugin_ids: options.selected_plugin_ids.as_ref(),
            app_settings: options.import_app_settings,
            selected_app_settings_sections: options.selected_app_settings_sections.as_ref(),
        }
    }
}

/// Apply sidecars in their established order after the core connection store has committed.
pub fn apply_oxide_sidecars(
    host: &mut impl OxideImportHost,
    mut envelope: ImportResultEnvelope,
    options: OxideSidecarOptions<'_>,
    i18n: &I18n,
) -> (OxideClientStateImportResult, bool) {
    envelope.imported_forwards = if options.forwards && !envelope.forward_records.is_empty() {
        let records = envelope
            .forward_records
            .iter()
            .map(owned_forward_import_record)
            .collect::<Vec<_>>();
        let replace_owners = envelope.forward_replace_owner_ids.iter().cloned().collect();
        let merge_owners = envelope.forward_merge_owner_ids.iter().cloned().collect();
        match host.import_forwards(&records, &replace_owners, &merge_owners) {
            Ok(count) => count,
            Err(error) => {
                envelope
                    .errors
                    .push(format!("Failed to save imported forwards: {error}"));
                0
            }
        }
    } else {
        0
    };
    let (imported_quick_commands, skipped_quick_commands, quick_commands_errors) = match (
        envelope.quick_commands_json.as_deref(),
        options.quick_commands,
    ) {
        (Some(snapshot), Some(strategy)) => {
            let result = host.import_quick_commands(snapshot, strategy);
            (result.imported, !result.errors.is_empty(), result.errors)
        }
        (Some(_), None) => (0, true, Vec::new()),
        (None, _) => (0, false, Vec::new()),
    };
    let (imported_plugin_settings, plugin_settings_failed) =
        if !options.plugin_settings || envelope.plugin_settings.is_empty() {
            (0, false)
        } else {
            let settings = envelope
                .plugin_settings
                .iter()
                .filter(|entry| includes_plugin_setting(entry, options.selected_plugin_ids))
                .cloned()
                .collect::<Vec<_>>();
            match host.import_plugin_settings(&settings) {
                Ok(count) => (count, false),
                Err(_) => (0, true),
            }
        };
    let skipped_plugin_settings = !options.plugin_settings && !envelope.plugin_settings.is_empty();
    let (imported_app_settings, skipped_app_settings) =
        match (options.app_settings, envelope.app_settings_json.as_deref()) {
            (true, Some(snapshot)) => {
                match merge_oxide_settings_snapshot(
                    host.current_settings(),
                    snapshot,
                    options.selected_app_settings_sections,
                ) {
                    Ok(merged) => match host.import_settings(merged) {
                        Ok(()) => (true, false),
                        Err(()) => {
                            host.show_import_status(i18n.t("modals.import.skipped_app_settings"));
                            (false, true)
                        }
                    },
                    Err(error) => {
                        host.show_import_status(format!("应用设置导入失败: {error}"));
                        (false, true)
                    }
                }
            }
            (false, Some(_)) => (false, true),
            (_, None) => (false, false),
        };
    if options.portable_secrets {
        restore_oxide_portable_secrets(&mut envelope, |id, key| {
            host.store_ai_provider_key(id, key)
        });
    }
    (
        OxideClientStateImportResult {
            envelope,
            imported_app_settings,
            skipped_app_settings,
            imported_quick_commands,
            skipped_quick_commands,
            quick_commands_errors,
            imported_plugin_settings,
            skipped_plugin_settings,
        },
        plugin_settings_failed,
    )
}

pub fn restore_oxide_portable_secrets(
    envelope: &mut ImportResultEnvelope,
    mut store_key: impl FnMut(&str, Zeroizing<String>) -> Result<(), String>,
) {
    let total = envelope.portable_secrets.len();
    if total == 0 {
        return;
    }

    let mut imported = 0usize;
    for secret in envelope.portable_secrets.drain(..) {
        if secret.kind != "ai_provider_key" || secret.id.trim().is_empty() {
            envelope.errors.push(format!(
                "Unsupported portable secret kind '{}' for id '{}'",
                secret.kind, secret.id
            ));
            continue;
        }

        match store_key(&secret.id, secret.secret) {
            Ok(()) => imported += 1,
            Err(error) => envelope.errors.push(format!(
                "Failed to import portable secret '{}': {error}",
                secret.id
            )),
        }
    }

    envelope.imported_portable_secrets = imported;
    envelope.skipped_portable_secrets = total.saturating_sub(imported);
}

pub(super) fn includes_plugin_setting(
    entry: &EncryptedPluginSetting,
    selected: Option<&HashSet<String>>,
) -> bool {
    selected.is_none_or(|ids| {
        plugin_id_from_setting_storage_key(&entry.storage_key).is_some_and(|id| ids.contains(&id))
    })
}

fn owned_forward_import_record(record: &OxideForwardRecord) -> OwnedForwardImportRecord {
    OwnedForwardImportRecord {
        owner_connection_id: record.connection_id.clone(),
        forward_type: record.forward_type.clone(),
        bind_address: record.bind_address.clone(),
        bind_port: record.bind_port,
        target_host: record.target_host.clone(),
        target_port: record.target_port,
        description: record.description.clone(),
        auto_start: record.auto_start,
        tunnel: record
            .tunnel
            .as_ref()
            .map(|profile| oxideterm_forwarding::SshTunnelProfile {
                name: profile.name.clone(),
                group: profile.group.clone(),
                connect_on_startup: profile.connect_on_startup,
            }),
    }
}

pub(super) fn plugin_id_from_setting_storage_key(storage_key: &str) -> Option<String> {
    const PREFIX: &str = "oxide-plugin-";
    const SEPARATOR: &str = "-setting-";

    let remainder = storage_key.strip_prefix(PREFIX)?;
    let separator_index = remainder.find(SEPARATOR)?;
    let plugin_id = &remainder[..separator_index];
    let setting_id = &remainder[separator_index + SEPARATOR.len()..];
    if plugin_id.is_empty() || setting_id.is_empty() {
        return None;
    }
    Some(plugin_id.to_string())
}
