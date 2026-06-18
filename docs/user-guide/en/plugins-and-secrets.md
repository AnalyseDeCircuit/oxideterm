# Plugins and Secrets

Use the plugin manager in the desktop app for normal plugin work: browsing, installing, enabling, disabling, updating, and configuring plugins. Use the CLI companion only for headless checks and scripted changes.

Developers building plugins should use [Native Plugin Development](./plugin-development.md). That guide covers the native manifest/runtime model, protocol frames, declarative UI, host APIs, and migration from Tauri/Web plugins.

## Plugin Manager

Open Plugins from the activity bar to inspect installed plugins and available plugin actions. Before enabling a plugin you did not write, review its identity, permissions, settings, and any surfaces it adds to the app.

Typical flow:

1. Open Plugins.
2. Select a plugin.
3. Review its description, version, permissions, and settings.
4. Enable or disable it.
5. Reopen the related app surface if the plugin changes menus, tools, panels, or settings.

If a plugin appears to be installed but inactive, check the plugin manager first. Do not edit plugin state files directly while the desktop app is running.

## Plugin Settings

Configure plugin settings from the plugin manager. Use clear labels for non-secret settings. Put credentials only in fields that are explicitly designed as secret fields.

For repeatable setup across machines, export settings through a supported app or CLI flow, review the exported JSON, then import it into the target profile.

## Secrets

Secrets include AI provider keys, plugin tokens, cloud-sync credentials, connection passwords or passphrases, privilege credentials, and portable bundle secrets.

Secret rules:

- Enter secrets through secret fields or credential storage flows.
- Do not put secret values in plugin names, labels, tags, notes, or ordinary text fields.
- Do not paste secrets into AI prompts, support bundles, issue reports, or logs.
- Keep privilege credentials in the dedicated Settings surface; terminal hints or context actions should only submit through the app's secret-aware path.
- Status views should show hints or configured flags, not secret values.

## CLI Companion

For headless plugin checks or scripted setup, use the CLI companion:

```sh
oxideterm plugins list --json
oxideterm plugins enable demo.plugin --dry-run
oxideterm plugins disable demo.plugin --dry-run
oxideterm plugins settings export --json
```

For CLI secret writes, prefer stdin or environment variables:

```sh
oxideterm secrets status --scope ai --json
printf '%s' "$OPENAI_API_KEY" | oxideterm secrets set --scope ai --id builtin-openai --stdin
oxideterm secrets set --scope plugin --plugin-id demo.plugin --key token --env PLUGIN_TOKEN
oxideterm secrets clear --scope cloud-sync --key token
```
