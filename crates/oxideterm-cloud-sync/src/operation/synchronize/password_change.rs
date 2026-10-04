// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl AppliedSync {
    /// The active settings change only after the new snapshot is read back and
    /// authenticated. The previous namespace and its password remain available.
    pub async fn change_password(
        mut self,
        password: Zeroizing<String>,
        provider: &mut impl CloudSyncSecretProvider,
        settings_path: &std::path::Path,
    ) -> Result<SyncOutcome> {
        if password.chars().count() < 6 {
            bail!("password_too_short");
        }
        if self.outcome.cleanup_pending {
            bail!("sync_cleanup_pending");
        }
        if self.secrets.sync_password.as_deref().map(String::as_str) == Some(password.as_str()) {
            return self.publish().await;
        }
        let settings = if let Some(settings) = &self.local.pending_password_change {
            let previous =
                provider.get_secret(settings.password_secret_key(), SecretReadMode::Prompt)?;
            if previous.as_deref().map(String::as_str) != Some(password.as_str()) {
                bail!("password_change_pending");
            }
            settings.clone()
        } else {
            let mut settings = self.settings.clone();
            settings.namespace = format!("{}-{}", settings.namespace, uuid::Uuid::new_v4());
            let reference = format!("sync-v3-password-{}", uuid::Uuid::new_v4());
            provider.store_secret(&reference, Some(password.as_str()))?;
            settings.sync_password_ref = Some(reference);
            self.local.pending_password_change = Some(settings.clone());
            self.store.save(&self.local)?;
            settings
        };
        let store = open_replica(settings_path, &settings, provider)?;
        let existing = store.load()?;
        let resuming = existing.is_some();
        let mut local = existing.unwrap_or_default();
        let mut replica = SyncReplica::load(local.snapshot.clone(), local.writer)?;
        replica.merge(self.local.snapshot.clone())?;
        local.snapshot = replica.snapshot();
        local.effective = self.local.effective.clone();
        local.legacy_imported = true;
        store.bind_sync_password(&mut local, &password)?;
        store.save(&local)?;
        let mut secrets = self.secrets;
        secrets.sync_password = Some(password);
        if !self
            .backend
            .list_publications(&settings, &secrets)
            .await?
            .is_empty()
            && !resuming
        {
            bail!("The new synchronization namespace is already occupied");
        }
        let mut outcome = self.outcome;
        outcome.switched_settings = Some(settings.clone());
        outcome.publication = None;
        outcome.published = false;
        outcome.publication_pending = true;
        let next = AppliedSync {
            _permit: self._permit,
            store,
            local,
            backend: self.backend,
            settings,
            secrets,
            outcome,
        };
        let outcome = next.publish().await?;
        self.local.pending_password_change = None;
        self.store.save(&self.local)?;
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };

    #[test]
    fn password_change_switches_only_after_exact_readback_and_retains_old_replica() {
        for corrupt_readback in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let finished = stop.clone();
            let server = std::thread::spawn(move || {
                let mut uploaded = Vec::new();
                while !finished.load(Ordering::SeqCst) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(value) => value,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                        Err(error) => panic!("{error}"),
                    };
                    // macOS accepts sockets with the listener's nonblocking mode.
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(10)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 4096];
                    let end = loop {
                        let size = stream.read(&mut buffer).unwrap();
                        assert!(size > 0);
                        request.extend_from_slice(&buffer[..size]);
                        if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n")
                        {
                            break end + 4;
                        }
                    };
                    let header = String::from_utf8(request[..end].to_vec()).unwrap();
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    while request.len() < end + length {
                        let size = stream.read(&mut buffer).unwrap();
                        assert!(size > 0);
                        request.extend_from_slice(&buffer[..size]);
                    }
                    let (status, body) = if header.starts_with("PUT ") {
                        uploaded = request[end..end + length].to_vec();
                        ("200 OK", b"{}".to_vec())
                    } else if header.lines().next().unwrap().contains("?prefix=") {
                        ("200 OK", br#"{"objects":[],"nextCursor":null}"#.to_vec())
                    } else if uploaded.is_empty() {
                        ("404 Not Found", Vec::new())
                    } else {
                        let mut bytes = uploaded.clone();
                        if corrupt_readback {
                            bytes.push(0);
                        }
                        ("200 OK", bytes)
                    };
                    write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .unwrap();
                    stream.write_all(&body).unwrap();
                }
                uploaded
            });
            let directory = std::env::temp_dir()
                .join(format!("oxide-password-change-{}", uuid::Uuid::new_v4()));
            let settings_path = directory.join("settings.json");
            let mut provider = crate::sync_v3::tests::Secrets::default();
            provider
                .store_secret(secret_keys::SYNC_PASSWORD, Some("old-password"))
                .unwrap();
            let settings = CloudSyncSettings {
                backend_type: BackendType::HttpJson,
                auth_mode: crate::AuthMode::None,
                endpoint,
                namespace: "original".into(),
                ..Default::default()
            };
            let store = open_replica(&settings_path, &settings, &mut provider).unwrap();
            let mut local = LocalReplica::new();
            let resource = SyncResource {
                kind: crate::sync_v3::ResourceKind::QuickCommand,
                id: "deploy".into(),
            };
            let name = crate::sync_v3::SyncField {
                resource: resource.clone(),
                group: "name".into(),
            };
            let values = SyncValues::from([
                (
                    crate::sync_v3::SyncField::presence(resource.clone()),
                    crate::sync_v3::FieldValue::encode(&true).unwrap(),
                ),
                (
                    name.clone(),
                    crate::sync_v3::FieldValue::encode(&"Deploy").unwrap(),
                ),
            ]);
            let mut replica = SyncReplica::load(local.snapshot.clone(), local.writer).unwrap();
            replica
                .capture_local(
                    &SyncValues::new(),
                    &values,
                    &BTreeSet::from([resource.clone()]),
                )
                .unwrap();
            local.snapshot = replica.snapshot();
            local.effective = values;
            store
                .bind_sync_password(&mut local, "old-password")
                .unwrap();
            store.save(&local).unwrap();
            let original_writer = local.writer;
            let service = CloudSyncOperationService::new();
            let applied = AppliedSync {
                _permit: service
                    .guard
                    .begin(CloudSyncOperationKind::Upload, false)
                    .unwrap()
                    .unwrap(),
                store,
                local,
                backend: service.backend,
                settings: settings.clone(),
                secrets: CloudSyncSecrets {
                    sync_password: Some(Zeroizing::new("old-password".into())),
                    ..Default::default()
                },
                outcome: SyncOutcome {
                    switched_settings: None,
                    applied: true,
                    published: false,
                    publication_pending: true,
                    recovered: false,
                    cleanup_pending: false,
                    publication: None,
                    created_remote_id: None,
                    local_snapshot: Default::default(),
                    conflicts: Vec::new(),
                },
            };
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let result = runtime.block_on(applied.change_password(
                Zeroizing::new("new-password".into()),
                &mut provider,
                &settings_path,
            ));
            stop.store(true, Ordering::SeqCst);
            let bytes = server.join().unwrap();
            let mut state = crate::state::CloudSyncPersistedState {
                settings: settings.clone(),
                ..Default::default()
            };
            if corrupt_readback {
                assert!(result.is_err());
            } else {
                let outcome = result.unwrap();
                let target = outcome.switched_settings.as_ref().unwrap();
                assert_ne!(target.namespace, "original");
                let reference = target.sync_password_ref.as_deref().unwrap();
                assert_eq!(
                    provider
                        .get_secret(reference, SecretReadMode::Prompt)
                        .unwrap()
                        .unwrap()
                        .as_str(),
                    "new-password"
                );
                let published = PublishedReplica::decode(
                    outcome.publication.as_ref().unwrap(),
                    &bytes,
                    &mut OxideBatchDecryptionContext::new("new-password").unwrap(),
                )
                .unwrap();
                let replica = SyncReplica::load(published.snapshot, published.writer).unwrap();
                let restored = replica
                    .materialize(&SyncValues::new(), &BTreeSet::from([resource]))
                    .unwrap();
                assert_eq!(restored[&name].decode::<String>().unwrap(), "Deploy");
                crate::state_transitions::finish_causal_sync_state(
                    &mut state,
                    &outcome,
                    "2026-10-01T00:00:00Z".into(),
                );
                assert_eq!(state.settings, target.clone());
            }
            let source = open_replica(&settings_path, &settings, &mut provider).unwrap();
            let original = source.load().unwrap().unwrap();
            assert_eq!(original.writer, original_writer);
            assert_eq!(original.pending_password_change.is_some(), corrupt_readback);
            if let Some(target) = &original.pending_password_change {
                let staged = open_replica(&settings_path, target, &mut provider).unwrap();
                let pending = staged.load().unwrap().unwrap().pending.unwrap();
                assert_eq!(pending.bytes, bytes);
                assert_eq!(
                    provider
                        .get_secret(target.password_secret_key(), SecretReadMode::Prompt)
                        .unwrap()
                        .unwrap()
                        .as_str(),
                    "new-password"
                );
            }
            let replica = SyncReplica::load(original.snapshot, original.writer).unwrap();
            let preserved = replica
                .materialize(&SyncValues::new(), &replica.resources().unwrap())
                .unwrap();
            assert_eq!(preserved[&name].decode::<String>().unwrap(), "Deploy");
            drop(source);
            assert_eq!(
                provider
                    .get_secret(secret_keys::SYNC_PASSWORD, SecretReadMode::Prompt)
                    .unwrap()
                    .unwrap()
                    .as_str(),
                "old-password"
            );
            std::fs::remove_dir_all(directory).unwrap();
        }
    }
}
