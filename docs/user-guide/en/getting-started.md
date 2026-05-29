# Getting Started

## Install Shape

Native packages bundle the desktop app, icons, remote agent binaries, and the standalone `oxideterm` CLI companion. The desktop app is the main entry point.

On macOS, packaged artifacts include a `.dmg`, an `.app.zip`, and a portable archive. Linux and Windows portable packages use platform-appropriate archive formats.

## First Launch

Open the OxideTerm desktop app first. The main window is a tabbed SSH workspace with a left activity bar for sessions, files, forwarding, plugins, cloud sync, notifications, and settings.

Start with a local terminal tab:

1. Create a local terminal.
2. Run a simple command such as `pwd` or `echo ok`.
3. Open Settings and confirm terminal font, theme, shell, and keyboard behavior.
4. Add one saved SSH connection.
5. Connect to the host and confirm the terminal opens.

After that, try the app surfaces you expect to use most: SFTP/file manager, connection monitor, port forwarding, IDE workspace, AI sidebar, and plugin manager.

## Check The App

After the first local terminal and first SSH connection work, check the app surfaces you expect to use:

- Sessions: saved connections and active SSH nodes.
- Connection monitor: connection health, stale nodes, and reconnect state.
- File manager or SFTP: remote directory browsing and transfers.
- IDE workspace: remote project folders and editor tabs.
- AI sidebar: current workspace context and tool approvals.
- Plugins: installed plugins and plugin settings.
- Cloud sync: sync status and backup state.

## CLI Companion Diagnostics

Use the CLI companion only when you want a read-only diagnostic view of paths and health:

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

The desktop app and CLI read the same configuration files. Use the desktop Settings UI for normal interactive changes. Use `paths` when you need to see the active files for diagnostics or scripting:

```sh
oxideterm paths --json
```

For scripts, CI, or migrations, the CLI can point at another config root:

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

For everyday app use, make ordinary configuration changes from Settings, the connection manager, the plugin manager, or the cloud sync surface. For scripted CLI writes, inspect the plan first, then repeat with `--yes` only when the change is expected:

```sh
oxideterm settings set terminal.fontSize 14 --dry-run --json
oxideterm settings set terminal.fontSize 14 --yes
```

Backups and restore flows should be used before high-risk changes such as bulk imports, cloud-sync apply, or `.oxide` imports.
