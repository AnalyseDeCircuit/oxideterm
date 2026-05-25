// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Plugin settings input model helpers.

pub fn plugin_setting_input_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Null => String::new(),
        value => value.to_string(),
    }
}

pub fn plugin_setting_draft_to_value(
    setting_type: &str,
    draft: &str,
) -> Result<serde_json::Value, String> {
    match setting_type {
        "string" => Ok(serde_json::Value::String(draft.to_string())),
        "number" => {
            let value = draft.trim().parse::<f64>().map_err(|error| {
                format!("Plugin number setting requires a numeric value: {error}")
            })?;
            serde_json::Number::from_f64(value)
                .map(serde_json::Value::Number)
                .ok_or_else(|| "Plugin number setting cannot be NaN or infinite".to_string())
        }
        // Boolean/select plugin settings are edited by dedicated controls. The
        // shared text-input route should reject stale or mismatched setting ids.
        other => Err(format!(
            "Plugin text input cannot edit setting type \"{other}\""
        )),
    }
}

pub fn parse_focus_handoff_command_list(input: &str) -> Vec<String> {
    let mut commands = Vec::new();
    for token in input.split(|ch: char| ch.is_whitespace() || ch == ',') {
        let token = token.trim().to_lowercase();
        if token.is_empty()
            || !token
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '+' | '-'))
            || commands.iter().any(|existing| existing == &token)
        {
            continue;
        }
        commands.push(token);
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_handoff_commands_are_normalized_deduped_and_validated() {
        assert_eq!(
            parse_focus_handoff_command_list("Ctrl+C ctrl+c, paste bad/slash"),
            vec!["ctrl+c".to_string(), "paste".to_string()]
        );
    }
}
