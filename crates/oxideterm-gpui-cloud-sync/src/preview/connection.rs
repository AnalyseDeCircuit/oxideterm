// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn app_settings_changed_fields(
    before: &std::collections::HashMap<String, String>,
    after: &std::collections::HashMap<String, String>,
) -> Vec<CloudSyncFieldDiffField> {
    before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|field_key| {
            let before = before
                .get(&field_key)
                .map(|value| format!("{field_key}: {value}"));
            let after = after
                .get(&field_key)
                .map(|value| format!("{field_key}: {value}"));
            (before != after)
                .then(|| field("plugin.cloud_sync.diff_fields.setting_field", before, after))
        })
        .collect()
}

pub(super) fn app_settings_summary_fields(
    values: &std::collections::HashMap<String, String>,
) -> Vec<CloudSyncFieldDiffField> {
    values
        .iter()
        .map(|(field_key, value)| {
            field(
                "plugin.cloud_sync.diff_fields.setting_field",
                None,
                Some(format!("{field_key}: {value}")),
            )
        })
        .collect()
}

pub(super) fn connection_changed_fields(
    before: &ConnectionInfo,
    after: &ConnectionInfo,
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
        "plugin.cloud_sync.diff_fields.host",
        Some(before.host.clone()),
        Some(after.host.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port",
        Some(before.port.to_string()),
        Some(after.port.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.username",
        Some(before.username.clone()),
        Some(after.username.clone()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.auth_type",
        Some(format!("{:?}", before.auth_type)),
        Some(format!("{:?}", after.auth_type)),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.key_path",
        before.key_path.clone(),
        after.key_path.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.cert_path",
        before.cert_path.clone(),
        after.cert_path.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.managed_key",
        before.managed_key_id.clone(),
        after.managed_key_id.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.gssapi_authentication",
        Some(before.gssapi_authentication.to_string()),
        Some(after.gssapi_authentication.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.gssapi_server_identity",
        before.gssapi_server_identity.clone(),
        after.gssapi_server_identity.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.gssapi_delegate_credentials",
        Some(before.gssapi_delegate_credentials.to_string()),
        Some(after.gssapi_delegate_credentials.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.proxy_chain",
        Some(before.proxy_chain.len().to_string()),
        Some(after.proxy_chain.len().to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.agent_forwarding",
        Some(before.agent_forwarding.to_string()),
        Some(after.agent_forwarding.to_string()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.post_connect_command",
        before
            .post_connect_command
            .as_ref()
            .map(|_| redacted_changed_value()),
        after
            .post_connect_command
            .as_ref()
            .map(|_| redacted_changed_value()),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.color",
        before.color.clone(),
        after.color.clone(),
    );
    push_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tags",
        Some(before.tags.join(", ")),
        Some(after.tags.join(", ")),
    );
    fields
}

pub(super) fn connection_merge_fields(
    base: &ConnectionInfo,
    local: &ConnectionInfo,
    remote: &ConnectionInfo,
    effective: &ConnectionInfo,
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
        "plugin.cloud_sync.diff_fields.host",
        Some(base.host.clone()),
        Some(local.host.clone()),
        Some(remote.host.clone()),
        Some(effective.host.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.port",
        Some(base.port.to_string()),
        Some(local.port.to_string()),
        Some(remote.port.to_string()),
        Some(effective.port.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.username",
        Some(base.username.clone()),
        Some(local.username.clone()),
        Some(remote.username.clone()),
        Some(effective.username.clone()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.auth_type",
        Some(format!("{:?}", base.auth_type)),
        Some(format!("{:?}", local.auth_type)),
        Some(format!("{:?}", remote.auth_type)),
        Some(format!("{:?}", effective.auth_type)),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.key_path",
        base.key_path.clone(),
        local.key_path.clone(),
        remote.key_path.clone(),
        effective.key_path.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.cert_path",
        base.cert_path.clone(),
        local.cert_path.clone(),
        remote.cert_path.clone(),
        effective.cert_path.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.managed_key",
        base.managed_key_id.clone(),
        local.managed_key_id.clone(),
        remote.managed_key_id.clone(),
        effective.managed_key_id.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.gssapi_authentication",
        Some(base.gssapi_authentication.to_string()),
        Some(local.gssapi_authentication.to_string()),
        Some(remote.gssapi_authentication.to_string()),
        Some(effective.gssapi_authentication.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.gssapi_server_identity",
        base.gssapi_server_identity.clone(),
        local.gssapi_server_identity.clone(),
        remote.gssapi_server_identity.clone(),
        effective.gssapi_server_identity.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.gssapi_delegate_credentials",
        Some(base.gssapi_delegate_credentials.to_string()),
        Some(local.gssapi_delegate_credentials.to_string()),
        Some(remote.gssapi_delegate_credentials.to_string()),
        Some(effective.gssapi_delegate_credentials.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.proxy_chain",
        Some(base.proxy_chain.len().to_string()),
        Some(local.proxy_chain.len().to_string()),
        Some(remote.proxy_chain.len().to_string()),
        Some(effective.proxy_chain.len().to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.agent_forwarding",
        Some(base.agent_forwarding.to_string()),
        Some(local.agent_forwarding.to_string()),
        Some(remote.agent_forwarding.to_string()),
        Some(effective.agent_forwarding.to_string()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.post_connect_command",
        redacted_presence(base.post_connect_command.as_ref()),
        redacted_presence(local.post_connect_command.as_ref()),
        redacted_presence(remote.post_connect_command.as_ref()),
        redacted_presence(effective.post_connect_command.as_ref()),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.color",
        base.color.clone(),
        local.color.clone(),
        remote.color.clone(),
        effective.color.clone(),
        conflict_strategy,
    );
    push_merge_changed(
        &mut fields,
        "plugin.cloud_sync.diff_fields.tags",
        Some(base.tags.join(", ")),
        Some(local.tags.join(", ")),
        Some(remote.tags.join(", ")),
        Some(effective.tags.join(", ")),
        conflict_strategy,
    );
    fields
}

pub(super) fn connection_summary_fields(value: &ConnectionInfo) -> Vec<CloudSyncFieldDiffField> {
    vec![
        field(
            "plugin.cloud_sync.diff_fields.host",
            None,
            Some(value.host.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.port",
            None,
            Some(value.port.to_string()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.username",
            None,
            Some(value.username.clone()),
        ),
        field(
            "plugin.cloud_sync.diff_fields.auth_type",
            None,
            Some(format!("{:?}", value.auth_type)),
        ),
    ]
}

pub(super) fn connection_options_changed_fields(
    before: &ConnectionOptions,
    after: &ConnectionOptions,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    for (label_key, before, after) in ssh_algorithm_field_values(before, after) {
        push_changed(&mut fields, label_key, before, after);
    }
    fields
}

pub(super) fn connection_options_merge_fields(
    base: &ConnectionOptions,
    local: &ConnectionOptions,
    remote: &ConnectionOptions,
    effective: &ConnectionOptions,
    conflict_strategy: &ConflictStrategy,
) -> Vec<CloudSyncFieldDiffField> {
    let mut fields = Vec::new();
    for (label_key, base, local, remote, effective) in [
        (
            "plugin.cloud_sync.diff_fields.ssh_kex_algorithms",
            algorithm_list_value(&base.ssh_algorithms.kex),
            algorithm_list_value(&local.ssh_algorithms.kex),
            algorithm_list_value(&remote.ssh_algorithms.kex),
            algorithm_list_value(&effective.ssh_algorithms.kex),
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_host_key_algorithms",
            algorithm_list_value(&base.ssh_algorithms.host_key),
            algorithm_list_value(&local.ssh_algorithms.host_key),
            algorithm_list_value(&remote.ssh_algorithms.host_key),
            algorithm_list_value(&effective.ssh_algorithms.host_key),
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_cipher_algorithms",
            algorithm_list_value(&base.ssh_algorithms.cipher),
            algorithm_list_value(&local.ssh_algorithms.cipher),
            algorithm_list_value(&remote.ssh_algorithms.cipher),
            algorithm_list_value(&effective.ssh_algorithms.cipher),
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_mac_algorithms",
            algorithm_list_value(&base.ssh_algorithms.mac),
            algorithm_list_value(&local.ssh_algorithms.mac),
            algorithm_list_value(&remote.ssh_algorithms.mac),
            algorithm_list_value(&effective.ssh_algorithms.mac),
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_compression_algorithms",
            algorithm_list_value(&base.ssh_algorithms.compression),
            algorithm_list_value(&local.ssh_algorithms.compression),
            algorithm_list_value(&remote.ssh_algorithms.compression),
            algorithm_list_value(&effective.ssh_algorithms.compression),
        ),
    ] {
        push_merge_changed(
            &mut fields,
            label_key,
            base,
            local,
            remote,
            effective,
            conflict_strategy,
        );
    }
    fields
}

pub(super) fn connection_options_summary_fields(
    value: &ConnectionOptions,
) -> Vec<CloudSyncFieldDiffField> {
    [
        (
            "plugin.cloud_sync.diff_fields.ssh_kex_algorithms",
            &value.ssh_algorithms.kex,
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_host_key_algorithms",
            &value.ssh_algorithms.host_key,
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_cipher_algorithms",
            &value.ssh_algorithms.cipher,
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_mac_algorithms",
            &value.ssh_algorithms.mac,
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_compression_algorithms",
            &value.ssh_algorithms.compression,
        ),
    ]
    .into_iter()
    .filter_map(|(label_key, algorithms)| {
        algorithm_list_value(algorithms).map(|value| field(label_key, None, Some(value)))
    })
    .collect()
}

fn ssh_algorithm_field_values(
    before: &ConnectionOptions,
    after: &ConnectionOptions,
) -> [(&'static str, Option<String>, Option<String>); 5] {
    [
        (
            "plugin.cloud_sync.diff_fields.ssh_kex_algorithms",
            algorithm_list_value(&before.ssh_algorithms.kex),
            algorithm_list_value(&after.ssh_algorithms.kex),
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_host_key_algorithms",
            algorithm_list_value(&before.ssh_algorithms.host_key),
            algorithm_list_value(&after.ssh_algorithms.host_key),
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_cipher_algorithms",
            algorithm_list_value(&before.ssh_algorithms.cipher),
            algorithm_list_value(&after.ssh_algorithms.cipher),
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_mac_algorithms",
            algorithm_list_value(&before.ssh_algorithms.mac),
            algorithm_list_value(&after.ssh_algorithms.mac),
        ),
        (
            "plugin.cloud_sync.diff_fields.ssh_compression_algorithms",
            algorithm_list_value(&before.ssh_algorithms.compression),
            algorithm_list_value(&after.ssh_algorithms.compression),
        ),
    ]
}

fn algorithm_list_value(algorithms: &[String]) -> Option<String> {
    (!algorithms.is_empty()).then(|| algorithms.join(", "))
}
