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
oxideterm connections edit prod --totp-credential "Shared MFA" --dry-run
oxideterm connections edit prod --clear-totp-credential --dry-run
oxideterm connections rename prod production --yes
oxideterm connections validate --strict
oxideterm connections export --format raw-safe --json
```

`connections open` resolves an exact saved SSH connection name or id, then opens it through the native application's normal saved-connection flow. The GUI remains responsible for loading credentials, proxy settings, and other connection options.

`connections create` and `connections edit` accept `--totp-credential` with an existing credential's name or ID. Names are matched without case sensitivity; use an ID if several credentials share a name. JSON specs use `totpCredential`, including within `proxyChain` entries, or `clearTotpCredential: true` to remove the target connection's binding. Credential seeds stay in protected storage.

For password or passphrase input, prefer `--password-stdin`, `--password-env`, `--passphrase-stdin`, or `--passphrase-env`. Do not pass secret values directly as shell arguments.

`connections batch-create` creates many connections at once from an OpenSSH-style template plus one data line per host.

Template: at most one `Host` block, whose `HostName` is `{host}` or omitted. Placeholders `{host}`, `{profile}`, `{user}`, `{port}`, `{name}`, `{group}` are substituted in `User`, `IdentityFile`, `CertificateFile`, and `ProxyCommand`; `Port` takes a literal number. Comment lines starting with `#` are dropped, and any other directive is reported with its line number.

Data: `[user[:auth]@]host[:port][?name=…&group=…&profile=…&tags=a,b]`, one connection per line, `#` starts a comment. Every row needs a username from the row or the template. When a template value a row actually uses contains `{profile}` (the template user, its `ProxyCommand`, or the template identity for rows without their own credential), that row must supply `profile=`. An inline `auth` slot that contains `/` or `\`, or starts with `~`, is a private key path; anything else is a password. Use `key=` to force a key path and `pass=` to force a password that happens to look like a path. A row's inline credential beats the template `IdentityFile`. Rows without one use the template identity; if the template has no identity file, the shared `--password-env`/`--password-stdin`/`--key-path` flags supply authentication, and they are rejected when the template carries `IdentityFile` or `CertificateFile`. An empty slot (`user:@host`) is an error, and a batch is limited to 2,000 rows.

Input: `--spec <path>` points to a JSON file with `template`, `data`, and an optional `duplicate` policy. It cannot be combined with `--template-file`/`--data-file`. `--dry-run` lists the rows that would be added, skipped, or rejected without writing; `--json` switches the report to JSON. Writes need `--yes`, and a write is refused while any line is invalid, so a batch is never applied partially.

Duplicates: rows are the same connection only when the account and the credential match (`user@host:port` plus the same key or password); such a repeated row is always skipped. A name that already exists for the same `user@host:port` is reported as `batchUnchanged` when the row repeats it under the default skip policy, and as `batchUpdate` when the row differs (a new `group=` or `tags=` lands on the stored connection); the same name on a different target is skipped by default, or suffixed when you pass `--duplicate rename`. A `ProxyCommand` template needs ProxyCommand authorization enabled in the app settings.

```sh
oxideterm connections batch-create --template-file ./template.conf --data-file ./hosts.txt --dry-run
oxideterm connections batch-create --template-file ./template.conf --data-file ./hosts.txt --password-env OX_SSH_PW --yes
```

Inline credentials in a data or spec file are plaintext on disk: prefer `--password-stdin`/`--password-env` for a shared password, and protect or delete any file that carries one.

ProxyCommand placeholders are expanded within individual arguments, so spaces and quotes in data do not create extra arguments. Use a direct executable, such as `ProxyCommand aws ssm start-session --profile {profile} --target %h --document-name AWS-StartSSHSession --parameters portNumber=%p`. Keep the executable and any inline shell script fixed; pass dynamic values to a POSIX shell as separate positional arguments. Windows command hosts and `env -S` do not accept dynamic values in batch templates.

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
oxideterm cloud-sync sync --dry-run --json
oxideterm cloud-sync sync --yes --json
oxideterm cloud-sync secrets status --json
```

Close the desktop app before running CLI sync, including a dry-run: the command requires exclusive access to the configuration. `sync --dry-run` prepares a plan from the remote backend without applying or publishing it. `preview` and `diff` inspect cached state. Review the plan before using `--yes`.

The current causal sync merges independent changes. The retained `push`, `pull` and `apply --from remote` commands use the same merge coordinator; their names do not select upload-only or download-only behavior. `pull` and `apply` accept only the merge strategy. Choose individual remote conflict candidates in the desktop preview. `resolve --strategy local-wins` selects local candidates for conflicts; inspect its dry-run before applying. Archive-import strategies such as skip, rename and replace belong to `.oxide` imports.

Local application and remote publication have separate outcomes. If publication fails after local changes were applied, retain the configuration and retry sync. See [conflicts and interrupted sync](cloud-sync-and-backups.md#conflicts-and-interrupted-sync) for recovery.

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
