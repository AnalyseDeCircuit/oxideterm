// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

/// Package shape of the running installation used for manifest selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallFlavor {
    MacApp,
    WindowsNsis,
    LinuxAppImage,
    LinuxDeb,
    Portable,
    Standard,
}

impl InstallFlavor {
    // Portable markers take precedence because they define storage and package
    // behavior independently from the executable's platform-specific shape.
    pub fn infer(target: &PlatformTarget, current_exe: &Path, portable: bool) -> Self {
        if portable {
            return Self::Portable;
        }

        match target.os() {
            "macos" => Self::MacApp,
            "windows" => Self::WindowsNsis,
            "linux" if path_is_appimage(current_exe) => Self::LinuxAppImage,
            "linux" => Self::LinuxDeb,
            _ => Self::Standard,
        }
    }
}

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

    // Flavor-specific keys come first. Only installed formats with historical
    // generic aliases fall back, so portable and DEB installs cannot cross over.
    pub fn candidate_keys(&self, flavor: InstallFlavor) -> Vec<String> {
        let arch = self.arch;
        match (self.os, flavor) {
            ("macos", InstallFlavor::MacApp) => vec![
                format!("darwin-{arch}-app"),
                format!("macos-{arch}-app"),
                format!("darwin-{arch}"),
                format!("macos-{arch}"),
                format!("{arch}-apple-darwin"),
            ],
            ("windows", InstallFlavor::WindowsNsis) => vec![
                format!("windows-{arch}-nsis"),
                format!("{arch}-pc-windows-msvc-nsis"),
                format!("windows-{arch}"),
                format!("{arch}-pc-windows-msvc"),
                format!("{arch}-pc-windows-gnu"),
            ],
            ("linux", InstallFlavor::LinuxAppImage) => vec![
                format!("linux-{arch}-appimage"),
                format!("linux-{arch}"),
                format!("{arch}-unknown-linux-gnu"),
                format!("{arch}-unknown-linux-musl"),
            ],
            ("linux", InstallFlavor::LinuxDeb) => vec![
                format!("linux-{arch}-deb"),
                format!("{arch}-unknown-linux-gnu-deb"),
            ],
            ("macos", InstallFlavor::Portable) => vec![
                format!("darwin-{arch}-portable"),
                format!("macos-{arch}-portable"),
                format!("{arch}-apple-darwin-portable"),
            ],
            ("windows", InstallFlavor::Portable) => vec![
                format!("windows-{arch}-portable"),
                format!("{arch}-pc-windows-msvc-portable"),
                format!("{arch}-pc-windows-gnu-portable"),
            ],
            ("linux", InstallFlavor::Portable) => vec![
                format!("linux-{arch}-portable"),
                format!("{arch}-unknown-linux-gnu-portable"),
                format!("{arch}-unknown-linux-musl-portable"),
            ],
            (other, InstallFlavor::Standard) => vec![format!("{other}-{arch}")],
            _ => Vec::new(),
        }
    }
}

fn path_is_appimage(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("appimage"))
        .unwrap_or(false)
}

pub fn current_platform_target() -> PlatformTarget {
    PlatformTarget::new(std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_candidates_match_tauri_manifest_names() {
        let keys = PlatformTarget::new("macos", "aarch64").candidate_keys(InstallFlavor::MacApp);
        assert_eq!(keys.first().map(String::as_str), Some("darwin-aarch64-app"));
        assert!(keys.contains(&"darwin-aarch64".to_string()));
        assert!(keys.contains(&"aarch64-apple-darwin".to_string()));
    }

    #[test]
    fn flavor_inference_covers_supported_install_shapes() {
        assert_eq!(
            InstallFlavor::infer(
                &PlatformTarget::new("windows", "x86_64"),
                Path::new("C:/Users/me/OxideTerm/oxideterm-native.exe"),
                false,
            ),
            InstallFlavor::WindowsNsis
        );
        assert_eq!(
            InstallFlavor::infer(
                &PlatformTarget::new("linux", "x86_64"),
                Path::new("/tmp/OxideTerm.AppImage"),
                false,
            ),
            InstallFlavor::LinuxAppImage
        );
        assert_eq!(
            InstallFlavor::infer(
                &PlatformTarget::new("linux", "x86_64"),
                Path::new("/opt/oxideterm/oxideterm-native"),
                false,
            ),
            InstallFlavor::LinuxDeb
        );
        assert_eq!(
            InstallFlavor::infer(
                &PlatformTarget::new("macos", "aarch64"),
                Path::new("/Applications/OxideTerm.app/Contents/MacOS/oxideterm-native"),
                true,
            ),
            InstallFlavor::Portable
        );
    }

    #[test]
    fn portable_candidates_never_fall_back_to_installed_keys() {
        let keys = PlatformTarget::new("windows", "x86_64").candidate_keys(InstallFlavor::Portable);
        assert_eq!(
            keys.first().map(String::as_str),
            Some("windows-x86_64-portable")
        );
        assert!(!keys.contains(&"windows-x86_64".to_string()));
    }
}
