// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn serial_profile_changed_fields(
    before: &SerialProfile,
    after: &SerialProfile,
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
        "plugin.cloud_sync.diff_fields.group",
        before.group.clone(),
        after.group.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port_path",
        Some(before.port_path.clone()),
        Some(after.port_path.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.baud_rate",
        Some(before.baud_rate.to_string()),
        Some(after.baud_rate.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.data_bits",
        Some(before.data_bits.to_string()),
        Some(after.data_bits.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.stop_bits",
        Some(before.stop_bits.to_string()),
        Some(after.stop_bits.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.parity",
        Some(format!("{:?}", before.parity)),
        Some(format!("{:?}", after.parity)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.flow_control",
        Some(format!("{:?}", before.flow_control)),
        Some(format!("{:?}", after.flow_control)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.connect_on_open",
        Some(before.connect_on_open.to_string()),
        Some(after.connect_on_open.to_string()),
    );
    fields
}

pub(super) fn serial_profile_merge_fields(
    base: &SerialProfile,
    local: &SerialProfile,
    remote: &SerialProfile,
    effective: &SerialProfile,
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
        "plugin.cloud_sync.diff_fields.group",
        base.group.clone(),
        local.group.clone(),
        remote.group.clone(),
        effective.group.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port_path",
        Some(base.port_path.clone()),
        Some(local.port_path.clone()),
        Some(remote.port_path.clone()),
        Some(effective.port_path.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.baud_rate",
        Some(base.baud_rate.to_string()),
        Some(local.baud_rate.to_string()),
        Some(remote.baud_rate.to_string()),
        Some(effective.baud_rate.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.data_bits",
        Some(base.data_bits.to_string()),
        Some(local.data_bits.to_string()),
        Some(remote.data_bits.to_string()),
        Some(effective.data_bits.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.stop_bits",
        Some(base.stop_bits.to_string()),
        Some(local.stop_bits.to_string()),
        Some(remote.stop_bits.to_string()),
        Some(effective.stop_bits.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.parity",
        Some(format!("{:?}", base.parity)),
        Some(format!("{:?}", local.parity)),
        Some(format!("{:?}", remote.parity)),
        Some(format!("{:?}", effective.parity)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.flow_control",
        Some(format!("{:?}", base.flow_control)),
        Some(format!("{:?}", local.flow_control)),
        Some(format!("{:?}", remote.flow_control)),
        Some(format!("{:?}", effective.flow_control)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.connect_on_open",
        Some(base.connect_on_open.to_string()),
        Some(local.connect_on_open.to_string()),
        Some(remote.connect_on_open.to_string()),
        Some(effective.connect_on_open.to_string()),
        conflict_strategy,
    );
    fields
}

pub(super) fn serial_profile_summary_fields(value: &SerialProfile) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.port_path",
            None,
            Some(value.port_path.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.baud_rate",
            None,
            Some(value.baud_rate.to_string()),
        ),
    ]
}
