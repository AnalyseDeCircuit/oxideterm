// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Pure quick-command editing and filtering behavior shared by UI adapters.

use crate::{
    MAX_CATEGORIES, QuickCommand, QuickCommandAvailability, QuickCommandCategory,
    QuickCommandConfirmationPolicy, QuickCommandIcon, QuickCommandTargetProtocol,
    default_quick_command_categories, new_quick_category_id, new_quick_command_id,
};

#[derive(Clone, Eq, PartialEq)]
pub struct QuickCommandDraft {
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub parameters: Option<Vec<crate::QuickCommandParameter>>,
    pub protocols: Option<Vec<QuickCommandTargetProtocol>>,
    pub host_patterns: Option<Vec<String>>,
    pub confirmation: Option<QuickCommandConfirmationPolicy>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuickCommandCategoryDraft {
    pub id: Option<String>,
    pub name: String,
    pub icon: QuickCommandIcon,
}

pub fn quick_command_draft_can_save(draft: &QuickCommandDraft) -> bool {
    !draft.name.trim().is_empty() && !draft.command.trim().is_empty()
}

pub fn quick_command_category_draft_can_save(draft: &QuickCommandCategoryDraft) -> bool {
    !draft.name.trim().is_empty()
}

pub fn visible_quick_commands(
    commands: &[QuickCommand],
    active_category: &str,
    query: &str,
    target_fields: &[String],
) -> Vec<QuickCommand> {
    let normalized_query = query.trim().to_lowercase();
    commands
        .iter()
        .filter(|command| command.category == active_category)
        .filter(|command| {
            match_quick_command_host_patterns(&command.availability.host_patterns, target_fields)
        })
        .filter(|command| quick_command_matches_query(command, &normalized_query))
        .cloned()
        .collect()
}

pub fn visible_quick_commands_for_management(
    commands: &[QuickCommand],
    active_category: &str,
    query: &str,
) -> Vec<QuickCommand> {
    // Management must expose unavailable commands so users can repair their
    // host and protocol constraints without switching terminal context.
    let normalized_query = query.trim().to_lowercase();
    commands
        .iter()
        .filter(|command| command.category == active_category)
        .filter(|command| quick_command_matches_query(command, &normalized_query))
        .cloned()
        .collect()
}

fn quick_command_matches_query(command: &QuickCommand, normalized_query: &str) -> bool {
    normalized_query.is_empty()
        || command.name.to_lowercase().contains(normalized_query)
        || command.command.to_lowercase().contains(normalized_query)
        || command
            .description
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(normalized_query)
}

pub fn upsert_quick_command(
    commands: &mut Vec<QuickCommand>,
    categories: &[QuickCommandCategory],
    draft: QuickCommandDraft,
    now: u64,
) -> bool {
    // Creation time is stable across edits; update time records each accepted draft.
    let existing = draft
        .id
        .as_ref()
        .and_then(|id| commands.iter().find(|command| &command.id == id));
    let existing_created_at = existing.map(|command| command.created_at);
    let existing_category = existing
        .map(|command| command.category.clone())
        .unwrap_or_else(|| "custom".to_string());
    let existing_description = existing.and_then(|command| command.description.clone());
    let existing_parameters = existing
        .map(|command| command.parameters.clone())
        .unwrap_or_default();
    let existing_protocols = existing
        .map(|command| command.availability.protocols.clone())
        .unwrap_or_default();
    let existing_host_patterns = existing
        .map(|command| command.availability.host_patterns.clone())
        .unwrap_or_default();
    let existing_confirmation = existing
        .map(|command| command.confirmation)
        .unwrap_or_default();
    let existing_sort_order = existing
        .map(|command| command.sort_order)
        .unwrap_or_else(|| next_command_sort_order(commands));
    let command = QuickCommand {
        id: draft.id.unwrap_or_else(new_quick_command_id),
        name: draft.name.trim().to_string(),
        command: draft.command.trim().to_string(),
        category: draft.category.map_or(existing_category, |category| {
            if categories.iter().any(|item| item.id == category) {
                category
            } else {
                "custom".to_string()
            }
        }),
        description: draft
            .description
            .map(|description| trim_optional(&description))
            .unwrap_or(existing_description),
        parameters: draft.parameters.unwrap_or(existing_parameters),
        availability: QuickCommandAvailability {
            protocols: draft.protocols.unwrap_or(existing_protocols),
            host_patterns: draft
                .host_patterns
                .map_or(existing_host_patterns, |patterns| {
                    patterns
                        .into_iter()
                        .filter_map(|pattern| trim_optional(&pattern))
                        .collect()
                }),
        },
        confirmation: draft.confirmation.unwrap_or(existing_confirmation),
        sort_order: existing_sort_order,
        created_at: existing_created_at.unwrap_or(now),
        updated_at: now,
    };
    if command.name.is_empty() || command.command.is_empty() {
        return false;
    }
    if crate::validate_quick_command_template(&command.command, &command.parameters).is_err() {
        return false;
    }

    if let Some(existing) = commands.iter_mut().find(|item| item.id == command.id) {
        *existing = command;
    } else {
        commands.push(command);
    }
    true
}

pub fn delete_quick_command(commands: &mut Vec<QuickCommand>, id: &str) -> bool {
    let previous_len = commands.len();
    commands.retain(|command| command.id != id);
    commands.len() != previous_len
}

pub fn upsert_quick_command_category(
    categories: &mut Vec<QuickCommandCategory>,
    draft: QuickCommandCategoryDraft,
    current_active_category: &str,
) -> String {
    let existing_sort_order = draft
        .id
        .as_ref()
        .and_then(|id| categories.iter().find(|category| &category.id == id))
        .map(|category| category.sort_order)
        .unwrap_or_else(|| next_category_sort_order(categories));
    let category = QuickCommandCategory {
        id: draft.id.unwrap_or_else(new_quick_category_id),
        name: draft.name.trim().to_string(),
        icon: draft.icon,
        sort_order: existing_sort_order,
    };
    if category.name.is_empty() {
        return current_active_category.to_string();
    }

    if let Some(existing) = categories.iter_mut().find(|item| item.id == category.id) {
        *existing = category.clone();
    } else if categories.len() < MAX_CATEGORIES {
        categories.push(category.clone());
    }
    category.id
}

pub fn delete_quick_command_category(
    categories: &mut Vec<QuickCommandCategory>,
    commands: &[QuickCommand],
    id: &str,
) -> bool {
    if default_quick_command_categories()
        .iter()
        .any(|category| category.id == id)
        || commands.iter().any(|command| command.category == id)
    {
        return false;
    }
    let previous_len = categories.len();
    categories.retain(|category| category.id != id);
    categories.len() != previous_len
}

pub fn ensure_active_quick_command_category(
    categories: &[QuickCommandCategory],
    active_category: &mut String,
) {
    if categories
        .iter()
        .any(|category| category.id == *active_category)
    {
        return;
    }
    *active_category = categories
        .first()
        .map(|category| category.id.clone())
        .unwrap_or_else(|| "custom".to_string());
}

pub fn match_quick_command_host_pattern(pattern: Option<&str>, target_fields: &[String]) -> bool {
    let Some(pattern) = pattern.map(str::trim).filter(|pattern| !pattern.is_empty()) else {
        return true;
    };
    let normalized_pattern = pattern.to_lowercase();
    target_fields
        .iter()
        .any(|field| wildcard_match(&normalized_pattern, &field.to_lowercase()))
}

pub fn match_quick_command_host_patterns(patterns: &[String], target_fields: &[String]) -> bool {
    patterns.is_empty()
        || patterns
            .iter()
            .any(|pattern| match_quick_command_host_pattern(Some(pattern.as_str()), target_fields))
}

pub fn quick_command_available_for_target(
    command: &QuickCommand,
    protocol: QuickCommandTargetProtocol,
    target_fields: &[String],
) -> bool {
    let protocol_matches = command.availability.protocols.is_empty()
        || command.availability.protocols.contains(&protocol);
    let host_matches =
        match_quick_command_host_patterns(&command.availability.host_patterns, target_fields);
    protocol_matches && host_matches
}

fn next_command_sort_order(commands: &[QuickCommand]) -> i64 {
    commands
        .iter()
        .map(|command| command.sort_order)
        .max()
        .unwrap_or(-1)
        .saturating_add(1)
}

fn next_category_sort_order(categories: &[QuickCommandCategory]) -> i64 {
    categories
        .iter()
        .map(|category| category.sort_order)
        .max()
        .unwrap_or(-1)
        .saturating_add(1)
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return pattern == value;
    }
    let mut cursor = 0;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(found) = value[cursor..].find(part) else {
            return false;
        };
        if index == 0 && found != 0 {
            return false;
        }
        cursor += found + part.len();
    }
    pattern.ends_with('*') || parts.last().is_none_or(|last| value.ends_with(last))
}

fn trim_optional(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_upsert_normalizes_fields_and_preserves_creation_time() {
        let categories = default_quick_command_categories();
        let mut commands = vec![QuickCommand {
            id: "existing".to_string(),
            name: "Old".to_string(),
            command: "old".to_string(),
            category: "custom".to_string(),
            description: None,
            parameters: Vec::new(),
            availability: QuickCommandAvailability::default(),
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            sort_order: 0,
            created_at: 7,
            updated_at: 7,
        }];
        assert!(upsert_quick_command(
            &mut commands,
            &categories,
            QuickCommandDraft {
                id: Some("existing".to_string()),
                name: " Updated ".to_string(),
                command: " echo ready ".to_string(),
                category: Some("missing".to_string()),
                description: Some("  ".to_string()),
                parameters: None,
                protocols: None,
                host_patterns: Some(vec![" *.example.com ".to_string()]),
                confirmation: None,
            },
            11,
        ));
        assert_eq!(commands[0].name, "Updated");
        assert_eq!(commands[0].category, "custom");
        assert_eq!(commands[0].created_at, 7);
        assert_eq!(commands[0].updated_at, 11);
        assert_eq!(commands[0].availability.host_patterns, ["*.example.com"]);
    }

    #[test]
    fn command_upsert_preserves_advanced_fields_when_patch_omits_them() {
        let categories = default_quick_command_categories();
        let mut commands = vec![QuickCommand {
            id: "existing".to_string(),
            name: "Old".to_string(),
            command: "old".to_string(),
            category: "files".to_string(),
            description: Some("kept".to_string()),
            parameters: vec![crate::QuickCommandParameter {
                name: "path".to_string(),
                label: "Path".to_string(),
                required: true,
                ..crate::QuickCommandParameter::default()
            }],
            availability: QuickCommandAvailability {
                protocols: vec![QuickCommandTargetProtocol::Ssh],
                host_patterns: vec!["one.example.com".to_string(), "two.example.com".to_string()],
            },
            confirmation: QuickCommandConfirmationPolicy::Always,
            sort_order: 0,
            created_at: 7,
            updated_at: 7,
        }];

        assert!(upsert_quick_command(
            &mut commands,
            &categories,
            QuickCommandDraft {
                id: Some("existing".to_string()),
                name: "New".to_string(),
                command: "new".to_string(),
                category: None,
                description: None,
                parameters: None,
                protocols: None,
                host_patterns: None,
                confirmation: None,
            },
            11,
        ));

        assert_eq!(commands[0].category, "files");
        assert_eq!(commands[0].description.as_deref(), Some("kept"));
        assert_eq!(commands[0].parameters[0].name, "path");
        assert_eq!(
            commands[0].availability.protocols,
            [QuickCommandTargetProtocol::Ssh]
        );
        assert_eq!(commands[0].availability.host_patterns.len(), 2);
        assert_eq!(
            commands[0].confirmation,
            QuickCommandConfirmationPolicy::Always
        );
    }

    #[test]
    fn host_pattern_matching_preserves_anchored_wildcard_semantics() {
        let targets = vec!["prod.example.com".to_string()];
        assert!(match_quick_command_host_pattern(
            Some("*.example.com"),
            &targets
        ));
        assert!(!match_quick_command_host_pattern(
            Some("example.*"),
            &targets
        ));
    }
}
