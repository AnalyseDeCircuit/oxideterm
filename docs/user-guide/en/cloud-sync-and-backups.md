# Cloud Sync and Backups

Use Cloud Sync to share selected configuration between devices. Use an encrypted `.oxide` export for a separate backup or a one-time move to another device.

## Cloud Sync Status

Open Cloud Sync to see whether sync is configured, when it last ran, and whether local or remote state needs attention. Before changing sync direction, inspect the status and any warnings in the app.

Synchronize to merge independent changes from both sides. When the same item has conflicting changes, choose the version to keep in the preview.

## Configure Sync

### First device

1. Open **Cloud Sync → Configure** and choose a remote backend. **Local file** provides manual import/export rather than device-to-device synchronization.
2. Fill in the storage location and its authentication. The location must be accessible to every device you want to connect.
3. Choose a **Namespace**, folder path, or object prefix for this set of devices. Set a **Sync Password** for encrypting the shared data. Keep this password available for the second device; it is separate from the storage provider's login or access token.
4. Select the data to sync: connections, saved forwards, quick commands, transport profiles, app settings, or plugin settings. Review the selected settings groups. Local terminal environment variables and sensitive credentials have separate opt-in controls.
5. Click **Save Settings**. Start with a manual sync before enabling automatic uploads.
6. Click **Synchronize**, review **Sync preview**, then click **Apply and synchronize**. With an empty remote location, this publishes your selected local configuration.
7. Check the result and the last-sync time in **Overview**. If using a newly created GitHub Gist, note its Gist ID for the next device.

The fields depend on the backend:

| Backend | Location to use on both devices |
| --- | --- |
| WebDAV | Endpoint and namespace |
| S3 Compatible | Service endpoint, bucket, region, and object prefix |
| Git Repository | Repository, branch, and namespace |
| GitHub Gist | The same Gist ID and namespace; an empty ID can create a private Gist on first publication |
| Dropbox | Account and folder path |
| OneDrive / Google Drive | Account and configured sync folder or namespace |
| HTTP JSON | A server implementing OxideTerm's current sync protocol, endpoint, and namespace |

Enter passwords and tokens in the dedicated credential fields. With sensitive sync disabled, connection passwords, key passphrases, managed private keys, AI keys, and privilege credentials are excluded. Enable **Sync sensitive credentials** only for the selected data you intend to share; the app asks for confirmation.

### Add a second device

1. Install a version that supports the same cloud-sync format.
2. Open Cloud Sync → Configure. Select the same backend and storage location, including the namespace, branch, bucket, or Gist ID where applicable.
3. Authenticate this device with the storage provider and enter the **same sync password**. Select the data you want on this device and save the settings.
4. Click Synchronize. Inspect the preview, especially if this device already has its own connections or settings.
5. Choose versions for any conflicts and click Apply and synchronize. Check the imported connections and settings, then try one connection.
6. If credentials were excluded, supply them on the second device. A key-file path from the first device may also need to be changed to a path available here.

You can enable auto upload after a successful manual sync. Independent edits merge; conflicting edits remain available for review. Turning off a category excludes it from this device's sync selection and does not delete its remote data.

## Conflicts and interrupted sync

The preview lists changed fields and competing versions. Choose the version you want for each conflict; **Current device** identifies the current local candidate. Values such as credentials appear as **Protected value**. Unselected conflicts keep the current local value and remain unresolved. Apply and synchronize saves the selected result and publishes it to the shared location; synchronize the other devices to receive it.

If local settings changed while the remote preview was loading, the apply step includes those edits in the merge. Review any conflicts that remain after applying.

| Message or symptom | Next step |
| --- | --- |
| Authentication or storage access failed | Check the provider login, endpoint, and access to the chosen folder, bucket, or repository; save and retry |
| Shared data cannot be decrypted | Confirm that the location and sync password match the other device |
| Upload failed after local changes were applied | Keep the current configuration and synchronize again to retry publication |
| Cleanup is pending | Synchronize again to finish cleanup |
| Sync was interrupted while applying data | Reopen OxideTerm so it can recover the pending operation, then retry sync |
| HTTP JSON server upgrade required | Update that server to support the current protocol before retrying |

If recovery remains blocked, keep the current data directory and report the error. Do not remove the recovery files to bypass it. For command-line recovery, close the desktop app and run `oxideterm cloud-sync sync --yes`; this command can apply and publish changes as well as recover an interrupted operation.

## Change the sync password

Enter the new password in Cloud Sync settings and save. After confirmation, OxideTerm creates a new sync space and switches to it once the upload is verified. The previous space and its data remain available. Update the namespace and password on your other devices to join the new space.

If the upload fails, submit the same new password again to resume the change.

Save other settings changes before changing the password. The CLI command `oxideterm cloud-sync change-password --yes` reads the new password from standard input, which can be supplied by a password manager.

## Backups

### `.oxide` files

New `.oxide` exports encrypt metadata such as connection names and counts. Enter the password to preview their contents. Older files remain readable; new files require a client that supports the new format. Archives containing a complete certificate and private-key pair are imported into managed key storage, preserving certificate authentication and the private-key passphrase.

### Create a backup

Create a backup before high-impact operations:

- Bulk connection imports.
- `.oxide` imports.
- Cloud sync apply or conflict resolution.
- Plugin state migrations.
- Settings changes that affect terminal, SSH, privilege credentials, AI, or sync behavior.

For a manual encrypted backup, choose **Local file** in Cloud Sync → Configure, set the archive password and selected content, save, then use **Export to Local File**. Keep the file and password separately. To restore it, use **Import from Local File**, inspect the preview, select the needed data and conflict strategy, then confirm. See [portable bundles](portable-oxide.md) for credential options. Save your remote-backend configuration again when returning to remote synchronization.

If the Cloud Sync History view contains an earlier rollback backup, select it and use **Restore Selected Backup** to review the restore preview. This applies the saved configuration locally; synchronize again when you want other devices to receive the resulting changes.

The CLI also creates a JSON backup of settings, connections, and cloud-sync metadata. It does not replace an encrypted credential export. A settings-only restore can be reviewed and applied with:

```sh
oxideterm backup create --output ./oxideterm-backup.json --json
oxideterm backup verify ./oxideterm-backup.json --json
oxideterm backup restore ./oxideterm-backup.json --section settings --dry-run --json
```

After reviewing the dry-run, apply that settings-only restore:

```sh
oxideterm backup restore ./oxideterm-backup.json --section settings --yes --json
```

Reopen the affected settings and verify the result. For a complete copy of local history and recordings, see [data-directory backups](portable-oxide.md#data-directory-backups).

## Support Bundles

Use support bundles when you need to share diagnostics. Review the generated bundle before sending it. It should contain paths, counts, warnings, revisions, and secret hints rather than secret values, including for privilege credentials.

## CLI Companion

For scripted sync, restore plans, CI checks, or support bundles, use the CLI companion:

```sh
oxideterm cloud-sync status --json
oxideterm cloud-sync sync --dry-run --json
oxideterm cloud-sync preview --json
oxideterm cloud-sync diff --dirty-only --format table
oxideterm backup preview --json
oxideterm backup create --output ./oxideterm-backup.json --json
oxideterm report --bundle ./oxideterm-report.json --json
```

`cloud-sync sync --dry-run` prepares a preview from the remote backend without applying or publishing it. The separate `preview` and `diff` commands inspect cached state. Close the desktop app before running sync from the CLI, which needs exclusive access to the configuration.

For CLI writes, run a dry-run first and confirm after the plan matches the intended changes:

```sh
oxideterm cloud-sync sync --yes --json
```
