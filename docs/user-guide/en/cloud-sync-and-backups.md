# Cloud Sync and Backups

Use the Cloud Sync and backup surfaces in the desktop app for normal sync status, manual sync, conflict review, and recovery checks. Use the CLI companion for automation, CI, and support bundles after you understand the visible app state.

## Cloud Sync Status

Open Cloud Sync to see whether sync is configured, when it last ran, and whether local or remote state needs attention. Before changing sync direction, inspect the status and any warnings in the app.

Use manual sync actions from the app when you want to push or pull intentionally. If a conflict appears, resolve it from the visible state rather than guessing from file names or timestamps.

## Configure Sync

Configure the backend from the Cloud Sync settings surface. Keep backend names, namespaces, and endpoints descriptive, but do not put tokens or passwords in labels.

Secrets should be entered through secret fields or the app's credential storage flow. Status views should show hints, configured flags, or missing-secret warnings, not raw secret values.

## Backups

Create a backup before high-impact operations:

- Bulk connection imports.
- `.oxide` imports.
- Cloud sync apply or conflict resolution.
- Plugin state migrations.
- Settings changes that affect terminal, SSH, privilege credentials, AI, or sync behavior.

Use the app's backup or restore surface to inspect what will change before applying it. For important restores, check the plan first, apply the smallest needed section, then reopen the affected app surface and verify the result.

## Support Bundles

Use support bundles when you need to share diagnostics. Review the generated bundle before sending it. It should contain paths, counts, warnings, revisions, and secret hints rather than secret values, including for privilege credentials.

## CLI Companion

For scripted sync, restore plans, CI checks, or support bundles, use the CLI companion:

```sh
oxideterm cloud-sync status --json
oxideterm cloud-sync preview --json
oxideterm cloud-sync diff --dirty-only --format table
oxideterm backup preview --json
oxideterm backup create --output ./oxideterm-backup.json --json
oxideterm report --bundle ./oxideterm-report.json --json
```

For CLI writes, run a dry-run first and only confirm after the plan matches the intended direction:

```sh
oxideterm cloud-sync push --dry-run --json
oxideterm cloud-sync apply --from remote --strategy merge --dry-run
oxideterm backup restore ./oxideterm-backup.json --section settings --dry-run --json
```
