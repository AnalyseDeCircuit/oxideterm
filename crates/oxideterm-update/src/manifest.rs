// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{PlatformTarget, is_update_newer};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeUpdateManifest {
    pub version: String,
    #[serde(default, alias = "body", alias = "notes")]
    pub body: Option<String>,
    #[serde(default, alias = "date", alias = "pub_date")]
    pub date: Option<String>,
    #[serde(default)]
    pub platforms: BTreeMap<String, NativeUpdateAsset>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeUpdateAsset {
    pub url: String,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeUpdatePackage {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
    pub date: Option<String>,
    pub platform_key: String,
    pub url: String,
    pub signature: Option<String>,
}

impl NativeUpdateManifest {
    pub fn select_package(
        &self,
        current_version: &str,
        target: &PlatformTarget,
    ) -> Option<NativeUpdatePackage> {
        if !is_update_newer(&self.version, current_version) {
            return None;
        }

        let (platform_key, asset) = target
            .candidate_keys()
            .iter()
            .find_map(|key| self.platforms.get_key_value(key))?;

        Some(NativeUpdatePackage {
            version: self.version.clone(),
            current_version: current_version.to_string(),
            body: self.body.clone(),
            date: self.date.clone(),
            platform_key: platform_key.clone(),
            url: asset.url.clone(),
            signature: asset.signature.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tauri_manifest_aliases_and_selects_target() {
        let manifest: NativeUpdateManifest = serde_json::from_str(
            r#"{
              "version": "1.2.0-gpui-preview.1",
              "notes": "Preview notes",
              "pub_date": "2026-05-27T00:00:00Z",
              "platforms": {
                "darwin-aarch64": {
                  "url": "https://example.invalid/OxideTerm.app.tar.gz",
                  "signature": "sig"
                }
              }
            }"#,
        )
        .expect("manifest should parse");

        let target = PlatformTarget::new("macos", "aarch64");
        let package = manifest
            .select_package("1.2.0-gpui-preview.0", &target)
            .expect("newer target package should be selected");

        assert_eq!(package.platform_key, "darwin-aarch64");
        assert_eq!(package.body.as_deref(), Some("Preview notes"));
        assert_eq!(package.signature.as_deref(), Some("sig"));
    }
}
