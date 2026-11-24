//! Submission snapshots contain profile data, never editor state or transport handles.
use crate::form_model::*;
use crate::form_state::*;
use oxideterm_remote_desktop::RemoteDesktopSessionOptions;
use zeroize::Zeroizing;

pub struct PortableConnectionDraft {
    pub name: String,
    pub group: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub color: String,
    pub icon: String,
    pub icon_background_color: String,
    pub terminal: ConnectionTerminalOptions,
    pub pre_connect_command: String,
    pub profile_id: Option<String>,
    pub protocol: PortableProtocolDraft,
}

pub enum PortableProtocolDraft {
    Ssh(PortableSshEndpoint, PortableSshOptions),
    StandaloneSftp {
        primary: PortableSshEndpoint,
        secondary: Option<PortableSshEndpoint>,
    },
    Ftp {
        host: String,
        port: u16,
        username: String,
        password: PortableCredential,
        tls: bool,
        save_password: bool,
        initial_remote_path: String,
        timeout_seconds: u64,
        proxy: PortableUpstreamProxy,
    },
    Mosh {
        ssh: PortableSshEndpoint,
        executable: String,
        udp_host: String,
        udp_port: MoshUdpPortSelection,
        locale: String,
        ip_family: MoshIpFamily,
        prediction: MoshPredictionMode,
    },
    Telnet {
        host: String,
        port: u16,
        input: SerialLineEnding,
        output: SerialLineEnding,
        proxy: PortableUpstreamProxy,
    },
    RemoteDesktop {
        protocol: oxideterm_remote_desktop::RemoteDesktopProtocol,
        host: String,
        port: u16,
        username: String,
        password: PortableCredential,
        empty_password: bool,
        save_password: bool,
        gateway: Option<String>,
        options: RemoteDesktopSessionOptions,
        proxy: PortableUpstreamProxy,
    },
    Tunnel {
        host_id: String,
        kind: super::TunnelKind,
        startup: bool,
        /// Existing rules keep their endpoints when editing metadata.
        endpoints: Option<PortableTunnelEndpoints>,
    },
    Serial {
        path: String,
        baud: u32,
        data_bits: u8,
        stop_bits: u8,
        parity: SerialParity,
        flow_control: SerialFlowControl,
    },
    Local {
        shell_id: Option<String>,
        cwd: String,
    },
}

pub struct PortableTunnelEndpoints {
    pub bind_host: String,
    pub bind_port: u16,
    pub target: Option<(String, u16)>,
}

pub struct PortableSshOptions {
    pub x11: ConnectionX11ForwardingOptions,
    pub dedicated_new_terminal_connection: bool,
    pub open_sftp_on_connect: bool,
    pub channel_strategy: SshChannelStrategy,
    pub post_connect_command: String,
    pub agent_forwarding: bool,
    pub agent_forwarding_socket: Option<String>,
}

pub struct PortableSshEndpoint {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: PortableAuth,
    pub identity_agent: String,
    pub legacy_compatibility: bool,
    pub algorithms: SshAlgorithmPreferences,
    pub timeout_seconds: u64,
    pub initial_remote_path: String,
    pub save_password: bool,
    pub hops: Vec<PortableJumpHost>,
    pub proxy_command: Option<PortableCredential>,
    pub upstream: PortableUpstreamProxy,
}

/// The host resolves a saved credential against the original profile and endpoint.
pub enum PortableCredential {
    KeepSaved,
    Value(Zeroizing<String>),
}
impl PortableCredential {
    fn from_field(value: &str, saved: bool) -> Self {
        if saved && value.is_empty() {
            Self::KeepSaved
        } else {
            Self::Value(secret(value))
        }
    }
}

pub struct PortableAuth {
    pub method: SshAuthTab,
    pub password: Option<PortableCredential>,
    pub empty_password: bool,
    pub key_path: String,
    pub managed_key_id: String,
    pub cert_path: String,
    pub passphrase: Option<Zeroizing<String>>,
    pub gssapi: bool,
    pub server_identity: String,
    pub delegate_credentials: bool,
    pub totp_credential_id: Option<String>,
}
pub enum PortableUpstreamProxy {
    Direct,
    UseGlobal,
    Custom {
        protocol: SavedUpstreamProxyProtocol,
        host: String,
        port: u16,
        credentials: Option<(String, PortableCredential)>,
        remote_dns: bool,
        no_proxy: String,
    },
}
pub struct PortableJumpHost {
    pub saved_connection_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: PortableAuth,
    pub identity_agent: String,
    pub agent_forwarding: bool,
    pub agent_forwarding_socket: Option<String>,
    pub legacy_compatibility: bool,
    pub algorithms: SshAlgorithmPreferences,
}

type Validation<T> = Result<T, &'static str>;
fn required(value: &str) -> Validation<String> {
    if value.trim().is_empty() {
        Err("ssh.form.validation_required")
    } else {
        Ok(value.trim().into())
    }
}
fn port(value: &str) -> Validation<u16> {
    value
        .trim()
        .parse()
        .ok()
        .filter(|p| *p > 0)
        .ok_or("ssh.form.validation_required")
}
fn secret(value: &str) -> Zeroizing<String> {
    Zeroizing::new(value.to_owned())
}

// Both endpoints and jump hosts expose the same authentication controls. Only the active
// method's credentials may cross the host boundary after a user switches methods.
macro_rules! auth {
    ($f:expr,$totp:expr,$saved:expr) => {{
        let f = $f;
        match f.auth_tab {
            SshAuthTab::SshKey if f.key_path.trim().is_empty() => {
                return Err("ssh.form.key_path_required");
            }
            SshAuthTab::ManagedKey if f.managed_key_id.trim().is_empty() => {
                return Err("ssh.form.managed_key_required");
            }
            SshAuthTab::Certificate
                if f.key_path.trim().is_empty() || f.cert_path.trim().is_empty() =>
            {
                return Err("ssh.form.certificate_paths_required");
            }
            _ => {}
        }
        PortableAuth {
            method: f.auth_tab,
            password: (f.auth_tab == SshAuthTab::Password && !f.empty_password).then(|| {
                if $saved {
                    PortableCredential::KeepSaved
                } else {
                    PortableCredential::Value(secret(&f.password))
                }
            }),
            empty_password: f.empty_password,
            key_path: f.key_path.clone(),
            managed_key_id: f.managed_key_id.clone(),
            cert_path: f.cert_path.clone(),
            passphrase: matches!(
                f.auth_tab,
                SshAuthTab::DefaultKey
                    | SshAuthTab::SshKey
                    | SshAuthTab::ManagedKey
                    | SshAuthTab::Certificate
            )
            .then(|| secret(&f.passphrase)),
            gssapi: f.gssapi_enabled,
            server_identity: f.gssapi_server_identity.clone(),
            delegate_credentials: f.gssapi_delegate_credentials,
            totp_credential_id: $totp,
        }
    }};
}
macro_rules! upstream {
    ($f:expr) => {
        match $f.upstream_proxy_policy {
            NewConnectionUpstreamProxyPolicy::Direct => PortableUpstreamProxy::Direct,
            NewConnectionUpstreamProxyPolicy::UseGlobal => PortableUpstreamProxy::UseGlobal,
            NewConnectionUpstreamProxyPolicy::Custom => PortableUpstreamProxy::Custom {
                protocol: $f.upstream_proxy_protocol,
                host: required(&$f.upstream_proxy_host)?,
                port: port(&$f.upstream_proxy_port)?,
                credentials: if $f.upstream_proxy_auth == NewConnectionUpstreamProxyAuth::Password {
                    Some((
                        required(&$f.upstream_proxy_username)?,
                        PortableCredential::from_field(
                            &$f.upstream_proxy_password,
                            $f.upstream_proxy_password_keychain_id.is_some(),
                        ),
                    ))
                } else {
                    None
                },
                remote_dns: $f.upstream_proxy_remote_dns,
                no_proxy: $f.upstream_proxy_no_proxy.clone(),
            },
        }
    };
}
fn hop(f: &NewConnectionProxyHop) -> Validation<PortableJumpHost> {
    Ok(PortableJumpHost {
        host: required(&f.host)?,
        port: port(&f.port)?,
        username: required(&f.username)?,
        auth: auth!(
            f,
            f.totp_credential_id.clone(),
            f.password.is_empty()
                && (!f.saved_connection_id.is_empty() || f.persisted_proxy_hop_index.is_some())
        ),
        saved_connection_id: f.saved_connection_id.clone(),
        identity_agent: f.identity_agent.clone(),
        agent_forwarding: f.agent_forwarding,
        agent_forwarding_socket: f.agent_forwarding_socket.clone(),
        legacy_compatibility: f.legacy_ssh_compatibility,
        algorithms: f.ssh_algorithms.clone(),
    })
}
macro_rules! endpoint {
    ($f:expr,$path:expr,$totp:expr,$saved:expr) => {{
        let f = $f;
        PortableSshEndpoint {
            host: required(&f.host)?,
            port: port(&f.port)?,
            username: required(&f.username)?,
            auth: auth!(f, $totp, $saved),
            identity_agent: f.identity_agent.clone(),
            legacy_compatibility: f.legacy_ssh_compatibility,
            algorithms: f.ssh_algorithms.clone(),
            timeout_seconds: f
                .connect_timeout_seconds_text
                .trim()
                .parse()
                .ok()
                .filter(|v| *v > 0)
                .ok_or("ssh.form.connect_timeout_invalid")?,
            initial_remote_path: $path.clone(),
            save_password: f.save_password,
            hops: f.proxy_hops.iter().map(hop).collect::<Result<_, _>>()?,
            proxy_command: if f.proxy_command_enabled {
                if f.proxy_command.trim().is_empty() && f.proxy_command_keychain_id.is_none() {
                    return Err("ssh.form.validation_required");
                }
                Some(PortableCredential::from_field(
                    &f.proxy_command,
                    f.proxy_command_keychain_id.is_some(),
                ))
            } else {
                None
            },
            upstream: upstream!(f),
        }
    }};
}
impl PortableConnectionDraft {
    pub(super) fn snapshot(
        f: &NewConnectionForm,
        env: &super::ConnectionFormEnvironment,
        saved_ssh_id: Option<String>,
    ) -> Validation<Self> {
        use ConnectionTransport::*;
        let (profile_id, protocol) = if let Some(tunnel) = &env.tunnel {
            required(&f.name)?;
            let host_id = tunnel
                .host_id
                .as_ref()
                .filter(|id| env.connections.iter().any(|c| &c.id == *id))
                .ok_or("ssh_tunnel.choose_host")?
                .clone();
            (
                None,
                PortableProtocolDraft::Tunnel {
                    host_id,
                    kind: tunnel.kind,
                    startup: tunnel.startup,
                    endpoints: if tunnel.editing {
                        None
                    } else {
                        Some(PortableTunnelEndpoints {
                            bind_host: required(&f.host)?,
                            bind_port: port(&f.port)?,
                            target: if tunnel.kind == super::TunnelKind::Dynamic {
                                None
                            } else {
                                Some((
                                    required(&f.tunnel_target_host)?,
                                    port(&f.tunnel_target_port)?,
                                ))
                            },
                        })
                    },
                },
            )
        } else {
            match f.transport {
                Ssh => (
                    saved_ssh_id,
                    PortableProtocolDraft::Ssh(
                        endpoint!(
                            f,
                            f.sftp_initial_remote_path,
                            f.totp_credential_id.clone(),
                            f.saved_password_keychain_id.is_some()
                                && (f.password_from_store
                                    || !f.password_loaded && f.password.is_empty())
                        ),
                        PortableSshOptions {
                            x11: f.x11_forwarding.clone(),
                            dedicated_new_terminal_connection: f.dedicated_new_terminal_connection,
                            open_sftp_on_connect: f.open_sftp_on_connect,
                            channel_strategy: f.ssh_channel_strategy,
                            post_connect_command: f.post_connect_command.clone(),
                            agent_forwarding: f.agent_forwarding,
                            agent_forwarding_socket: f.agent_forwarding_socket.clone(),
                        },
                    ),
                ),
                StandaloneSftp => {
                    let s = &f.standalone_sftp_secondary;
                    (
                        f.standalone_sftp_profile_id.clone(),
                        PortableProtocolDraft::StandaloneSftp {
                            primary: endpoint!(
                                f,
                                f.sftp_initial_remote_path,
                                f.totp_credential_id.clone(),
                                f.saved_password_keychain_id.is_some()
                                    && (f.password_from_store
                                        || !f.password_loaded && f.password.is_empty())
                            ),
                            secondary: if f.standalone_sftp_transfer_mode
                                == StandaloneSftpTransferMode::RemoteRemote
                            {
                                Some(endpoint!(
                                    s,
                                    s.initial_remote_path,
                                    None,
                                    s.password_keychain_id.is_some() && s.password.is_empty()
                                ))
                            } else {
                                None
                            },
                        },
                    )
                }
                Ftp => (
                    f.ftp_profile_id.clone(),
                    PortableProtocolDraft::Ftp {
                        host: required(&f.host)?,
                        port: port(&f.port)?,
                        username: required(&f.username)?,
                        password: PortableCredential::from_field(
                            &f.password,
                            f.saved_password_keychain_id.is_some() && !f.empty_password,
                        ),
                        tls: f.ftp_tls,
                        save_password: f.save_password,
                        initial_remote_path: f.sftp_initial_remote_path.clone(),
                        timeout_seconds: f
                            .connect_timeout_seconds_text
                            .trim()
                            .parse()
                            .ok()
                            .filter(|v| *v > 0)
                            .ok_or("ssh.form.connect_timeout_invalid")?,
                        proxy: upstream!(f),
                    },
                ),
                Mosh => (
                    f.mosh_profile_id.clone(),
                    PortableProtocolDraft::Mosh {
                        ssh: endpoint!(
                            f,
                            f.sftp_initial_remote_path,
                            f.totp_credential_id.clone(),
                            f.saved_password_keychain_id.is_some()
                                && (f.password_from_store
                                    || !f.password_loaded && f.password.is_empty())
                        ),
                        executable: required(&f.mosh_server_executable)?,
                        udp_host: f.mosh_udp_host.clone(),
                        udp_port: parse_mosh_udp_port(&f.mosh_udp_port)
                            .map_err(|_| "mosh.form.invalid_udp_port")?,
                        locale: f.mosh_locale.clone(),
                        ip_family: f.mosh_ip_family,
                        prediction: f.mosh_prediction,
                    },
                ),
                Telnet => (
                    f.telnet_profile_id.clone(),
                    PortableProtocolDraft::Telnet {
                        host: required(&f.host)?,
                        port: port(&f.port)?,
                        input: f.telnet_input_line_ending,
                        output: f.telnet_output_line_ending,
                        proxy: upstream!(f),
                    },
                ),
                Rdp | Vnc => {
                    if f.transport == Rdp
                        && (f.username.trim().is_empty()
                            || (!f.empty_password
                                && f.password.is_empty()
                                && f.remote_desktop_profile_id.is_none()))
                    {
                        return Err("ssh.form.validation_required");
                    }
                    (
                        f.remote_desktop_profile_id.clone(),
                        PortableProtocolDraft::RemoteDesktop {
                            protocol: if f.transport == Rdp {
                                oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp
                            } else {
                                oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc
                            },
                            host: required(&f.host)?,
                            port: port(&f.port)?,
                            username: f.username.clone(),
                            password: PortableCredential::from_field(
                                &f.password,
                                f.saved_password_keychain_id.is_some() && !f.empty_password,
                            ),
                            empty_password: f.empty_password,
                            save_password: f.save_password,
                            gateway: f.remote_desktop_ssh_gateway_connection_id.clone(),
                            options: f.remote_desktop_session_options.clone(),
                            proxy: upstream!(f),
                        },
                    )
                }
                Serial => (
                    f.serial_profile_id.clone(),
                    PortableProtocolDraft::Serial {
                        path: required(&f.serial_port_path)?,
                        baud: f
                            .serial_baud_rate
                            .trim()
                            .parse()
                            .ok()
                            .filter(|v| *v > 0)
                            .ok_or("modals.new_connection.serial_invalid_baud_rate")?,
                        data_bits: f.serial_data_bits,
                        stop_bits: f.serial_stop_bits,
                        parity: f.serial_parity,
                        flow_control: f.serial_flow_control,
                    },
                ),
                LocalTerminal => (
                    f.local_profile_id.clone(),
                    PortableProtocolDraft::Local {
                        shell_id: f.local_shell_id.clone(),
                        cwd: f.local_cwd.clone(),
                    },
                ),
                Ble | WslGraphics => return Err("ssh.form.validation_required"),
            }
        };
        let name = if env.tunnel.is_some() {
            &f.name
        } else {
            match f.transport {
                Serial => &f.serial_profile_name,
                Telnet => &f.telnet_profile_name,
                _ => &f.name,
            }
        };
        Ok(Self {
            profile_id,
            protocol,
            pre_connect_command: f.pre_connect_command.clone(),
            terminal: f.terminal.clone(),
            name: name.clone(),
            group: f.group.clone(),
            notes: f.notes.clone(),
            tags: f.tags.clone(),
            color: f.color.clone(),
            icon: f.icon.clone(),
            icon_background_color: f.icon_background_color.clone(),
        })
    }
}
