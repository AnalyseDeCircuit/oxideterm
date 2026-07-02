#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_gpui_ui::TauriTableMetrics;

    const MANAGER_COL_CHECKBOX: f32 = 32.0;
    const MANAGER_COL_NAME_BASIS: f32 = 140.0;
    const MANAGER_COL_HOST: f32 = 130.0;
    const MANAGER_COL_PORT: f32 = 50.0;
    const MANAGER_COL_USERNAME: f32 = 90.0;
    const MANAGER_COL_AUTH: f32 = 72.0;
    const MANAGER_COL_GROUP: f32 = 100.0;
    const MANAGER_COL_LAST_USED: f32 = 90.0;
    const MANAGER_COL_ACTIONS: f32 = 84.0;

    fn manager_table_min_width_for_metrics(metrics: TauriTableMetrics) -> f32 {
        // Tauri ConnectionTable columns: px-2 wrapper plus w-8, w-[140px],
        // w-[130px], w-[50px], w-[90px], w-[72px], w-[100px], w-[90px],
        // and sticky w-[84px] actions.
        metrics.padding_x * 2.0
            + MANAGER_COL_CHECKBOX
            + MANAGER_COL_NAME_BASIS
            + MANAGER_COL_HOST
            + MANAGER_COL_PORT
            + MANAGER_COL_USERNAME
            + MANAGER_COL_AUTH
            + MANAGER_COL_GROUP
            + MANAGER_COL_LAST_USED
            + MANAGER_COL_ACTIONS
    }

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

    fn raw_tcp_profile_fixture() -> RawTcpProfile {
        let now = chrono::Utc::now();
        RawTcpProfile {
            id: "raw-tcp-1".to_string(),
            name: "Lab console".to_string(),
            group: Some("Lab".to_string()),
            host: "device.local".to_string(),
            port: 443,
            line_ending: oxideterm_connections::RawTcpLineEnding::Lf,
            display_mode: oxideterm_connections::RawTcpDisplayMode::Mixed,
            send_mode: oxideterm_connections::RawTcpSendMode::Hex,
            tls_mode: oxideterm_connections::RawTcpTlsMode::Enabled,
            tls_verification: oxideterm_connections::RawTcpTlsVerification::AllowInvalidCertificates,
            tls_server_name: Some("device-tls.local".to_string()),
            connect_on_open: false,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }

    fn connection_info_fixture(icon: Option<&str>) -> ConnectionInfo {
        ConnectionInfo {
            id: "conn-1".to_string(),
            name: "Home".to_string(),
            group: Some("Ungrouped".to_string()),
            host: "192.168.1.2".to_string(),
            port: 22,
            username: "me".to_string(),
            auth_type: AuthType::Agent,
            key_path: None,
            cert_path: None,
            managed_key_id: None,
            managed_key_name: None,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            created_at: "2026-06-15T00:00:00Z".to_string(),
            last_used_at: None,
            color: None,
            icon: icon.map(ToOwned::to_owned),
            tags: Vec::new(),
            agent_forwarding: false,
            legacy_ssh_compatibility: false,
            post_connect_command: None,
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
    fn session_menu_dismissal_closes_batch_move_popover() {
        let mut state = SessionManagerState {
            show_batch_move: true,
            ..SessionManagerState::default()
        };

        assert!(close_session_menu_state(&mut state));
        assert!(!state.show_batch_move);
    }

    #[test]
    fn raw_tcp_display_item_exposes_endpoint_and_tls_metadata() {
        let item = SessionManagerDisplayItem::RawTcp(raw_tcp_profile_fixture());

        assert_eq!(item.name(), "Lab console");
        assert_eq!(item.host(), "device.local");
        assert_eq!(item.port_sort_key(), 443);
        assert_eq!(item.subtitle(), "device.local:443 · TLS");
        assert!(matches!(item.icon(), LucideIcon::Cable));
    }

    #[test]
    fn connection_display_item_uses_custom_icon_when_present() {
        let item = SessionManagerDisplayItem::Connection(connection_info_fixture(Some("cloud")));

        assert!(matches!(item.icon(), LucideIcon::Cloud));
    }

    #[test]
    fn connection_display_item_falls_back_to_server_icon() {
        let item = SessionManagerDisplayItem::Connection(connection_info_fixture(Some("missing")));

        assert!(matches!(item.icon(), LucideIcon::Server));
    }

    #[test]
    fn save_request_from_form_preserves_custom_icon() {
        let form = NewConnectionForm {
            icon: "cloud".to_string(),
            ..base_form()
        };
        let request = save_request_from_form(&form, Some("conn-1".to_string())).unwrap();

        assert_eq!(request.icon.as_deref(), Some("cloud"));
    }

    #[test]
    fn raw_tcp_display_item_search_includes_host_port_group_and_tls_name() {
        let item = SessionManagerDisplayItem::RawTcp(raw_tcp_profile_fixture());
        let search_text = item.search_text().to_lowercase();

        for expected in ["lab console", "device.local", "443", "lab", "device-tls.local", "tls"] {
            assert!(
                search_text.contains(expected),
                "missing Raw TCP search token: {expected}"
            );
        }
    }

    #[test]
    fn raw_tcp_open_action_config_preserves_profile_options() {
        let profile = raw_tcp_profile_fixture();
        let config = terminal_raw_tcp_config_from_profile(&profile);

        assert_eq!(config.host, profile.host);
        assert_eq!(config.port, profile.port);
        assert_eq!(config.line_ending, oxideterm_terminal::RawTcpLineEnding::Lf);
        assert_eq!(config.display_mode, oxideterm_terminal::RawTcpDisplayMode::Mixed);
        assert_eq!(config.send_mode, oxideterm_terminal::RawTcpSendMode::Hex);
        assert!(config.tls.enabled);
        assert_eq!(
            config.tls.verification,
            oxideterm_terminal::RawTcpTlsVerification::AllowInvalidCertificates
        );
        assert_eq!(config.tls.server_name.as_deref(), Some("device-tls.local"));
    }

    #[test]
    fn raw_tcp_profile_delete_removes_saved_profile() {
        let path = std::env::temp_dir().join(format!(
            "oxideterm-session-manager-raw-tcp-delete-{}.json",
            uuid::Uuid::new_v4()
        ));
        let mut store = ConnectionStore::load(&path).expect("store should load");
        let profile = store
            .upsert_raw_tcp_profile(oxideterm_connections::SaveRawTcpProfileRequest {
                name: "Lab console".to_string(),
                host: "device.local".to_string(),
                port: 443,
                ..oxideterm_connections::SaveRawTcpProfileRequest::default()
            })
            .expect("profile should save");

        assert!(
            store
                .delete_raw_tcp_profile(&profile.id)
                .expect("delete should succeed")
        );
        assert!(store.raw_tcp_profiles().is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn oxide_export_logical_scroll_change_detects_inner_consumption() {
        // GPUI ListState owns measured row heights internally, so scroll-chain
        // decisions must compare actual logical movement instead of estimates.
        assert!(!oxide_export_logical_scroll_changed(0, 0.0, 0, 0.0));
        assert!(!oxide_export_logical_scroll_changed(0, 12.0, 0, 12.004));
        assert!(oxide_export_logical_scroll_changed(0, 0.0, 0, 24.0));
        assert!(oxide_export_logical_scroll_changed(0, 24.0, 1, 0.0));
    }

    #[test]
    fn oxide_export_selection_count_label_uses_locale_placeholders() {
        assert_eq!(
            oxide_export_selection_count_label(
                "Select Connections to Export ({{selected}}/{{total}})".to_string(),
                2,
                5,
            ),
            "Select Connections to Export (2/5)"
        );
    }

    #[test]
    fn oxide_export_native_i18n_keys_resolve_without_tauri_namespace() {
        // Native modals.json flattens the export dialog as `export.*`; using
        // Tauri's `modals.export.*` namespace renders raw keys in the dialog.
        let i18n = oxideterm_i18n::I18n::new(oxideterm_i18n::Locale::ZhCn);
        for key in [
            "export.select_connections",
            "export.select_all",
            "export.new_since_last_export",
            "export.badge_new",
            "export.credential_material",
            "export.content_summary_title",
            "export.app_settings_section_terminal_appearance",
        ] {
            assert_ne!(i18n.t(key), key, "unresolved export i18n key: {key}");
        }
        assert_eq!(
            i18n.t("modals.export.select_connections"),
            "modals.export.select_connections"
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
                saved_connection_id: String::new(),
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
                legacy_ssh_compatibility: true,
            });

        let request = save_request_from_form(&form, None).unwrap();

        assert_eq!(request.proxy_chain.len(), 1);
        let hop = &request.proxy_chain[0];
        assert_eq!(hop.host, "jump.example.com");
        assert_eq!(hop.port, 2222);
        assert_eq!(hop.username, "ops");
        assert!(hop.agent_forwarding);
        assert!(hop.legacy_ssh_compatibility);
        match &hop.auth {
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(password),
            } => assert_eq!(password, "jump-secret"),
            other => panic!("unexpected proxy auth: {other:?}"),
        }
    }

    #[test]
    fn proxy_hop_two_factor_is_saved_as_keyboard_interactive() {
        let mut form = NewConnectionForm {
            auth_tab: SshAuthTab::Agent,
            ..base_form()
        };
        form.proxy_hops
            .push(crate::workspace::new_connection::NewConnectionProxyHop {
                saved_connection_id: String::new(),
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
                legacy_ssh_compatibility: false,
            });

        let request = save_request_from_form(&form, None).unwrap();

        assert!(matches!(
            request.proxy_chain[0].auth,
            oxideterm_connections::SavedAuth::KeyboardInteractive
        ));
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

}
