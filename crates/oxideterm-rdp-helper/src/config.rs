// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn build_client_rdp_config(config: &RdpWorkerConfig) -> Result<ClientRdpConfig, String> {
    let requested_size = normalized_rdp_desktop_size(config.size);
    let width = u16::try_from(requested_size.width).unwrap_or(u16::MAX);
    let height = u16::try_from(requested_size.height).unwrap_or(u16::MAX);
    let codecs = client_codecs_capabilities(RDP_CLIENT_BITMAP_CODECS)
        .map_err(|error| format!("RDP bitmap codec setup failed: {error}"))?;
    let connector = connector::Config {
        // A smart-card placeholder prevents IronRDP from emitting the
        // username-derived mstshash cookie during certificate preflight.
        // Real credentials replace this value only after the user accepts the
        // certificate bound to the current TLS stream.
        credentials: Credentials::SmartCard {
            pin: String::new(),
            config: None,
        },
        domain: None,
        enable_tls: true,
        enable_credssp: true,
        enable_standard_rdp_security: false,
        desktop_size: connector::DesktopSize { width, height },
        desktop_scale_factor: config.scale_factor,
        keyboard_type: KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_layout: 0,
        keyboard_functional_keys_count: 12,
        connection_type: ConnectionType::Lan,
        ime_file_name: String::new(),
        bitmap: Some(connector::BitmapConfig {
            lossy_compression: true,
            color_depth: 32,
            codecs,
        }),
        dig_product_id: String::new(),
        client_build: client_build_number()?,
        client_name: RDP_CLIENT_NAME.to_string(),
        client_dir: "C:\\Windows\\System32\\mstscax.dll".to_string(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        platform: current_platform_type(),
        hardware_id: None,
        license_cache: None,
        request_data: None,
        autologon: false,
        enable_audio_playback: config.session_options.audio.playback,
        enable_server_pointer: true,
        pointer_software_rendering: false,
        multitransport_flags: None,
        support_dyn_vc_gfx_protocol: !config.session_options.rdp.disable_graphics_pipeline,
        compression_type: Some(CompressionType::Rdp61),
        performance_flags: PerformanceFlags::default(),
        timezone_info: TimezoneInfo::default(),
    };
    log_rdp_client_graphics_config(&connector);

    let destination = ClientRdpDestination::from_parts(&config.endpoint.host, config.endpoint.port);
    let transport_destination = config
        .transport_endpoint
        .as_ref()
        .map(|endpoint| ClientRdpDestination::from_parts(&endpoint.host, endpoint.port))
        .unwrap_or_else(|| destination.clone());
    Ok(ClientRdpConfig {
        destination,
        transport_destination,
        connector,
        graphics_epoch: config.graphics_epoch,
        session_options: config.session_options,
        monitor_layout: config.monitor_layout.clone(),
    })
}

pub(super) fn log_rdp_client_graphics_config(config: &connector::Config) {
    if !remote_rdp_helper_graphics_diagnostics_enabled() {
        return;
    }

    let codec_labels = config
        .bitmap
        .as_ref()
        .map(|bitmap| rdp_bitmap_codec_labels(&bitmap.codecs))
        .unwrap_or_else(|| "bitmap-capabilities-disabled".to_string());
    let bitmap_summary = config.bitmap.as_ref().map(|bitmap| {
        format!(
            "color_depth={} lossy_compression={}",
            bitmap.color_depth, bitmap.lossy_compression
        )
    });

    eprintln!(
        "[oxideterm:rdp-helper-capabilities] requested_size={}x{} scale={} compression={:?} egfx={} bitmap={} codecs={}",
        config.desktop_size.width,
        config.desktop_size.height,
        config.desktop_scale_factor,
        config.compression_type,
        config.support_dyn_vc_gfx_protocol,
        bitmap_summary.unwrap_or_else(|| "disabled".to_string()),
        codec_labels,
    );
}

pub(super) fn log_rdp_negotiated_graphics(config: &connector::Config, result: &ConnectionResult) {
    if !remote_rdp_helper_graphics_diagnostics_enabled() {
        return;
    }

    let codec_labels = config
        .bitmap
        .as_ref()
        .map(|bitmap| rdp_bitmap_codec_labels(&bitmap.codecs))
        .unwrap_or_else(|| "bitmap-capabilities-disabled".to_string());
    eprintln!(
        "[oxideterm:rdp-helper-capabilities] negotiated_size={}x{} compression={:?} server_pointer={} pointer_software={} advertised_codecs={}",
        result.desktop_size.width,
        result.desktop_size.height,
        result.compression_type,
        result.enable_server_pointer,
        result.pointer_software_rendering,
        codec_labels,
    );
}

pub(super) fn rdp_bitmap_codec_labels(codecs: &BitmapCodecs) -> String {
    if codecs.0.is_empty() {
        return "raw-bitmap-fallback".to_string();
    }

    codecs
        .0
        .iter()
        .map(|codec| rdp_bitmap_codec_label(codec.id))
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn client_build_number() -> Result<u32, String> {
    let version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("RDP client version parse failed: {error}"))?;
    let build = version
        .major
        .saturating_mul(100)
        .saturating_add(version.minor.saturating_mul(10))
        .saturating_add(version.patch);
    u32::try_from(build).map_err(|error| format!("RDP client build number overflowed: {error}"))
}

pub(super) fn format_graceful_disconnect(reason: GracefulDisconnectReason) -> String {
    reason.to_string()
}

pub(super) fn connector_error_requires_legacy_security(error: &connector::ConnectorError) -> bool {
    connector_error_search_text(error).contains("STANDARD_RDP_SECURITY")
}

pub(super) fn connector_error_category(
    error: &connector::ConnectorError,
) -> RemoteDesktopErrorCategory {
    match error.kind() {
        _ if connector_error_requires_legacy_security(error) => {
            RemoteDesktopErrorCategory::LegacySecurity
        }
        ConnectorErrorKind::Credssp(_) | ConnectorErrorKind::AccessDenied => {
            RemoteDesktopErrorCategory::Authentication
        }
        ConnectorErrorKind::Negotiation(_)
        | ConnectorErrorKind::Encode(_)
        | ConnectorErrorKind::Decode(_)
        | ConnectorErrorKind::Reason(_) => RemoteDesktopErrorCategory::Protocol,
        ConnectorErrorKind::Custom => RemoteDesktopErrorCategory::Unknown,
        ConnectorErrorKind::General => {
            // IronRDP uses General for certificate-bound credential validation,
            // so recover the user-facing category from its retained context.
            remote_desktop_error_category_from_message(&error.to_string())
        }
        _ => RemoteDesktopErrorCategory::Unknown,
    }
}

pub(super) fn format_connector_error(stage: &str, error: &connector::ConnectorError) -> String {
    match error.kind() {
        _ if connector_error_requires_legacy_security(error) => {
            LEGACY_RDP_SECURITY_MESSAGE.to_string()
        }
        ConnectorErrorKind::Reason(reason) => format!("{stage}: {reason}"),
        ConnectorErrorKind::Negotiation(failure) => format!("{stage}: {failure}"),
        ConnectorErrorKind::Credssp(_) => format!("{stage}: CredSSP authentication failed."),
        ConnectorErrorKind::Encode(_) => {
            format!("{stage}: failed to encode an RDP protocol message.")
        }
        ConnectorErrorKind::Decode(_) => {
            format!("{stage}: failed to decode an RDP protocol message.")
        }
        ConnectorErrorKind::AccessDenied => format!("{stage}: access denied by the RDP server."),
        ConnectorErrorKind::General => {
            let summary = sanitize_connector_error_text(&error.to_string());
            if summary.is_empty() {
                format!("{stage}: general RDP connector error.")
            } else {
                format!("{stage}: {summary}")
            }
        }
        ConnectorErrorKind::Custom => connector_error_source_summary(error)
            .map(|summary| format!("{stage}: {summary}"))
            .unwrap_or_else(|| format!("{stage}: RDP connector error.")),
        _ => connector_error_source_summary(error)
            .map(|summary| format!("{stage}: {summary}"))
            .unwrap_or_else(|| format!("{stage}: RDP connector error.")),
    }
}

pub(super) fn remote_desktop_error_category_from_message(
    message: &str,
) -> RemoteDesktopErrorCategory {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("standard_rdp_security") || message.contains(LEGACY_RDP_SECURITY_MESSAGE)
    {
        RemoteDesktopErrorCategory::LegacySecurity
    } else if normalized.contains("authentication")
        || normalized.contains("access denied")
        || normalized.contains("password")
        || normalized.contains("credssp")
    {
        RemoteDesktopErrorCategory::Authentication
    } else if normalized.contains("tcp")
        || normalized.contains("socket")
        || normalized.contains("network")
        || normalized.contains("transport")
        || normalized.contains("server closed established")
        || normalized.contains("connection reset")
        || normalized.contains("broken pipe")
        || normalized.contains("unexpected eof")
    {
        RemoteDesktopErrorCategory::Network
    } else if normalized.contains("protocol")
        || normalized.contains("decode")
        || normalized.contains("encode")
        || normalized.contains("negotiation")
    {
        RemoteDesktopErrorCategory::Protocol
    } else if normalized.contains("unavailable")
        || normalized.contains("not available")
        || normalized.contains("runtime startup")
    {
        RemoteDesktopErrorCategory::Dependency
    } else {
        RemoteDesktopErrorCategory::Unknown
    }
}

pub(super) fn connector_error_search_text(error: &connector::ConnectorError) -> String {
    let mut parts = vec![error.kind().to_string()];
    parts.extend(connector_error_source_messages(error));
    parts.join(" | ")
}

pub(super) fn connector_error_source_summary(error: &connector::ConnectorError) -> Option<String> {
    let messages = connector_error_source_messages(error);
    if messages.is_empty() {
        None
    } else {
        Some(messages.join("; caused by: "))
    }
}

pub(super) fn connector_error_source_messages(error: &connector::ConnectorError) -> Vec<String> {
    use std::error::Error as _;

    let mut messages = Vec::new();
    let mut source = error.source();
    while let Some(current) = source {
        let message = sanitize_connector_error_text(&current.to_string());
        if !message.is_empty() && !messages.iter().any(|existing| existing == &message) {
            messages.push(message);
        }
        source = current.source();
    }
    messages
}

pub(super) fn sanitize_connector_error_text(message: &str) -> String {
    let mut output = String::with_capacity(message.len());
    let mut cursor = 0;
    while let Some(relative_at) = message[cursor..].find(" @ ") {
        let at = cursor + relative_at;
        let Some(close_relative) = message[at..].find(']') else {
            break;
        };
        let close = at + close_relative;
        let location = &message[at + 3..close];
        if looks_like_source_location(location) {
            // IronRDP's Display includes construction locations. Keep the
            // protocol context but strip local checkout paths before UI output.
            output.push_str(&message[cursor..at]);
            cursor = close;
        } else {
            output.push_str(&message[cursor..at + 3]);
            cursor = at + 3;
        }
    }
    output.push_str(&message[cursor..]);
    output
}

pub(super) fn looks_like_source_location(value: &str) -> bool {
    let Some((path, line)) = value.rsplit_once(':') else {
        return false;
    };
    !path.is_empty()
        && line.chars().all(|character| character.is_ascii_digit())
        && (path.contains('/') || path.contains('\\') || path.ends_with(".rs"))
}

pub(super) fn current_platform_type() -> MajorPlatformType {
    if cfg!(target_os = "windows") {
        MajorPlatformType::WINDOWS
    } else if cfg!(target_os = "macos") {
        MajorPlatformType::MACINTOSH
    } else if cfg!(target_os = "ios") {
        MajorPlatformType::IOS
    } else if cfg!(target_os = "android") {
        MajorPlatformType::ANDROID
    } else {
        MajorPlatformType::UNIX
    }
}
