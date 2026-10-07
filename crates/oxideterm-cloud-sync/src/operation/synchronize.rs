// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::backend::sha256_hex;
use crate::secrets::CloudSyncSecrets;
use crate::sync_v3::{
    ConfigurationView, LocalReplica, PublicationId, PublishedReplica, RecoveryJournal,
    ReplicaStore, SyncConflict, SyncReplica, SyncResource, SyncValues,
};
use std::collections::HashSet;
mod password_change;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    #[serde(skip)]
    pub switched_settings: Option<CloudSyncSettings>,
    pub applied: bool,
    pub published: bool,
    pub publication_pending: bool,
    pub recovered: bool,
    pub cleanup_pending: bool,
    pub publication: Option<PublicationId>,
    pub created_remote_id: Option<String>,
    #[serde(skip)]
    pub local_snapshot: CloudSyncLocalSnapshot,
    pub conflicts: Vec<SyncConflict>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncPlanSummary {
    pub changed_fields: usize,
    pub conflicts: Vec<SyncConflict>,
    pub upgrading: bool,
}

#[derive(Clone)]
pub struct SyncConflictPreview {
    pub name: Option<String>,
    pub candidates: Vec<Option<zeroize::Zeroizing<String>>>,
    pub current: Option<usize>,
}

/// Preparation holds the replica lock without publishing remote data. Callers
/// recover and apply changes on the live configuration owner.
pub struct PreparedSync {
    _permit: CloudSyncOperationPermit,
    store: ReplicaStore,
    local: LocalReplica,
    remote: Vec<PublishedReplica>,
    observed: Vec<PublicationId>,
    upgrade: Option<crate::sync_v3::ReplicaSnapshot>,
    backend: CloudSyncBackend,
    settings: CloudSyncSettings,
    secrets: CloudSyncSecrets,
    scope: crate::SyncScope,
    filter: StructuredUploadItemFilter,
    recovered: bool,
    resolutions: Vec<(SyncConflict, usize)>,
}

impl std::fmt::Debug for PreparedSync {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSync").finish_non_exhaustive()
    }
}

pub struct AppliedSync {
    _permit: CloudSyncOperationPermit,
    store: ReplicaStore,
    local: LocalReplica,
    backend: CloudSyncBackend,
    settings: CloudSyncSettings,
    secrets: CloudSyncSecrets,
    outcome: SyncOutcome,
}

impl CloudSyncOperationService {
    pub fn has_local_replica(
        settings_path: &std::path::Path,
        settings: &CloudSyncSettings,
    ) -> Result<bool> {
        Ok(replica_directory(settings_path, settings)?
            .join("replica.oxide")
            .try_exists()?)
    }
    pub async fn prepare_sync(
        &self,
        connections: &mut ConnectionStore,
        forwards: &ForwardingRegistry,
        settings_store: &mut SettingsStore,
        settings: &CloudSyncSettings,
        provider: &mut impl CloudSyncSecretProvider,
        scope: crate::SyncScope,
        filter: StructuredUploadItemFilter,
    ) -> Result<PreparedSync> {
        if settings.local_file_mode {
            bail!(
                "Cloud synchronization requires a remote backend; use .oxide import/export in local-file mode"
            );
        }
        let permit = self
            .guard
            .begin(CloudSyncOperationKind::Upload, false)?
            .ok_or_else(|| anyhow::anyhow!("Cloud sync is already running"))?;
        let store = open_replica(settings_store.path(), settings, provider)?;
        if RecoveryJournal::has_pending(settings_store.path())? {
            bail!(
                "Cloud sync recovery must complete on the configuration owner before preparing a new operation"
            );
        }
        let recovered = false;
        let mut local = store.load()?.unwrap_or_default();
        let mut secrets = get_action_secrets(settings, provider, true, SecretReadMode::Prompt)?;
        self.prepare_action_secrets(settings, provider, &mut secrets)
            .await?;
        let password =
            required_sync_password(secrets.sync_password.as_deref().map(String::as_str))?;
        store.bind_sync_password(&mut local, password)?;
        let mut settings = settings.clone();
        if settings.backend_type == BackendType::GithubGist
            && settings.git_repository.trim().is_empty()
        {
            if let Some(id) = &local.created_gist_id {
                settings.git_repository = id.clone();
            }
        }
        let view = collect(connections, forwards, settings_store, &scope, &filter)?;
        let mut replica = SyncReplica::load(local.snapshot.clone(), local.writer)?;
        let selected = view.selected(
            replica
                .resources()?
                .into_iter()
                .chain(local.effective.keys().map(|field| field.resource.clone())),
            &scope,
            &filter,
        );
        replica.capture_local(
            &capture_baseline(&local.effective, &scope),
            &view.values,
            &selected,
        )?;
        local.snapshot = replica.snapshot();
        update_effective(&mut local.effective, &view.values, &selected, &scope);
        store.save(&local)?;

        let new_gist = settings.backend_type == BackendType::GithubGist
            && settings.git_repository.trim().is_empty();
        let publications = if new_gist {
            Vec::new()
        } else {
            self.backend.list_publications(&settings, &secrets).await?
        };
        let upgrade = if !new_gist && publications.is_empty() && !local.legacy_imported {
            let metadata = self
                .backend
                .fetch_remote_metadata(&settings, &secrets)
                .await?;
            if metadata.exists {
                Some(
                    self.read_upgrade_snapshot(connections, &settings, &secrets, metadata, &scope)
                        .await?,
                )
            } else {
                None
            }
        } else {
            None
        };
        let latest = latest_publications(publications)?;
        if local.observed.contains_key(&local.writer)
            && !latest.iter().any(|id| id.writer == local.writer)
        {
            local.request_publication();
            store.save(&local)?;
        }
        let password =
            required_sync_password(secrets.sync_password.as_deref().map(String::as_str))?;
        let mut decryption = OxideBatchDecryptionContext::new(password)?;
        let mut remote = Vec::new();
        let mut observed = Vec::new();
        for id in latest {
            if let Some(previous) = local.observed.get(&id.writer) {
                if previous == &id {
                    continue;
                }
                if previous.sequence >= id.sequence {
                    bail!("Cloud sync publication history moved backwards or changed identity");
                }
            }
            if id.writer == local.writer && id.sequence >= local.next_sequence {
                bail!(
                    "Cloud sync writer identity is already used by another copy of this data directory"
                );
            }
            let object = self
                .backend
                .read_remote_object(&settings, &secrets, &id.path())
                .await?
                .context("Published cloud sync snapshot is missing")?;
            remote.push(PublishedReplica::decode(
                &id,
                &object.bytes,
                &mut decryption,
            )?);
            observed.push(id);
        }
        Ok(PreparedSync {
            _permit: permit,
            store,
            local,
            remote,
            upgrade,
            observed,
            backend: self.backend.clone(),
            settings,
            secrets,
            scope,
            filter,
            recovered,
            resolutions: Vec::new(),
        })
    }

    pub async fn synchronize(
        &self,
        connections: &mut ConnectionStore,
        forwards: &ForwardingRegistry,
        settings_store: &mut SettingsStore,
        settings: &CloudSyncSettings,
        provider: &mut impl CloudSyncSecretProvider,
        scope: crate::SyncScope,
        filter: StructuredUploadItemFilter,
    ) -> Result<SyncOutcome> {
        self.prepare_sync(
            connections,
            forwards,
            settings_store,
            settings,
            provider,
            scope,
            filter,
        )
        .await?
        .apply(connections, forwards, settings_store)?
        .publish()
        .await
    }
}

impl PreparedSync {
    pub fn matches_settings(&self, settings: &CloudSyncSettings) -> bool {
        let mut expected = settings.clone();
        if expected.backend_type == BackendType::GithubGist
            && expected.git_repository.trim().is_empty()
        {
            if let Some(id) = &self.local.created_gist_id {
                expected.git_repository = id.clone();
            }
        }
        self.settings == expected
    }
    pub fn conflict_previews(&self) -> Result<Vec<SyncConflictPreview>> {
        use crate::sync_v3::{ResourceKind, SyncField};
        let mut replica = SyncReplica::load(self.local.snapshot.clone(), self.local.writer)?;
        for remote in &self.remote {
            replica.merge(remote.snapshot.clone())?;
        }
        if let Some(upgrade) = &self.upgrade {
            replica.merge(upgrade.clone())?;
        }
        self.summary()?
            .conflicts
            .into_iter()
            .map(|conflict| {
                let name = self
                    .local
                    .effective
                    .get(&SyncField {
                        resource: conflict.field.resource.clone(),
                        group: "name".into(),
                    })
                    .and_then(|value| value.decode::<String>().ok());
                let current = conflict.candidates.iter().position(|candidate| {
                    replica.candidate_value(candidate) == self.local.effective.get(&conflict.field)
                });
                let sensitive = matches!(
                    conflict.field.resource.kind,
                    ResourceKind::Credential
                        | ResourceKind::ManagedKey
                        | ResourceKind::AiCredential
                        | ResourceKind::PluginCredential
                        | ResourceKind::PrivilegeCredential
                        | ResourceKind::PluginSettings
                ) || conflict.field.group.contains("customEnvVars");
                let candidates = conflict
                    .candidates
                    .iter()
                    .map(|candidate| {
                        if sensitive {
                            return Ok(None);
                        }
                        let Some(value) = replica.candidate_value(candidate) else {
                            return Ok(Some(zeroize::Zeroizing::new(String::new())));
                        };
                        let owned = crate::sync_v3::PrivateJson(value.decode()?);
                        let value = &owned.0;
                        let text = if let Some(value) = value.as_str() {
                            value.to_owned()
                        } else if let Some(object) = value.as_object() {
                            [
                                "host",
                                "username",
                                "port",
                                "port_path",
                                "baud_rate",
                                "cwd",
                                "shell_id",
                                "body",
                            ]
                            .iter()
                            .filter_map(|key| object.get(*key))
                            .map(|value| {
                                value
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| value.to_string())
                            })
                            .collect::<Vec<_>>()
                            .join(" · ")
                        } else {
                            value.to_string()
                        };
                        let text = zeroize::Zeroizing::new(text);
                        Ok(Some(zeroize::Zeroizing::new(
                            text.chars().take(400).collect(),
                        )))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(SyncConflictPreview {
                    name,
                    candidates,
                    current,
                })
            })
            .collect()
    }
    pub fn summary(&self) -> Result<SyncPlanSummary> {
        let mut replica = SyncReplica::load(self.local.snapshot.clone(), self.local.writer)?;
        for remote in &self.remote {
            replica.merge(remote.snapshot.clone())?;
        }
        if let Some(upgrade) = &self.upgrade {
            replica.merge(upgrade.clone())?;
        }
        let resources = replica.resources()?;
        let mut view = ConfigurationView::empty();
        view.values = replica.materialize(&self.local.effective, &resources)?;
        let selected = view.selected(resources.into_iter(), &self.scope, &self.filter);
        let conflicts = replica
            .conflicts()?
            .into_iter()
            .filter(|conflict| {
                selected.contains(&conflict.field.resource)
                    && field_enabled(&conflict.field, &self.scope)
            })
            .collect();
        let effective = replica.materialize(&self.local.effective, &selected)?;
        let changed_fields = effective
            .keys()
            .chain(self.local.effective.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|field| {
                selected.contains(&field.resource)
                    && field_enabled(field, &self.scope)
                    && effective.get(*field) != self.local.effective.get(*field)
            })
            .count();
        Ok(SyncPlanSummary {
            changed_fields,
            conflicts,
            upgrading: self.upgrade.is_some(),
        })
    }

    pub fn choose_candidate(&mut self, conflict: SyncConflict, index: usize) -> Result<()> {
        if index >= conflict.candidates.len() {
            bail!("Invalid conflict candidate");
        }
        self.resolutions
            .retain(|(previous, _)| previous.field != conflict.field);
        self.resolutions.push((conflict, index));
        Ok(())
    }

    pub fn choose_local_conflicts(&mut self) -> Result<()> {
        let mut replica = SyncReplica::load(self.local.snapshot.clone(), self.local.writer)?;
        for remote in &self.remote {
            replica.merge(remote.snapshot.clone())?;
        }
        if let Some(upgrade) = &self.upgrade {
            replica.merge(upgrade.clone())?;
        }
        for conflict in self.summary()?.conflicts {
            let local = self.local.effective.get(&conflict.field);
            if let Some(index) = conflict
                .candidates
                .iter()
                .position(|candidate| replica.candidate_value(candidate) == local)
            {
                self.choose_candidate(conflict, index)?;
            }
        }
        Ok(())
    }
    pub fn apply(
        mut self,
        connections: &mut ConnectionStore,
        forwards: &ForwardingRegistry,
        settings_store: &mut SettingsStore,
    ) -> Result<AppliedSync> {
        // Re-read values from the live owner after network work, before observing
        // remote history. These edits must remain concurrent with remote edits.
        let current = collect(
            connections,
            forwards,
            settings_store,
            &self.scope,
            &self.filter,
        )?;
        let mut replica = SyncReplica::load(self.local.snapshot.clone(), self.local.writer)?;
        let selected = current.selected(
            replica.resources()?.into_iter().chain(
                self.local
                    .effective
                    .keys()
                    .map(|field| field.resource.clone()),
            ),
            &self.scope,
            &self.filter,
        );
        replica.capture_local(
            &capture_baseline(&self.local.effective, &self.scope),
            &current.values,
            &selected,
        )?;
        self.local.snapshot = replica.snapshot();
        update_effective(
            &mut self.local.effective,
            &current.values,
            &selected,
            &self.scope,
        );
        self.store.save(&self.local)?;
        for remote in self.remote {
            replica.merge(remote.snapshot)?;
        }
        let upgraded = self.upgrade.is_some();
        if let Some(upgrade) = self.upgrade {
            replica.merge(upgrade)?;
        }
        for (conflict, index) in self.resolutions {
            let choice = replica
                .candidate_value(&conflict.candidates[index])
                .cloned();
            replica.resolve(&conflict, choice.as_ref())?;
        }
        let resources = replica.resources()?;
        let merged = replica.materialize(&current.values, &resources)?;
        let selected =
            current.select_merged(merged, resources.into_iter(), &self.scope, &self.filter);
        let mut desired = replica.materialize(&current.values, &selected)?;
        desired.retain(|field, _| field_enabled(field, &self.scope));
        let changed = desired != current.values;
        let conflicts = replica
            .conflicts()?
            .into_iter()
            .filter(|conflict| {
                selected.contains(&conflict.field.resource)
                    && field_enabled(&conflict.field, &self.scope)
            })
            .collect();
        let mut next = self.local.clone();
        next.legacy_imported |= upgraded;
        next.observed
            .extend(self.observed.into_iter().map(|id| (id.writer, id)));
        next.snapshot = replica.snapshot();
        update_effective(&mut next.effective, &desired, &selected, &self.scope);
        let mut cleanup_pending = false;
        if changed {
            // All decoding and domain-independent validation precede checkpointing.
            let configuration = current.resolved_connections(&desired)?;
            let owners = configuration
                .connections
                .records
                .iter()
                .filter(|record| !record.deleted)
                .map(|record| record.id.clone())
                .collect::<HashSet<_>>();
            let forward_records = current.resolved_forwards(&desired)?;
            if forward_records.iter().any(|record| {
                record
                    .owner_connection_id
                    .as_ref()
                    .is_none_or(|id| !owners.contains(id))
            }) {
                bail!("A synchronized forward references an unavailable connection");
            }
            let commands = current.resolved_commands(&desired)?;
            let settings = current.resolved_settings(&desired, settings_store.settings())?;
            let plugins = current.resolved_plugins(&desired)?;
            let mut credentials = current.resolved_credentials(&desired)?;
            let secret_changes = desired
                .iter()
                .filter(|(field, value)| current.values.get(*field) != Some(*value))
                .map(|(field, value)| (field.clone(), value.clone()))
                .collect();
            let auxiliary = crate::sync_v3::auxiliary_secrets::auxiliary_values(&secret_changes)?;
            let managed = crate::sync_v3::auxiliary_secrets::managed_values(&secret_changes)?;
            let privileges = crate::sync_v3::auxiliary_secrets::privilege_values(&desired)?;
            let providers = oxideterm_ai::provider_views(&settings.ai.providers);
            if auxiliary.iter().any(|secret| {
                secret.kind == "ai_provider_key"
                    && (secret.id.starts_with("plugin-secret:")
                        || !providers.iter().any(|provider| provider.id == secret.id))
            }) {
                bail!("Synchronized AI key has no matching provider");
            }
            if credentials
                .iter()
                .any(|secret| !oxideterm_connections::is_profile_credential(secret))
            {
                bail!(
                    "A synchronized credential requires its application owner before it can be applied"
                );
            }
            let mut journal = RecoveryJournal::begin(
                &self.store,
                &self.local,
                connections,
                forwards,
                settings_store,
            )?;
            let audit = oxideterm_audit::AuditOperation::begin(
                oxideterm_audit::AuditCategory::Configuration,
                "cloud_sync_apply",
                None,
                Some("causal"),
            );
            let result = (|| -> Result<()> {
                journal.checkpoint_auxiliary(&self.store, &auxiliary)?;
                for key in managed {
                    if let Some(old) = connections.prepare_managed_key_sync(key, |reference| {
                        journal.record_managed_creation(&self.store, reference)
                    })? {
                        journal.defer_managed_cleanup(old);
                    }
                }
                let prepared = connections.prepare_resolved_configuration(configuration)?;
                let cleanup = connections.commit_prepared_saved_connections_snapshot(prepared)?;
                forwards.replace_resolved_forwards(forward_records, &owners)?;
                oxideterm_quick_commands::save_snapshot(settings_store.path(), &commands)
                    .map_err(anyhow::Error::msg)?;
                settings_store.replace_and_save(settings)?;
                crate::plugin_settings::replace_resolved_plugin_settings(
                    settings_store.path(),
                    plugins,
                )
                .map_err(anyhow::Error::msg)?;
                let credential_cleanup = if self.scope.sync_sensitive_credentials {
                    let selection = crate::credentials::credential_selection(
                        connections,
                        &self.scope,
                        &self.filter,
                    );
                    let stale = connections.prepare_privilege_sync(
                        privileges,
                        &selection.connection_ids,
                        |reference| journal.record_privilege_creation(&self.store, reference),
                    )?;
                    journal.defer_privilege_cleanup(stale);
                    let mut proxy = crate::credentials::global_proxy(settings_store.settings());
                    let prepared = connections.prepare_profile_credentials_with_journal(
                        &credentials,
                        &selection,
                        &mut proxy,
                        |reference| journal.record_credential_creation(&self.store, reference),
                    )?;
                    let mut settings = settings_store.settings().clone();
                    crate::credentials::apply_global_proxy_reference(&mut settings, proxy.as_ref());
                    settings_store.replace_and_save(settings)?;
                    connections.save()?;
                    Some(prepared)
                } else {
                    None
                };
                crate::sync_v3::auxiliary_secrets::apply(&auxiliary)?;
                journal.commit(&self.store, &next, Some(cleanup), credential_cleanup)?;
                Ok(())
            })();
            audit.result(&result);
            for secret in &mut credentials {
                zeroize::Zeroize::zeroize(&mut secret.secret);
            }
            if let Err(error) = result {
                RecoveryJournal::recover(&self.store, connections, forwards, settings_store)
                    .context("Cloud sync apply failed and recovery remains pending")?;
                return Err(error);
            }
            cleanup_pending =
                RecoveryJournal::recover(&self.store, connections, forwards, settings_store)
                    .is_err();
        } else {
            self.store.save(&next)?;
        }
        let mut local_snapshot =
            build_local_snapshot(connections, forwards, settings_store, None, None)?;
        local_snapshot.scope = self.scope.clone();
        local_snapshot.dirty.current_state =
            crate::build_structured_local_state(&local_snapshot.metadata, &self.scope);
        let publication_pending = next.needs_publication();
        Ok(AppliedSync {
            _permit: self._permit,
            store: self.store,
            local: next,
            backend: self.backend,
            settings: self.settings,
            secrets: self.secrets,
            outcome: SyncOutcome {
                switched_settings: None,
                applied: changed,
                published: false,
                publication_pending,
                recovered: self.recovered,
                cleanup_pending,
                publication: None,
                created_remote_id: None,
                local_snapshot,
                conflicts,
            },
        })
    }
}

impl AppliedSync {
    pub async fn publish(mut self) -> Result<SyncOutcome> {
        self.outcome.publication = self.local.observed.get(&self.local.writer).cloned();
        if self.outcome.cleanup_pending {
            return Ok(self.outcome);
        }
        if self.settings.backend_type == BackendType::GithubGist {
            if self.settings.git_repository.trim().is_empty() {
                let id = self
                    .backend
                    .create_github_gist(&self.settings, &self.secrets)
                    .await?;
                // Persist the destination before uploading any configuration. A
                // retry after a failed upload must reuse this same private Gist.
                self.local.created_gist_id = Some(id.clone());
                self.store.save(&self.local)?;
                self.settings.git_repository = id;
            }
            self.outcome.created_remote_id = self.local.created_gist_id.clone();
        }
        while self.local.needs_publication() {
            let password =
                required_sync_password(self.secrets.sync_password.as_deref().map(String::as_str))?;
            let context = OxideBatchEncryptionContext::new(password)?;
            self.store.stage_publication(&mut self.local, &context)?;
            let pending = self
                .local
                .pending
                .clone()
                .context("Cloud sync publication was not staged")?;
            self.backend
                .publish_replica(&self.settings, &self.secrets, &pending)
                .await?;
            self.store
                .confirm_publication(&mut self.local, &pending.path)?;
            self.outcome.published = true;
            self.outcome.publication = Some(PublicationId::parse(&pending.path)?);
        }
        for id in self
            .local
            .superseded_publications(chrono::Utc::now().timestamp_millis())
        {
            if self
                .backend
                .delete_publication(&self.settings, &self.secrets, &id)
                .await
                .is_err()
            {
                self.outcome.cleanup_pending = true;
                break;
            }
            self.store.confirm_removal(&mut self.local, &id)?;
        }
        self.outcome.publication_pending = self.local.needs_publication();
        Ok(self.outcome)
    }
}

fn replica_directory(
    settings_path: &std::path::Path,
    settings: &CloudSyncSettings,
) -> Result<std::path::PathBuf> {
    let target = serde_json::to_vec(&(
        &settings.backend_type,
        &settings.endpoint,
        &settings.namespace,
        &settings.git_repository,
        &settings.git_branch,
        &settings.s3_bucket,
        &settings.s3_region,
    ))?;
    Ok(settings_path
        .parent()
        .context("Settings directory is unavailable")?
        .join("cloud-sync-v3")
        .join(sha256_hex(&target).replace(':', "-")))
}

fn open_replica(
    settings_path: &std::path::Path,
    settings: &CloudSyncSettings,
    provider: &mut impl CloudSyncSecretProvider,
) -> Result<ReplicaStore> {
    let directory = replica_directory(settings_path, settings)?;
    if settings.backend_type == BackendType::GithubGist
        && !settings.git_repository.trim().is_empty()
    {
        let mut initial = settings.clone();
        initial.git_repository.clear();
        let bootstrap = replica_directory(settings_path, &initial)?;
        if bootstrap.join("replica.oxide").try_exists()? {
            let store = ReplicaStore::open(&bootstrap, provider)?;
            if store
                .load()?
                .and_then(|local| local.created_gist_id)
                .as_deref()
                == Some(settings.git_repository.as_str())
            {
                return Ok(store);
            }
        }
    }
    ReplicaStore::open(&directory, provider)
}

fn collect(
    connections: &ConnectionStore,
    forwards: &ForwardingRegistry,
    settings: &SettingsStore,
    scope: &crate::SyncScope,
    filter: &StructuredUploadItemFilter,
) -> Result<ConfigurationView> {
    let mut view = ConfigurationView::collect(connections, forwards, settings, scope)?;
    if scope.sync_sensitive_credentials {
        view.include_credentials(connections.export_sync_credentials(
            &crate::credentials::credential_selection(connections, scope, filter),
            crate::credentials::global_proxy(settings.settings()).as_ref(),
        )?)?;
        crate::sync_v3::auxiliary_secrets::collect(
            &mut view,
            connections,
            settings,
            scope,
            filter,
        )?;
    }
    Ok(view)
}

fn update_effective(
    effective: &mut SyncValues,
    current: &SyncValues,
    selected: &BTreeSet<SyncResource>,
    scope: &crate::SyncScope,
) {
    effective
        .retain(|field, _| !selected.contains(&field.resource) || !field_enabled(field, scope));
    effective.extend(
        current
            .iter()
            .filter(|(field, _)| selected.contains(&field.resource))
            .map(|(field, value)| (field.clone(), value.clone())),
    );
}

fn field_enabled(field: &crate::sync_v3::SyncField, scope: &crate::SyncScope) -> bool {
    if scope.include_local_terminal_env_vars
        || field.resource.kind != crate::sync_v3::ResourceKind::AppSettings
    {
        return true;
    }
    !field
        .group
        .strip_prefix("setting:")
        .and_then(|path| serde_json::from_str::<Vec<String>>(path).ok())
        .is_some_and(|path| {
            path.len() >= 2 && path[0] == "localTerminal" && path[1] == "customEnvVars"
        })
}

fn capture_baseline(effective: &SyncValues, scope: &crate::SyncScope) -> SyncValues {
    effective
        .iter()
        .filter(|(field, _)| field_enabled(field, scope))
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect()
}

fn latest_publications(publications: Vec<PublicationId>) -> Result<Vec<PublicationId>> {
    let mut latest = std::collections::BTreeMap::<uuid::Uuid, PublicationId>::new();
    for publication in publications {
        if let Some(previous) = latest.get(&publication.writer) {
            if previous.sequence == publication.sequence && previous.digest != publication.digest {
                bail!("A cloud sync writer published conflicting sequence identities");
            }
            if previous.sequence > publication.sequence {
                continue;
            }
        }
        latest.insert(publication.writer, publication);
    }
    Ok(latest.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync_v3::{FieldValue, ResourceKind, SyncField};
    use oxideterm_connections::SaveLocalTerminalProfileRequest;

    #[test]
    fn created_gist_keeps_pending_history_when_configuration_gains_its_id() {
        let directory =
            std::env::temp_dir().join(format!("oxide-gist-bootstrap-{}", uuid::Uuid::new_v4()));
        let settings_path = directory.join("settings.json");
        let mut provider = crate::sync_v3::tests::Secrets::default();
        let mut config = CloudSyncSettings {
            backend_type: BackendType::GithubGist,
            namespace: "personal".into(),
            ..Default::default()
        };
        let store = open_replica(&settings_path, &config, &mut provider).unwrap();
        let mut local = LocalReplica::new();
        let writer = local.writer;
        local.created_gist_id = Some("created-gist".into());
        local.next_sequence = 7;
        let path = format!("sync-v3/{writer}/6-{:x}.oxide", Sha256::digest([4, 2, 7]));
        local.pending = Some(crate::sync_v3::PendingPublication {
            path: path.clone(),
            bytes: vec![4, 2, 7],
        });
        store.save(&local).unwrap();
        drop(store);
        config.git_repository = "created-gist".into();
        let store = open_replica(&settings_path, &config, &mut provider).unwrap();
        let reopened = store.load().unwrap().unwrap();
        assert_eq!(reopened.writer, writer);
        assert_eq!(reopened.next_sequence, 7);
        let pending = reopened.pending.unwrap();
        assert_eq!(
            (pending.path.as_str(), pending.bytes.as_slice()),
            (path.as_str(), [4, 2, 7].as_slice())
        );
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn apply_merges_live_owner_edits_and_preserves_excluded_settings() {
        let directory =
            std::env::temp_dir().join(format!("oxide-sync-apply-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut connections = ConnectionStore::load(directory.join("connections.json")).unwrap();
        connections
            .upsert_local_terminal_profile(SaveLocalTerminalProfileRequest {
                id: Some("terminal".into()),
                name: "Original".into(),
                cwd: Some("/original".into()),
                ..Default::default()
            })
            .unwrap();
        let forwards = ForwardingRegistry::new_with_store(
            oxideterm_forwarding::SavedForwardStore::load(directory.join("forwards.json")).unwrap(),
        );
        let mut settings = SettingsStore::load_from_path(directory.join("settings.json")).unwrap();
        let scope = crate::SyncScope {
            app_settings_sections: vec!["localTerminal".into()],
            ..Default::default()
        };
        let view = ConfigurationView::collect(&connections, &forwards, &settings, &scope).unwrap();
        let resource = SyncResource {
            kind: ResourceKind::LocalTerminal,
            id: "terminal".into(),
        };
        let settings_resource = SyncResource {
            kind: ResourceKind::AppSettings,
            id: "localTerminal".into(),
        };
        let selected = BTreeSet::from([resource.clone(), settings_resource.clone()]);
        let mut local = LocalReplica::new();
        let mut document = SyncReplica::load(local.snapshot.clone(), local.writer).unwrap();
        document
            .capture_local(&SyncValues::new(), &view.values, &selected)
            .unwrap();
        local.snapshot = document.snapshot();
        local.effective = view.values.clone();
        let mut remote = SyncReplica::load(local.snapshot.clone(), uuid::Uuid::new_v4()).unwrap();
        let mut remote_values = view.values.clone();
        remote_values.insert(
            SyncField {
                resource,
                group: "name".into(),
            },
            FieldValue::encode(&"Remote name").unwrap(),
        );
        remote_values.insert(
            SyncField {
                resource: settings_resource,
                group: r#"setting:["localTerminal","customEnvVars","PRIVATE_TOKEN"]"#.into(),
            },
            FieldValue::encode(&"remote-sensitive-fixture").unwrap(),
        );
        remote
            .capture_local(&view.values, &remote_values, &selected)
            .unwrap();
        let encryption = OxideBatchEncryptionContext::new("fixture-password").unwrap();
        let pending =
            PublishedReplica::encode(uuid::Uuid::new_v4(), 1, remote.snapshot(), &encryption)
                .unwrap();
        let id = PublicationId::parse(&pending.path).unwrap();
        let published = PublishedReplica::decode(
            &id,
            &pending.bytes,
            &mut OxideBatchDecryptionContext::new("fixture-password").unwrap(),
        )
        .unwrap();
        let store = ReplicaStore::open(
            &directory.join("replica"),
            &mut crate::sync_v3::tests::Secrets::default(),
        )
        .unwrap();
        store.save(&local).unwrap();
        let service = CloudSyncOperationService::new();
        let prepared = PreparedSync {
            _permit: service
                .guard
                .begin(CloudSyncOperationKind::Upload, false)
                .unwrap()
                .unwrap(),
            store,
            local,
            remote: vec![published],
            upgrade: None,
            observed: vec![id],
            backend: service.backend,
            settings: CloudSyncSettings::default(),
            secrets: CloudSyncSecrets::default(),
            scope: scope.clone(),
            filter: Default::default(),
            recovered: false,
            resolutions: Vec::new(),
        };
        assert_eq!(prepared.summary().unwrap().changed_fields, 1);
        // This edit happens after preparation, while the network phase is away.
        connections
            .upsert_local_terminal_profile(SaveLocalTerminalProfileRequest {
                id: Some("terminal".into()),
                name: "Original".into(),
                cwd: Some("/during-download".into()),
                ..Default::default()
            })
            .unwrap();
        let before_settings = serde_json::to_value(settings.settings()).unwrap();
        let applied = prepared
            .apply(&mut connections, &forwards, &mut settings)
            .unwrap();
        assert!(applied.outcome.applied);
        assert!(applied.outcome.conflicts.is_empty());
        let persisted = ConnectionStore::load(directory.join("connections.json")).unwrap();
        assert_eq!(persisted.local_terminal_profiles()[0].name, "Remote name");
        assert_eq!(
            persisted.local_terminal_profiles()[0].cwd.as_deref(),
            Some("/during-download")
        );
        assert_eq!(
            serde_json::to_value(settings.settings()).unwrap(),
            before_settings
        );
        let saved = ConfigurationView::collect(&connections, &forwards, &settings, &scope).unwrap();
        for (field, value) in &applied.local.effective {
            assert_eq!(
                saved.values.get(field),
                Some(value),
                "applied field changed during persistence: {field:?}"
            );
        }
        assert!(!directory.join("replica/recovery.oxide").exists());
        connections
            .upsert_local_terminal_profile(SaveLocalTerminalProfileRequest {
                id: Some("terminal".into()),
                name: "Edited during upload".into(),
                cwd: Some("/during-download".into()),
                ..Default::default()
            })
            .unwrap();
        let mut state = crate::state::CloudSyncPersistedState::default();
        crate::state_transitions::finish_causal_sync_state(
            &mut state,
            &applied.outcome,
            "2026-10-01T00:00:00Z".into(),
        );
        let after = build_local_snapshot(
            &connections,
            &forwards,
            &settings,
            state.last_synced_structured_state.as_ref(),
            None,
        )
        .unwrap();
        assert!(
            after.dirty.dirty_sections.connections,
            "editing during publication must remain unsynchronized"
        );
        drop(applied);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
