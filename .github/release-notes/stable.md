# OxideTerm 2.0

OxideTerm 2.0 is the stable, GPU-rendered desktop workspace for SSH, terminals, files, forwarding, host operations, remote desktop, and OxideSens AI.

Use this channel for daily work. It includes the desktop app, signed update metadata, and the recommended update path for most users.

<!-- RELEASE_DOWNLOADS -->

## What This Release Is

- The stable OxideTerm 2.0 desktop build.
- Intended for daily SSH, SFTP, terminal, port forwarding, remote desktop, serial, file, and OxideSens workflows.
- Published with updater metadata for the stable channel.
- Suitable for users who do not want preview-channel churn.

<!-- RELEASE_CHANGELOG -->

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

# Fedora/RHEL-compatible systems
sudo dnf install ./OxideTerm_*_linux_*.rpm
```

</details>

## Links

- Documentation: https://oxideterm.app
- GitHub Issues: https://github.com/AnalyseDeCircuit/oxideterm/issues
- Changelog: https://github.com/AnalyseDeCircuit/oxideterm/blob/main/.github/release-notes/stable-changelog.md
