# OxideTerm Native GPUI Preview

This is a GPUI/native preview build for testing the next-generation OxideTerm desktop app.

If you just want the most stable daily-use OxideTerm release, do not download this preview yet. Use the current Tauri/WebView release instead. This preview exists so users can try the native UI early and report parity, packaging, terminal, SFTP, port forwarding, SSH, and AI workflow issues before it becomes the default app.

## What This Preview Is

- A native GPUI desktop build of OxideTerm.
- A preview channel, not the recommended stable channel.
- Built from the same product direction as OxideTerm: local-first remote server work over SSH, with terminal, SFTP, port forwarding, connection management, file workflows, and OxideSens AI context.
- Intended for users who are comfortable testing early builds and filing detailed issues.

## GPUI Preview 12 Highlights

This preview focuses on native plugin runtime reliability, packaging cleanup, and tightening GPUI workflow polish before the 2.0 line becomes the default app.

- Wasm plugin execution is bundled again in standard GPUI preview packages so Wasm plugins work without a separate runtime download.
- The optional `oxideterm-wasm-runtime` sidecar path remains available for future lightweight or externally managed builds.
- The Wasm runtime compatibility model now checks the host update channel, host version, plugin protocol, Wasm guest ABI, WASI profile, platform target, and asset checksum for sidecar installs.
- Native plugin runtime ownership was narrowed so the in-process host API, bundled Wasm executor, and optional sidecar process bridge stay separate.
- Quick Commands, settings, and narrow-width GPUI forms received layout and overflow fixes across the native preview UI.
- SSH authentication selection was simplified into password, key, Agent, and two-factor groups while keeping existing saved connection formats unchanged.
- Session icons, legal notices, onboarding, and plugin manager surfaces received additional polish and localization coverage.
- Serial and VNC work continues in the native preview track; please report device-specific and remote-desktop edge cases with logs and screenshots where possible.

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
