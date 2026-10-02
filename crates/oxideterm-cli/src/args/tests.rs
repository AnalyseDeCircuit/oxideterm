// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::Parser;

use super::*;

#[test]
fn parses_temporary_ssh_launch() {
    let cli = Cli::parse_from(["oxideterm", "ssh", "alice@example.com", "-p", "2222"]);
    let Command::Ssh(args) = cli.command else {
        panic!("expected ssh command");
    };
    assert_eq!(args.target, "alice@example.com");
    assert_eq!(args.port, Some(2222));
    assert!(!args.password_stdin);
}

#[test]
fn parses_connection_uri_launch_without_exposing_it_in_debug_output() {
    let cli = Cli::parse_from(["oxideterm", "open", "ssh://alice:uri-password@example.com"]);
    let rendered = format!("{cli:?}");

    assert!(matches!(cli.command, Command::Open(_)));
    assert!(!rendered.contains("uri-password"));

    let ssh_cli = Cli::parse_from([
        "oxideterm",
        "ssh",
        "ssh://alice:second-password@example.com",
    ]);
    assert!(!format!("{ssh_cli:?}").contains("second-password"));
}

#[test]
fn parses_json_only_commands() {
    let cloud_sync_status = Cli::parse_from(["oxideterm", "cloud-sync", "status", "--json"]);
    assert!(matches!(
        cloud_sync_status.command,
        Command::CloudSync(command)
            if matches!(&command.action, CloudSyncAction::Status(args) if args.json)
    ));

    let settings_sections = Cli::parse_from(["oxideterm", "settings", "sections", "--json"]);
    assert!(matches!(
        settings_sections.command,
        Command::Settings(command)
            if matches!(&command.action, SettingsAction::Sections(args) if args.json)
    ));

    let diagnostics = Cli::parse_from(["oxideterm", "diagnose", "--json"]);
    assert!(matches!(diagnostics.command, Command::Diagnose(args) if args.json));

    let backup_preview = Cli::parse_from(["oxideterm", "backup", "preview", "--json"]);
    assert!(matches!(
        backup_preview.command,
        Command::Backup(command)
            if matches!(&command.action, BackupAction::Preview(args) if args.json)
    ));
}

#[test]
fn parses_cloud_sync_diff() {
    let cli = Cli::parse_from([
        "oxideterm",
        "cloud-sync",
        "diff",
        "--dirty-only",
        "--category",
        "app-settings",
        "--format",
        "table",
        "--json",
    ]);
    let Command::CloudSync(CloudSyncCommand {
        action: CloudSyncAction::Diff(args),
    }) = cli.command
    else {
        panic!("expected cloud-sync diff command");
    };
    assert!(args.dirty_only);
    assert_eq!(args.category, Some(CloudSyncDiffCategory::AppSettings));
    assert_eq!(args.format, Some(CloudSyncDiffFormat::Table));
    assert!(args.json);
}

#[test]
fn parses_cloud_sync_state_get() {
    let cli = Cli::parse_from([
        "oxideterm",
        "cloud-sync",
        "state",
        "get",
        "settings.namespace",
        "--json",
    ]);
    let Command::CloudSync(CloudSyncCommand {
        action:
            CloudSyncAction::State(CloudSyncStateCommand {
                action: CloudSyncStateAction::Get(args),
            }),
    }) = cli.command
    else {
        panic!("expected cloud-sync state get command");
    };
    assert_eq!(args.key, "settings.namespace");
    assert!(args.json);
}

#[test]
fn parses_settings_export_sections() {
    let cli = Cli::parse_from([
        "oxideterm",
        "settings",
        "export",
        "--section",
        "general",
        "--include-local-terminal-env-vars",
        "--json",
    ]);
    let Command::Settings(SettingsCommand {
        action: SettingsAction::Export(args),
    }) = cli.command
    else {
        panic!("expected settings export command");
    };
    assert_eq!(args.sections, ["general"]);
    assert!(args.include_local_terminal_env_vars);
    assert!(args.json);
}

#[test]
fn parses_connections_export_format() {
    let cli = Cli::parse_from([
        "oxideterm",
        "connections",
        "export",
        "--format",
        "raw-safe",
        "--json",
    ]);
    let Command::Connections(ConnectionsCommand {
        action: ConnectionsAction::Export(args),
    }) = cli.command
    else {
        panic!("expected connections export command");
    };
    assert_eq!(args.format, ConnectionsExportFormat::RawSafe);
    assert!(args.json);
}

#[test]
fn parses_connections_create_and_edit_specs() {
    let create = Cli::parse_from([
        "oxideterm",
        "connections",
        "create",
        "--spec",
        "connection.json",
        "--dry-run",
        "--json",
    ]);
    let Command::Connections(ConnectionsCommand {
        action: ConnectionsAction::Create(args),
    }) = create.command
    else {
        panic!("expected connections create command");
    };
    assert_eq!(args.spec_path.as_deref(), Some("connection.json"));
    assert!(args.write.dry_run);
    assert!(args.write.json);

    let edit = Cli::parse_from([
        "oxideterm",
        "connections",
        "edit",
        "prod",
        "--spec",
        "patch.json",
        "--yes",
        "--json",
    ]);
    let Command::Connections(ConnectionsCommand {
        action: ConnectionsAction::Edit(args),
    }) = edit.command
    else {
        panic!("expected connections edit command");
    };
    assert_eq!(args.query, "prod");
    assert_eq!(args.spec_path.as_deref(), Some("patch.json"));
    assert!(args.write.yes);
    assert!(args.write.json);
}

#[test]
fn parses_connections_group_rename() {
    let cli = Cli::parse_from([
        "oxideterm",
        "connections",
        "group",
        "rename",
        "old",
        "new",
        "--dry-run",
        "--json",
    ]);
    let Command::Connections(ConnectionsCommand {
        action:
            ConnectionsAction::Group(ConnectionsGroupCommand {
                action: ConnectionsGroupAction::Rename(args),
            }),
    }) = cli.command
    else {
        panic!("expected connections group rename command");
    };
    assert_eq!(args.old_name, "old");
    assert_eq!(args.new_name, "new");
    assert!(args.write.dry_run);
    assert!(args.write.json);
}

#[test]
fn parses_connections_apply_snapshot_strategy() {
    let cli = Cli::parse_from([
        "oxideterm",
        "connections",
        "apply-snapshot",
        "connections.json",
        "--strategy",
        "merge",
        "--dry-run",
        "--json",
    ]);
    let Command::Connections(ConnectionsCommand {
        action: ConnectionsAction::ApplySnapshot(args),
    }) = cli.command
    else {
        panic!("expected connections apply-snapshot command");
    };
    assert_eq!(args.path, "connections.json");
    assert_eq!(args.strategy, ConnectionsApplyStrategy::Merge);
    assert!(args.write.dry_run);
    assert!(args.write.json);
}

#[test]
fn parses_oxide_preview_import() {
    let cli = Cli::parse_from([
        "oxideterm",
        "oxide",
        "preview-import",
        "bundle.oxide",
        "--strategy",
        "replace",
        "--password-stdin",
        "--json",
    ]);
    let Command::Oxide(OxideCommand {
        action: OxideAction::PreviewImport(args),
    }) = cli.command
    else {
        panic!("expected oxide preview-import command");
    };
    assert_eq!(args.path, "bundle.oxide");
    assert_eq!(args.strategy, OxideImportStrategy::Replace);
    assert!(args.password.password_stdin);
    assert!(args.json);
}

#[test]
fn parses_oxide_import_defaults_to_dry_run_until_yes() {
    let cli = Cli::parse_from([
        "oxideterm",
        "oxide",
        "import",
        "bundle.oxide",
        "--strategy",
        "merge",
        "--password-env",
        "OXIDE_PASSWORD",
        "--section",
        "appearance",
        "--no-quick-commands",
        "--plugin",
        "com.example.plugin",
        "--json",
    ]);
    let Command::Oxide(OxideCommand {
        action: OxideAction::Import(args),
    }) = cli.command
    else {
        panic!("expected oxide import command");
    };
    assert_eq!(args.strategy, OxideImportStrategy::Merge);
    assert_eq!(
        args.password.password_env.as_deref(),
        Some("OXIDE_PASSWORD")
    );
    assert!(!args.write.yes);
    assert_eq!(args.sections, vec!["appearance"]);
    assert!(args.no_quick_commands);
    assert_eq!(args.plugin_ids, vec!["com.example.plugin"]);
    assert!(args.write.json);
}

#[test]
fn parses_oxide_export() {
    let cli = Cli::parse_from([
        "oxideterm",
        "oxide",
        "export",
        "bundle.oxide",
        "--connection",
        "prod",
        "--password-stdin",
        "--overwrite",
        "--json",
    ]);
    let Command::Oxide(OxideCommand {
        action: OxideAction::Export(args),
    }) = cli.command
    else {
        panic!("expected oxide export command");
    };
    assert_eq!(args.connection_queries, ["prod"]);
    assert!(args.password.password_stdin);
    assert!(args.overwrite);
    assert!(args.json);
}

#[test]
fn parses_settings_unset_with_confirmation() {
    let cli = Cli::parse_from([
        "oxideterm",
        "settings",
        "unset",
        "ai.customSystemPrompt",
        "--yes",
        "--no-backup",
        "--json",
    ]);
    let Command::Settings(SettingsCommand {
        action: SettingsAction::Unset(args),
    }) = cli.command
    else {
        panic!("expected settings unset command");
    };
    assert_eq!(args.key, "ai.customSystemPrompt");
    assert!(args.write.yes);
    assert!(args.write.no_backup);
    assert!(args.write.json);
}

#[test]
fn parses_settings_import_sections() {
    let cli = Cli::parse_from([
        "oxideterm",
        "settings",
        "import",
        "snapshot.json",
        "--section",
        "general",
        "--dry-run",
        "--json",
    ]);
    let Command::Settings(SettingsCommand {
        action: SettingsAction::Import(args),
    }) = cli.command
    else {
        panic!("expected settings import command");
    };
    assert_eq!(args.path, "snapshot.json");
    assert_eq!(args.sections, ["general"]);
    assert!(args.write.dry_run);
    assert!(args.write.json);
}

#[test]
fn parses_strict_validation_commands() {
    let connection_validation =
        Cli::parse_from(["oxideterm", "connections", "validate", "--strict", "--json"]);
    assert!(matches!(
        connection_validation.command,
        Command::Connections(command)
            if matches!(&command.action, ConnectionsAction::Validate(args) if args.strict && args.json)
    ));

    let doctor = Cli::parse_from(["oxideterm", "doctor", "--strict", "--json"]);
    assert!(matches!(doctor.command, Command::Doctor(args) if args.strict && args.json));

    let settings_validation =
        Cli::parse_from(["oxideterm", "settings", "validate", "--strict", "--json"]);
    assert!(matches!(
        settings_validation.command,
        Command::Settings(command)
            if matches!(&command.action, SettingsAction::Validate(args) if args.strict && args.json)
    ));
}

#[test]
fn parses_backup_inspect() {
    // Full and section-scoped inspection share one backup parsing contract.
    for cli in [
        Cli::parse_from([
            "oxideterm",
            "backup",
            "inspect",
            "backup.json",
            "--full",
            "--json",
        ]),
        Cli::parse_from([
            "oxideterm",
            "backup",
            "inspect",
            "backup.json",
            "--section",
            "cloud-sync",
            "--json",
        ]),
    ] {
        let Command::Backup(BackupCommand {
            action: BackupAction::Inspect(args),
        }) = cli.command
        else {
            panic!("expected backup inspect command");
        };
        assert_eq!(args.query, "backup.json");
        assert!(args.json);
        assert_ne!(args.full, args.section.is_some());
        if !args.full {
            assert_eq!(args.section, Some(BackupInspectSection::CloudSync));
        }
    }
}

#[test]
fn parses_backup_verify() {
    let cli = Cli::parse_from(["oxideterm", "backup", "verify", "backup.json", "--json"]);
    let Command::Backup(BackupCommand {
        action: BackupAction::Verify(args),
    }) = cli.command
    else {
        panic!("expected backup verify command");
    };
    assert_eq!(args.query, "backup.json");
    assert!(args.json);
}

#[test]
fn parses_backup_create_output() {
    let cli = Cli::parse_from([
        "oxideterm",
        "backup",
        "create",
        "--output",
        "/tmp/backup.json",
        "--json",
    ]);
    let Command::Backup(BackupCommand {
        action: BackupAction::Create(args),
    }) = cli.command
    else {
        panic!("expected backup create command");
    };
    assert_eq!(args.output.as_deref(), Some("/tmp/backup.json"));
    assert!(args.json);
}

#[test]
fn parses_backup_restore_defaults_to_dry_run_until_yes() {
    let cli = Cli::parse_from([
        "oxideterm",
        "backup",
        "restore",
        "backup.json",
        "--section",
        "settings",
        "--json",
    ]);
    let Command::Backup(BackupCommand {
        action: BackupAction::Restore(args),
    }) = cli.command
    else {
        panic!("expected backup restore command");
    };
    assert_eq!(args.query, "backup.json");
    assert_eq!(args.section, Some(BackupInspectSection::Settings));
    assert!(!args.write.yes);
    assert!(args.write.json);
}

#[test]
fn parses_cloud_sync_history_failed_only() {
    let cli = Cli::parse_from([
        "oxideterm",
        "cloud-sync",
        "history",
        "--failed-only",
        "--json",
    ]);
    let Command::CloudSync(CloudSyncCommand {
        action: CloudSyncAction::History(args),
    }) = cli.command
    else {
        panic!("expected cloud-sync history command");
    };
    assert!(args.failed_only);
    assert!(args.json);
}

#[test]
fn parses_cloud_sync_backend_settings_and_write_flags() {
    let cases = [
        (
            Cli::parse_from([
                "oxideterm",
                "cloud-sync",
                "configure",
                "--backend",
                "s3",
                "--s3-bucket",
                "oxide-sync",
                "--s3-region",
                "us-east-1",
                "--default-conflict-strategy",
                "merge",
                "--dry-run",
                "--json",
            ]),
            CloudSyncBackendArg::S3,
            Some("oxide-sync"),
            Some("us-east-1"),
            None,
            Some(CloudSyncConflictStrategy::Merge),
        ),
        (
            Cli::parse_from([
                "oxideterm",
                "cloud-sync",
                "configure",
                "--backend",
                "google-drive",
                "--google-oauth-client-id",
                "google-client-id",
                "--dry-run",
                "--json",
            ]),
            CloudSyncBackendArg::GoogleDrive,
            None,
            None,
            Some("google-client-id"),
            None,
        ),
    ];
    for (cli, backend, bucket, region, client, strategy) in cases {
        let Command::CloudSync(CloudSyncCommand {
            action: CloudSyncAction::Configure(args),
        }) = cli.command
        else {
            panic!("expected cloud-sync configure command");
        };
        assert_eq!(args.backend, Some(backend));
        assert_eq!(args.s3_bucket.as_deref(), bucket);
        assert_eq!(args.s3_region.as_deref(), region);
        assert_eq!(args.google_oauth_client_id.as_deref(), client);
        assert_eq!(args.default_conflict_strategy, strategy);
        assert!(args.write.dry_run);
        assert!(args.write.json);
    }
}

#[test]
fn parses_cloud_sync_apply_remote() {
    let cli = Cli::parse_from([
        "oxideterm",
        "cloud-sync",
        "apply",
        "--from",
        "remote",
        "--strategy",
        "replace",
        "--yes",
        "--json",
    ]);
    let Command::CloudSync(CloudSyncCommand {
        action: CloudSyncAction::Apply(args),
    }) = cli.command
    else {
        panic!("expected cloud-sync apply command");
    };
    assert_eq!(args.from, CloudSyncApplySource::Remote);
    assert_eq!(args.strategy, Some(CloudSyncConflictStrategy::Replace));
    assert!(args.write.yes);
    assert!(args.write.json);
}

#[test]
fn parses_cloud_sync_secrets_set_env() {
    let cli = Cli::parse_from([
        "oxideterm",
        "cloud-sync",
        "secrets",
        "set",
        "sync-password",
        "--env",
        "OXIDE_SYNC_PASSWORD",
        "--json",
    ]);
    let Command::CloudSync(CloudSyncCommand {
        action:
            CloudSyncAction::Secrets(CloudSyncSecretsCommand {
                action: CloudSyncSecretsAction::Set(args),
            }),
    }) = cli.command
    else {
        panic!("expected cloud-sync secrets set command");
    };
    assert_eq!(args.key, "sync-password");
    assert_eq!(args.env.as_deref(), Some("OXIDE_SYNC_PASSWORD"));
    assert!(args.json);
}

#[test]
fn parses_cloud_sync_secrets_status() {
    let cli = Cli::parse_from(["oxideterm", "cloud-sync", "secrets", "status", "--json"]);
    let Command::CloudSync(CloudSyncCommand {
        action:
            CloudSyncAction::Secrets(CloudSyncSecretsCommand {
                action: CloudSyncSecretsAction::Status(args),
            }),
    }) = cli.command
    else {
        panic!("expected cloud-sync secrets status command");
    };
    assert!(args.json);
}

#[test]
fn parses_connections_direct_create() {
    let cli = Cli::parse_from([
        "oxideterm",
        "connections",
        "create",
        "--name",
        "prod",
        "--host",
        "prod.example.com",
        "--user",
        "deploy",
        "--port",
        "2222",
        "--auth",
        "agent",
        "--dry-run",
    ]);
    let Command::Connections(ConnectionsCommand {
        action: ConnectionsAction::Create(args),
    }) = cli.command
    else {
        panic!("expected connections create command");
    };
    assert_eq!(args.direct.name.as_deref(), Some("prod"));
    assert_eq!(args.direct.host.as_deref(), Some("prod.example.com"));
    assert_eq!(args.direct.username.as_deref(), Some("deploy"));
    assert_eq!(args.direct.port, Some(2222));
    assert_eq!(args.direct.auth, Some(ConnectionAuthArg::Agent));
}

#[test]
fn parses_forwards_quick_commands_plugins_and_secrets() {
    let forward = Cli::parse_from([
        "oxideterm",
        "forwards",
        "create",
        "--type",
        "local",
        "--bind-port",
        "8080",
        "--target-host",
        "localhost",
        "--target-port",
        "80",
    ]);
    assert!(matches!(
        forward.command,
        Command::Forwards(ForwardsCommand {
            action: ForwardsAction::Create(_)
        })
    ));

    let quick = Cli::parse_from([
        "oxideterm",
        "quick-commands",
        "create",
        "--name",
        "Uptime",
        "--command",
        "uptime",
    ]);
    assert!(matches!(
        quick.command,
        Command::QuickCommands(QuickCommandsCommand {
            action: QuickCommandsAction::Create(_)
        })
    ));

    let plugin = Cli::parse_from([
        "oxideterm",
        "plugins",
        "settings",
        "set",
        "oxide-plugin-demo-setting-token",
        "--value-json",
        "\"configured\"",
    ]);
    assert!(matches!(
        plugin.command,
        Command::Plugins(PluginsCommand {
            action: PluginsAction::Settings(_)
        })
    ));

    let secret = Cli::parse_from([
        "oxideterm",
        "secrets",
        "set",
        "--scope",
        "ai",
        "--id",
        "provider-1",
        "--env",
        "OXIDE_AI_KEY",
    ]);
    assert!(matches!(
        secret.command,
        Command::Secrets(SecretsCommand {
            action: SecretsAction::Set(_)
        })
    ));
}

#[test]
fn parses_portable_runtime_commands() {
    let status = Cli::parse_from(["oxideterm", "portable", "status", "--json"]);
    assert!(matches!(
        status.command,
        Command::Portable(PortableCommand {
            action: PortableAction::Status(PortableStatusArgs { json: true })
        })
    ));

    let setup = Cli::parse_from(["oxideterm", "portable", "setup", "--password-stdin"]);
    assert!(matches!(
        setup.command,
        Command::Portable(PortableCommand {
            action: PortableAction::Setup(PortablePasswordArgs {
                password_stdin: true,
                ..
            })
        })
    ));

    let change = Cli::parse_from([
        "oxideterm",
        "portable",
        "change-password",
        "--current-password-env",
        "OLD",
        "--new-password-env",
        "NEW",
    ]);
    assert!(matches!(
        change.command,
        Command::Portable(PortableCommand {
            action: PortableAction::ChangePassword(PortableChangePasswordArgs {
                current_password_env: Some(_),
                new_password_env: Some(_),
                ..
            })
        })
    ));
}

#[test]
fn parses_cloud_sync_backend_configurations() {
    // Each backend must retain its backend-specific credential-free configuration fields.
    let cli = Cli::parse_from([
        "oxideterm",
        "cloud-sync",
        "backend",
        "s3",
        "configure",
        "--s3-bucket",
        "oxideterm",
        "--s3-region",
        "us-east-1",
        "--dry-run",
    ]);
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Backend(command) => match command.action {
                CloudSyncBackendAction::S3(command) => match command.action {
                    CloudSyncBackendConfigureAction::Configure(args) => {
                        assert_eq!(args.s3_bucket.as_deref(), Some("oxideterm"));
                        assert_eq!(args.s3_region.as_deref(), Some("us-east-1"));
                        assert!(args.write.dry_run);
                    }
                },
                _ => panic!("expected s3 backend"),
            },
            _ => panic!("expected cloud-sync backend"),
        },
        _ => panic!("expected cloud-sync command"),
    }

    let cli = Cli::parse_from([
        "oxideterm",
        "cloud-sync",
        "backend",
        "onedrive",
        "configure",
        "--microsoft-oauth-client-id",
        "client-id",
        "--dry-run",
    ]);
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Backend(command) => match command.action {
                CloudSyncBackendAction::OneDrive(command) => match command.action {
                    CloudSyncBackendConfigureAction::Configure(args) => {
                        assert_eq!(args.microsoft_oauth_client_id.as_deref(), Some("client-id"));
                        assert!(args.write.dry_run);
                    }
                },
                _ => panic!("expected onedrive backend"),
            },
            _ => panic!("expected cloud-sync backend"),
        },
        _ => panic!("expected cloud-sync command"),
    }

    let cli = Cli::parse_from([
        "oxideterm",
        "cloud-sync",
        "backend",
        "google-drive",
        "configure",
        "--google-oauth-client-id",
        "google-client-id",
        "--dry-run",
    ]);
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Backend(command) => match command.action {
                CloudSyncBackendAction::GoogleDrive(command) => match command.action {
                    CloudSyncBackendConfigureAction::Configure(args) => {
                        assert_eq!(
                            args.google_oauth_client_id.as_deref(),
                            Some("google-client-id")
                        );
                        assert!(args.write.dry_run);
                    }
                },
                _ => panic!("expected google-drive backend"),
            },
            _ => panic!("expected cloud-sync backend"),
        },
        _ => panic!("expected cloud-sync command"),
    }
}
