// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use roxmltree::Node;
use serde_json::Value;

use crate::{
    SemanticClass, SemanticScheme, SemanticSchemeDocument, built_in_scheme_document,
    import_scheme_document, validate_scheme_document,
};

const MAX_SCHEME_IMPORT_BYTES: usize = 2 * 1024 * 1024;

pub fn import_external_scheme_document(
    source: &str,
    fallback_name: &str,
) -> Result<SemanticSchemeDocument, String> {
    if source.len() > MAX_SCHEME_IMPORT_BYTES {
        return Err("Semantic scheme file is larger than 2 MiB".to_string());
    }
    let source = source.trim_start_matches('\u{feff}');
    if let Ok(document) = import_scheme_document(source) {
        return Ok(document);
    }

    let fallback_name = normalized_import_name(fallback_name);
    let (name, colors) = if source.contains("<plist") {
        import_textmate_plist(source, &fallback_name)?
    } else {
        import_json_theme(source, &fallback_name)?
    };
    if colors.is_empty() {
        return Err("The imported theme has no supported semantic colors".to_string());
    }

    // External theme formats describe colors, not OxideTerm's runtime rules.
    // Keep the balanced rules and map only scopes with a stable semantic meaning.
    let mut document = built_in_scheme_document(SemanticScheme::Balanced);
    document.id = "custom:imported".to_string();
    document.name = name;
    document.colors = colors;
    validate_scheme_document(&document)?;
    Ok(document)
}

fn import_json_theme(
    source: &str,
    fallback_name: &str,
) -> Result<(String, BTreeMap<SemanticClass, String>), String> {
    let normalized = strip_json_line_comments(source);
    let root: Value = serde_json::from_str(&normalized)
        .map_err(|error| format!("Invalid WindTerm or TextMate JSON theme: {error}"))?;
    let name = root
        .get("name")
        .or_else(|| root.get("displayName"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback_name)
        .to_string();
    let mut colors = BTreeMap::new();

    if let Some(styles) = root.get("styles").and_then(Value::as_array) {
        for style in styles {
            let Some(scope) = style.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(color) = style
                .get("style")
                .and_then(|style| style.get("foreground"))
                .and_then(Value::as_str)
                .and_then(normalize_hex_color)
            else {
                continue;
            };
            insert_scope_color(&mut colors, scope, color);
        }
    }

    if let Some(token_colors) = root.get("tokenColors").and_then(Value::as_array) {
        for rule in token_colors {
            let Some(color) = rule
                .get("settings")
                .and_then(|settings| settings.get("foreground"))
                .and_then(Value::as_str)
                .and_then(normalize_hex_color)
            else {
                continue;
            };
            for scope in json_scopes(rule.get("scope")) {
                insert_scope_color(&mut colors, scope, color.clone());
            }
        }
    }
    Ok((name, colors))
}

fn import_textmate_plist(
    source: &str,
    fallback_name: &str,
) -> Result<(String, BTreeMap<SemanticClass, String>), String> {
    let source = strip_plist_doctype(source);
    let document = roxmltree::Document::parse(&source)
        .map_err(|error| format!("Invalid TextMate plist theme: {error}"))?;
    let root_dict = document
        .descendants()
        .find(|node| node.has_tag_name("dict"))
        .ok_or_else(|| "TextMate theme is missing its root dictionary".to_string())?;
    let name = plist_dict_value(root_dict, "name")
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback_name)
        .to_string();
    let settings = plist_dict_value(root_dict, "settings")
        .filter(|node| node.has_tag_name("array"))
        .ok_or_else(|| "TextMate theme is missing its settings array".to_string())?;
    let mut colors = BTreeMap::new();

    for rule in settings.children().filter(|node| node.has_tag_name("dict")) {
        let Some(scope) = plist_dict_value(rule, "scope").and_then(|node| node.text()) else {
            continue;
        };
        let Some(style) =
            plist_dict_value(rule, "settings").filter(|node| node.has_tag_name("dict"))
        else {
            continue;
        };
        let Some(color) = plist_dict_value(style, "foreground")
            .and_then(|node| node.text())
            .and_then(normalize_hex_color)
        else {
            continue;
        };
        for scope in scope
            .split(',')
            .map(str::trim)
            .filter(|scope| !scope.is_empty())
        {
            insert_scope_color(&mut colors, scope, color.clone());
        }
    }
    Ok((name, colors))
}

fn plist_dict_value<'a, 'input>(dict: Node<'a, 'input>, key: &str) -> Option<Node<'a, 'input>> {
    let mut children = dict.children().filter(Node::is_element);
    while let Some(candidate) = children.next() {
        if candidate.has_tag_name("key") && candidate.text() == Some(key) {
            return children.next();
        }
    }
    None
}

fn json_scopes(value: Option<&Value>) -> Vec<&str> {
    match value {
        Some(Value::String(scope)) => scope.split(',').map(str::trim).collect(),
        Some(Value::Array(scopes)) => scopes.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

fn insert_scope_color(colors: &mut BTreeMap<SemanticClass, String>, scope: &str, color: String) {
    if let Some(class) = semantic_class_for_scope(scope) {
        colors.insert(class, color);
    }
}

fn semantic_class_for_scope(scope: &str) -> Option<SemanticClass> {
    let scope = scope.to_ascii_lowercase();
    if scope.contains("error-token") || scope.contains("invalid") {
        return Some(SemanticClass::Error);
    }
    if scope.contains("warn-token") {
        return Some(SemanticClass::Warning);
    }
    if scope.contains("success-token") {
        return Some(SemanticClass::Success);
    }
    if scope.contains("info-token") {
        return Some(SemanticClass::Info);
    }
    if scope.contains("filename") || scope.contains("path") {
        return Some(SemanticClass::Path);
    }
    if scope.contains("link") || scope.contains("uri") {
        return Some(SemanticClass::Link);
    }
    if scope.contains("comment") {
        return Some(SemanticClass::Comment);
    }
    if scope.contains("string") || scope.contains("character") {
        return Some(SemanticClass::String);
    }
    if scope.contains("constant.numeric") || scope == "number" || scope.starts_with("number.") {
        return Some(SemanticClass::Number);
    }
    if scope.contains("support.function") || scope.contains("entity.name.function") {
        return Some(SemanticClass::Command);
    }
    if scope.contains("variable.language") {
        return Some(SemanticClass::Option);
    }
    if scope.contains("variable") || scope.contains("constant") {
        return Some(SemanticClass::Variable);
    }
    if scope.contains("keyword") || scope.contains("storage") {
        return Some(SemanticClass::Keyword);
    }
    if scope.contains("operator") || scope.contains("punctuation") {
        return Some(SemanticClass::Operator);
    }
    None
}

fn normalize_hex_color(value: &str) -> Option<String> {
    let color = value.split(',').next()?.trim();
    let digits = color.strip_prefix('#')?;
    if digits.len() < 6 || !digits.as_bytes()[..6].iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    Some(format!("#{}", &digits[..6]).to_ascii_lowercase())
}

fn normalized_import_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "Imported Scheme".to_string()
    } else {
        name.chars().take(80).collect()
    }
}

fn strip_json_line_comments(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_plist_doctype(source: &str) -> String {
    // TextMate files carry a remote Apple DTD declaration. roxmltree rejects
    // DTDs by design, and this importer needs no entity definitions from it.
    let Some(start) = source.find("<!DOCTYPE") else {
        return source.to_string();
    };
    let Some(relative_end) = source[start..].find('>') else {
        return source.to_string();
    };
    let mut normalized = source.to_string();
    normalized.replace_range(start..=start + relative_end, "");
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_windterm_scheme_theme_colors() {
        let source = r##"// WindTerm scheme
        {
          "styles": [
            {"name":"token.error-token.linux","style":{"foreground":"#ff3355"}},
            {"name":"support.function.linux","style":{"foreground":"#22aacc"}}
          ]
        }"##;
        let scheme = import_external_scheme_document(source, "Dige Black").expect("import theme");

        assert_eq!(scheme.name, "Dige Black");
        assert_eq!(scheme.colors[&SemanticClass::Error], "#ff3355");
        assert_eq!(scheme.colors[&SemanticClass::Command], "#22aacc");
    }

    #[test]
    fn imports_textmate_plist_scope_colors() {
        let source = r##"<?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0"><dict>
          <key>name</key><string>Ocean</string>
          <key>settings</key><array><dict>
            <key>scope</key><string>comment, punctuation.definition.comment</string>
            <key>settings</key><dict><key>foreground</key><string>#667788</string></dict>
          </dict></array>
        </dict></plist>"##;
        let scheme = import_external_scheme_document(source, "Fallback").expect("import theme");

        assert_eq!(scheme.name, "Ocean");
        assert_eq!(scheme.colors[&SemanticClass::Comment], "#667788");
    }

    #[test]
    fn imports_vscode_token_colors() {
        let source = r##"{
          "name": "Night",
          "tokenColors": [
            {"scope":["keyword.control","storage.type"],"settings":{"foreground":"#AA22CCFF"}}
          ]
        }"##;
        let scheme = import_external_scheme_document(source, "Fallback").expect("import theme");

        assert_eq!(scheme.colors[&SemanticClass::Keyword], "#aa22cc");
    }
}
