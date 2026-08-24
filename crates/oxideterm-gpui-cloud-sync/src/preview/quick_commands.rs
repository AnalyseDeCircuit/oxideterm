// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn quick_command_changed_fields(
    before: &QuickCommand,
    after: &QuickCommand,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(before.name.clone()),
        Some(after.name.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.command",
        Some(before.command.clone()),
        Some(after.command.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.category",
        Some(before.category.clone()),
        Some(after.category.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.description",
        before.description.clone(),
        after.description.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.host_pattern",
        serialized_field(&before.availability.host_patterns),
        serialized_field(&after.availability.host_patterns),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.protocols",
        serialized_field(&before.availability.protocols),
        serialized_field(&after.availability.protocols),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.parameters",
        parameter_field(&before.parameters),
        parameter_field(&after.parameters),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.confirmation",
        serialized_field(&before.confirmation),
        serialized_field(&after.confirmation),
    );
    fields
}

pub(super) fn quick_command_merge_fields(
    base: &QuickCommand,
    local: &QuickCommand,
    remote: &QuickCommand,
    effective: &QuickCommand,
    conflict_strategy: &ConflictStrategy,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.name",
        Some(base.name.clone()),
        Some(local.name.clone()),
        Some(remote.name.clone()),
        Some(effective.name.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.command",
        Some(base.command.clone()),
        Some(local.command.clone()),
        Some(remote.command.clone()),
        Some(effective.command.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.category",
        Some(base.category.clone()),
        Some(local.category.clone()),
        Some(remote.category.clone()),
        Some(effective.category.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.description",
        base.description.clone(),
        local.description.clone(),
        remote.description.clone(),
        effective.description.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.host_pattern",
        serialized_field(&base.availability.host_patterns),
        serialized_field(&local.availability.host_patterns),
        serialized_field(&remote.availability.host_patterns),
        serialized_field(&effective.availability.host_patterns),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.protocols",
        serialized_field(&base.availability.protocols),
        serialized_field(&local.availability.protocols),
        serialized_field(&remote.availability.protocols),
        serialized_field(&effective.availability.protocols),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.parameters",
        parameter_field(&base.parameters),
        parameter_field(&local.parameters),
        parameter_field(&remote.parameters),
        parameter_field(&effective.parameters),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.confirmation",
        serialized_field(&base.confirmation),
        serialized_field(&local.confirmation),
        serialized_field(&remote.confirmation),
        serialized_field(&effective.confirmation),
        conflict_strategy,
    );
    fields
}

fn serialized_field(value: &impl std::fmt::Debug) -> Option<String> {
    Some(format!("{value:?}"))
}

fn parameter_field(
    parameters: &[oxideterm_quick_commands::QuickCommandParameter],
) -> Option<String> {
    // Default values can contain credentials; cloud-sync diffs expose only the
    // structural parameter contract and never the stored substitution value.
    Some(
        parameters
            .iter()
            .map(|parameter| {
                format!(
                    "{} ({:?}, {})",
                    parameter.name,
                    parameter.kind,
                    if parameter.required {
                        "required"
                    } else {
                        "optional"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    )
}

pub(super) fn quick_command_summary_fields(value: &QuickCommand) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.command",
            None,
            Some(value.command.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.category",
            None,
            Some(value.category.clone()),
        ),
    ]
}
