# Cloud Sync and Backups

Use the Cloud Sync and backup surfaces in the desktop app for normal sync status, manual sync, conflict review, and recovery checks. Use the CLI companion for automation, CI, and support bundles after you understand the visible app state.

## Cloud Sync Status

Open Cloud Sync to see whether sync is configured, when it last ran, and whether local or remote state needs attention. Before changing sync direction, inspect the status and any warnings in the app.

Synchronize to merge independent changes from both sides. When the same item has conflicting changes, choose the version to keep in the preview.

## Configure Sync

Configure the backend from the Cloud Sync settings surface. Keep backend names, namespaces, and endpoints descriptive, but do not put tokens or passwords in labels.

Secrets should be entered through secret fields or the app's credential storage flow. Status views should show hints, configured flags, or missing-secret warnings, not raw secret values.

## Backups

### Change the sync password

Enter the new password in Cloud Sync settings and save. After confirmation, OxideTerm creates a new sync space and switches to it once the upload is verified. The previous space and its data remain available. Update the namespace and password on your other devices to join the new space.

If the upload fails, submit the same new password again to resume the change.

Save other settings changes before changing the password. The CLI command `oxideterm cloud-sync change-password --yes` reads the new password from standard input, which can be supplied by a password manager.

### `.oxide` files

New `.oxide` exports encrypt metadata such as connection names and counts. Enter the password to preview their contents. Older files remain readable; new files require a client that supports the new format. Archives containing a complete certificate and private-key pair are imported into managed key storage, preserving certificate authentication and the private-key passphrase.

### Create a backup

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
