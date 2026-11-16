Beta 版用于提前试用新功能和修复。安装时会替换现有的 OxideTerm；如果日常工作离不开它，建议继续使用稳定版。

Try the beta for early access to new features and fixes. Installing it replaces your current OxideTerm installation. Stick with the stable release if you rely on OxideTerm for critical work.

<!-- RELEASE_CHANGELOG -->

## 问题反馈 / Reporting issues

反馈问题时，请注明操作系统、处理器架构、安装包文件名和复现步骤。如果稳定版和 Beta 版的表现不同，请一并说明。

Include your OS, CPU architecture, installer filename, and steps to reproduce. If the stable and beta releases behave differently, describe both results.

<details>
<summary>Installation Tips / 安装提示</summary>

### macOS

如果 Gatekeeper 阻止打开下载的 `.dmg`，可在终端运行：

If Gatekeeper blocks the downloaded `.dmg`, run this in Terminal:

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

## 相关链接 / Links

- [文档 / Documentation](https://oxideterm.app)
- [问题反馈 / GitHub Issues](https://github.com/AnalyseDeCircuit/oxideterm/issues)
- [更新日志 / Changelog](https://github.com/AnalyseDeCircuit/oxideterm/tree/main/docs/changelog)
