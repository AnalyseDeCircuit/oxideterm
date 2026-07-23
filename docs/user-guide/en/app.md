# Application Guide

This guide introduces the OxideTerm desktop app. Use it for day-to-day terminal, SSH, file, forwarding, Host Tools, graphics/VNC, AI, plugin, sync, and settings work. The `oxideterm` CLI is a separate companion for automation, diagnostics, CI, migration, and recovery.

## First Run

Open OxideTerm from your operating system launcher. On first run, start with a local terminal tab before adding remote hosts. A local tab confirms that rendering, keyboard input, shell startup, font settings, and theme settings work on the current machine.

Recommended first pass:

1. Open a local terminal.
2. Type a simple command such as `pwd` or `echo ok`.
3. Open Settings and review terminal font, theme, shell, and AI settings.
4. Add one saved SSH connection.
5. Connect to the host and verify terminal input, file browsing, Host Tools, and connection status.

## App Layout

The main window is organized around a tabbed workspace with a left activity bar and contextual side panels.

Primary areas:

- Sessions: saved connections, active SSH nodes, and terminal sessions.
- Connection pool: reusable connection/runtime state.
- Connection monitor and Host Tools: health, reconnect status, processes, Docker, services, tmux, packages, logs, ports, filesystems, and metrics.
- Connection matrix: a broader connection overview.
- File manager and SFTP: browse, preview, upload, download, and edit remote files.
- Graphics/VNC: open visual remote sessions when a connected node supports them.
- Plugins: install, enable, disable, and configure plugins.
- Cloud sync: sync app state and inspect sync status.
- Notifications: review app events and warnings.
- Settings: configure app behavior.

Tabs are the main work surface. A tab can hold a local terminal, SSH terminal, SFTP view, IDE workspace, graphics/VNC view, settings page, file manager, plugin manager, monitor, or other app surface.

## Tabs and Panes

Use tabs to separate tasks. Use split panes inside a terminal tab when one task needs multiple shells side by side.

Terminal state belongs to the pane. Closing a pane closes the visible terminal, but it should not be treated as deleting a saved connection profile. For remote work, connection/runtime state is tracked separately from the visible terminal tab.

Typical terminal workflow:

1. Open or select a saved connection.
2. Start an SSH terminal.
3. Split the terminal if needed.
4. Use command marks and shell history to follow command output.
5. Keep the connection monitor visible for long-running or unstable sessions.

Terminal-adjacent helpers stay tied to the active terminal pane:

- Use the terminal context menu or command bar for copy, paste, search, command selection, and explicit transfer actions.
- Configure terminal background images from Settings; the background is visual state, not terminal scrollback.
- When an X/Y/ZMODEM prompt appears after a real transfer command such as `rz`, `sz`, `rx`, or `rb`, choose the local file or directory and watch progress from the visible prompt/notification.
- Manage privilege credentials from Settings. Do not place sudo/su passwords in connection names, notes, quick commands, AI prompts, logs, or support bundles.

## Saved Connections

Use saved connections for hosts you connect to repeatedly. A saved connection can include:

- Name, host, port, and username.
- Group, color, and tags for navigation.
- Authentication mode, such as SSH agent, key, password, or default SSH behavior.
- Optional connection behavior such as post-connect commands.
- Optional proxy or jump-host configuration when supported by the configured connection.

Use groups and tags for navigation. Do not store passwords or token values in names, groups, tags, or notes.

## Connecting to Hosts

From the Sessions area, select a saved connection and open it. The app creates runtime state for the SSH node and opens a terminal when the connection succeeds.

If a host disconnects, use the connection pool or monitor to understand whether the runtime is reconnecting, stale, or unavailable. Reconnect from the app state instead of recreating the saved profile.

Host Tools and graphics/VNC sessions also belong to the connected node. Opening or closing those views should not rewrite the saved profile, and stale resource snapshots should be refreshed or reconnected from the node state.

## SFTP and Remote Files

Use SFTP or the file manager for remote file operations:

- Browse remote directories.
- Preview files before downloading or editing.
- Upload and download files.
- Start large transfers and track their progress.
- Retry transfers after reconnecting an unstable host.

File Transfer settings default to **Auto**: SFTP remains the browsing and preferred transfer protocol, while legacy SCP is used only when the connected POSIX host cannot open its SFTP subsystem. You can force SFTP or SCP for compatibility testing. SCP can pause and resume while its current channel is alive, but a retry after disconnect starts from the beginning because legacy SCP has no offset-resume protocol.

Terminal-native modem transfers are separate from SFTP. Use them when the remote program expects X/Y/ZMODEM protocol bytes through the current terminal channel.

Before overwriting important remote files, confirm the path and keep a backup. Remote file writes are real writes on the target system.

## IDE Workspace

Use the IDE surface when you need a project-style remote file workflow. Open an IDE workspace for a connected node, choose a remote folder, and work with project files in tabs.

IDE workspace state is separate from an ordinary terminal tab. The active editor tab, project root, dirty buffers, and open files belong to the IDE surface. If the connection is interrupted, reconnect before relying on saves or project search.

## Port Forwarding

Use the forwarding UI for local, remote, and dynamic forwards:

- Local forward: expose a local port that connects to a remote target.
- Remote forward: expose a remote port that connects back to a local target.
- Dynamic forward: create a SOCKS-style tunnel.

Use auto-start only for forwards that should start whenever the owning connection opens. Keep the connection monitor visible when testing a new forward.

## Host Tools and Graphics

Use Host Tools from the connected-node context when you need a read-oriented view of processes, containers, services, tmux, packages, logs, ports, filesystems, scheduled tasks, or host metrics. Actions that change host state should be reviewed in the app confirmation flow before execution.

Use graphics/VNC sessions for remote visual workflows. The viewer is an app surface attached to the connected node; rendered frames are not terminal output, and closing the viewer is separate from deleting the saved connection.

## AI Sidebar

The AI sidebar is intended to work with the current app context. It can inspect targets, use terminal and file tools when tool use is enabled, and summarize or act on the current workspace state.

Good AI workflow:

1. Open the relevant terminal, connection, SFTP, IDE, or settings surface.
2. Include context only when needed.
3. Let the AI list or select explicit targets before running commands.
4. Review approval prompts for write, interactive, or destructive actions.
5. Check tool results before accepting follow-up changes.

Never paste secrets into AI prompts. Use the app's provider key and secret storage surfaces for API keys or credentials.

## Settings

Use Settings for interactive configuration:

- General app behavior.
- Appearance, theme, and whole-window opacity.
- Terminal renderer, shell, font, encoding, background images, transfer helpers, and local terminal behavior.
- Privilege credentials and their prompt/scope settings.
- SSH, reconnect, SFTP, and IDE behavior.
- AI providers, model selection, memory, tool use, and knowledge settings.
- Keybindings, help, and update information.

Use the desktop UI when you are making exploratory or visual changes. Use the CLI only when the change must be scripted or repeated across environments.

## Plugins

Use the plugin manager to install, enable, disable, update, and configure plugins. Review plugin permissions and settings before enabling plugins you did not write.

Plugin settings and plugin secrets should be managed through app surfaces designed for that purpose. Do not put secret values in plugin names, labels, or ordinary text fields unless the field is explicitly a secret field.

## Cloud Sync and Backups

Use Cloud Sync from the desktop app when you want to inspect sync status, run a manual sync, or resolve user-visible sync issues. Keep backups enabled before applying high-impact imports, restores, or sync changes.

For a support bundle or automated restore plan, use the CLI companion after confirming the issue in the desktop app.

## CLI Companion

Use the CLI companion when the work is headless, repeatable, or diagnostic:

- `doctor` and support reports.
- Scripted settings changes.
- Headless connection validation.
- Backup, restore, and cloud-sync automation.
- CI checks for exported configuration.

For normal daily work, start in the desktop app. The CLI should support the desktop workflow, not replace it.
