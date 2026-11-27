// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::{DiscoverySource, SettingsStore};
use anyhow::{Result, ensure};
use oxideterm_plugin_manifest::discovery::secret_key;
use std::collections::BTreeMap;
use zeroize::Zeroizing;

#[derive(Clone, PartialEq, Eq)]
pub enum DiscoverySecretBinding {
    Plugin(String),
    SshPassphrase(String),
}

impl DiscoverySource {
    pub fn secret_bindings(&self) -> BTreeMap<String, DiscoverySecretBinding> {
        let mut bindings: BTreeMap<_, _> = self
            .config
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(field, binding)| {
                Some((
                    field.clone(),
                    DiscoverySecretBinding::Plugin(secret_key(&self.id, field, binding)?),
                ))
            })
            .collect();
        if let Some(account) = self.ssh_preferences.passphrase_account(&self.id) {
            bindings.insert(
                crate::DISCOVERY_SSH_PASSPHRASE_FIELD.into(),
                DiscoverySecretBinding::SshPassphrase(account),
            );
        }
        bindings
    }
}

impl SettingsStore {
    /// Write fresh, unreachable revisions first so a failed settings commit leaves old credentials usable.
    pub fn save_discovery_source(
        &mut self,
        mut source: DiscoverySource,
        new_source: bool,
        replacements: Vec<(String, Zeroizing<String>)>,
        mut write: impl FnMut(&DiscoverySecretBinding, Zeroizing<String>) -> Result<()>,
        mut remove: impl FnMut(&DiscoverySecretBinding) -> Result<()>,
    ) -> Result<()> {
        let mut sources = self.settings().discovery_sources.clone();
        let old = sources.iter_mut().find(|s| s.id == source.id);
        ensure!(
            old.is_none() == new_source,
            "Source was removed or replaced"
        );
        let previous = old
            .as_ref()
            .map(|s| s.secret_bindings())
            .unwrap_or_default();
        let current = source.secret_bindings();
        let mut pending = Vec::new();
        for (field, secret) in replacements {
            let key = current
                .get(&field)
                .ok_or_else(|| anyhow::anyhow!("Invalid secret field"))?;
            ensure!(
                previous.get(&field) != Some(key),
                "Secret replacement requires a fresh revision"
            );
            pending.push((key.clone(), secret));
        }
        ensure!(
            current
                .iter()
                .all(|(field, key)| previous.get(field) == Some(key)
                    || pending.iter().any(|(new, _)| new == key)),
            "Saved credential changed"
        );
        if let Some(old) = old {
            ensure!(
                !old.removal_pending
                    && old.plugin_id == source.plugin_id
                    && old.provider_id == source.provider_id,
                "Source provider changed"
            );
            // Enablement belongs to the source list and may have changed since editing began.
            source.enabled = old.enabled;
            *old = source;
        } else {
            source.enabled = true;
            sources.push(source);
        }
        let mut staged = Vec::new();
        let result = (|| {
            for (key, secret) in pending {
                staged.push(key);
                write(staged.last().unwrap(), secret)?;
            }
            self.replace_discovery_sources(sources)?;
            Ok(())
        })();
        if let Err(error) = result {
            for key in staged {
                remove(&key)?;
            }
            return Err(error);
        }
        for (field, key) in previous {
            if current.get(&field) != Some(&key) {
                remove(&key)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;

    #[test]
    fn discovery_source_list_owns_enablement_across_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut store = SettingsStore::load_from_path(&path).unwrap();
        let mut draft = DiscoverySource {
            id: "inventory".into(),
            name: "Initial".into(),
            enabled: false,
            config: json!({"site": "example"}),
            ..Default::default()
        };
        let save = |store: &mut SettingsStore, source, new| {
            store.save_discovery_source(source, new, vec![], |_, _| unreachable!(), |_| unreachable!()).unwrap();
        };
        save(&mut store, draft.clone(), true);
        let read = || SettingsStore::load_from_path(&path).unwrap().settings().discovery_sources[0].clone();
        draft = read();
        assert!(draft.enabled);
        let mut list = SettingsStore::load_from_path(&path).unwrap();
        list.set_discovery_source_enabled("inventory", false).unwrap();
        draft.name = "Updated".into();
        store = SettingsStore::load_from_path(&path).unwrap();
        save(&mut store, draft.clone(), false);
        draft.enabled = false;
        assert_eq!(read(), draft);
        list.set_discovery_source_enabled("inventory", true).unwrap();
        draft.enabled = true;
        assert_eq!(read(), draft);
    }

    #[test]
    fn discovery_switching_from_a_key_to_agent_removes_only_the_host_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = SettingsStore::load_from_path(dir.path().join("settings.json")).unwrap();
        let mut source = DiscoverySource {
            id: "inventory".into(),
            plugin_id: Some("inventory-plugin".into()),
            provider_id: "assets".into(),
            name: "Inventory".into(),
            config: json!({"root/token": {"scope": {}, "discoverySecret": "aaaa1111-0000-4000-8000-000000000000"}}),
            ..Default::default()
        };
        source.ssh_preferences.auth = crate::DiscoverySshAuth::KeyFile("fixture/key".into());
        source.ssh_preferences.passphrase_revision = Some(uuid::Uuid::from_u128(7));
        let keys = RefCell::new(BTreeMap::new());
        let account = |binding: &DiscoverySecretBinding| match binding {
            DiscoverySecretBinding::Plugin(id) => ("plugin", id.clone()),
            DiscoverySecretBinding::SshPassphrase(id) => ("ssh", id.clone()),
        };
        let write = |binding: &DiscoverySecretBinding, value| {
            keys.borrow_mut().insert(account(binding), value);
            Ok(())
        };
        let remove = |binding: &DiscoverySecretBinding| {
            keys.borrow_mut().remove(&account(binding));
            Ok(())
        };
        settings
            .save_discovery_source(
                source.clone(),
                true,
                vec![
                    (
                        "root/token".into(),
                        Zeroizing::new("fixture-api-token".into()),
                    ),
                    (
                        crate::DISCOVERY_SSH_PASSPHRASE_FIELD.into(),
                        Zeroizing::new("fixture-unlock".into()),
                    ),
                ],
                write,
                remove,
            )
            .unwrap();
        let key = (
            "ssh",
            "discovery-ssh:9:inventory:00000000-0000-0000-0000-000000000007".to_owned(),
        );
        assert_eq!(keys.borrow()[&key].as_str(), "fixture-unlock");
        source.ssh_preferences.auth = crate::DiscoverySshAuth::Agent;
        source.ssh_preferences.passphrase_revision = None;
        settings
            .save_discovery_source(source, false, vec![], write, remove)
            .unwrap();
        assert_eq!(
            keys.borrow()
                .iter()
                .map(|((owner, _), value)| (*owner, value.as_str()))
                .collect::<Vec<_>>(),
            [("plugin", "fixture-api-token")]
        );
    }

    #[test]
    fn discovery_failed_credential_rotation_keeps_the_committed_reference_and_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = SettingsStore::load_from_path(&path).unwrap();
        let source = DiscoverySource {
            id: "source".into(),
            plugin_id: Some("inventory".into()),
            provider_id: "hosts".into(),
            name: "Inventory".into(),
            config: json!({"root/key":{
                "scope":{}, "discoverySecret":"aaaa1111-0000-4000-8000-000000000000"}}),
            ..Default::default()
        };
        let keys = RefCell::new(BTreeMap::new());
        let write = |binding: &DiscoverySecretBinding, secret| {
            let DiscoverySecretBinding::Plugin(key) = binding else {
                panic!("unexpected host credential")
            };
            keys.borrow_mut().insert(key.to_owned(), secret);
            Ok(())
        };
        let remove = |binding: &DiscoverySecretBinding| {
            let DiscoverySecretBinding::Plugin(key) = binding else {
                panic!("unexpected host credential")
            };
            keys.borrow_mut().remove(key);
            Ok(())
        };
        let input = |value: &str| vec![("root/key".into(), Zeroizing::new(value.to_owned()))];
        settings
            .save_discovery_source(
                source.clone(),
                true,
                input("fixture-original"),
                write,
                remove,
            )
            .unwrap();
        let mut updated = source.clone();
        updated.config["root/key"]["discoverySecret"] =
            json!("bbbb2222-0000-4000-8000-000000000000");
        settings.writes_blocked = true;
        assert!(
            settings
                .save_discovery_source(updated.clone(), false, input("fixture-new"), write, remove)
                .is_err()
        );
        assert_eq!(
            SettingsStore::load_from_path(&path)
                .unwrap()
                .settings()
                .discovery_sources,
            [source]
        );
        assert_eq!(
            keys.borrow()
                .values()
                .map(|v| v.as_str())
                .collect::<Vec<_>>(),
            ["fixture-original"]
        );
        settings.writes_blocked = false;
        settings
            .save_discovery_source(updated.clone(), false, input("fixture-new"), write, remove)
            .unwrap();
        assert_eq!(
            keys.borrow()
                .values()
                .map(|v| v.as_str())
                .collect::<Vec<_>>(),
            ["fixture-new"]
        );
        assert_eq!(
            SettingsStore::load_from_path(&path)
                .unwrap()
                .settings()
                .discovery_sources,
            [updated]
        );
        assert!(!std::fs::read_to_string(path).unwrap().contains("fixture-"));
    }
}
