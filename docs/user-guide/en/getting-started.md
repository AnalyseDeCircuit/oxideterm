# Getting Started

## Install Shape

Native packages bundle the desktop app, icons, Linux musl remote file/code-agent
binaries, and the standalone `oxideterm` CLI companion. The remote agent uses
stdio JSON-RPC for structured file, search, Git, and symbol operations; it is
not an SSH authentication agent and does not own terminal, SFTP, or forwarding
connections. The desktop app is the main entry point.

On macOS, packaged artifacts include a `.dmg`, an `.app.zip`, and a portable archive. Linux and Windows portable packages use platform-appropriate archive formats.

To change launch at login on macOS, manage OxideTerm under Login Items in System Settings.

## First Launch

Use the left activity bar for connections and global pages, the top bar to filter tabs, and the right side for OxideSens and Host Tools. See [Workspace Navigation](workspace.md).

1. Download the package for your OS and processor from the [official releases](https://github.com/AnalyseDeCircuit/oxideterm/releases). On macOS, open the DMG and drag OxideTerm to Applications; on Windows, run the installer. Linux packages include AppImage and distribution-specific packages. For an installation that carries its own data, see [portable folders](portable-oxide.md).
2. Start OxideTerm and choose **Local Terminal** on the welcome screen. Run `echo ok` to check shell startup and keyboard input. In **Settings**, check the terminal font, theme, shell and keyboard shortcuts before opening a remote session.
3. Choose **New Connection** and select **SSH**. Give the profile a recognizable name. Enter the server address, port and remote account supplied by your administrator. Choose the authentication method that server accepts; for a private key, select the private key file rather than its public counterpart.
4. Choose **Save & Connect**. On the first connection, compare the host-key fingerprint with a trusted copy from the administrator before accepting it. A key passphrase unlocks your private key; a server password signs in to the remote account. Saving a connection does not by itself authorize storing either secret.
5. At the remote prompt, run `hostname` and check the result. Close the terminal tab, open **Session Manager**, and reopen the saved profile. A new terminal can use a node that is already connected.

To browse files on that server, open SFTP from its session rather than creating another saved host. To stop the connection used by all its tools, disconnect the node. [Connections and forwards](connections-and-forwards.md) explains the difference.

## Check The App

After the first local terminal and first SSH connection work, check the app surfaces you expect to use:

- Sessions: saved connections and active SSH nodes.
- Connection monitor and Host Tools: connection health, stale nodes, reconnect state, host resources, and action results.
- File Manager: browse local folders. Open SFTP on a connected node to browse remote files and transfer them.
- Terminal helpers: context menu actions, command bar actions, background image settings, and X/Y/ZMODEM transfer prompts.
- Remote desktop: install and enable the RDP or VNC plugin, then open a profile from New Connection or Session Manager. Windows WSL Graphics is a separate built-in feature.
- IDE workspace: remote project folders and editor tabs.
- AI sidebar: current workspace context and tool approvals.
- Plugins: installed plugins and plugin settings.
- Cloud sync: sync status and backup state.

## CLI Companion Diagnostics

To check paths and application health without changing settings, run:

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

For everyday app use, make ordinary configuration changes from Settings, the connection manager, the privilege credentials page, the plugin manager, or the cloud sync surface. For scripted CLI writes, inspect the plan first, then repeat with `--yes` only when the change is expected:

```sh
oxideterm settings set terminal.fontSize 14 --dry-run --json
oxideterm settings set terminal.fontSize 14 --yes
```

Backups and restore flows should be used before high-risk changes such as bulk imports, cloud-sync apply, privilege credential changes, or `.oxide` imports.
