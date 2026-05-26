// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde_json::Value;

pub(crate) fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    // Dotted keys are enough for CLI inspection without adding a query language.
    let mut current = value;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = current.get(segment)?;
    }
    Some(current)
}

pub(crate) fn value_to_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => "null".to_string(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn dotted_path_reads_nested_keys() {
        let value = json!({
            "terminal": {
                "fontFamily": "JetBrains Mono"
            }
        });

        assert_eq!(
            value_at_path(&value, "terminal.fontFamily"),
            Some(&json!("JetBrains Mono"))
        );
        assert!(value_at_path(&value, "terminal.missing").is_none());
    }
}
