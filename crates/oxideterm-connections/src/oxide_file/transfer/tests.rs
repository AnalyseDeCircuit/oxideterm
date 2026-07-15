#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::{
        PrivilegeCredentialKind, SavePrivilegeCredentialRequest, SaveSerialProfileRequest,
        SavedUpstreamProxyProtocol, SerialFlowControl, SerialProfile, SerialProfilesSyncSnapshot,
    };
    use rand10::{rand_core::UnwrapErr, rngs::SysRng};
    use russh::keys::ssh_key::LineEnding;
    use russh::keys::{Algorithm, PrivateKey};

    fn temp_store(name: &str) -> ConnectionStore {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-oxide-file-{name}-{}.json",
            Uuid::new_v4()
        ));
        ConnectionStore::load(path).unwrap()
    }

    fn generated_private_key_text() -> String {
        let key_path =
            std::env::temp_dir().join(format!("oxideterm-managed-key-{}.key", Uuid::new_v4()));
        let mut rng = UnwrapErr(SysRng);
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap();
        key.write_openssh_file(&key_path, LineEnding::LF).unwrap();
        let private_key = fs::read_to_string(&key_path).unwrap();
        let _ = fs::remove_file(key_path);
        private_key
    }

    fn saved_connection(id: &str, name: &str) -> SavedConnection {
        SavedConnection {
            id: id.to_string(),
            version: CONFIG_VERSION,
            name: name.to_string(),
            group: Some("Ops".to_string()),
            host: "example.com".to_string(),
            port: 2222,
            username: "deploy".to_string(),
            auth: SavedAuth::Key {
                key_path: "~/.ssh/id_ed25519".to_string(),
                has_passphrase: true,
                passphrase_keychain_id: None,
                plaintext_passphrase: Some(SecretString::from("phrase")),
            },
            proxy_chain: vec![SavedProxyHop {
                host: "jump.example.com".to_string(),
                port: 22,
                username: "jump".to_string(),
                auth: SavedAuth::Agent,
                agent_forwarding: false,
                legacy_ssh_compatibility: false,
            }],
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            options: ConnectionOptions {
                keep_alive_interval: 30,
                compression: true,
                jump_host: None,
                term_type: Some("xterm-256color".to_string()),
                agent_forwarding: true,
                legacy_ssh_compatibility: false,
                post_connect_command: None,
            },
            created_at: Utc::now(),
            last_used_at: None,
            updated_at: Some(Utc::now()),
            color: Some("#ff6a00".to_string()),
            icon: Some("server".to_string()),
            tags: vec!["prod".to_string()],
            post_connect_command: None,
            privilege_credentials: Vec::new(),
        }
    }

    #[test]
    fn export_import_roundtrip_preserves_connections_and_payload_sections() {
        let mut source = temp_store("source");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();

        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions {
                description: Some("backup".to_string()),
                app_settings_json: Some(
                    r#"{"format":"oxide-settings-sections-v1","sectionIds":["ai","localTerminal"],"settings":{"ai":{"enabled":true},"localTerminal":{"customEnvVars":{"FOO":"bar"}}}}"#
                        .to_string(),
                ),
                quick_commands_json: Some(
                    r#"{"commands":[{"id":"1"}],"categories":[{}]}"#.to_string(),
                ),
                forwards: vec![OxideForwardRecord {
                    id: Some("forward-1".to_string()),
                    connection_id: "conn-1".to_string(),
                    forward_type: "local".to_string(),
                    bind_address: "127.0.0.1".to_string(),
                    bind_port: 8080,
                    target_host: "127.0.0.1".to_string(),
                    target_port: 80,
                    description: Some("web".to_string()),
                    auto_start: true,
                }],
                ..OxideExportOptions::default()
            },
        )
        .unwrap();

        let file = OxideFile::from_bytes(&bytes).unwrap();
        assert_eq!(file.metadata.num_connections, 1);
        assert_eq!(file.metadata.quick_commands_count, Some(1));
        assert_eq!(file.metadata.quick_command_categories_count, Some(1));

        let preview = preview_oxide_import(
            &temp_store("preview"),
            &bytes,
            "secret!",
            ImportConflictStrategy::Rename,
        )
        .unwrap();
        assert_eq!(preview.total_connections, 1);
        assert_eq!(preview.unchanged, vec!["Prod".to_string()]);
        assert_eq!(preview.total_forwards, 1);
        assert_eq!(preview.forward_details.len(), 1);
        assert_eq!(
            preview.forward_details[0].description,
            "web (L:8080 -> 127.0.0.1:80)"
        );
        assert_eq!(preview.records.len(), 1);
        assert_eq!(preview.records[0].action, "import");
        assert_eq!(preview.records[0].reason_code, "new-connection");
        assert!(preview.has_quick_commands);
        assert_eq!(
            preview.app_settings_section_ids,
            vec!["localTerminal".to_string()]
        );
        assert!(preview.app_settings_contains_local_terminal_env_vars);

        let mut target = temp_store("target");
        let result = apply_oxide_import(
            &mut target,
            &bytes,
            "secret!",
            ImportConflictStrategy::Rename,
        )
        .unwrap();
        assert_eq!(result.imported, 1);
        assert_eq!(result.imported_forwards, 1);
        assert_eq!(result.forward_records.len(), 1);
        assert_eq!(result.forward_records[0].forward_type, "local");
        assert!(result.quick_commands_json.is_some());

        let imported = target.connections().first().unwrap();
        assert_eq!(imported.name, "Prod");
        assert_eq!(imported.host, "example.com");
        assert_eq!(imported.port, 2222);
        assert_eq!(imported.options.keep_alive_interval, 30);
        assert!(imported.options.compression);
        assert_eq!(imported.proxy_chain.len(), 1);
        assert!(
            target
                .get_connection_passphrase(&imported.id)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn import_validates_late_profile_resources_before_committing_connections() {
        let mut source = temp_store("late-invalid-profile-source");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();
        let mut invalid_profile = SerialProfile::new("Invalid", "/dev/cu.invalid");
        invalid_profile.port_path.clear();
        let serial_snapshot = SerialProfilesSyncSnapshot {
            revision: "invalid".to_string(),
            exported_at: Utc::now().to_rfc3339(),
            records: vec![invalid_profile],
        };
        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions {
                serial_profiles_json: Some(serde_json::to_string(&serial_snapshot).unwrap()),
                ..OxideExportOptions::default()
            },
        )
        .unwrap();
        let mut target = temp_store("late-invalid-profile-target");
        let target_path = target.path().to_path_buf();

        let result = apply_oxide_import(
            &mut target,
            &bytes,
            "secret!",
            ImportConflictStrategy::Rename,
        );

        assert!(result.is_err());
        assert!(target.connections().is_empty());
        assert!(
            ConnectionStore::load(target_path)
                .unwrap()
                .connections()
                .is_empty()
        );
    }

    #[test]
    fn failed_connection_upsert_restores_profiles_and_store_file() {
        const IMPORT_PASSWORD: &str = "secret!";
        const CONNECTION_SECRET: &str = "oxide-rollback-secret";

        let mut source = temp_store("transaction-source");
        let mut imported_connection = saved_connection("conn-import", "Imported");
        imported_connection.auth = SavedAuth::Password {
            keychain_id: None,
            plaintext_password: Some(SecretString::from(CONNECTION_SECRET)),
        };
        source
            .upsert_imported_connection(imported_connection)
            .unwrap();
        source
            .upsert_serial_profile(SaveSerialProfileRequest {
                id: Some("serial-import".to_string()),
                name: "Imported Serial".to_string(),
                port_path: "/dev/cu.imported".to_string(),
                ..SaveSerialProfileRequest::default()
            })
            .unwrap();
        let serial_profiles_json =
            serde_json::to_string_pretty(&source.export_serial_profiles_snapshot().unwrap())
                .unwrap();
        let exported = export_connections_to_oxide(
            &source,
            &["conn-import".to_string()],
            IMPORT_PASSWORD,
            OxideExportOptions {
                include_passwords: true,
                serial_profiles_json: Some(serial_profiles_json),
                ..OxideExportOptions::default()
            },
        )
        .unwrap();

        // Re-encrypt a checksum-valid archive whose connection fails only in
        // the final store upsert, after the profile stage has been persisted.
        let exported_file = OxideFile::from_bytes(&exported).unwrap();
        let mut payload = decrypt_payload(&exported, IMPORT_PASSWORD).unwrap();
        payload.connections[0].host.clear();
        payload.checksum = compute_checksum(&payload).unwrap();
        let bytes = encrypt_oxide_file(&payload, IMPORT_PASSWORD, exported_file.metadata)
            .unwrap()
            .to_bytes()
            .unwrap();

        let mut target = temp_store("transaction-target");
        target
            .upsert_imported_connection(saved_connection("conn-local", "Local"))
            .unwrap();
        target
            .upsert_serial_profile(SaveSerialProfileRequest {
                id: Some("serial-local".to_string()),
                name: "Local Serial".to_string(),
                port_path: "/dev/cu.local".to_string(),
                ..SaveSerialProfileRequest::default()
            })
            .unwrap();
        let target_path = target.path().to_path_buf();
        let original_file = fs::read(&target_path).unwrap();
        let original_connections = target
            .connections()
            .iter()
            .map(|connection| connection.id.clone())
            .collect::<Vec<_>>();
        let original_serial_profiles = target.serial_profiles().to_vec();

        let error = apply_oxide_import(
            &mut target,
            &bytes,
            IMPORT_PASSWORD,
            ImportConflictStrategy::Rename,
        )
        .unwrap_err();

        assert!(!error.to_string().contains(CONNECTION_SECRET));
        assert_eq!(
            target
                .connections()
                .iter()
                .map(|connection| connection.id.clone())
                .collect::<Vec<_>>(),
            original_connections
        );
        assert_eq!(target.serial_profiles(), original_serial_profiles);
        assert_eq!(fs::read(&target_path).unwrap(), original_file);

        let reloaded = ConnectionStore::load(target_path).unwrap();
        assert_eq!(
            reloaded
                .connections()
                .iter()
                .map(|connection| connection.id.clone())
                .collect::<Vec<_>>(),
            original_connections
        );
        assert_eq!(reloaded.serial_profiles(), original_serial_profiles);
    }

    #[test]
    fn export_import_roundtrip_preserves_serial_profiles() {
        let mut source = temp_store("serial-profile-source");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();
        let profile = source
            .upsert_serial_profile(SaveSerialProfileRequest {
                id: Some("serial-1".to_string()),
                name: "Lab console".to_string(),
                port_path: "/dev/cu.usbserial-1".to_string(),
                flow_control: Some(SerialFlowControl::Hardware),
                ..SaveSerialProfileRequest::default()
            })
            .unwrap();
        let serial_profiles_json =
            serde_json::to_string_pretty(&source.export_serial_profiles_snapshot().unwrap())
                .unwrap();

        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions {
                serial_profiles_json: Some(serial_profiles_json),
                ..OxideExportOptions::default()
            },
        )
        .unwrap();
        let file = OxideFile::from_bytes(&bytes).unwrap();
        assert_eq!(file.metadata.serial_profiles_count, Some(1));

        let preview = preview_oxide_import(
            &temp_store("serial-profile-preview"),
            &bytes,
            "secret!",
            ImportConflictStrategy::Rename,
        )
        .unwrap();
        assert_eq!(preview.serial_profiles_count, 1);

        let mut target = temp_store("serial-profile-target");
        let imported = apply_oxide_import(
            &mut target,
            &bytes,
            "secret!",
            ImportConflictStrategy::Rename,
        )
        .unwrap();
        assert_eq!(imported.imported_serial_profiles, 1);
        assert_eq!(target.serial_profiles(), &[profile]);

        let mut skipped_target = temp_store("serial-profile-skip-target");
        let skipped = apply_oxide_import_with_options(
            &mut skipped_target,
            &bytes,
            "secret!",
            OxideImportOptions {
                import_serial_profiles: false,
                ..OxideImportOptions::default()
            },
        )
        .unwrap();
        assert_eq!(skipped.imported_serial_profiles, 0);
        assert_eq!(skipped.skipped_serial_profiles, 1);
        assert!(skipped_target.serial_profiles().is_empty());
    }

    #[test]
    fn legacy_oxide_payload_ignores_removed_raw_profile_sections() {
        // Older archives remain readable because removed sections deserialize as unknown fields.
        let payload: EncryptedPayload = serde_json::from_value(serde_json::json!({
            "version": 2,
            "connections": [],
            "raw_tcp_profiles_json": "{\"records\":[]}",
            "raw_udp_profiles_json": "{\"records\":[]}",
            "checksum": "legacy-checksum"
        }))
        .unwrap();

        let serialized = serde_json::to_value(payload).unwrap();
        assert_eq!(serialized["version"], 2);
        assert!(serialized.get("raw_tcp_profiles_json").is_none());
        assert!(serialized.get("raw_udp_profiles_json").is_none());
    }

    #[test]
    fn encrypted_export_import_restores_privilege_credential_secret() {
        let mut source = temp_store("privilege-source");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();
        source
            .save_privilege_credential(SavePrivilegeCredentialRequest {
                connection_id: "conn-1".to_string(),
                credential_id: Some("sudo-prod".to_string()),
                label: "sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: Some("deploy".to_string()),
                prompt_patterns: Vec::new(),
                secret: Some(SecretString::from("sudo-secret")),
                enabled: true,
                require_click_to_send: true,
            })
            .unwrap();

        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions {
                include_passwords: true,
                ..OxideExportOptions::default()
            },
        )
        .unwrap();
        let mut target = temp_store("privilege-target");
        apply_oxide_import_with_options(
            &mut target,
            &bytes,
            "secret!",
            OxideImportOptions {
                conflict_strategy: ImportConflictStrategy::Replace,
                ..OxideImportOptions::default()
            },
        )
        .unwrap();
        let imported = target
            .connections()
            .into_iter()
            .find(|connection| connection.name == "Prod")
            .unwrap();

        assert_eq!(imported.privilege_credentials.len(), 1);
        assert_eq!(
            target
                .get_privilege_credential_secret(&imported.id, "sudo-prod")
                .unwrap(),
            "sudo-secret"
        );
    }

    #[test]
    fn managed_key_export_import_restores_managed_key_store_entry() {
        let mut source = temp_store("managed-source");
        let private_key = generated_private_key_text();
        let managed_key = source
            .create_managed_ssh_key_from_text(
                SecretString::from(private_key.clone()),
                Some("Deploy key".to_string()),
                None,
            )
            .unwrap();
        let mut connection = saved_connection("conn-1", "Prod");
        connection.auth = SavedAuth::ManagedKey {
            key_id: managed_key.id,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        };
        connection.proxy_chain.clear();
        source.upsert_imported_connection(connection).unwrap();

        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions::default(),
        )
        .unwrap();
        let file = OxideFile::from_bytes(&bytes).unwrap();
        assert_eq!(file.metadata.managed_key_count, Some(1));
        let payload = decrypt_payload(&bytes, "secret!").unwrap();
        assert!(matches!(
            payload.connections[0].auth,
            EncryptedAuth::Key {
                managed_key: Some(_),
                embedded_key: Some(_),
                ..
            }
        ));

        let mut target = temp_store("managed-target");
        let result = apply_oxide_import(
            &mut target,
            &bytes,
            "secret!",
            ImportConflictStrategy::Rename,
        )
        .unwrap();

        assert_eq!(result.imported, 1);
        let keys = target.managed_ssh_keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "Deploy key");
        let imported = target.connections().first().unwrap();
        assert!(matches!(
            &imported.auth,
            SavedAuth::ManagedKey { key_id, .. } if key_id == &keys[0].id
        ));
        let restored_key = target
            .resolve_managed_ssh_key_private_key(&keys[0].id)
            .unwrap();
        assert_eq!(restored_key.expose_secret(), private_key);
    }

    #[test]
    fn managed_key_import_can_extract_embedded_key_when_restore_disabled() {
        let mut source = temp_store("managed-fallback-source");
        let private_key = generated_private_key_text();
        let managed_key = source
            .create_managed_ssh_key_from_text(
                SecretString::from(private_key),
                Some("Deploy key".to_string()),
                None,
            )
            .unwrap();
        let mut connection = saved_connection("conn-1", "Prod");
        connection.auth = SavedAuth::ManagedKey {
            key_id: managed_key.id,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        };
        connection.proxy_chain.clear();
        source.upsert_imported_connection(connection).unwrap();

        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions::default(),
        )
        .unwrap();
        let mut target = temp_store("managed-fallback-target");
        let result = apply_oxide_import_with_options(
            &mut target,
            &bytes,
            "secret!",
            OxideImportOptions {
                restore_managed_keys: false,
                ..OxideImportOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.imported, 1);
        assert!(target.managed_ssh_keys().is_empty());
        let imported = target.connections().first().unwrap();
        assert!(
            matches!(&imported.auth, SavedAuth::Key { key_path, .. } if key_path.contains(".ssh/imported"))
        );
    }

    #[test]
    fn transfer_progress_matches_tauri_stage_lifecycle() {
        let mut source = temp_store("progress-source");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();

        let mut export_progress = Vec::new();
        let bytes = export_connections_to_oxide_with_progress(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions::default(),
            |stage, current, total| export_progress.push((stage.to_string(), current, total)),
        )
        .unwrap();
        assert_eq!(
            export_progress,
            vec![
                ("collecting_connections".to_string(), 1, 10),
                ("collecting_portable_secrets".to_string(), 2, 10),
                ("computing_checksum".to_string(), 3, 10),
                ("building_metadata".to_string(), 4, 10),
                ("generating_salt_nonce".to_string(), 5, 10),
                ("deriving_key".to_string(), 6, 10),
                ("serializing_payload".to_string(), 7, 10),
                ("encrypting_payload".to_string(), 8, 10),
                ("finalizing_file".to_string(), 9, 10),
                ("serializing_file".to_string(), 10, 10),
            ]
        );

        let mut preview_progress = Vec::new();
        preview_oxide_import_with_progress(
            &temp_store("progress-preview"),
            &bytes,
            "secret!",
            ImportConflictStrategy::Rename,
            |stage, current, total| preview_progress.push((stage.to_string(), current, total)),
        )
        .unwrap();
        assert_eq!(
            preview_progress,
            vec![
                ("parsing_file".to_string(), 1, 8),
                ("deriving_key".to_string(), 2, 8),
                ("decrypting_payload".to_string(), 3, 8),
                ("deserializing_payload".to_string(), 4, 8),
                ("verifying_checksum".to_string(), 5, 8),
                ("collecting_existing".to_string(), 6, 8),
                ("building_preview".to_string(), 7, 8),
                ("analyzing_preview".to_string(), 8, 8),
            ]
        );

        let mut apply_progress = Vec::new();
        let mut target = temp_store("progress-apply");
        apply_oxide_import_with_options_with_progress(
            &mut target,
            &bytes,
            "secret!",
            OxideImportOptions::default(),
            |stage, current, total| apply_progress.push((stage.to_string(), current, total)),
        )
        .unwrap();
        assert_eq!(
            apply_progress,
            vec![
                ("parsing_file".to_string(), 1, 10),
                ("deriving_key".to_string(), 2, 10),
                ("decrypting_payload".to_string(), 3, 10),
                ("deserializing_payload".to_string(), 4, 10),
                ("verifying_checksum".to_string(), 5, 10),
                ("filtering_selection".to_string(), 6, 10),
                ("collecting_existing".to_string(), 7, 10),
                ("preparing_connections".to_string(), 8, 10),
                ("saving_config".to_string(), 9, 10),
                ("applying_connections".to_string(), 10, 10),
            ]
        );
    }

    #[test]
    fn rename_strategy_matches_copy_suffix_contract() {
        let mut store = temp_store("rename");
        store
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();

        let payload = vec![EncryptedConnection {
            name: "Prod".to_string(),
            group: None,
            host: "example.org".to_string(),
            port: 22,
            username: "me".to_string(),
            auth: EncryptedAuth::Agent,
            color: None,
            icon: None,
            tags: Vec::new(),
            options: ConnectionOptions::default(),
            upstream_proxy: EncryptedUpstreamProxyPolicy::UseGlobal,
            proxy_chain: Vec::new(),
            forwards: Vec::new(),
            privilege_credentials: Vec::new(),
        }];

        let plans = plan_import(&store, &payload, ImportConflictStrategy::Rename);
        assert!(matches!(
            plans.first(),
            Some(PlannedImportAction::Rename(name)) if name == "Prod (Copy)"
        ));
    }

    #[test]
    fn replace_strategy_only_replaces_first_same_name_record() {
        let mut store = temp_store("replace-duplicate");
        store
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();

        let payload = vec![
            encrypted_agent_connection("Prod", "one.example.com"),
            encrypted_agent_connection("Prod", "two.example.com"),
        ];

        let plans = plan_import(&store, &payload, ImportConflictStrategy::Replace);
        assert!(matches!(
            plans.first(),
            Some(PlannedImportAction::Replace(_))
        ));
        assert!(matches!(
            plans.get(1),
            Some(PlannedImportAction::Rename(name)) if name == "Prod (Copy)"
        ));
    }

    #[test]
    fn export_missing_connection_id_errors_like_tauri() {
        let source = temp_store("missing-export-id");
        let error = export_connections_to_oxide(
            &source,
            &["missing".to_string()],
            "secret!",
            OxideExportOptions::default(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("Connection missing not found"));
    }

    #[test]
    fn export_quick_command_metadata_counts_are_optional_like_tauri() {
        let mut source = temp_store("quick-command-metadata");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();

        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions {
                quick_commands_json: Some(r#"{"commands":[]}"#.to_string()),
                ..OxideExportOptions::default()
            },
        )
        .unwrap();
        let file = OxideFile::from_bytes(&bytes).unwrap();

        assert_eq!(file.metadata.has_quick_commands, Some(true));
        assert_eq!(file.metadata.quick_commands_count, None);
        assert_eq!(file.metadata.quick_command_categories_count, None);
    }

    #[test]
    fn export_converts_legacy_jump_host_to_proxy_chain() {
        let mut source = temp_store("legacy-jump-export");
        let mut jump = saved_connection("jump-1", "Jump");
        jump.host = "jump.example.com".to_string();
        jump.username = "jump".to_string();
        jump.proxy_chain.clear();
        source.upsert_imported_connection(jump).unwrap();

        let mut target = saved_connection("target-1", "Target");
        target.proxy_chain.clear();
        target.options.jump_host = Some("jump-1".to_string());
        source.upsert_imported_connection(target).unwrap();

        let bytes = export_connections_to_oxide(
            &source,
            &["target-1".to_string()],
            "secret!",
            OxideExportOptions::default(),
        )
        .unwrap();
        let payload = decrypt_payload(&bytes, "secret!").unwrap();
        let exported = payload.connections.first().unwrap();

        assert_eq!(exported.proxy_chain.len(), 1);
        assert_eq!(exported.proxy_chain[0].host, "jump.example.com");
        assert_eq!(exported.proxy_chain[0].username, "jump");
        assert_eq!(exported.options.jump_host.as_deref(), Some("jump-1"));
    }

    #[test]
    fn upstream_proxy_export_import_preserves_metadata_without_secret() {
        let mut source = temp_store("upstream-proxy-source");
        let mut connection = saved_connection("conn-1", "Prod");
        connection.proxy_chain.clear();
        connection.upstream_proxy = SavedUpstreamProxyPolicy::Custom {
            proxy: SavedUpstreamProxyConfig {
                protocol: SavedUpstreamProxyProtocol::Socks5,
                host: "proxy.example.com".to_string(),
                port: 1080,
                auth: SavedUpstreamProxyAuth::Password {
                    username: "proxy-user".to_string(),
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from("proxy-secret")),
                },
                remote_dns: true,
                no_proxy: "localhost,*.internal".to_string(),
            },
        };
        source.upsert_imported_connection(connection).unwrap();

        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions::default(),
        )
        .unwrap();
        let payload = decrypt_payload(&bytes, "secret!").unwrap();
        let exported = payload.connections.first().unwrap();

        match &exported.upstream_proxy {
            EncryptedUpstreamProxyPolicy::Custom { proxy } => {
                assert_eq!(proxy.host, "proxy.example.com");
                assert!(matches!(
                    proxy.auth,
                    EncryptedUpstreamProxyAuth::Password { ref username }
                        if username == "proxy-user"
                ));
            }
            other => panic!("unexpected upstream proxy policy: {other:?}"),
        }
        let payload_json = serde_json::to_string(&payload).unwrap();
        assert!(!payload_json.contains("proxy-secret"));
        assert!(!payload_json.contains("keychain_id"));

        let mut target = temp_store("upstream-proxy-target");
        apply_oxide_import(
            &mut target,
            &bytes,
            "secret!",
            ImportConflictStrategy::Rename,
        )
        .unwrap();
        let imported = target.connections().first().unwrap();
        match &imported.upstream_proxy {
            SavedUpstreamProxyPolicy::Custom { proxy } => {
                assert_eq!(proxy.host, "proxy.example.com");
                assert!(matches!(
                    &proxy.auth,
                    SavedUpstreamProxyAuth::Password {
                        username,
                        keychain_id: None,
                        plaintext_password: None,
                    } if username == "proxy-user"
                ));
            }
            other => panic!("unexpected imported upstream proxy policy: {other:?}"),
        }
    }

    #[test]
    fn preflight_does_not_count_proxy_auth_kinds() {
        let mut source = temp_store("preflight-proxy-counts");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();

        let result = preflight_export(&source, &["conn-1".to_string()], false, true, 0);

        assert_eq!(result.connections_with_keys, 1);
        assert_eq!(result.connections_with_agent, 0);
        assert_eq!(result.connections_with_passwords, 0);
    }

    #[test]
    fn preflight_blocks_managed_key_connections_when_excluded() {
        let mut source = temp_store("preflight-managed-key-excluded");
        let managed_key = source
            .create_managed_ssh_key_from_text(
                SecretString::from(generated_private_key_text()),
                Some("Deploy key".to_string()),
                None,
            )
            .unwrap();
        let mut connection = saved_connection("conn-1", "Prod");
        connection.auth = SavedAuth::ManagedKey {
            key_id: managed_key.id,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        };
        connection.proxy_chain.clear();
        source.upsert_imported_connection(connection).unwrap();

        let result = preflight_export(&source, &["conn-1".to_string()], false, false, 0);

        assert!(!result.can_export);
        assert_eq!(result.managed_key_count, 1);
        assert_eq!(result.blocked_managed_key_connections, vec!["Prod"]);
    }

    #[test]
    fn preflight_allows_managed_key_connections_when_included() {
        let mut source = temp_store("preflight-managed-key-included");
        let managed_key = source
            .create_managed_ssh_key_from_text(
                SecretString::from(generated_private_key_text()),
                Some("Deploy key".to_string()),
                None,
            )
            .unwrap();
        let mut connection = saved_connection("conn-1", "Prod");
        connection.auth = SavedAuth::ManagedKey {
            key_id: managed_key.id,
            passphrase_keychain_id: None,
            plaintext_passphrase: None,
        };
        connection.proxy_chain.clear();
        source.upsert_imported_connection(connection).unwrap();

        let result = preflight_export(&source, &["conn-1".to_string()], false, true, 0);

        assert!(result.can_export);
        assert_eq!(result.managed_key_count, 1);
        assert!(result.blocked_managed_key_connections.is_empty());
    }

    #[test]
    fn import_options_filter_selection_and_skip_forward_persistence() {
        let mut source = temp_store("selected-source");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();
        source
            .upsert_imported_connection(saved_connection("conn-2", "Staging"))
            .unwrap();
        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string(), "conn-2".to_string()],
            "secret!",
            OxideExportOptions {
                forwards: vec![
                    OxideForwardRecord {
                        id: Some("forward-prod".to_string()),
                        connection_id: "conn-1".to_string(),
                        forward_type: "local".to_string(),
                        bind_address: "127.0.0.1".to_string(),
                        bind_port: 8080,
                        target_host: "127.0.0.1".to_string(),
                        target_port: 80,
                        description: Some("prod".to_string()),
                        auto_start: true,
                    },
                    OxideForwardRecord {
                        id: Some("forward-staging".to_string()),
                        connection_id: "conn-2".to_string(),
                        forward_type: "remote".to_string(),
                        bind_address: "127.0.0.1".to_string(),
                        bind_port: 9090,
                        target_host: "127.0.0.1".to_string(),
                        target_port: 90,
                        description: Some("staging".to_string()),
                        auto_start: false,
                    },
                ],
                ..OxideExportOptions::default()
            },
        )
        .unwrap();

        let mut target = temp_store("selected-target");
        let result = apply_oxide_import_with_options(
            &mut target,
            &bytes,
            "secret!",
            OxideImportOptions {
                selected_names: Some(vec!["Prod".to_string()]),
                import_forwards: false,
                ..OxideImportOptions::default()
            },
        )
        .unwrap();

        assert_eq!(result.imported, 1);
        assert_eq!(target.connections().len(), 1);
        assert_eq!(target.connections()[0].name, "Prod");
        assert_eq!(result.imported_forwards, 0);
        assert_eq!(result.skipped_forwards, 1);
        assert!(result.forward_records.is_empty());
    }

    #[test]
    fn renamed_import_counts_as_imported_like_tauri() {
        let mut source = temp_store("rename-count-source");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();
        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions::default(),
        )
        .unwrap();

        let mut target = temp_store("rename-count-target");
        target
            .upsert_imported_connection(saved_connection("existing", "Prod"))
            .unwrap();
        let result = apply_oxide_import(
            &mut target,
            &bytes,
            "secret!",
            ImportConflictStrategy::Rename,
        )
        .unwrap();

        assert_eq!(result.imported, 1);
        assert_eq!(result.renamed, 1);
        assert_eq!(target.connections().len(), 2);
        assert!(
            target
                .connections()
                .iter()
                .any(|connection| connection.name == "Prod (Copy)")
        );
    }

    #[test]
    fn replace_and_merge_import_report_forward_owner_operations() {
        let mut source = temp_store("forward-op-source");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();
        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions {
                forwards: vec![OxideForwardRecord {
                    id: Some("forward-1".to_string()),
                    connection_id: "conn-1".to_string(),
                    forward_type: "local".to_string(),
                    bind_address: "127.0.0.1".to_string(),
                    bind_port: 8080,
                    target_host: "127.0.0.1".to_string(),
                    target_port: 80,
                    description: None,
                    auto_start: true,
                }],
                ..OxideExportOptions::default()
            },
        )
        .unwrap();

        let mut replace_target = temp_store("forward-op-replace");
        replace_target
            .upsert_imported_connection(saved_connection("existing", "Prod"))
            .unwrap();
        let replaced = apply_oxide_import(
            &mut replace_target,
            &bytes,
            "secret!",
            ImportConflictStrategy::Replace,
        )
        .unwrap();
        assert_eq!(
            replaced.forward_replace_owner_ids,
            vec!["existing".to_string()]
        );
        assert!(replaced.forward_merge_owner_ids.is_empty());

        let mut merge_target = temp_store("forward-op-merge");
        merge_target
            .upsert_imported_connection(saved_connection("existing", "Prod"))
            .unwrap();
        let merged = apply_oxide_import(
            &mut merge_target,
            &bytes,
            "secret!",
            ImportConflictStrategy::Merge,
        )
        .unwrap();
        assert_eq!(merged.forward_merge_owner_ids, vec!["existing".to_string()]);
        assert!(merged.forward_replace_owner_ids.is_empty());
    }

    #[test]
    fn import_portable_secrets_default_skip_and_opt_in_import() {
        let mut source = temp_store("portable-source");
        source
            .upsert_imported_connection(saved_connection("conn-1", "Prod"))
            .unwrap();
        let bytes = export_connections_to_oxide(
            &source,
            &["conn-1".to_string()],
            "secret!",
            OxideExportOptions {
                portable_secrets: vec![EncryptedPortableSecret {
                    kind: "ai_provider_key".to_string(),
                    id: "deepseek".to_string(),
                    secret: Zeroizing::new("sk-test".to_string()),
                }],
                ..OxideExportOptions::default()
            },
        )
        .unwrap();

        let mut default_target = temp_store("portable-default-target");
        let skipped = apply_oxide_import_with_options(
            &mut default_target,
            &bytes,
            "secret!",
            OxideImportOptions::default(),
        )
        .unwrap();
        assert_eq!(skipped.imported_portable_secrets, 0);
        assert_eq!(skipped.skipped_portable_secrets, 1);
        assert!(skipped.portable_secrets.is_empty());

        let mut opt_in_target = temp_store("portable-opt-in-target");
        let imported = apply_oxide_import_with_options(
            &mut opt_in_target,
            &bytes,
            "secret!",
            OxideImportOptions {
                import_portable_secrets: true,
                ..OxideImportOptions::default()
            },
        )
        .unwrap();
        assert_eq!(imported.imported_portable_secrets, 1);
        assert_eq!(imported.skipped_portable_secrets, 0);
        assert_eq!(imported.portable_secrets.len(), 1);
    }

    fn encrypted_agent_connection(name: &str, host: &str) -> EncryptedConnection {
        EncryptedConnection {
            name: name.to_string(),
            group: None,
            host: host.to_string(),
            port: 22,
            username: "me".to_string(),
            auth: EncryptedAuth::Agent,
            color: None,
            icon: None,
            tags: Vec::new(),
            options: ConnectionOptions::default(),
            upstream_proxy: EncryptedUpstreamProxyPolicy::UseGlobal,
            proxy_chain: Vec::new(),
            forwards: Vec::new(),
            privilege_credentials: Vec::new(),
        }
    }
}
