// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshCapabilityReport {
    pub default_offer: SshAlgorithmOffer,
    pub legacy_compatibility_offer: SshAlgorithmOffer,
    pub integration: SshIntegrationCapabilities,
    pub limitations: Vec<SshCapabilityLimitation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshAlgorithmOffer {
    pub kex: Vec<String>,
    pub host_key_algorithms: Vec<String>,
    pub ciphers: Vec<String>,
    pub macs: Vec<String>,
    pub compression: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshIntegrationCapabilities {
    pub auth_methods: Vec<&'static str>,
    pub channel_features: Vec<&'static str>,
    pub openssh_extensions: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshCapabilityLimitation {
    pub capability: &'static str,
    pub layer: SshCapabilityLayer,
    pub status: SshCapabilityStatus,
    pub note: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SshCapabilityLayer {
    RusshCore,
    #[serde(rename = "oxideterm-integration")]
    OxideTermIntegration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SshCapabilityStatus {
    Unsupported,
    Partial,
    OptIn,
}

pub fn ssh_capability_report() -> SshCapabilityReport {
    // Keep this report data-only so diagnostics can inspect SSH capabilities
    // without opening a connection or touching user secrets.
    SshCapabilityReport {
        default_offer: algorithm_offer(&russh::Preferred::DEFAULT),
        legacy_compatibility_offer: algorithm_offer(&russh::Preferred::legacy_compatibility()),
        integration: integration_capabilities(),
        limitations: known_limitations(),
    }
}

fn algorithm_offer(preferred: &russh::Preferred) -> SshAlgorithmOffer {
    SshAlgorithmOffer {
        kex: preferred
            .kex
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .collect(),
        host_key_algorithms: preferred
            .key
            .iter()
            .map(|algorithm| algorithm.as_str().to_string())
            .collect(),
        ciphers: preferred
            .cipher
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .collect(),
        macs: preferred
            .mac
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .collect(),
        compression: preferred
            .compression
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .collect(),
    }
}

fn integration_capabilities() -> SshIntegrationCapabilities {
    SshIntegrationCapabilities {
        auth_methods: vec![
            "password",
            "publickey-private-key",
            "publickey-certificate",
            "publickey-managed-key",
            "publickey-agent",
            "keyboard-interactive",
        ],
        channel_features: vec![
            "shell",
            "exec",
            "pty",
            "subsystem",
            "env",
            "window-change",
            "direct-tcpip",
            "tcpip-forward",
            "streamlocal-forward",
            "agent-forwarding",
            "x11-forwarding",
        ],
        openssh_extensions: vec![
            "ext-info-c",
            "server-sig-algs",
            "kex-strict-c-v00@openssh.com",
            "no-more-sessions@openssh.com",
            "auth-agent-req@openssh.com",
            "direct-streamlocal@openssh.com",
            "streamlocal-forward@openssh.com",
        ],
    }
}

fn known_limitations() -> Vec<SshCapabilityLimitation> {
    vec![
        SshCapabilityLimitation {
            capability: "gssapi-with-mic",
            layer: SshCapabilityLayer::RusshCore,
            status: SshCapabilityStatus::Unsupported,
            note: "Kerberos and GSSAPI authentication are not implemented.",
        },
        SshCapabilityLimitation {
            capability: "hostbased",
            layer: SshCapabilityLayer::RusshCore,
            status: SshCapabilityStatus::Partial,
            note: "The method name is represented, but OxideTerm has no usable client auth flow.",
        },
        SshCapabilityLimitation {
            capability: "umac",
            layer: SshCapabilityLayer::RusshCore,
            status: SshCapabilityStatus::Unsupported,
            note: "SHA-2 HMAC and ETM variants are available; UMAC variants are not implemented.",
        },
        SshCapabilityLimitation {
            capability: "direct-fido-security-key",
            layer: SshCapabilityLayer::OxideTermIntegration,
            status: SshCapabilityStatus::Partial,
            note: "Agent-backed security-key signing is the intended path; direct private-key loading is rejected.",
        },
        SshCapabilityLimitation {
            capability: "legacy-ssh-algorithms",
            layer: SshCapabilityLayer::OxideTermIntegration,
            status: SshCapabilityStatus::OptIn,
            note: "Legacy SHA-1 DH, CBC ciphers, and SHA-1 MACs require the per-connection compatibility option.",
        },
        SshCapabilityLimitation {
            capability: "openssh-hostkey-rotation",
            layer: SshCapabilityLayer::OxideTermIntegration,
            status: SshCapabilityStatus::Partial,
            note: "hostkeys-00 reception exists, but full host-key rotation parity is not proven.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_uses_runtime_preferred_algorithm_lists() {
        let report = ssh_capability_report();

        assert!(
            report
                .default_offer
                .kex
                .contains(&"curve25519-sha256".to_string())
        );
        assert!(
            report
                .default_offer
                .kex
                .contains(&"sntrup761x25519-sha512".to_string())
        );
        assert!(
            !report
                .default_offer
                .kex
                .contains(&"diffie-hellman-group14-sha1".to_string())
        );
        assert!(
            report
                .legacy_compatibility_offer
                .kex
                .contains(&"diffie-hellman-group14-sha1".to_string())
        );
    }

    #[test]
    fn report_distinguishes_core_and_integration_limitations() {
        let report = ssh_capability_report();

        assert!(report.limitations.iter().any(|limitation| {
            limitation.capability == "gssapi-with-mic"
                && limitation.layer == SshCapabilityLayer::RusshCore
        }));
        assert!(report.limitations.iter().any(|limitation| {
            limitation.capability == "direct-fido-security-key"
                && limitation.layer == SshCapabilityLayer::OxideTermIntegration
        }));
        assert!(
            report
                .limitations
                .iter()
                .all(|limitation| !limitation.capability.starts_with("sntrup761x25519"))
        );
    }
}
