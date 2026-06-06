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

}
