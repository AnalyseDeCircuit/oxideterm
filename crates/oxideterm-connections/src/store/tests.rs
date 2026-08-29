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
            notes: None,
            host: "192.168.1.2".to_string(),
            port: 22,
            username: "me".to_string(),
            auth,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            proxy_command: None,
            color: None,
            icon_background_color: None,
            icon: None,
            tags: Vec::new(),
            connect_timeout_seconds: DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS,
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            x11_forwarding: ConnectionX11ForwardingOptions::default(),
            dedicated_new_terminal_connection: false,
            ssh_channel_strategy: SshChannelStrategy::default(),
            post_connect_command: None,
            terminal: ConnectionTerminalOptions::default(),
        }
    }

    #[test]
    fn connection_notes_are_optional_multiline_metadata_and_not_searchable() {
        let mut store = load_empty_store("connection-notes");
        let mut with_notes = request("conn-notes", SavedAuth::Agent);
        with_notes.notes = Some("  Rack B\nOwned by Network Operations  ".to_string());
        let info = store.upsert(with_notes).unwrap();

        assert_eq!(
            info.notes.as_deref(),
            Some("Rack B\nOwned by Network Operations")
        );
        assert!(!info.matches_search_query("Network Operations"));

        let saved = store.get("conn-notes").unwrap();
        let mut legacy_value = serde_json::to_value(saved).unwrap();
        legacy_value.as_object_mut().unwrap().remove("notes");
        let legacy: SavedConnection = serde_json::from_value(legacy_value).unwrap();
        assert!(legacy.notes.is_none());
        assert!(
            serde_json::to_value(&legacy)
                .unwrap()
                .get("notes")
                .is_none()
        );
        assert!(
            serde_json::to_value(ConnectionInfo::from(&legacy))
                .unwrap()
                .get("notes")
                .is_none()
        );
    }

    fn mosh_request(id: &str, auth: SavedAuth) -> SaveMoshProfileRequest {
        SaveMoshProfileRequest {
            id: Some(id.to_string()),
            name: "Mobile shell".to_string(),
            group: None,
            notes: None,
            icon: None,
            color: None,
            icon_background_color: None,
            host: "mosh.example.test".to_string(),
            ssh_port: 22,
            username: "me".to_string(),
            auth,
            proxy_chain: Vec::new(),
            server_executable: "mosh-server".to_string(),
            udp_host_override: None,
            udp_port: MoshUdpPortSelection::Automatic,
            ip_family: MoshIpFamily::Auto,
            prediction: MoshPredictionMode::Adaptive,
            locale: None,
            terminal: ConnectionTerminalOptions::default(),
            identity_agent: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
        }
    }

    fn standalone_sftp_request(
        id: &str,
        auth: SavedAuth,
    ) -> SaveStandaloneSftpProfileRequest {
        SaveStandaloneSftpProfileRequest {
            id: Some(id.to_string()),
            name: "Archive SFTP".to_string(),
            group: Some("Storage".to_string()),
            notes: Some("  Separate file endpoint  ".to_string()),
            icon: Some("server".to_string()),
            color: None,
            icon_background_color: None,
            host: "sftp.example.test".to_string(),
            port: 2222,
            username: "archive".to_string(),
            auth,
            connect_timeout_seconds: 45,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            proxy_command: None,
            identity_agent: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            initial_remote_path: Some(" /srv/archive ".to_string()),
            transfer_mode: StandaloneSftpTransferMode::LocalRemote,
            secondary_endpoint: None,
        }
    }

    fn load_empty_store(name: &str) -> ConnectionStore {
        ConnectionStore::load(temp_store_path(name)).expect("store should load")
    }

    #[test]
    fn standalone_sftp_persists_references_and_loads_redacted_runtime_secrets() {
        const PRIMARY_SECRET: &str = "standalone-primary-secret";
        const HOP_SECRET: &str = "standalone-hop-secret";
        const PROXY_SECRET: &str = "standalone-proxy-secret";
        const COMMAND_SECRET: &str = "standalone-command-secret";
        const SECONDARY_SECRET: &str = "standalone-secondary-secret";

        let mut store = load_empty_store("standalone-sftp-secrets");
        let mut invalid = standalone_sftp_request(
            "sftp-invalid",
            SavedAuth::Password {
                keychain_id: Some("standalone-invalid-keychain-id".to_string()),
                plaintext_password: Some(SecretString::from(PRIMARY_SECRET)),
            },
        );
        invalid.connect_timeout_seconds = 0;
        assert!(store.upsert_standalone_sftp_profile(invalid).is_err());
        assert!(
            store
                .keychain
                .get("standalone-invalid-keychain-id")
                .is_err()
        );

        let mut request = standalone_sftp_request(
            "sftp-1",
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(SecretString::from(PRIMARY_SECRET)),
            },
        );
        request.proxy_chain.push(SavedProxyHop {
            host: "jump.example.test".to_string(),
            port: 22,
            username: "jump".to_string(),
            auth: SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(SecretString::from(HOP_SECRET)),
            },
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
        });
        request.upstream_proxy = SavedUpstreamProxyPolicy::Custom {
            proxy: SavedUpstreamProxyConfig {
                protocol: SavedUpstreamProxyProtocol::Socks5,
                host: "proxy.example.test".to_string(),
                port: 1080,
                auth: SavedUpstreamProxyAuth::Password {
                    username: "proxy-user".to_string(),
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from(PROXY_SECRET)),
                },
                remote_dns: true,
                no_proxy: String::new(),
            },
        };
        request.proxy_command = Some(SavedProxyCommand {
            keychain_id: None,
            plaintext_command: Some(SecretString::from(COMMAND_SECRET)),
        });
        request.transfer_mode = StandaloneSftpTransferMode::RemoteRemote;
        request.secondary_endpoint = Some(StandaloneSftpEndpoint {
            host: "mirror.example.test".to_string(),
            port: 2200,
            username: "mirror".to_string(),
            auth: SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(SecretString::from(SECONDARY_SECRET)),
            },
            connect_timeout_seconds: 50,
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::Direct,
            proxy_command: None,
            identity_agent: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            initial_remote_path: Some(" /srv/mirror ".to_string()),
        });

        let request_debug = format!("{request:?}");
        for secret in [
            PRIMARY_SECRET,
            HOP_SECRET,
            PROXY_SECRET,
            COMMAND_SECRET,
            SECONDARY_SECRET,
        ] {
            assert!(!request_debug.contains(secret));
        }
        let (profile, immediate_secrets) = store
            .upsert_standalone_sftp_profile_with_runtime_secrets(request)
            .unwrap();
        assert_eq!(profile.notes.as_deref(), Some("Separate file endpoint"));
        assert_eq!(profile.initial_remote_path.as_deref(), Some("/srv/archive"));
        assert_eq!(profile.connect_timeout_seconds, 45);
        assert_eq!(
            profile
                .secondary_endpoint
                .as_ref()
                .and_then(|endpoint| endpoint.initial_remote_path.as_deref()),
            Some("/srv/mirror")
        );

        let serialized = serde_json::to_string(&profile).unwrap();
        let immediate_debug = format!("{immediate_secrets:?}");
        for secret in [
            PRIMARY_SECRET,
            HOP_SECRET,
            PROXY_SECRET,
            COMMAND_SECRET,
            SECONDARY_SECRET,
        ] {
            assert!(!serialized.contains(secret));
            assert!(!immediate_debug.contains(secret));
        }

        let loaded = store
            .load_standalone_sftp_profile_runtime_secrets("sftp-1")
            .unwrap();
        assert_eq!(loaded.auth.as_ref().unwrap(), PRIMARY_SECRET);
        assert_eq!(loaded.proxy_chain[0].as_ref().unwrap(), HOP_SECRET);
        assert_eq!(loaded.upstream_proxy.as_ref().unwrap(), PROXY_SECRET);
        assert_eq!(loaded.proxy_command.as_ref().unwrap(), COMMAND_SECRET);
        assert_eq!(
            loaded
                .secondary_endpoint
                .as_ref()
                .and_then(|endpoint| endpoint.auth.as_ref())
                .unwrap(),
            SECONDARY_SECRET
        );

        let mut snapshot = store.export_standalone_sftp_profiles_snapshot().unwrap();
        let portable_json = serde_json::to_string(&snapshot).unwrap();
        for secret in [
            PRIMARY_SECRET,
            HOP_SECRET,
            PROXY_SECRET,
            COMMAND_SECRET,
            SECONDARY_SECRET,
        ] {
            assert!(!portable_json.contains(secret));
        }
        snapshot.records[0].auth = SavedAuth::with_kerberos_preferred(
            snapshot.records[0].auth.clone(),
            Some("host/sftp.example.test".to_string()),
            true,
        );
        snapshot.records[0].notes = Some("Synced metadata".to_string());
        snapshot.records[0].updated_at += Duration::seconds(1);
        assert_eq!(store.apply_standalone_sftp_profiles_snapshot(snapshot).unwrap(), 1);
        let loaded_after_sync = store
            .load_standalone_sftp_profile_runtime_secrets("sftp-1")
            .unwrap();
        assert_eq!(loaded_after_sync.auth.as_ref().unwrap(), PRIMARY_SECRET);
        assert_eq!(
            store.standalone_sftp_profiles()[0].auth.gssapi_options(),
            Some((Some("host/sftp.example.test"), true))
        );
        assert_eq!(loaded_after_sync.proxy_chain[0].as_ref().unwrap(), HOP_SECRET);
        assert_eq!(
            loaded_after_sync.upstream_proxy.as_ref().unwrap(),
            PROXY_SECRET
        );
        assert_eq!(
            loaded_after_sync.proxy_command.as_ref().unwrap(),
            COMMAND_SECRET
        );
        assert_eq!(
            loaded_after_sync
                .secondary_endpoint
                .as_ref()
                .and_then(|endpoint| endpoint.auth.as_ref())
                .unwrap(),
            SECONDARY_SECRET
        );
        let profile = store.get_standalone_sftp_profile("sftp-1").unwrap();
        let keychain_ids = collect_standalone_sftp_keychain_ids(profile);

        let mut legacy_value = serde_json::to_value(profile).unwrap();
        legacy_value.as_object_mut().unwrap().remove("version");
        legacy_value
            .as_object_mut()
            .unwrap()
            .remove("connect_timeout_seconds");
        legacy_value.as_object_mut().unwrap().remove("transfer_mode");
        legacy_value
            .as_object_mut()
            .unwrap()
            .remove("secondary_endpoint");
        let legacy: StandaloneSftpProfile = serde_json::from_value(legacy_value).unwrap();
        assert_eq!(legacy.version, CONFIG_VERSION);
        assert_eq!(
            legacy.transfer_mode,
            StandaloneSftpTransferMode::LocalRemote
        );
        assert!(
            serde_json::to_value(&legacy)
                .unwrap()
                .get("transfer_mode")
                .is_none()
        );
        assert_eq!(
            legacy.connect_timeout_seconds,
            DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS
        );
        let mut default_timeout = profile.clone();
        default_timeout.connect_timeout_seconds = DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS;
        assert!(
            serde_json::to_value(default_timeout)
                .unwrap()
                .get("connect_timeout_seconds")
                .is_none()
        );

        assert!(store.delete_standalone_sftp_profile("sftp-1").unwrap());
        assert!(store.get_standalone_sftp_profile("sftp-1").is_none());
        for keychain_id in keychain_ids {
            assert!(store.keychain.get(&keychain_id).is_err());
        }
    }

    #[test]
    fn terminal_options_round_trip_without_changing_legacy_defaults() {
        let default_options = serde_json::to_value(ConnectionOptions::default()).unwrap();
        assert!(default_options.get("terminal").is_none());
        assert!(default_options.get("x11_forwarding").is_none());
        assert!(default_options.get("connect_timeout_seconds").is_none());
        assert!(
            default_options
                .get("dedicated_new_terminal_connection")
                .is_none()
        );
        assert!(default_options.get("ssh_channel_strategy").is_none());

        let options = ConnectionOptions {
            dedicated_new_terminal_connection: true,
            ssh_channel_strategy: SshChannelStrategy::DedicatedPerConsumer,
            terminal: ConnectionTerminalOptions {
                encoding: Some(ConnectionTerminalEncoding::Utf8),
                backspace_sequence: Some(ConnectionTerminalBackspaceSequence::ControlH),
                delete_sequence: Some(ConnectionTerminalDeleteSequence::Delete),
                semantic_scheme: Some("conservative".to_string()),
                highlight_rule_set: Some("network-devices".to_string()),
                session_log_policy: ConnectionTerminalSessionLogPolicy::Automatic,
            },
            ..ConnectionOptions::default()
        };
        let serialized = serde_json::to_value(&options).unwrap();
        assert_eq!(serialized["terminal"]["encoding"], "utf-8");
        assert_eq!(serialized["terminal"]["backspaceSequence"], "controlH");
        assert_eq!(serialized["terminal"]["deleteSequence"], "delete");
        assert_eq!(serialized["terminal"]["semanticScheme"], "conservative");
        assert_eq!(
            serialized["terminal"]["highlightRuleSet"],
            "network-devices"
        );
        assert_eq!(serialized["terminal"]["sessionLogPolicy"], "automatic");
        assert_eq!(serialized["dedicated_new_terminal_connection"], true);
        assert_eq!(serialized["ssh_channel_strategy"], "dedicated_per_consumer");
        assert_eq!(
            serde_json::to_value(ConnectionTerminalEncoding::EucJp).unwrap(),
            "euc-jp"
        );
        assert_eq!(
            serde_json::to_value(ConnectionTerminalEncoding::Windows1252).unwrap(),
            "windows-1252"
        );

        let decoded: ConnectionOptions = serde_json::from_value(serialized).unwrap();
        assert_eq!(decoded.terminal, options.terminal);
        assert!(decoded.dedicated_new_terminal_connection);
        assert_eq!(
            decoded.ssh_channel_strategy,
            SshChannelStrategy::DedicatedPerConsumer
        );

        let legacy: ConnectionOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(
            legacy.effective_connect_timeout_seconds(),
            DEFAULT_SSH_CONNECT_TIMEOUT_SECONDS
        );
        assert!(legacy.terminal.inherits_application_defaults());
        assert!(!legacy.dedicated_new_terminal_connection);
        assert_eq!(legacy.ssh_channel_strategy, SshChannelStrategy::Multiplexed);
        assert_eq!(
            legacy.x11_forwarding,
            ConnectionX11ForwardingOptions::default()
        );

        let custom_timeout: ConnectionOptions = serde_json::from_value(serde_json::json!({
            "connect_timeout_seconds": 120
        }))
        .unwrap();
        assert_eq!(custom_timeout.effective_connect_timeout_seconds(), 120);
    }

    #[test]
    fn single_channel_strategy_disables_shared_forwarding_policies() {
        let mut store = load_empty_store("single-channel-policy");
        let mut request = request("single-channel", SavedAuth::Agent);
        request.ssh_channel_strategy = SshChannelStrategy::DedicatedPerConsumer;
        request.agent_forwarding = true;
        request.dedicated_new_terminal_connection = false;
        request.x11_forwarding.enabled = true;

        store.upsert(request).unwrap();
        let saved = store.get("single-channel").unwrap();

        assert!(!saved.options.agent_forwarding);
        assert!(!saved.options.dedicated_new_terminal_connection);
        assert!(!saved.options.x11_forwarding.enabled);
    }

    #[test]
    fn terminal_highlight_rule_set_update_persists_without_touching_connection_identity() {
        let path = temp_store_path("highlight-rule-set");
        let mut store = ConnectionStore::load(&path).unwrap();
        store
            .upsert(request("conn-1", SavedAuth::Agent))
            .expect("connection saved");

        assert!(
            store
                .set_terminal_highlight_rule_set(
                    "conn-1",
                    Some(" network-devices ".to_string())
                )
                .unwrap()
        );
        assert_eq!(
            store
                .get("conn-1")
                .and_then(|connection| connection.options.terminal.highlight_rule_set.as_deref()),
            Some("network-devices")
        );

        let reloaded = ConnectionStore::load(&path).unwrap();
        assert_eq!(
            reloaded
                .get("conn-1")
                .and_then(|connection| connection.options.terminal.highlight_rule_set.as_deref()),
            Some("network-devices")
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn x11_policy_round_trips_without_runtime_authority_material() {
        let options = ConnectionOptions {
            x11_forwarding: ConnectionX11ForwardingOptions {
                enabled: true,
                mode: ConnectionX11ForwardingMode::Trusted,
                untrusted_timeout_seconds: 900,
            },
            ..ConnectionOptions::default()
        };

        let serialized = serde_json::to_value(&options).unwrap();
        assert_eq!(serialized["x11_forwarding"]["enabled"], true);
        assert_eq!(serialized["x11_forwarding"]["mode"], "trusted");
        assert_eq!(serialized["x11_forwarding"]["untrusted_timeout_seconds"], 900);
        let text = serialized.to_string();
        assert!(!text.contains("DISPLAY"));
        assert!(!text.contains("MIT-MAGIC-COOKIE-1"));

        let decoded: ConnectionOptions = serde_json::from_value(serialized).unwrap();
        assert_eq!(decoded.x11_forwarding, options.x11_forwarding);
    }

    #[test]
    fn group_rename_and_delete_apply_to_the_entire_saved_session_subtree() {
        let store_path = temp_store_path("group-subtree-maintenance");
        let mut store = ConnectionStore::load(&store_path).unwrap();

        let mut connection_request = request("ssh-in-subtree", SavedAuth::Agent);
        connection_request.group = Some("Production/Core".to_string());
        store.upsert(connection_request).unwrap();

        let mut serial = SerialProfile::new("Console", "/dev/tty.test");
        serial.group = Some("Production/Core/Serial".to_string());
        let serial_id = serial.id.clone();
        store.data.serial_profiles.push(serial);

        let mut telnet = TelnetProfile::new("Router", "router.example.com", 23);
        telnet.group = Some("Production/Network".to_string());
        let telnet_id = telnet.id.clone();
        store.data.telnet_profiles.push(telnet);

        let mut standalone_sftp =
            standalone_sftp_request("sftp-in-subtree", SavedAuth::Agent);
        standalone_sftp.group = Some("Production/Core/SFTP".to_string());
        let standalone_sftp_id = store
            .upsert_standalone_sftp_profile(standalone_sftp)
            .unwrap()
            .id;

        let mut remote = RemoteDesktopProfile::new(
            "Desktop",
            RemoteDesktopProtocol::Rdp,
            "desktop.example.com",
            3389,
        );
        remote.group = Some("Production/Core".to_string());
        let remote_id = remote.id.clone();
        store.data.remote_desktop_profiles.push(remote);
        store.data.groups.push("Unrelated".to_string());
        store.data.groups.push("Production-Backup".to_string());
        store.normalize();
        store.save().unwrap();

        let updated = store
            .rename_group("Production", "Live".to_string())
            .unwrap();

        assert!(updated >= 5);
        assert!(
            store
                .groups()
                .iter()
                .all(|group| !group_path_is_within(group, "Production"))
        );
        assert!(store.groups().contains(&"Live/Core".to_string()));
        assert_eq!(store.get("ssh-in-subtree").unwrap().group.as_deref(), Some("Live/Core"));
        assert_eq!(
            store
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == serial_id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Live/Core/Serial")
        );
        assert_eq!(
            store
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == telnet_id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Live/Network")
        );
        assert_eq!(
            store
                .remote_desktop_profiles()
                .iter()
                .find(|profile| profile.id == remote_id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Live/Core")
        );
        assert_eq!(
            store
                .get_standalone_sftp_profile(&standalone_sftp_id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Live/Core/SFTP")
        );
        assert!(store.groups().contains(&"Unrelated".to_string()));
        assert!(store.groups().contains(&"Production-Backup".to_string()));

        store.delete_group("Live").unwrap();
        let reloaded = ConnectionStore::load(&store_path).unwrap();
        assert!(
            reloaded
                .groups()
                .iter()
                .all(|group| !group_path_is_within(group, "Live"))
        );
        assert!(reloaded.get("ssh-in-subtree").unwrap().group.is_none());
        assert!(reloaded.serial_profiles()[0].group.is_none());
        assert!(reloaded.telnet_profiles()[0].group.is_none());
        assert!(reloaded.standalone_sftp_profiles()[0].group.is_none());
        assert!(reloaded.remote_desktop_profiles()[0].group.is_none());
        assert!(reloaded.groups().contains(&"Unrelated".to_string()));
        assert!(
            reloaded
                .groups()
                .contains(&"Production-Backup".to_string())
        );
        let _ = fs::remove_file(store_path);
    }

    #[test]
    fn group_rename_rejects_existing_destinations_and_own_descendants() {
        let mut store = load_empty_store("group-rename-conflicts");
        store.create_group("Source/Child".to_string()).unwrap();
        store.create_group("Destination".to_string()).unwrap();

        assert!(
            store
                .rename_group("Source", "Destination".to_string())
                .is_err()
        );
        assert!(
            store
                .rename_group("Source", "Source/Child/New".to_string())
                .is_err()
        );
        assert!(store.groups().contains(&"Source/Child".to_string()));
        assert!(store.groups().contains(&"Destination".to_string()));
        let _ = fs::remove_file(store.path());
    }

    #[test]
    fn upsert_runtime_handoff_preserves_secret_allocations_and_persists_no_plaintext() {
        let store_path = temp_store_path("runtime-secret-handoff");
        let mut store = ConnectionStore::load(&store_path).expect("store should load");
        let target_secret = SecretString::from("target-secret-marker");
        let target_pointer = target_secret.expose_secret().as_ptr();
        let proxy_secret = SecretString::from("proxy-secret-marker");
        let proxy_pointer = proxy_secret.expose_secret().as_ptr();
        let upstream_secret = SecretString::from("upstream-secret-marker");
        let upstream_pointer = upstream_secret.expose_secret().as_ptr();
        let mut request = request(
            "conn-runtime-handoff",
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(target_secret),
            },
        );
        request.proxy_chain.push(SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(proxy_secret),
            },
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
        });
        request.upstream_proxy = SavedUpstreamProxyPolicy::Custom {
            proxy: SavedUpstreamProxyConfig {
                protocol: SavedUpstreamProxyProtocol::Socks5,
                host: "proxy.example.com".to_string(),
                port: 1080,
                auth: SavedUpstreamProxyAuth::Password {
                    username: "proxy-user".to_string(),
                    keychain_id: None,
                    plaintext_password: Some(upstream_secret),
                },
                remote_dns: true,
                no_proxy: String::new(),
            },
        };

        let (_connection, handoff) = store
            .upsert_with_runtime_secrets(request)
            .expect("connection should save");

        assert_eq!(
            handoff
                .auth
                .as_ref()
                .expect("target runtime secret")
                .expose_secret()
                .as_ptr(),
            target_pointer
        );
        assert_eq!(
            handoff.proxy_chain[0]
                .as_ref()
                .expect("proxy runtime secret")
                .expose_secret()
                .as_ptr(),
            proxy_pointer
        );
        assert_eq!(
            handoff
                .upstream_proxy
                .as_ref()
                .expect("upstream runtime secret")
                .expose_secret()
                .as_ptr(),
            upstream_pointer
        );
        let persisted = fs::read_to_string(store_path).expect("persisted connection store");
        for secret in [
            "target-secret-marker",
            "proxy-secret-marker",
            "upstream-secret-marker",
        ] {
            assert!(!persisted.contains(secret));
            assert!(!format!("{handoff:?}").contains(secret));
        }
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

        store
            .store_connection_credential(
                "conn-1",
                ConnectionCredentialSlot::Primary,
                &SecretString::from("replacement"),
            )
            .unwrap();
        assert_eq!(
            store.get_connection_password("conn-1").unwrap(),
            "replacement"
        );
        assert!(store.get("conn-1").unwrap().last_used_at.is_none());
        assert!(
            store
                .forget_connection_credential("conn-1", ConnectionCredentialSlot::Primary)
                .unwrap()
        );
        assert!(store.get_connection_password("conn-1").is_err());
    }

    #[test]
    fn proxy_command_is_protected_and_returned_for_one_runtime_handoff() {
        let mut store = load_empty_store("proxy-command-save");
        let store_path = store.path().to_path_buf();
        let mut save_request = request("conn-1", SavedAuth::Agent);
        save_request.proxy_command = Some(SavedProxyCommand {
            keychain_id: None,
            plaintext_command: Some(SecretString::from(
                "helper --token proxy-command-secret",
            )),
        });

        let (_, runtime_secrets) = store
            .upsert_with_runtime_secrets(save_request)
            .unwrap();
        let connection = store.get("conn-1").unwrap();
        let saved_command = connection.proxy_command.as_ref().unwrap();
        let keychain_id = saved_command.keychain_id.clone().unwrap();

        assert_eq!(
            runtime_secrets.proxy_command.as_ref().unwrap(),
            "helper --token proxy-command-secret"
        );
        assert_eq!(
            store.get_saved_proxy_command(saved_command).unwrap(),
            "helper --token proxy-command-secret"
        );
        let persisted = fs::read_to_string(store_path).unwrap();
        assert!(persisted.contains(&keychain_id));
        assert!(!persisted.contains("proxy-command-secret"));
        assert!(!format!("{saved_command:?}").contains("proxy-command-secret"));
        let decoded: SavedProxyCommand = serde_json::from_str(
            r#"{"keychain_id":"reference","command":"proxy-command-secret"}"#,
        )
        .unwrap();
        assert!(decoded.plaintext_command.is_none());

        store.delete("conn-1").unwrap();
        assert!(store.keychain.get(&keychain_id).is_err());
    }

    #[test]
    fn mosh_password_is_saved_to_keychain_reference() {
        let mut store = load_empty_store("mosh-password-save");
        let secret = "mosh-secret";

        store
            .upsert_mosh_profile(mosh_request(
                "mosh-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from(secret)),
                },
            ))
            .unwrap();

        let profile = store.get_mosh_profile("mosh-1").unwrap();
        assert!(matches!(
            profile.auth,
            SavedAuth::Password {
                keychain_id: Some(_),
                plaintext_password: None,
            }
        ));
        assert_eq!(
            store.get_saved_auth_password(&profile.auth).unwrap(),
            secret
        );
        store
            .store_mosh_profile_credential("mosh-1", &SecretString::from("replacement"))
            .unwrap();
        let profile = store.get_mosh_profile("mosh-1").unwrap();
        assert_eq!(
            store.get_saved_auth_password(&profile.auth).unwrap(),
            "replacement"
        );
        assert!(profile.last_used_at.is_none());
        assert!(store.forget_mosh_profile_credential("mosh-1").unwrap());
        let saved = fs::read_to_string(store.path()).unwrap();
        assert!(!saved.contains(secret));
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
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: true,
            ssh_algorithms: SshAlgorithmPreferences::default(),
        }];

        store.upsert(req).unwrap();

        let hop = &store.get("conn-1").unwrap().proxy_chain[0];
        assert!(hop.agent_forwarding);
        assert!(hop.legacy_ssh_compatibility);
        match &hop.auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
            } => assert_eq!(store.keychain.get(keychain_id).unwrap(), "jump-secret"),
            other => panic!("unexpected proxy auth: {other:?}"),
        }
    }

    #[test]
    fn copied_saved_password_uses_an_independent_proxy_keychain_owner() {
        let mut store = load_empty_store("copy-saved-password-for-proxy-hop");
        store
            .upsert(request(
                "source-hop",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from("source-hop-secret")),
                },
            ))
            .unwrap();

        let source_keychain_id = match &store.get("source-hop").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected source auth: {other:?}"),
        };
        let copied_auth = store
            .copy_saved_auth_for_new_owner(&store.get("source-hop").unwrap().auth)
            .unwrap();
        assert!(matches!(
            &copied_auth,
            SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(_),
            }
        ));

        let mut destination = request("target-with-hop", SavedAuth::Agent);
        destination.proxy_chain.push(SavedProxyHop {
            host: "jump.example.com".to_string(),
            port: 22,
            username: "ops".to_string(),
            auth: copied_auth,
            agent_forwarding: false,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
        });
        let (_connection, runtime_secrets) = store
            .upsert_with_runtime_secrets(destination)
            .expect("destination should save");

        assert_eq!(
            runtime_secrets.proxy_chain[0]
                .as_ref()
                .expect("copied proxy runtime secret"),
            &SecretString::from("source-hop-secret")
        );
        let destination_keychain_id = match &store.get("target-with-hop").unwrap().proxy_chain[0].auth
        {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                plaintext_password: None,
            } => keychain_id.clone(),
            other => panic!("unexpected destination auth: {other:?}"),
        };
        assert_ne!(source_keychain_id, destination_keychain_id);

        assert!(store.delete("source-hop").unwrap());
        assert_eq!(
            store.keychain.get(&destination_keychain_id).unwrap(),
            "source-hop-secret"
        );
        let persisted = fs::read_to_string(store.path()).unwrap();
        assert!(!persisted.contains("source-hop-secret"));
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
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
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

        assert_eq!(
            credential.connection_id,
            LOCAL_SHELL_PRIVILEGE_CONNECTION_ID
        );
        assert_eq!(
            store
                .list_privilege_credentials(LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
                .unwrap(),
            vec![credential]
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
        let keychain_id = credential.keychain_id;

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
        let keychain_id = credential.keychain_id.unwrap();

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
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
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
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
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
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: false,
            ssh_algorithms: SshAlgorithmPreferences::default(),
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
            notes: None,
            host: "good.example.com".to_string(),
            port: 22,
            username: "me".to_string(),
            auth: SavedAuth::Password {
                keychain_id: None,
                plaintext_password: Some(SecretString::from("secret")),
            },
            proxy_chain: Vec::new(),
            upstream_proxy: SavedUpstreamProxyPolicy::UseGlobal,
            proxy_command: None,
            options: ConnectionOptions::default(),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            updated_at: None,
            color: None,
            icon_background_color: None,
            icon: None,
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
    fn imported_privilege_targets_only_include_secrets_that_will_be_written() {
        let mut store = load_empty_store("import-privilege-snapshot-scope");
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        let mut imported = store.get("conn-1").unwrap().clone();
        let now = Utc::now();
        imported.privilege_credentials = vec![
            SavedPrivilegeCredential {
                id: "preserved".to_string(),
                connection_id: imported.id.clone(),
                label: "Preserved".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: None,
                prompt_patterns: Vec::new(),
                keychain_id: Some(privilege_keychain_id(&imported.id, "preserved")),
                plaintext_secret: None,
                enabled: true,
                require_click_to_send: true,
                created_at: now,
                updated_at: now,
            },
            SavedPrivilegeCredential {
                id: "replaced".to_string(),
                connection_id: imported.id.clone(),
                label: "Replaced".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: None,
                prompt_patterns: Vec::new(),
                keychain_id: None,
                plaintext_secret: Some(SecretString::from("replacement")),
                enabled: true,
                require_click_to_send: true,
                created_at: now,
                updated_at: now,
            },
        ];

        let ids = collect_imported_privilege_keychain_ids(&[imported]);

        assert_eq!(
            ids,
            HashSet::from([privilege_keychain_id("conn-1", "replaced")])
        );
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
    fn saved_connection_sync_snapshot_revision_tracks_record_updated_at() {
        let mut store = load_empty_store("sync-updated-at-revision");
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        let first = store.export_saved_connections_snapshot().unwrap();
        let first_record_revision = first.records[0].revision.clone();

        store.data.connections[0].updated_at = Some(Utc::now() + chrono::Duration::seconds(1));
        let second = store.export_saved_connections_snapshot().unwrap();

        assert_eq!(second.records[0].revision, first_record_revision);
        assert_ne!(second.revision, first.revision);
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
    fn saved_connection_sync_merge_applies_kerberos_policy_and_keeps_local_fallback_secret() {
        let mut target = load_empty_store("sync-kerberos-target");
        target
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from("local-fallback-secret")),
                },
            ))
            .unwrap();
        let local_keychain_id = match &target.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };

        let mut source = load_empty_store("sync-kerberos-source");
        source
            .upsert(request(
                "conn-1",
                SavedAuth::with_kerberos_preferred(
                    SavedAuth::Password {
                        keychain_id: None,
                        plaintext_password: None,
                    },
                    Some("host/server.example.test".to_string()),
                    true,
                ),
            ))
            .unwrap();

        target
            .apply_saved_connections_snapshot(
                source.export_saved_connections_snapshot().unwrap(),
                SavedConnectionsConflictStrategy::Merge,
            )
            .unwrap();

        assert!(matches!(
            &target.get("conn-1").unwrap().auth,
            SavedAuth::KerberosPreferred {
                server_identity: Some(identity),
                delegate_credentials: true,
                fallback,
            } if identity == "host/server.example.test"
                && matches!(fallback.as_ref(), SavedAuth::Password {
                    keychain_id: Some(keychain_id),
                    plaintext_password: None,
                } if keychain_id == &local_keychain_id)
        ));
        assert_eq!(
            target.keychain.get(&local_keychain_id).unwrap(),
            "local-fallback-secret"
        );
    }

    #[test]
    fn prepared_saved_connection_sync_rollback_preserves_file_and_keychain_secret() {
        let path = temp_store_path("sync-prepare-rollback");
        let mut target = ConnectionStore::load(&path).unwrap();
        target
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from("rollback-secret-marker")),
                },
            ))
            .unwrap();
        let keychain_id = match &target.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };
        let original_file = fs::read(&path).unwrap();

        let mut source = load_empty_store("sync-prepare-rollback-source");
        source.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        source.delete("conn-1").unwrap();
        let prepared = target
            .prepare_saved_connections_snapshot(
                source.export_saved_connections_snapshot().unwrap(),
                SavedConnectionsConflictStrategy::Merge,
            )
            .unwrap();

        assert!(target.get("conn-1").is_none());
        assert_eq!(
            target.keychain.get(&keychain_id).unwrap(),
            "rollback-secret-marker"
        );
        assert!(!format!("{prepared:?}").contains("rollback-secret-marker"));

        target
            .rollback_prepared_saved_connections_snapshot(&prepared)
            .unwrap();

        assert!(target.get("conn-1").is_some());
        assert_eq!(fs::read(&path).unwrap(), original_file);
        assert_eq!(
            target.keychain.get(&keychain_id).unwrap(),
            "rollback-secret-marker"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn connection_store_checkpoint_restores_complete_data_and_exact_file() {
        let path = temp_store_path("complete-checkpoint");
        let mut store = ConnectionStore::load(&path).unwrap();
        store.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        let now = Utc::now();
        store.data.groups.push("checkpoint-group-marker".to_string());
        store.data.recent.push("conn-1".to_string());
        store.data.connection_tombstones.push(DeletedConnectionTombstone {
            id: "deleted-connection-marker".to_string(),
            deleted_at: now,
        });
        store.data.managed_ssh_keys.push(ManagedSshKey {
            id: "managed-key-marker".to_string(),
            secret_id: "managed-secret-reference".to_string(),
            name: "Managed checkpoint key".to_string(),
            fingerprint: "SHA256:checkpoint".to_string(),
            public_key: "ssh-ed25519 checkpoint".to_string(),
            requires_passphrase: false,
            origin: ManagedSshKeyOrigin::OxideImport,
            created_at: now,
            updated_at: now,
        });
        store
            .data
            .serial_profiles
            .push(SerialProfile::new("Serial checkpoint", "/dev/tty-test"));
        store
            .data
            .telnet_profiles
            .push(TelnetProfile::new("Telnet checkpoint", "telnet.test", 23));
        store.save().unwrap();
        let original_data = serde_json::to_value(&store.data).unwrap();
        let original_file = fs::read(&path).unwrap();
        let checkpoint = store.create_checkpoint().unwrap();

        store.data = ConnectionStoreData::default();
        store.data.groups.push("replacement".to_string());
        store.save().unwrap();
        store.restore_checkpoint(&checkpoint).unwrap();

        assert_eq!(serde_json::to_value(&store.data).unwrap(), original_data);
        assert_eq!(fs::read(&path).unwrap(), original_file);
        assert_eq!(
            serde_json::to_value(ConnectionStore::load(&path).unwrap().data).unwrap(),
            original_data
        );
        let checkpoint_debug = format!("{checkpoint:?}");
        assert!(!checkpoint_debug.contains("checkpoint-group-marker"));
        assert!(!checkpoint_debug.contains("managed-secret-reference"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn connection_store_checkpoint_preserves_unknown_raw_fields() {
        let path = temp_store_path("checkpoint-unknown-fields");
        let original_file = serde_json::to_vec_pretty(&serde_json::json!({
            "version": CONFIG_VERSION,
            "groups": ["original"],
            "futureCompatibleMarker": {
                "nested": "must-survive-rollback"
            }
        }))
        .unwrap();
        fs::write(&path, &original_file).unwrap();
        let mut store = ConnectionStore::load(&path).unwrap();
        let checkpoint = store.create_checkpoint().unwrap();

        store.data.groups.push("prepared".to_string());
        store.save().unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("futureCompatibleMarker"));

        store.restore_checkpoint(&checkpoint).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original_file);
        assert!(fs::read_to_string(&path).unwrap().contains("must-survive-rollback"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn committed_saved_connection_sync_defers_secret_cleanup_until_finalize() {
        let mut target = load_empty_store("sync-deferred-cleanup");
        target
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from("deferred-secret")),
                },
            ))
            .unwrap();
        let keychain_id = match &target.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };
        let privilege = target
            .save_privilege_credential(SavePrivilegeCredentialRequest {
                connection_id: "conn-1".to_string(),
                credential_id: None,
                label: "Sudo".to_string(),
                kind: PrivilegeCredentialKind::SudoPassword,
                username_hint: None,
                prompt_patterns: Vec::new(),
                secret: Some(SecretString::from("deferred-privilege-secret")),
                enabled: true,
                require_click_to_send: true,
            })
            .unwrap();
        let privilege_keychain_id = privilege.keychain_id.unwrap();

        let mut source = load_empty_store("sync-deferred-cleanup-source");
        source.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        source.delete("conn-1").unwrap();
        let prepared = target
            .prepare_saved_connections_snapshot(
                source.export_saved_connections_snapshot().unwrap(),
                SavedConnectionsConflictStrategy::Merge,
            )
            .unwrap();
        let mut cleanup = target
            .commit_prepared_saved_connections_snapshot(prepared)
            .unwrap();

        assert_eq!(cleanup.pending_keychain_entries(), 2);
        assert_eq!(
            target.keychain.get(&keychain_id).unwrap(),
            "deferred-secret"
        );

        target
            .finalize_saved_connections_sync_cleanup(&mut cleanup)
            .unwrap();

        assert_eq!(cleanup.pending_keychain_entries(), 0);
        assert!(target.keychain.get(&keychain_id).is_err());
        assert!(
            target
                .privilege_keychain
                .get(&privilege_keychain_id)
                .is_err()
        );
    }

    #[test]
    fn dropping_prepared_saved_connection_sync_keeps_data_and_secret_for_explicit_recovery() {
        let path = temp_store_path("sync-prepared-drop");
        let mut target = ConnectionStore::load(&path).unwrap();
        target
            .upsert(request(
                "conn-1",
                SavedAuth::Password {
                    keychain_id: None,
                    plaintext_password: Some(SecretString::from("prepared-drop-secret")),
                },
            ))
            .unwrap();
        let keychain_id = match &target.get("conn-1").unwrap().auth {
            SavedAuth::Password {
                keychain_id: Some(keychain_id),
                ..
            } => keychain_id.clone(),
            other => panic!("unexpected auth: {other:?}"),
        };

        let mut source = load_empty_store("sync-prepared-drop-source");
        source.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        source.delete("conn-1").unwrap();
        let prepared = target
            .prepare_saved_connections_snapshot(
                source.export_saved_connections_snapshot().unwrap(),
                SavedConnectionsConflictStrategy::Merge,
            )
            .unwrap();
        drop(prepared);

        assert!(target.get("conn-1").is_none());
        assert!(ConnectionStore::load(&path).unwrap().get("conn-1").is_none());
        assert_eq!(
            target.keychain.get(&keychain_id).unwrap(),
            "prepared-drop-secret"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn failed_saved_connection_sync_prepare_restores_original_state() {
        let path = temp_store_path("sync-prepare-save-failure");
        let mut target = ConnectionStore::load(&path).unwrap();
        target.upsert(request("local", SavedAuth::Agent)).unwrap();
        let original_file = fs::read(&path).unwrap();

        let mut source = load_empty_store("sync-prepare-save-failure-source");
        source.upsert(request("remote", SavedAuth::Agent)).unwrap();
        inject_atomic_replace_failure();

        assert!(
            target
                .prepare_saved_connections_snapshot(
                    source.export_saved_connections_snapshot().unwrap(),
                    SavedConnectionsConflictStrategy::Replace,
                )
                .is_err()
        );
        assert!(target.get("local").is_some());
        assert!(target.get("remote").is_none());
        assert_eq!(fs::read(&path).unwrap(), original_file);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn failed_saved_connection_sync_rollback_can_be_retried() {
        let path = temp_store_path("sync-rollback-retry");
        let mut target = ConnectionStore::load(&path).unwrap();
        target.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        let original_file = fs::read(&path).unwrap();

        let mut source = load_empty_store("sync-rollback-retry-source");
        source.upsert(request("conn-1", SavedAuth::Agent)).unwrap();
        source.delete("conn-1").unwrap();
        let prepared = target
            .prepare_saved_connections_snapshot(
                source.export_saved_connections_snapshot().unwrap(),
                SavedConnectionsConflictStrategy::Merge,
            )
            .unwrap();
        let prepared_file = fs::read(&path).unwrap();
        inject_atomic_replace_failure();

        assert!(
            target
                .rollback_prepared_saved_connections_snapshot(&prepared)
                .is_err()
        );
        assert!(target.get("conn-1").is_none());
        assert_eq!(fs::read(&path).unwrap(), prepared_file);

        target
            .rollback_prepared_saved_connections_snapshot(&prepared)
            .unwrap();
        assert!(target.get("conn-1").is_some());
        assert_eq!(fs::read(&path).unwrap(), original_file);
        let _ = fs::remove_file(path);
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
    fn saved_connection_sync_roundtrip_preserves_upstream_proxy_and_all_options() {
        let mut source = load_empty_store("sync-all-fields-source");
        let mut source_request = request("conn-1", SavedAuth::Agent);
        source_request.name = "Production".to_string();
        source_request.group = Some("Operations".to_string());
        source_request.notes = Some("Primary host\nOwner: Platform".to_string());
        source_request.color = Some("#123456".to_string());
        source_request.icon = Some("server".to_string());
        source_request.tags = vec!["prod".to_string(), "critical".to_string()];
        source.upsert(source_request).unwrap();
        let source_connection = source.data.connections.first_mut().unwrap();
        source_connection.upstream_proxy = SavedUpstreamProxyPolicy::Custom {
            proxy: SavedUpstreamProxyConfig {
                protocol: SavedUpstreamProxyProtocol::HttpConnect,
                host: "proxy.example.test".to_string(),
                port: 8443,
                auth: SavedUpstreamProxyAuth::Password {
                    username: "proxy-user".to_string(),
                    keychain_id: Some("proxy-keychain-id".to_string()),
                    plaintext_password: None,
                },
                remote_dns: false,
                no_proxy: "localhost,.internal".to_string(),
            },
        };
        source_connection.options = ConnectionOptions {
            connect_timeout_seconds: Some(180),
            keep_alive_interval: 37,
            compression: true,
            jump_host: Some("legacy-jump".to_string()),
            term_type: Some("xterm-direct".to_string()),
            agent_forwarding: true,
            identity_agent: None,
            agent_forwarding_socket: None,
            legacy_ssh_compatibility: true,
            ssh_algorithms: SshAlgorithmPreferences::default(),
            x11_forwarding: ConnectionX11ForwardingOptions {
                enabled: true,
                mode: ConnectionX11ForwardingMode::Untrusted,
                untrusted_timeout_seconds: 1_800,
            },
            dedicated_new_terminal_connection: true,
            ssh_channel_strategy: SshChannelStrategy::DedicatedPerConsumer,
            post_connect_command: Some("uname -a".to_string()),
            terminal: ConnectionTerminalOptions::default(),
        };
        source.save().unwrap();

        let snapshot = source.export_saved_connections_snapshot().unwrap();
        let snapshot_json = serde_json::to_string(&snapshot).unwrap();
        assert!(!snapshot_json.contains("proxy-keychain-id"));
        let mut target = load_empty_store("sync-all-fields-target");
        target
            .apply_saved_connections_snapshot(snapshot, SavedConnectionsConflictStrategy::Replace)
            .unwrap();

        let imported = target.get("conn-1").unwrap();
        assert_eq!(imported.name, "Production");
        assert_eq!(imported.group.as_deref(), Some("Operations"));
        assert_eq!(
            imported.notes.as_deref(),
            Some("Primary host\nOwner: Platform")
        );
        assert_eq!(imported.color.as_deref(), Some("#123456"));
        assert_eq!(imported.icon.as_deref(), Some("server"));
        assert_eq!(imported.tags, vec!["prod", "critical"]);
        assert_eq!(imported.options.connect_timeout_seconds, Some(180));
        assert_eq!(imported.options.keep_alive_interval, 37);
        assert!(imported.options.compression);
        assert_eq!(imported.options.jump_host.as_deref(), Some("legacy-jump"));
        assert_eq!(imported.options.term_type.as_deref(), Some("xterm-direct"));
        assert!(imported.options.agent_forwarding);
        assert!(imported.options.legacy_ssh_compatibility);
        assert_eq!(
            imported.options.x11_forwarding,
            ConnectionX11ForwardingOptions {
                enabled: true,
                mode: ConnectionX11ForwardingMode::Untrusted,
                untrusted_timeout_seconds: 1_800,
            }
        );
        assert!(imported.options.dedicated_new_terminal_connection);
        assert_eq!(
            imported.options.post_connect_command.as_deref(),
            Some("uname -a")
        );
        let SavedUpstreamProxyPolicy::Custom { proxy } = &imported.upstream_proxy else {
            panic!("custom upstream proxy should survive sync");
        };
        assert_eq!(proxy.protocol, SavedUpstreamProxyProtocol::HttpConnect);
        assert_eq!(proxy.host, "proxy.example.test");
        assert_eq!(proxy.port, 8443);
        assert!(!proxy.remote_dns);
        assert_eq!(proxy.no_proxy, "localhost,.internal");
        let SavedUpstreamProxyAuth::Password {
            username,
            keychain_id,
            plaintext_password,
        } = &proxy.auth
        else {
            panic!("proxy password metadata should survive sync");
        };
        assert_eq!(username, "proxy-user");
        assert!(keychain_id.is_none());
        assert!(plaintext_password.is_none());
    }

    #[test]
    fn saved_connection_sync_accepts_legacy_records_without_full_options() {
        let mut source = load_empty_store("sync-legacy-options-source");
        let mut source_request = request("conn-1", SavedAuth::Agent);
        source_request.agent_forwarding = true;
        source_request.legacy_ssh_compatibility = true;
        source_request.post_connect_command = Some("whoami".to_string());
        source.upsert(source_request).unwrap();
        let snapshot = source.export_saved_connections_snapshot().unwrap();
        let mut snapshot_json = serde_json::to_value(snapshot).unwrap();
        snapshot_json["records"][0]
            .as_object_mut()
            .unwrap()
            .remove("options");
        let legacy_snapshot: SavedConnectionsSyncSnapshot =
            serde_json::from_value(snapshot_json).unwrap();

        let mut target = load_empty_store("sync-legacy-options-target");
        target
            .apply_saved_connections_snapshot(
                legacy_snapshot,
                SavedConnectionsConflictStrategy::Replace,
            )
            .unwrap();

        let imported = target.get("conn-1").unwrap();
        assert!(imported.options.agent_forwarding);
        assert!(imported.options.legacy_ssh_compatibility);
        assert_eq!(
            imported.options.post_connect_command.as_deref(),
            Some("whoami")
        );
        assert_eq!(imported.options.keep_alive_interval, 0);
        assert!(!imported.options.compression);
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
    fn connection_store_data_ignores_removed_raw_profiles() {
        let data: ConnectionStoreData = serde_json::from_value(serde_json::json!({
            "version": CONFIG_VERSION,
            "connections": [],
            "groups": [],
            "recent": [],
            "raw_tcp_profiles": [{"id": "legacy-tcp"}],
            "raw_udp_profiles": [{"id": "legacy-udp"}]
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
            notes: Some("Rack B".to_string()),
            icon: Some("radio".to_string()),
            color: Some("#fcd34d".to_string()),
            icon_background_color: Some("#451a03".to_string()),
            port_path: "/dev/cu.usbserial-1".to_string(),
            baud_rate: 115_200,
            data_bits: 8,
            stop_bits: 1,
            parity: SerialParity::None,
            flow_control: SerialFlowControl::Hardware,
            terminal: ConnectionTerminalOptions::default(),
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
        assert_eq!(value["serial_profiles"][0]["icon"], "radio");
        assert_eq!(value["serial_profiles"][0]["color"], "#fcd34d");
        assert_eq!(
            value["serial_profiles"][0]["icon_background_color"],
            "#451a03"
        );
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
            notes: Some("Legacy management plane".to_string()),
            icon: Some("network".to_string()),
            color: Some("#86efac".to_string()),
            icon_background_color: Some("#052e16".to_string()),
            host: "192.168.1.1".to_string(),
            port: 23,
            terminal: ConnectionTerminalOptions {
                encoding: Some(ConnectionTerminalEncoding::Big5),
                backspace_sequence: None,
                delete_sequence: Some(ConnectionTerminalDeleteSequence::ControlH),
                semantic_scheme: None,
                highlight_rule_set: None,
                session_log_policy: ConnectionTerminalSessionLogPolicy::Disabled,
            },
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
        assert_eq!(
            value["telnet_profiles"][0]["terminal"]["sessionLogPolicy"],
            "disabled"
        );
        assert_eq!(value["telnet_profiles"][0]["icon"], "network");
        assert_eq!(value["telnet_profiles"][0]["color"], "#86efac");
        assert_eq!(
            value["telnet_profiles"][0]["terminal"]["encoding"],
            "big5"
        );
        assert_eq!(
            value["telnet_profiles"][0]["icon_background_color"],
            "#052e16"
        );
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
    fn non_ssh_asset_profiles_persist_custom_icons() {
        let mut store = load_empty_store("asset-profile-icons");
        let serial = store
            .upsert_serial_profile(SaveSerialProfileRequest {
                name: "Serial".to_string(),
                icon: Some("radio".to_string()),
                color: Some("#fcd34d".to_string()),
                icon_background_color: Some("#451a03".to_string()),
                port_path: "/dev/ttyUSB0".to_string(),
                ..SaveSerialProfileRequest::default()
            })
            .unwrap();
        let telnet = store
            .upsert_telnet_profile(SaveTelnetProfileRequest {
                name: "Telnet".to_string(),
                icon: Some("network".to_string()),
                color: Some("#86efac".to_string()),
                icon_background_color: Some("#052e16".to_string()),
                host: "telnet.example.com".to_string(),
                port: 23,
                ..SaveTelnetProfileRequest::default()
            })
            .unwrap();
        let rdp = store
            .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
                name: "RDP".to_string(),
                icon: Some("monitor".to_string()),
                color: Some("#7dd3fc".to_string()),
                icon_background_color: Some("#082f49".to_string()),
                protocol: RemoteDesktopProtocol::Rdp,
                host: "rdp.example.com".to_string(),
                port: 3389,
                ..SaveRemoteDesktopProfileRequest::default()
            })
            .unwrap();
        let vnc = store
            .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
                name: "VNC".to_string(),
                icon: Some("desktop".to_string()),
                color: Some("#c4b5fd".to_string()),
                icon_background_color: Some("#2e1065".to_string()),
                protocol: RemoteDesktopProtocol::Vnc,
                host: "vnc.example.com".to_string(),
                port: 5900,
                ..SaveRemoteDesktopProfileRequest::default()
            })
            .unwrap();

        let store_path = store.path().to_path_buf();
        let reloaded = ConnectionStore::load(store_path).unwrap();

        assert_eq!(
            reloaded
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == serial.id)
                .and_then(|profile| profile.icon.as_deref()),
            Some("radio")
        );
        assert_eq!(
            reloaded
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == serial.id)
                .and_then(|profile| profile.icon_background_color.as_deref()),
            Some("#451a03")
        );
        assert_eq!(
            reloaded
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == telnet.id)
                .and_then(|profile| profile.icon.as_deref()),
            Some("network")
        );
        assert_eq!(
            reloaded
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == telnet.id)
                .and_then(|profile| profile.icon_background_color.as_deref()),
            Some("#052e16")
        );
        assert_eq!(
            reloaded
                .get_remote_desktop_profile(&rdp.id)
                .and_then(|profile| profile.icon.as_deref()),
            Some("monitor")
        );
        assert_eq!(
            reloaded
                .get_remote_desktop_profile(&rdp.id)
                .and_then(|profile| profile.icon_background_color.as_deref()),
            Some("#082f49")
        );
        assert_eq!(
            reloaded
                .get_remote_desktop_profile(&vnc.id)
                .and_then(|profile| profile.icon.as_deref()),
            Some("desktop")
        );
        assert_eq!(
            reloaded
                .get_remote_desktop_profile(&vnc.id)
                .and_then(|profile| profile.icon_background_color.as_deref()),
            Some("#2e1065")
        );
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
        assert!(
            !serde_json::to_string(&info)
                .unwrap()
                .contains("PRIVATE KEY")
        );
    }

    #[test]
    fn managed_key_resolve_repairs_legacy_hex_encoded_secret() {
        let mut store = load_empty_store("managed-key-legacy-hex");
        let private_key = generated_private_key_text(None);
        let info = store
            .create_managed_ssh_key_from_text(
                SecretString::from(private_key.clone()),
                Some("Legacy Keychain Key".to_string()),
                None,
            )
            .unwrap();
        let secret_id = store.data.managed_ssh_keys[0].secret_id.clone();
        let legacy_hex = private_key
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        store
            .managed_keychain
            .store(&secret_id, &SecretString::from(legacy_hex))
            .unwrap();

        let restored = store
            .resolve_managed_ssh_key_private_key(&info.id)
            .expect("legacy hexadecimal managed key should be repaired");

        assert_eq!(restored, private_key.as_str());
        assert_eq!(
            store.managed_keychain.get(&secret_id).unwrap(),
            private_key.as_str(),
            "the repaired private key should replace the legacy hexadecimal value"
        );
    }

    #[test]
    fn managed_key_resolve_does_not_persist_unverified_encrypted_recovery() {
        let mut store = load_empty_store("managed-key-encrypted-legacy-hex");
        let passphrase = SecretString::from("secret-passphrase");
        let private_key = generated_private_key_text(Some(passphrase.expose_secret()));
        let info = store
            .create_managed_ssh_key_from_text(
                SecretString::from(private_key.clone()),
                Some("Encrypted Legacy Keychain Key".to_string()),
                Some(passphrase.clone()),
            )
            .unwrap();
        let secret_id = store.data.managed_ssh_keys[0].secret_id.clone();
        let legacy_hex = private_key
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        store
            .managed_keychain
            .store(&secret_id, &SecretString::from(legacy_hex.clone()))
            .unwrap();

        let restored = store
            .resolve_managed_ssh_key_private_key(&info.id)
            .expect("encrypted legacy hexadecimal key should be usable for authentication");

        assert_eq!(restored, private_key.as_str());
        decode_managed_private_key(&restored, Some(&passphrase))
            .expect("the caller-provided passphrase should validate the recovered key");
        assert_eq!(
            store.managed_keychain.get(&secret_id).unwrap(),
            legacy_hex.as_str(),
            "encrypted recovery must not be persisted before fingerprint validation"
        );
    }

    #[test]
    fn managed_key_resolve_rejects_legacy_hex_with_wrong_fingerprint() {
        let mut store = load_empty_store("managed-key-legacy-hex-mismatch");
        let expected_key = generated_private_key_text(None);
        let different_key = generated_private_key_text(None);
        let info = store
            .create_managed_ssh_key_from_text(
                SecretString::from(expected_key),
                Some("Expected Key".to_string()),
                None,
            )
            .unwrap();
        let secret_id = store.data.managed_ssh_keys[0].secret_id.clone();
        let legacy_hex = different_key
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        store
            .managed_keychain
            .store(&secret_id, &SecretString::from(legacy_hex.clone()))
            .unwrap();

        let error = store
            .resolve_managed_ssh_key_private_key(&info.id)
            .expect_err("a different legacy key must fail its metadata integrity check");

        assert_eq!(error.to_string(), "Managed SSH key integrity check failed");
        assert_eq!(
            store.managed_keychain.get(&secret_id).unwrap(),
            legacy_hex.as_str()
        );
    }

    #[test]
    fn managed_key_secret_file_round_trips_large_private_key_material() {
        let data_dir =
            std::env::temp_dir().join(format!("oxideterm-managed-key-secret-{}", Uuid::new_v4()));
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
        store.managed_keychain =
            ConnectionKeychain::with_max_secret_bytes_for_tests("com.oxideterm.managed-test", 256);
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
                    key_id: info.id,
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
            notes: None,
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
            proxy_command: None,
            options: ConnectionOptions::default(),
            created_at: Utc::now(),
            last_used_at: None,
            updated_at: None,
            color: None,
            icon_background_color: None,
            icon: None,
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

    #[test]
    fn remote_desktop_profile_persists_only_protected_credential_reference() {
        const CREDENTIAL: &str = "remote-desktop-secret-value";
        let mut store = load_empty_store("remote-desktop-secret-boundary");
        let request = SaveRemoteDesktopProfileRequest {
            id: Some("remote-1".to_string()),
            name: "Lab desktop".to_string(),
            group: Some("Lab".to_string()),
            protocol: RemoteDesktopProtocol::Rdp,
            host: "rdp.example.com".to_string(),
            port: 3389,
            username: Some("operator".to_string()),
            credential: Some(SecretString::from(CREDENTIAL)),
            ..SaveRemoteDesktopProfileRequest::default()
        };

        assert!(!format!("{request:?}").contains(CREDENTIAL));
        let profile = store.upsert_remote_desktop_profile(request).unwrap();
        let reference = profile.credential_ref.clone().unwrap();
        assert_eq!(
            store
                .get_remote_desktop_credential(&profile.id)
                .unwrap()
                .unwrap(),
            CREDENTIAL
        );

        let metadata = fs::read_to_string(store.path()).unwrap();
        assert!(!metadata.contains(CREDENTIAL));
        assert!(metadata.contains(&reference));
        assert!(!metadata.contains("\"credential\""));

        assert!(store.delete_remote_desktop_profile(&profile.id).unwrap());
        assert!(store.keychain.get_optional(&reference).unwrap().is_none());

        let invalid_result =
            store.upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
                id: Some("remote-invalid".to_string()),
                name: String::new(),
                protocol: RemoteDesktopProtocol::Vnc,
                host: "vnc.example.com".to_string(),
                port: 5900,
                credential: Some(SecretString::from("rejected-secret")),
                ..SaveRemoteDesktopProfileRequest::default()
            });
        assert!(invalid_result.is_err());
        assert!(
            store
                .keychain
                .get_optional("remote-desktop:remote-invalid")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn remote_desktop_profile_update_preserves_options_and_controls_credential_lifecycle() {
        let mut store = load_empty_store("remote-desktop-update");
        let initial_options = oxideterm_remote_desktop::RemoteDesktopSessionOptions {
            clipboard: oxideterm_remote_desktop::RemoteDesktopClipboardOptions {
                text: false,
                images: true,
                files: true,
            },
            audio: oxideterm_remote_desktop::RemoteDesktopAudioOptions {
                playback: false,
                capture: true,
            },
            display: oxideterm_remote_desktop::RemoteDesktopDisplayOptions {
                use_all_monitors: true,
            },
            rdp: oxideterm_remote_desktop::RemoteDesktopRdpOptions {
                network_profile:
                    oxideterm_remote_desktop::RemoteDesktopRdpNetworkProfile::Broadband,
                disable_graphics_pipeline: true,
            },
            vnc: oxideterm_remote_desktop::RemoteDesktopVncOptions {
                security_policy:
                    oxideterm_remote_desktop::RemoteDesktopVncSecurityPolicy::AllowLegacy,
                session_mode: oxideterm_remote_desktop::RemoteDesktopVncSessionMode::Exclusive,
                image_quality:
                    oxideterm_remote_desktop::RemoteDesktopVncImageQuality::BestQuality,
                compression: oxideterm_remote_desktop::RemoteDesktopVncCompression::High,
            },
        };
        let created = store
            .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
                id: Some("remote-edit".to_string()),
                name: "Original".to_string(),
                protocol: RemoteDesktopProtocol::Rdp,
                host: "old.example.com".to_string(),
                port: 3389,
                username: Some("operator".to_string()),
                domain: Some("EXAMPLE".to_string()),
                credential: Some(SecretString::from("original-secret")),
                read_only: true,
                session_options: initial_options,
                ..SaveRemoteDesktopProfileRequest::default()
            })
            .unwrap();
        let credential_ref = created.credential_ref.clone();

        let updated = store
            .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
                id: Some(created.id.clone()),
                name: "Updated".to_string(),
                group: Some("Lab".to_string()),
                protocol: RemoteDesktopProtocol::Rdp,
                host: "new.example.com".to_string(),
                port: 3390,
                username: Some("admin".to_string()),
                domain: created.domain.clone(),
                read_only: created.read_only,
                session_options: created.session_options,
                ..SaveRemoteDesktopProfileRequest::default()
            })
            .unwrap();

        assert_eq!(updated.created_at, created.created_at);
        assert_eq!(updated.credential_ref, credential_ref);
        assert_eq!(updated.session_options, initial_options);
        assert_eq!(updated.domain.as_deref(), Some("EXAMPLE"));
        assert!(updated.read_only);
        assert_eq!(
            store
                .get_remote_desktop_credential(&updated.id)
                .unwrap()
                .unwrap(),
            "original-secret"
        );

        let store_path = store.path().to_path_buf();
        let mut store = ConnectionStore::load(store_path).unwrap();
        let reloaded = store
            .get_remote_desktop_profile(&updated.id)
            .cloned()
            .expect("updated remote desktop profile should reload");
        assert_eq!(reloaded, updated);

        let cleared = store
            .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
                id: Some(reloaded.id.clone()),
                name: reloaded.name.clone(),
                group: reloaded.group.clone(),
                protocol: reloaded.protocol,
                host: reloaded.host.clone(),
                port: reloaded.port,
                username: reloaded.username.clone(),
                domain: reloaded.domain.clone(),
                clear_credential: true,
                read_only: reloaded.read_only,
                session_options: reloaded.session_options,
                ..SaveRemoteDesktopProfileRequest::default()
            })
            .unwrap();

        assert!(cleared.credential_ref.is_none());
        assert!(
            store
                .get_remote_desktop_credential(&cleared.id)
                .unwrap()
                .is_none()
        );

        store
            .upsert(request("ssh-move", SavedAuth::Agent))
            .unwrap();
        assert_eq!(
            store
                .move_session_assets_to_group(
                    &["ssh-move".to_string()],
                    &[],
                    &[],
                    &[],
                    &[],
                    std::slice::from_ref(&cleared.id),
                    Some("Moved"),
                )
                .unwrap(),
            2
        );
        assert_eq!(
            store.get("ssh-move").and_then(|connection| connection.group.as_deref()),
            Some("Moved")
        );
        assert_eq!(
            store
                .get_remote_desktop_profile(&cleared.id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Moved")
        );
    }

    #[test]
    fn move_session_assets_to_group_includes_every_saved_profile_type() {
        let mut store = load_empty_store("move-all-session-assets");
        store
            .upsert(request("ssh-move", SavedAuth::Agent))
            .unwrap();
        let serial = store
            .upsert_serial_profile(SaveSerialProfileRequest {
                id: Some("serial-move".to_string()),
                name: "Serial console".to_string(),
                port_path: "/dev/tty.test".to_string(),
                ..SaveSerialProfileRequest::default()
            })
            .unwrap();
        let telnet = store
            .upsert_telnet_profile(SaveTelnetProfileRequest {
                id: Some("telnet-move".to_string()),
                name: "Telnet console".to_string(),
                host: "telnet.example.test".to_string(),
                port: 23,
                ..SaveTelnetProfileRequest::default()
            })
            .unwrap();
        let mosh = store
            .upsert_mosh_profile(mosh_request("mosh-move", SavedAuth::Agent))
            .unwrap();
        let standalone_sftp = store
            .upsert_standalone_sftp_profile(standalone_sftp_request(
                "standalone-sftp-move",
                SavedAuth::Agent,
            ))
            .unwrap();
        let remote = store
            .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
                id: Some("remote-move".to_string()),
                name: "Remote desktop".to_string(),
                protocol: RemoteDesktopProtocol::Rdp,
                host: "remote.example.test".to_string(),
                port: 3389,
                ..SaveRemoteDesktopProfileRequest::default()
            })
            .unwrap();

        assert_eq!(
            store
                .move_session_assets_to_group(
                    &["ssh-move".to_string()],
                    std::slice::from_ref(&serial.id),
                    std::slice::from_ref(&telnet.id),
                    std::slice::from_ref(&mosh.id),
                    std::slice::from_ref(&standalone_sftp.id),
                    std::slice::from_ref(&remote.id),
                    Some("Moved"),
                )
                .unwrap(),
            6
        );
        assert_eq!(
            store.get("ssh-move").and_then(|connection| connection.group.as_deref()),
            Some("Moved")
        );
        assert_eq!(
            store
                .serial_profiles()
                .iter()
                .find(|profile| profile.id == serial.id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Moved")
        );
        assert_eq!(
            store
                .telnet_profiles()
                .iter()
                .find(|profile| profile.id == telnet.id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Moved")
        );
        assert_eq!(
            store
                .mosh_profiles()
                .iter()
                .find(|profile| profile.id == mosh.id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Moved")
        );
        assert_eq!(
            store
                .get_standalone_sftp_profile(&standalone_sftp.id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Moved")
        );
        assert_eq!(
            store
                .get_remote_desktop_profile(&remote.id)
                .and_then(|profile| profile.group.as_deref()),
            Some("Moved")
        );
    }

    #[test]
    fn remote_desktop_snapshot_drops_device_local_credential_references() {
        let mut store = load_empty_store("remote-desktop-snapshot-redaction");
        let profile = store
            .upsert_remote_desktop_profile(SaveRemoteDesktopProfileRequest {
                id: Some("remote-1".to_string()),
                name: "VNC lab".to_string(),
                protocol: RemoteDesktopProtocol::Vnc,
                host: "vnc.example.com".to_string(),
                port: 5900,
                credential: Some(SecretString::from("not-in-snapshot")),
                ..SaveRemoteDesktopProfileRequest::default()
            })
            .unwrap();

        let snapshot = store.export_remote_desktop_profiles_snapshot().unwrap();
        assert_eq!(snapshot.records.len(), 1);
        assert!(snapshot.records[0].credential_ref.is_none());
        let serialized = serde_json::to_string(&snapshot).unwrap();
        assert!(!serialized.contains("not-in-snapshot"));
        assert!(!serialized.contains("remote-desktop:remote-1"));

        let mut imported = snapshot.records[0].clone();
        imported.credential_ref = Some("foreign-device-reference".to_string());
        imported.updated_at += Duration::seconds(1);
        store
            .apply_remote_desktop_profiles_snapshot(RemoteDesktopProfilesSyncSnapshot {
                revision: "foreign".to_string(),
                exported_at: Utc::now().to_rfc3339(),
                records: vec![imported],
            })
            .unwrap();
        assert_eq!(
            store
                .get_remote_desktop_profile(&profile.id)
                .unwrap()
                .credential_ref
                .as_deref(),
            profile.credential_ref.as_deref()
        );

        let mut fresh_store = load_empty_store("remote-desktop-snapshot-import");
        let mut foreign_profile = snapshot.records[0].clone();
        foreign_profile.credential_ref = Some("foreign-device-reference".to_string());
        fresh_store
            .apply_remote_desktop_profiles_snapshot(RemoteDesktopProfilesSyncSnapshot {
                revision: "foreign".to_string(),
                exported_at: Utc::now().to_rfc3339(),
                records: vec![foreign_profile],
            })
            .unwrap();
        assert!(
            fresh_store
                .get_remote_desktop_profile(&profile.id)
                .unwrap()
                .credential_ref
                .is_none()
        );
    }
}
