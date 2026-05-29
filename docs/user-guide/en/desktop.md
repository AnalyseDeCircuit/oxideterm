# Desktop Workflows

## First Launch

Open OxideTerm, then check the left activity bar for the main work areas: sessions, connection pool, connection monitor, connection matrix, plugins, cloud sync, file manager, notifications, and settings.

If the app starts with no sessions, create a local shell tab first. This verifies the terminal renderer, shell integration, input handling, and theme settings before you add remote hosts.

## Activity Bar

Use the activity bar as the entry point for app surfaces:

- Sessions: create, open, group, and monitor SSH work.
- File manager and SFTP: browse files and manage transfers.
- Connection pool and monitor: inspect connection runtime state.
- Plugins: manage installed plugins and plugin settings.
- Cloud sync: inspect sync status and run sync actions.
- Notifications: review recent warnings and errors.
- Settings: change app behavior and provider configuration.

When a workflow becomes confusing, return to Sessions or Connection Monitor first. Those views show whether a host is saved, connecting, connected, stale, or unavailable.

## Terminal Panes

Use terminal tabs for local shells and SSH sessions. Split panes when a task needs multiple shells in the same workspace. Command marks, shell integration, and terminal history belong to the pane, so closing a pane should not be treated as disconnecting a saved SSH host.

For long-running jobs, keep the owning connection visible in the connection pool or monitor. Reconnect behavior is tied to the connection/runtime state, not only to the visible terminal tab.

Common pane patterns:

- One tab per task when tasks are unrelated.
- Split panes for commands that should be compared side by side.
- Keep one monitoring pane open for logs while another pane performs edits or deploy steps.
- Close only the pane or tab you no longer need; keep the saved connection profile intact.

## Saved Connections

Use saved connections for hosts you expect to reuse. Set the host, user, port, group, color, tags, auth method, and optional post-connect command. Prefer SSH agent or key-based auth where possible.

Groups are for navigation and bulk organization. They should not encode secrets or environment-specific passwords.

After saving a connection, open it from the Sessions view. If it fails, edit the saved connection instead of creating duplicates with nearly identical hostnames or labels.

## Connection Runtime Views

The connection pool and monitor show live runtime state. Use them when a terminal looks stuck, SFTP cannot read a directory, a forward is not responding, or reconnect behavior needs to be checked.

Runtime state answers different questions than saved profiles:

- Saved profile: what host should OxideTerm connect to?
- Runtime node: is that host currently connected or reconnecting?
- Terminal session: which visible shell is attached to the runtime?
- SFTP session: is file browsing using a live transport?

## File Manager and SFTP

Use the file manager for remote browsing, uploads, downloads, previews, and basic file operations. Treat remote edits as real remote writes: keep backups for critical files, and verify paths before overwriting.

When a connection is unstable, pause large transfers and reconnect before retrying. Saved connection state and transfer state are separate; a failed transfer should not require deleting the connection.

## IDE Workspace

Use the IDE workspace for project-style remote editing. Open it from a connected node, choose a remote folder, then work with file tabs inside the IDE surface.

Before saving important changes, confirm the connection is still healthy. Dirty editor buffers belong to the IDE workspace, so do not close the IDE tab until you have saved, discarded, or intentionally kept the edits.

## AI Sidebar

Use the AI sidebar when the current terminal, connection, file, or settings context matters. Keep the relevant tab open before asking for help. If tool use is enabled, review approval prompts for writes, terminal input, and dangerous commands.

For command execution, prefer asking the AI to target a specific saved connection, SSH node, terminal session, SFTP session, or IDE workspace. Avoid asking it to infer a host from a command string.

## Settings

Settings are grouped by feature area. Use the desktop UI for interactive changes such as appearance, terminal behavior, AI provider setup, cloud sync, portable runtime, and help/about.

For scripted or repeatable changes, use the CLI with `--dry-run` first. The CLI and desktop app read the same configuration files.

## Command Palette and Navigation

Use tabs and the activity bar for normal navigation. Use the command palette when you know the action name but do not want to leave the keyboard.

If a surface opens the wrong context, switch back to the Sessions view, select the intended connection or tab, and reopen the surface from there.

## Updates

Use Settings → Help & About to check the active version and update channel. Stable, beta, and GPUI preview builds use separate update channels, so choose the channel that matches the build you installed.
