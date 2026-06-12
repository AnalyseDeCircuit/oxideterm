// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use clap::Parser;

use super::*;

#[test]
fn parses_global_config_dir_and_profile() {
    let cli = Cli::parse_from([
        "oxideterm",
        "--config-dir",
        "/tmp/oxide",
        "--profile",
        "ci",
        "paths",
    ]);

    assert_eq!(cli.config_dir.unwrap().to_string_lossy(), "/tmp/oxide");
    assert_eq!(cli.profile.as_deref(), Some("ci"));
}

#[test]
fn parses_connections_show_json() {
    let cli = Cli::parse_from(["oxideterm", "connections", "show", "prod", "--json"]);
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Show(args) => {
                assert_eq!(args.query, "prod");
                assert!(args.json);
            }
            _ => panic!("expected show command"),
        },
        _ => panic!("expected connections command"),
    }
}

#[test]
fn parses_cloud_sync_status() {
    let cli = Cli::parse_from(["oxideterm", "cloud-sync", "status", "--json"]);
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Status(args) => assert!(args.json),
            _ => panic!("expected status command"),
        },
        _ => panic!("expected cloud-sync command"),
    }
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
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Diff(args) => {
                assert!(args.dirty_only);
                assert_eq!(args.category, Some(CloudSyncDiffCategory::AppSettings));
                assert_eq!(args.format, Some(CloudSyncDiffFormat::Table));
                assert!(args.json);
            }
            _ => panic!("expected diff command"),
        },
        _ => panic!("expected cloud-sync command"),
    }
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
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::State(command) => match command.action {
                CloudSyncStateAction::Get(args) => {
                    assert_eq!(args.key, "settings.namespace");
                    assert!(args.json);
                }
                _ => panic!("expected get command"),
            },
            _ => panic!("expected state command"),
        },
        _ => panic!("expected cloud-sync command"),
    }
}

#[test]
fn parses_connections_search() {
    let cli = Cli::parse_from(["oxideterm", "connections", "search", "prod", "--json"]);
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Search(args) => {
                assert_eq!(args.query, "prod");
                assert!(args.json);
            }
            _ => panic!("expected search command"),
        },
        _ => panic!("expected connections command"),
    }
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
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Export(args) => {
                assert_eq!(args.sections, ["general"]);
                assert!(args.include_local_terminal_env_vars);
                assert!(args.json);
            }
            _ => panic!("expected export command"),
        },
        _ => panic!("expected settings command"),
    }
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
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Export(args) => {
                assert_eq!(args.format, ConnectionsExportFormat::RawSafe);
                assert!(args.json);
            }
            _ => panic!("expected export command"),
        },
        _ => panic!("expected connections command"),
    }
}

#[test]
fn parses_connections_delete_dry_run() {
    let cli = Cli::parse_from([
        "oxideterm",
        "connections",
        "delete",
        "prod",
        "--dry-run",
        "--json",
    ]);
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Delete(args) => {
                assert_eq!(args.query, "prod");
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected delete command"),
        },
        _ => panic!("expected connections command"),
    }
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
    match create.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Create(args) => {
                assert_eq!(args.spec_path.as_deref(), Some("connection.json"));
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected create command"),
        },
        _ => panic!("expected connections command"),
    }

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
    match edit.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Edit(args) => {
                assert_eq!(args.query, "prod");
                assert_eq!(args.spec_path.as_deref(), Some("patch.json"));
                assert!(args.write.yes);
                assert!(args.write.json);
            }
            _ => panic!("expected edit command"),
        },
        _ => panic!("expected connections command"),
    }
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
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Group(command) => match command.action {
                ConnectionsGroupAction::Rename(args) => {
                    assert_eq!(args.old_name, "old");
                    assert_eq!(args.new_name, "new");
                    assert!(args.write.dry_run);
                    assert!(args.write.json);
                }
                _ => panic!("expected group rename command"),
            },
            _ => panic!("expected group command"),
        },
        _ => panic!("expected connections command"),
    }
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
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::ApplySnapshot(args) => {
                assert_eq!(args.path, "connections.json");
                assert_eq!(args.strategy, ConnectionsApplyStrategy::Merge);
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected apply-snapshot command"),
        },
        _ => panic!("expected connections command"),
    }
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
    match cli.command {
        Command::Oxide(command) => match command.action {
            OxideAction::PreviewImport(args) => {
                assert_eq!(args.path, "bundle.oxide");
                assert_eq!(args.strategy, OxideImportStrategy::Replace);
                assert!(args.password.password_stdin);
                assert!(args.json);
            }
            _ => panic!("expected oxide preview-import command"),
        },
        _ => panic!("expected oxide command"),
    }
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
    match cli.command {
        Command::Oxide(command) => match command.action {
            OxideAction::Import(args) => {
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
            _ => panic!("expected oxide import command"),
        },
        _ => panic!("expected oxide command"),
    }
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
    match cli.command {
        Command::Oxide(command) => match command.action {
            OxideAction::Export(args) => {
                assert_eq!(args.connection_queries, ["prod"]);
                assert!(args.password.password_stdin);
                assert!(args.overwrite);
                assert!(args.json);
            }
            _ => panic!("expected oxide export command"),
        },
        _ => panic!("expected oxide command"),
    }
}

#[test]
fn parses_settings_sections() {
    let cli = Cli::parse_from(["oxideterm", "settings", "sections", "--json"]);
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Sections(args) => assert!(args.json),
            _ => panic!("expected sections command"),
        },
        _ => panic!("expected settings command"),
    }
}

#[test]
fn parses_settings_set_dry_run() {
    let cli = Cli::parse_from([
        "oxideterm",
        "settings",
        "set",
        "terminal.scrollback",
        "2000",
        "--dry-run",
        "--json",
    ]);
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Set(args) => {
                assert_eq!(args.key, "terminal.scrollback");
                assert_eq!(args.value, "2000");
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected set command"),
        },
        _ => panic!("expected settings command"),
    }
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
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Unset(args) => {
                assert_eq!(args.key, "ai.customSystemPrompt");
                assert!(args.write.yes);
                assert!(args.write.no_backup);
                assert!(args.write.json);
            }
            _ => panic!("expected unset command"),
        },
        _ => panic!("expected settings command"),
    }
}

#[test]
fn parses_settings_apply_dry_run() {
    let cli = Cli::parse_from([
        "oxideterm",
        "settings",
        "apply",
        "snapshot.json",
        "--dry-run",
        "--json",
    ]);
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Apply(args) => {
                assert_eq!(args.path, "snapshot.json");
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected apply command"),
        },
        _ => panic!("expected settings command"),
    }
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
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Import(args) => {
                assert_eq!(args.path, "snapshot.json");
                assert_eq!(args.sections, ["general"]);
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected import command"),
        },
        _ => panic!("expected settings command"),
    }
}

#[test]
fn parses_connections_validate() {
    let cli = Cli::parse_from(["oxideterm", "connections", "validate", "--strict", "--json"]);
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Validate(args) => {
                assert!(args.strict);
                assert!(args.json);
            }
            _ => panic!("expected validate command"),
        },
        _ => panic!("expected connections command"),
    }
}

#[test]
fn parses_top_level_diagnostics() {
    let cli = Cli::parse_from(["oxideterm", "diagnose", "--json"]);
    match cli.command {
        Command::Diagnose(args) => assert!(args.json),
        _ => panic!("expected diagnose command"),
    }
}

#[test]
fn parses_doctor() {
    let cli = Cli::parse_from(["oxideterm", "doctor", "--strict", "--json"]);
    match cli.command {
        Command::Doctor(args) => {
            assert!(args.strict);
            assert!(args.json);
        }
        _ => panic!("expected doctor command"),
    }
}

#[test]
fn parses_backup_inspect() {
    let cli = Cli::parse_from([
        "oxideterm",
        "backup",
        "inspect",
        "backup.json",
        "--full",
        "--json",
    ]);
    match cli.command {
        Command::Backup(command) => match command.action {
            BackupAction::Inspect(args) => {
                assert_eq!(args.query, "backup.json");
                assert!(args.full);
                assert!(args.section.is_none());
                assert!(args.json);
            }
            _ => panic!("expected inspect command"),
        },
        _ => panic!("expected backup command"),
    }
}

#[test]
fn parses_backup_preview() {
    let cli = Cli::parse_from(["oxideterm", "backup", "preview", "--json"]);
    match cli.command {
        Command::Backup(command) => match command.action {
            BackupAction::Preview(args) => assert!(args.json),
            _ => panic!("expected preview command"),
        },
        _ => panic!("expected backup command"),
    }
}

#[test]
fn parses_backup_verify() {
    let cli = Cli::parse_from(["oxideterm", "backup", "verify", "backup.json", "--json"]);
    match cli.command {
        Command::Backup(command) => match command.action {
            BackupAction::Verify(args) => {
                assert_eq!(args.query, "backup.json");
                assert!(args.json);
            }
            _ => panic!("expected verify command"),
        },
        _ => panic!("expected backup command"),
    }
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
    match cli.command {
        Command::Backup(command) => match command.action {
            BackupAction::Create(args) => {
                assert_eq!(args.output.as_deref(), Some("/tmp/backup.json"));
                assert!(args.json);
            }
            _ => panic!("expected create command"),
        },
        _ => panic!("expected backup command"),
    }
}

#[test]
fn parses_settings_validate() {
    let cli = Cli::parse_from(["oxideterm", "settings", "validate", "--strict", "--json"]);
    match cli.command {
        Command::Settings(command) => match command.action {
            SettingsAction::Validate(args) => {
                assert!(args.strict);
                assert!(args.json);
            }
            _ => panic!("expected validate command"),
        },
        _ => panic!("expected settings command"),
    }
}

#[test]
fn parses_backup_inspect_section() {
    let cli = Cli::parse_from([
        "oxideterm",
        "backup",
        "inspect",
        "backup.json",
        "--section",
        "cloud-sync",
        "--json",
    ]);
    match cli.command {
        Command::Backup(command) => match command.action {
            BackupAction::Inspect(args) => {
                assert_eq!(args.section, Some(BackupInspectSection::CloudSync));
                assert!(args.json);
            }
            _ => panic!("expected inspect command"),
        },
        _ => panic!("expected backup command"),
    }
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
    match cli.command {
        Command::Backup(command) => match command.action {
            BackupAction::Restore(args) => {
                assert_eq!(args.query, "backup.json");
                assert_eq!(args.section, Some(BackupInspectSection::Settings));
                assert!(!args.write.yes);
                assert!(args.write.json);
            }
            _ => panic!("expected restore command"),
        },
        _ => panic!("expected backup command"),
    }
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
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::History(args) => {
                assert!(args.failed_only);
                assert!(args.json);
            }
            _ => panic!("expected history command"),
        },
        _ => panic!("expected cloud-sync command"),
    }
}

#[test]
fn parses_cloud_sync_push_dry_run() {
    let cli = Cli::parse_from(["oxideterm", "cloud-sync", "push", "--dry-run", "--json"]);
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Push(args) => {
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected cloud-sync push command"),
        },
        _ => panic!("expected cloud-sync command"),
    }
}

#[test]
fn parses_cloud_sync_configure_multi_backend_settings() {
    let cli = Cli::parse_from([
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
    ]);
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Configure(args) => {
                assert_eq!(args.backend, Some(CloudSyncBackendArg::S3));
                assert_eq!(args.s3_bucket.as_deref(), Some("oxide-sync"));
                assert_eq!(args.s3_region.as_deref(), Some("us-east-1"));
                assert_eq!(
                    args.default_conflict_strategy,
                    Some(CloudSyncConflictStrategy::Merge)
                );
                assert!(args.write.dry_run);
                assert!(args.write.json);
            }
            _ => panic!("expected cloud-sync configure command"),
        },
        _ => panic!("expected cloud-sync command"),
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
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Apply(args) => {
                assert_eq!(args.from, CloudSyncApplySource::Remote);
                assert_eq!(args.strategy, Some(CloudSyncConflictStrategy::Replace));
                assert!(args.write.yes);
                assert!(args.write.json);
            }
            _ => panic!("expected cloud-sync apply command"),
        },
        _ => panic!("expected cloud-sync command"),
    }
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
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Secrets(command) => match command.action {
                CloudSyncSecretsAction::Set(args) => {
                    assert_eq!(args.key, "sync-password");
                    assert_eq!(args.env.as_deref(), Some("OXIDE_SYNC_PASSWORD"));
                    assert!(args.json);
                }
                _ => panic!("expected secrets set command"),
            },
            _ => panic!("expected secrets command"),
        },
        _ => panic!("expected cloud-sync command"),
    }
}

#[test]
fn parses_cloud_sync_secrets_status() {
    let cli = Cli::parse_from(["oxideterm", "cloud-sync", "secrets", "status", "--json"]);
    match cli.command {
        Command::CloudSync(command) => match command.action {
            CloudSyncAction::Secrets(command) => match command.action {
                CloudSyncSecretsAction::Status(args) => assert!(args.json),
                _ => panic!("expected secrets status command"),
            },
            _ => panic!("expected secrets command"),
        },
        _ => panic!("expected cloud-sync command"),
    }
}

#[test]
fn parses_report() {
    let cli = Cli::parse_from([
        "oxideterm",
        "report",
        "--bundle",
        "/tmp/report.json",
        "--json",
    ]);
    match cli.command {
        Command::Report(args) => {
            assert_eq!(args.bundle.as_deref(), Some("/tmp/report.json"));
            assert!(args.json);
        }
        _ => panic!("expected report command"),
    }
}

#[test]
fn parses_completion_shell() {
    let cli = Cli::parse_from(["oxideterm", "completion", "zsh"]);
    match cli.command {
        Command::Completion(args) => {
            assert_eq!(args.shell, Some(CompletionShell::Zsh));
        }
        _ => panic!("expected completion command"),
    }
}

#[test]
fn parses_completion_install() {
    let cli = Cli::parse_from(["oxideterm", "completion", "install", "zsh", "--force"]);
    match cli.command {
        Command::Completion(args) => match args.action {
            Some(CompletionAction::Install(args)) => {
                assert_eq!(args.shell, CompletionShell::Zsh);
                assert!(args.force);
            }
            _ => panic!("expected completion install command"),
        },
        _ => panic!("expected completion command"),
    }
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
    match cli.command {
        Command::Connections(command) => match command.action {
            ConnectionsAction::Create(args) => {
                assert_eq!(args.direct.name.as_deref(), Some("prod"));
                assert_eq!(args.direct.host.as_deref(), Some("prod.example.com"));
                assert_eq!(args.direct.username.as_deref(), Some("deploy"));
                assert_eq!(args.direct.port, Some(2222));
                assert_eq!(args.direct.auth, Some(ConnectionAuthArg::Agent));
            }
            _ => panic!("expected connections create command"),
        },
        _ => panic!("expected connections command"),
    }
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
fn parses_cloud_sync_backend_configure() {
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
}

#[test]
fn parses_cloud_sync_onedrive_backend_configure() {
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
}
