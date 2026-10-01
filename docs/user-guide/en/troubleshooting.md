# Troubleshooting

Start troubleshooting from the desktop app. The visible app state usually tells you whether the problem is a saved profile, a live SSH node, a terminal session, Host Tools, graphics/VNC, SFTP, forwarding, sync, settings, or a plugin.

## First Checks In The App

Check the relevant surface before editing files or running repair commands:

- Sessions: confirm the saved connection exists and has the expected host, user, port, group, and authentication mode.
- Connection Monitor: check whether the node is connected, connecting, stale, reconnecting, or unavailable.
- Host Tools: check whether resource snapshots are fresh and whether an action failed with a visible error.
- Terminal tab: confirm the shell accepts input, whether a command is still running, and whether a terminal helper prompt is active.
- Graphics/VNC: confirm the saved profile/provider or owning node is live and the viewer is connected.
- SFTP or File Manager: confirm the target node is live before retrying directory reads or transfers.
- Settings: check recent changes to terminal background images, privilege credentials, SSH, AI, cloud sync, plugin, or update settings.
- Notifications: review recent warnings and errors.

If a connection or surface is stale, try reconnecting from the app before changing configuration.

## Common Recovery Steps

For settings issues, reopen Settings and check the section that was changed most recently. If the app reports invalid settings, revert the smallest visible change first.

For connection issues, edit the saved connection and retry it from Sessions. Avoid creating duplicates until you know the original profile is wrong.

For SFTP or forwarding issues, check the owning SSH node in Connection Monitor. Retry after the node is live.

For Host Tools issues, refresh the tool page first. If the sampler or action still fails, reconnect the owning node and retry the smallest action. Avoid using Host Tools for hidden cleanup or recursive disk scans.

For graphics/VNC issues, check the saved profile/provider or owning node, reconnect the viewer, then restart the helper or graphics session if its backing process stopped. Viewer state is separate from terminal output and saved connection data.

For terminal background issues, reopen Settings and confirm the background image is enabled for the current tab type. Native currently treats the background as a selected image slot; adding a new image replaces the current selection.

For stale blocks after a full-screen TUI exits, first try `clear` or reopen the terminal pane. If the issue repeats with a command such as `yazi`, treat it as terminal graphics/image-placement state and include the command name in the bug report.

For terminal file transfers, follow the matching SSH command or serial **Binary transfer** action in [terminal transfers](desktop.md#transfer-files-through-a-terminal). Cancel an unexpected transfer prompt before trying again.

For privilege credential issues, check the dedicated Settings page and the active terminal pane. Do not paste sudo/su passwords into logs, AI prompts, support bundles, quick commands, or connection notes while debugging.

For cloud sync issues, inspect the current preview and follow [conflicts and interrupted sync](cloud-sync-and-backups.md#conflicts-and-interrupted-sync).

For serial terminal issues, start from the device and permission boundary:

- No ports listed: enter `/dev/cu.*`, `/dev/ttyUSB*`, `/dev/ttyACM*`, or `COMx` manually and confirm the OS can see the device.
- Permission denied: on Linux, check `dialout`, `uucp`, or the distribution-specific serial group and log out/in after changing membership. On macOS, check system permissions and USB serial drivers.
- Device busy: close other terminal programs, debuggers, flashing tools, or OxideTerm tabs that may already hold the port.
- Device unplugged: close the current serial terminal, reconnect the device, and reopen it. If the OS assigned a new path, update the serial profile.

## Connection and transfer checks

| Symptom | What to check |
| --- | --- |
| SSH connection times out before authentication | Check the saved host and port, network/VPN access, and each jump host shown in connection progress |
| Password or key authentication is rejected | Confirm the username and selected authentication method; make sure the key file or managed key is available on this device |
| Host-key confirmation or a changed-key warning appears | Compare the fingerprint with a trusted record or the server administrator before accepting it |
| Kerberos has no usable credentials | Renew the operating system's sign-in or ticket, then check the configured server identity and fallback method |
| A TOTP code is not filled in or is rejected | Check the binding for that specific host, enabled state, prompt expression, device clock, and code parameters; see [TOTP setup](connections-and-forwards.md#automatic-verification-codes-totp) |
| Mosh starts through SSH but shows no usable terminal | Check direct UDP access, the port/range and host override, and firewall or address translation rules |
| SSH works but SFTP fails | Confirm that the server permits SFTP channels and that the selected directory is accessible; inspect the SFTP error before reconnecting |
| A transfer stops or cannot write its destination | Check free space and permissions at the receiving end, the transfer queue's error, and whether the connection has recovered |
| A download gets a new filename | The default keeps the existing file; use the sender's explicit overwrite request only when replacement is intended |
| A remote edit reports a file conflict | Compare your buffer with the current remote file, then choose whether to reload or overwrite; keep a copy of edits you still need |

If output stops during an SSH network interruption, watch the reconnect phase. OxideTerm first probes the existing connection before replacing it. Reopening or deleting the profile is not required for this recovery. Check transfer and editor status after reconnection before repeating an operation that may already have completed.

If no audit recording appears, check both **Enable audit** and **Record terminal output**. They are initially off, so there is no earlier output to replay. Missing or expired-content indicators are explained in [session recordings](desktop.md#audit-and-session-recordings).

## Backups First

Before applying a restore, import, sync apply, or manual file repair, [create or verify a backup](cloud-sync-and-backups.md#create-a-backup). Review the restore plan and apply the smallest section that solves the issue.

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
