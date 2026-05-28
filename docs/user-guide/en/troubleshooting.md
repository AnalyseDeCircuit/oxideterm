# Troubleshooting

## Start With Read-Only Checks

Use these commands before editing files:

```sh
oxideterm paths --json
oxideterm diagnose --json
oxideterm doctor --strict
oxideterm report --json
```

`doctor --strict` treats warnings as failures, which is useful for CI or migration scripts.

## Common Recovery Steps

If settings fail to load:

```sh
oxideterm settings validate --strict
oxideterm settings show --json
```

If connections look wrong:

```sh
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
```

If cloud sync behaves unexpectedly:

```sh
oxideterm cloud-sync status --json
oxideterm cloud-sync history --failed-only
oxideterm cloud-sync diff --dirty-only --format table
```

## Backups First

Before fixing state by writing over files, create a backup:

```sh
oxideterm backup create --output ./before-fix.json --json
```

Then apply a focused fix with dry-run first.

## Shell Completion

Generate or install completions:

```sh
oxideterm completion zsh > ~/.zfunc/_oxideterm
oxideterm completion path zsh
oxideterm completion install zsh
```

Use `--force` only when replacing an existing generated file.

## Bug Reports

For issue reports, attach a redacted bundle rather than raw config files:

```sh
oxideterm report --bundle ./oxideterm-report.json --json
```

Review the bundle before sharing. Remove private hostnames, usernames, paths, or project names if needed.
