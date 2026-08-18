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
- Graphics/VNC: open a saved RDP/VNC profile, or open node-launched graphics when a connected SSH node provides it.
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

### Advanced command sender

Use the advanced command sender when the same input must be delivered to one or more terminal panes on a schedule. It supports text or hexadecimal input, line or character pacing, repeat counts, and current, all, or explicitly selected terminal targets.

The sender freezes its target panes when a job starts. Changing tabs or opening panes does not widen a running job, and a closed target is skipped rather than replaced or reconnected. Hiding the sender panel does not cancel an active job; stop it explicitly when needed. Progress counts local terminal-input acceptance, not remote command completion.

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

Host Tools belong to the connected node. A graphics/VNC session may instead belong to a saved RDP/VNC profile; node-launched graphics use the SSH node as their runtime owner. Opening or closing these views should not rewrite saved profiles, and stale state should be refreshed through the owning profile or node.

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

Use graphics/VNC sessions for remote visual workflows. A viewer is backed either by a saved RDP/VNC profile and its helper process or by a node-owned graphics runtime; rendered frames are not terminal output, and closing the viewer is separate from deleting saved connection data.

## AI Sidebar

The AI sidebar is intended to work with the current app context. It can inspect targets, use terminal and file tools when tool use is enabled, and summarize or act on the current workspace state.

When the selected provider and model expose reasoning controls, the chat surface shows a provider-aware **Thinking Effort** menu. Known models are normalized against capability data; an unknown model with a known provider uses that provider's request format, while an unknown provider is treated as unsupported. ACP sessions use their own session options rather than the native provider reasoning field.

Good AI workflow:

1. Open the relevant terminal, connection, SFTP, IDE, or settings surface.
2. Include context only when needed.
3. Let the AI list or select explicit targets before running commands.
4. Review approval prompts for write, interactive, or destructive actions.
5. Check tool results before accepting follow-up changes.

Never paste secrets into AI prompts. Use the app's provider key and secret storage surfaces for API keys or credentials.

### Agent Skills

OxideSens can discover bounded `SKILL.md` workflows from the workspace, user data directories, and enabled native plugins. Open **Settings → OxideSens → Tools → Agent Skills** to review discovered skills, enable or disable them, and refresh the catalog. Loading a skill provides instructions only; it does not grant terminal, file, credential, or network permissions. Every resulting action still goes through the existing tool policy and approval mode. See the [Agent Skills reference](../../agent-skills.md) for discovery precedence and resource limits.

## External MCP Clients

External editors, command-line AI tools, and other MCP clients can use saved SSH connections through OxideTerm when diagnosing a server. Open **Settings → Network & Proxy → External MCP Control**, then:

1. Create a standard or full-access client. Standard mode requires in-app approval for high-risk actions; full-access mode skips per-action approval only for enabled tool groups.
2. Copy the loopback HTTP endpoint and the client credential, which is shown only once.
   The listening port defaults to `0`, which lets the app choose automatically. Enter and apply a value from `1` to `65535` when a fixed port is required. The preference remains device-local and is excluded from ordinary settings, `.oxide` exports, and cloud sync.
3. Configure the endpoint as a Streamable HTTP URL in the external MCP client and add an `Authorization: Bearer <credential>` request header.
   If the client supports stdio only, configure the command as `oxideterm mcp bridge` and provide `OXIDETERM_MCP_TOKEN=<credential>` through the client's secret environment settings. The bridge discovers the current loopback endpoint automatically. Never put the credential in command arguments.

   CC Switch and clients that accept a `command`, `args`, and `env` object can use the configuration generated by **Copy stdio JSON** in the app:

   ```json
   {
     "command": "oxideterm",
     "args": ["mcp", "bridge"],
     "env": {
       "OXIDETERM_MCP_TOKEN": "<client credential>"
     }
   }
   ```

   If the client requires a top-level `mcpServers` object, wrap the same configuration as follows:

   ```json
   {
     "mcpServers": {
       "oxideterm": {
         "command": "oxideterm",
         "args": ["mcp", "bridge"],
         "env": {
           "OXIDETERM_MCP_TOKEN": "<client credential>"
         }
       }
     }
   }
   ```

   Both JSON forms contain the client credential. Store them only in a trusted client's secret configuration, never in project files, logs, or public content. Install the command-line tool under **Settings → General** and keep OxideTerm running.
4. Have the client call `connections_browse`, then inspect the selected connection. The connection-management group can create, update, or remove SSH, Mosh, Telnet, serial, RDP, and VNC profiles. Credential tools can store a new value, report presence, or forget a slot, but never read an existing secret.
5. In standard mode, review the actual client, target, and command under **Pending Actions** in OxideTerm, then have the client call `mcp_commit_action`. Full-access mode executes directly.
6. Use the returned `command_ref` to query state and read bounded stdout and stderr ranges.

The current implementation exposes connection and credential management, NodeRouter leases, terminal and recording workflows, RDP/VNC, SSH exec, bounded temporary artifacts, SFTP and background single-file transfers, remote IDE workspaces, forwarding, Quick Commands, managed addon lifecycle, Cloud Sync, the current client's redacted MCP audit records, and typed Host Tools snapshots/actions. Background transfers move data only between an authorized SFTP root and client-owned artifact storage; they never accept local paths. The current bound is 64 MiB and restartable resume is not yet available. IDE workspaces are mounted from an authorized SFTP file session; reading and structured editing are separate grants, reads are limited to 4 MiB, and edits require an observed revision. Cloud Sync returns a one-use preview plan and rechecks both local and remote revisions before applying it. Remote publishes do not claim false undo support, and pulls containing SSH, Mosh, or credentials retain only the product's existing encrypted recovery backup. Host Tools accepts only fixed resource and action schemas; it does not expose free-form shell text or plugin calls. Releasing an MCP node lease does not disconnect a physical SSH node still used by terminal, SFTP, IDE, or forwarding consumers; only an explicitly approved `nodes_disconnect` does that.

Background commands and SFTP transfers return an `operation_ref` in addition to their domain handle. `mcp_operation` reads a unified redacted status and progress projection, while `mcp_cancel_operation` requests cancellation. Cancelling a command or upload does not claim that remote side effects were reversed.

An action is reversible only when its result explicitly contains an `undo_ref`. `mcp_revert` currently reuses the strict local Cloud Sync restore path, including the Cloud Sync tool-group and revision checks. Remote publishes, commands, uploads, and permanent deletion never receive a fabricated undo handle.

OxideTerm stores only a digest of each client credential. Disabling or revoking a client immediately cancels its commands, pending actions, and node leases. The endpoint, authorization records, and credential are excluded from ordinary settings, `.oxide` exports, and cloud sync. The external client process is responsible for protecting the stdio bridge environment variable; OxideTerm does not write it back to configuration files.

Full-access mode does not enable every tool. Each client still needs explicit grants for connection, node, command, audit, and temporary-content groups. A client may request additional groups with `mcp_request_access`, but that request always needs approval in the app even in full-access mode. `mcp_revoke_access` immediately disables the client's selected groups and releases their capabilities. Disabled tools are neither advertised nor callable. Bearer authentication, app lock, secret non-disclosure, and audit remain active in both modes.

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
