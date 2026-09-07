use super::*;

fn store() -> ConnectionStore {
    ConnectionStore::load(
        std::env::temp_dir().join(format!("credential-sync-{}.json", Uuid::new_v4())),
    )
    .unwrap()
}

fn password_auth(store: &ConnectionStore, value: &str) -> SavedAuth {
    let reference = Uuid::new_v4().to_string();
    store
        .keychain
        .store(&reference, &SecretString::from(value))
        .unwrap();
    SavedAuth::Password {
        keychain_id: Some(reference),
        plaintext_password: None,
    }
}

fn proxy(store: &ConnectionStore, value: &str) -> SavedUpstreamProxyConfig {
    let SavedAuth::Password { keychain_id, .. } = password_auth(store, value) else {
        unreachable!()
    };
    SavedUpstreamProxyConfig {
        protocol: SavedUpstreamProxyProtocol::Socks5,
        host: "proxy.test".into(),
        port: 1080,
        auth: SavedUpstreamProxyAuth::Password {
            username: "proxy-user".into(),
            keychain_id,
            plaintext_password: None,
        },
        remote_dns: true,
        no_proxy: String::new(),
    }
}

fn fixture(store: &mut ConnectionStore) -> CredentialSyncSelection {
    let mut sftp = StandaloneSftpProfile::new(
        "files",
        "files.test",
        22,
        "file-user",
        password_auth(store, "sftp-secret"),
    );
    sftp.upstream_proxy = SavedUpstreamProxyPolicy::Custom {
        proxy: proxy(store, "sftp-proxy-secret"),
    };
    sftp.transfer_mode = StandaloneSftpTransferMode::RemoteRemote;
    sftp.secondary_endpoint = Some(StandaloneSftpEndpoint::new(
        "backup.test",
        22,
        "backup-user",
        password_auth(store, "secondary-secret"),
    ));
    let mosh = MoshProfile::new(
        "shell",
        "shell.test",
        22,
        "shell-user",
        password_auth(store, "mosh-secret"),
    );
    let rdp = store
        .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
            name: "desktop".into(),
            protocol: RemoteDesktopProtocol::Rdp,
            host: "desktop.test".into(),
            port: 3389,
            username: Some("desktop-user".into()),
            credential: Some(SecretString::from("rdp-secret")),
            ..Default::default()
        })
        .unwrap();
    let vnc = store
        .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
            name: "screen".into(),
            protocol: RemoteDesktopProtocol::Vnc,
            host: "screen.test".into(),
            port: 5900,
            credential: Some(SecretString::from("vnc-secret")),
            ..Default::default()
        })
        .unwrap();
    let selection = CredentialSyncSelection {
        sftp_ids: BTreeSet::from([sftp.id.clone()]),
        mosh_ids: BTreeSet::from([mosh.id.clone()]),
        remote_desktop_ids: BTreeSet::from([rdp.id, vnc.id]),
        ..Default::default()
    };
    store.data.standalone_sftp_profiles.push(sftp);
    store.data.mosh_profiles.push(mosh);
    store.save().unwrap();
    selection
}

fn copy_metadata(source: &ConnectionStore, target: &mut ConnectionStore) {
    target
        .apply_standalone_sftp_profiles_snapshot(
            source.export_standalone_sftp_profiles_snapshot().unwrap(),
        )
        .unwrap();
    target
        .apply_mosh_profiles_snapshot(source.export_mosh_profiles_snapshot().unwrap())
        .unwrap();
    target
        .apply_remote_desktop_profiles_snapshot(
            source.export_remote_desktop_profiles_snapshot().unwrap(),
        )
        .unwrap();
}

#[test]
fn profile_credentials_round_trip_without_device_references_in_metadata() {
    let mut source = store();
    let selection = fixture(&mut source);
    let secrets = source.export_profile_credentials(&selection, None).unwrap();
    assert_eq!(secrets.len(), 6);
    assert!(!format!("{secrets:?}").contains("rdp-secret"));
    let mut target = store();
    copy_metadata(&source, &mut target);
    assert!(
        target
            .export_profile_credentials(&selection, None)
            .unwrap()
            .is_empty()
    );
    let mut prepared = target
        .prepare_profile_credentials(&secrets, &selection, &mut None)
        .unwrap();
    assert_eq!(prepared.summary.restored, 6);
    target.save().unwrap();
    target.commit_profile_credentials(&mut prepared).unwrap();
    let restored = target.export_profile_credentials(&selection, None).unwrap();
    for before in &secrets {
        let after = restored.iter().find(|s| s.id == before.id).unwrap();
        assert_eq!(after.secret, before.secret);
    }
    let saved = fs::read_to_string(target.path()).unwrap();
    assert!(!saved.contains("rdp-secret"));
    assert_ne!(
        source.data.remote_desktop_profiles[0].credential_ref,
        target.data.remote_desktop_profiles[0].credential_ref
    );
}

#[test]
fn selected_owners_and_target_identity_bound_credential_restore() {
    let mut source = store();
    let selection = fixture(&mut source);
    let secrets = source.export_profile_credentials(&selection, None).unwrap();
    let mut target = store();
    copy_metadata(&source, &mut target);
    target.data.remote_desktop_profiles[0].host = "different.test".into();
    let only_desktops = CredentialSyncSelection {
        remote_desktop_ids: selection.remote_desktop_ids.clone(),
        ..Default::default()
    };
    let mut prepared = target
        .prepare_profile_credentials(&secrets, &only_desktops, &mut None)
        .unwrap();
    assert_eq!(prepared.summary.restored, 1);
    assert_eq!(prepared.summary.skipped, 5);
    assert!(
        target.data.remote_desktop_profiles[0]
            .credential_ref
            .is_none()
    );
    assert!(
        target
            .export_profile_credentials(
                &CredentialSyncSelection {
                    sftp_ids: selection.sftp_ids,
                    ..Default::default()
                },
                None
            )
            .unwrap()
            .is_empty()
    );
    target.save().unwrap();
    target.commit_profile_credentials(&mut prepared).unwrap();
}

#[test]
fn absent_password_preserves_local_value_but_explicit_clear_propagates() {
    let mut source = store();
    let selection = fixture(&mut source);
    let mut target = store();
    copy_metadata(&source, &mut target);
    let secrets = source.export_profile_credentials(&selection, None).unwrap();
    let mut prepared = target
        .prepare_profile_credentials(&secrets, &selection, &mut None)
        .unwrap();
    target.save().unwrap();
    target.commit_profile_credentials(&mut prepared).unwrap();
    let id = source.data.remote_desktop_profiles[0].id.clone();
    let revision = source.profile_credentials_revision().unwrap();
    source.delete_remote_desktop_credential(&id).unwrap();
    assert_ne!(source.profile_credentials_revision().unwrap(), revision);
    copy_metadata(&source, &mut target);
    assert_eq!(
        target.get_remote_desktop_credential(&id).unwrap().unwrap(),
        "rdp-secret"
    );
    let secrets = source.export_profile_credentials(&selection, None).unwrap();
    assert!(
        secrets
            .iter()
            .any(|secret| secret.kind == CLEARED_PROFILE_CREDENTIAL_KIND)
    );
    let mut prepared = target
        .prepare_profile_credentials(&secrets, &selection, &mut None)
        .unwrap();
    assert_eq!(prepared.summary.cleared, 1);
    target.save().unwrap();
    target.commit_profile_credentials(&mut prepared).unwrap();
    assert!(target.get_remote_desktop_credential(&id).unwrap().is_none());
}

#[test]
fn credential_only_updates_change_revision_and_failed_prepare_keeps_old_password() {
    let mut source = store();
    let selection = fixture(&mut source);
    let id = source.data.remote_desktop_profiles[0].id.clone();
    let before = source.profile_credentials_revision().unwrap();
    source
        .save_remote_desktop_credential(&id, &SecretString::from("updated-secret"))
        .unwrap();
    assert_ne!(source.profile_credentials_revision().unwrap(), before);
    let mut secrets = source.export_profile_credentials(&selection, None).unwrap();
    let mut target = store();
    copy_metadata(&source, &mut target);
    target.keychain =
        ConnectionKeychain::with_max_secret_bytes_for_tests("credential-sync-failure", 32);
    target
        .save_remote_desktop_credential(&id, &SecretString::from("existing-secret"))
        .unwrap();
    let old_data = fs::read(target.path()).unwrap();
    secrets.last_mut().unwrap().secret = zeroize::Zeroizing::new("x".repeat(64));
    assert!(
        target
            .prepare_profile_credentials(&secrets, &selection, &mut None)
            .is_err()
    );
    assert_eq!(fs::read(target.path()).unwrap(), old_data);
    assert_eq!(
        target.get_remote_desktop_credential(&id).unwrap().unwrap(),
        "existing-secret"
    );
}

#[test]
fn global_proxy_restore_uses_new_local_slot_and_clear_is_explicit() {
    let mut source = store();
    let reference = source
        .save_global_upstream_proxy_password(&SecretString::from("global-secret"))
        .unwrap();
    let mut source_proxy = proxy(&source, "unused-fixture-secret");
    if let SavedUpstreamProxyAuth::Password { keychain_id, .. } = &mut source_proxy.auth {
        *keychain_id = Some(reference);
    }
    let selection = CredentialSyncSelection {
        global_proxy: true,
        ..Default::default()
    };
    let secrets = source
        .export_profile_credentials(&selection, Some(&source_proxy))
        .unwrap();
    let mut target = store();
    let mut target_proxy = source_proxy.clone();
    if let SavedUpstreamProxyAuth::Password { keychain_id, .. } = &mut target_proxy.auth {
        *keychain_id = None;
    }
    let mut target_proxy = Some(target_proxy);
    let mut prepared = target
        .prepare_profile_credentials(&secrets, &selection, &mut target_proxy)
        .unwrap();
    target.save().unwrap();
    target.commit_profile_credentials(&mut prepared).unwrap();
    let SavedUpstreamProxyAuth::Password {
        keychain_id: Some(reference),
        ..
    } = &target_proxy.as_ref().unwrap().auth
    else {
        panic!("restored proxy reference");
    };
    assert_ne!(reference, GLOBAL_UPSTREAM_PROXY_PASSWORD_KEYCHAIN_ID);
    assert_eq!(
        target
            .get_global_upstream_proxy_password(reference)
            .unwrap(),
        "global-secret"
    );
    let revision = source.profile_credentials_revision().unwrap();
    source.delete_global_upstream_proxy_password().unwrap();
    if let SavedUpstreamProxyAuth::Password { keychain_id, .. } = &mut source_proxy.auth {
        *keychain_id = None;
    }
    assert_ne!(source.profile_credentials_revision().unwrap(), revision);
    let cleared = source
        .export_profile_credentials(&selection, Some(&source_proxy))
        .unwrap();
    assert_eq!(cleared[0].kind, CLEARED_PROFILE_CREDENTIAL_KIND);
    let mut prepared = target
        .prepare_profile_credentials(&cleared, &selection, &mut target_proxy)
        .unwrap();
    assert_eq!(prepared.summary.cleared, 1);
    target.save().unwrap();
    target.commit_profile_credentials(&mut prepared).unwrap();
}

#[test]
fn sftp_key_passphrase_and_mosh_hop_password_survive_encrypted_archive() {
    use crate::oxide_file::{
        OxideExportOptions, OxideFile, decrypt_oxide_file, export_connections_to_oxide,
    };
    let mut source = store();
    let selection = fixture(&mut source);
    let auth = password_auth(&source, "key-passphrase");
    let SavedAuth::Password { keychain_id, .. } = auth else {
        unreachable!()
    };
    source.data.standalone_sftp_profiles[0].auth = SavedAuth::Key {
        key_path: "~/.ssh/private-test".into(),
        has_passphrase: true,
        passphrase_keychain_id: keychain_id,
        plaintext_passphrase: None,
    };
    let hop_auth = password_auth(&source, "jump-secret");
    source.data.mosh_profiles[0]
        .proxy_chain
        .push(SavedProxyHop {
            host: "jump.test".into(),
            port: 22,
            username: "jump-user".into(),
            auth: hop_auth,
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: Default::default(),
        });
    let secrets = source.export_profile_credentials(&selection, None).unwrap();
    let bytes = export_connections_to_oxide(
        &source,
        &[],
        "test-sync-passphrase",
        OxideExportOptions {
            portable_secrets: secrets,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        !bytes
            .windows(b"jump-secret".len())
            .any(|window| window == b"jump-secret")
    );
    assert!(
        decrypt_oxide_file(
            &OxideFile::from_bytes(&bytes).unwrap(),
            "wrong-sync-password"
        )
        .is_err()
    );
    let preview = crate::oxide_file::preview_oxide_import(
        &source,
        &bytes,
        "test-sync-passphrase",
        crate::oxide_file::ImportConflictStrategy::Merge,
    )
    .unwrap();
    assert_eq!(preview.profile_credentials.len(), 7);
    let preview_json = serde_json::to_string(&preview).unwrap();
    assert!(!preview_json.contains("jump-secret"));
    assert!(!preview_json.contains("key-passphrase"));
    let payload = decrypt_oxide_file(
        &OxideFile::from_bytes(&bytes).unwrap(),
        "test-sync-passphrase",
    )
    .unwrap();
    let mut target = store();
    copy_metadata(&source, &mut target);
    let mut prepared = target
        .prepare_profile_credentials(&payload.portable_secrets, &selection, &mut None)
        .unwrap();
    target.save().unwrap();
    target.commit_profile_credentials(&mut prepared).unwrap();
    assert_eq!(
        target
            .get_saved_auth_passphrase(&target.data.standalone_sftp_profiles[0].auth)
            .unwrap()
            .unwrap(),
        "key-passphrase"
    );
    assert_eq!(
        target
            .get_saved_auth_password(&target.data.mosh_profiles[0].proxy_chain[0].auth)
            .unwrap(),
        "jump-secret"
    );
}

#[test]
fn failed_metadata_commit_rolls_back_staged_slots_and_keeps_original_password() {
    let mut source = store();
    let selection = fixture(&mut source);
    let secrets = source.export_profile_credentials(&selection, None).unwrap();
    let mut target = store();
    copy_metadata(&source, &mut target);
    let id = target.data.remote_desktop_profiles[0].id.clone();
    target
        .save_remote_desktop_credential(&id, &SecretString::from("original-secret"))
        .unwrap();
    let original_file = fs::read(target.path()).unwrap();
    let checkpoint = target.create_checkpoint().unwrap();
    let prepared = target
        .prepare_profile_credentials(&secrets, &selection, &mut None)
        .unwrap();
    let staged_references = target
        .credential_bindings(None)
        .iter()
        .filter_map(|binding| binding.reference.map(str::to_string))
        .collect::<Vec<_>>();
    inject_atomic_replace_failure();
    assert!(target.save().is_err());
    target.restore_checkpoint(&checkpoint).unwrap();
    target.rollback_profile_credentials(prepared).unwrap();
    assert_eq!(fs::read(target.path()).unwrap(), original_file);
    assert_eq!(
        target.get_remote_desktop_credential(&id).unwrap().unwrap(),
        "original-secret"
    );
    assert!(
        staged_references.iter().all(|reference| target
            .keychain
            .get_optional(reference)
            .unwrap()
            .is_none())
    );
}

#[test]
fn http_proxy_password_and_explicit_forget_are_exported_for_selected_ssh_owner() {
    let mut source = store();
    let mut connection: SavedConnection = serde_json::from_value(serde_json::json!({
        "id":"ssh-proxy", "name":"ssh", "host":"ssh.test", "username":"ssh-user", "auth":{"type":"agent"}, "created_at":Utc::now()
    })).unwrap();
    let mut config = proxy(&source, "http-secret");
    config.protocol = SavedUpstreamProxyProtocol::HttpConnect;
    connection.upstream_proxy = SavedUpstreamProxyPolicy::Custom { proxy: config };
    source.data.connections.push(connection);
    source.save().unwrap();
    let selection = CredentialSyncSelection {
        connection_ids: BTreeSet::from(["ssh-proxy".into()]),
        ..Default::default()
    };
    assert_eq!(
        source.export_profile_credentials(&selection, None).unwrap()[0]
            .secret
            .as_str(),
        "http-secret"
    );
    let revision = source.profile_credentials_revision().unwrap();
    source
        .forget_connection_credential("ssh-proxy", ConnectionCredentialSlot::UpstreamProxy)
        .unwrap();
    assert_ne!(source.profile_credentials_revision().unwrap(), revision);
    assert_eq!(
        source.export_profile_credentials(&selection, None).unwrap()[0].kind,
        CLEARED_PROFILE_CREDENTIAL_KIND
    );
    assert!(
        source
            .export_profile_credentials(&CredentialSyncSelection::default(), None)
            .unwrap()
            .is_empty()
    );
}
