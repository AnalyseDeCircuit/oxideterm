# Getting Started

## Install Shape

Native packages bundle the desktop app, the standalone `oxideterm` CLI, icons, and remote agent binaries. The CLI is built for the same target as the app and stored under the app resources.

On macOS, packaged artifacts include a `.dmg`, an `.app.zip`, and a portable archive. Linux and Windows portable packages use platform-appropriate archive formats.

## First Checks

Use the CLI to inspect paths and run a read-only health check:

```sh
oxideterm paths
oxideterm doctor --strict
oxideterm report --json
```

During development, run the same commands through Cargo:

```sh
cargo run -p oxideterm-cli -- paths
cargo run -p oxideterm-cli -- doctor --strict
```

## Configuration Directory

By default, OxideTerm reads the same configuration files as the desktop app. Use `paths` to see the active files:

```sh
oxideterm paths --json
```

For scripts, CI, or migrations, point the CLI at another config root:

```sh
oxideterm --config-dir ./fixtures/profile-a paths
OXIDETERM_CONFIG_DIR=./fixtures/profile-a oxideterm doctor --strict
```

Use named profiles when one config root must hold several isolated profiles:

```sh
oxideterm --config-dir ./fixtures --profile staging paths
```

Profile data is stored under `profiles/<name>` inside the selected config directory.

## Safe Write Pattern

Most write commands default to dry-run. Inspect the plan first, then repeat with `--yes` only when the change is expected:

```sh
oxideterm settings set terminal.fontSize 14 --dry-run --json
oxideterm settings set terminal.fontSize 14 --yes
```

Backups and restore flows should be used before high-risk changes such as bulk imports, cloud-sync apply, or `.oxide` imports.
