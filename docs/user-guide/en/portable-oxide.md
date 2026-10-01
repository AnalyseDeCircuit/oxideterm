# Portable `.oxide` Bundles

`.oxide` bundles are encrypted portable exports for moving OxideTerm data between machines or profiles. They can include connections, forwards, app settings, quick commands, plugin settings, managed SSH keys, and optionally portable secrets.

Use the desktop app for normal import and export flows so you can review what will change before applying it.

## Preview Before Import

Open the portable import surface, choose the `.oxide` file, and preview its contents before importing. Check:

- Which connections, forwards, settings, quick commands, and plugin settings are included.
- Whether managed SSH keys are present and whether they will be restored into OxideTerm.
- Whether portable secrets are present.
- Which records conflict with the current profile.
- Which conflict strategy will be used.

Do not import a bundle until the preview matches what you expect.

## Import Strategies

Choose a conflict strategy:

- `skip`: keep existing local records.
- `rename`: import conflicts under new names.
- `replace`: replace local records.
- `merge`: merge compatible records.

Use the smallest strategy that matches the task. For example, prefer `skip` or `rename` when inspecting a bundle from another machine; use `replace` only when you intentionally want the bundle to override local records.

## Export

Use the export surface to choose what should be included in a bundle. Add a clear description so the receiving machine can identify the bundle later.

Credential material is explicit:

- Saved server passwords are excluded by default.
- External private key files are copied only when key embedding is enabled.
- Saved key or certificate passphrases have a separate visible option.
- OxideTerm-managed SSH keys can be included so managed-key connections restore to the managed key store.
- Managed-key passphrases are excluded by default unless the export explicitly includes them.
- Portable secrets are for portable migration and similar self-contained profile moves.

Use portable secrets only when the recipient machine needs encrypted secret material. Embed private key files or managed keys only when that is intentional and the bundle password is strong.

## Managed SSH Keys

Managed keys are OxideTerm-owned credentials. A connection stores only a managed key reference, while the private key material stays in the local keychain or portable keystore.

When a bundle contains managed keys, the import preview lets you restore them into OxideTerm. If restore is disabled, OxideTerm may use embedded fallback key files when available; otherwise affected connections need auth repair after import.

Duplicate managed keys are matched by fingerprint and should reuse the existing key instead of creating another copy.

## Cloud Sync Boundary

Cloud Sync uploads encrypted snapshots according to its selected scope. Passwords, managed private keys, AI keys, and other supported credentials require the separate sensitive-sync opt-in. Use a manual `.oxide` export for a one-time credential transfer, and check its individual inclusion options. See [cloud sync setup](cloud-sync-and-backups.md#configure-sync).

## Portable Runtime

The portable runtime keystore protects portable secrets after import. Set it up through the app's portable runtime or secret storage surface. If the keystore is locked, unlock it before relying on imported portable secrets.

Only reset the portable runtime when you intentionally want to remove the local portable keystore.

### Set up a portable folder

1. Put the application in a writable folder. On macOS, use the folder containing `OxideTerm.app`; for an AppImage, use the folder containing the AppImage; for an unpacked executable, use its folder.
2. With OxideTerm closed, create an empty file named `portable` in that folder. On the next launch, data will use its `data` subfolder.
3. Alternatively, create `portable.json` there to choose a relative data directory:

   ```json
   { "enabled": true, "dataDir": "data" }
   ```

   `dataDir` must stay inside the portable folder: absolute paths and `.` or `..` path components are rejected. `portable.json` takes precedence over the empty marker.
4. Launch OxideTerm and complete the portable password setup or unlock prompt. In **Settings → General**, check the displayed data directory and portable status before importing a bundle.
5. Import the selected `.oxide` data and try a saved connection. When moving the portable installation, close it first and copy the application, marker/configuration, and the complete selected data directory, including `keystore.vault`.

Keep the portable password when moving devices. A device-specific automatic unlock does not replace it. Portable SSH configuration lookup is described in [OpenSSH configuration](connections-and-forwards.md#openssh-configuration).

## Data-directory backups

Find the active location under **Settings → General → Data Directory** or run `oxideterm paths --json`. A normal installation may use a custom directory; portable mode uses the directory selected above.

Quit OxideTerm and stop CLI operations before copying the entire directory. This preserves local databases, history, and recordings together with their files. In an installed profile, protected keys also depend on the original operating system's credential store; a directory copy alone is not a complete credential migration. Use an encrypted `.oxide` export with the needed credential options when moving to another device. Portable profiles instead need their complete keystore and password.

Changing Data Directory requires a restart and does not automatically move existing files. Record the old location, copy the needed data while the app is closed, then check the selected location and data after restarting. Keep the original copy until the restored configuration has been verified.

## CLI Companion

Use the CLI companion for automation, CI validation, or scripted migration:

```sh
oxideterm oxide validate ./profile.oxide
oxideterm oxide preview-import ./profile.oxide --password-stdin --json
oxideterm oxide diff ./profile.oxide --strategy merge --password-env OXIDE_PASSWORD
oxideterm portable status --json
```

Prefer stdin or environment variables for bundle passwords. Do not put passwords directly in shell history.
