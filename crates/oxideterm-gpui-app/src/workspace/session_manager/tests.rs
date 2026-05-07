#[cfg(test)]
mod tests {
    use super::*;

    fn base_form() -> NewConnectionForm {
        NewConnectionForm {
            name: "Home".to_string(),
            host: "192.168.1.2".to_string(),
            port: "22".to_string(),
            username: "me".to_string(),
            group: "Ungrouped".to_string(),
            ..NewConnectionForm::default()
        }
    }

    #[test]
    fn new_connection_save_password_false_does_not_request_keychain_storage() {
        let form = NewConnectionForm {
            password: "secret".to_string(),
            save_password: false,
            ..base_form()
        };

        let request = save_request_from_form(&form, None).unwrap();

        match request.auth {
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: None,
            } => {}
            other => panic!("unexpected auth: {other:?}"),
        }
    }

    #[test]
    fn new_connection_save_password_true_keeps_empty_password_as_submitted_secret() {
        let form = NewConnectionForm {
            password: String::new(),
            save_password: true,
            ..base_form()
        };

        let request = save_request_from_form(&form, None).unwrap();

        match request.auth {
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(password),
            } => assert_eq!(password, ""),
            other => panic!("unexpected auth: {other:?}"),
        }
    }

    #[test]
    fn edit_properties_unloaded_password_preserves_saved_keychain_id() {
        let existing = SavedAuth::Password {
            keychain_id: Some("kc-password".to_string()),
            plaintext_password: None,
        };
        let form = NewConnectionForm {
            password: String::new(),
            password_loaded: false,
            save_password: true,
            ..base_form()
        };

        let request = save_request_from_form_with_existing_auth(
            &form,
            Some("conn-1".to_string()),
            Some(&existing),
        )
        .unwrap();

        match request.auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
            } => assert_eq!(keychain_id, "kc-password"),
            other => panic!("unexpected auth: {other:?}"),
        }
    }

    #[test]
    fn edit_properties_same_key_empty_passphrase_submits_no_new_secret() {
        let existing = SavedAuth::Key {
            key_path: "/tmp/id_ed25519".to_string(),
            has_passphrase: true,
            passphrase_keychain_id: Some("kc-passphrase".to_string()),
            plaintext_passphrase: None,
        };
        let form = NewConnectionForm {
            auth_tab: SshAuthTab::SshKey,
            key_path: "/tmp/id_ed25519".to_string(),
            passphrase: String::new(),
            ..base_form()
        };

        let request = save_request_from_form_with_existing_auth(
            &form,
            Some("conn-1".to_string()),
            Some(&existing),
        )
        .unwrap();

        match request.auth {
            SavedAuth::Key {
                key_path,
                has_passphrase,
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            } => {
                assert_eq!(key_path, "/tmp/id_ed25519");
                assert!(!has_passphrase);
            }
            other => panic!("unexpected auth: {other:?}"),
        }
    }

    #[test]
    fn new_connection_request_carries_proxy_chain() {
        let mut form = NewConnectionForm {
            auth_tab: SshAuthTab::Agent,
            ..base_form()
        };
        form.proxy_hops
            .push(crate::workspace::new_connection::NewConnectionProxyHop {
                host: "jump.example.com".to_string(),
                port: "2222".to_string(),
                username: "ops".to_string(),
                auth_tab: SshAuthTab::Password,
                password: "jump-secret".to_string(),
                key_path: String::new(),
                cert_path: String::new(),
                passphrase: String::new(),
                agent_forwarding: true,
            });

        let request = save_request_from_form(&form, None).unwrap();

        assert_eq!(request.proxy_chain.len(), 1);
        let hop = &request.proxy_chain[0];
        assert_eq!(hop.host, "jump.example.com");
        assert_eq!(hop.port, 2222);
        assert_eq!(hop.username, "ops");
        assert!(hop.agent_forwarding);
        match &hop.auth {
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(password),
            } => assert_eq!(password, "jump-secret"),
            other => panic!("unexpected proxy auth: {other:?}"),
        }
    }

    #[test]
    fn saved_proxy_chain_becomes_ssh_config_chain() {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-gpui-session-manager-test-{}-connections.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = ConnectionStore::load(&path).unwrap();
        let now = Utc::now();
        let conn = SavedConnection {
            id: "conn-1".to_string(),
            name: "Home".to_string(),
            group: None,
            host: "target.example.com".to_string(),
            port: 22,
            username: "me".to_string(),
            auth: SavedAuth::Agent,
            proxy_chain: vec![SavedProxyHop {
                host: "jump.example.com".to_string(),
                port: 2222,
                username: "ops".to_string(),
                auth: SavedAuth::Agent,
                agent_forwarding: true,
            }],
            options: oxideterm_connections::ConnectionOptions {
                agent_forwarding: false,
            },
            created_at: now,
            last_used_at: None,
            updated_at: Some(now),
            color: None,
            tags: Vec::new(),
        };

        let config = ssh_config_from_saved_connection(&store, &conn).unwrap();

        assert!(config.strict_host_key_checking);
        let chain = config.proxy_chain.unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].host, "jump.example.com");
        assert_eq!(chain[0].port, 2222);
        assert_eq!(chain[0].username, "ops");
        assert!(chain[0].agent_forwarding);
        let _ = std::fs::remove_file(path);
    }
}
