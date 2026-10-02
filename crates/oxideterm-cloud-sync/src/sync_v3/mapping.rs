// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use anyhow::{Context, Result};
use oxideterm_connections::oxide_file::EncryptedPortableSecret;
use oxideterm_connections::{
    ConnectionStore, ResolvedConnectionConfiguration, SavedConnectionSyncRecord,
    SavedConnectionsSyncSnapshot,
};
use oxideterm_forwarding::{ForwardingRegistry, PersistedForwardDto};
use oxideterm_settings::{
    PersistedSettings, SettingsStore, export_oxide_settings_snapshot_json,
    merge_oxide_settings_snapshot, migrate_legacy_theme_selection,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use zeroize::{Zeroize, Zeroizing};

use super::model::PrivateJson;
use super::{FieldValue, ResourceKind, SyncField, SyncResource, SyncValues};
use crate::{
    OXIDE_APP_SETTINGS_SECTION_IDS, SyncScope, operation::StructuredUploadItemFilter,
    plugin_settings,
};

const DISPLAY_FIELDS: &[&str] = &[
    "name",
    "group",
    "notes",
    "description",
    "icon",
    "color",
    "icon_background_color",
    "iconBackgroundColor",
    "tags",
];
const RUNTIME_FIELDS: &[&str] = &["updated_at", "updatedAt", "last_used_at", "lastUsedAt"];

/// Captures portable domain records and local-only reconstruction fields. Only
/// `values` participates in synchronization; originals never enter a publication.
pub struct ConfigurationView {
    pub values: SyncValues,
    originals: BTreeMap<SyncResource, FieldValue>,
}

impl ConfigurationView {
    pub(crate) fn empty() -> Self {
        Self {
            values: BTreeMap::new(),
            originals: BTreeMap::new(),
        }
    }

    pub(crate) fn mark_deleted(&mut self, kind: ResourceKind, id: &str) -> Result<()> {
        self.put(
            &SyncResource {
                kind,
                id: id.into(),
            },
            "$present",
            &false,
        )
    }

    pub(crate) fn insert_connections(
        &mut self,
        snapshot: SavedConnectionsSyncSnapshot,
    ) -> Result<()> {
        for record in snapshot.records {
            let resource = SyncResource {
                kind: ResourceKind::Connection,
                id: record.id.clone(),
            };
            if record.deleted {
                self.put(&resource, "$present", &false)?;
                continue;
            }
            if let Some(payload) = record.payload {
                let mut value = PrivateJson(serde_json::to_value(payload)?);
                value.0["options"] = serde_json::to_value(record.options.unwrap_or_default())?;
                self.insert(ResourceKind::Connection, &record.id, &value.0)?;
            }
        }
        for record in snapshot.local_terminal_profiles {
            self.insert(ResourceKind::LocalTerminal, &record.id, &record)?;
        }
        for record in snapshot.local_terminal_tombstones {
            self.mark_deleted(ResourceKind::LocalTerminal, &record.id)?;
        }
        for record in snapshot.totp_credentials {
            self.insert(ResourceKind::Totp, &record.id, &record.portable())?;
        }
        Ok(())
    }

    pub(crate) fn insert_app_settings(&mut self, json: &str) -> Result<()> {
        let mut envelope = PrivateJson(serde_json::from_str(json)?);
        envelope
            .0
            .as_object_mut()
            .context("Invalid archived settings")?
            .remove("exportedAt");
        let includes_appearance = envelope
            .0
            .get("sectionIds")
            .and_then(Value::as_array)
            .is_none_or(|sections| {
                sections
                    .iter()
                    .any(|section| section.as_str() == Some("appearance"))
            });
        if includes_appearance && let Some(settings) = envelope.0.get_mut("settings") {
            // The supplied-field filter must retain the migrated theme, without
            // changing the application theme for terminal-only archives.
            migrate_legacy_theme_selection(settings);
        }
        let merged = merge_oxide_settings_snapshot(&PersistedSettings::default(), json, None)?;
        for section in OXIDE_APP_SETTINGS_SECTION_IDS {
            let section_json = Zeroizing::new(export_oxide_settings_snapshot_json(
                &merged,
                Some(&HashSet::from([section.to_string()])),
                true,
            )?);
            let mut selected = PrivateJson(serde_json::from_str(&section_json)?);
            let Some(fields) = envelope.0.get("settings") else {
                continue;
            };
            let selected_fields = selected
                .0
                .get_mut("settings")
                .context("Invalid settings section")?;
            retain_supplied_settings(selected_fields, fields);
            if selected_fields
                .as_object()
                .is_none_or(|fields| fields.is_empty())
            {
                continue;
            }
            let fields = PrivateJson(
                selected
                    .0
                    .as_object_mut()
                    .unwrap()
                    .remove("settings")
                    .unwrap(),
            );
            selected.0.as_object_mut().unwrap().remove("exportedAt");
            let resource = SyncResource {
                kind: ResourceKind::AppSettings,
                id: section.to_string(),
            };
            self.put(&resource, "$present", &true)?;
            self.put(&resource, "configuration", &selected.0)?;
            self.insert_settings(&resource, Vec::new(), &fields.0)?;
        }
        Ok(())
    }
    pub(crate) fn insert_secret(
        &mut self,
        kind: ResourceKind,
        id: &str,
        value: &impl Serialize,
    ) -> Result<()> {
        let resource = SyncResource {
            kind,
            id: id.into(),
        };
        self.put(&resource, "$present", &true)?;
        self.put(&resource, "configuration", value)
    }
    pub fn collect(
        connections: &ConnectionStore,
        forwards: &ForwardingRegistry,
        settings: &SettingsStore,
        scope: &SyncScope,
    ) -> Result<Self> {
        let mut view = Self {
            values: BTreeMap::new(),
            originals: BTreeMap::new(),
        };
        let saved = connections.export_saved_connections_snapshot()?;
        for record in saved.records {
            if let Some(payload) = record.payload.filter(|_| !record.deleted) {
                let mut value = PrivateJson(serde_json::to_value(payload)?);
                value.0["options"] = serde_json::to_value(record.options.unwrap_or_default())?;
                view.insert(ResourceKind::Connection, &record.id, &value.0)?;
            }
        }
        for record in saved.local_terminal_profiles {
            view.insert(ResourceKind::LocalTerminal, &record.id, &record)?;
        }
        for record in saved.totp_credentials {
            view.insert(ResourceKind::Totp, &record.id, &record.portable())?;
        }
        for record in connections.export_serial_profiles_snapshot()?.records {
            view.insert(ResourceKind::Serial, &record.id, &record)?;
        }
        for record in connections.export_telnet_profiles_snapshot()?.records {
            view.insert(ResourceKind::Telnet, &record.id, &record)?;
        }
        for record in connections.export_mosh_profiles_snapshot()?.records {
            view.insert(ResourceKind::Mosh, &record.id, &record)?;
        }
        let sftp = connections.export_standalone_sftp_profiles_snapshot()?;
        for record in sftp.records {
            view.insert(ResourceKind::StandaloneSftp, &record.id, &record)?;
        }
        if let Some(ftp) = sftp.ftp {
            for record in ftp.records {
                view.insert(ResourceKind::Ftp, &record.id, &record)?;
            }
        }
        for mut record in connections
            .export_remote_desktop_profiles_snapshot()?
            .records
        {
            record.credential_ref = None;
            view.insert(ResourceKind::RemoteDesktop, &record.id, &record)?;
        }
        for record in forwards.export_saved_forwards_snapshot()?.records {
            if let Some(payload) = record.payload.filter(|_| !record.deleted) {
                view.insert(ResourceKind::Forward, &record.id, &payload)?;
            }
        }
        let commands =
            oxideterm_quick_commands::load_snapshot(settings.path()).map_err(anyhow::Error::msg)?;
        for record in commands.commands {
            view.insert(ResourceKind::QuickCommand, &record.id, &record)?;
        }
        for record in commands.categories {
            view.insert(ResourceKind::QuickCommandCategory, &record.id, &record)?;
        }
        for section in OXIDE_APP_SETTINGS_SECTION_IDS {
            let json = Zeroizing::new(export_oxide_settings_snapshot_json(
                settings.settings(),
                Some(&HashSet::from([section.to_string()])),
                scope.include_local_terminal_env_vars,
            )?);
            let mut value = PrivateJson(serde_json::from_str(&json)?);
            value.0.as_object_mut().unwrap().remove("exportedAt");
            let fields = PrivateJson(
                value
                    .0
                    .as_object_mut()
                    .unwrap()
                    .remove("settings")
                    .unwrap_or(Value::Null),
            );
            let resource = SyncResource {
                kind: ResourceKind::AppSettings,
                id: section.to_string(),
            };
            view.put(&resource, "$present", &true)?;
            view.put(&resource, "configuration", &value.0)?;
            view.insert_settings(&resource, Vec::new(), &fields.0)?;
        }
        for record in
            plugin_settings::load_plugin_settings(settings.path()).map_err(anyhow::Error::msg)?
        {
            if plugin_settings::plugin_id_from_setting_storage_key(&record.storage_key)
                .is_some_and(|id| id != crate::CLOUD_SYNC_PLUGIN_ID)
            {
                let resource = SyncResource {
                    kind: ResourceKind::PluginSettings,
                    id: record.storage_key.clone(),
                };
                view.put(&resource, "$present", &true)?;
                view.put(&resource, "configuration", &record.serialized_value)?;
            }
        }
        Ok(view)
    }

    pub fn include_credentials(&mut self, mut secrets: Vec<EncryptedPortableSecret>) -> Result<()> {
        let result = (|| {
            for secret in &secrets {
                // Setting and clearing a secret are writes to the same register.
                let resource = SyncResource {
                    kind: ResourceKind::Credential,
                    id: secret.id.clone(),
                };
                self.put(&resource, "$present", &true)?;
                self.put(&resource, "configuration", secret)?;
            }
            Ok(())
        })();
        for secret in &mut secrets {
            secret.secret.zeroize();
        }
        result
    }

    pub fn selected(
        &self,
        known: impl Iterator<Item = SyncResource>,
        scope: &SyncScope,
        filter: &StructuredUploadItemFilter,
    ) -> BTreeSet<SyncResource> {
        let mut selected: BTreeSet<_> = known
            .chain(self.values.keys().map(|field| field.resource.clone()))
            .filter(|resource| {
                let within = |ids: &Option<BTreeSet<String>>| {
                    ids.as_ref().is_none_or(|ids| ids.contains(&resource.id))
                };
                match resource.kind {
                    ResourceKind::Connection | ResourceKind::LocalTerminal => {
                        scope.sync_connections && within(&filter.connection_ids)
                    }
                    ResourceKind::StandaloneSftp | ResourceKind::Ftp => scope.sync_connections,
                    ResourceKind::Totp => scope.sync_connections && filter.connection_ids.is_none(),
                    ResourceKind::Forward => scope.sync_forwards && within(&filter.forward_ids),
                    ResourceKind::QuickCommand => {
                        scope.sync_quick_commands && within(&filter.quick_command_ids)
                    }
                    ResourceKind::QuickCommandCategory => {
                        scope.sync_quick_commands && filter.quick_command_ids.is_none()
                    }
                    ResourceKind::Serial => {
                        scope.sync_serial_profiles && within(&filter.serial_profile_ids)
                    }
                    ResourceKind::Telnet => {
                        scope.sync_telnet_profiles && within(&filter.telnet_profile_ids)
                    }
                    ResourceKind::Mosh => {
                        scope.sync_mosh_profiles && within(&filter.mosh_profile_ids)
                    }
                    ResourceKind::RemoteDesktop => {
                        scope.sync_remote_desktop_profiles
                            && within(&filter.remote_desktop_profile_ids)
                    }
                    ResourceKind::AppSettings => {
                        scope.sync_app_settings
                            && scope.app_settings_sections.contains(&resource.id)
                    }
                    ResourceKind::PluginSettings => {
                        scope.sync_plugin_settings
                            && plugin_settings::plugin_id_from_setting_storage_key(&resource.id)
                                .is_some_and(|id| {
                                    id != crate::CLOUD_SYNC_PLUGIN_ID
                                        && scope
                                            .plugin_ids
                                            .as_ref()
                                            .is_none_or(|ids| ids.contains(&id))
                                })
                    }
                    ResourceKind::Credential => {
                        scope.sync_sensitive_credentials
                            && credential_selected(&resource.id, scope, filter)
                    }
                    ResourceKind::ManagedKey => false,
                    ResourceKind::AiCredential => {
                        scope.sync_sensitive_credentials
                            && scope.sync_app_settings
                            && scope.app_settings_sections.iter().any(|id| id == "ai")
                    }
                    ResourceKind::PluginCredential => {
                        scope.sync_sensitive_credentials
                            && super::auxiliary_secrets::plugin_selected(&resource.id, scope)
                    }
                    ResourceKind::PrivilegeCredential => {
                        scope.sync_sensitive_credentials
                            && scope.sync_connections
                            && serde_json::from_str::<(String, String)>(&resource.id)
                                .ok()
                                .is_some_and(|(owner, _)| {
                                    filter
                                        .connection_ids
                                        .as_ref()
                                        .is_none_or(|ids| ids.contains(&owner))
                                })
                    }
                }
            })
            .collect();
        let mut dependencies = BTreeSet::new();
        for (field, value) in &self.values {
            if field.group != "configuration" || !selected.contains(&field.resource) {
                continue;
            }
            if let Ok(value) = value.decode::<Value>() {
                let value = PrivateJson(value);
                match field.resource.kind {
                    ResourceKind::Connection
                    | ResourceKind::Mosh
                    | ResourceKind::StandaloneSftp => {
                        collect_totp_references(&value.0, &mut dependencies);
                    }
                    ResourceKind::QuickCommand => {
                        if let Some(id) = value.0.get("collectionId").and_then(Value::as_str) {
                            dependencies.insert(SyncResource {
                                kind: ResourceKind::QuickCommandCategory,
                                id: id.into(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        if !scope.sync_sensitive_credentials {
            dependencies.retain(|resource| resource.kind != ResourceKind::ManagedKey);
        }
        selected.extend(dependencies);
        if scope.sync_sensitive_credentials {
            for field in self
                .values
                .keys()
                .filter(|field| field.resource.kind == ResourceKind::Credential)
            {
                if let Ok(oxideterm_connections::CredentialTarget {
                    owner: oxideterm_connections::CredentialOwner::Totp(id),
                    ..
                }) = serde_json::from_str(&field.resource.id)
                    && selected.contains(&SyncResource {
                        kind: ResourceKind::Totp,
                        id,
                    })
                {
                    selected.insert(field.resource.clone());
                }
            }
        }
        selected
    }

    pub fn select_merged(
        &self,
        values: SyncValues,
        known: impl Iterator<Item = SyncResource>,
        scope: &SyncScope,
        filter: &StructuredUploadItemFilter,
    ) -> BTreeSet<SyncResource> {
        Self {
            values,
            originals: BTreeMap::new(),
        }
        .selected(known, scope, filter)
    }

    pub fn resolved_connections(
        &self,
        values: &SyncValues,
    ) -> Result<ResolvedConnectionConfiguration> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut records = Vec::new();
        for resource in active_resources(values, ResourceKind::Connection)? {
            let mut value = self.record(&resource, values)?;
            let options = value
                .0
                .as_object_mut()
                .unwrap()
                .remove("options")
                .map(serde_json::from_value)
                .transpose()
                .map_err(|_| anyhow::anyhow!("Invalid resolved connection options"))?;
            let payload = serde_json::from_value(value.0.clone())
                .map_err(|_| anyhow::anyhow!("Invalid resolved connection configuration"))?;
            records.push(SavedConnectionSyncRecord {
                id: resource.id,
                revision: "sync-v3".into(),
                updated_at: now.clone(),
                deleted: false,
                payload: Some(payload),
                options,
            });
        }
        Ok(ResolvedConnectionConfiguration {
            connections: SavedConnectionsSyncSnapshot {
                revision: "sync-v3".into(),
                exported_at: now,
                records,
                local_terminal_profiles: self.records(ResourceKind::LocalTerminal, values)?,
                local_terminal_tombstones: Vec::new(),
                totp_credentials: self.records(ResourceKind::Totp, values)?,
            },
            serial: self.records(ResourceKind::Serial, values)?,
            telnet: self.records(ResourceKind::Telnet, values)?,
            mosh: self.records(ResourceKind::Mosh, values)?,
            sftp: self.records(ResourceKind::StandaloneSftp, values)?,
            ftp: self.records(ResourceKind::Ftp, values)?,
            remote_desktop: self.records(ResourceKind::RemoteDesktop, values)?,
        })
    }

    pub fn resolved_forwards(&self, values: &SyncValues) -> Result<Vec<PersistedForwardDto>> {
        self.records(ResourceKind::Forward, values)
    }

    pub fn resolved_commands(
        &self,
        values: &SyncValues,
    ) -> Result<oxideterm_quick_commands::QuickCommandsSnapshot> {
        Ok(oxideterm_quick_commands::QuickCommandsSnapshot {
            version: oxideterm_quick_commands::QUICK_COMMANDS_SCHEMA_VERSION,
            categories: self.records(ResourceKind::QuickCommandCategory, values)?,
            commands: self.records(ResourceKind::QuickCommand, values)?,
            updated_at: chrono::Utc::now().timestamp_millis().max(0) as u64,
        })
    }

    pub fn resolved_settings(
        &self,
        values: &SyncValues,
        current: &PersistedSettings,
    ) -> Result<PersistedSettings> {
        let mut next = current.clone();
        for resource in active_resources(values, ResourceKind::AppSettings)? {
            let mut envelope =
                PrivateJson(required_value(values, &resource, "configuration")?.decode()?);
            let mut settings = PrivateJson(Value::Object(Map::new()));
            for (field, value) in values
                .iter()
                .filter(|(field, _)| field.resource == resource)
            {
                if let Some(path) = field.group.strip_prefix("setting:") {
                    let path: Vec<String> = serde_json::from_str(path)
                        .map_err(|_| anyhow::anyhow!("Invalid settings path"))?;
                    assign_setting(&mut settings.0, &path, value.decode()?)?;
                }
            }
            envelope.0["settings"] = std::mem::take(&mut settings.0);
            let encoded = Zeroizing::new(serde_json::to_string(&envelope.0)?);
            next = merge_oxide_settings_snapshot(
                &next,
                &encoded,
                Some(&HashSet::from([resource.id])),
            )?;
        }
        crate::credentials::preserve_global_proxy_reference(current, &mut next);
        Ok(next)
    }

    pub fn resolved_plugins(
        &self,
        values: &SyncValues,
    ) -> Result<Vec<oxideterm_connections::oxide_file::EncryptedPluginSetting>> {
        active_resources(values, ResourceKind::PluginSettings)?
            .into_iter()
            .map(|resource| {
                Ok(oxideterm_connections::oxide_file::EncryptedPluginSetting {
                    serialized_value: required_value(values, &resource, "configuration")?
                        .decode()?,
                    storage_key: resource.id,
                })
            })
            .collect()
    }

    pub fn resolved_credentials(
        &self,
        values: &SyncValues,
    ) -> Result<Vec<EncryptedPortableSecret>> {
        active_resources(values, ResourceKind::Credential)?
            .iter()
            .map(|resource| {
                let mut secret: EncryptedPortableSecret =
                    required_value(values, resource, "configuration")?.decode()?;
                if secret.id != resource.id
                    || !oxideterm_connections::is_profile_credential(&secret)
                {
                    secret.secret.zeroize();
                    anyhow::bail!("Invalid synchronized credential identity");
                }
                Ok(secret)
            })
            .collect()
    }

    pub(crate) fn insert(
        &mut self,
        kind: ResourceKind,
        id: &str,
        record: &impl Serialize,
    ) -> Result<()> {
        let resource = SyncResource {
            kind,
            id: id.into(),
        };
        let original = FieldValue::encode(record)?;
        let mut value = PrivateJson(original.decode()?);
        self.originals.insert(resource.clone(), original);
        let object = value
            .0
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid portable sync record"))?;
        for name in RUNTIME_FIELDS
            .iter()
            .chain(["id", "credential_ref", "passwordKeychainId"].iter())
        {
            if let Some(removed) = object.remove(*name) {
                drop(PrivateJson(removed));
            }
        }
        self.put(&resource, "$present", &true)?;
        for name in DISPLAY_FIELDS {
            if let Some(display) = object.remove(*name) {
                let display = PrivateJson(display);
                self.put(&resource, name, &display.0)?;
            }
        }
        self.put(&resource, "configuration", &value.0)
    }

    fn put(&mut self, resource: &SyncResource, group: &str, value: &impl Serialize) -> Result<()> {
        self.values.insert(
            SyncField {
                resource: resource.clone(),
                group: group.into(),
            },
            FieldValue::encode(value)?,
        );
        Ok(())
    }

    fn insert_settings(
        &mut self,
        resource: &SyncResource,
        path: Vec<String>,
        value: &Value,
    ) -> Result<()> {
        if let Value::Object(fields) = value {
            if !fields.is_empty() {
                for (key, value) in fields {
                    let mut child = path.clone();
                    child.push(key.clone());
                    self.insert_settings(resource, child, value)?;
                }
                return Ok(());
            }
        }
        self.put(
            resource,
            &format!("setting:{}", serde_json::to_string(&path)?),
            value,
        )
    }

    fn record(&self, resource: &SyncResource, values: &SyncValues) -> Result<PrivateJson> {
        let mut value = PrivateJson(required_value(values, resource, "configuration")?.decode()?);
        let object = value
            .0
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid resolved sync record"))?;
        object.insert("id".into(), Value::String(resource.id.clone()));
        for name in DISPLAY_FIELDS {
            if let Some(field) = values.get(&SyncField {
                resource: resource.clone(),
                group: name.to_string(),
            }) {
                object.insert(name.to_string(), field.decode()?);
            }
        }
        let original = self
            .originals
            .get(resource)
            .map(FieldValue::decode)
            .transpose()?
            .map(PrivateJson);
        for name in ["last_used_at", "lastUsedAt"] {
            if let Some(previous) = original.as_ref().and_then(|original| original.0.get(name)) {
                object.insert(name.into(), previous.clone());
            }
        }
        match resource.kind {
            ResourceKind::QuickCommand | ResourceKind::QuickCommandCategory => {
                object.insert(
                    "updatedAt".into(),
                    Value::from(chrono::Utc::now().timestamp_millis().max(0) as u64),
                );
            }
            ResourceKind::Ftp => {
                object.insert(
                    "updatedAt".into(),
                    Value::String(chrono::Utc::now().to_rfc3339()),
                );
            }
            _ => {
                object.insert(
                    "updated_at".into(),
                    Value::String(chrono::Utc::now().to_rfc3339()),
                );
            }
        }
        Ok(value)
    }

    fn records<T: DeserializeOwned>(
        &self,
        kind: ResourceKind,
        values: &SyncValues,
    ) -> Result<Vec<T>> {
        active_resources(values, kind)?
            .iter()
            .map(|resource| {
                let value = self.record(resource, values)?;
                serde_json::from_value(value.0.clone())
                    .map_err(|_| anyhow::anyhow!("Invalid resolved sync configuration"))
            })
            .collect()
    }
}

fn retain_supplied_settings(value: &mut Value, supplied: &Value) {
    if let Value::Object(fields) = value {
        fields.retain(|key, value| {
            let Some(incoming) = supplied.get(key) else {
                return false;
            };
            retain_supplied_settings(value, incoming);
            !value.as_object().is_some_and(|fields| fields.is_empty())
        });
    }
}

fn credential_selected(id: &str, scope: &SyncScope, filter: &StructuredUploadItemFilter) -> bool {
    use oxideterm_connections::{CredentialOwner, CredentialTarget};
    let Ok(target) = serde_json::from_str::<CredentialTarget>(id) else {
        return false;
    };
    let within =
        |id: &str, ids: &Option<BTreeSet<String>>| ids.as_ref().is_none_or(|ids| ids.contains(id));
    match target.owner {
        CredentialOwner::Connection(id) => {
            scope.sync_connections && within(&id, &filter.connection_ids)
        }
        CredentialOwner::StandaloneSftp(_) | CredentialOwner::Ftp(_) => scope.sync_connections,
        CredentialOwner::Totp(_) => false,
        CredentialOwner::Mosh(id) => {
            scope.sync_mosh_profiles && within(&id, &filter.mosh_profile_ids)
        }
        CredentialOwner::Telnet(id) => {
            scope.sync_telnet_profiles && within(&id, &filter.telnet_profile_ids)
        }
        CredentialOwner::RemoteDesktop(id) => {
            scope.sync_remote_desktop_profiles && within(&id, &filter.remote_desktop_profile_ids)
        }
        CredentialOwner::GlobalProxy => {
            scope.sync_app_settings && scope.app_settings_sections.iter().any(|id| id == "network")
        }
    }
}

fn collect_totp_references(value: &Value, resources: &mut BTreeSet<SyncResource>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if matches!(key.as_str(), "managed_key_id" | "managedKeyId" | "key_id")
                    && let Some(id) = value.as_str()
                {
                    resources.insert(SyncResource {
                        kind: ResourceKind::ManagedKey,
                        id: id.into(),
                    });
                } else if key == "totp_credential_id"
                    && let Some(id) = value.as_str()
                {
                    resources.insert(SyncResource {
                        kind: ResourceKind::Totp,
                        id: id.into(),
                    });
                } else {
                    collect_totp_references(value, resources);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_totp_references(value, resources);
            }
        }
        _ => {}
    }
}

fn required_value<'a>(
    values: &'a SyncValues,
    resource: &SyncResource,
    group: &str,
) -> Result<&'a FieldValue> {
    values
        .get(&SyncField {
            resource: resource.clone(),
            group: group.into(),
        })
        .ok_or_else(|| anyhow::anyhow!("Incomplete resolved sync record"))
}

fn active_resources(values: &SyncValues, kind: ResourceKind) -> Result<Vec<SyncResource>> {
    values
        .iter()
        .filter(|(field, _)| field.resource.kind == kind && field.is_presence())
        .filter_map(|(field, value)| match value.decode::<bool>() {
            Ok(true) => Some(Ok(field.resource.clone())),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn assign_setting(target: &mut Value, path: &[String], value: Value) -> Result<()> {
    if path.is_empty() {
        *target = value;
        return Ok(());
    }
    let object = target
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Conflicting settings structure"))?;
    if path.len() == 1 {
        object.insert(path[0].clone(), value);
    } else {
        assign_setting(
            object
                .entry(path[0].clone())
                .or_insert_with(|| Value::Object(Map::new())),
            &path[1..],
            value,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_connections::{
        CLEARED_PROFILE_CREDENTIAL_KIND, CredentialOwner, CredentialSlot, CredentialTarget,
        PROFILE_CREDENTIAL_KIND,
    };

    #[test]
    fn theme_selections_survive_cloud_upgrade_and_section_selection() {
        let mut source = PersistedSettings::default();
        source.appearance.theme = "github-dark".into();
        source.terminal.theme = "monokai".into();
        let exported = export_oxide_settings_snapshot_json(&source, None, false).unwrap();
        for legacy in [false, true] {
            let mut envelope: Value = serde_json::from_str(&exported).unwrap();
            if legacy {
                envelope["settings"]["appearance"]
                    .as_object_mut()
                    .unwrap()
                    .remove("theme");
            }
            let mut view = ConfigurationView::empty();
            view.insert_app_settings(&envelope.to_string()).unwrap();
            for (sections, application, terminal) in [
                (
                    vec!["appearance"],
                    if legacy { "monokai" } else { "github-dark" },
                    "default",
                ),
                (vec!["terminalAppearance"], "solarized-light", "monokai"),
                (
                    vec!["appearance", "terminalAppearance"],
                    if legacy { "monokai" } else { "github-dark" },
                    "monokai",
                ),
            ] {
                let scope = SyncScope {
                    sync_app_settings: true,
                    app_settings_sections: sections.into_iter().map(str::to_owned).collect(),
                    ..Default::default()
                };
                let selected = view.selected(
                    std::iter::empty(),
                    &scope,
                    &StructuredUploadItemFilter::default(),
                );
                let values = view
                    .values
                    .iter()
                    .filter(|(field, _)| selected.contains(&field.resource))
                    .map(|(field, value)| (field.clone(), value.clone()))
                    .collect();
                let mut current = PersistedSettings::default();
                current.appearance.theme = "solarized-light".into();
                let restored = view.resolved_settings(&values, &current).unwrap();
                assert_eq!(
                    restored.appearance.theme, application,
                    "legacy={legacy}, sections={:?}",
                    scope.app_settings_sections
                );
                assert_eq!(
                    restored.terminal.theme, terminal,
                    "legacy={legacy}, sections={:?}",
                    scope.app_settings_sections
                );
            }
        }
        let terminal_only = export_oxide_settings_snapshot_json(
            &source,
            Some(&HashSet::from(["terminalAppearance".into()])),
            false,
        )
        .unwrap();
        let mut view = ConfigurationView::empty();
        view.insert_app_settings(&terminal_only).unwrap();
        let mut current = PersistedSettings::default();
        current.appearance.theme = "solarized-light".into();
        let restored = view.resolved_settings(&view.values, &current).unwrap();
        assert_eq!(restored.appearance.theme, "solarized-light");
        assert_eq!(restored.terminal.theme, "monokai");
    }

    #[test]
    fn scoped_credentials_share_a_register_for_set_and_clear() {
        let mut view = ConfigurationView {
            values: BTreeMap::new(),
            originals: BTreeMap::new(),
        };
        let target = |id: &str| {
            serde_json::to_string(&CredentialTarget {
                owner: CredentialOwner::Connection(id.into()),
                slot: CredentialSlot::Primary,
                identity: "endpoint-account".into(),
            })
            .unwrap()
        };
        let first = target("selected");
        let second = target("excluded");
        view.include_credentials(vec![EncryptedPortableSecret {
            kind: PROFILE_CREDENTIAL_KIND.into(),
            id: first.clone(),
            secret: Zeroizing::new("old-secret".into()),
        }])
        .unwrap();
        view.include_credentials(vec![
            EncryptedPortableSecret {
                kind: CLEARED_PROFILE_CREDENTIAL_KIND.into(),
                id: first.clone(),
                secret: Zeroizing::new(String::new()),
            },
            EncryptedPortableSecret {
                kind: PROFILE_CREDENTIAL_KIND.into(),
                id: second.clone(),
                secret: Zeroizing::new("excluded-secret".into()),
            },
        ])
        .unwrap();
        let resolved = view.resolved_credentials(&view.values).unwrap();
        let cleared = resolved.iter().find(|secret| secret.id == first).unwrap();
        assert_eq!(cleared.kind, CLEARED_PROFILE_CREDENTIAL_KIND);
        assert_eq!(cleared.secret.as_str(), "");
        assert_eq!(
            resolved.iter().filter(|secret| secret.id == first).count(),
            1
        );
        let filter = StructuredUploadItemFilter {
            connection_ids: Some(BTreeSet::from(["selected".into()])),
            ..Default::default()
        };
        let scope = SyncScope {
            sync_sensitive_credentials: true,
            ..Default::default()
        };
        assert_eq!(
            view.selected(std::iter::empty(), &scope, &filter),
            BTreeSet::from([SyncResource {
                kind: ResourceKind::Credential,
                id: first
            }])
        );
        assert_eq!(
            view.selected(
                std::iter::empty(),
                &SyncScope {
                    sync_sensitive_credentials: false,
                    ..scope
                },
                &filter
            ),
            BTreeSet::new()
        );
    }
}
