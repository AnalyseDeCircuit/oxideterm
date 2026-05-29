# Portable `.oxide` Bundles

`.oxide` bundles are encrypted portable exports for moving OxideTerm data between machines or profiles. They can include connections, forwards, app settings, quick commands, plugin settings, and optionally portable secrets.

Use the desktop app for normal import and export flows so you can review what will change before applying it.

## Preview Before Import

Open the portable import surface, choose the `.oxide` file, and preview its contents before importing. Check:

- Which connections, forwards, settings, quick commands, and plugin settings are included.
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

Use portable secrets only when the recipient machine needs encrypted secret material. Embed private key files only when that is intentional and the bundle password is strong.

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
