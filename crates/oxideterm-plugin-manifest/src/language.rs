// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

/// File associations are shared by installed manifests and the market catalog.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePluginLanguageDefinition {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grammar_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_names: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_declarations_preserve_flat_manifests_and_literal_associations() {
        let legacy = serde_json::json!({"id":"elixir", "highlights":"highlights.scm",
            "parserSha256":"a".repeat(64), "highlightsSha256":"b".repeat(64)});
        let language: crate::NativePluginLanguage = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(serde_json::to_value(language).unwrap(), legacy);
        let definition = NativePluginLanguageDefinition {
            id: "custom-lang".into(),
            display_name: Some("Custom Language".into()),
            grammar_name: Some("elixir".into()),
            extensions: vec!["expr".into(), "custom.expr".into()],
            file_names: vec!["Customfile".into()],
        };
        definition.validate().unwrap();
        for (file, score) in [
            ("a.CUSTOM.EXPR", Some(11)),
            ("Customfile", Some(usize::MAX)),
            ("a.expr", Some(4)),
            ("aexpr", None),
            ("unrelated.txt", None),
        ] {
            assert_eq!(
                definition.match_path(std::path::Path::new(file)),
                score,
                "{file}"
            );
        }
        for invalid in [
            serde_json::json!({"id":"../lang"}),
            serde_json::json!({"id":"custom-lang","grammarName":"tree_sitter/x"}),
            serde_json::json!({"id":"custom-lang","extensions":["*.expr"]}),
            serde_json::json!({"id":"custom-lang","fileNames":["dir/Customfile"]}),
            serde_json::json!({"id":"custom-lang","displayName":"bad\nname"}),
        ] {
            let definition: NativePluginLanguageDefinition =
                serde_json::from_value(invalid).unwrap();
            assert!(definition.validate().is_err());
        }
    }
}

impl NativePluginLanguageDefinition {
    pub fn validate(&self) -> Result<(), String> {
        let valid_id = |value: &str| {
            !value.is_empty()
                && value.len() <= 64
                && value.bytes().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'-')
                })
                && value.as_bytes()[0].is_ascii_lowercase()
        };
        if !valid_id(&self.id)
            || self
                .grammar_name
                .as_deref()
                .is_some_and(|name| !valid_id(name))
        {
            return Err("Invalid language or grammar identifier".into());
        }
        if self.display_name.as_deref().is_some_and(|name| {
            name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control)
        }) {
            return Err("Invalid language display name".into());
        }
        if self.extensions.len() > 64 || self.file_names.len() > 64 {
            return Err("Too many language file associations".into());
        }
        if self.extensions.iter().any(|extension| {
            extension.is_empty()
                || extension.len() > 64
                || extension.starts_with('.')
                || extension.ends_with('.')
                || !extension
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-' | b'+'))
        }) {
            return Err("Invalid language extension".into());
        }
        if self.file_names.iter().any(|name| {
            name.is_empty()
                || name.len() > 256
                || matches!(name.as_str(), "." | "..")
                || name.contains(['/', '\\'])
                || name.chars().any(char::is_control)
        }) {
            return Err("Language file names must be literal file names".into());
        }
        Ok(())
    }

    /// Exact names outrank extensions; compound extensions outrank shorter ones.
    pub fn match_path(&self, path: &std::path::Path) -> Option<usize> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        if self
            .file_names
            .iter()
            .any(|file| file.eq_ignore_ascii_case(&name))
        {
            return Some(usize::MAX);
        }
        self.extensions
            .iter()
            .filter(|extension| name.ends_with(&format!(".{}", extension.to_ascii_lowercase())))
            .map(String::len)
            .max()
    }
}
