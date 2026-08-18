// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Mutations and interchange helpers for custom terminal semantic schemes.

use std::time::{SystemTime, UNIX_EPOCH};

use oxideterm_settings::{MAX_CUSTOM_SEMANTIC_SCHEMES, PersistedSettings, TerminalSemanticScheme};
pub use oxideterm_terminal_semantic::{
    MAX_SEMANTIC_RULES, SEMANTIC_CLASSES, SemanticClass, SemanticRuleContext,
    SemanticRuleDefinition, SemanticSchemeDocument,
};
use oxideterm_terminal_semantic::{
    SemanticScheme, built_in_scheme_document, export_scheme_document,
    import_external_scheme_document, validate_scheme_document,
};

pub const CUSTOM_SEMANTIC_SCHEME_PREFIX: &str = "custom:";

pub fn create_custom_semantic_scheme(
    settings: &mut PersistedSettings,
    name: String,
    source: TerminalSemanticScheme,
) -> Result<String, String> {
    ensure_custom_scheme_capacity(settings)?;
    let source = match source {
        TerminalSemanticScheme::Balanced => SemanticScheme::Balanced,
        TerminalSemanticScheme::Conservative => SemanticScheme::Conservative,
    };
    let mut document = built_in_scheme_document(source);
    document.id = unique_custom_scheme_id(settings, &name);
    document.name = normalized_scheme_name(&name)?;
    let id = document.id.clone();
    settings.terminal.custom_semantic_schemes.push(document);
    settings.terminal.semantic_custom_scheme = Some(id.clone());
    Ok(id)
}

pub fn duplicate_custom_semantic_scheme(
    settings: &mut PersistedSettings,
    source_id: &str,
    name: String,
) -> Result<String, String> {
    ensure_custom_scheme_capacity(settings)?;
    let mut document = settings
        .terminal
        .custom_semantic_schemes
        .iter()
        .find(|scheme| scheme.id == source_id)
        .cloned()
        .ok_or_else(|| "Custom semantic scheme not found".to_string())?;
    document.id = unique_custom_scheme_id(settings, &name);
    document.name = normalized_scheme_name(&name)?;
    let id = document.id.clone();
    settings.terminal.custom_semantic_schemes.push(document);
    settings.terminal.semantic_custom_scheme = Some(id.clone());
    Ok(id)
}

pub fn import_custom_semantic_scheme(
    settings: &mut PersistedSettings,
    source: &str,
) -> Result<String, String> {
    import_custom_semantic_scheme_named(settings, source, "Imported Scheme")
}

pub fn import_custom_semantic_scheme_named(
    settings: &mut PersistedSettings,
    source: &str,
    fallback_name: &str,
) -> Result<String, String> {
    ensure_custom_scheme_capacity(settings)?;
    let mut document = import_external_scheme_document(source, fallback_name)?;
    document.id = unique_custom_scheme_id(settings, &document.name);
    let id = document.id.clone();
    settings.terminal.custom_semantic_schemes.push(document);
    settings.terminal.semantic_custom_scheme = Some(id.clone());
    Ok(id)
}

pub fn export_custom_semantic_scheme(
    settings: &PersistedSettings,
    scheme_id: &str,
) -> Result<String, String> {
    let document = settings
        .terminal
        .custom_semantic_schemes
        .iter()
        .find(|scheme| scheme.id == scheme_id)
        .ok_or_else(|| "Custom semantic scheme not found".to_string())?;
    export_scheme_document(document)
}

pub fn update_custom_semantic_scheme(
    settings: &mut PersistedSettings,
    document: SemanticSchemeDocument,
) -> Result<(), String> {
    validate_scheme_document(&document)?;
    if !document.id.starts_with(CUSTOM_SEMANTIC_SCHEME_PREFIX) {
        return Err("Only custom semantic schemes can be updated".to_string());
    }
    let target = settings
        .terminal
        .custom_semantic_schemes
        .iter_mut()
        .find(|scheme| scheme.id == document.id)
        .ok_or_else(|| "Custom semantic scheme not found".to_string())?;
    *target = document;
    Ok(())
}

pub fn delete_custom_semantic_scheme(settings: &mut PersistedSettings, scheme_id: &str) -> bool {
    let original_len = settings.terminal.custom_semantic_schemes.len();
    settings
        .terminal
        .custom_semantic_schemes
        .retain(|scheme| scheme.id != scheme_id);
    let removed = settings.terminal.custom_semantic_schemes.len() != original_len;
    if removed && settings.terminal.semantic_custom_scheme.as_deref() == Some(scheme_id) {
        settings.terminal.semantic_custom_scheme = None;
    }
    if removed {
        settings
            .local_terminal
            .semantic_scheme_by_shell
            .retain(|_, selected_id| selected_id != scheme_id);
    }
    removed
}

pub fn add_custom_semantic_rule(settings: &mut PersistedSettings) -> Result<(), String> {
    let document = active_custom_scheme_mut(settings)?;
    if document.rules.len() >= MAX_SEMANTIC_RULES {
        return Err(format!("Semantic rule limit reached: {MAX_SEMANTIC_RULES}"));
    }
    let mut suffix = document.rules.len() + 1;
    let id = loop {
        let candidate = format!("custom-rule-{suffix}");
        if !document.rules.iter().any(|rule| rule.id == candidate) {
            break candidate;
        }
        suffix += 1;
    };
    document.rules.push(SemanticRuleDefinition {
        id,
        enabled: false,
        pattern: r"\bTODO\b".to_string(),
        capture: 0,
        class: SemanticClass::Info,
        priority: 50,
        context: SemanticRuleContext::Any,
    });
    Ok(())
}

pub fn delete_custom_semantic_rule(settings: &mut PersistedSettings, index: usize) -> bool {
    let Ok(document) = active_custom_scheme_mut(settings) else {
        return false;
    };
    if index >= document.rules.len() {
        return false;
    }
    document.rules.remove(index);
    true
}

pub fn edit_custom_semantic_scheme(
    settings: &mut PersistedSettings,
    edit: impl FnOnce(&mut SemanticSchemeDocument),
) -> Result<(), String> {
    let document = active_custom_scheme_mut(settings)?;
    let mut updated = document.clone();
    edit(&mut updated);
    validate_scheme_document(&updated)?;
    *document = updated;
    Ok(())
}

fn active_custom_scheme_mut(
    settings: &mut PersistedSettings,
) -> Result<&mut SemanticSchemeDocument, String> {
    let active_id = settings
        .terminal
        .semantic_custom_scheme
        .as_deref()
        .ok_or_else(|| "No custom semantic scheme selected".to_string())?;
    settings
        .terminal
        .custom_semantic_schemes
        .iter_mut()
        .find(|scheme| scheme.id == active_id)
        .ok_or_else(|| "Custom semantic scheme not found".to_string())
}

fn ensure_custom_scheme_capacity(settings: &PersistedSettings) -> Result<(), String> {
    if settings.terminal.custom_semantic_schemes.len() >= MAX_CUSTOM_SEMANTIC_SCHEMES {
        Err(format!(
            "Custom semantic scheme limit reached: {MAX_CUSTOM_SEMANTIC_SCHEMES}"
        ))
    } else {
        Ok(())
    }
}

fn normalized_scheme_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        Err("Semantic scheme name is empty or too long".to_string())
    } else {
        Ok(name.to_string())
    }
}

fn unique_custom_scheme_id(settings: &PersistedSettings, name: &str) -> String {
    let slug = slugify_scheme_name(name);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let base = format!("{CUSTOM_SEMANTIC_SCHEME_PREFIX}{slug}-{timestamp}");
    let mut id = base.clone();
    let mut suffix = 2;
    while settings
        .terminal
        .custom_semantic_schemes
        .iter()
        .any(|scheme| scheme.id == id)
    {
        id = format!("{base}-{suffix}");
        suffix += 1;
    }
    id
}

fn slugify_scheme_name(name: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in name.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "scheme".to_string()
    } else {
        slug.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_scheme_crud_preserves_selection_and_interchange() {
        let mut settings = PersistedSettings::default();
        let id = create_custom_semantic_scheme(
            &mut settings,
            "Operations".to_string(),
            TerminalSemanticScheme::Balanced,
        )
        .expect("create scheme");
        let exported = export_custom_semantic_scheme(&settings, &id).expect("export scheme");

        let imported_id =
            import_custom_semantic_scheme(&mut settings, &exported).expect("import scheme");
        assert_ne!(imported_id, id);
        assert_eq!(
            settings.terminal.semantic_custom_scheme.as_deref(),
            Some(imported_id.as_str())
        );
        settings
            .local_terminal
            .semantic_scheme_by_shell
            .insert("bash".to_string(), imported_id.clone());
        assert!(delete_custom_semantic_scheme(&mut settings, &imported_id));
        assert!(settings.terminal.semantic_custom_scheme.is_none());
        assert!(
            settings
                .local_terminal
                .semantic_scheme_for_shell("bash")
                .is_none()
        );
    }

    #[test]
    fn built_in_scheme_ids_cannot_be_updated_as_custom_documents() {
        let mut settings = PersistedSettings::default();
        let document = built_in_scheme_document(SemanticScheme::Balanced);

        assert!(update_custom_semantic_scheme(&mut settings, document).is_err());
    }
}
