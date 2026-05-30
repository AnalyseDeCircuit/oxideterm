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

Cloud Sync can upload encrypted `.oxide` snapshots, but background or plugin-driven sync does not silently include managed SSH keys. Use the manual export or portable migration flow when you need to move complete credential material between machines.

## Portable Runtime

The portable runtime keystore protects portable secrets after import. Set it up through the app's portable runtime or secret storage surface. If the keystore is locked, unlock it before relying on imported portable secrets.

Only reset the portable runtime when you intentionally want to remove the local portable keystore.

## CLI Companion

Use the CLI companion for automation, CI validation, or scripted migration:

```sh
oxideterm oxide validate ./profile.oxide
oxideterm oxide preview-import ./profile.oxide --password-stdin --json
oxideterm oxide diff ./profile.oxide --strategy merge --password-env OXIDE_PASSWORD
oxideterm portable status --json
```

Prefer stdin or environment variables for bundle passwords. Do not put passwords directly in shell history.
