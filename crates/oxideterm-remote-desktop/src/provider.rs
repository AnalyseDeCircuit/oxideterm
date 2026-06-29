// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::RemoteDesktopProtocol;

pub const REMOTE_DESKTOP_PROVIDER_MANIFEST: &str = "remote_desktop_provider.json";
const BUILTIN_RDP_PROVIDER_ID: &str = "builtin-rdp";
const BUILTIN_VNC_PROVIDER_ID: &str = "builtin-vnc";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopProviderEntry {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub working_dir: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopProviderCapabilities {
    #[serde(default)]
    pub clipboard_text: bool,
    #[serde(default)]
    pub clipboard_data: bool,
    #[serde(default)]
    pub resize: bool,
    #[serde(default)]
    pub cursor: bool,
    #[serde(default)]
    pub binary_frames: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopProviderUi {
    pub default_port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDesktopProviderManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub version: String,
    pub protocol: RemoteDesktopProtocol,
    pub entry: RemoteDesktopProviderEntry,
    #[serde(default)]
    pub capabilities: RemoteDesktopProviderCapabilities,
    pub ui: Option<RemoteDesktopProviderUi>,
}

impl RemoteDesktopProviderManifest {
    pub fn validate(&self) -> Result<(), RemoteDesktopProviderError> {
        validate_provider_id(&self.id)?;
        require_non_empty("name", &self.name)?;
        require_non_empty("version", &self.version)?;
        require_non_empty("entry.command", &self.entry.command)?;

        if let Some(working_dir) = &self.entry.working_dir {
            require_non_empty("entry.workingDir", working_dir)?;
        }

        Ok(())
    }

    pub fn effective_default_port(&self) -> u16 {
        self.ui
            .as_ref()
            .and_then(|ui| ui.default_port)
            .unwrap_or_else(|| self.protocol.default_port())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteDesktopProviderError {
    #[error("remote desktop provider directory is unavailable: {0}")]
    DirectoryUnavailable(PathBuf),
    #[error("remote desktop provider manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("remote desktop provider manifest could not be read: {0}")]
    ReadFailed(#[from] std::io::Error),
    #[error("remote desktop provider manifest JSON is invalid: {0}")]
    JsonFailed(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Default)]
pub struct RemoteDesktopProviderRegistry {
    providers: BTreeMap<String, RemoteDesktopProviderManifest>,
}

impl RemoteDesktopProviderRegistry {
    pub fn from_manifests(
        manifests: impl IntoIterator<Item = RemoteDesktopProviderManifest>,
    ) -> Result<Self, RemoteDesktopProviderError> {
        let mut providers = BTreeMap::new();
        for manifest in manifests {
            manifest.validate()?;
            providers.insert(manifest.id.clone(), manifest);
        }
        Ok(Self { providers })
    }

    pub fn load_from_dir(path: impl AsRef<Path>) -> Result<Self, RemoteDesktopProviderError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(RemoteDesktopProviderError::DirectoryUnavailable(
                path.to_path_buf(),
            ));
        }

        let mut manifests = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let manifest_path = entry.path().join(REMOTE_DESKTOP_PROVIDER_MANIFEST);
            if manifest_path.exists() {
                manifests.push(read_manifest(&manifest_path)?);
            }
        }

        Self::from_manifests(manifests)
    }

    pub fn get(&self, id: &str) -> Option<&RemoteDesktopProviderManifest> {
        self.providers.get(id)
    }

    pub fn get_for_protocol(
        &self,
        protocol: RemoteDesktopProtocol,
    ) -> Option<&RemoteDesktopProviderManifest> {
        self.providers
            .values()
            .find(|provider| provider.protocol == protocol)
    }

    pub fn providers(&self) -> impl Iterator<Item = &RemoteDesktopProviderManifest> {
        self.providers.values()
    }
}

pub fn read_manifest(
    path: impl AsRef<Path>,
) -> Result<RemoteDesktopProviderManifest, RemoteDesktopProviderError> {
    let manifest = serde_json::from_slice::<RemoteDesktopProviderManifest>(&fs::read(path)?)?;
    manifest.validate()?;
    Ok(manifest)
}

pub fn builtin_provider_manifest(protocol: RemoteDesktopProtocol) -> RemoteDesktopProviderManifest {
    builtin_provider_manifest_with_mode(protocol, false)
}

pub fn builtin_preview_provider_manifest(
    protocol: RemoteDesktopProtocol,
) -> RemoteDesktopProviderManifest {
    builtin_provider_manifest_with_mode(protocol, true)
}

fn builtin_provider_manifest_with_mode(
    protocol: RemoteDesktopProtocol,
    fake_preview: bool,
) -> RemoteDesktopProviderManifest {
    let (id, name, command) = match protocol {
        RemoteDesktopProtocol::Rdp => (
            BUILTIN_RDP_PROVIDER_ID,
            "Built-in RDP Helper",
            "oxideterm-rdp-helper",
        ),
        RemoteDesktopProtocol::Vnc => (
            BUILTIN_VNC_PROVIDER_ID,
            "Built-in VNC Helper",
            "oxideterm-vnc-helper",
        ),
    };
    let mut args = vec!["--stdio".to_string()];
    if fake_preview {
        args.push("--fake".to_string());
    }

    RemoteDesktopProviderManifest {
        id: id.to_string(),
        name: name.to_string(),
        description: "Bundled OxideTerm remote desktop helper.".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        protocol,
        entry: RemoteDesktopProviderEntry {
            command: command.to_string(),
            args,
            working_dir: None,
        },
        capabilities: builtin_provider_capabilities(protocol),
        ui: Some(RemoteDesktopProviderUi {
            default_port: Some(protocol.default_port()),
        }),
    }
}

fn builtin_provider_capabilities(
    protocol: RemoteDesktopProtocol,
) -> RemoteDesktopProviderCapabilities {
    RemoteDesktopProviderCapabilities {
        clipboard_text: true,
        clipboard_data: matches!(protocol, RemoteDesktopProtocol::Rdp),
        resize: matches!(protocol, RemoteDesktopProtocol::Rdp),
        cursor: true,
        binary_frames: true,
    }
}

pub fn builtin_provider_registry()
-> Result<RemoteDesktopProviderRegistry, RemoteDesktopProviderError> {
    RemoteDesktopProviderRegistry::from_manifests([
        builtin_provider_manifest(RemoteDesktopProtocol::Rdp),
        builtin_provider_manifest(RemoteDesktopProtocol::Vnc),
    ])
}

pub fn builtin_preview_provider_registry()
-> Result<RemoteDesktopProviderRegistry, RemoteDesktopProviderError> {
    RemoteDesktopProviderRegistry::from_manifests([
        builtin_preview_provider_manifest(RemoteDesktopProtocol::Rdp),
        builtin_preview_provider_manifest(RemoteDesktopProtocol::Vnc),
    ])
}

fn validate_provider_id(value: &str) -> Result<(), RemoteDesktopProviderError> {
    require_non_empty("id", value)?;
    if value.contains('/') || value.contains('\\') || value.contains("..") {
        return Err(RemoteDesktopProviderError::InvalidManifest(
            "id must not contain path separators or parent directory segments".to_string(),
        ));
    }
    Ok(())
}

fn require_non_empty(field: &str, value: &str) -> Result<(), RemoteDesktopProviderError> {
    if value.trim().is_empty() {
        return Err(RemoteDesktopProviderError::InvalidManifest(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    fn manifest(id: &str, protocol: RemoteDesktopProtocol) -> RemoteDesktopProviderManifest {
        RemoteDesktopProviderManifest {
            id: id.to_string(),
            name: format!("{id} provider"),
            description: String::new(),
            version: "0.1.0".to_string(),
            protocol,
            entry: RemoteDesktopProviderEntry {
                command: format!("{id}-helper"),
                args: vec!["--stdio".to_string()],
                working_dir: None,
            },
            capabilities: RemoteDesktopProviderCapabilities {
                clipboard_text: true,
                clipboard_data: matches!(protocol, RemoteDesktopProtocol::Rdp),
                resize: true,
                cursor: true,
                binary_frames: true,
            },
            ui: None,
        }
    }

    #[test]
    fn rejects_provider_id_with_path_segments() {
        let mut manifest = manifest("../rdp", RemoteDesktopProtocol::Rdp);

        let error = manifest.validate().unwrap_err().to_string();

        assert!(error.contains("path"));
        manifest.id = "rdp".to_string();
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn registry_finds_provider_by_protocol() {
        let registry = RemoteDesktopProviderRegistry::from_manifests([
            manifest("rdp", RemoteDesktopProtocol::Rdp),
            manifest("vnc", RemoteDesktopProtocol::Vnc),
        ])
        .unwrap();

        assert_eq!(
            registry
                .get_for_protocol(RemoteDesktopProtocol::Vnc)
                .unwrap()
                .id,
            "vnc"
        );
    }

    #[test]
    fn registry_loads_manifests_from_provider_directories() {
        let root = unique_temp_dir("remote-desktop-provider-registry");
        let provider_dir = root.join("rdp");
        fs::create_dir_all(&provider_dir).unwrap();
        fs::write(
            provider_dir.join(REMOTE_DESKTOP_PROVIDER_MANIFEST),
            serde_json::to_vec(&manifest("rdp", RemoteDesktopProtocol::Rdp)).unwrap(),
        )
        .unwrap();

        let registry = RemoteDesktopProviderRegistry::load_from_dir(&root).unwrap();

        assert!(registry.get("rdp").is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn builtin_registry_exposes_rdp_and_vnc_helpers() {
        let registry = builtin_provider_registry().unwrap();
        let rdp = registry
            .get_for_protocol(RemoteDesktopProtocol::Rdp)
            .unwrap();
        let vnc = registry
            .get_for_protocol(RemoteDesktopProtocol::Vnc)
            .unwrap();

        assert_eq!(rdp.entry.command, "oxideterm-rdp-helper");
        assert_eq!(rdp.entry.args, vec!["--stdio".to_string()]);
        assert!(rdp.capabilities.clipboard_text);
        assert!(rdp.capabilities.clipboard_data);
        assert!(rdp.capabilities.resize);
        assert!(rdp.capabilities.cursor);
        assert!(rdp.capabilities.binary_frames);

        assert_eq!(vnc.effective_default_port(), 5900);
        assert!(vnc.capabilities.clipboard_text);
        assert!(!vnc.capabilities.clipboard_data);
        assert!(!vnc.capabilities.resize);
        assert!(vnc.capabilities.cursor);
        assert!(vnc.capabilities.binary_frames);
    }

    #[test]
    fn builtin_preview_registry_keeps_fake_helpers_explicit() {
        let registry = builtin_preview_provider_registry().unwrap();

        assert!(
            registry
                .get_for_protocol(RemoteDesktopProtocol::Rdp)
                .unwrap()
                .entry
                .args
                .contains(&"--fake".to_string())
        );
        assert_eq!(
            registry
                .get_for_protocol(RemoteDesktopProtocol::Vnc)
                .unwrap()
                .effective_default_port(),
            5900
        );
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
