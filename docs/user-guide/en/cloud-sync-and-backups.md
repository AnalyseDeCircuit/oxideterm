# Cloud Sync and Backups

## Cloud Sync Status

Cloud sync keeps selected local state aligned with a configured remote backend. Inspect before changing anything:

```sh
oxideterm cloud-sync status --json
oxideterm cloud-sync preview --json
oxideterm cloud-sync diff --dirty-only --format table
```

Use backend-specific configure commands when possible:

```sh
oxideterm cloud-sync backend webdav configure \
  --endpoint https://example.invalid/sync \
  --namespace personal \
  --dry-run
```

## Push, Pull, Apply, Resolve

Write operations should be reviewed first:

```sh
oxideterm cloud-sync push --dry-run --json
oxideterm cloud-sync pull --dry-run --json
oxideterm cloud-sync apply --from remote --strategy merge --dry-run
oxideterm cloud-sync resolve --strategy local-wins --dry-run
```

Use `--yes` only after the JSON plan matches the intended direction.

## Cloud Sync Secrets

Cloud-sync secrets should be written through stdin or environment variables:

```sh
oxideterm cloud-sync secrets status --json
printf '%s' "$SYNC_TOKEN" | oxideterm cloud-sync secrets set token --stdin
oxideterm cloud-sync secrets clear token
```

Status output should contain hints, not secret values.

## Backups

Create a backup before bulk imports, sync apply, or risky settings changes:

```sh
oxideterm backup preview --json
oxideterm backup create --output ./oxideterm-backup.json --json
oxideterm backup inspect ./oxideterm-backup.json --summary
oxideterm backup verify ./oxideterm-backup.json
```

Restore defaults to dry-run behavior. Review the restore plan before confirming:

```sh
oxideterm backup restore ./oxideterm-backup.json --section settings --dry-run --json
oxideterm backup diff ./oxideterm-backup.json --section connections --json
```

## Support Bundles

Use a redacted report bundle for issue reports:

```sh
oxideterm report --bundle ./oxideterm-report.json --json
```

Review the file before sharing it. The bundle is designed to contain paths, counts, warnings, revisions, and secret hints rather than secret values.
