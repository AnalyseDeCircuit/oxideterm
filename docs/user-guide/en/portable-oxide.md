# Portable `.oxide` Bundles

`.oxide` bundles are encrypted portable exports for moving OxideTerm data between machines or profiles. They can include connections, forwards, app settings, quick commands, plugin settings, and optionally portable secrets.

## Validate and Preview

Always validate and preview before importing:

```sh
oxideterm oxide validate ./profile.oxide
oxideterm oxide preview-import ./profile.oxide --password-stdin --json
oxideterm oxide diff ./profile.oxide --strategy merge --password-env OXIDE_PASSWORD
```

Prefer stdin or environment variables for bundle passwords. Do not put passwords directly in shell history.

## Import

Choose a conflict strategy:

- `skip`: keep existing local records.
- `rename`: import conflicts under new names.
- `replace`: replace local records.
- `merge`: merge compatible records.

Example:

```sh
oxideterm oxide import ./profile.oxide \
  --strategy merge \
  --import-portable-secrets \
  --password-env OXIDE_PASSWORD \
  --dry-run
```

Repeat with `--yes` after reviewing the plan.

## Export

Export a selected profile:

```sh
oxideterm oxide export ./profile.oxide \
  --connection prod \
  --forward web \
  --description "Production workspace" \
  --password-env OXIDE_PASSWORD \
  --json
```

Use `--include-portable-secrets` only when the recipient machine needs encrypted portable secrets. Use `--embed-keys` only when you intentionally want private key files inside the encrypted bundle.

## Portable Runtime

The portable runtime keystore protects portable secrets:

```sh
oxideterm portable status --json
printf '%s' "$PORTABLE_PASSWORD" | oxideterm portable setup --password-stdin
oxideterm portable unlock --password-env OXIDETERM_PORTABLE_PASSWORD
```

Use `portable reset --yes` only when you intentionally want to delete the local portable keystore.
