# Desktop Workflows

## First Launch

Open OxideTerm, then check the left activity bar for the main work areas: sessions, connection pool, connection monitor, connection matrix, plugins, cloud sync, file manager, notifications, and settings.

If the app starts with no sessions, create a local shell tab first. This verifies the terminal renderer, shell integration, input handling, and theme settings before you add remote hosts.

## Terminal Panes

Use terminal tabs for local shells and SSH sessions. Split panes when a task needs multiple shells in the same workspace. Command marks, shell integration, and terminal history belong to the pane, so closing a pane should not be treated as disconnecting a saved SSH host.

For long-running jobs, keep the owning connection visible in the connection pool or monitor. Reconnect behavior is tied to the connection/runtime state, not only to the visible terminal tab.

## Saved Connections

Use saved connections for hosts you expect to reuse. Set the host, user, port, group, color, tags, auth method, and optional post-connect command. Prefer SSH agent or key-based auth where possible.

Groups are for navigation and bulk organization. They should not encode secrets or environment-specific passwords.

## File Manager and SFTP

Use the file manager for remote browsing, uploads, downloads, previews, and basic file operations. Treat remote edits as real remote writes: keep backups for critical files, and verify paths before overwriting.

When a connection is unstable, pause large transfers and reconnect before retrying. Saved connection state and transfer state are separate; a failed transfer should not require deleting the connection.

## Settings

Settings are grouped by feature area. Use the desktop UI for interactive changes such as appearance, terminal behavior, AI provider setup, cloud sync, portable runtime, and help/about.

For scripted or repeatable changes, use the CLI with `--dry-run` first. The CLI and desktop app read the same configuration files.

## Updates

Use Settings → Help & About to check the active version and update channel. Stable, beta, and GPUI preview builds use separate update channels, so choose the channel that matches the build you installed.
