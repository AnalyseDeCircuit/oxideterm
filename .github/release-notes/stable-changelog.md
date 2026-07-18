# OxideTerm Stable Changelog

Stable releases are listed newest first. The release workflow uses each versioned
section as the detailed changelog attached to the corresponding GitHub Release.

## 2.0.6

OxideTerm 2.0.6 improves terminal input and command-mark reliability, adds
customizable settings navigation, and expands Linux window-manager integration.

### Highlights

- Added drag-and-drop settings navigation so pages and groups can be reordered, regrouped, saved, or restored to the default layout.
- Added a Linux-only option to hide the application titlebar for tiling compositors while leaving macOS and Windows behavior unchanged.

### Fixes

- Prevented the terminal command bar from crashing when IME composition creates overflow inside its compact input surface.
- Restored the blinking caret before the placeholder when an empty terminal command bar is focused.
- Reset visual command marks when saved history is cleared or terminal resizing reflows the grid, while preserving completed command facts.
- Improved Fish Shell command capture and command-line placement when shell integration events arrive without the usual prompt-start event.

### Release Maintenance

- Refreshed the terminal, SFTP, port-forwarding, and Mini IDE documentation screenshots.

## 2.0.5

OxideTerm 2.0.5 restores several native interface details and improves terminal
selection, glyph rendering, appearance controls, and packaging reliability.

### Fixes

- Fixed custom-drawn Powerline separators and related terminal glyph geometry across supported shapes and cell sizes.
- Fixed copying terminal selections that span multiple wrapped or visible pages.
- Restored the proven grayscale text path on Windows to prevent overlapping or duplicated glyphs.
- Restored onboarding step content that could collapse to zero height in the GPUI layout.
- Refined select-menu hover and selected surfaces, and added localized tooltips to icon-only terminal actions.
- Expanded the managed appearance gallery to three protected bundled backgrounds and extended background opacity control to the full supported range.
- Corrected the introduction page to describe the current WASM plugin system instead of the legacy ESM runtime.

### Release Maintenance

- Made macOS DMG packaging recover from transient busy-volume detach failures.
- Shipped provenance and CC BY 4.0 licensing for bundled background artwork, and limited remote-agent CI to relevant agent changes.

## 2.0.4

OxideTerm 2.0.4 refreshes the native GPUI runtime, improves Linux and virtualized
graphics compatibility, and polishes remote desktop, transfers, scrolling, and
desktop integration.

### Highlights

- Migrated the native desktop layer to a pinned, vendored GPUI-CE baseline while preserving OxideTerm's platform integration across macOS, Windows, and Linux.
- Improved Linux and virtual-machine graphics startup with deterministic Vulkan/OpenGL adapter selection, virtual-GPU handling, recoverable WGPU device resets, and actionable failure diagnostics.
- Updated remote desktop rendering to reuse a stable dynamic texture, apply incremental framebuffer updates, restore content after renderer resets, and hide the local pointer when displaying a remote cursor.

### Fixes

- Fell back from Wayland to X11 when the selected compositor is missing required capabilities, including affected WSLg environments, and reported both backend failures when neither can start.
- Prevented one trackpad or mouse-wheel gesture from scrolling nested inner and outer surfaces at the same time.
- Improved title-bar window controls and wrapped long graphics-startup diagnostics so they remain readable.
- Enlarged the small Windows taskbar and Linux desktop-panel icon artwork without changing macOS application icons or larger launcher assets.
- Recovered interrupted active SFTP transfers as paused work after restart and removed stale completed or cancelled progress records.
- Limited native application logs to one bounded 10 MiB file while retaining the most recent complete entries.
- Refined cloud-sync metadata layout, background-scope selection motion, and localized remote-agent size information.

### Release Maintenance

- Added a pinned GPUI-CE provenance ledger, vendored-source verification, refreshed third-party notices, and packaged license validation.

## 2.0.3

OxideTerm 2.0.3 improves terminal input compatibility, remote directory awareness,
Linux desktop integration, and interface scaling across the native workspace.

### Highlights

- Added user-controlled remote Shell integration for exact SSH working-directory awareness, with inspectable hook files, ask/always/disabled deployment policies, and explicit install, repair, and removal actions.
- Added an application-wide UI font-size setting and a temporary on-screen indicator when terminal font-size shortcuts are used.
- Added a terminal link activation preference that requires Ctrl + click by default on Linux and Windows, or Command + click on macOS, while retaining direct-click activation as an option.

### Fixes

- Restored Ctrl + Insert and Shift + Insert clipboard shortcuts in terminal panes.
- Fixed Linux window identification so the running application matches the stable desktop entry in Ubuntu Dock and other desktop shells.
- Fixed terminal mouse hit testing during fractional smooth scrolling so selections and command blocks align with the painted rows.
- Kept terminal-grid completion owned by the active Shell or TUI application by limiting OxideTerm ghost text to privilege prompts.
- Prevented edited saved connections and jump-host routes from reusing stale SSH nodes that point to an earlier endpoint.

## 2.0.2

OxideTerm 2.0.2 improves application privacy controls, terminal session ownership,
and visual consistency across the native workspace.

### Highlights

- Added application locking with macOS Touch ID and Windows Hello unlock support, plus a setting to hide the lock action from the activity bar.
- Improved terminal, pane, tab, and SSH node ownership so independent consumers keep shared sessions alive and terminal endpoints remain traceable.
- Added animated authentication selection while preserving the existing connection form appearance and reduced-motion behavior.

### Fixes

- Fixed modal backdrops so confirmations dim the complete application window instead of only the current content surface.
- Refined plugin manager typography, selected connection styling, sidebar selection motion, and other native workspace details.
- Improved terminal graphics cache ownership, rendering state, session cleanup, and transfer integration.
- Fixed local terminal background lifecycle handling and several tab, split-pane, SFTP, forwarding, cloud-sync, and plugin host edge cases.

## 2.0.1

OxideTerm 2.0.1 is a maintenance release focused on Linux startup reliability,
cross-version updater compatibility, and settings navigation consistency.

### Fixes

- Fixed a Linux startup panic caused by a Rust/WGSL backdrop-blur structure name mismatch in the Blade renderer.
- Kept stable updater manifests compatible with 1.x clients, including the gzip-compressed macOS application archives and installer-specific platform keys expected by the legacy Tauri updater.
- Fixed settings navigation selection and hover surfaces stretching vertically when the window had spare height.

### Release Maintenance

- Stable releases now become GitHub's Latest release automatically, while prereleases remain excluded from Latest promotion.
- Added release-time validation for the legacy updater package contract so future 2.x releases remain reachable from 1.x installations.
- Removed obsolete preview-status messaging from the localized READMEs and refreshed third-party notices.

## 2.0.0

OxideTerm 2.0 is the largest release in the project's history. The desktop application has been rebuilt around Rust and GPUI, replacing the bundled WebView application shell with a GPU-rendered workspace while preserving OxideTerm's local-first approach to remote operations.

This release brings terminals, saved connections, SFTP, remote editing, port forwarding, Host Tools, RDP/VNC, serial devices, cloud sync, plugins, the `oxideterm` CLI, and OxideSens AI into one shared workspace and runtime model.

### Highlights

- A new GPUI desktop workspace for macOS, Windows, and Linux, with no Electron or bundled browser runtime.
- Replaced the Tauri WebView and xterm.js terminal path with a direct Rust implementation built around `alacritty_terminal`, `portable-pty`, and `russh`, rendered by GPUI.
- Reduced measured idle memory from roughly 300 MB in 1.x to just over 100 MB in the current 2.0 build.
- One SSH node can now serve terminals, SFTP, remote editing, port forwarding, Host Tools, and downstream connections without tying their lifetime to one terminal tab.
- Grace Period reconnect can preserve an existing SSH runtime across short network interruptions when the original connection recovers in time.
- Host Tools add monitoring, processes, services, logs, ports, scheduled tasks, disks, packages, containers, and tmux workflows beside the active connection.
- Built-in RDP and VNC sessions, plus a Windows WSL Graphics connection flow.
- OxideSens now operates inside the native workspace with BYOK providers, MCP, local knowledge retrieval, risk-aware tools, and user-controlled action policy.
- A new native plugin model supports manifest-only, Wasm, and external process runtimes with capability-scoped host APIs.
- Oxide cloud sync is now a built-in workspace feature with multiple user-owned storage backends, conflict preview, history, and rollback.
- The old `oxt` desktop RPC client is replaced by a standalone `oxideterm` CLI for configuration, automation, diagnostics, migration, and recovery.

### Desktop Workspace

- Rebuilt the activity bar, saved-session sidebar, tab strip, auxiliary sidebar, dialogs, overlays, command palette, and settings surfaces in GPUI.
- Added a Session Manager for searching, sorting, grouping, importing, exporting, and editing saved connections.
- Separated saved connections, active SSH nodes, terminal panes, SFTP sessions, forwarding rules, IDE workspaces, and Host Tools so closing one view does not implicitly destroy unrelated runtime owners.
- Added connection topology and runtime views for jump-host relationships, downstream nodes, active consumers, reconnect state, and connection capabilities.
- Added split terminal panes with draggable dividers, pane focus, pane close behavior, and layout restoration.
- Added horizontal tab overflow with wheel-to-horizontal mapping, a visible scrollbar, pointer dragging, and automatic reveal of the active tab.
- Added matching overflow behavior to Host Tools and other compact tool strips.
- Added resizable and persistent sidebars, including resize behavior that remains available after virtualized content loads.
- Added page, dialog, tab-close, sidebar, toast, and popover motion with Normal, Fast, Reduced, and Off profiles.
- Added Zen mode, workspace restore, notification center, connection-status surfaces, diagnostics, and command-palette navigation.
- Added single-instance application handling and responsive system-tray behavior on Windows.
- Restored reopening from the Dock after the last window closes on macOS.

### Terminal and Local Shell

- Replaced the WebView/xterm.js terminal path with a Rust terminal model rendered directly by GPUI.
- Added local and SSH terminals with shared selection, search, scrollback, hyperlink, context-menu, clipboard, and encoding behavior.
- Added terminal search, command marks, shell integration, scrollback viewing, command playback metadata, and clickable links.
- Added Kitty, Sixel, and iTerm-style terminal graphics infrastructure.
- Added IME composition, Unicode bidirectional text handling, CJK font fallback, selectable terminal encodings, and improved wide-character layout.
- Added configurable cursor shape and blink behavior, terminal fonts, separate CJK fonts, font preview, and an opt-in font-ligatures setting.
- Added smooth scrolling, draggable scrollbars, double-click word selection, shift selection, and native terminal context menus.
- Added a multiline command bar with command history, path suggestions, Quick Commands, completion specifications, and risk-aware execution.
- Added terminal-aware current-directory controls and hooks for Git and project context.
- Added optional `sudo` and `su` credential helpers with scoped prompt detection.
- Added in-band trzsz transfers and modem transfer paths for X/Y/ZMODEM workflows.
- Added configurable automatic pane close after the underlying terminal exits.
- Added local shell discovery and configuration for common Unix shells, Command Prompt, Windows PowerShell, PowerShell Core, Git Bash, and Nushell.
- Added shell integration for Bash, Zsh, Fish, Nushell, and PowerShell so local and remote working-directory metadata does not depend on prompt parsing.
- Suppressed background console windows for local shell discovery and helper commands on Windows.

### SSH, Authentication, and Connection Management

- Moved desktop SSH transport to the Rust `russh` stack with `ring` cryptography and no OpenSSL/libssh2 dependency in the SSH implementation.
- Added password, private-key, OpenSSH certificate, managed-key, SSH Agent, and Keyboard-Interactive authentication flows.
- Added support for keyboard-interactive prompts used by common one-time-password, hardware-token, and challenge-response systems.
- Added managed SSH keys that can be imported or pasted, referenced by saved connections, and optionally moved through encrypted `.oxide` bundles.
- Added SSH Agent forwarding.
- Added multi-hop connection trees with independent host, port, username, and authentication settings at each hop.
- Added reuse of saved connections as jump hosts and next-hop nodes.
- Added HTTP CONNECT and SOCKS5 upstream proxies with global, per-connection, and force-direct policies.
- Added strict host-key confirmation, saved host-key removal, and clearer host-key mismatch handling.
- Added a per-connection legacy SSH compatibility option for servers that cannot negotiate the default algorithms.
- Added more specific algorithm-negotiation and authentication diagnostics without including credential values.
- Added optional post-connect commands.
- Improved OpenSSH config parsing for `Match` blocks and multiple aliases in one `Host` declaration.
- Added import flows for OpenSSH config and supported third-party connection managers, including import preview, unsupported-field warnings, duplicate handling, and source groups.
- Added temporary SSH connections that do not need to be saved first.

### Grace Period Reconnect and Runtime Ownership

- Added a node-level Grace Period reconnect pipeline.
- When a supported SSH connection appears lost, OxideTerm probes the original connection for up to 30 seconds before replacing it.
- If the original connection recovers during the grace period, existing terminal programs can continue on that runtime.
- If replacement is required, OxideTerm updates the node runtime and lets supported consumers reacquire the new transport.
- SFTP and remote editing retain their node identity while reacquiring SSH-backed capabilities.
- Saved and active forwarding rules can be restored after a node reconnect, subject to local port availability, permissions, and remote bind acceptance.
- Downstream nodes now observe jump-host link loss and enter the corresponding disconnected or reconnecting state.
- Added clearer runtime and connection-monitor status for connecting, active, idle, link-down, and reconnecting states.
- Removed duplicate reconnect messages from terminal output.

### Directory, Git, and Project Awareness

- Added current-working-directory awareness for local and SSH terminals.
- Added remote shell integration that reports directory, host, Git, and project metadata without scraping the visible prompt.
- Preserved SSH login banners, MOTD output, and last-login text while staging shell integration outside the visible interactive input stream.
- Added current-directory navigation, parent and child browsing, path insertion, and workspace search entry points.
- Added local and remote Git repository detection, branch or detached-HEAD identity, upstream state, ahead/behind counts, staged changes, modifications, untracked files, and conflicts.
- Added time-bounded remote Git status scans that return repository identity before slower working-tree details.
- Added branches, worktrees, changes, staging, history, references, sync, and conflict-oriented Git views.
- Added detection of merge, rebase, cherry-pick, and revert operations in progress.
- Added project-root and project-type detection, project task discovery, task search, and task execution.
- Isolated Git and project snapshots by host, terminal runtime, and working directory to prevent state from leaking between sessions.

### SFTP, Files, and Remote Editing

- Rebuilt SFTP as a node-level capability rather than a terminal-session attachment.
- Added remote navigation, path editing, refresh, selection, bookmarks, and independently opened SFTP views.
- Added single-file and directory upload/download with background queues, parallelism controls, speed limits, progress, throughput, and ETA.
- Added pause, resume, retry, and cancellation for supported transfer stages.
- Added archive-based directory transfer for suitable workloads with fallback to ordinary recursive transfer.
- Added remote archive extraction.
- Added overwrite, skip, rename, and apply-to-all conflict strategies.
- Improved directory progress accounting, remote modification-time handling, short-read recovery, symlink classification, and Windows/POSIX path normalization.
- Made SFTP paths selectable and copyable.
- Added local file management with navigation, drives, sorting, filtering, favorites, creation, copy, cut, paste, rename, delete, drag-and-drop, and context-menu actions.
- Added previews for supported text, source code, Markdown, images, audio, video, hexadecimal data, and fonts.
- Added font specimen and glyph coverage views for code fonts, CJK fonts, and Nerd Font symbols.
- Added a lightweight local and remote editor with project trees, multiple tabs, syntax highlighting, line wrapping, dirty-buffer tracking, save conflicts, safe writes, and workspace state.
- Added symbol indexing and completion support to remote-agent-backed project workflows.

### Port Forwarding and Network Tools

- Added local (`-L`), remote (`-R`), and dynamic SOCKS5 (`-D`) forwarding in the native runtime.
- Bound forwarding ownership to SSH nodes and exposed running, failed, stopped, paused, and restoring states.
- Added saved forwarding rules, optional connection-time startup, pause/resume, reconnect-aware restore, and actionable failure details.
- Added remote listening-port discovery and connection-topology entry points.
- Improved IPv4, IPv6, host-and-port normalization, local port conflict reporting, permission errors, and remote bind diagnostics.
- Added X11 forwarding infrastructure with DISPLAY allocation, Xauthority management, and remote `xauth` setup.
- Added update and SSH proxy settings without exposing proxy credentials in ordinary diagnostics.

### Telnet and Serial

- Added native Telnet sessions with option negotiation, binary mode, echo, terminal type, and window-size negotiation.
- Added local serial terminals with device enumeration or manual device paths, configurable baud rate, data bits, stop bits, parity, and flow control.
- Added saved, editable, importable, and exportable serial profiles with classified device, permission, busy-port, parameter, and disconnect errors.

### Host Tools and Runtime Views

- Added node-scoped Host Tools that remain available independently of any one terminal pane.
- Added CPU, memory, swap, disk, load, network, mount, interface, process, GPU-when-available, and RTT monitoring.
- Added process search, filtering, sorting, TERM/KILL, stop/continue, and nice-value actions where the remote platform permits them.
- Added service discovery and supported lifecycle operations for systemd, launchd, BSD services, and Windows services.
- Added Docker container status, metadata, ports, start, stop, restart, and log actions.
- Added host logs with presets, snapshots, and follow mode.
- Added tmux session, window, and pane discovery with create, attach, rename, close, and send-command operations.
- Added listening-port inspection with process association and public-exposure hints.
- Added filesystem and mount views with capacity, usage, read-only state, and low-space indicators.
- Added scheduled-task discovery and supported run, enable, disable, and log operations across Linux, macOS, and Windows.
- Added remote package-manager discovery, package lists, status, and package detail views.
- Isolated samplers and parsers by capability so one unsupported tool does not mark the SSH node itself as disconnected.
- Added responsive and virtualized tables that prioritize entity names at narrow sidebar widths.
- Host Tools degrade to partial or unavailable states when the remote operating system, command-line utilities, or privileges do not provide a capability.

### Remote Desktop and Graphics

- Added built-in RDP and VNC workspace sessions with separate helper-process boundaries.
- Added keyboard, mouse, clipboard, scaling, reconnect, and viewport-aware rendering paths.
- Added dynamic remote resolution handling for supported RDP sessions.
- Added VNC decoding for Raw, CopyRect, Hextile, and ZRLE server updates.
- Added a Windows WSL Graphics connection flow for discovering and opening WSLg graphical sessions.
- Added workspace graphics surfaces for terminal-owned images and supported remote graphical workflows.

### OxideSens AI

- Moved OxideSens into the Rust workspace with access to user-selected terminal, file, connection, and workspace context.
- Kept the BYOK model with OpenAI, Anthropic, Gemini, DeepSeek, Ollama, and custom OpenAI-compatible providers.
- Added a unified workspace tool layer for target selection, terminal observation, command execution, file operations, transfers, navigation, and preference changes.
- Added risk classes for read, write, execute, interactive, destructive, and credential-related operations.
- Added command-policy detection for destructive filesystem operations, formatting, reboot, privilege escalation, container deletion, and Kubernetes resource deletion.
- Read-only tools may execute directly; other actions follow the user's configured approval and safety policy.
- Added streaming output, conversation persistence, message branching, follow-up suggestions, tool-result compaction, and context-window budgeting.
- Added ACP agent integration with configurable external processes and presets for supported coding-agent CLIs.
- Added provider-bound context redaction for common private keys, authorization headers, database URLs, tokens, and credential-like values as a defense-in-depth measure.

### MCP and Knowledge

- Added native MCP transports for local stdio, Streamable HTTP, and Legacy SSE servers.
- Added MCP tool discovery and invocation, resource listing and reading, authentication headers, custom headers, environment variables, retry, and runtime status.
- Redacted MCP authentication values, headers, and environment secrets from ordinary debug output.
- Added document collections, document editing, scopes, index rebuilds, and knowledge search.
- Added BM25 and persistent HNSW vector retrieval with Reciprocal Rank Fusion and duplicate-reduction ranking.
- Added BM25 fallback when no embedding provider is available.
- Added character-bigram tokenization for Chinese, Japanese, and Korean retrieval.

### Plugins

- Introduced the 2.0 plugin model with manifest-only, Wasm, and external process runtimes.
- Bundled the Wasm executor in standard desktop packages.
- Added manifest contributions for custom tabs, sidebars, settings, terminal hooks, connection hooks, AI tools, and scoped host API access.
- Added host-rendered declarative plugin UI instead of loading plugin React, CSS, or WebView pages.
- Added capability and namespace checks for terminal, SFTP, forwarding, IDE, settings, sync, application state, and plugin-secret APIs.
- Added plugin discovery, install, enable, disable, update, settings, compatibility, health, and runtime status flows.
- Added protocol, guest ABI, WASI profile, host channel, host version, platform target, and checksum compatibility checks for Wasm runtimes.
- Added stable, validated keychain account identifiers for plugin secrets.
- Legacy Tauri/Web plugins can be discovered for information or removal, but their JavaScript entry points do not execute in 2.0.

### Oxide Cloud Sync

- Moved cloud sync from an optional 1.x plugin into the built-in 2.0 workspace.
- Added WebDAV, HTTP JSON, Dropbox, OneDrive, Google Drive, GitHub Gist, S3, and Git backends.
- Added manual upload, remote inspection, pull preview, conflict handling, automatic upload, history, and rollback backups.
- Added independent sync scopes for connections, forwards, Quick Commands, serial profiles, application settings, and plugin settings.
- Sensitive credentials and local-terminal environment variables remain excluded by default and require explicit opt-in.
- Added partition revisions, baselines, and tombstones for incremental updates and deletion tracking.
- Added pre-apply checkpoints and best-effort whole-operation rollback when connection, setting, forwarding, or plugin-setting writes fail.
- Added local rollback retention and bounded sync history.
- Fixed managed SSH keys blocking GitHub Gist upload preflight.
- Tightened upload, pull, conflict, tombstone, delivery-state, and rollback transitions.

### Encrypted `.oxide` Bundles and Portable Workflows

- Expanded and rebuilt `.oxide` import/export for connections, forwarding rules, application settings, Quick Commands, serial profiles, plugin settings, managed SSH keys, and optional portable secrets.
- Added content preview and per-resource selection before import.
- Added rename, skip, replace, and merge conflict policies, with rename as the conservative default.
- Added managed-key fingerprint reuse and explicit choices for restoring managed keys and passphrases.
- Kept saved server passwords, portable secrets, and managed-key passphrases out of ordinary exports unless the user explicitly includes them.
- Added validation and storage checkpoints before applying imports, with rollback when later stages fail.
- Continued ChaCha20-Poly1305 payload encryption and added the current Argon2id KDF profile while retaining support for older `.oxide` KDF files.
- Added portable profile locking, status, keystore, and recovery workflows.

### Standalone `oxideterm` CLI

- Replaced the 1.x `oxt` desktop JSON-RPC client with the standalone `oxideterm` command.
- The new CLI links directly to Rust domain modules and does not require the desktop app to be running.
- Added commands for settings, connections, temporary SSH, forwarding, Quick Commands, plugins, portable profiles, secrets, `.oxide`, cloud sync, paths, diagnostics, doctor checks, backups, batches, reports, completion, and error lookup.
- Added structured JSON output and machine-readable error codes for scripts and CI.
- Added dry-run plans and `--yes` guards to state-changing and high-impact operations where supported.
- Added redacted diagnostic reports and support bundles.
- Added shell completion for Bash, Zsh, Fish, PowerShell, and Elvish.

### Security and Secret Handling

- Unified passwords, key passphrases, AI keys, cloud credentials, plugin secrets, and portable secrets behind the OS keychain or portable keystore boundaries.
- Added encrypted local storage for saved connection metadata, with the local encryption key protected by the platform credential store.
- Added `SecretString`, `Zeroizing`, and redacted `Debug` handling across major secret-owning Rust boundaries.
- Kept credential values out of connection diagnostics, plugin status, cloud-sync summaries, CLI reports, and structured logs.
- Added standard-input and environment-based CLI secret input so scripts do not need to place secrets in process arguments.
- Continued strict SSH host-key verification with rejection of unexpected key changes.
- Added risk-aware AI tools and command-policy checks while keeping approval behavior user-configurable.

### Appearance, Settings, and Internationalization

- Rebuilt settings with native controls, categorized navigation, virtualized long pages, validation, and search-oriented organization.
- Added custom theme editing, interface fonts, terminal fonts, separate CJK font selection, application icon selection, and terminal highlight rules.
- Added background image libraries, opacity, blur, fit modes, per-surface selection, and content-only or full-window background scope.
- Added platform visual-material settings where supported.
- Added configurable shortcuts and native keybinding recording.
- Added update proxy, SSH proxy, SFTP, reconnect, terminal, AI, plugin, cloud-sync, and privacy-oriented settings surfaces.
- Added 11 shipped interface languages across the major 2.0 workflows.
- Added reduced and disabled animation modes; disabling motion removes transition delays instead of scheduling zero-duration exits.

### Packaging, Installation, and Updates

- Added six release targets: macOS arm64/x64, Windows arm64/x64, and Linux arm64/x64.
- Added macOS DMG, app archive, and portable archive outputs.
- Added Windows NSIS installers and portable ZIP outputs.
- Added Linux AppImage, DEB, RPM, and portable archive outputs.
- Added signed updater metadata and SHA-256 release checksums.
- Added Windows installer options for Start Menu and optional desktop shortcuts.
- Added a dedicated Windows update helper that stages the installer, waits for OxideTerm to exit, uses Restart Manager on a best-effort basis, keeps an `old` rollback directory, and completes replacement outside the running app.
- Added no-window process creation for Windows shell discovery, Git helpers, PowerShell, updater helpers, and other background commands.
- Added stable, beta, and GPUI Preview update-channel boundaries.
- Stable 2.0 updates use the GitHub Latest manifest; the old Preview-facing stable manifest remains frozen to prevent unintended cross-channel replacement.

### Important Fixes Since the GPUI Previews

- Restored Ubuntu MOTD, login banners, and last-login output during SSH shell integration startup.
- Fixed initial remote directory metadata reporting `~` instead of the actual directory.
- Fixed remote directory awareness becoming unavailable after the next prompt.
- Fixed remote Git and project detection ordering and bounded slow status scans.
- Fixed Host Tools showing counts while rows remained empty.
- Fixed Host Tools becoming impossible to resize after monitoring or process data arrived.
- Fixed tab and Host Tools scrollbars that were visible but could not be dragged.
- Fixed Windows background shell and Git discovery repeatedly opening console windows.
- Fixed Windows auto-update uninstall/reinstall sequencing with the dedicated update helper.
- Fixed Windows system-tray interaction after minimizing the app.
- Fixed saved connections failing to switch from key or Agent authentication to a newly entered password.
- Fixed keychain-backed passwords being read before the user explicitly requested to reveal them.
- Fixed SFTP path selection, Windows home-directory handling, remote extraction, transfer short reads, and several progress-state stalls.
- Fixed terminal IME focus, text-selection drag, Windows editing shortcuts, duplicate paste handling, pane-close cleanup, and duplicate reconnect output.
- Fixed remote desktop frame updates, sizing, clipboard, and reconnect edge cases.
- Fixed plugin runtime packaging so standard builds include Wasm execution again.
- Fixed cloud-sync managed-key preflight, conflict accounting, tombstone handling, and rollback state.
- Fixed multiple modal, toolbar, dropdown, toast, sidebar, and narrow-window interaction regressions across the GPUI workspace.

### Breaking Changes

- **CLI:** `oxt`, its JSON-RPC protocol, Unix socket, Windows named pipe, and old command syntax are not compatible with 2.0. Automation must move to the standalone `oxideterm` subcommands.
- **CLI scope:** the new CLI focuses on configuration, automation, diagnostics, migration, and recovery; not every live desktop-session RPC operation from `oxt` has a direct replacement.
- **Plugins:** Tauri/Web plugins that depend on `main.js`, React components, CSS injection, WebView APIs, or arbitrary Tauri commands do not execute in 2.0.
- **Plugin migration:** plugins must move to the 2.0 manifest, declarative UI, Wasm, or process protocol and request explicit host capabilities.
- **Cloud sync:** cloud sync is now built in; the old cloud-sync plugin is no longer the feature owner.
- **Preview updates:** GPUI Preview builds do not update directly to Stable 2.0 through the Stable channel.

### Upgrading to 2.0

#### Before You Upgrade

- Close active terminals, transfers, port forwards, Host Tools actions, and remote desktop sessions before installing the update. Active runtimes and in-progress operations do not survive the required application restart.
- Keep a current backup or encrypted export of important connections and settings.
- On first launch, OxideTerm creates a one-time snapshot of the existing data directory before loading and migrating mutable settings and connection data.
- The migration snapshot is a recovery copy, not an automatic rollback mechanism.
- OxideTerm continues to use the existing default data directory and honors a custom data directory selected through `bootstrap.json`.

#### From OxideTerm 1.x Stable

- Installed macOS releases can use the Stable update after 2.0 is promoted, using a compatibility archive understood by the 1.x updater.
- Current-user Windows installations can use the Stable update; the 2.0 installer detects the existing per-user installation and upgrades it in place.
- Linux AppImage installations can use the application update path, which replaces the AppImage after OxideTerm exits.
- Linux DEB or RPM users should install the matching 2.0 package manually. OxideTerm 2.0 publishes both package formats alongside AppImage and portable archives.
- Portable installations do not update themselves. Extract the 2.0 portable package separately and preserve the existing portable data directory before replacing files.
- If a GPUI Preview is installed alongside 1.x, verify that the 1.x stable application is the one performing an automatic Stable upgrade.
- Existing connection and settings data is migrated into the 2.0 storage model where supported, but review connections, authentication, cloud sync, plugins, and AI provider settings after first launch.
- Legacy keychain entries may be migrated to protected 2.0 entries on first read; the operating system may display a one-time credential access prompt.
- Existing keychain passwords remain unloaded in edit forms until the user explicitly reveals them.
- Strict host-key verification remains active after upgrade.
- Third-party connection imports may report fields that cannot be represented exactly; review the import preview before applying it.
- Data written by 2.0 is not guaranteed to remain fully understandable to a subsequently launched 1.x application.

#### From a GPUI Preview

- GPUI Preview cannot update directly to Stable 2.0 through the Stable update channel.
- Install the final 2.0 package manually from the stable GitHub Release, or use an installed 1.x stable build as the supported automatic-upgrade origin.
- Older Preview builds use a frozen `updater-stable` manifest and will not receive 2.0 Stable in place.
- Preview and Stable use separate application identities and can remain installed side by side while Stable is verified, but they share the same OxideTerm data directory.
- After confirming that Stable opens the existing data correctly, the GPUI Preview application can be removed.

### Compatibility and Known Boundaries

- Release packages are provided for macOS, Windows, and Linux on x64 and arm64.
- The optional remote project agent is packaged only for Linux x86_64 and Linux aarch64. OxideTerm asks before deployment; other remote architectures require a separately built agent or SFTP-compatible fallback behavior.
- Host Tools depend on the remote operating system, installed command-line tools, service manager, and privileges. Unsupported capabilities are shown as partial or unavailable.
- Grace Period reconnect is best-effort and depends on the original SSH runtime recovering or supported consumers successfully acquiring a replacement transport; it is not a guarantee of lossless recovery for every network failure.
- Unsaved editor buffers still require an explicit save, reload, or discard decision and should not be treated as remotely persisted state.
- Port-forward restore can fail when local ports are occupied, privileges are insufficient, or the remote server rejects binding.
- RDP and VNC support common interactive workflows but do not claim parity with every platform-specific enterprise client or extension.
- VNC uses the server framebuffer and scales it into the local viewport; dynamic remote resolution behavior primarily applies to supported RDP sessions.
- File preview supports selected text, code, Markdown, image, audio, video, hexadecimal, and font formats. PDF preview is not included in 2.0.
- Configurable keyboard shortcuts are included; vi-mode is not a 2.0 feature.
- Shell directory, Git, and project awareness is available through supported shell integration or explicit probing. Restricted shells, unusual startup files, or custom environments may reduce available metadata.
- Wasm plugins run inside Wasmtime; external process plugins are separate executables and should not be described as OS-sandboxed.
- Sensitive cloud-sync sections are disabled by default but can be explicitly included by the user in the encrypted sync payload.
- `.oxide` payload content is encrypted, but non-secret file metadata used to identify and describe a bundle is not a secret-storage boundary.
- OxideSens context redaction is defense in depth, not a formal guarantee that arbitrary user content can never contain an undiscovered secret pattern.
- AI conversation persistence is application-wide in 2.0; it is not guaranteed to remain semantically bound to the lifetime of an individual tab or workspace surface.
