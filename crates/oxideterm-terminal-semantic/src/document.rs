// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::SemanticClass;

pub const SEMANTIC_SCHEME_FORMAT_VERSION: u32 = 1;
pub const MAX_SEMANTIC_RULES: usize = 128;
pub const MAX_SEMANTIC_PATTERN_LENGTH: usize = 1024;
const MAX_SCHEME_ID_LENGTH: usize = 128;
const MAX_SCHEME_NAME_LENGTH: usize = 80;
const MAX_RULE_ID_LENGTH: usize = 80;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticRuleContext {
    #[default]
    Any,
    Command,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticRuleDefinition {
    pub id: String,
    #[serde(default = "default_rule_enabled")]
    pub enabled: bool,
    pub pattern: String,
    #[serde(default)]
    pub capture: usize,
    pub class: SemanticClass,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub context: SemanticRuleContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSchemeDocument {
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub rules: Vec<SemanticRuleDefinition>,
    #[serde(default)]
    pub colors: BTreeMap<SemanticClass, String>,
}

fn default_rule_enabled() -> bool {
    true
}

pub fn validate_scheme_document(document: &SemanticSchemeDocument) -> Result<(), String> {
    if document.version != SEMANTIC_SCHEME_FORMAT_VERSION {
        return Err(format!(
            "Unsupported semantic scheme version: {}",
            document.version
        ));
    }
    validate_identifier("scheme", &document.id, MAX_SCHEME_ID_LENGTH)?;
    let name = document.name.trim();
    if name.is_empty() || name.chars().count() > MAX_SCHEME_NAME_LENGTH {
        return Err("Semantic scheme name is empty or too long".to_string());
    }
    if document.rules.len() > MAX_SEMANTIC_RULES {
        return Err(format!(
            "Semantic scheme has more than {MAX_SEMANTIC_RULES} rules"
        ));
    }

    let mut rule_ids = HashSet::new();
    for rule in &document.rules {
        validate_identifier("rule", &rule.id, MAX_RULE_ID_LENGTH)?;
        if !rule_ids.insert(rule.id.as_str()) {
            return Err(format!("Duplicate semantic rule ID: {}", rule.id));
        }
        if rule.pattern.is_empty() || rule.pattern.chars().count() > MAX_SEMANTIC_PATTERN_LENGTH {
            return Err(format!("Invalid pattern length for rule: {}", rule.id));
        }
        let regex = Regex::new(&rule.pattern)
            .map_err(|error| format!("Invalid regex for rule {}: {error}", rule.id))?;
        if regex.is_match("") {
            return Err(format!("Rule {} may not match empty text", rule.id));
        }
        if rule.capture >= regex.captures_len() {
            return Err(format!("Invalid capture index for rule: {}", rule.id));
        }
    }

    for color in document.colors.values() {
        if !is_hex_color(color) {
            return Err(format!("Invalid semantic color: {color}"));
        }
    }
    Ok(())
}

pub fn import_scheme_document(json: &str) -> Result<SemanticSchemeDocument, String> {
    let document = serde_json::from_str::<SemanticSchemeDocument>(json)
        .map_err(|error| format!("Invalid semantic scheme JSON: {error}"))?;
    validate_scheme_document(&document)?;
    Ok(document)
}

pub fn export_scheme_document(document: &SemanticSchemeDocument) -> Result<String, String> {
    validate_scheme_document(document)?;
    serde_json::to_string_pretty(document)
        .map_err(|error| format!("Failed to serialize semantic scheme: {error}"))
}

fn validate_identifier(kind: &str, value: &str, max_length: usize) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.len() <= max_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid semantic {kind} ID: {value}"))
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_document() -> SemanticSchemeDocument {
        SemanticSchemeDocument {
            version: SEMANTIC_SCHEME_FORMAT_VERSION,
            id: "custom:operations".to_string(),
            name: "Operations".to_string(),
            rules: vec![SemanticRuleDefinition {
                id: "failed-status".to_string(),
                enabled: true,
                pattern: r"(?i)\b(failed|denied)\b".to_string(),
                capture: 1,
                class: SemanticClass::Error,
                priority: 80,
                context: SemanticRuleContext::Output,
            }],
            colors: BTreeMap::from([(SemanticClass::Error, "#ff5c5c".to_string())]),
        }
    }

    #[test]
    fn scheme_documents_round_trip_without_losing_rules_or_colors() {
        let document = valid_document();
        let json = export_scheme_document(&document).expect("export scheme");
        let imported = import_scheme_document(&json).expect("import scheme");

        assert_eq!(imported, document);
    }

    #[test]
    fn scheme_validation_rejects_unsafe_or_ambiguous_rules() {
        let mut document = valid_document();
        document.rules[0].pattern = "(".to_string();
        assert!(validate_scheme_document(&document).is_err());

        document = valid_document();
        document.rules[0].pattern = ".*".to_string();
        assert!(validate_scheme_document(&document).is_err());

        document = valid_document();
        document.rules[0].capture = 2;
        assert!(validate_scheme_document(&document).is_err());
    }

    #[test]
    fn scheme_validation_rejects_invalid_colors_and_duplicate_ids() {
        let mut document = valid_document();
        document
            .colors
            .insert(SemanticClass::Error, "red".to_string());
        assert!(validate_scheme_document(&document).is_err());

        document = valid_document();
        document.rules.push(document.rules[0].clone());
        assert!(validate_scheme_document(&document).is_err());
    }
}
