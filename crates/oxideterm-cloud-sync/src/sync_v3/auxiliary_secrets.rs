// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::{ConfigurationView, ResourceKind, SyncValues};
use crate::{SyncScope, operation::StructuredUploadItemFilter};
use anyhow::{Result, bail};
use oxideterm_ai::AiProviderKeyStore;
use oxideterm_connections::{
    ConnectionStore, ManagedSshKeySyncRecord, oxide_file::EncryptedPortableSecret,
};
use oxideterm_settings::SettingsStore;

pub(crate) fn collect(
    view: &mut ConfigurationView,
    connections: &ConnectionStore,
    settings: &SettingsStore,
    scope: &SyncScope,
    filter: &StructuredUploadItemFilter,
) -> Result<()> {
    if !scope.sync_sensitive_credentials {
        return Ok(());
    }
    let selected = view.selected(std::iter::empty(), scope, filter);
    let owners = selected
        .iter()
        .filter(|resource| resource.kind == ResourceKind::Connection)
        .map(|resource| resource.id.clone())
        .collect();
    for record in connections.export_privilege_sync_records(&owners)? {
        let id = serde_json::to_string(&(&record.metadata.connection_id, &record.metadata.id))?;
        view.insert_secret(ResourceKind::PrivilegeCredential, &id, &record)?;
    }
    let key_store = AiProviderKeyStore::new();
    if scope.sync_app_settings && scope.app_settings_sections.iter().any(|id| id == "ai")
        || scope.sync_plugin_settings
    {
        let accounts = key_store.sync_accounts()?;
        let mut selected_accounts = std::collections::BTreeMap::new();
        if scope.sync_app_settings && scope.app_settings_sections.iter().any(|id| id == "ai") {
            for provider in oxideterm_ai::provider_views(&settings.settings().ai.providers) {
                AiProviderKeyStore::validate_sync_account(&provider.id)?;
                if provider.id.starts_with("plugin-secret:") {
                    bail!("AI provider uses a reserved secret account");
                }
                selected_accounts.insert(provider.id, ResourceKind::AiCredential);
            }
        }
        for id in accounts.keys().filter(|id| plugin_selected(id, scope)) {
            selected_accounts.insert(id.clone(), ResourceKind::PluginCredential);
        }
        let ids = selected_accounts.keys().cloned().collect::<Vec<_>>();
        let mut secrets = key_store
            .get_provider_keys(&ids)?
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        for (id, kind) in selected_accounts {
            let value = secrets.remove(&id);
            if value.is_some() || accounts.get(&id) == Some(&true) {
                view.insert_secret(kind, &id, &value)?;
            }
        }
    }
    for resource in selected
        .iter()
        .filter(|resource| resource.kind == ResourceKind::ManagedKey)
    {
        if !connections
            .managed_ssh_keys()
            .iter()
            .any(|key| key.id == resource.id)
        {
            continue;
        }
        view.insert_secret(
            ResourceKind::ManagedKey,
            &resource.id,
            &connections.export_managed_key_for_sync(&resource.id)?,
        )?;
    }
    Ok(())
}

pub(crate) fn plugin_selected(account: &str, scope: &SyncScope) -> bool {
    let Some(rest) = account.strip_prefix("plugin-secret:") else {
        return false;
    };
    let Some((length, rest)) = rest.split_once(':') else {
        return false;
    };
    let Ok(length) = length.parse::<usize>() else {
        return false;
    };
    let Some(plugin_id) = rest.get(..length) else {
        return false;
    };
    let Some(rest) = rest.get(length..).and_then(|rest| rest.strip_prefix(':')) else {
        return false;
    };
    let Some((length, key)) = rest.split_once(':') else {
        return false;
    };
    if length.parse::<usize>().ok() != Some(key.len())
        || key.is_empty()
        || key.chars().any(char::is_control)
    {
        return false;
    }
    !plugin_id.is_empty()
        && !plugin_id.contains(['/', '\\'])
        && !plugin_id.contains("..")
        && plugin_id != crate::CLOUD_SYNC_PLUGIN_ID
        && scope.sync_plugin_settings
        && scope
            .plugin_ids
            .as_ref()
            .is_none_or(|ids| ids.iter().any(|id| id == plugin_id))
}

pub(crate) fn auxiliary_values(values: &SyncValues) -> Result<Vec<EncryptedPortableSecret>> {
    values
        .iter()
        .filter(|(field, _)| {
            field.group == "configuration"
                && matches!(
                    field.resource.kind,
                    ResourceKind::AiCredential | ResourceKind::PluginCredential
                )
        })
        .map(|(field, value)| {
            Ok(EncryptedPortableSecret {
                kind: match field.resource.kind {
                    ResourceKind::AiCredential => "ai_provider_key",
                    _ => "plugin_secret",
                }
                .into(),
                id: field.resource.id.clone(),
                secret: value
                    .decode::<Option<zeroize::Zeroizing<String>>>()?
                    .unwrap_or_default(),
            })
        })
        .collect()
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct AuxiliarySecretCheckpoint {
    secrets: Vec<EncryptedPortableSecret>,
    inventory: std::collections::BTreeMap<String, Option<bool>>,
}

pub(crate) fn checkpoint(secrets: &[EncryptedPortableSecret]) -> Result<AuxiliarySecretCheckpoint> {
    if secrets.is_empty() {
        return Ok(AuxiliarySecretCheckpoint::default());
    }
    let store = AiProviderKeyStore::new();
    let accounts = store.sync_accounts()?;
    let ids = secrets
        .iter()
        .map(|secret| secret.id.clone())
        .collect::<Vec<_>>();
    let mut values = store
        .get_provider_keys(&ids)?
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    Ok(AuxiliarySecretCheckpoint {
        secrets: secrets
            .iter()
            .map(|secret| EncryptedPortableSecret {
                kind: secret.kind.clone(),
                id: secret.id.clone(),
                secret: values.remove(&secret.id).unwrap_or_default(),
            })
            .collect(),
        inventory: secrets
            .iter()
            .map(|secret| (secret.id.clone(), accounts.get(&secret.id).copied()))
            .collect(),
    })
}

pub(crate) fn restore(previous: &AuxiliarySecretCheckpoint) -> Result<()> {
    if previous.secrets.is_empty() {
        return Ok(());
    }
    apply(&previous.secrets)?;
    AiProviderKeyStore::new().restore_sync_accounts(&previous.inventory)
}

pub(crate) fn apply(secrets: &[EncryptedPortableSecret]) -> Result<()> {
    let store = AiProviderKeyStore::new();
    for secret in secrets {
        if !matches!(secret.kind.as_str(), "ai_provider_key" | "plugin_secret") {
            bail!("Invalid auxiliary secret owner");
        }
        store.store_provider_key(&secret.id, secret.secret.clone())?;
    }
    Ok(())
}

pub(crate) fn privilege_values(
    values: &SyncValues,
) -> Result<Vec<oxideterm_connections::PrivilegeCredentialSyncRecord>> {
    values
        .iter()
        .filter(|(field, _)| {
            field.group == "configuration"
                && field.resource.kind == ResourceKind::PrivilegeCredential
        })
        .map(|(field, value)| {
            let record: oxideterm_connections::PrivilegeCredentialSyncRecord = value.decode()?;
            if serde_json::to_string(&(&record.metadata.connection_id, &record.metadata.id))?
                != field.resource.id
            {
                bail!("Invalid privilege credential identity");
            }
            Ok(record)
        })
        .collect()
}

pub(crate) fn managed_values(values: &SyncValues) -> Result<Vec<ManagedSshKeySyncRecord>> {
    values
        .iter()
        .filter(|(field, _)| {
            field.group == "configuration" && field.resource.kind == ResourceKind::ManagedKey
        })
        .map(|(field, value)| {
            let record: ManagedSshKeySyncRecord = value.decode()?;
            if record.metadata.id != field.resource.id {
                bail!("Invalid managed key identity");
            }
            Ok(record)
        })
        .collect()
}
