# OxideTerm Native GPUI Preview

This is a GPUI/native preview build for testing the next-generation OxideTerm desktop app.

If you just want the most stable daily-use OxideTerm release, do not download this preview yet. Use the current Tauri/WebView release instead. This preview exists so users can try the native UI early and report parity, packaging, terminal, SFTP, port forwarding, SSH, and AI workflow issues before it becomes the default app.

## What This Preview Is

- A native GPUI desktop build of OxideTerm.
- A preview channel, not the recommended stable channel.
- Built from the same product direction as OxideTerm: local-first remote server work over SSH, with terminal, SFTP, port forwarding, connection management, file workflows, and OxideSens AI context.
- Intended for users who are comfortable testing early builds and filing detailed issues.

## GPUI Preview 8 Highlights

This preview focuses on connection ownership, saved connection coverage, and release polish.

- SSH drill-down and jump workflows now keep saved presets in the session manager instead of adding ambiguous actions to the running session tree.
- Telnet profiles can now be saved from the connection form and managed through the session manager.
- Serial profile persistence remains aligned with the saved connection model so local serial terminals can round-trip through the same management surface.
- The README set was refreshed across localized documents to describe SSH, Telnet, SFTP, port forwarding, serial terminals, and the native workspace model more accurately.
- Security copy now scopes the SSH dependency claim to OpenSSL/libssh2 instead of overclaiming the full dependency tree.
- Localized README wording was tightened to avoid awkward English/local-language mixing and CJK spacing artifacts.
- Native plugin process-runtime tests were made less sensitive to scheduler load during full workspace test runs.

## What To Test

- SSH connection creation, saved connections, reconnect, known-host prompts, and jump/proxy routes.
- Telnet and serial connection creation, saved profiles, and session manager editing.
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
