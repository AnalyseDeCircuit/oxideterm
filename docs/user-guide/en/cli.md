# CLI Companion

The `oxideterm` CLI is for headless inspection, automation, CI checks, migration, and recovery. It should not print secret values. Commands that expose credentials should report hints or status only.

## Global Options

```sh
oxideterm --config-dir <path> <command>
oxideterm --profile <name> <command>
OXIDETERM_CONFIG_DIR=<path> oxideterm <command>
```

Use `--json` or `--format json` for scripts. Use `doctor --strict` or command-specific `--strict` flags when warnings should fail CI.

Most write commands share the same safety flags:

- `--dry-run`: show the plan without writing.
- `--yes`: confirm a real write.
- `--json` or `--format json`: produce machine-readable output.

## Diagnostics

```sh
oxideterm paths --json
oxideterm diagnose --json
oxideterm doctor --strict
oxideterm report --json
```

Use `report --bundle <path>` when preparing a support bundle. Review the bundle before sharing it.

## Settings

```sh
oxideterm settings validate --strict
oxideterm settings sections --json
oxideterm settings get ai.providers --json
oxideterm settings set terminal.fontSize 14 --dry-run
oxideterm settings export --section appearance --json
oxideterm settings diff ./settings-snapshot.json --section appearance
```

`set` and `unset` update existing JSON paths only. Use `--yes` to confirm writes.

## Connections

```sh
oxideterm connections list
oxideterm connections search prod --json
oxideterm connections open prod
oxideterm connections create --name prod --host example.internal --user deploy --port 22 --dry-run
oxideterm connections rename prod production --yes
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
```

`connections open` resolves an exact saved SSH connection name or id, then opens it through the native application's normal saved-connection flow. The GUI remains responsible for loading credentials, proxy settings, and other connection options.

For password or passphrase input, prefer `--password-stdin`, `--password-env`, `--passphrase-stdin`, or `--passphrase-env`. Do not pass secret values directly as shell arguments.

## Backups and Restore

```sh
oxideterm backup create --output ./oxideterm-backup.json --json
oxideterm backup inspect ./oxideterm-backup.json --summary
oxideterm backup restore ./oxideterm-backup.json --section settings --dry-run --json
```

Restore commands should be reviewed in dry-run form before `--yes`.

## Cloud Sync

```sh
oxideterm cloud-sync status --json
oxideterm cloud-sync diff --dirty-only --format table
oxideterm cloud-sync backend webdav configure --endpoint https://example.invalid/sync --dry-run
oxideterm cloud-sync push --dry-run --json
oxideterm cloud-sync pull --dry-run --json
oxideterm cloud-sync apply --from remote --strategy merge --dry-run
oxideterm cloud-sync secrets status --json
```

Secret commands must only print hints or status. Use stdin or environment variables for secret writes.

## External MCP stdio bridge

When an external client supports stdio MCP only, first create a client under **Settings → Network & Proxy → External MCP Control** and copy its one-time credential. Configure the client command as:

```sh
OXIDETERM_MCP_TOKEN='<client credential>' oxideterm mcp bridge
```

CC Switch and similar clients that accept one MCP server object can use:

```json
{
  "command": "oxideterm",
  "args": ["mcp", "bridge"],
  "env": {
    "OXIDETERM_MCP_TOKEN": "<client credential>"
  }
}
```

For clients that require a top-level `mcpServers` object, place this object under `mcpServers.oxideterm`.

The bridge discovers the running loopback endpoint from the active configuration directory. Add the same `--profile` or `--config-dir` used by the app when applicable. Set `OXIDETERM_MCP_ENDPOINT` or `--endpoint` only when automatic discovery is unavailable; the override must still be an HTTP `/mcp` URL on `localhost`, `127.0.0.1`, or `::1`. Store the credential in the external client's secret environment settings, never in command arguments, logs, or project files.

## Batch Plans

Batch plans combine several changes into one reviewed operation:

```sh
oxideterm batch apply ./plan.json --dry-run
oxideterm batch apply ./plan.json --yes --json
```

Use batch mode for scripted setup where settings, connection snapshots, and cloud-sync configuration should be reviewed together.

## Completion

```sh
oxideterm completion zsh > ~/.zfunc/_oxideterm
oxideterm completion path zsh
oxideterm completion install zsh
```

Use `--force` with `completion install` only when replacing an existing generated completion file.
