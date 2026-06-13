# OxideTerm Native GPUI Preview

This is a GPUI/native preview build for testing the next-generation OxideTerm desktop app.

If you just want the most stable daily-use OxideTerm release, do not download this preview yet. Use the current Tauri/WebView release instead. This preview exists so users can try the native UI early and report parity, packaging, terminal, SFTP, port forwarding, SSH, and AI workflow issues before it becomes the default app.

## What This Preview Is

- A native GPUI desktop build of OxideTerm.
- A preview channel, not the recommended stable channel.
- Built from the same product direction as OxideTerm: local-first remote server work over SSH, with terminal, SFTP, port forwarding, connection management, file workflows, and OxideSens AI context.
- Intended for users who are comfortable testing early builds and filing detailed issues.

## GPUI Preview 7 Highlights

This preview is mostly a cloud sync release.

- Cloud sync now supports more backends, including GitHub Gist, OneDrive, and Google Drive alongside the existing sync targets.
- The cloud sync page has been reorganized into clearer sections for setup, scope, preflight, preview, rollback, and history.
- Sync scope is more complete: saved connections, saved forwards, quick commands, serial profiles, app settings sections, plugin settings, and opt-in sensitive credentials can be controlled more explicitly.
- Preflight and preview flows now show richer local/remote impact, conflict context, field-level changes, and item/section selection before applying or uploading.
- Sensitive credential sync is opt-in and reports restored/skipped credential categories without exposing secret values.
- Cloud sync conflict handling is stronger, including per-section/per-item choices and three-way field merging for supported data.
- New OAuth-backed provider flows were added for GitHub Gist, Microsoft OneDrive, and Google Drive, with more specific setup and API error messages.
- Offline `.oxide` export/import now includes saved serial connection profiles so direct migration stays aligned with cloud sync coverage.
- SSH node disconnects now show a confirmation dialog in the GPUI build.

## What To Test

- SSH connection creation, saved connections, reconnect, known-host prompts, and jump/proxy routes.
- Terminal tabs, split panes, command bar, quick commands, broadcast input, recording, and local shell behavior.
- SFTP browsing, transfers, previews, and remote file workflows.
- Local, remote, and dynamic port forwarding.
- Cloud sync setup, scope selection, upload/pull preview, conflict handling, rollback backups, and provider-specific OAuth flows.
- OxideSens AI sidebar, model/provider setup, context capture, tool calls, approval behavior, and credential redaction.
- Settings, i18n, theme rendering, keyboard shortcuts, and visual parity with the Tauri version.

## Known Preview Caveats

- This build may still have visual parity issues compared with the Tauri version.
- Some workflows may be incomplete or rough around edge cases.
- macOS builds are not notarized unless explicitly stated in a later release.
- Windows SmartScreen and macOS Gatekeeper may warn because this is an unsigned/early preview build.
- If a workflow is business-critical, keep the stable Tauri release installed as your fallback.

<details>
<summary>Installation Tips / 安装提示</summary>

### macOS

Downloaded `.dmg` files may be quarantined by Gatekeeper. Run in Terminal:

```bash
xattr -cr ~/Downloads/OxideTerm_*.dmg
# or after install / 或安装后
xattr -cr /Applications/OxideTerm.app
```

### Windows

If SmartScreen warns, click **More info** -> **Run anyway**.

若 SmartScreen 弹出警告，点击 **更多信息** -> **仍要运行**。

### Linux

```bash
# AppImage
chmod +x OxideTerm_*_linux_*.AppImage && ./OxideTerm_*_linux_*.AppImage

# Debian/Ubuntu
sudo dpkg -i OxideTerm_*_linux_*.deb && sudo apt-get install -f
```

</details>

## Reporting Issues

Please include:

- OS and CPU architecture.
- The exact asset you installed.
- Whether this is a fresh install or an update from another OxideTerm build.
- Steps to reproduce.
- Screenshots or logs when useful.
- Whether the same workflow works in the Tauri version.

GitHub Issues: https://github.com/AnalyseDeCircuit/oxideterm/issues
