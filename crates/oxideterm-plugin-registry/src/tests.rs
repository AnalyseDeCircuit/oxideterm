// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Registry tests covering package install, validation, and runtime contributions.

use super::*;

use std::io::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::{ZipWriter, write::SimpleFileOptions};

fn minimal_manifest() -> NativePluginManifest {
    NativePluginManifest {
        id: "com.example.demo".to_string(),
        name: "Demo".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        author: None,
        main: None,
        engines: None,
        manifest_version: None,
        format: None,
        assets: None,
        styles: None,
        shared_dependencies: None,
        repository: None,
        checksum: None,
        contributes: None,
        locales: None,
        runtime: None,
        permissions: NativePluginPermissions::default(),
    }
}

fn plugin_package(entries: &[(&str, String)]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default();
    for (path, content) in entries {
        zip.start_file(path, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn manifest_json(id: &str, version: &str) -> String {
    serde_json::json!({
        "id": id,
        "name": "Packaged Demo",
        "version": version,
        "contributes": {
            "settings": [{
                "id": "enabled",
                "type": "boolean",
                "default": true,
                "title": "Enabled"
            }]
        }
    })
    .to_string()
}

#[test]
fn legacy_tauri_manifest_is_visible_but_not_executable() {
    let mut manifest = minimal_manifest();
    manifest.main = Some("main.js".to_string());

    let plan = native_runtime_plan_for_manifest(&manifest).unwrap();
    assert_eq!(
        plan,
        NativePluginRuntimePlan::UnsupportedLegacyJs {
            entry: "main.js".to_string()
        }
    );
}

#[test]
fn native_wasm_runtime_uses_explicit_runtime_block() {
    let mut manifest = minimal_manifest();
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Wasm,
        entry: "plugin.wasm".to_string(),
    });

    let plan = native_runtime_plan_for_manifest(&manifest).unwrap();
    assert_eq!(
        plan,
        NativePluginRuntimePlan::Wasm {
            entry: "plugin.wasm".to_string()
        }
    );
}

#[test]
fn plugin_paths_cannot_escape_install_directory() {
    assert!(validate_plugin_relative_path("panel/native.json").is_ok());
    assert!(validate_plugin_relative_path("../secret").is_err());
    assert!(validate_plugin_relative_path("/tmp/plugin.wasm").is_err());
    assert!(validate_native_plugin_package_url("https://example.invalid/plugin.zip").is_ok());
    assert!(validate_native_plugin_package_url("file:///tmp/plugin.zip").is_err());
}

#[test]
fn manifest_permissions_default_to_an_empty_sensitive_capability_request() {
    let manifest: NativePluginManifest = serde_json::from_value(serde_json::json!({
        "id": "com.example.demo",
        "name": "Demo",
        "version": "1.0.0"
    }))
    .unwrap();

    // Empty permission declarations do not remove the host's safe default data plane.
    assert!(manifest.permissions.capabilities.is_empty());
}

#[test]
fn manifest_permissions_use_camel_case_and_round_trip() {
    let manifest: NativePluginManifest = serde_json::from_value(serde_json::json!({
        "id": "com.example.demo",
        "name": "Demo",
        "version": "1.0.0",
        "permissions": {
            "capabilities": ["terminal.content.read", "terminal.input.send"]
        }
    }))
    .unwrap();

    let value = serde_json::to_value(manifest).unwrap();
    assert_eq!(
        value.pointer("/permissions/capabilities"),
        Some(&serde_json::json!([
            "terminal.content.read",
            "terminal.input.send"
        ]))
    );
}

#[test]
fn host_monitor_manifest_defaults_are_bounded_and_structured() {
    let manifest: NativePluginManifest = serde_json::from_value(serde_json::json!({
        "id": "com.example.demo",
        "name": "Demo",
        "version": "1.0.0",
        "contributes": {
            "hostMonitors": [{
                "id": "workers",
                "title": "Workers",
                "commands": { "linux": "printf '[{\"pid\":1}]'" }
            }]
        }
    }))
    .unwrap();

    validate_native_plugin_manifest(&manifest).unwrap();
    let monitor = &manifest
        .contributes
        .as_ref()
        .unwrap()
        .host_monitors
        .as_ref()
        .unwrap()[0];
    assert_eq!(monitor.timeout_seconds, 10);
    assert_eq!(monitor.max_output_bytes, 256 * 1024);
    assert_eq!(monitor.output.max_rows, 1_000);
    assert_eq!(
        monitor.output.format,
        NativePluginHostMonitorOutputFormat::Json
    );
}

#[test]
fn host_monitor_manifest_rejects_unsafe_or_ambiguous_declarations() {
    let monitor = NativePluginHostMonitorDef {
        id: "workers".to_string(),
        title: "Workers".to_string(),
        description: None,
        commands: HashMap::from([("unknown".to_string(), "status".to_string())]),
        output: NativePluginHostMonitorOutputDef::default(),
        timeout_seconds: 10,
        max_output_bytes: 256 * 1024,
    };
    let mut contributes = NativePluginContributes {
        host_monitors: Some(vec![monitor.clone()]),
        ..NativePluginContributes::default()
    };
    assert!(validate_native_plugin_contributions(&contributes).is_err());

    let mut valid_monitor = monitor;
    valid_monitor.commands = HashMap::from([("linux".to_string(), "status".to_string())]);
    valid_monitor.output.format = NativePluginHostMonitorOutputFormat::Tsv;
    contributes.host_monitors = Some(vec![valid_monitor.clone()]);
    assert!(validate_native_plugin_contributions(&contributes).is_err());

    valid_monitor.output.columns = vec!["pid".to_string()];
    contributes.host_monitors = Some(vec![valid_monitor.clone(), valid_monitor]);
    assert!(validate_native_plugin_contributions(&contributes).is_err());
}

#[test]
fn activity_bar_manifest_rejects_duplicate_ids_and_invalid_positions() {
    let item = NativePluginActivityBarItemDef {
        id: "refresh".to_string(),
        title: "Refresh".to_string(),
        icon: "refresh-cw".to_string(),
        command: "dashboard.refresh".to_string(),
        position: "top".to_string(),
    };
    let duplicate_items = NativePluginContributes {
        activity_bar_items: Some(vec![item.clone(), item.clone()]),
        ..NativePluginContributes::default()
    };
    assert!(validate_native_plugin_contributions(&duplicate_items).is_err());

    let mut invalid_item = item;
    invalid_item.position = "middle".to_string();
    let invalid_position = NativePluginContributes {
        activity_bar_items: Some(vec![invalid_item]),
        ..NativePluginContributes::default()
    };
    assert!(validate_native_plugin_contributions(&invalid_position).is_err());
}

#[test]
fn repository_host_tools_dashboard_example_is_a_valid_native_plugin() {
    let plugin_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/host-tools-dashboard");
    let manifest_bytes = fs::read(plugin_dir.join(PLUGIN_MANIFEST_FILENAME)).unwrap();
    let manifest: NativePluginManifest = serde_json::from_slice(&manifest_bytes).unwrap();

    validate_native_plugin_manifest(&manifest).unwrap();
    let runtime_plan = native_runtime_plan_for_manifest(&manifest).unwrap();
    validate_runtime_entry_exists(&plugin_dir, &runtime_plan).unwrap();
    assert_eq!(manifest.id, "com.oxideterm.examples.host-tools-dashboard");
}

#[test]
fn permission_capabilities_normalize_order_and_whitespace() {
    let capabilities = vec![
        " terminal.input.send ".to_string(),
        "terminal.content.read".to_string(),
    ];

    assert_eq!(
        normalize_native_plugin_capabilities(&capabilities).unwrap(),
        vec![
            "terminal.content.read".to_string(),
            "terminal.input.send".to_string()
        ]
    );
}

#[test]
fn permission_capabilities_reject_empty_wildcard_and_duplicate_values() {
    assert!(normalize_native_plugin_capabilities(&[" ".to_string()]).is_err());
    assert!(normalize_native_plugin_capabilities(&["terminal.*".to_string()]).is_err());
    assert!(
        normalize_native_plugin_capabilities(&[
            "terminal.input.send".to_string(),
            " terminal.input.send ".to_string()
        ])
        .is_err()
    );
}

#[test]
fn capability_fingerprint_is_independent_of_declaration_order() {
    let left = vec!["terminal.input.send".to_string(), "file.read".to_string()];
    let right = vec!["file.read".to_string(), "terminal.input.send".to_string()];

    assert_eq!(
        native_plugin_capabilities_fingerprint(&left).unwrap(),
        native_plugin_capabilities_fingerprint(&right).unwrap()
    );
}

#[test]
fn capability_approval_allows_version_updates_and_narrower_requests() {
    let mut manifest = minimal_manifest();
    manifest.permissions.capabilities = vec![
        "terminal.input.send".to_string(),
        "terminal.content.read".to_string(),
    ];
    let config = NativePluginConfigEntry {
        approved_capabilities: vec![
            "terminal.content.read".to_string(),
            "terminal.input.send".to_string(),
        ],
        approved_for_version: Some("1.0.0".to_string()),
        approved_runtime_kind: Some("wasm".to_string()),
        ..NativePluginConfigEntry::default()
    };

    assert!(native_plugin_capability_approval_matches(
        &manifest, "wasm", &config
    ));
    assert!(!native_plugin_capability_approval_matches(
        &manifest, "process", &config
    ));

    manifest.version = "1.1.0".to_string();
    assert!(native_plugin_capability_approval_matches(
        &manifest, "wasm", &config
    ));

    manifest.permissions.capabilities = vec!["terminal.content.read".to_string()];
    assert!(native_plugin_capability_approval_matches(
        &manifest, "wasm", &config
    ));

    manifest
        .permissions
        .capabilities
        .push("file.content.read".to_string());
    assert!(!native_plugin_capability_approval_matches(
        &manifest, "wasm", &config
    ));
}

#[test]
fn manifest_validation_rejects_unsafe_permission_declarations() {
    let mut manifest = minimal_manifest();
    manifest.permissions.capabilities = vec!["terminal.*".to_string()];

    assert!(validate_native_plugin_manifest(&manifest).is_err());
}

#[test]
fn registry_index_parses_capabilities_summary() {
    let registry: NativePluginRegistryIndex = serde_json::from_value(serde_json::json!({
        "version": 1,
        "plugins": [{
            "id": "com.example.demo",
            "name": "Demo",
            "version": "1.2.0",
            "description": "demo plugin",
            "downloadUrl": "https://example.invalid/demo.zip",
            "checksum": "sha256:abc",
            "capabilitiesSummary": ["terminal read", "status item"]
        }]
    }))
    .unwrap();

    assert_eq!(
        registry.plugins[0].capabilities_summary.as_deref(),
        Some(&["terminal read".to_string(), "status item".to_string()][..])
    );
}

#[test]
fn plugin_package_install_supports_flat_nested_conflict_and_updates() {
    let temp_dir = unique_temp_dir("plugin-package-install");
    let settings_path = temp_dir.join("settings.json");
    let flat_package = plugin_package(&[
        ("plugin.json", manifest_json("com.example.demo", "1.0.0")),
        ("README.md", "demo".to_string()),
    ]);
    let result = NativePluginRegistry::install_plugin_package_from_bytes(
        &settings_path,
        &flat_package,
        None,
        false,
    )
    .unwrap();
    assert_eq!(result.manifest.id, "com.example.demo");
    assert!(!result.replaced_existing);
    assert_eq!(result.checksum, native_plugin_sha256_hex(&flat_package));

    let conflict = NativePluginRegistry::install_plugin_package_from_bytes(
        &settings_path,
        &flat_package,
        None,
        false,
    )
    .unwrap_err();
    assert!(conflict.contains("PLUGIN_ID_CONFLICT:com.example.demo"));

    let nested_package = plugin_package(&[
        (
            "oxideterm-demo-main/plugin.json",
            manifest_json("com.example.demo", "1.1.0"),
        ),
        ("oxideterm-demo-main/bin/plugin", "#!/bin/sh".to_string()),
    ]);
    let replaced = NativePluginRegistry::install_plugin_package_from_bytes(
        &settings_path,
        &nested_package,
        Some(&format!(
            "sha256:{}",
            native_plugin_sha256_hex(&nested_package)
        )),
        true,
    )
    .unwrap();
    assert!(replaced.replaced_existing);

    let registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(registry.plugins()[0].manifest.version, "1.1.0");
    let updates = NativePluginRegistry::check_plugin_updates(
        NativePluginRegistryIndex {
            version: 1,
            plugins: vec![
                NativePluginRegistryEntry {
                    id: "com.example.demo".to_string(),
                    name: "Demo".to_string(),
                    description: None,
                    author: None,
                    version: "1.2.0".to_string(),
                    min_oxideterm_version: None,
                    download_url: "https://example.invalid/demo.zip".to_string(),
                    checksum: None,
                    size: None,
                    tags: None,
                    capabilities_summary: Some(vec![
                        "terminal read".to_string(),
                        "status item".to_string(),
                    ]),
                    homepage: None,
                    updated_at: None,
                },
                NativePluginRegistryEntry {
                    id: "com.example.other".to_string(),
                    name: "Other".to_string(),
                    description: None,
                    author: None,
                    version: "9.0.0".to_string(),
                    min_oxideterm_version: None,
                    download_url: "https://example.invalid/other.zip".to_string(),
                    checksum: None,
                    size: None,
                    tags: None,
                    capabilities_summary: None,
                    homepage: None,
                    updated_at: None,
                },
            ],
        },
        &[NativePluginInstalledInfo {
            id: "com.example.demo".to_string(),
            version: "1.1.0".to_string(),
        }],
    );
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].version, "1.2.0");
    assert_eq!(
        updates[0].capabilities_summary.as_deref(),
        Some(&["terminal read".to_string(), "status item".to_string()][..])
    );
    let expected_package = plugin_package(&[(
        "plugin.json",
        manifest_json("com.example.expected", "1.0.0"),
    )]);
    let expected_manifest = NativePluginRegistry::install_plugin_package(
        &settings_path,
        "com.example.expected",
        None,
        &expected_package,
    )
    .unwrap();
    assert_eq!(expected_manifest.id, "com.example.expected");
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn plugin_package_rejects_zip_slip_and_checksum_mismatch_without_replacing_existing() {
    let temp_dir = unique_temp_dir("plugin-package-safety");
    let settings_path = temp_dir.join("settings.json");
    let installed = plugin_package(&[("plugin.json", manifest_json("com.example.demo", "1.0.0"))]);
    NativePluginRegistry::install_plugin_package_from_bytes(
        &settings_path,
        &installed,
        None,
        false,
    )
    .unwrap();

    let bad_path_package = plugin_package(&[("../plugin.json", manifest_json("com.bad", "1.0.0"))]);
    let bad_path_error = NativePluginRegistry::install_plugin_package_from_bytes(
        &settings_path,
        &bad_path_package,
        None,
        true,
    )
    .unwrap_err();
    assert!(bad_path_error.contains("escapes target dir"));

    let replacement =
        plugin_package(&[("plugin.json", manifest_json("com.example.demo", "2.0.0"))]);
    let checksum_error = NativePluginRegistry::install_plugin_package_from_bytes(
        &settings_path,
        &replacement,
        Some("sha256:0000"),
        true,
    )
    .unwrap_err();
    assert!(checksum_error.contains("Checksum mismatch"));
    let registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(registry.plugins()[0].manifest.version, "1.0.0");
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn uninstall_plugin_removes_directory_contributions_and_optional_state() {
    let temp_dir = unique_temp_dir("plugin-uninstall");
    let settings_path = temp_dir.join("settings.json");
    let package = plugin_package(&[("plugin.json", manifest_json("com.example.demo", "1.0.0"))]);
    NativePluginRegistry::install_plugin_package_from_bytes(&settings_path, &package, None, false)
        .unwrap();

    let mut registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(registry.contributions().settings.len(), 1);
    registry
        .set_plugin_storage_value("com.example.demo", "recent", serde_json::json!("yes"))
        .unwrap();
    assert!(
        registry
            .plugin_storage_value("com.example.demo", "recent")
            .is_some()
    );
    registry.uninstall_plugin("com.example.demo", true).unwrap();
    assert!(registry.plugins().is_empty());
    assert_eq!(registry.contributions().total_count(), 0);
    assert!(
        !native_plugins_dir(&settings_path)
            .join("com.example.demo")
            .exists()
    );
    assert_eq!(
        registry.plugin_storage_value("com.example.demo", "recent"),
        None
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn plugin_config_round_trips_disabled_and_error_state() {
    let temp_dir = unique_temp_dir("plugin-config-round-trip");
    fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join(PLUGIN_CONFIG_FILENAME);
    let mut config = NativePluginGlobalConfig::default();
    config.plugins.insert(
        "com.example.demo".to_string(),
        NativePluginConfigEntry {
            enabled: false,
            last_error: Some("disabled by test".to_string()),
            runtime_kind: Some("wasm".to_string()),
            approved_capabilities: vec!["terminal.content.read".to_string()],
            approved_for_version: Some("1.0.0".to_string()),
            approved_runtime_kind: Some("wasm".to_string()),
            ..NativePluginConfigEntry::default()
        },
    );

    save_native_plugin_config(&config_path, &config).unwrap();
    let loaded = load_native_plugin_config(&config_path);
    let entry = loaded.plugins.get("com.example.demo").unwrap();
    assert!(!entry.enabled);
    assert_eq!(entry.last_error.as_deref(), Some("disabled by test"));
    assert_eq!(entry.runtime_kind.as_deref(), Some("wasm"));
    assert_eq!(
        entry.approved_capabilities,
        vec!["terminal.content.read".to_string()]
    );
    assert_eq!(entry.approved_for_version.as_deref(), Some("1.0.0"));
    assert_eq!(entry.approved_runtime_kind.as_deref(), Some("wasm"));
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn legacy_plugin_config_defaults_permission_approval_metadata() {
    let config: NativePluginGlobalConfig = serde_json::from_value(serde_json::json!({
        "version": 1,
        "plugins": {
            "com.example.demo": {
                "enabled": true,
                "runtimeKind": "wasm"
            }
        }
    }))
    .unwrap();
    let entry = &config.plugins["com.example.demo"];

    // Existing installations remain readable and simply have no sensitive approval snapshot.
    assert!(entry.approved_capabilities.is_empty());
    assert!(entry.approved_for_version.is_none());
    assert!(entry.approved_runtime_kind.is_none());
}

#[test]
fn corrupt_plugin_config_is_quarantined_and_recreated() {
    let temp_dir = unique_temp_dir("plugin-config-corrupt-recovery");
    fs::create_dir_all(&temp_dir).unwrap();
    let settings_path = temp_dir.join("settings.json");
    let config_path = native_plugin_config_path(&settings_path);
    fs::write(&config_path, b"{ not valid json").unwrap();

    let registry = NativePluginRegistry::discover(&settings_path);

    assert_eq!(registry.configured_plugin_count(), 0);
    assert!(config_path.exists());
    let backup_count = fs::read_dir(&temp_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().starts_with(&format!(
                "{PLUGIN_CONFIG_FILENAME}.{PLUGIN_CONFIG_CORRUPT_MARKER}-"
            ))
        })
        .count();
    assert_eq!(backup_count, 1);
    let loaded = load_native_plugin_config(&config_path);
    assert_eq!(loaded.version, PLUGIN_CONFIG_SCHEMA_VERSION);
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn runtime_state_respects_config_before_runtime_kind() {
    let disabled = NativePluginConfigEntry {
        enabled: false,
        ..NativePluginConfigEntry::default()
    };
    assert_eq!(
        native_plugin_state_for(
            &NativePluginRuntimePlan::Wasm {
                entry: "plugin.wasm".to_string()
            },
            &disabled,
        ),
        NativePluginState::Disabled
    );

    let auto_disabled = NativePluginConfigEntry {
        auto_disabled: true,
        ..NativePluginConfigEntry::default()
    };
    assert_eq!(
        native_plugin_state_for(&NativePluginRuntimePlan::ManifestOnly, &auto_disabled),
        NativePluginState::AutoDisabled
    );
}

#[test]
fn executable_native_runtime_requires_existing_entry() {
    let temp_dir = unique_temp_dir("plugin-runtime-entry");
    fs::create_dir_all(&temp_dir).unwrap();
    let plan = NativePluginRuntimePlan::Process {
        entry: "bin/plugin".to_string(),
    };
    assert!(validate_runtime_entry_exists(&temp_dir, &plan).is_err());

    let bin_dir = temp_dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(bin_dir.join("plugin"), b"#!/bin/sh\n").unwrap();
    assert!(validate_runtime_entry_exists(&temp_dir, &plan).is_ok());
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn invalid_manifest_reports_diagnostic_without_crashing_discovery() {
    let temp_dir = unique_temp_dir("plugin-invalid-manifest");
    let plugins_dir = temp_dir.join(PLUGINS_DIR_NAME);
    let broken_dir = plugins_dir.join("broken");
    fs::create_dir_all(&broken_dir).unwrap();
    fs::write(broken_dir.join(PLUGIN_MANIFEST_FILENAME), b"{").unwrap();

    let (plugins, diagnostics) =
        discover_native_plugins_in_dir(&plugins_dir, &NativePluginGlobalConfig::default());
    assert!(plugins.is_empty());
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("Invalid plugin.json"));
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn missing_executable_runtime_entry_reports_diagnostic() {
    let temp_dir = unique_temp_dir("plugin-missing-runtime-entry");
    let plugins_dir = temp_dir.join(PLUGINS_DIR_NAME);
    let plugin_dir = plugins_dir.join("native-process");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = minimal_manifest();
    manifest.id = "com.example.process".to_string();
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Process,
        entry: "bin/plugin".to_string(),
    });
    write_manifest(&plugin_dir, &manifest);

    let (plugins, diagnostics) =
        discover_native_plugins_in_dir(&plugins_dir, &NativePluginGlobalConfig::default());
    assert!(plugins.is_empty());
    assert_eq!(
        diagnostics[0].plugin_id.as_deref(),
        Some("com.example.process")
    );
    assert!(diagnostics[0].message.contains("does not exist"));
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn discovery_classifies_native_wasm_and_process_runtime_states() {
    let temp_dir = unique_temp_dir("plugin-runtime-state");
    let plugins_dir = temp_dir.join(PLUGINS_DIR_NAME);
    let wasm_dir = plugins_dir.join("wasm");
    let process_dir = plugins_dir.join("process");
    fs::create_dir_all(&wasm_dir).unwrap();
    fs::create_dir_all(&process_dir).unwrap();

    let mut wasm_manifest = minimal_manifest();
    wasm_manifest.id = "com.example.wasm".to_string();
    wasm_manifest.name = "Wasm".to_string();
    wasm_manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Wasm,
        entry: "plugin.wasm".to_string(),
    });
    fs::write(wasm_dir.join("plugin.wasm"), b"\0asm").unwrap();
    write_manifest(&wasm_dir, &wasm_manifest);

    let mut process_manifest = minimal_manifest();
    process_manifest.id = "com.example.process".to_string();
    process_manifest.name = "Process".to_string();
    process_manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Process,
        entry: "bin/plugin".to_string(),
    });
    fs::create_dir_all(process_dir.join("bin")).unwrap();
    fs::write(process_dir.join("bin/plugin"), b"#!/bin/sh\n").unwrap();
    write_manifest(&process_dir, &process_manifest);

    let (plugins, diagnostics) =
        discover_native_plugins_in_dir(&plugins_dir, &NativePluginGlobalConfig::default());
    assert!(diagnostics.is_empty());
    assert_eq!(plugins.len(), 2);
    // Process runtimes remain disabled until the user trusts the unsandboxed boundary.
    assert_eq!(plugins[0].state, NativePluginState::Disabled);
    assert_eq!(plugins[1].state, NativePluginState::ReadyWasm);
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn process_activation_plans_and_runtime_state_transitions_are_host_owned() {
    let temp_dir = unique_temp_dir("plugin-process-activation-plan");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("process");
    fs::create_dir_all(plugin_dir.join("bin")).unwrap();
    fs::write(plugin_dir.join("bin/plugin"), b"#!/bin/sh\n").unwrap();

    let mut manifest = minimal_manifest();
    manifest.id = "com.example.process".to_string();
    manifest.name = "Process".to_string();
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Process,
        entry: "bin/plugin".to_string(),
    });
    write_manifest(&plugin_dir, &manifest);

    let mut registry = NativePluginRegistry::discover(&settings_path);
    assert!(registry.process_activation_plans().is_empty());
    registry
        .set_plugin_enabled("com.example.process", true)
        .unwrap();
    let plans = registry.process_activation_plans();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].plugin_id, "com.example.process");
    assert_eq!(plans[0].entry, "bin/plugin");

    registry
        .mark_runtime_loading("com.example.process")
        .unwrap();
    assert_eq!(registry.plugins()[0].state, NativePluginState::Loading);
    registry.mark_runtime_active("com.example.process").unwrap();
    assert_eq!(registry.plugins()[0].state, NativePluginState::Active);
    registry
        .mark_runtime_error("com.example.process", "activate failed".to_string())
        .unwrap();
    assert_eq!(registry.plugins()[0].state, NativePluginState::Error);

    let config = load_native_plugin_config(registry.config_path());
    assert_eq!(
        config.plugins["com.example.process"].last_error.as_deref(),
        Some("activate failed")
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn wasm_activation_plans_are_host_owned() {
    let temp_dir = unique_temp_dir("plugin-wasm-activation-plan");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("wasm");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();

    let mut manifest = minimal_manifest();
    manifest.id = "com.example.wasm".to_string();
    manifest.name = "Wasm".to_string();
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Wasm,
        entry: "plugin.wasm".to_string(),
    });
    write_manifest(&plugin_dir, &manifest);

    let registry = NativePluginRegistry::discover(&settings_path);
    let plans = registry.wasm_activation_plans();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].plugin_id, "com.example.wasm");
    assert_eq!(plans[0].entry, "plugin.wasm");
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn set_plugin_enabled_persists_config_and_refreshes_state() {
    let temp_dir = unique_temp_dir("plugin-toggle-enabled");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_manifest(&plugin_dir, &minimal_manifest());

    let mut registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(
        registry.plugins()[0].state,
        NativePluginState::ReadyManifestOnly
    );

    registry
        .set_plugin_enabled("com.example.demo", false)
        .unwrap();
    assert_eq!(registry.plugins()[0].state, NativePluginState::Disabled);

    let config = load_native_plugin_config(registry.config_path());
    assert!(!config.plugins["com.example.demo"].enabled);
    assert_eq!(
        config.plugins["com.example.demo"].runtime_kind.as_deref(),
        Some("manifest-only")
    );

    registry
        .set_plugin_enabled("com.example.demo", true)
        .unwrap();
    assert_eq!(
        registry.plugins()[0].state,
        NativePluginState::ReadyManifestOnly
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn manifest_only_contributions_are_indexed_without_runtime_execution() {
    let temp_dir = unique_temp_dir("plugin-contributions");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = minimal_manifest();
    manifest.contributes = Some(sample_contributes());
    write_manifest(&plugin_dir, &manifest);

    let registry = NativePluginRegistry::discover(&settings_path);
    let contributions = registry.contributions();
    assert_eq!(contributions.tabs.len(), 1);
    assert_eq!(contributions.sidebar_panels.len(), 1);
    assert_eq!(contributions.settings.len(), 1);
    assert_eq!(contributions.ai_tools.len(), 1);
    assert_eq!(contributions.terminal_shortcuts.len(), 1);
    assert_eq!(contributions.terminal_transports.len(), 1);
    assert_eq!(contributions.connection_hooks.len(), 1);
    assert_eq!(contributions.api_commands.len(), 1);
    assert_eq!(contributions.host_monitors.len(), 1);
    assert_eq!(
        contributions
            .host_monitor("com.example.demo", "workers")
            .unwrap()
            .definition
            .title,
        "Workers"
    );
    assert!(
        contributions
            .host_monitor("com.example.other", "workers")
            .is_none()
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn disabling_plugin_removes_manifest_only_contributions() {
    let temp_dir = unique_temp_dir("plugin-contributions-disabled");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = minimal_manifest();
    manifest.contributes = Some(sample_contributes());
    write_manifest(&plugin_dir, &manifest);

    let mut registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(registry.contributions().total_count(), 9);
    registry
        .set_plugin_enabled("com.example.demo", false)
        .unwrap();
    assert_eq!(registry.contributions().total_count(), 0);
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn plugin_ai_tool_metadata_uses_namespaced_pending_runtime_definitions() {
    let temp_dir = unique_temp_dir("plugin-ai-tool-definitions");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = minimal_manifest();
    manifest.id = "com.example.demo-plugin".to_string();
    manifest.contributes = Some(sample_contributes());
    write_manifest(&plugin_dir, &manifest);

    let registry = NativePluginRegistry::discover(&settings_path);
    let definitions = registry.contributions().ai_tool_definitions();
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        definitions[0].name,
        "plugin::com_example_demo-plugin::demo_tool"
    );
    assert!(definitions[0].description.contains("[Plugin: Demo]"));
    assert_eq!(
        definitions[0].parameters,
        serde_json::json!({"type": "object"})
    );
    assert!(is_native_plugin_ai_tool_name(&definitions[0].name));
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn plugin_setting_values_resolve_defaults_validate_and_persist() {
    let temp_dir = unique_temp_dir("plugin-setting-values");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = minimal_manifest();
    manifest.contributes = Some(sample_contributes());
    write_manifest(&plugin_dir, &manifest);

    let mut registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(
        registry.plugin_setting_value("com.example.demo", "mode"),
        Some(Value::String("auto".to_string()))
    );
    assert!(
        registry
            .set_plugin_setting_value(
                "com.example.demo",
                "mode",
                Value::String("manual".to_string()),
            )
            .is_err()
    );
    registry
        .set_plugin_setting_value(
            "com.example.demo",
            "mode",
            Value::String("auto".to_string()),
        )
        .unwrap();

    let loaded = load_native_plugin_config(registry.config_path());
    assert_eq!(
        loaded.settings["com.example.demo"]["mode"],
        Value::String("auto".to_string())
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn plugin_storage_values_are_plugin_scoped_validated_and_persisted() {
    let temp_dir = unique_temp_dir("plugin-storage-values");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let first_plugin_dir = plugins_dir.join("demo-a");
    let second_plugin_dir = plugins_dir.join("demo-b");
    fs::create_dir_all(&first_plugin_dir).unwrap();
    fs::create_dir_all(&second_plugin_dir).unwrap();
    write_manifest(&first_plugin_dir, &minimal_manifest());
    let mut second_manifest = minimal_manifest();
    second_manifest.id = "com.example.other".to_string();
    write_manifest(&second_plugin_dir, &second_manifest);

    let mut registry = NativePluginRegistry::discover(&settings_path);
    registry
        .set_plugin_storage_value(
            "com.example.demo",
            "recent",
            serde_json::json!({"path": "/tmp/a"}),
        )
        .unwrap();
    registry
        .set_plugin_storage_value(
            "com.example.other",
            "recent",
            serde_json::json!({"path": "/tmp/b"}),
        )
        .unwrap();

    assert_eq!(
        registry.plugin_storage_value("com.example.demo", "recent"),
        Some(serde_json::json!({"path": "/tmp/a"}))
    );
    assert_eq!(
        registry.plugin_storage_value("com.example.other", "recent"),
        Some(serde_json::json!({"path": "/tmp/b"}))
    );

    let loaded = load_native_plugin_config(registry.config_path());
    assert_eq!(
        loaded.storage["com.example.demo"]["recent"],
        serde_json::json!({"path": "/tmp/a"})
    );
    assert!(
        registry
            .set_plugin_storage_value("com.example.demo", "", serde_json::json!({"invalid": true}),)
            .is_err()
    );
    let oversized_key = "x".repeat(PLUGIN_STORAGE_MAX_KEY_BYTES + 1);
    assert!(
        registry
            .set_plugin_storage_value("com.example.demo", &oversized_key, Value::Null)
            .is_err()
    );
    assert!(
        registry
            .set_plugin_storage_value(
                "com.example.demo",
                "too-large",
                Value::String("x".repeat(PLUGIN_STORAGE_MAX_PLUGIN_BYTES + 1)),
            )
            .is_err()
    );

    registry
        .remove_plugin_storage_value("com.example.demo", "recent")
        .unwrap();
    assert_eq!(
        registry.plugin_storage_value("com.example.demo", "recent"),
        None
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn runtime_registrations_feed_host_owned_contribution_store_and_cleanup() {
    let temp_dir = unique_temp_dir("plugin-runtime-registrations");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = minimal_manifest();
    manifest.contributes = Some(NativePluginContributes {
        terminal_hooks: Some(NativePluginTerminalHooksDef {
            input_interceptor: Some(true),
            output_processor: Some(true),
            shortcuts: Some(vec![NativePluginShortcutDef {
                key: "Ctrl+Shift+K".to_string(),
                command: "demo.focus".to_string(),
            }]),
        }),
        ..NativePluginContributes::default()
    });
    write_manifest(&plugin_dir, &manifest);

    let mut registry = NativePluginRegistry::discover(&settings_path);
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "cmd-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::Command,
            metadata: serde_json::json!({
                "id": "demo.run",
                "label": "Run Demo",
                "icon": "play",
                "shortcut": "cmd+shift+d",
                "section": "Demo",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "key-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::Keybinding,
            metadata: serde_json::json!({
                "keybinding": "Cmd+Shift+R",
                "command": "demo.run",
                "label": "Run Demo",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "status-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::StatusBar,
            metadata: serde_json::json!({
                "text": "Demo Ready",
                "alignment": "right",
                "priority": 10,
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "menu-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::ContextMenu,
            metadata: serde_json::json!({
                "target": "terminal",
                "items": [
                    { "label": "Run Demo", "icon": "play", "enabled": true }
                ],
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "theme-sub-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::EventSubscription,
            metadata: serde_json::json!({
                "namespace": "app",
                "method": "onThemeChange",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "custom-sub-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::EventSubscription,
            metadata: serde_json::json!({
                "namespace": "events",
                "method": "on",
                "name": "build.done",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "layout-sub-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::EventSubscription,
            metadata: serde_json::json!({
                "namespace": "ui",
                "method": "onLayoutChange",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "saved-forwards-sub-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::EventSubscription,
            metadata: serde_json::json!({
                "namespace": "forward",
                "method": "onSavedForwardsChange",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "transfer-progress-sub-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::EventSubscription,
            metadata: serde_json::json!({
                "namespace": "transfers",
                "method": "onProgress",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "profiler-metrics-sub-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::EventSubscription,
            metadata: serde_json::json!({
                "namespace": "profiler",
                "method": "onMetrics",
                "nodeId": "node-1",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "ide-active-sub-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::EventSubscription,
            metadata: serde_json::json!({
                "namespace": "ide",
                "method": "onActiveFileChange",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "ai-message-sub-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::EventSubscription,
            metadata: serde_json::json!({
                "namespace": "ai",
                "method": "onMessage",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "terminal-shortcut-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::TerminalShortcut,
            metadata: serde_json::json!({
                "command": "demo.focus",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "terminal-input-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::TerminalInputInterceptor,
            metadata: serde_json::json!({
                "command": "demo.input",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "terminal-output-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::TerminalOutputProcessor,
            metadata: serde_json::json!({
                "command": "demo.output",
            }),
        })
        .unwrap();

    let contributions = registry.contributions();
    assert_eq!(contributions.runtime_commands[0].command, "demo.run");
    assert_eq!(contributions.runtime_commands[0].label, "Run Demo");
    assert_eq!(
        contributions.runtime_keybindings[0].keybinding,
        "Cmd+Shift+R"
    );
    assert_eq!(
        contributions.runtime_keybindings[0].normalized_keybinding,
        "ctrl+r+shift"
    );
    assert_eq!(contributions.runtime_keybindings[0].command, "demo.run");
    assert_eq!(
        contributions.runtime_keybindings[1].keybinding,
        "Ctrl+Shift+K"
    );
    assert_eq!(
        contributions.runtime_keybindings[1].normalized_keybinding,
        "ctrl+k+shift"
    );
    assert_eq!(contributions.runtime_keybindings[1].command, "demo.focus");
    assert_eq!(
        contributions.runtime_terminal_input_interceptors[0].command,
        "demo.input"
    );
    assert_eq!(
        contributions.runtime_terminal_output_processors[0].command,
        "demo.output"
    );
    assert_eq!(
        contributions
            .runtime_keybinding_for_normalized_key("ctrl+r+shift")
            .map(|entry| entry.command.as_str()),
        Some("demo.run")
    );
    assert_eq!(contributions.runtime_status_items[0].alignment, "right");
    assert_eq!(contributions.runtime_context_menus[0].target, "terminal");
    assert_eq!(
        contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT)[0]
            .registration_id,
        "theme-sub-1"
    );
    assert_eq!(
        contributions.runtime_event_subscriptions_for("plugin.com.example.demo:build.done")[0]
            .registration_id,
        "custom-sub-1"
    );
    assert_eq!(
        contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT)[0]
            .registration_id,
        "layout-sub-1"
    );
    assert_eq!(
        contributions
            .runtime_event_subscriptions_for(NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT)[0]
            .registration_id,
        "saved-forwards-sub-1"
    );
    assert_eq!(
        contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT)[0]
            .registration_id,
        "transfer-progress-sub-1"
    );
    assert_eq!(
        contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_PROFILER_METRICS_EVENT)[0]
            .filter,
        Some(serde_json::json!({ "nodeId": "node-1" }))
    );
    assert_eq!(
        contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT)
            [0]
        .registration_id,
        "ide-active-sub-1"
    );
    assert_eq!(
        contributions.runtime_event_subscriptions_for(NATIVE_PLUGIN_AI_MESSAGE_EVENT)[0]
            .registration_id,
        "ai-message-sub-1"
    );
    assert_eq!(contributions.total_count(), 16);

    assert!(registry.dispose_runtime_registration("com.example.demo", "cmd-1"));
    assert!(registry.contributions().runtime_commands.is_empty());
    assert_eq!(
        registry.cleanup_runtime_plugin_contributions("com.example.demo"),
        14
    );
    assert_eq!(registry.contributions().total_count(), 1);
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn terminal_shortcut_registration_requires_manifest_declaration() {
    let temp_dir = unique_temp_dir("plugin-terminal-shortcut-gate");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_manifest(&plugin_dir, &minimal_manifest());

    let mut registry = NativePluginRegistry::discover(&settings_path);
    let error = registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "terminal-shortcut-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::TerminalShortcut,
            metadata: serde_json::json!({
                "command": "demo.focus",
            }),
        })
        .unwrap_err();

    assert!(error.contains("not declared in manifest contributes.terminalHooks.shortcuts"));
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn terminal_hook_registration_requires_manifest_declaration() {
    let temp_dir = unique_temp_dir("plugin-terminal-hook-gate");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    write_manifest(&plugin_dir, &minimal_manifest());

    let mut registry = NativePluginRegistry::discover(&settings_path);
    let input_error = registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "terminal-input-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::TerminalInputInterceptor,
            metadata: serde_json::json!({
                "command": "demo.input",
            }),
        })
        .unwrap_err();
    let output_error = registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "terminal-output-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::TerminalOutputProcessor,
            metadata: serde_json::json!({
                "command": "demo.output",
            }),
        })
        .unwrap_err();

    assert!(input_error.contains("inputInterceptor not declared"));
    assert!(output_error.contains("outputProcessor not declared"));
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn runtime_tab_and_sidebar_views_require_manifest_declarations_and_valid_schema() {
    let temp_dir = unique_temp_dir("plugin-declarative-ui");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = minimal_manifest();
    manifest.contributes = Some(NativePluginContributes {
        tabs: Some(vec![NativePluginTabDef {
            id: "deploy".to_string(),
            title: "Deploy".to_string(),
            icon: "rocket".to_string(),
        }]),
        sidebar_panels: Some(vec![NativePluginSidebarDef {
            id: "jobs".to_string(),
            title: "Jobs".to_string(),
            icon: "list".to_string(),
            position: "top".to_string(),
        }]),
        activity_bar_items: Some(vec![NativePluginActivityBarItemDef {
            id: "refresh".to_string(),
            title: "Refresh".to_string(),
            icon: "refresh-cw".to_string(),
            command: "dashboard.refresh".to_string(),
            position: "top".to_string(),
        }]),
        ..NativePluginContributes::default()
    });
    write_manifest(&plugin_dir, &manifest);

    let schema = serde_json::json!({
        "kind": "form",
        "sections": [{
            "id": "deploy",
            "title": "Deploy",
            "controls": [
                { "kind": "text", "id": "target", "label": "Target" },
                { "kind": "select", "id": "env", "label": "Environment", "options": [
                    { "label": "Prod", "value": "prod" }
                ] },
                { "kind": "button", "id": "run", "label": "Run" }
            ]
        }]
    });
    let mut registry = NativePluginRegistry::discover(&settings_path);
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "tab-view-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::Tab,
            metadata: serde_json::json!({
                "tabId": "deploy",
                "schema": schema,
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "activity-item-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::ActivityBarItem,
            metadata: serde_json::json!({
                "itemId": "refresh",
            }),
        })
        .unwrap();
    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "sidebar-view-1".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::SidebarPanel,
            metadata: serde_json::json!({
                "panelId": "jobs",
                "schema": {
                    "kind": "form",
                    "controls": [
                        { "kind": "emptyState", "label": "No jobs" }
                    ]
                },
            }),
        })
        .unwrap();

    let contributions = registry.contributions();
    assert_eq!(
        contributions
            .runtime_tab_view("com.example.demo", "deploy")
            .unwrap()
            .title,
        "Deploy"
    );
    assert_eq!(contributions.runtime_sidebar_panels()[0].panel_id, "jobs");
    let activity_item = &contributions.runtime_activity_bar_items()[0];
    assert_eq!(activity_item.item_id, "refresh");
    assert_eq!(activity_item.icon, "refresh-cw");
    assert_eq!(activity_item.command, "dashboard.refresh");

    registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "activity-item-replacement".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::ActivityBarItem,
            metadata: serde_json::json!({
                "itemId": "refresh",
                "command": "runtime.cannot.override",
            }),
        })
        .unwrap();
    let activity_items = registry.contributions().runtime_activity_bar_items();
    assert_eq!(activity_items.len(), 1);
    assert_eq!(
        activity_items[0].registration_id,
        "activity-item-replacement"
    );
    assert_eq!(activity_items[0].command, "dashboard.refresh");

    let undeclared_error = registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "tab-view-2".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::Tab,
            metadata: serde_json::json!({
                "tabId": "unknown",
                "schema": { "kind": "form", "controls": [{ "kind": "divider" }] },
            }),
        })
        .unwrap_err();
    assert!(undeclared_error.contains("not declared"));

    let undeclared_activity_error = registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "activity-item-2".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::ActivityBarItem,
            metadata: serde_json::json!({
                "itemId": "unknown",
            }),
        })
        .unwrap_err();
    assert!(undeclared_activity_error.contains("not declared"));

    let schema_error = registry
        .apply_runtime_registration(PluginRegistration {
            registration_id: "tab-view-3".to_string(),
            plugin_id: "com.example.demo".to_string(),
            kind: PluginRegistrationKind::Tab,
            metadata: serde_json::json!({
                "tabId": "deploy",
                "schema": { "kind": "form", "controls": [{ "kind": "reactComponent" }] },
            }),
        })
        .unwrap_err();
    assert!(schema_error.contains("unsupported value"));

    assert!(registry.dispose_runtime_registration("com.example.demo", "tab-view-1"));
    assert!(
        registry
            .contributions()
            .runtime_tab_view("com.example.demo", "deploy")
            .is_none()
    );
    assert_eq!(
        registry.cleanup_runtime_plugin_contributions("com.example.demo"),
        2
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn disabled_or_loading_declarative_buttons_are_not_actionable() {
    let active = NativePluginDeclarativeUiControl {
        kind: "button".to_string(),
        id: Some("run".to_string()),
        label: Some("Run".to_string()),
        description: None,
        placeholder: None,
        icon: None,
        variant: None,
        tone: None,
        size: None,
        gap: None,
        value: None,
        text: None,
        language: None,
        options: None,
        rows: None,
        columns: None,
        column_defs: None,
        children: Vec::new(),
        min: None,
        max: None,
        step: None,
        indeterminate: false,
        strong: false,
        disabled: false,
        loading: false,
    };
    let mut disabled = active.clone();
    disabled.disabled = true;
    let mut loading = active.clone();
    loading.loading = true;
    let mut icon_button = active.clone();
    icon_button.kind = "iconButton".to_string();
    icon_button.icon = Some("refresh-cw".to_string());

    assert!(native_plugin_declarative_control_is_actionable(&active));
    assert!(native_plugin_declarative_control_is_actionable(
        &icon_button
    ));
    assert!(!native_plugin_declarative_control_is_actionable(&disabled));
    assert!(!native_plugin_declarative_control_is_actionable(&loading));
}

#[test]
fn declarative_component_schema_accepts_shared_components_and_rejects_unsafe_shapes() {
    let schema = runtime_declarative_ui_schema(&serde_json::json!({
        "componentVersion": 1,
        "kind": "form",
        "controls": [{
            "kind": "card",
            "variant": "inspector",
            "children": [
                { "kind": "statusBadge", "label": "Ready", "tone": "success" },
                { "kind": "select", "id": "environment", "options": [
                    { "label": "Production", "value": "production" }
                ] },
                { "kind": "slider", "id": "parallelism", "min": 1, "max": 8, "step": 1 },
                { "kind": "iconButton", "id": "refresh", "icon": "refresh-cw", "label": "Refresh" }
            ]
        }]
    }))
    .unwrap();
    assert!(validate_native_plugin_declarative_ui_schema(&schema).is_ok());

    let unsupported_version = runtime_declarative_ui_schema(&serde_json::json!({
        "componentVersion": 2,
        "kind": "form",
        "controls": [{ "kind": "divider" }]
    }))
    .unwrap();
    assert!(
        validate_native_plugin_declarative_ui_schema(&unsupported_version)
            .unwrap_err()
            .contains("componentVersion")
    );

    let duplicate_ids = runtime_declarative_ui_schema(&serde_json::json!({
        "componentVersion": 1,
        "kind": "form",
        "controls": [{
            "kind": "stack",
            "children": [
                { "kind": "text", "id": "target" },
                { "kind": "select", "id": "target", "options": [
                    { "label": "One", "value": 1 }
                ] }
            ]
        }]
    }))
    .unwrap();
    assert!(
        validate_native_plugin_declarative_ui_schema(&duplicate_ids)
            .unwrap_err()
            .contains("Duplicate")
    );

    let invalid_range = runtime_declarative_ui_schema(&serde_json::json!({
        "componentVersion": 1,
        "kind": "form",
        "controls": [{ "kind": "slider", "id": "range", "min": 10, "max": 1 }]
    }))
    .unwrap();
    assert!(
        validate_native_plugin_declarative_ui_schema(&invalid_range)
            .unwrap_err()
            .contains("max")
    );

    let password_with_value = runtime_declarative_ui_schema(&serde_json::json!({
        "componentVersion": 1,
        "kind": "form",
        "controls": [{ "kind": "password", "id": "secret", "value": "plaintext" }]
    }))
    .unwrap();
    assert!(
        validate_native_plugin_declarative_ui_schema(&password_with_value)
            .unwrap_err()
            .contains("initial value")
    );
}

#[test]
fn runtime_registration_rejects_render_time_context_menu_predicate_shape() {
    let mut store = NativePluginContributionStore::default();
    let error = store
        .apply_runtime_registration(
            PluginRegistration {
                registration_id: "menu-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::ContextMenu,
                metadata: serde_json::json!({
                    "target": "terminal",
                    "items": [
                        { "label": "" }
                    ],
                }),
            },
            "Demo".to_string(),
        )
        .unwrap_err();

    assert!(error.contains("label"));
}

#[test]
fn runtime_event_subscription_rejects_invalid_custom_event_name() {
    let mut store = NativePluginContributionStore::default();
    let error = store
        .apply_runtime_registration(
            PluginRegistration {
                registration_id: "custom-sub-1".to_string(),
                plugin_id: "com.example.demo".to_string(),
                kind: PluginRegistrationKind::EventSubscription,
                metadata: serde_json::json!({
                    "namespace": "events",
                    "method": "on",
                    "name": "../escape",
                }),
            },
            "Demo".to_string(),
        )
        .unwrap_err();

    assert!(error.contains("Plugin event name"));
}

#[test]
fn malformed_contribution_definition_is_rejected_with_diagnostic() {
    let temp_dir = unique_temp_dir("plugin-bad-contribution");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = minimal_manifest();
    manifest.contributes = Some(NativePluginContributes {
        settings: Some(vec![NativePluginSettingDef {
            id: "mode".to_string(),
            setting_type: "select".to_string(),
            default: Value::String("auto".to_string()),
            title: "Mode".to_string(),
            description: None,
            options: None,
        }]),
        ..NativePluginContributes::default()
    });
    write_manifest(&plugin_dir, &manifest);

    let registry = NativePluginRegistry::discover(&settings_path);
    assert!(registry.plugins().is_empty());
    assert_eq!(registry.diagnostics().len(), 1);
    assert!(
        registry.diagnostics()[0]
            .message
            .contains("Select plugin settings require")
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn legacy_js_plugin_cannot_be_enabled_by_native_toggle() {
    let temp_dir = unique_temp_dir("plugin-legacy-enable");
    let settings_path = temp_dir.join("settings.json");
    let plugins_dir = native_plugins_dir(&settings_path);
    let plugin_dir = plugins_dir.join("legacy");
    fs::create_dir_all(&plugin_dir).unwrap();
    let mut manifest = minimal_manifest();
    manifest.main = Some("main.js".to_string());
    write_manifest(&plugin_dir, &manifest);

    let mut registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(
        registry.plugins()[0].state,
        NativePluginState::UnsupportedLegacyJs
    );
    registry
        .set_plugin_enabled("com.example.demo", false)
        .unwrap();
    assert_eq!(registry.plugins()[0].state, NativePluginState::Disabled);
    assert!(
        registry
            .set_plugin_enabled("com.example.demo", true)
            .is_err()
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn sensitive_wasm_waits_for_enable_approval_before_activation() {
    let temp_dir = unique_temp_dir("plugin-wasm-permission-review");
    let settings_path = temp_dir.join("settings.json");
    let plugin_dir = native_plugins_dir(&settings_path).join("wasm");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();
    let mut manifest = minimal_manifest();
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Wasm,
        entry: "plugin.wasm".to_string(),
    });
    manifest.permissions.capabilities = vec!["terminal.content.read".to_string()];
    write_manifest(&plugin_dir, &manifest);

    let mut registry = NativePluginRegistry::discover(&settings_path);
    let plugin = &registry.plugins()[0];
    assert_eq!(plugin.state, NativePluginState::Disabled);
    assert!(native_plugin_requires_permission_review(
        &plugin.manifest,
        &plugin.runtime_plan,
        &plugin.config
    ));
    assert!(registry.wasm_activation_plans().is_empty());

    registry
        .set_plugin_enabled("com.example.demo", true)
        .unwrap();
    assert_eq!(registry.plugins()[0].state, NativePluginState::ReadyWasm);
    assert_eq!(registry.wasm_activation_plans().len(), 1);
    let config = load_native_plugin_config(registry.config_path());
    let approval = &config.plugins["com.example.demo"];
    assert_eq!(
        approval.approved_capabilities,
        vec!["terminal.content.read".to_string()]
    );
    assert_eq!(approval.approved_for_version.as_deref(), Some("1.0.0"));
    assert_eq!(approval.approved_runtime_kind.as_deref(), Some("wasm"));
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn process_runtime_requires_implicit_trust_approval() {
    let temp_dir = unique_temp_dir("plugin-process-permission-review");
    let settings_path = temp_dir.join("settings.json");
    let plugin_dir = native_plugins_dir(&settings_path).join("process");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("plugin-process"), b"executable placeholder").unwrap();
    let mut manifest = minimal_manifest();
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Process,
        entry: "plugin-process".to_string(),
    });
    write_manifest(&plugin_dir, &manifest);

    let mut registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(registry.plugins()[0].state, NativePluginState::Disabled);
    assert!(registry.process_activation_plans().is_empty());

    registry
        .set_plugin_enabled("com.example.demo", true)
        .unwrap();
    assert_eq!(registry.plugins()[0].state, NativePluginState::ReadyProcess);
    assert_eq!(registry.process_activation_plans().len(), 1);
    let config = load_native_plugin_config(registry.config_path());
    assert_eq!(
        config.plugins["com.example.demo"].approved_capabilities,
        vec![NATIVE_PLUGIN_TRUSTED_PROCESS_CAPABILITY.to_string()]
    );
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn updates_only_require_review_for_expanded_permissions_or_runtime_changes() {
    let temp_dir = unique_temp_dir("plugin-permission-update");
    let settings_path = temp_dir.join("settings.json");
    let plugin_dir = native_plugins_dir(&settings_path).join("wasm");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();
    fs::write(plugin_dir.join("plugin-process"), b"executable placeholder").unwrap();
    let mut manifest = minimal_manifest();
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Wasm,
        entry: "plugin.wasm".to_string(),
    });
    manifest.permissions.capabilities = vec![
        "terminal.input.send".to_string(),
        "terminal.content.read".to_string(),
    ];
    write_manifest(&plugin_dir, &manifest);
    let mut registry = NativePluginRegistry::discover(&settings_path);
    registry
        .set_plugin_enabled("com.example.demo", true)
        .unwrap();

    // A version update and narrower request preserve the existing approval.
    manifest.version = "1.1.0".to_string();
    manifest.permissions.capabilities = vec!["terminal.content.read".to_string()];
    write_manifest(&plugin_dir, &manifest);
    let registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(registry.plugins()[0].state, NativePluginState::ReadyWasm);
    assert_eq!(registry.wasm_activation_plans().len(), 1);

    // A newly requested capability requires another explicit enable action.
    manifest
        .permissions
        .capabilities
        .push("file.content.read".to_string());
    write_manifest(&plugin_dir, &manifest);
    let registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(registry.plugins()[0].state, NativePluginState::Disabled);
    assert!(registry.wasm_activation_plans().is_empty());

    // Moving to an unsandboxed process also invalidates a WASM approval.
    manifest.permissions.capabilities = vec!["terminal.content.read".to_string()];
    manifest.runtime = Some(NativePluginRuntime {
        kind: NativePluginRuntimeKind::Process,
        entry: "plugin-process".to_string(),
    });
    write_manifest(&plugin_dir, &manifest);
    let registry = NativePluginRegistry::discover(&settings_path);
    assert_eq!(registry.plugins()[0].state, NativePluginState::Disabled);
    assert!(registry.process_activation_plans().is_empty());
    let _ = fs::remove_dir_all(temp_dir);
}

fn write_manifest(plugin_dir: &Path, manifest: &NativePluginManifest) {
    let manifest_json = serde_json::to_vec_pretty(manifest).unwrap();
    fs::write(plugin_dir.join(PLUGIN_MANIFEST_FILENAME), manifest_json).unwrap();
}

fn sample_contributes() -> NativePluginContributes {
    NativePluginContributes {
        tabs: Some(vec![NativePluginTabDef {
            id: "demo-tab".to_string(),
            title: "Demo".to_string(),
            icon: "Puzzle".to_string(),
        }]),
        sidebar_panels: Some(vec![NativePluginSidebarDef {
            id: "demo-sidebar".to_string(),
            title: "Demo".to_string(),
            icon: "Puzzle".to_string(),
            position: "bottom".to_string(),
        }]),
        activity_bar_items: None,
        settings: Some(vec![NativePluginSettingDef {
            id: "mode".to_string(),
            setting_type: "select".to_string(),
            default: Value::String("auto".to_string()),
            title: "Mode".to_string(),
            description: Some("Mode description".to_string()),
            options: Some(vec![NativePluginSettingOption {
                label: "Auto".to_string(),
                value: Value::String("auto".to_string()),
            }]),
        }]),
        terminal_hooks: Some(NativePluginTerminalHooksDef {
            input_interceptor: Some(true),
            output_processor: None,
            shortcuts: Some(vec![NativePluginShortcutDef {
                key: "Ctrl+Shift+D".to_string(),
                command: "demo.run".to_string(),
            }]),
        }),
        terminal_transports: Some(vec!["telnet".to_string()]),
        connection_hooks: Some(vec!["onConnect".to_string()]),
        ai_tools: Some(vec![NativePluginAiToolDef {
            name: "demo_tool".to_string(),
            description: "Demo tool".to_string(),
            parameters: Some(serde_json::json!({"type": "object"})),
            capabilities: Some(vec!["state.list".to_string()]),
            risk: Some("read".to_string()),
            target_kinds: Some(vec!["app-tab".to_string()]),
            result_schema: None,
        }]),
        api_commands: Some(vec!["demo_command".to_string()]),
        host_monitors: Some(vec![NativePluginHostMonitorDef {
            id: "workers".to_string(),
            title: "Workers".to_string(),
            description: Some("Worker process metadata".to_string()),
            commands: HashMap::from([("linux".to_string(), "printf '[{\"pid\":1}]'".to_string())]),
            output: NativePluginHostMonitorOutputDef::default(),
            timeout_seconds: 10,
            max_output_bytes: 256 * 1024,
        }]),
    }
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("oxideterm-{label}-{nanos}"))
}
