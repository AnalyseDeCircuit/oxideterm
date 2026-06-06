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
    fn session_manager_table_width_matches_tauri_connection_table_columns() {
        // This locks the Tauri ConnectionTable min-w-fit contract that keeps
        // horizontal scrolling, row dividers, and the sticky actions column aligned.
        assert_eq!(
            manager_table_min_width_for_metrics(TauriTableMetrics::default()),
            804.0
        );
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
    fn duplicate_template_name_uses_unique_tauri_copy_suffix() {
        let name = duplicate_connection_template_name(
            "Prod Copy",
            ["Prod", "Prod Copy", "Prod Copy 2"].into_iter(),
        );

        assert_eq!(name, "Prod Copy 3");
    }

    #[test]
    fn duplicate_template_name_falls_back_for_empty_source() {
        let name = duplicate_connection_template_name("", ["Connection Copy"].into_iter());

        assert_eq!(name, "Connection Copy 2");
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
                managed_key_id: String::new(),
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
    fn proxy_hop_two_factor_is_rejected_instead_of_saved_as_agent() {
        let mut form = NewConnectionForm {
            auth_tab: SshAuthTab::Agent,
            ..base_form()
        };
        form.proxy_hops
            .push(crate::workspace::new_connection::NewConnectionProxyHop {
                host: "jump.example.com".to_string(),
                port: "22".to_string(),
                username: "ops".to_string(),
                auth_tab: SshAuthTab::TwoFactor,
                password: String::new(),
                key_path: String::new(),
                managed_key_id: String::new(),
                cert_path: String::new(),
                passphrase: String::new(),
                agent_forwarding: false,
            });

        let error = save_request_from_form(&form, None).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Proxy hop 1 does not support keyboard-interactive/2FA"
        );
    }

    #[test]
    fn basic_dialog_tab_order_wraps_through_text_input_like_radix_dialog() {
        assert_eq!(
            browser_behavior::modal_footer_input_key_action(
                "tab",
                false,
                &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
                true,
                true,
                None,
                SessionManagerBasicDialogFooterAction::Cancel,
                None,
            ),
            Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(
                SessionManagerBasicDialogFooterAction::Cancel
            ))
        );

        assert_eq!(
            browser_behavior::modal_footer_input_key_action(
                "tab",
                false,
                &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
                true,
                false,
                Some(SessionManagerBasicDialogFooterAction::Primary),
                SessionManagerBasicDialogFooterAction::Cancel,
                None,
            ),
            Some(browser_behavior::ModalFooterInputKeyAction::FocusInput)
        );

        assert_eq!(
            browser_behavior::modal_footer_input_key_action(
                "tab",
                true,
                &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
                true,
                false,
                Some(SessionManagerBasicDialogFooterAction::Cancel),
                SessionManagerBasicDialogFooterAction::Cancel,
                None,
            ),
            Some(browser_behavior::ModalFooterInputKeyAction::FocusInput)
        );
    }

    #[test]
    fn basic_dialog_footer_arrows_stay_inside_footer_actions() {
        assert_eq!(
            browser_behavior::modal_footer_input_key_action(
                "arrowleft",
                false,
                &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
                false,
                false,
                Some(SessionManagerBasicDialogFooterAction::Cancel),
                SessionManagerBasicDialogFooterAction::Cancel,
                None,
            ),
            Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(
                SessionManagerBasicDialogFooterAction::Primary
            ))
        );

        assert_eq!(
            browser_behavior::modal_footer_input_key_action(
                "arrowright",
                false,
                &SESSION_MANAGER_BASIC_DIALOG_FOOTER_ACTIONS,
                false,
                false,
                Some(SessionManagerBasicDialogFooterAction::Primary),
                SessionManagerBasicDialogFooterAction::Cancel,
                None,
            ),
            Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(
                SessionManagerBasicDialogFooterAction::Cancel
            ))
        );
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
            version: oxideterm_connections::CONFIG_VERSION,
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
            upstream_proxy: oxideterm_connections::SavedUpstreamProxyPolicy::UseGlobal,
            options: oxideterm_connections::ConnectionOptions::default(),
            created_at: now,
            last_used_at: None,
            updated_at: Some(now),
            color: None,
            tags: Vec::new(),
            post_connect_command: None,
            privilege_credentials: Vec::new(),
        };

        let settings = PersistedSettings::default();
        let config = ssh_config_from_saved_connection(&store, &settings, &conn).unwrap();

        assert!(config.strict_host_key_checking);
        let chain = config.proxy_chain.unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].host, "jump.example.com");
        assert_eq!(chain[0].port, 2222);
        assert_eq!(chain[0].username, "ops");
        assert!(chain[0].agent_forwarding);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn saved_managed_key_becomes_reference_only_ssh_config() {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-gpui-managed-key-test-{}-connections.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = ConnectionStore::load(&path).unwrap();
        let now = Utc::now();
        let conn = SavedConnection {
            id: "conn-managed-key".to_string(),
            version: oxideterm_connections::CONFIG_VERSION,
            name: "Managed".to_string(),
            group: None,
            host: "target.example.com".to_string(),
            port: 22,
            username: "me".to_string(),
            auth: SavedAuth::ManagedKey {
                key_id: "managed-key-1".to_string(),
                passphrase_keychain_id: None,
                plaintext_passphrase: None,
            },
            proxy_chain: Vec::new(),
            upstream_proxy: oxideterm_connections::SavedUpstreamProxyPolicy::UseGlobal,
            options: oxideterm_connections::ConnectionOptions::default(),
            created_at: now,
            last_used_at: None,
            updated_at: Some(now),
            color: None,
            tags: Vec::new(),
            post_connect_command: None,
            privilege_credentials: Vec::new(),
        };

        let settings = PersistedSettings::default();
        let config = ssh_config_from_saved_connection(&store, &settings, &conn).unwrap();

        assert!(matches!(
            config.auth,
            AuthMethod::ManagedKey { key_id, passphrase }
                if key_id == "managed-key-1" && passphrase.is_none()
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn use_global_upstream_proxy_hydrates_settings_keychain_secret() {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-gpui-global-proxy-test-{}-connections.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = ConnectionStore::load(&path).unwrap();
        let keychain_id = store
            .save_global_upstream_proxy_password(&SecretString::new("global-secret"))
            .unwrap();
        let mut settings = PersistedSettings::default();
        settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
            protocol: SettingsUpstreamProxyProtocol::Socks5,
            host: "global-proxy.local".to_string(),
            port: 1080,
            auth: SettingsUpstreamProxyAuth::Password {
                username: "global-user".to_string(),
                keychain_id: Some(keychain_id),
            },
            remote_dns: true,
            no_proxy: "localhost".to_string(),
        });
        let policy = oxideterm_connections::SavedUpstreamProxyPolicy::UseGlobal;

        let proxy = upstream_proxy_config_from_saved_policy(&store, &settings, &policy).unwrap();

        assert_eq!(proxy.host, "global-proxy.local");
        assert_eq!(proxy.no_proxy, "localhost");
        match proxy.auth {
            UpstreamProxyAuth::Password { username, password } => {
                assert_eq!(username, "global-user");
                assert_eq!(password.as_str(), "global-secret");
            }
            UpstreamProxyAuth::None => panic!("expected password auth"),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn direct_upstream_proxy_policy_ignores_global_proxy() {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-gpui-direct-proxy-test-{}-connections.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let store = ConnectionStore::load(&path).unwrap();
        let mut settings = PersistedSettings::default();
        settings.network.upstream_proxy = Some(SettingsUpstreamProxyConfig {
            protocol: SettingsUpstreamProxyProtocol::Socks5,
            host: "global-proxy.local".to_string(),
            port: 1080,
            auth: SettingsUpstreamProxyAuth::None,
            remote_dns: true,
            no_proxy: String::new(),
        });
        let policy = oxideterm_connections::SavedUpstreamProxyPolicy::Direct;

        assert!(upstream_proxy_config_from_saved_policy(&store, &settings, &policy).is_none());
        let _ = std::fs::remove_file(path);
    }
}
