use super::*;
use oxideterm_connections::{ConnectionStoreData, SerialProfile, TelnetProfile};
use oxideterm_remote_desktop::{
    RemoteDesktopEndpoint, RemoteDesktopProtocol, RemoteDesktopProxyAuth, RemoteDesktopSocksProxy,
    builtin_provider_registry,
};

#[test]
fn saved_sessions_restore_from_current_profiles_and_removed_sessions_stay_removed() {
    let directory = tempfile::tempdir().unwrap();
    let store_path = directory.path().join("connections.json");
    let snapshot_path = directory.path().join("standalone_sessions.json");
    let mut data = ConnectionStoreData {
        serial_profiles: vec![SerialProfile::new("console", "COM5")],
        telnet_profiles: vec![TelnetProfile::new("switch", "switch.test", 23)],
        mosh_profiles: vec![MoshProfile::new(
            "shell",
            "mosh.test",
            22,
            "user",
            SavedAuth::Agent,
        )],
        remote_desktop_profiles: vec![
            RemoteDesktopProfile::new("desktop", RemoteDesktopProtocol::Rdp, "rdp.test", 3389),
            RemoteDesktopProfile::new("screen", RemoteDesktopProtocol::Vnc, "vnc.test", 5900),
        ],
        ..ConnectionStoreData::default()
    };
    fs::write(&store_path, serde_json::to_vec(&data).unwrap()).unwrap();
    let store = ConnectionStore::load_read_only(&store_path).unwrap();
    let mut registry = StandaloneConnectionRegistry::restore(snapshot_path.clone(), &store);
    let profiles = [
        (
            StandaloneConnectionKind::Serial,
            data.serial_profiles[0].id.clone(),
        ),
        (
            StandaloneConnectionKind::Telnet,
            data.telnet_profiles[0].id.clone(),
        ),
        (
            StandaloneConnectionKind::Mosh,
            data.mosh_profiles[0].id.clone(),
        ),
        (
            StandaloneConnectionKind::Rdp,
            data.remote_desktop_profiles[0].id.clone(),
        ),
        (
            StandaloneConnectionKind::Vnc,
            data.remote_desktop_profiles[1].id.clone(),
        ),
    ];
    let ids = profiles
        .iter()
        .map(|(kind, profile_id)| {
            let launch = LaunchSnapshot::Saved {
                profile_id: profile_id.clone(),
            }
            .restore(*kind, &store)
            .unwrap();
            registry.insert_pending(*kind, format!("{kind:?}"), launch)
        })
        .collect::<Vec<_>>();
    // A saved profile remains authoritative even when edited after the last active-session write.
    data.serial_profiles[0].baud_rate = 57600;
    fs::write(&store_path, serde_json::to_vec(&data).unwrap()).unwrap();
    let store = ConnectionStore::load_read_only(&store_path).unwrap();
    let mut restored = StandaloneConnectionRegistry::restore(snapshot_path.clone(), &store);
    assert_eq!(
        restored
            .records()
            .iter()
            .map(|record| (&record.id, record.kind))
            .collect::<Vec<_>>(),
        ids.iter()
            .zip(profiles.iter().map(|(kind, _)| *kind))
            .collect::<Vec<_>>()
    );
    for record in restored.records() {
        assert_eq!(record.readiness, ActiveSessionReadiness::Disconnected);
        assert!(record.surface.is_none());
        assert!(!restored.is_connecting_attempt(&record.id));
    }
    let StandaloneConnectionLaunch::SavedSerial { config, .. } = &restored.records()[0].launch
    else {
        panic!("serial record");
    };
    assert_eq!(
        (config.port_path.as_str(), config.baud_rate),
        ("COM5", 57600)
    );

    // Explicit removal is durable without an exit hook; deleting an asset also prevents revival.
    restored.remove(&ids[0]);
    data.telnet_profiles.clear();
    fs::write(&store_path, serde_json::to_vec(&data).unwrap()).unwrap();
    let store = ConnectionStore::load_read_only(&store_path).unwrap();
    let restored = StandaloneConnectionRegistry::restore(snapshot_path, &store);
    assert_eq!(
        restored
            .records()
            .iter()
            .map(|record| &record.id)
            .collect::<Vec<_>>(),
        ids[2..].iter().collect::<Vec<_>>()
    );
}

#[test]
fn temporary_sessions_persist_metadata_and_reauthenticate_into_the_same_record() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("standalone_sessions.json");
    let store = ConnectionStore::load_read_only(directory.path().join("connections.json")).unwrap();
    let mut registry = StandaloneConnectionRegistry::restore(path.clone(), &store);
    let serial = SerialSessionConfig {
        port_path: "COM5".to_string(),
        baud_rate: 115200,
        data_bits: 7,
        stop_bits: 2,
        parity: oxideterm_terminal::SerialParity::Even,
        flow_control: oxideterm_terminal::SerialFlowControl::Hardware,
        runtime_options: oxideterm_terminal::SerialRuntimeOptions {
            line_ending: oxideterm_terminal::SerialLineEnding::CrLf,
            display_mode: oxideterm_terminal::SerialDisplayMode::Mixed,
            local_echo: true,
            ..Default::default()
        },
    };
    let serial_id = registry.insert(
        StandaloneConnectionKind::Serial,
        "console".into(),
        StandaloneConnectionLaunch::Serial {
            config: serial.clone(),
            terminal_options: Default::default(),
        },
        StandaloneConnectionSurface::Terminal(TerminalSessionId(88)),
    );
    let telnet_id = registry.insert_pending(
        StandaloneConnectionKind::Telnet,
        "switch".into(),
        StandaloneConnectionLaunch::Telnet {
            config: TelnetSessionConfig {
                host: "switch.test".into(),
                port: 2323,
            },
            terminal_options: Default::default(),
        },
    );
    let mut config = SshConfig {
        host: "mosh.test".into(),
        username: "operator".into(),
        auth: AuthMethod::password("temporary-password"),
        ..Default::default()
    };
    config.proxy_chain = Some(vec![oxideterm_ssh::ProxyHopConfig {
        host: "jump.test".into(),
        port: 2222,
        username: "jump-user".into(),
        auth: AuthMethod::certificate(
            "/keys/jump",
            "/keys/jump-cert.pub",
            Some("temporary-passphrase".into()),
        ),
        agent_forwarding: false,
        identity_agent: None,
        agent_forwarding_socket: None,
        legacy_ssh_compatibility: false,
        ssh_algorithms: Default::default(),
        strict_host_key_checking: true,
        trust_host_key: None,
        expected_host_key_fingerprint: None,
    }]);
    let profile = MoshProfile::new("shell", "mosh.test", 22, "operator", SavedAuth::Agent);
    let mut options = new_connection::mosh_options_from_profile(&profile);
    options.saved_profile_id = None;
    options.public_mcp_open_token = Some("automation-token".into());
    options.udp_host_override = Some("udp.test".into());
    let mosh_id = registry.insert_pending(
        StandaloneConnectionKind::Mosh,
        "shell".into(),
        StandaloneConnectionLaunch::MoshPreflight { config, options },
    );
    let provider_registry = builtin_provider_registry().unwrap();
    let mut desktop_ids = Vec::new();
    for (kind, protocol, host, port) in [
        (
            StandaloneConnectionKind::Rdp,
            RemoteDesktopProtocol::Rdp,
            "rdp.test",
            3389,
        ),
        (
            StandaloneConnectionKind::Vnc,
            RemoteDesktopProtocol::Vnc,
            "vnc.test",
            5900,
        ),
    ] {
        let profile = RemoteDesktopConnectionProfile {
            id: "runtime-profile".into(),
            label: "desktop".into(),
            protocol,
            endpoint: RemoteDesktopEndpoint::new(host, port),
            transport_endpoint: Some(RemoteDesktopEndpoint::new("127.0.0.1", 59999)),
            socks_proxy: Some(Arc::new(RemoteDesktopSocksProxy {
                host: "proxy.test".into(),
                port: 1080,
                remote_dns: true,
                no_proxy: String::new(),
                auth: Some(RemoteDesktopProxyAuth {
                    username: "proxy-user".into(),
                    password: RemoteDesktopSecret::new("temporary-proxy-password"),
                }),
            })),
            username: Some("desktop-user".into()),
            domain: Some("DOMAIN".into()),
            credential_ref: None,
            read_only: true,
            session_options: Default::default(),
        };
        desktop_ids.push(
            registry.insert_pending(
                kind,
                host.into(),
                StandaloneConnectionLaunch::RemoteDesktop {
                    profile,
                    provider: provider_registry
                        .get_for_protocol(protocol)
                        .unwrap()
                        .clone(),
                    password: Some(RemoteDesktopSecret::new("temporary-desktop-password")),
                    ssh_gateway_connection_id: None,
                },
            ),
        );
    }
    let persisted = fs::read_to_string(&path).unwrap();
    for secret in [
        "temporary-password",
        "temporary-passphrase",
        "automation-token",
        "temporary-proxy-password",
        "temporary-desktop-password",
        "59999",
        "runtime-profile",
    ] {
        assert!(
            !persisted.contains(secret),
            "runtime value leaked: {secret}"
        );
    }
    let mut restored = StandaloneConnectionRegistry::restore(path.clone(), &store);
    assert_eq!(restored.records().len(), 5);
    let StandaloneConnectionLaunch::Serial { config, .. } =
        &restored.record(&serial_id).unwrap().launch
    else {
        panic!("temporary serial");
    };
    assert_eq!(config, &serial);
    let StandaloneConnectionLaunch::Telnet { config, .. } =
        &restored.record(&telnet_id).unwrap().launch
    else {
        panic!("temporary Telnet");
    };
    assert_eq!((config.host.as_str(), config.port), ("switch.test", 2323));
    let record = restored.record(&mosh_id).unwrap();
    let form = record.reauthentication_form().unwrap();
    assert_eq!(
        form.standalone_connection_id.as_deref(),
        Some(mosh_id.as_str())
    );
    assert!(form.mosh_profile_id.is_none() && !form.save_password && form.password.is_empty());
    assert_eq!(
        (
            form.host.as_str(),
            form.username.as_str(),
            form.mosh_udp_host.as_str()
        ),
        ("mosh.test", "operator", "udp.test")
    );
    assert_eq!(
        (
            form.proxy_hops[0].host.as_str(),
            form.proxy_hops[0].cert_path.as_str(),
            form.proxy_hops[0].passphrase.as_str()
        ),
        ("jump.test", "/keys/jump-cert.pub", "")
    );
    // Preparing and abandoning authentication must not start a transport or change readiness.
    drop(form);
    assert_eq!(record.readiness, ActiveSessionReadiness::Disconnected);
    assert!(record.surface.is_none());
    for id in desktop_ids {
        let form = restored
            .record(&id)
            .unwrap()
            .reauthentication_form()
            .unwrap();
        assert_eq!(form.standalone_connection_id.as_deref(), Some(id.as_str()));
        assert!(
            form.remote_desktop_profile_id.is_none()
                && form.password.is_empty()
                && !form.save_password
        );
        assert_eq!(
            (
                form.upstream_proxy_host.as_str(),
                form.upstream_proxy_username.as_str(),
                form.upstream_proxy_password.as_str()
            ),
            ("proxy.test", "proxy-user", "")
        );
    }
    let attempt = restored.begin_reconnect(&mosh_id).unwrap();
    let options = new_connection::mosh_options_from_profile(&profile);
    restored.replace_launch_for_attempt(
        &attempt,
        "updated shell".into(),
        StandaloneConnectionLaunch::MoshPreflight {
            config: SshConfig {
                host: "updated.test".into(),
                auth: AuthMethod::password("replacement-password"),
                ..Default::default()
            },
            options,
        },
    );
    assert!(restored.bind_surface_for_attempt(
        &attempt,
        StandaloneConnectionSurface::Terminal(TerminalSessionId(99))
    ));
    let restored = StandaloneConnectionRegistry::restore(path.clone(), &store);
    assert_eq!(restored.records().len(), 5);
    let record = restored.record(&mosh_id).unwrap();
    assert_eq!(record.title, "updated shell");
    assert_eq!(record.reauthentication_form().unwrap().host, "updated.test");
    assert_eq!(record.readiness, ActiveSessionReadiness::Disconnected);
    assert!(record.surface.is_none());
    assert!(
        !fs::read_to_string(path)
            .unwrap()
            .contains("replacement-password")
    );
}
