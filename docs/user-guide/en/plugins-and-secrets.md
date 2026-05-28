# Plugins and Secrets

## Plugins

Use the desktop plugin manager for interactive install, browse, enable, disable, and settings flows. Use the CLI for headless checks and scripted changes:

```sh
oxideterm plugins list --json
oxideterm plugins enable demo.plugin --dry-run
oxideterm plugins disable demo.plugin --dry-run
```

Plugin state managed by the CLI is intended for headless workflows. If the desktop app is open, close or refresh the related view before relying on changed state.

## Plugin Settings

Plugin settings are stored as serialized values:

```sh
oxideterm plugins settings list --json
oxideterm plugins settings get demo.plugin/theme --json
oxideterm plugins settings set demo.plugin/theme --value-json '"dark"' --dry-run
oxideterm plugins settings export --json
```

Use import/export for repeatable setup, and review JSON before confirmed writes.

## General Secrets

The unified `secrets` command handles AI provider keys, plugin secrets, cloud-sync secrets, connection secrets, and portable secrets without printing values:

```sh
oxideterm secrets status --scope ai --json
printf '%s' "$OPENAI_API_KEY" | oxideterm secrets set --scope ai --id builtin-openai --stdin
oxideterm secrets set --scope plugin --plugin-id demo.plugin --key token --env PLUGIN_TOKEN
oxideterm secrets clear --scope cloud-sync --key token
```

Secret rules:

- Prefer stdin or environment variables.
- Do not pass secret values as command arguments.
- Do not paste secrets into support reports.
- Check JSON output for hints/status, not values.
