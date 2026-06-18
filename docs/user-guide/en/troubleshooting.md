# Troubleshooting

Start troubleshooting from the desktop app. The visible app state usually tells you whether the problem is a saved profile, a live SSH node, a terminal session, Host Tools, graphics/VNC, SFTP, forwarding, sync, settings, or a plugin.

## First Checks In The App

Check the relevant surface before editing files or running repair commands:

- Sessions: confirm the saved connection exists and has the expected host, user, port, group, and authentication mode.
- Connection Monitor: check whether the node is connected, connecting, stale, reconnecting, or unavailable.
- Host Tools: check whether resource snapshots are fresh and whether an action failed with a visible error.
- Terminal tab: confirm the shell accepts input, whether a command is still running, and whether a terminal helper prompt is active.
- Graphics/VNC: confirm the owning node is live and the viewer is connected.
- SFTP or File Manager: confirm the target node is live before retrying directory reads or transfers.
- Settings: check recent changes to terminal background images, privilege credentials, SSH, AI, cloud sync, plugin, or update settings.
- Notifications: review recent warnings and errors.

If a connection or surface is stale, try reconnecting from the app before changing configuration.

## Common Recovery Steps

For settings issues, reopen Settings and check the section that was changed most recently. If the app reports invalid settings, revert the smallest visible change first.

For connection issues, edit the saved connection and retry it from Sessions. Avoid creating duplicates until you know the original profile is wrong.

For SFTP or forwarding issues, check the owning SSH node in Connection Monitor. Retry after the node is live.

For Host Tools issues, refresh the tool page first. If the sampler or action still fails, reconnect the owning node and retry the smallest action. Avoid using Host Tools for hidden cleanup or recursive disk scans.

For graphics/VNC issues, check the owning node, reconnect the viewer, then restart the graphics session if the backing process stopped. Viewer state is separate from terminal output and saved connection data.

For terminal background issues, reopen Settings and confirm the background image is enabled for the current tab type. Native currently treats the background as a selected image slot; adding a new image replaces the current selection.

For stale blocks after a full-screen TUI exits, first try `clear` or reopen the terminal pane. If the issue repeats with a command such as `yazi`, treat it as terminal graphics/image-placement state and include the command name in the bug report.

For X/Y/ZMODEM transfer issues, cancel unexpected prompts. Retry with an explicit transfer command such as `rz`, `sz`, `rx`, or `rb`, then choose the local path from the app prompt. Ordinary command output should not be treated as a transfer unless protocol context is clear.

For privilege credential issues, check the dedicated Settings page and the active terminal pane. Do not paste sudo/su passwords into logs, AI prompts, support bundles, quick commands, or connection notes while debugging.

For cloud sync issues, open Cloud Sync, inspect status, and review conflicts before choosing a direction.

For serial terminal issues, start from the device and permission boundary:

- No ports listed: enter `/dev/cu.*`, `/dev/ttyUSB*`, `/dev/ttyACM*`, or `COMx` manually and confirm the OS can see the device.
- Permission denied: on Linux, check `dialout`, `uucp`, or the distribution-specific serial group and log out/in after changing membership. On macOS, check system permissions and USB serial drivers.
- Device busy: close other terminal programs, debuggers, flashing tools, or OxideTerm tabs that may already hold the port.
- Device unplugged: close the current serial terminal, reconnect the device, and reopen it. If the OS assigned a new path, update the serial profile.

## Backups First

Before applying a restore, import, sync apply, or manual file repair, create or verify a backup from the app. Review the restore plan and apply the smallest section that solves the issue.

## CLI Companion Diagnostics

Use the CLI companion when you need read-only diagnostics, CI checks, or support bundles:

```sh
oxideterm paths --json
oxideterm diagnose --json
oxideterm doctor --strict
oxideterm report --json
```

`doctor --strict` treats warnings as failures, which is useful for CI or migration scripts.

For focused checks:

```sh
oxideterm settings validate --strict
oxideterm connections validate --strict
oxideterm cloud-sync status --json
```

## Bug Reports

For issue reports, attach a redacted bundle rather than raw config files:

```sh
oxideterm report --bundle ./oxideterm-report.json --json
```

Review the bundle before sharing. Remove private hostnames, usernames, paths, or project names if needed.
