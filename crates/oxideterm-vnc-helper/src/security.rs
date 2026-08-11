// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::net::ToSocketAddrs;

use native_tls::{Protocol, TlsConnector};
use oxideterm_remote_desktop::{RemoteDesktopVncSecurityPolicy, RemoteDesktopVncSessionMode};
use sha2::{Digest, Sha256};

use super::*;

const VNC_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const VNC_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const VNC_HANDSHAKE_SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(200);
const VNC_SOCKET_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const VNC_SECURITY_TIGHT: u8 = 16;
const VNC_SECURITY_VENCRYPT: u8 = 19;
const VNC_VENCRYPT_VERSION: [u8; 2] = [0, 2];
const VNC_VENCRYPT_TLS_NONE: u32 = 257;
const VNC_VENCRYPT_TLS_VNC: u32 = 258;
const VNC_VENCRYPT_X509_NONE: u32 = 260;
const VNC_VENCRYPT_X509_VNC: u32 = 261;
const MAX_VNC_REASON_BYTES: usize = 64 * 1024;

pub(super) struct VncSecurityPreflight {
    pub(super) protocol_version: VncProtocolVersion,
    pub(super) security: VncSecuritySelection,
    pub(super) offered_security_methods: Vec<String>,
    pub(super) peer_certificate_fingerprint: Option<String>,
    pub(super) tight_active: bool,
    password_challenge: Option<Zeroizing<[u8; 16]>>,
    transport: Option<Box<dyn VncTransport>>,
}

impl VncSecurityPreflight {
    pub(super) fn encrypted(&self) -> bool {
        matches!(
            self.security,
            VncSecuritySelection::TlsNone
                | VncSecuritySelection::TlsVnc
                | VncSecuritySelection::X509None
                | VncSecuritySelection::X509Vnc
        )
    }

    pub(super) fn peer_identity_verified(&self) -> bool {
        matches!(
            self.security,
            VncSecuritySelection::X509None | VncSecuritySelection::X509Vnc
        )
    }

    pub(super) fn requires_password(&self) -> bool {
        matches!(
            self.security,
            VncSecuritySelection::VncAuth
                | VncSecuritySelection::TlsVnc
                | VncSecuritySelection::X509Vnc
        )
    }

    pub(super) fn finish_authentication(
        &mut self,
        password: Option<&RemoteDesktopSecret>,
        session_mode: RemoteDesktopVncSessionMode,
    ) -> VncResult<Box<dyn VncTransport>> {
        let mut transport = self
            .transport
            .take()
            .ok_or_else(|| VncError::protocol("VNC preflight transport is unavailable."))?;
        if let Some(mut challenge) = self.password_challenge.take() {
            let password = password
                .filter(|secret| !secret.is_empty())
                .ok_or_else(|| {
                    VncError::authentication("VNC server requires password authentication.")
                })?;
            let key = vnc_auth_key(password);
            encrypt_vnc_challenge(&key, &mut challenge).map_err(VncError::authentication)?;
            transport
                .write_all(challenge.as_slice())
                .map_err(|error| map_io_error("VNC password response write failed", error))?;
            read_security_result(&mut transport, self.protocol_version)?;
        }
        write_client_init(&mut transport, session_mode)?;
        Ok(transport)
    }
}

pub(super) fn connect_vnc_security_preflight(
    endpoint: &RemoteDesktopEndpoint,
    security_policy: RemoteDesktopVncSecurityPolicy,
    password_available: bool,
    canceled: Arc<AtomicBool>,
) -> VncResult<VncSecurityPreflight> {
    let stream = connect_vnc_tcp(endpoint, canceled.clone())?;
    let mut stream = CancellableTcpStream::new(stream, canceled);
    stream.set_phase_timeout(VNC_HANDSHAKE_TIMEOUT);
    negotiate_vnc_security(&endpoint.host, stream, security_policy, password_available)
}

pub(super) fn negotiate_vnc_security(
    host: &str,
    mut stream: CancellableTcpStream,
    security_policy: RemoteDesktopVncSecurityPolicy,
    password_available: bool,
) -> VncResult<VncSecurityPreflight> {
    let protocol_version = negotiate_protocol_version(&mut stream)?;
    let (security, offered_security_methods, password_challenge, mut transport, tight_active) =
        if protocol_version == VncProtocolVersion::Rfb003003 {
            let (security, offered, challenge, transport) =
                negotiate_rfb33_security(stream, security_policy, password_available)?;
            (security, offered, challenge, transport, false)
        } else {
            negotiate_rfb37_or_38_security(
                host,
                stream,
                protocol_version,
                security_policy,
                password_available,
            )?
        };

    let peer_certificate_fingerprint = if matches!(
        security,
        VncSecuritySelection::X509None | VncSecuritySelection::X509Vnc
    ) {
        let der = transport.peer_certificate_der()?.ok_or_else(|| {
            VncError::certificate("VNC X509 server did not present a certificate.")
        })?;
        Some(sha256_fingerprint(&der))
    } else {
        None
    };

    transport.set_phase_timeout(None);
    Ok(VncSecurityPreflight {
        protocol_version,
        security,
        offered_security_methods,
        peer_certificate_fingerprint,
        tight_active,
        password_challenge,
        transport: Some(transport),
    })
}

fn connect_vnc_tcp(
    endpoint: &RemoteDesktopEndpoint,
    canceled: Arc<AtomicBool>,
) -> VncResult<TcpStream> {
    let addresses = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .map_err(|error| VncError::network(format!("VNC address resolution failed: {error}")))?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err(VncError::network(
            "VNC address resolution returned no addresses.",
        ));
    }

    let deadline = std::time::Instant::now() + VNC_CONNECT_TIMEOUT;
    let mut last_error = None;
    for address in addresses {
        while std::time::Instant::now() < deadline {
            if canceled.load(Ordering::Acquire) {
                return Err(VncError::cancelled());
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let attempt_timeout = remaining.min(VNC_HANDSHAKE_SOCKET_POLL_INTERVAL);
            match TcpStream::connect_timeout(&address, attempt_timeout) {
                Ok(stream) => {
                    stream.set_nodelay(true).map_err(|error| {
                        VncError::network(format!("VNC TCP option setup failed: {error}"))
                    })?;
                    stream
                        .set_read_timeout(Some(VNC_HANDSHAKE_SOCKET_POLL_INTERVAL))
                        .map_err(|error| {
                            VncError::network(format!("VNC read timeout setup failed: {error}"))
                        })?;
                    stream
                        .set_write_timeout(Some(VNC_SOCKET_WRITE_TIMEOUT))
                        .map_err(|error| {
                            VncError::network(format!("VNC write timeout setup failed: {error}"))
                        })?;
                    return Ok(stream);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) =>
                {
                    last_error = Some(error);
                }
                Err(error) => {
                    last_error = Some(error);
                    break;
                }
            }
        }
    }
    let detail = last_error
        .map(|error| error.to_string())
        .unwrap_or_else(|| "connection timed out".to_string());
    Err(VncError::network(format!(
        "VNC TCP connection failed: {detail}"
    )))
}

fn negotiate_protocol_version(stream: &mut CancellableTcpStream) -> VncResult<VncProtocolVersion> {
    let server_version = read_exact_array::<12, _>(stream)
        .map_err(|error| map_io_error("VNC protocol banner read failed", error))?;
    let protocol_version = select_protocol_version(server_version)?;
    stream
        .write_all(protocol_version.banner())
        .map_err(|error| map_io_error("VNC protocol banner write failed", error))?;
    Ok(protocol_version)
}

fn select_protocol_version(server_version: [u8; 12]) -> VncResult<VncProtocolVersion> {
    if &server_version[0..4] != b"RFB "
        || server_version[7] != b'.'
        || server_version[11] != b'\n'
        || !server_version[4..7].iter().all(u8::is_ascii_digit)
        || !server_version[8..11].iter().all(u8::is_ascii_digit)
    {
        return Err(VncError::version(
            "VNC server sent a malformed RFB protocol version.",
        ));
    }
    let major = u16::from(server_version[4] - b'0') * 100
        + u16::from(server_version[5] - b'0') * 10
        + u16::from(server_version[6] - b'0');
    let minor = u16::from(server_version[8] - b'0') * 100
        + u16::from(server_version[9] - b'0') * 10
        + u16::from(server_version[10] - b'0');
    if major != 3 || minor < 3 {
        return Err(VncError::version(
            "VNC server sent an unsupported RFB protocol version.",
        ));
    }
    // RFB version negotiation selects the highest client version that does
    // not exceed the well-formed server declaration.
    Ok(if minor >= 8 {
        VncProtocolVersion::Rfb003008
    } else if minor == 7 {
        VncProtocolVersion::Rfb003007
    } else {
        VncProtocolVersion::Rfb003003
    })
}

fn negotiate_rfb33_security(
    mut stream: CancellableTcpStream,
    security_policy: RemoteDesktopVncSecurityPolicy,
    password_available: bool,
) -> VncResult<(
    VncSecuritySelection,
    Vec<String>,
    Option<Zeroizing<[u8; 16]>>,
    Box<dyn VncTransport>,
)> {
    let security_type = read_be_u32(&mut stream)
        .map_err(|error| map_io_error("VNC security type read failed", error))?;
    let offered = vec![security_method_label(security_type).to_string()];
    if security_policy != RemoteDesktopVncSecurityPolicy::AllowLegacy {
        return Err(VncError::security(
            "VNC server only offers insecure RFB 3.3 security. Enable legacy VNC security explicitly to continue.",
        ));
    }
    match security_type {
        0 => Err(VncError::security(read_limited_reason(&mut stream)?)),
        1 => Ok((VncSecuritySelection::None, offered, None, Box::new(stream))),
        2 => {
            if !password_available {
                return Err(VncError::authentication(
                    "VNC server requires a password, but this connection has no password configured.",
                ));
            }
            let challenge = read_password_challenge(&mut stream)?;
            Ok((
                VncSecuritySelection::VncAuth,
                offered,
                Some(challenge),
                Box::new(stream),
            ))
        }
        other => Err(VncError::security(format!(
            "Unsupported RFB 3.3 security type {other}."
        ))),
    }
}

fn negotiate_rfb37_or_38_security(
    host: &str,
    mut stream: CancellableTcpStream,
    protocol_version: VncProtocolVersion,
    security_policy: RemoteDesktopVncSecurityPolicy,
    password_available: bool,
) -> VncResult<(
    VncSecuritySelection,
    Vec<String>,
    Option<Zeroizing<[u8; 16]>>,
    Box<dyn VncTransport>,
    bool,
)> {
    let count = read_u8(&mut stream)
        .map_err(|error| map_io_error("VNC security list read failed", error))?;
    if count == 0 {
        return Err(VncError::security(read_limited_reason(&mut stream)?));
    }
    let mut security_types = vec![0; usize::from(count)];
    stream
        .read_exact(&mut security_types)
        .map_err(|error| map_io_error("VNC security list read failed", error))?;
    let offered = security_types
        .iter()
        .map(|security_type| security_method_label(u32::from(*security_type)).to_string())
        .collect::<Vec<_>>();

    if security_types.contains(&VNC_SECURITY_VENCRYPT) {
        stream
            .write_all(&[VNC_SECURITY_VENCRYPT])
            .map_err(|error| map_io_error("VNC security selection failed", error))?;
        let (security, offered, challenge, transport) = negotiate_vencrypt(
            host,
            stream,
            protocol_version,
            security_policy,
            password_available,
            offered,
        )?;
        return Ok((security, offered, challenge, transport, false));
    }
    if security_policy == RemoteDesktopVncSecurityPolicy::AllowLegacy {
        if security_types.contains(&VNC_SECURITY_TIGHT) {
            stream
                .write_all(&[VNC_SECURITY_TIGHT])
                .map_err(|error| map_io_error("VNC Tight security selection failed", error))?;
            return negotiate_tight_security(stream, protocol_version, password_available, offered);
        }
        if password_available && security_types.contains(&VNC_SECURITY_VNC_AUTH) {
            stream
                .write_all(&[VNC_SECURITY_VNC_AUTH])
                .map_err(|error| map_io_error("VNC security selection failed", error))?;
            let challenge = read_password_challenge(&mut stream)?;
            return Ok((
                VncSecuritySelection::VncAuth,
                offered,
                Some(challenge),
                Box::new(stream),
                false,
            ));
        }
        if security_types.contains(&VNC_SECURITY_NONE) {
            stream
                .write_all(&[VNC_SECURITY_NONE])
                .map_err(|error| map_io_error("VNC security selection failed", error))?;
            if protocol_version == VncProtocolVersion::Rfb003008 {
                read_security_result(&mut stream, protocol_version)?;
            }
            return Ok((
                VncSecuritySelection::None,
                offered,
                None,
                Box::new(stream),
                false,
            ));
        }
        if security_types.contains(&VNC_SECURITY_VNC_AUTH) {
            return Err(VncError::authentication(
                "VNC server requires a password, but this connection has no password configured.",
            ));
        }
    }
    Err(VncError::security(format!(
        "VNC server does not offer a security method allowed by the selected policy: {offered:?}."
    )))
}

fn negotiate_tight_security(
    mut stream: CancellableTcpStream,
    protocol_version: VncProtocolVersion,
    password_available: bool,
    mut offered: Vec<String>,
) -> VncResult<(
    VncSecuritySelection,
    Vec<String>,
    Option<Zeroizing<[u8; 16]>>,
    Box<dyn VncTransport>,
    bool,
)> {
    let tunnel_count = read_be_u32(&mut stream)
        .map_err(|error| map_io_error("VNC Tight tunnel count read failed", error))?;
    let tunnel_count = usize::try_from(tunnel_count)
        .map_err(|_| VncError::protocol("VNC Tight tunnel count is invalid."))?;
    let tunnels =
        read_tight_capability_list(&mut stream, tunnel_count).map_err(VncError::protocol)?;
    if tunnel_count != 0 {
        let no_tunnel = tunnels
            .iter()
            .any(|capability| capability.is_exact(0, tight_vendor(), *b"NOTUNNEL"));
        if !no_tunnel {
            return Err(VncError::security(
                "VNC Tight server does not offer the registered no-tunnel capability.",
            ));
        }
        stream
            .write_all(&0i32.to_be_bytes())
            .map_err(|error| map_io_error("VNC Tight tunnel selection failed", error))?;
    }

    let auth_count = read_be_u32(&mut stream)
        .map_err(|error| map_io_error("VNC Tight auth count read failed", error))?;
    let auth_count = usize::try_from(auth_count)
        .map_err(|_| VncError::protocol("VNC Tight auth count is invalid."))?;
    let auth_capabilities =
        read_tight_capability_list(&mut stream, auth_count).map_err(VncError::protocol)?;
    let has_none = auth_count == 0
        || auth_capabilities
            .iter()
            .any(|capability| capability.is_exact(1, *b"STDV", *b"NOAUTH__"));
    let has_vnc = auth_capabilities
        .iter()
        .any(|capability| capability.is_exact(2, *b"STDV", *b"VNCAUTH_"));
    offered.extend(auth_capabilities.iter().map(|capability| {
        if capability.is_exact(1, *b"STDV", *b"NOAUTH__") {
            "tight/no-auth".to_string()
        } else if capability.is_exact(2, *b"STDV", *b"VNCAUTH_") {
            "tight/vnc-auth".to_string()
        } else {
            format!("tight/auth-{}", capability.code)
        }
    }));

    let selected_auth = if password_available && has_vnc {
        Some((2i32, VncSecuritySelection::VncAuth))
    } else if has_none {
        Some((1i32, VncSecuritySelection::None))
    } else if has_vnc {
        return Err(VncError::authentication(
            "VNC Tight server requires a password, but this connection has no password configured.",
        ));
    } else {
        None
    };
    let Some((auth_code, security)) = selected_auth else {
        return Err(VncError::security(
            "VNC Tight server does not offer registered None or VNC authentication.",
        ));
    };
    if auth_count != 0 {
        stream
            .write_all(&auth_code.to_be_bytes())
            .map_err(|error| map_io_error("VNC Tight auth selection failed", error))?;
    }

    let challenge = if security == VncSecuritySelection::VncAuth {
        Some(read_password_challenge(&mut stream)?)
    } else {
        if protocol_version == VncProtocolVersion::Rfb003008 {
            read_security_result(&mut stream, protocol_version)?;
        }
        None
    };
    Ok((security, offered, challenge, Box::new(stream), true))
}

fn negotiate_vencrypt(
    host: &str,
    mut stream: CancellableTcpStream,
    protocol_version: VncProtocolVersion,
    security_policy: RemoteDesktopVncSecurityPolicy,
    password_available: bool,
    offered_security_methods: Vec<String>,
) -> VncResult<(
    VncSecuritySelection,
    Vec<String>,
    Option<Zeroizing<[u8; 16]>>,
    Box<dyn VncTransport>,
)> {
    let server_version = read_exact_array::<2, _>(&mut stream)
        .map_err(|error| map_io_error("VeNCrypt version read failed", error))?;
    if server_version != VNC_VENCRYPT_VERSION {
        return Err(VncError::security(format!(
            "Unsupported VeNCrypt version {}.{}.",
            server_version[0], server_version[1]
        )));
    }
    stream
        .write_all(&VNC_VENCRYPT_VERSION)
        .map_err(|error| map_io_error("VeNCrypt version write failed", error))?;
    let version_ack = read_u8(&mut stream)
        .map_err(|error| map_io_error("VeNCrypt version acknowledgement read failed", error))?;
    if version_ack != 0 {
        return Err(VncError::security(
            "VNC server rejected VeNCrypt version 0.2.",
        ));
    }
    let subtype_count = read_u8(&mut stream)
        .map_err(|error| map_io_error("VeNCrypt subtype count read failed", error))?;
    if subtype_count == 0 {
        return Err(VncError::security(
            "VNC server offered no VeNCrypt subtypes.",
        ));
    }
    let mut subtypes = Vec::with_capacity(usize::from(subtype_count));
    for _ in 0..subtype_count {
        subtypes.push(
            read_be_u32(&mut stream)
                .map_err(|error| map_io_error("VeNCrypt subtype read failed", error))?,
        );
    }
    let selected = select_vencrypt_subtype(&subtypes, security_policy, password_available);
    let Some((subtype, security)) = selected else {
        return Err(VncError::security(format!(
            "VNC server does not offer an allowed VeNCrypt subtype: {subtypes:?}."
        )));
    };
    stream
        .write_all(&subtype.to_be_bytes())
        .map_err(|error| map_io_error("VeNCrypt subtype write failed", error))?;
    let subtype_ack = read_u8(&mut stream)
        .map_err(|error| map_io_error("VeNCrypt subtype acknowledgement read failed", error))?;
    if subtype_ack != 1 {
        return Err(VncError::security(
            "VNC server rejected the selected VeNCrypt subtype.",
        ));
    }

    let mut tls = upgrade_vencrypt_tls(host, stream)?;
    let password_challenge = if security.requires_password() {
        Some(read_password_challenge(&mut tls)?)
    } else {
        read_security_result(&mut tls, protocol_version)?;
        None
    };
    let transport: Box<dyn VncTransport> = Box::new(tls);
    let mut offered = offered_security_methods;
    offered.extend(
        subtypes
            .iter()
            .map(|subtype| vencrypt_subtype_label(*subtype).to_string()),
    );
    Ok((security, offered, password_challenge, transport))
}

fn select_vencrypt_subtype(
    subtypes: &[u32],
    security_policy: RemoteDesktopVncSecurityPolicy,
    password_available: bool,
) -> Option<(u32, VncSecuritySelection)> {
    let verified_preference = if password_available {
        [
            (VNC_VENCRYPT_X509_VNC, VncSecuritySelection::X509Vnc),
            (VNC_VENCRYPT_X509_NONE, VncSecuritySelection::X509None),
        ]
    } else {
        [
            (VNC_VENCRYPT_X509_NONE, VncSecuritySelection::X509None),
            (VNC_VENCRYPT_X509_VNC, VncSecuritySelection::X509Vnc),
        ]
    };
    let unverified_preference = if password_available {
        [
            (VNC_VENCRYPT_TLS_VNC, VncSecuritySelection::TlsVnc),
            (VNC_VENCRYPT_TLS_NONE, VncSecuritySelection::TlsNone),
        ]
    } else {
        [
            (VNC_VENCRYPT_TLS_NONE, VncSecuritySelection::TlsNone),
            (VNC_VENCRYPT_TLS_VNC, VncSecuritySelection::TlsVnc),
        ]
    };
    verified_preference
        .into_iter()
        .find(|(subtype, security)| {
            subtypes.contains(subtype) && (!security.requires_password() || password_available)
        })
        .or_else(|| {
            (security_policy != RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption)
                .then(|| {
                    unverified_preference
                        .into_iter()
                        .find(|(subtype, security)| {
                            subtypes.contains(subtype)
                                && (!security.requires_password() || password_available)
                        })
                })
                .flatten()
        })
}

fn upgrade_vencrypt_tls(
    host: &str,
    stream: CancellableTcpStream,
) -> VncResult<native_tls::TlsStream<CancellableTcpStream>> {
    let mut builder = TlsConnector::builder();
    // OxideTerm performs endpoint-scoped fingerprint confirmation before
    // credentials are released, so the platform validator cannot reject the
    // self-signed certificates commonly used by VNC servers first.
    builder.danger_accept_invalid_certs(true);
    builder.danger_accept_invalid_hostnames(true);
    builder.min_protocol_version(Some(Protocol::Tlsv12));
    let connector = builder
        .build()
        .map_err(|error| VncError::tls(format!("VNC TLS setup failed: {error}")))?;
    connector
        .connect(host, stream)
        .map_err(|error| VncError::tls(format!("VNC TLS handshake failed: {error}")))
}

fn read_password_challenge(stream: &mut impl Read) -> VncResult<Zeroizing<[u8; 16]>> {
    read_exact_array::<16, _>(stream)
        .map(Zeroizing::new)
        .map_err(|error| map_io_error("VNC password challenge read failed", error))
}

fn read_security_result(
    stream: &mut impl Read,
    protocol_version: VncProtocolVersion,
) -> VncResult<()> {
    let result = read_be_u32(stream)
        .map_err(|error| map_io_error("VNC security result read failed", error))?;
    if result == 0 {
        return Ok(());
    }
    let reason = if protocol_version == VncProtocolVersion::Rfb003008 {
        read_limited_reason(stream)?
    } else {
        "VNC authentication failed.".to_string()
    };
    Err(VncError::authentication(reason))
}

fn write_client_init(
    stream: &mut impl Write,
    session_mode: RemoteDesktopVncSessionMode,
) -> VncResult<()> {
    let shared = matches!(session_mode, RemoteDesktopVncSessionMode::Shared);
    stream
        .write_all(&[u8::from(shared)])
        .map_err(|error| map_io_error("VNC client init failed", error))
}

fn read_limited_reason(stream: &mut impl Read) -> VncResult<String> {
    let length = read_be_u32(stream)
        .map_err(|error| map_io_error("VNC failure reason length read failed", error))?
        as usize;
    if length > MAX_VNC_REASON_BYTES {
        return Err(VncError::protocol(
            "VNC failure reason exceeds the helper limit.",
        ));
    }
    let bytes = read_exact_vec(stream, length)
        .map_err(|error| map_io_error("VNC failure reason read failed", error))?;
    Ok(sanitize_server_text(&String::from_utf8_lossy(&bytes)))
}

fn sanitize_server_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\t') {
                '\u{fffd}'
            } else {
                character
            }
        })
        .collect()
}

fn map_io_error(stage: &str, error: io::Error) -> VncError {
    let kind = match error.kind() {
        _ if is_vnc_canceled_io(&error) => VncErrorKind::Cancelled,
        io::ErrorKind::TimedOut
        | io::ErrorKind::WouldBlock
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::ConnectionRefused
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::NotConnected
        | io::ErrorKind::UnexpectedEof => VncErrorKind::Network,
        _ => VncErrorKind::Protocol,
    };
    VncError::new(kind, format!("{stage}: {error}"))
}

fn sha256_fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn security_method_label(security_type: u32) -> &'static str {
    match security_type {
        0 => "invalid",
        1 => "none",
        2 => "vnc-auth",
        16 => "tight",
        19 => "vencrypt",
        _ => "unknown",
    }
}

fn vencrypt_subtype_label(subtype: u32) -> &'static str {
    match subtype {
        VNC_VENCRYPT_TLS_NONE => "tls-none",
        VNC_VENCRYPT_TLS_VNC => "tls-vnc",
        VNC_VENCRYPT_X509_NONE => "x509-none",
        VNC_VENCRYPT_X509_VNC => "x509-vnc",
        _ => "unknown-vencrypt",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };

    use super::*;

    fn loopback_pair() -> (CancellableTcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        server
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        (
            CancellableTcpStream::new(client, Arc::new(AtomicBool::new(false))),
            server,
        )
    }

    #[test]
    fn selects_highest_supported_rfb_version_not_above_server() {
        assert_eq!(
            select_protocol_version(*b"RFB 003.009\n").unwrap(),
            VncProtocolVersion::Rfb003008
        );
        assert_eq!(
            select_protocol_version(*b"RFB 003.007\n").unwrap(),
            VncProtocolVersion::Rfb003007
        );
        assert_eq!(
            select_protocol_version(*b"RFB 003.005\n").unwrap(),
            VncProtocolVersion::Rfb003003
        );
    }

    #[test]
    fn rejects_malformed_or_unsupported_rfb_versions() {
        assert!(select_protocol_version(*b"RFB 004.008\n").is_err());
        assert!(select_protocol_version(*b"RFB 003.002\n").is_err());
        assert!(select_protocol_version(*b"RFB 003x008\n").is_err());
        assert!(select_protocol_version(*b"RFB 03A.008\n").is_err());
    }

    #[test]
    fn password_availability_only_changes_vencrypt_subtype_preference() {
        let offered = [
            VNC_VENCRYPT_X509_NONE,
            VNC_VENCRYPT_X509_VNC,
            VNC_VENCRYPT_TLS_NONE,
            VNC_VENCRYPT_TLS_VNC,
        ];
        assert_eq!(
            select_vencrypt_subtype(
                &offered,
                RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption,
                false,
            ),
            Some((VNC_VENCRYPT_X509_NONE, VncSecuritySelection::X509None))
        );
        assert_eq!(
            select_vencrypt_subtype(
                &offered,
                RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption,
                true,
            ),
            Some((VNC_VENCRYPT_X509_VNC, VncSecuritySelection::X509Vnc))
        );
    }

    #[test]
    fn socket_state_machine_obeys_rfb_33_37_and_38_none_semantics() {
        for (banner, expected, send_security_result) in [
            (*b"RFB 003.003\n", VncProtocolVersion::Rfb003003, false),
            (*b"RFB 003.007\n", VncProtocolVersion::Rfb003007, false),
            (*b"RFB 003.008\n", VncProtocolVersion::Rfb003008, true),
        ] {
            let (client, mut server) = loopback_pair();
            let server_thread = thread::spawn(move || {
                server.write_all(&banner).unwrap();
                let mut selected_banner = [0; 12];
                server.read_exact(&mut selected_banner).unwrap();
                assert_eq!(selected_banner, banner);
                if expected == VncProtocolVersion::Rfb003003 {
                    server.write_all(&1_u32.to_be_bytes()).unwrap();
                } else {
                    server.write_all(&[1, VNC_SECURITY_NONE]).unwrap();
                    let mut selection = [0; 1];
                    server.read_exact(&mut selection).unwrap();
                    assert_eq!(selection, [VNC_SECURITY_NONE]);
                    if send_security_result {
                        server.write_all(&0_u32.to_be_bytes()).unwrap();
                    }
                }
            });
            let preflight = negotiate_vnc_security(
                "localhost",
                client,
                RemoteDesktopVncSecurityPolicy::AllowLegacy,
                false,
            )
            .unwrap();
            assert_eq!(preflight.protocol_version, expected);
            assert_eq!(preflight.security, VncSecuritySelection::None);
            server_thread.join().unwrap();
        }
    }

    #[test]
    fn password_response_is_not_sent_before_identity_challenge_acceptance() {
        let (client, mut server) = loopback_pair();
        let server_thread = thread::spawn(move || {
            server.write_all(VNC_PROTOCOL_VERSION_38).unwrap();
            let mut selected_banner = [0; 12];
            server.read_exact(&mut selected_banner).unwrap();
            assert_eq!(&selected_banner, VNC_PROTOCOL_VERSION_38);
            server.write_all(&[1, VNC_SECURITY_VNC_AUTH]).unwrap();
            let mut selection = [0; 1];
            server.read_exact(&mut selection).unwrap();
            assert_eq!(selection, [VNC_SECURITY_VNC_AUTH]);
            server.write_all(&[0xA5; 16]).unwrap();

            let mut unexpected_response = [0; 16];
            let error = server.read_exact(&mut unexpected_response).unwrap_err();
            assert!(matches!(
                error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ));
        });
        let preflight = negotiate_vnc_security(
            "localhost",
            client,
            RemoteDesktopVncSecurityPolicy::AllowLegacy,
            true,
        )
        .unwrap();
        assert_eq!(preflight.security, VncSecuritySelection::VncAuth);
        assert!(preflight.requires_password());
        server_thread.join().unwrap();
    }

    #[test]
    fn tight_security_requires_exact_no_tunnel_and_auth_capabilities() {
        let (client, mut server) = loopback_pair();
        let server_thread = thread::spawn(move || {
            server.write_all(VNC_PROTOCOL_VERSION_38).unwrap();
            let mut selected_banner = [0; 12];
            server.read_exact(&mut selected_banner).unwrap();
            server.write_all(&[1, VNC_SECURITY_TIGHT]).unwrap();
            let mut security_selection = [0; 1];
            server.read_exact(&mut security_selection).unwrap();
            assert_eq!(security_selection, [VNC_SECURITY_TIGHT]);

            server.write_all(&1u32.to_be_bytes()).unwrap();
            server.write_all(&0i32.to_be_bytes()).unwrap();
            server.write_all(b"TGHTNOTUNNEL").unwrap();
            let mut tunnel_selection = [0; 4];
            server.read_exact(&mut tunnel_selection).unwrap();
            assert_eq!(i32::from_be_bytes(tunnel_selection), 0);

            server.write_all(&1u32.to_be_bytes()).unwrap();
            server.write_all(&1i32.to_be_bytes()).unwrap();
            server.write_all(b"STDVNOAUTH__").unwrap();
            let mut auth_selection = [0; 4];
            server.read_exact(&mut auth_selection).unwrap();
            assert_eq!(i32::from_be_bytes(auth_selection), 1);
            server.write_all(&0u32.to_be_bytes()).unwrap();
        });
        let preflight = negotiate_vnc_security(
            "localhost",
            client,
            RemoteDesktopVncSecurityPolicy::AllowLegacy,
            false,
        )
        .unwrap();

        assert!(preflight.tight_active);
        assert_eq!(preflight.security, VncSecuritySelection::None);
        assert!(!preflight.encrypted());
        server_thread.join().unwrap();
    }

    #[test]
    fn cancellation_interrupts_an_idle_security_preflight() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint =
            RemoteDesktopEndpoint::new("127.0.0.1", listener.local_addr().unwrap().port());
        let canceled = Arc::new(AtomicBool::new(false));
        let client_canceled = canceled.clone();
        let client = thread::spawn(move || {
            connect_vnc_security_preflight(
                &endpoint,
                RemoteDesktopVncSecurityPolicy::RequireVerifiedEncryption,
                false,
                client_canceled,
            )
        });
        let (_server, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(50));
        canceled.store(true, Ordering::Release);

        let error = match client.join().unwrap() {
            Ok(_) => panic!("idle VNC preflight should have been canceled"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), VncErrorKind::Cancelled);
    }
}
