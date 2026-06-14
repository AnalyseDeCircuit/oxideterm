mod tests {
    use std::{fs, path::PathBuf};

    use rand10::{rand_core::UnwrapErr, rngs::SysRng};
    use russh::keys::ssh_key::{HashAlg, LineEnding};
    use russh::keys::{Algorithm, PrivateKey};

    use super::*;

    fn temp_store_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "oxideterm-connection-store-{name}-{}.json",
            Uuid::new_v4()
        ))
    }

    fn request(id: &str, auth: SavedAuth) -> SaveConnectionRequest {
        SaveConnectionRequest {
            id: Some(id.to_string()),
            name: "Home".to_string(),
            group: None,
            host: "192.168.1.2".to_string(),
            port: 22,
            username: "me".to_string(),
            auth,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            color: None,
            tags: Vec::new(),
            agent_forwarding: false,
            post_connect_command: None,
        }
    }

    fn load_empty_store(name: &str) -> ConnectionStore {
        ConnectionStore::load(temp_store_path(name)).expect("store should load")
    }

    fn generated_private_key_text(passphrase: Option<&str>) -> String {
        let key_path = temp_store_path("managed-key-source").with_extension("key");
        let mut rng = UnwrapErr(SysRng);
        let key = PrivateKey::random(&mut rng, Algorithm::Ed25519).unwrap();
        let key = match passphrase {
            Some(passphrase) => key.encrypt(&mut rng, passphrase).unwrap(),
            None => key,
        };
        key.write_openssh_file(&key_path, LineEnding::LF).unwrap();
        let private_key = fs::read_to_string(&key_path).unwrap();
        let _ = fs::remove_file(key_path);
        private_key
    }

    fn generated_large_rsa_private_key_text() -> String {
        let key_path = temp_store_path("managed-key-large-rsa-source").with_extension("key");
        let mut rng = UnwrapErr(SysRng);
        let key = PrivateKey::random(
            &mut rng,
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha256),
            },
        )
        .unwrap();
        key.write_openssh_file(&key_path, LineEnding::LF).unwrap();
        let private_key = fs::read_to_string(&key_path).unwrap();
        let _ = fs::remove_file(key_path);
        private_key
    }

    #[test]
    fn password_is_saved_to_keychain_reference() {
        let mut store = load_empty_store("password-save");

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from("secret")),
                },
            ))
            .unwrap();

        let conn = store.get("conn-1").unwrap();
        match &conn.auth {
            SavedAuth::Password {
                keychain_id: Some(_),
                plaintext_password: None,
            } => {}
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(store.get_connection_password("conn-1").unwrap(), "secret");
    }

    #[test]
    fn empty_password_is_saved_to_keychain_reference() {
        let mut store = load_empty_store("password-save-empty");

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::default()),
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(_),
                plaintext_password: None,
            } => {}
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(store.get_connection_password("conn-1").unwrap(), "");
    }

    #[test]
    fn password_auth_without_secret_keeps_no_keychain_reference() {
        let mut store = load_empty_store("password-no-save");

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: None,
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            } => {}
            other => panic!("unexpected auth: {other:?}"),
        }
        assert!(store.get_connection_password("conn-1").is_err());
    }

    #[test]
    fn loaded_empty_password_updates_existing_keychain_entry() {
        let mut store = load_empty_store("password-clear");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from("secret")),
                },
            ))
            .unwrap();
        let previous_keychain_id = match &store.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: Some(previous_keychain_id.clone()),
                    plaintext_password: Some(SecretString::default()),
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
            } => assert_eq!(keychain_id, &previous_keychain_id),
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(store.get_connection_password("conn-1").unwrap(), "");
    }

    #[test]
    fn unloaded_password_preserves_saved_keychain_entry() {
        let mut store = load_empty_store("password-preserve");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from("secret")),
                },
            ))
            .unwrap();
        let previous_auth = store.get("conn-1").unwrap().auth.clone();

        store.upsert(request("conn-1", previous_auth)).unwrap();

        assert_eq!(store.get_connection_password("conn-1").unwrap(), "secret");
    }

    #[test]
    fn saved_connection_store_writes_tauri_compatible_versions() {
        let path = temp_store_path("versioned-store");
        let mut store = ConnectionStore::load(&path).unwrap();

        store
            .upsert(request("conn-1", SavedAuth::Agent))
            .expect("connection saved");

        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("\"version\": 1"));
        assert_eq!(store.get("conn-1").unwrap().version, CONFIG_VERSION);
    }

    #[test]
    fn encrypted_tauri_connections_payload_round_trips() {
        let mut data = ConnectionStoreData::default();
        data.groups.push("Work".to_string());
        data.recent.push("conn-1".to_string());
        data.connections.push(SavedConnection {
            id: "conn-1".to_string(),
            version: CONFIG_VERSION,
            name: "Work Host".to_string(),
            group: Some("Work".to_string()),
            host: "work.example.com".to_string(),
            port: 22,
            username: "me".to_string(),
            auth: SavedAuth::Agent,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            options: ConnectionOptions {
                post_connect_command: Some("uptime".to_string()),
                ..ConnectionOptions::default()
            },
            created_at: "2026-01-01T00:00:00Z".parse().unwrap(),
            last_used_at: None,
            updated_at: None,
            color: None,
            tags: Vec::new(),
            post_connect_command: None,
            privilege_credentials: Vec::new(),
        });

        let key = [7u8; CONFIG_ENCRYPTION_KEY_LEN];
        let bytes = encode_encrypted_connection_store_data_for_tests(&data, &key);
        let raw = String::from_utf8(bytes.clone()).unwrap();
        assert!(raw.contains("\"format\": \"oxideterm.config.encrypted\""));
        assert!(!raw.contains("Work Host"));

        let loaded = decode_connection_store_data_for_tests(&bytes, &key).unwrap();
        assert_eq!(loaded.format, ConnectionStoreStorageFormat::Encrypted);
        assert_eq!(loaded.data.connections.len(), 1);
        assert_eq!(loaded.data.groups, vec!["Work"]);
        assert_eq!(loaded.data.recent, vec!["conn-1"]);
        assert_eq!(
            loaded.data.connections[0]
                .options
                .post_connect_command
                .as_deref(),
            Some("uptime")
        );
    }

    #[test]
    fn plaintext_load_preserves_recent_and_moves_post_connect_command_to_tauri_options() {
        let path = temp_store_path("tauri-options-command");
        fs::write(
            &path,
            r##"{
              "version": 1,
              "connections": [
                {
                  "id": "conn-1",
                  "version": 1,
                  "name": "Home",
                  "host": "192.168.1.2",
                  "port": 22,
                  "username": "me",
                  "auth": { "type": "agent" },
                  "options": {},
                  "created_at": "2026-01-01T00:00:00Z",
                  "post_connect_command": "echo ready"
                }
              ],
              "groups": [],
              "recent": ["conn-1"]
            }"##,
        )
        .unwrap();

        let store = ConnectionStore::load(&path).unwrap();

        assert_eq!(store.data.recent, vec!["conn-1"]);
        assert_eq!(
            store.get("conn-1").unwrap().options.post_connect_command.as_deref(),
            Some("echo ready")
        );
        store.save().unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        let saved_json: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved_json["recent"], serde_json::json!(["conn-1"]));
        let saved_connection = &saved_json["connections"][0];
        assert!(saved_connection.get("post_connect_command").is_none());
        assert_eq!(
            saved_connection["options"]["post_connect_command"],
            serde_json::json!("echo ready")
        );
    }

    #[test]
    fn edit_preserves_tauri_connection_options_and_marks_used() {
        let path = temp_store_path("preserve-options");
        fs::write(
            &path,
            r##"{
              "version": 1,
              "connections": [
                {
                  "id": "conn-1",
                  "version": 1,
                  "name": "Home",
                  "host": "192.168.1.2",
                  "port": 22,
                  "username": "me",
                  "auth": { "type": "agent" },
                  "options": {
                    "keep_alive_interval": 45,
                    "compression": true,
                    "jump_host": "legacy-jump",
                    "term_type": "vt100",
                    "agent_forwarding": false
                  },
                  "created_at": "2026-01-01T00:00:00Z"
                }
              ],
              "groups": []
            }"##,
        )
        .unwrap();
        let mut store = ConnectionStore::load(&path).unwrap();

        let mut update = request("conn-1", SavedAuth::Agent);
        update.name = "Home Edited".to_string();
        update.agent_forwarding = true;
        store.upsert(update).unwrap();

        let conn = store.get("conn-1").unwrap();
        assert_eq!(conn.options.keep_alive_interval, 45);
        assert!(conn.options.compression);
        assert_eq!(conn.options.jump_host.as_deref(), Some("legacy-jump"));
        assert_eq!(conn.options.term_type.as_deref(), Some("vt100"));
        assert!(conn.options.agent_forwarding);
        assert!(conn.last_used_at.is_some());
    }

    #[test]
    fn legacy_plaintext_password_and_passphrase_are_migrated() {
        let path = temp_store_path("legacy-migration");
        fs::write(
            &path,
            r##"{
              "connections": [
                {
                  "id": "conn-1",
                  "name": "Home",
                  "host": "192.168.1.2",
                  "port": 22,
                  "username": "me",
                  "auth": { "type": "password", "password": "secret" },
                  "created_at": "2026-01-01T00:00:00Z"
                },
                {
                  "id": "conn-2",
                  "name": "Key",
                  "host": "192.168.1.3",
                  "port": 22,
                  "username": "me",
                  "auth": { "type": "key", "key_path": "/tmp/id", "passphrase": "key-secret" },
                  "created_at": "2026-01-01T00:00:00Z"
                }
              ],
              "groups": []
            }"##,
        )
        .unwrap();

        let store = ConnectionStore::load(&path).unwrap();

        assert_eq!(store.get_connection_password("conn-1").unwrap(), "secret");
        assert_eq!(
            store.get_connection_passphrase("conn-2").unwrap(),
            Some(SecretString::from("key-secret"))
        );
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("\"keychain_id\""));
        assert!(saved.contains("\"passphrase_keychain_id\""));
        assert!(!saved.contains("\"password\": \"secret\""));
        assert!(!saved.contains("\"passphrase\": \"key-secret\""));
    }

    #[test]
    fn unchanged_key_path_preserves_passphrase_keychain_entry() {
        let mut store = load_empty_store("key-preserve");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Key {
                    key_path: "/tmp/id".to_string(),
                    has_passphrase: true,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: Some(SecretString::from("key-secret")),
                },
            ))
            .unwrap();
        let previous_keychain_id = match &store.get("conn-1").unwrap().auth {
            SavedAuth::Key {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Key {
                    key_path: "/tmp/id".to_string(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: None,
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Key {
                has_passphrase,
                passphrase_keychain_id: Some(keychain_id),
                plaintext_passphrase: None,
                ..
            } => {
                assert!(*has_passphrase);
                assert_eq!(keychain_id, &previous_keychain_id);
            }
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(
            store.get_connection_passphrase("conn-1").unwrap(),
            Some(SecretString::from("key-secret"))
        );
    }

    #[test]
    fn changed_key_path_without_passphrase_clears_passphrase_reference() {
        let mut store = load_empty_store("key-clear");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Key {
                    key_path: "/tmp/id".to_string(),
                    has_passphrase: true,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: Some(SecretString::from("key-secret")),
                },
            ))
            .unwrap();
        let previous_keychain_id = match &store.get("conn-1").unwrap().auth {
            SavedAuth::Key {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Key {
                    key_path: "/tmp/id-new".to_string(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: None,
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            } => {
                assert_eq!(key_path, "/tmp/id-new");
                assert!(!*has_passphrase);
            }
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(store.get_connection_passphrase("conn-1").unwrap(), None);
        assert!(store.keychain.get(&previous_keychain_id).is_err());
    }

    #[test]
    fn unchanged_certificate_paths_preserve_passphrase_keychain_entry() {
        let mut store = load_empty_store("cert-preserve");
        store
            .upsert(request(
                "conn-1",
                SavedAuth::Certificate {
                    key_path: "/tmp/id".to_string(),
                    cert_path: "/tmp/id-cert.pub".to_string(),
                    has_passphrase: true,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: Some(SecretString::from("cert-secret")),
                },
            ))
            .unwrap();

        store
            .upsert(request(
                "conn-1",
                SavedAuth::Certificate {
                    key_path: "/tmp/id".to_string(),
                    cert_path: "/tmp/id-cert.pub".to_string(),
                    has_passphrase: false,
                    passphrase_keychain_id: None,
                    plaintext_passphrase: None,
                },
            ))
            .unwrap();

        match &store.get("conn-1").unwrap().auth {
            SavedAuth::Certificate {
                has_passphrase,
                passphrase_keychain_id: Some(_),
                plaintext_passphrase: None,
                ..
            } => assert!(*has_passphrase),
            other => panic!("unexpected auth: {other:?}"),
        }
        assert_eq!(
            store.get_connection_passphrase("conn-1").unwrap(),
            Some(SecretString::from("cert-secret"))
        );
    }

    #[test]
    fn proxy_hop_password_is_saved_to_keychain_reference() {
        let mut store = load_empty_store("proxy-hop-password");
        let mut req = request("conn-1", SavedAuth::Agent);
        req.proxy_chain = vec![SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 2222,
            username: "ops".to_string(),
            auth: SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(SecretString::from("jump-secret")),
            },
            agent_forwarding: true,
        }];

        store.upsert(req).unwrap();

        let hop = &store.get("conn-1").unwrap().proxy_chain[0];
        assert!(hop.agent_forwarding);
        match &hop.auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
            } => assert_eq!(store.keychain.get(keychain_id).unwrap(), "jump-secret"),
            other => panic!("unexpected proxy auth: {other:?}"),
        }
    }

    #[test]
    fn upstream_proxy_password_is_saved_to_keychain_reference() {
        let mut store = load_empty_store("upstream-proxy-password");
        let path = store.path().to_path_buf();
        let mut req = request("conn-1", SavedAuth::Agent);
        req.upstream_proxy = SavedUpstreamProxyPolicy::Custom {
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
                no_proxy: "localhost,127.0.0.1".to_string(),
            },
        };

        store.upsert(req).unwrap();

        let conn = store.get("conn-1").unwrap();
        let SavedUpstreamProxyPolicy::Custom { proxy } = &conn.upstream_proxy else {
            panic!("expected custom upstream proxy policy");
        };
        match &proxy.auth {
            SavedUpstreamProxyAuth::Password {
                username,
                keychain_id: Some(keychain_id),
                plaintext_password: None,
            } => {
                assert_eq!(username, "proxy-user");
                assert_eq!(store.keychain.get(keychain_id).unwrap(), "proxy-secret");
            }
            other => panic!("unexpected upstream proxy auth: {other:?}"),
        }

        let saved = fs::read_to_string(path).unwrap();
        assert!(saved.contains("proxy.example.com"));
        assert!(saved.contains("proxy-user"));
        assert!(saved.contains("keychain_id"));
        assert!(!saved.contains("proxy-secret"));
        assert!(!saved.contains("Proxy-Authorization"));
    }

    #[test]
    fn deleting_connection_removes_main_and_proxy_keychain_entries() {
        let mut store = load_empty_store("delete-cleans-secrets");
        let mut req = request(
            "conn-1",
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(SecretString::from("target-secret")),
            },
        );
        req.proxy_chain = vec![SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(SecretString::from("jump-secret")),
            },
            agent_forwarding: false,
        }];
        store.upsert(req).unwrap();

        let conn = store.get("conn-1").unwrap();
        let target_keychain_id = match &conn.auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected target auth: {other:?}"),
        };
        let proxy_keychain_id = match &conn.proxy_chain[0].auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected proxy auth: {other:?}"),
        };

        assert!(store.delete("conn-1").unwrap());

        assert!(store.keychain.get(&target_keychain_id).is_err());
        assert!(store.keychain.get(&proxy_keychain_id).is_err());
    }

    #[test]
    fn privilege_credential_secret_is_stored_outside_connection_json() {
        let mut store = load_empty_store("privilege-save");
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();

        let credential = store
            .save_privilege_credential(SavePrivilegeCredentialRequest {
                connection_id: "conn-1".to_string(),
                credential_id: None,
                label: "sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: Some("root".to_string()),
                prompt_patterns: Vec::new(),
                secret: Some(SecretString::from("sudo-secret")),
                enabled: true,
                require_click_to_send: true,
            })
            .unwrap();

        assert_eq!(
            store
                .get_privilege_credential_secret("conn-1", &credential.id)
                .unwrap(),
            SecretString::from("sudo-secret")
        );
        let saved = fs::read_to_string(store.path()).unwrap();
        assert!(saved.contains("\"privilege_credentials\""));
        assert!(!saved.contains("sudo-secret"));
    }

    #[test]
    fn sudo_privilege_credential_uses_tauri_default_prompt_fragments() {
        let mut store = load_empty_store("privilege-default-sudo-patterns");
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();

        let credential = store
            .save_privilege_credential(SavePrivilegeCredentialRequest {
                connection_id: "conn-1".to_string(),
                credential_id: Some("cred-1".to_string()),
                label: "sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: None,
                prompt_patterns: Vec::new(),
                secret: Some(SecretString::from("sudo-secret")),
                enabled: true,
                require_click_to_send: true,
            })
            .unwrap();

        // Prompt patterns are substring fragments, not glob patterns. Keep the
        // defaults broad enough to match Tauri's helper behavior.
        assert_eq!(
            credential.prompt_patterns,
            vec![
                "[sudo]".to_string(),
                "password for".to_string(),
                "的密码".to_string(),
                "sudo password".to_string()
            ]
        );
    }

    #[test]
    fn legacy_sudo_privilege_prompt_fragments_are_displayed_as_current_defaults() {
        let mut store = load_empty_store("privilege-legacy-sudo-patterns");
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        let now = Utc::now();
        store
            .privilege_credentials_for_scope_mut("conn-1")
            .unwrap()
            .push(SavedPrivilegeCredential {
                id: "cred-legacy".to_string(),
                connection_id: "conn-1".to_string(),
                label: "sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: None,
                prompt_patterns: vec![
                    "[sudo] password for".to_string(),
                    "sudo password".to_string(),
                ],
                keychain_id: None,
                plaintext_secret: None,
                enabled: true,
                require_click_to_send: true,
                created_at: now,
                updated_at: now,
            });

        let credentials = store.list_privilege_credentials("conn-1").unwrap();
        assert_eq!(
            credentials[0].prompt_patterns,
            vec![
                "[sudo]".to_string(),
                "password for".to_string(),
                "的密码".to_string(),
                "sudo password".to_string()
            ]
        );
    }

    #[test]
    fn local_shell_privilege_credential_uses_dedicated_scope() {
        let mut store = load_empty_store("privilege-local-shell");

        let credential = store
            .save_privilege_credential(SavePrivilegeCredentialRequest {
                connection_id: LOCAL_SHELL_PRIVILEGE_CONNECTION_ID.to_string(),
                credential_id: Some("local-sudo".to_string()),
                label: "local sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: Some("deploy".to_string()),
                prompt_patterns: Vec::new(),
                secret: Some(SecretString::from("local-secret")),
                enabled: true,
                require_click_to_send: true,
            })
            .unwrap();

        assert_eq!(credential.connection_id, LOCAL_SHELL_PRIVILEGE_CONNECTION_ID);
        assert_eq!(
            store
                .list_privilege_credentials(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
                .unwrap(),
            vec![credential.clone()]
        );
        assert_eq!(
            store
                .get_privilege_credential_secret(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID, "local-sudo")
                .unwrap(),
            SecretString::from("local-secret")
        );
        assert!(store.get("local-shell:default").is_none());
    }

    #[test]
    fn privilege_credential_metadata_update_preserves_existing_secret() {
        let mut store = load_empty_store("privilege-metadata-update");
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();

        let credential = store
            .save_privilege_credential(SavePrivilegeCredentialRequest {
                connection_id: "conn-1".to_string(),
                credential_id: Some("cred-1".to_string()),
                label: "sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: Some("deploy".to_string()),
                prompt_patterns: Vec::new(),
                secret: Some(SecretString::from("sudo-secret")),
                enabled: true,
                require_click_to_send: true,
            })
            .unwrap();
        let keychain_id = credential.keychain_id.clone();

        let updated = store
            .save_privilege_credential(SavePrivilegeCredentialRequest {
                connection_id: "conn-1".to_string(),
                credential_id: Some("cred-1".to_string()),
                label: "renamed sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: Some("deploy".to_string()),
                prompt_patterns: Vec::new(),
                secret: None,
                enabled: true,
                require_click_to_send: true,
            })
            .unwrap();

        assert_eq!(updated.keychain_id, keychain_id);
        assert_eq!(
            store
                .get_privilege_credential_secret("conn-1", "cred-1")
                .unwrap(),
            SecretString::from("sudo-secret")
        );
    }

    #[test]
    fn privilege_credential_request_debug_redacts_secret() {
        let request = SavePrivilegeCredentialRequest {
            connection_id: "conn-1".to_string(),
            credential_id: Some("cred-1".to_string()),
            label: "sudo".to_string(),
            kind: PrivilegeCredentialKind::SudoPassword,
            username_hint: None,
            prompt_patterns: Vec::new(),
            secret: Some(SecretString::from("sudo-secret")),
            enabled: true,
            require_click_to_send: true,
        };

        let debug = format!("{request:?}");

        assert!(debug.contains("[redacted secret]"));
        assert!(!debug.contains("sudo-secret"));
    }

    #[test]
    fn deleting_connection_removes_privilege_keychain_entries() {
        let mut store = load_empty_store("privilege-delete");
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        let credential = store
            .save_privilege_credential(SavePrivilegeCredentialRequest {
                connection_id: "conn-1".to_string(),
                credential_id: Some("cred-1".to_string()),
                label: "sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: None,
                prompt_patterns: Vec::new(),
                secret: Some(SecretString::from("sudo-secret")),
                enabled: true,
                require_click_to_send: true,
            })
            .unwrap();
        let keychain_id = credential.keychain_id.clone().unwrap();

        assert!(store.delete("conn-1").unwrap());
        assert!(store.privilege_keychain.get(&keychain_id).is_err());
    }

    #[test]
    fn duplicated_connection_does_not_copy_privilege_credentials() {
        let mut store = load_empty_store("privilege-duplicate");
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        store
            .save_privilege_credential(SavePrivilegeCredentialRequest {
                connection_id: "conn-1".to_string(),
                credential_id: Some("cred-1".to_string()),
                label: "sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: None,
                prompt_patterns: Vec::new(),
                secret: Some(SecretString::from("sudo-secret")),
                enabled: true,
                require_click_to_send: true,
            })
            .unwrap();

        let duplicate = store.duplicate("conn-1").unwrap().unwrap();

        assert!(
            store
                .get(&duplicate.id)
                .unwrap()
                .privilege_credentials
                .is_empty()
        );
    }

    #[test]
    fn explicit_proxy_hop_key_update_without_passphrase_clears_old_keychain_entry() {
        let mut store = load_empty_store("proxy-hop-passphrase-clear");
        let mut req = request("conn-1", SavedAuth::Agent);
        req.proxy_chain = vec![SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: SavedAuth::Key {
                key_path: "/tmp/jump-key".to_string(),
                has_passphrase: true,
                passphrase_keychain_id: None,
                plaintext_passphrase: Some(SecretString::from("jump-key-secret")),
            },
            agent_forwarding: false,
        }];
        store.upsert(req).unwrap();
        let previous_keychain_id = match &store.get("conn-1").unwrap().proxy_chain[0].auth {
            SavedAuth::Key {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected proxy auth: {other:?}"),
        };

        let mut update = request("conn-1", SavedAuth::Agent);
        update.proxy_chain = vec![SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: SavedAuth::Key {
                key_path: "/tmp/jump-key".to_string(),
                has_passphrase: false,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            },
            agent_forwarding: false,
        }];
        store.upsert(update).unwrap();

        match &store.get("conn-1").unwrap().proxy_chain[0].auth {
            SavedAuth::Key {
                has_passphrase,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
                ..
            } => assert!(!*has_passphrase),
            other => panic!("unexpected proxy auth: {other:?}"),
        }
        assert!(store.keychain.get(&previous_keychain_id).is_err());
    }

    #[test]
    fn copied_existing_proxy_hop_preserves_passphrase_keychain_entry() {
        let mut store = load_empty_store("proxy-hop-passphrase-preserve");
        let mut req = request("conn-1", SavedAuth::Agent);
        req.proxy_chain = vec![SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: SavedAuth::Key {
                key_path: "/tmp/jump-key".to_string(),
                has_passphrase: true,
                passphrase_keychain_id: None,
                plaintext_passphrase: Some(SecretString::from("jump-key-secret")),
            },
            agent_forwarding: false,
        }];
        store.upsert(req).unwrap();
        let existing_hop = store.get("conn-1").unwrap().proxy_chain[0].clone();
        let previous_keychain_id = match &existing_hop.auth {
            SavedAuth::Key {
                passphrase_keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected proxy auth: {other:?}"),
        };

        let mut update = request("conn-1", SavedAuth::Agent);
        update.proxy_chain = vec![existing_hop];
        store.upsert(update).unwrap();

        match &store.get("conn-1").unwrap().proxy_chain[0].auth {
            SavedAuth::Key {
                has_passphrase,
                passphrase_keychain_id: Some(keychain_id),
                plaintext_passphrase: None,
                ..
            } => {
                assert!(*has_passphrase);
                assert_eq!(keychain_id, &previous_keychain_id);
            }
            other => panic!("unexpected proxy auth: {other:?}"),
        }
        assert_eq!(
            store.keychain.get(&previous_keychain_id).unwrap(),
            SecretString::from("jump-key-secret")
        );
    }

    #[test]
    fn imported_connection_transaction_rolls_back_staged_config_on_later_error() {
        let mut store = load_empty_store("import-transaction-rollback");
        let good = SavedConnection {
            id: "good".to_string(),
            version: CONFIG_VERSION,
            name: "Good".to_string(),
            group: None,
            host: "good.example.com".to_string(),
            port: 22,
            username: "me".to_string(),
            auth: SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(SecretString::from("secret")),
            },
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            options: ConnectionOptions::default(),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            updated_at: None,
            color: None,
            tags: Vec::new(),
            post_connect_command: None,
            privilege_credentials: Vec::new(),
        };
        let mut bad = good.clone();
        bad.id = "bad".to_string();
        bad.name = "Bad".to_string();
        bad.host.clear();

        let result = store.upsert_imported_connections_transaction(vec![good, bad]);

        assert!(result.is_err());
        assert!(store.connections().is_empty());
    }

    #[test]
    fn saved_connection_sync_snapshot_exports_delete_tombstones() {
        let mut store = load_empty_store("sync-tombstone-export");
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        store.delete("conn-1").unwrap();

        let snapshot = store.export_saved_connections_snapshot().unwrap();

        assert_eq!(snapshot.records.len(), 1);
        let record = &snapshot.records[0];
        assert_eq!(record.id, "conn-1");
        assert!(record.deleted);
        assert!(record.payload.is_none());
        assert!(!record.revision.is_empty());
    }

    #[test]
    fn saved_connection_sync_apply_delete_removes_connection() {
        let mut target = load_empty_store("sync-delete-target");
        target.upsert(request("conn-1", SavedAuth::Agent)).unwrap();

        let mut source = load_empty_store("sync-delete-source");
        source.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        source.delete("conn-1").unwrap();
        let snapshot = source.export_saved_connections_snapshot().unwrap();

        let outcome = target
            .apply_saved_connections_snapshot(snapshot, SavedConnectionsConflictStrategy::Merge)
            .unwrap();

        assert_eq!(outcome.result.applied, 1);
        assert_eq!(outcome.deleted_connection_ids, vec!["conn-1".to_string()]);
        assert!(target.get("conn-1").is_none());
    }

    #[test]
    fn saved_connection_sync_apply_skip_reports_name_conflict() {
        let mut source = load_empty_store("sync-name-source");
        let mut source_req = request("remote-id", SavedAuth::Agent);
        source_req.name = "Shared".to_string();
        source.upsert(source_req).unwrap();
        let snapshot = source.export_saved_connections_snapshot().unwrap();

        let mut target = load_empty_store("sync-name-target");
        let mut target_req = request("local-id", SavedAuth::Agent);
        target_req.name = "Shared".to_string();
        target.upsert(target_req).unwrap();
        let outcome = target
            .apply_saved_connections_snapshot(snapshot, SavedConnectionsConflictStrategy::Skip)
            .unwrap();

        assert_eq!(outcome.result.applied, 0);
        assert_eq!(outcome.result.skipped, 1);
        assert_eq!(outcome.result.conflicts, 1);
        assert!(target.get("local-id").is_some());
        assert!(target.get("remote-id").is_none());
    }

    #[test]
    fn connection_store_data_deserializes_missing_managed_keys_as_empty() {
        let data: ConnectionStoreData = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "connections": [],
            "groups": [],
            "recent": []
        }))
        .unwrap();

        assert!(data.managed_ssh_keys.is_empty());
    }

    #[test]
    fn connection_store_data_deserializes_missing_serial_profiles_as_empty() {
        let data: ConnectionStoreData = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "connections": [],
            "groups": [],
            "recent": []
        }))
        .unwrap();

        assert!(data.serial_profiles.is_empty());
        assert!(data.telnet_profiles.is_empty());
        assert!(data.connections.is_empty());
    }

    #[test]
    fn serial_profile_metadata_round_trips_without_ssh_fields() {
        let now = Utc::now();
        let profile = SerialProfile {
            id: "serial-1".to_string(),
            name: "Lab console".to_string(),
            group: Some("Lab".to_string()),
            port_path: "/dev/cu.usbserial-1".to_string(),
            baud_rate: 115_200,
            data_bits: 8,
            stop_bits: 1,
            parity: SerialParity::None,
            flow_control: SerialFlowControl::Hardware,
            connect_on_open: true,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        };
        let data = ConnectionStoreData {
            serial_profiles: vec![profile.clone()],
            ..ConnectionStoreData::default()
        };

        let value = serde_json::to_value(&data).unwrap();

        assert_eq!(value["serial_profiles"][0]["id"], "serial-1");
        assert_eq!(value["serial_profiles"][0]["flow_control"], "hardware");
        assert!(value["serial_profiles"][0].get("host").is_none());
        assert!(value["serial_profiles"][0].get("username").is_none());
        assert!(value["serial_profiles"][0].get("auth").is_none());

        let round_trip: ConnectionStoreData = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.serial_profiles, vec![profile]);
        assert!(round_trip.connections.is_empty());
    }

    #[test]
    fn telnet_profile_metadata_round_trips_without_ssh_fields() {
        let now = Utc::now();
        let profile = TelnetProfile {
            id: "telnet-1".to_string(),
            name: "Router console".to_string(),
            group: Some("Lab".to_string()),
            host: "192.168.1.1".to_string(),
            port: 23,
            connect_on_open: true,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        };
        let data = ConnectionStoreData {
            telnet_profiles: vec![profile.clone()],
            ..ConnectionStoreData::default()
        };

        let value = serde_json::to_value(&data).unwrap();

        assert_eq!(value["telnet_profiles"][0]["id"], "telnet-1");
        assert_eq!(value["telnet_profiles"][0]["host"], "192.168.1.1");
        assert!(value["telnet_profiles"][0].get("username").is_none());
        assert!(value["telnet_profiles"][0].get("auth").is_none());
        assert!(value["telnet_profiles"][0].get("proxy_chain").is_none());

        let round_trip: ConnectionStoreData = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.telnet_profiles, vec![profile]);
        assert!(round_trip.connections.is_empty());
    }

    #[test]
    fn telnet_profile_validation_rejects_missing_identity_or_host() {
        let mut profile = TelnetProfile::new("Router console", "192.168.1.1", 23);
        assert!(profile.validate().is_ok());

        profile.name.clear();
        assert!(profile.validate().is_err());

        profile.name = "Router console".to_string();
        profile.host.clear();
        assert!(profile.validate().is_err());

        profile.host = "192.168.1.1".to_string();
        profile.id.clear();
        assert!(profile.validate().is_err());
    }

    #[test]
    fn serial_profile_validation_rejects_invalid_parameters() {
        let mut profile = SerialProfile::new("Lab console", "/dev/cu.usbserial-1");
        assert!(profile.validate().is_ok());

        profile.data_bits = 9;
        assert!(profile.validate().is_err());

        profile.data_bits = 8;
        profile.stop_bits = 3;
        assert!(profile.validate().is_err());

        profile.stop_bits = 1;
        profile.baud_rate = 0;
        assert!(profile.validate().is_err());

        profile.baud_rate = 115_200;
        profile.port_path.clear();
        assert!(profile.validate().is_err());
    }

    #[test]
    fn managed_ssh_key_metadata_round_trips_without_private_key() {
        let now = Utc::now();
        let data = ConnectionStoreData {
            managed_ssh_keys: vec![ManagedSshKey {
                id: "managed-key-1".to_string(),
                secret_id: "managed-key-secret-1".to_string(),
                name: "Production deploy key".to_string(),
                fingerprint: "SHA256:test".to_string(),
                public_key: "ssh-ed25519 AAAATEST".to_string(),
                requires_passphrase: true,
                origin: ManagedSshKeyOrigin::ImportedFile,
                created_at: now,
                updated_at: now,
            }],
            ..ConnectionStoreData::default()
        };

        let value = serde_json::to_value(&data).unwrap();

        assert_eq!(value["managed_ssh_keys"][0]["id"], "managed-key-1");
        assert_eq!(value["managed_ssh_keys"][0]["origin"], "imported_file");
        assert!(value.to_string().contains("ssh-ed25519 AAAATEST"));
        assert!(!value.to_string().contains("PRIVATE KEY"));

        let round_trip: ConnectionStoreData = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.managed_ssh_keys, data.managed_ssh_keys);
    }

    #[test]
    fn managed_key_create_stores_secret_and_returns_metadata_only() {
        let mut store = load_empty_store("managed-key-create");
        let private_key = generated_private_key_text(None);

        let info = store
            .create_managed_ssh_key_from_text(
                SecretString::from(private_key.clone()),
                Some("Deploy Key".to_string()),
                None,
            )
            .unwrap();

        assert_eq!(info.name, "Deploy Key");
        assert_eq!(info.origin, ManagedSshKeyOrigin::PastedText);
        assert!(!info.requires_passphrase);
        assert!(info.public_key.starts_with("ssh-ed25519 "));
        assert_eq!(store.data.managed_ssh_keys.len(), 1);
        assert_eq!(
            store
                .managed_keychain
                .get(&store.data.managed_ssh_keys[0].secret_id)
                .unwrap(),
            private_key.as_str()
        );
        assert!(!serde_json::to_string(&info).unwrap().contains("PRIVATE KEY"));
    }

    #[test]
    fn managed_key_secret_file_round_trips_large_private_key_material() {
        let data_dir = std::env::temp_dir().join(format!(
            "oxideterm-managed-key-secret-{}",
            Uuid::new_v4()
        ));
        let config_key = [42u8; CONFIG_ENCRYPTION_KEY_LEN];
        let secret_id = "managed-key-large-rsa";
        let private_key = SecretString::from(format!(
            "-----BEGIN OPENSSH PRIVATE KEY-----\n{}\n-----END OPENSSH PRIVATE KEY-----\n",
            "A".repeat(4096)
        ));

        write_managed_ssh_key_secret_file(&data_dir, secret_id, &private_key, &config_key).unwrap();

        let secret_path = managed_ssh_key_secret_file_path(&data_dir, secret_id).unwrap();
        let secret_file = fs::read_to_string(secret_path).unwrap();
        assert!(!secret_file.contains(private_key.expose_secret()));

        let restored = read_managed_ssh_key_secret_file(&data_dir, secret_id, &config_key).unwrap();
        assert_eq!(restored, private_key);

        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn managed_key_create_falls_back_to_secret_file_for_large_rsa_keychain_failure() {
        let _config_key = with_config_encryption_key_for_tests([43u8; CONFIG_ENCRYPTION_KEY_LEN]);
        let mut store = load_empty_store("managed-key-large-rsa-fallback");
        store.managed_keychain = ConnectionKeychain::with_max_secret_bytes_for_tests(
            "com.oxideterm.managed-test",
            256,
        );
        let private_key = generated_large_rsa_private_key_text();

        let info = store
            .create_managed_ssh_key_from_text(
                SecretString::from(private_key.clone()),
                Some("Large RSA Key".to_string()),
                None,
            )
            .unwrap();

        assert_eq!(info.name, "Large RSA Key");
        assert!(info.public_key.starts_with("ssh-rsa "));
        assert_eq!(store.data.managed_ssh_keys.len(), 1);

        let secret_id = &store.data.managed_ssh_keys[0].secret_id;
        assert!(store.managed_keychain.get(secret_id).is_err());

        let secret_path = managed_ssh_key_secret_file_path(store.data_dir().unwrap(), secret_id)
            .expect("fallback secret path should be valid");
        let secret_file = fs::read_to_string(secret_path).unwrap();
        assert!(!secret_file.contains(&private_key));

        let restored = store
            .resolve_managed_ssh_key_private_key(&info.id)
            .expect("fallback secret file should restore the managed key");
        assert_eq!(restored, private_key.as_str());
    }

    #[test]
    fn managed_key_create_rejects_invalid_key_without_echoing_secret() {
        let mut store = load_empty_store("managed-key-invalid");
        let marker = "not-a-private-key-secret-marker";

        let error = store
            .create_managed_ssh_key_from_text(SecretString::from(marker), None, None)
            .unwrap_err()
            .to_string();

        assert_eq!(error, "Invalid SSH private key");
        assert!(!error.contains(marker));
        assert!(store.data.managed_ssh_keys.is_empty());
    }

    #[test]
    fn managed_key_create_detects_passphrase_protected_key() {
        let mut store = load_empty_store("managed-key-passphrase");
        let private_key = generated_private_key_text(Some("secret-passphrase"));

        let info = store
            .create_managed_ssh_key_from_text(
                SecretString::from(private_key),
                None,
                Some(SecretString::from("secret-passphrase")),
            )
            .unwrap();

        assert!(info.requires_passphrase);
    }

    #[test]
    fn managed_key_delete_blocks_referenced_key_without_force() {
        let mut store = load_empty_store("managed-key-delete-blocked");
        let private_key = generated_private_key_text(None);
        let info = store
            .create_managed_ssh_key_from_text(SecretString::from(private_key), None, None)
            .unwrap();
        store
            .upsert(request(
                "conn-1",
                SavedAuth::ManagedKey {
                    key_id: info.id.clone(),
                    passphrase_keychain_id: None,
                    plaintext_passphrase: None,
                },
            ))
            .unwrap();

        let usage = store.managed_ssh_key_usage(&info.id).unwrap();
        let error = store.delete_managed_ssh_key(&info.id, false).unwrap_err();

        assert_eq!(usage.count, 1);
        assert!(error.to_string().contains("used by 1 saved connection"));
        assert_eq!(store.managed_ssh_keys().len(), 1);
    }

    #[test]
    fn managed_key_connection_delete_does_not_delete_managed_key_secret() {
        let mut store = load_empty_store("managed-key-connection-delete");
        let private_key = generated_private_key_text(None);
        let info = store
            .create_managed_ssh_key_from_text(SecretString::from(private_key.clone()), None, None)
            .unwrap();
        let secret_id = store.data.managed_ssh_keys[0].secret_id.clone();
        store
            .upsert(request(
                "conn-1",
                SavedAuth::ManagedKey {
                    key_id: info.id.clone(),
                    passphrase_keychain_id: None,
                    plaintext_passphrase: None,
                },
            ))
            .unwrap();

        assert!(store.delete("conn-1").unwrap());
        assert_eq!(
            store.managed_keychain.get(&secret_id).unwrap(),
            private_key.as_str()
        );
        assert_eq!(store.managed_ssh_keys().len(), 1);
    }

    #[test]
    fn managed_key_connection_info_exposes_reference_only() {
        let conn = SavedConnection {
            id: "conn-1".to_string(),
            version: CONFIG_VERSION,
            name: "Managed".to_string(),
            group: None,
            host: "example.com".to_string(),
            port: 22,
            username: "deploy".to_string(),
            auth: SavedAuth::ManagedKey {
                key_id: "managed-key-1".to_string(),
                passphrase_keychain_id: Some("kc-managed-pass".to_string()),
                plaintext_passphrase: None,
            },
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            options: ConnectionOptions::default(),
            created_at: Utc::now(),
            last_used_at: None,
            updated_at: None,
            color: None,
            tags: Vec::new(),
            post_connect_command: None,
            privilege_credentials: Vec::new(),
        };

        let info = ConnectionInfo::from(&conn);

        assert_eq!(info.auth_type, AuthType::ManagedKey);
        assert_eq!(info.managed_key_id.as_deref(), Some("managed-key-1"));
        assert!(info.managed_key_name.is_none());
        assert!(info.key_path.is_none());
        assert!(info.cert_path.is_none());
    }
}
