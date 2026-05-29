# Troubleshooting

Start troubleshooting from the desktop app. The visible app state usually tells you whether the problem is a saved profile, a live SSH node, a terminal session, SFTP, forwarding, sync, settings, or a plugin.

## First Checks In The App

Check the relevant surface before editing files or running repair commands:

- Sessions: confirm the saved connection exists and has the expected host, user, port, group, and authentication mode.
- Connection Monitor: check whether the node is connected, connecting, stale, reconnecting, or unavailable.
- Terminal tab: confirm the shell accepts input and whether a command is still running.
- SFTP or File Manager: confirm the target node is live before retrying directory reads or transfers.
- Settings: check recent changes to terminal, SSH, AI, cloud sync, plugin, or update settings.
- Notifications: review recent warnings and errors.

If a connection or surface is stale, try reconnecting from the app before changing configuration.

## Common Recovery Steps

For settings issues, reopen Settings and check the section that was changed most recently. If the app reports invalid settings, revert the smallest visible change first.

For connection issues, edit the saved connection and retry it from Sessions. Avoid creating duplicates until you know the original profile is wrong.

For SFTP or forwarding issues, check the owning SSH node in Connection Monitor. Retry after the node is live.

For cloud sync issues, open Cloud Sync, inspect status, and review conflicts before choosing a direction.

## Backups First

Before applying a restore, import, sync apply, or manual file repair, create or verify a backup from the app. Review the restore plan and apply the smallest section that solves the issue.

## CLI Companion Diagnostics

Use the CLI companion when you need read-only diagnostics, CI checks, or support bundles:

```sh
oxideterm paths --json
oxideterm diagnose --json
oxideterm doctor --strict
oxideterm report --json
```

`doctor --strict` treats warnings as failures, which is useful for CI or migration scripts.

For focused checks:

```sh
oxideterm settings validate --strict
oxideterm connections validate --strict
oxideterm cloud-sync status --json
```

## Bug Reports

For issue reports, attach a redacted bundle rather than raw config files:

```sh
oxideterm report --bundle ./oxideterm-report.json --json
```

Review the bundle before sharing. Remove private hostnames, usernames, paths, or project names if needed.
