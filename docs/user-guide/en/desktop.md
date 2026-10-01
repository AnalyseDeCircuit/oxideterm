# Desktop Workflows

## First Launch

Open OxideTerm, then check the left activity bar for the main work areas: sessions, connection pool, connection monitor, Host Tools, graphics/VNC, plugins, cloud sync, file manager, notifications, and settings.

If the app starts with no sessions, create a local shell tab first. This verifies the terminal renderer, shell integration, input handling, and theme settings before you add remote hosts.

## Activity Bar

Use the activity bar as the entry point for app surfaces:

- Sessions: create, open, group, and monitor SSH work.
- File manager and SFTP: browse files and manage transfers.
- Connection pool, monitor, and Host Tools: inspect connection runtime state, resource snapshots, processes, containers, services, tmux, logs, ports, and metrics.
- Graphics/VNC: open saved RDP/VNC profiles or visual sessions launched from a connected node.
- Plugins: manage installed plugins and plugin settings.
- Cloud sync: inspect sync status and run sync actions.
- Notifications & Audit: review notifications, recorded operations, session history, and terminal-output recordings.
- Settings: change app behavior and provider configuration.

When a workflow becomes confusing, return to Sessions or Connection Monitor first. Those views show whether a host is saved, connecting, connected, stale, or unavailable.

## Terminal Panes

Use terminal tabs for local shells and SSH sessions. Split panes when a task needs multiple shells in the same workspace. Command marks, shell integration, and terminal history belong to the pane, so closing a pane should not be treated as disconnecting a saved SSH host.

### Combine pages in one tab

For example, keep a terminal beside SFTP while uploading files:

1. Open the terminal and SFTP as separate tabs.
2. Drag the SFTP tab into the terminal's content area. Move to the left, right, top, or bottom edge and release over the layout preview.
3. Drag the divider to adjust the space given to each page. Click inside a pane to make it active.
4. To separate a page again, use **Move to new tab** in its pane header, or the header's action for moving it to a new window.
5. To remove a page, use its pane close action. If it has unsaved work or a running process, handle the close prompt. When one page remains, the extra pane header disappears and the main tab takes that page's name.

A combined tab holds up to four panes. Terminals, SFTP, IDE, port forwards, notes, settings, cloud sync, audit, remote desktops, and plugin pages can share a layout. Opening a single-instance page, such as Settings, again focuses its existing pane. Moving the whole combined tab to another window preserves its live sessions and layout.

For long-running jobs, keep the owning connection visible in the connection pool or monitor. Reconnect behavior is tied to the connection/runtime state, not only to the visible terminal tab.

Common pane patterns:

- One tab per task when tasks are unrelated.
- Split panes for commands that should be compared side by side.
- Keep one monitoring pane open for logs while another pane performs edits or deploy steps.
- Close only the pane or tab you no longer need; keep the saved connection profile intact.

Use the terminal context menu and command bar for explicit pane actions such as copy, paste, search, command selection, and terminal-native file transfers. Background images and terminal image placements are visual/rendering state; if a full-screen TUI leaves stale image content behind, clear or reopen the pane rather than editing saved connection data.

### Free Type Mode

Enable Settings → Terminal → Free Type Mode to edit the active ordinary shell command with the mouse:

- Click inside the active command to move the remote line-editor cursor.
- Select command text and press Backspace/Delete, type, paste, or use Copy/Cut to edit it.
- Double-click inside matching `()`, `[]`, or `{}` pairs to select their innermost contents.
- Drag selected command text to move it; hold Ctrl while dragging to copy it instead.
- Drag selected single-line history output to insert a copy at the target command position.
- Alt-drag selected text to replace the current command.

Toggle the mode with Command+Shift+F on macOS or Ctrl+Alt+F on Windows and Linux. The action is also available in the command palette and can be remapped under Settings → Keyboard Shortcuts.

While the mode owns an ordinary command input, Command+C/X/V on macOS and Ctrl+C/X/V on Windows and Linux perform editor-style copy, cut, and paste. The configurable terminal actions also default to Ctrl+Shift+C/X/V on Windows and Linux. Outside a verified command input or selection, these keys keep their existing terminal behavior.

OxideTerm sends ordinary terminal key and text sequences; the remote shell or editor remains the source of truth. Full-screen and mouse-tracking programs keep their own pointer input. Vim, Neovim, and Emacs can additionally expose their current mode and selection through OxideTerm's explicit adapter, allowing the same Copy/Cut/Paste shortcuts to operate on a verified editor selection without weakening the alternate-screen or mouse protections.

OxideTerm does not alter editor startup files. To opt in for Vim or Neovim, add this to `vimrc` or `init.vim`:

```vim
if exists('$OXIDETERM_VIM_INTEGRATION') && filereadable($OXIDETERM_VIM_INTEGRATION)
  execute 'source ' . fnameescape($OXIDETERM_VIM_INTEGRATION)
endif
```

For a Neovim `init.lua`, use:

```lua
local adapter = vim.env.OXIDETERM_VIM_INTEGRATION
if adapter and vim.fn.filereadable(adapter) == 1 then
  vim.cmd("source " .. vim.fn.fnameescape(adapter))
end
```

For Emacs, add this to the init file:

```elisp
(when-let ((adapter (getenv "OXIDETERM_EMACS_INTEGRATION")))
  (when (file-readable-p adapter)
    (load adapter nil t)
    (oxideterm-free-type-mode 1)))
```

Local terminal sessions provide these adapter paths automatically. For SSH sessions, first install or repair Remote Shell Integration under Settings → Terminal → Awareness & Integration; it installs the readable adapter files under `~/.oxideterm/shell-integration`. Their paths are exported only when the SSH server accepts OxideTerm's per-channel capability marker. Restrictive servers still retain standard OSC 7 directory awareness but disable the private editor enhancement. Loaded adapters suppress private messages inside tmux, GNU screen, and Zellij because a shared pane cannot isolate them by attached client. An active adapter reports only editor identity, mode, selection shape, capabilities, and a user-requested copied/cut selection. Clipboard responses are bounded and ignored unless they match a recent user shortcut.

### Backspace and Delete compatibility

Settings → Terminal also lets you choose the sequences sent by the physical Backspace and Delete keys. The defaults are `DEL (0x7F)` for Backspace and `CSI 3~` for Delete. Change them only when a legacy shell, serial device, or remote application expects `Ctrl+H (0x08)` or another offered sequence. Kitty keyboard protocol sessions keep their protocol-defined key encoding.

### Edit before pasting

1. Copy the text, then right-click the destination terminal and choose **Edit before pasting**. A multiline paste confirmation also offers **Edit**.
2. Edit the text in the dialog. You can add or remove lines and use **Undo** and **Redo**.
3. If you copied a Markdown code block, choose **Remove code block markers** to remove its surrounding fence. This is an explicit action; ordinary paste keeps the text as copied.
4. Review the destination and the final text, then click **Paste**. **Cancel** closes the dialog without sending the draft.

Newlines are sent with the pasted text and may cause the receiving shell to execute commands. The dialog keeps edits locally until you choose Paste.

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
- Host Tools snapshot: what did the last resource sampler observe?
- Graphics/VNC session: is the viewer connected to its saved profile/helper or live node-owned session?

Use Host Tools for read-oriented host inspection. Keep destructive host actions explicit and review app confirmation output before running them.

## File Manager and SFTP

Use the file manager for remote browsing, uploads, downloads, previews, and basic file operations. Treat remote edits as real remote writes: keep backups for critical files, and verify paths before overwriting.

When a connection is unstable, pause large transfers and reconnect before retrying. Saved connection state and transfer state are separate; a failed transfer should not require deleting the connection.

### Transfer files through a terminal

In an SSH terminal, start the matching transfer program on the remote host. OxideTerm recognizes its transfer request and opens the local file or destination picker.

| Task | Remote command | Local action |
| --- | --- | --- |
| Upload with trzsz | `trz` | Choose local files |
| Download with trzsz | `tsz report.txt` | Choose a destination folder |
| Upload with ZMODEM | `rz` | Choose local files |
| Download with ZMODEM | `sz report.txt` | Choose a destination folder |

The remote host needs the corresponding program installed. For a download that should replace an existing local file, run `tsz -y report.txt` or `sz -y report.txt`. Without that overwrite request, a conflicting filename is renamed. OxideTerm keeps the existing file while receiving its replacement: trzsz commits after validation, and ZMODEM commits replacements after the batch succeeds. Cancelling the transfer preserves the existing file.

For serial devices, open **Binary transfer** in the terminal control bar and choose the matching XMODEM, YMODEM, or ZMODEM upload/receive action. Put the device into its matching transfer mode first. XMODEM reception also needs a destination filename because the protocol does not carry one.

Use SFTP when you want to browse directories and manage a transfer queue. See [transfer troubleshooting](troubleshooting.md#connection-and-transfer-checks) if a terminal transfer does not start.

## Audit and session recordings

Open **Notifications & Audit** from the activity bar. Notifications show recent messages; **Audit**, **Sessions**, and **Session recordings** provide operation history and playback.

### Record and find operations

1. Select **Audit** and turn on **Enable audit**. It is off by default and records subsequent operations.
2. Perform the connection, command, file, or forwarding operation you want to inspect, then return and click **Refresh**.
3. Choose a time range and other filters. Enter a search and press Enter to apply it.
4. Open a record to inspect its target, source, result, and result evidence. **Sent** means the input was sent; an exit code or protocol result provides stronger evidence of completion.
5. Use **Sessions** to find related operations for one session, then open that session's history or recordings.

Disabling audit stops new collection; existing records remain readable. Retention and capacity can be changed in the page's settings and applied with **Save**.

### Record and replay terminal output

1. With audit enabled, select **Session recordings** and turn on **Record terminal output**.
2. Read and accept the recording prompt. Future displayed output from local terminals, SSH, Telnet, Mosh, and serial sessions is recorded.
3. Return to Session recordings, refresh the list, and open a recording. Use play/pause, the position control, playback speed, or restart to inspect it.
4. Turn off Record terminal output when you no longer want new output captured. Disabling audit also stops new output recording.

Recordings contain displayed output and terminal size changes. Typed keys and terminal file-transfer payloads are excluded, but text echoed by a shell or program can still appear in the output. Recordings are encrypted on this device and are not included in cloud sync. An incomplete or expired-content indicator means some output is unavailable for playback.

| Data | Default retention | Default capacity |
| --- | --- | --- |
| Operation records | 90 days | 512 MiB |
| Terminal output | 7 days | 2 GiB |

Both switches are off initially. Each retention setting accepts 1–3650 days and each capacity accepts 1–65536 MiB. Save changes in the corresponding Audit or Session recordings view.

### Export records

Use **Export summaries as JSON** or **Export summaries as CSV** for the selected audit filters. **Export protected details** asks for confirmation and includes commands, paths, endpoints, accounts, and operation details in a plaintext file; it does not include session recordings. Review that file before sharing it. Export records you need to keep before clearing them or reducing retention.

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

For context-sensitive helpers such as privilege prompts or modem transfers, make the intended terminal pane active first. The helper should act on the active pane/session rather than on a prompt string, tab title, or saved host label.

## Updates

Use Settings → Help & About to check the active version and update channel. Stable and beta builds use separate update channels, so choose the channel that matches the build you installed.
