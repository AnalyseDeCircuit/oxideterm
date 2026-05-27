// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformTarget {
    os: &'static str,
    arch: &'static str,
}

impl PlatformTarget {
    pub const fn new(os: &'static str, arch: &'static str) -> Self {
        Self { os, arch }
    }

    pub fn os(&self) -> &'static str {
        self.os
    }

    pub fn arch(&self) -> &'static str {
        self.arch
    }

    pub fn candidate_keys(&self) -> Vec<String> {
        let arch = self.arch;
        match self.os {
            "macos" => vec![
                format!("darwin-{arch}"),
                format!("macos-{arch}"),
                format!("{arch}-apple-darwin"),
            ],
            "windows" => vec![
                format!("windows-{arch}"),
                format!("{arch}-pc-windows-msvc"),
                format!("{arch}-pc-windows-gnu"),
            ],
            "linux" => vec![
                format!("linux-{arch}"),
                format!("{arch}-unknown-linux-gnu"),
                format!("{arch}-unknown-linux-musl"),
            ],
            other => vec![format!("{other}-{arch}")],
        }
    }
}

pub fn current_platform_target() -> PlatformTarget {
    PlatformTarget::new(std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_candidates_match_tauri_manifest_names() {
        let keys = PlatformTarget::new("macos", "aarch64").candidate_keys();
        assert!(keys.contains(&"darwin-aarch64".to_string()));
        assert!(keys.contains(&"aarch64-apple-darwin".to_string()));
    }
}
