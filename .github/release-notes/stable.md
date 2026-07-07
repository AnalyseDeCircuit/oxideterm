# OxideTerm Native Release

This is the stable OxideTerm native desktop release.

Use this channel for daily work when you want the default supported build. It includes the native desktop app, signed update metadata, and the recommended update path for most users.

## What This Release Is

- A stable OxideTerm desktop build.
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
```

</details>

## Links

- Documentation: https://oxideterm.app
- GitHub Issues: https://github.com/AnalyseDeCircuit/oxideterm/issues
- Changelog: https://github.com/AnalyseDeCircuit/oxideterm/tree/main/docs/changelog
