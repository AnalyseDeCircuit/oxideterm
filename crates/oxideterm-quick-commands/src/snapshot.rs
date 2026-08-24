// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::Deserialize;

use crate::{
    QUICK_COMMANDS_SCHEMA_VERSION, QuickCommand, QuickCommandAvailability, QuickCommandCategory,
    QuickCommandConfirmationPolicy, QuickCommandsSnapshot, v1::QuickCommandsSnapshotV1,
};

const QUICK_COMMANDS_SCHEMA_VERSION_V1: u32 = 1;

#[derive(Deserialize)]
struct SnapshotVersion {
    version: u32,
}

/// Decodes every published native snapshot into the current domain model.
pub fn decode_snapshot_json(json: &str) -> Result<QuickCommandsSnapshot, String> {
    // Version dispatch stays outside serde aliases so only the published V1
    // contract is migrated and malformed hybrids are rejected deterministically.
    let version = serde_json::from_str::<SnapshotVersion>(json)
        .map_err(|error| format!("failed to read Quick Commands snapshot version: {error}"))?
        .version;
    match version {
        QUICK_COMMANDS_SCHEMA_VERSION_V1 => {
            let snapshot = serde_json::from_str::<QuickCommandsSnapshotV1>(json)
                .map_err(|error| format!("failed to decode Quick Commands version 1: {error}"))?;
            Ok(migrate_v1(snapshot))
        }
        QUICK_COMMANDS_SCHEMA_VERSION => serde_json::from_str::<QuickCommandsSnapshot>(json)
            .map_err(|error| format!("failed to decode Quick Commands version 2: {error}")),
        unsupported => Err(format!(
            "Unsupported Quick Commands schema version {unsupported}"
        )),
    }
}

pub fn encode_snapshot_json(snapshot: &QuickCommandsSnapshot) -> Result<String, String> {
    serde_json::to_string_pretty(snapshot).map_err(|error| error.to_string())
}

fn migrate_v1(snapshot: QuickCommandsSnapshotV1) -> QuickCommandsSnapshot {
    debug_assert_eq!(snapshot.version, QUICK_COMMANDS_SCHEMA_VERSION_V1);
    let categories = snapshot
        .categories
        .into_iter()
        .enumerate()
        .map(|(index, category)| QuickCommandCategory {
            id: category.id,
            name: category.name,
            icon: category.icon,
            sort_order: migration_sort_order(index),
        })
        .collect();
    let commands = snapshot
        .commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| QuickCommand {
            id: command.id,
            name: command.name,
            command: command.command,
            category: command.category,
            description: command.description,
            parameters: Vec::new(),
            availability: QuickCommandAvailability {
                protocols: Vec::new(),
                host_patterns: command.host_pattern.into_iter().collect(),
            },
            confirmation: QuickCommandConfirmationPolicy::Inherit,
            sort_order: migration_sort_order(index),
            created_at: command.created_at,
            updated_at: command.updated_at,
        })
        .collect();
    QuickCommandsSnapshot {
        version: QUICK_COMMANDS_SCHEMA_VERSION,
        categories,
        commands,
        updated_at: snapshot.updated_at,
    }
}

fn migration_sort_order(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_v1_snapshot_migrates_to_v2() {
        let snapshot = decode_snapshot_json(
            r#"{
                "version": 1,
                "categories": [{"id":"ops","name":"Ops","icon":"server"}],
                "commands": [{
                    "id":"uptime","name":"Uptime","command":"uptime","category":"ops",
                    "description":"Host uptime","hostPattern":"*.example.com",
                    "createdAt":7,"updatedAt":11
                }],
                "updatedAt":13
            }"#,
        )
        .unwrap();

        assert_eq!(snapshot.version, QUICK_COMMANDS_SCHEMA_VERSION);
        assert_eq!(snapshot.categories[0].id, "ops");
        assert_eq!(snapshot.commands[0].command, "uptime");
        assert_eq!(
            snapshot.commands[0].availability.host_patterns,
            ["*.example.com"]
        );
        assert_eq!(snapshot.commands[0].created_at, 7);
    }
}
