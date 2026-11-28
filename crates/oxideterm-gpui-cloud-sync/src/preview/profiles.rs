// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn ble_profile_summary_fields(value: &BleProfile) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.device_name",
            None,
            value.device_name.clone(),
        ),
        field(
            "plugin.cloud_sync.diff_fields.service_uuid",
            None,
            Some(oxideterm_ble::service_display_name_from_str(
                &value.service_uuid,
            )),
        ),
    ]
}

pub(super) fn ble_profile_changed_fields(
    before: &BleProfile,
    after: &BleProfile,
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
        "plugin.cloud_sync.diff_fields.device_mac",
        before.device_mac.clone(),
        after.device_mac.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.device_name",
        before.device_name.clone(),
        after.device_name.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.service_uuid",
        Some(before.service_uuid.clone()),
        Some(after.service_uuid.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.mtu",
        before.mtu.map(|mtu| mtu.to_string()),
        after.mtu.map(|mtu| mtu.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.write_without_response",
        Some(before.write_without_response.to_string()),
        Some(after.write_without_response.to_string()),
    );
    fields
}

pub(super) fn ble_profile_merge_fields(
    base: &BleProfile,
    local: &BleProfile,
    remote: &BleProfile,
    effective: &BleProfile,
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
        "plugin.cloud_sync.diff_fields.device_mac",
        base.device_mac.clone(),
        local.device_mac.clone(),
        remote.device_mac.clone(),
        effective.device_mac.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.device_name",
        base.device_name.clone(),
        local.device_name.clone(),
        remote.device_name.clone(),
        effective.device_name.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.service_uuid",
        Some(base.service_uuid.clone()),
        Some(local.service_uuid.clone()),
        Some(remote.service_uuid.clone()),
        Some(effective.service_uuid.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.mtu",
        base.mtu.map(|mtu| mtu.to_string()),
        local.mtu.map(|mtu| mtu.to_string()),
        remote.mtu.map(|mtu| mtu.to_string()),
        effective.mtu.map(|mtu| mtu.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.write_without_response",
        Some(base.write_without_response.to_string()),
        Some(local.write_without_response.to_string()),
        Some(remote.write_without_response.to_string()),
        Some(effective.write_without_response.to_string()),
        conflict_strategy,
    );
    fields
}

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

/// Renders the engine target as the user-facing launch surface: a Docker
/// engine identifies itself by its socket, kube by profile namespace.
fn container_engine_label(engine: &oxideterm_docker::EngineTarget) -> String {
    match engine {
        oxideterm_docker::EngineTarget::DockerSocket { socket_path } => socket_path.clone(),
        oxideterm_docker::EngineTarget::Kube { context, namespace } => match context {
            Some(context) => format!("kube:{context}:{namespace}"),
            None => format!("kube:{namespace}"),
        },
    }
}

fn container_launch_label(launch: &oxideterm_docker::ContainerLaunchMode) -> String {
    match launch {
        oxideterm_docker::ContainerLaunchMode::Shell { shell } => match shell {
            Some(shell) => format!("shell:{shell}"),
            None => "shell".to_string(),
        },
        oxideterm_docker::ContainerLaunchMode::Command { command } => {
            format!("command:{command}")
        }
        oxideterm_docker::ContainerLaunchMode::Attach => "attach".to_string(),
    }
}

pub(super) fn container_profile_summary_fields(
    value: &ContainerProfile,
) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.container_name",
            None,
            Some(value.container_name.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.engine",
            None,
            Some(container_engine_label(&value.engine)),
        ),
        field(
            "plugin.cloud_sync.diff_fields.launch_mode",
            None,
            Some(container_launch_label(&value.launch)),
        ),
    ]
}

pub(super) fn container_profile_changed_fields(
    before: &ContainerProfile,
    after: &ContainerProfile,
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
        "plugin.cloud_sync.diff_fields.engine",
        Some(container_engine_label(&before.engine)),
        Some(container_engine_label(&after.engine)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.container_name",
        Some(before.container_name.clone()),
        Some(after.container_name.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.launch_mode",
        Some(container_launch_label(&before.launch)),
        Some(container_launch_label(&after.launch)),
    );
    fields
}

pub(super) fn container_profile_merge_fields(
    base: &ContainerProfile,
    local: &ContainerProfile,
    remote: &ContainerProfile,
    effective: &ContainerProfile,
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
        "plugin.cloud_sync.diff_fields.engine",
        Some(container_engine_label(&base.engine)),
        Some(container_engine_label(&local.engine)),
        Some(container_engine_label(&remote.engine)),
        Some(container_engine_label(&effective.engine)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.container_name",
        Some(base.container_name.clone()),
        Some(local.container_name.clone()),
        Some(remote.container_name.clone()),
        Some(effective.container_name.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.launch_mode",
        Some(container_launch_label(&base.launch)),
        Some(container_launch_label(&local.launch)),
        Some(container_launch_label(&remote.launch)),
        Some(container_launch_label(&effective.launch)),
        conflict_strategy,
    );
    fields
}
