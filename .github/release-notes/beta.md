# OxideTerm Native Beta

This is a beta OxideTerm native desktop release.

Use this channel when you want to test changes before they reach the stable channel. Beta builds may include packaging, updater, compatibility, and workflow changes that still need wider validation.

## What This Beta Is

- A pre-stable OxideTerm desktop build.
- Intended for users who can tolerate small regressions and report issues with enough detail to reproduce them.
- Published with updater metadata for the beta channel.
- Less stable than the stable channel and intended for pre-release validation.

<!-- RELEASE_CHANGELOG -->

## 插件市场兼容说明

插件市场 v1 已冻结，原地址、索引内容和对应安装包持续保留。旧客户端仍可使用冻结目录中的插件，但不会看到后续的新插件和更新；请升级到支持 v2 目录的主程序版本。后续插件、新版本和版本历史仅发布到 v2。

## Plugin marketplace compatibility

The v1 plugin marketplace is frozen. Its original URL, exact contents, and referenced packages remain available. Existing clients can keep using the frozen catalog but will not see subsequent plugins or updates; upgrade to a host version supporting v2 to receive them. All subsequent plugins, releases, and version histories are published only to v2.

## Beta Notes

- Keep a stable build installed if OxideTerm is business-critical for your daily work.
- Report regressions with OS, CPU architecture, installed asset name, and steps to reproduce.
- If a beta workflow behaves differently from the stable release, include both results in the issue.

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
