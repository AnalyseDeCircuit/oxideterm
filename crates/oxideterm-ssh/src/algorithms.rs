// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::borrow::Cow;

use oxideterm_connections::SshAlgorithmPreferences;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshAlgorithmCategory {
    Kex,
    HostKey,
    Cipher,
    Mac,
    Compression,
}

impl SshAlgorithmCategory {
    pub const ALL: [Self; 5] = [
        Self::Kex,
        Self::HostKey,
        Self::Cipher,
        Self::Mac,
        Self::Compression,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kex => "KEX",
            Self::HostKey => "host key",
            Self::Cipher => "cipher",
            Self::Mac => "MAC",
            Self::Compression => "compression",
        }
    }
}

#[derive(Debug, Error)]
pub enum SshAlgorithmPreferenceError {
    #[error("invalid SSH algorithm preferences")]
    InvalidPreferences,
    #[error("unsupported SSH {category} algorithm: {algorithm}")]
    Unsupported {
        category: &'static str,
        algorithm: String,
    },
}

pub fn preferred_algorithms(
    legacy_compatibility: bool,
    preferences: &SshAlgorithmPreferences,
) -> Result<russh::Preferred, SshAlgorithmPreferenceError> {
    preferences
        .validate()
        .map_err(|_| SshAlgorithmPreferenceError::InvalidPreferences)?;
    let mut preferred = if legacy_compatibility {
        russh::Preferred::legacy_compatibility()
    } else {
        russh::Preferred::DEFAULT
    };

    if !preferences.kex.is_empty() {
        let mut selected = preferences
            .kex
            .iter()
            .map(|name| parse_kex(name))
            .collect::<Result<Vec<_>, _>>()?;
        // Extension markers are protocol capabilities rather than selectable KEX
        // algorithms. Preserve them even when the user replaces the visible list.
        selected.extend(
            preferred
                .kex
                .iter()
                .copied()
                .filter(|name| is_internal_kex_name(name.as_ref())),
        );
        preferred.kex = Cow::Owned(selected);
    }
    if !preferences.host_key.is_empty() {
        preferred.key = Cow::Owned(
            preferences
                .host_key
                .iter()
                .map(|name| parse_host_key(name, &preferred))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if !preferences.cipher.is_empty() {
        preferred.cipher = Cow::Owned(
            preferences
                .cipher
                .iter()
                .map(|name| parse_cipher(name))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if !preferences.mac.is_empty() {
        preferred.mac = Cow::Owned(
            preferences
                .mac
                .iter()
                .map(|name| parse_mac(name))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if !preferences.compression.is_empty() {
        preferred.compression = Cow::Owned(
            preferences
                .compression
                .iter()
                .map(|name| parse_compression(name))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(preferred)
}

pub fn visible_algorithm_names(
    preferred: &russh::Preferred,
    category: SshAlgorithmCategory,
) -> Vec<String> {
    match category {
        SshAlgorithmCategory::Kex => preferred
            .kex
            .iter()
            .filter(|name| !is_internal_kex_name(name.as_ref()))
            .map(|name| name.as_ref().to_string())
            .collect(),
        SshAlgorithmCategory::HostKey => preferred
            .key
            .iter()
            .map(|name| name.as_str().to_string())
            .collect(),
        SshAlgorithmCategory::Cipher => preferred
            .cipher
            .iter()
            .map(|name| name.as_ref().to_string())
            .collect(),
        SshAlgorithmCategory::Mac => preferred
            .mac
            .iter()
            .map(|name| name.as_ref().to_string())
            .collect(),
        SshAlgorithmCategory::Compression => preferred
            .compression
            .iter()
            .map(|name| name.as_ref().to_string())
            .collect(),
    }
}

fn is_internal_kex_name(name: &str) -> bool {
    matches!(
        name,
        "ext-info-c"
            | "ext-info-s"
            | "kex-strict-c-v00@openssh.com"
            | "kex-strict-s-v00@openssh.com"
    )
}

fn parse_kex(name: &str) -> Result<russh::kex::Name, SshAlgorithmPreferenceError> {
    if name == "none" || is_internal_kex_name(name) {
        return Err(unsupported(SshAlgorithmCategory::Kex, name));
    }
    russh::kex::Name::try_from(name).map_err(|_| unsupported(SshAlgorithmCategory::Kex, name))
}

fn parse_host_key(
    name: &str,
    baseline: &russh::Preferred,
) -> Result<russh::keys::Algorithm, SshAlgorithmPreferenceError> {
    let supported = baseline
        .key
        .iter()
        .any(|algorithm| algorithm.as_str() == name)
        || russh::Preferred::DEFAULT
            .key
            .iter()
            .any(|algorithm| algorithm.as_str() == name);
    if !supported {
        return Err(unsupported(SshAlgorithmCategory::HostKey, name));
    }
    russh::keys::Algorithm::new(name).map_err(|_| unsupported(SshAlgorithmCategory::HostKey, name))
}

fn parse_cipher(name: &str) -> Result<russh::cipher::Name, SshAlgorithmPreferenceError> {
    if name == "none" || name == "clear" {
        return Err(unsupported(SshAlgorithmCategory::Cipher, name));
    }
    russh::cipher::Name::try_from(name).map_err(|_| unsupported(SshAlgorithmCategory::Cipher, name))
}

fn parse_mac(name: &str) -> Result<russh::mac::Name, SshAlgorithmPreferenceError> {
    if name == "none" {
        return Err(unsupported(SshAlgorithmCategory::Mac, name));
    }
    russh::mac::Name::try_from(name).map_err(|_| unsupported(SshAlgorithmCategory::Mac, name))
}

fn parse_compression(name: &str) -> Result<russh::compression::Name, SshAlgorithmPreferenceError> {
    russh::compression::Name::try_from(name)
        .map_err(|_| unsupported(SshAlgorithmCategory::Compression, name))
}

fn unsupported(category: SshAlgorithmCategory, algorithm: &str) -> SshAlgorithmPreferenceError {
    SshAlgorithmPreferenceError::Unsupported {
        category: category.as_str(),
        algorithm: algorithm.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_kex_order_keeps_protocol_extension_markers() {
        let baseline =
            visible_algorithm_names(&russh::Preferred::DEFAULT, SshAlgorithmCategory::Kex);
        let selected = vec![baseline[1].clone(), baseline[0].clone()];
        let preferences = SshAlgorithmPreferences {
            kex: selected.clone(),
            ..SshAlgorithmPreferences::default()
        };

        let preferred = preferred_algorithms(false, &preferences).unwrap();

        assert_eq!(
            visible_algorithm_names(&preferred, SshAlgorithmCategory::Kex),
            selected
        );
        assert!(
            preferred
                .kex
                .iter()
                .any(|name| name.as_ref() == "ext-info-c")
        );
        assert!(
            preferred
                .kex
                .iter()
                .any(|name| name.as_ref() == "kex-strict-c-v00@openssh.com")
        );
    }

    #[test]
    fn unsupported_algorithm_is_rejected_before_connection() {
        let preferences = SshAlgorithmPreferences {
            cipher: vec!["unsupported-cipher".to_string()],
            ..SshAlgorithmPreferences::default()
        };

        assert!(matches!(
            preferred_algorithms(false, &preferences),
            Err(SshAlgorithmPreferenceError::Unsupported { .. })
        ));
    }
}
