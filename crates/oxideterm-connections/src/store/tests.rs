mod tests {
    use std::{fs, path::PathBuf};

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
            color: None,
            tags: Vec::new(),
            agent_forwarding: false,
        }
    }

    fn load_empty_store(name: &str) -> ConnectionStore {
        ConnectionStore::load(temp_store_path(name)).expect("store should load")
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
}
