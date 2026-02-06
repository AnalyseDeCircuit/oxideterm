# OxideTerm v1.6.0 Release Notes

## 📋 What's Changed


### 🔒 安全升级

#### AI API Key 存储迁移至 OS Keychain

**问题**：v1.5.x 及之前版本的 AI API Key 使用 XOR 混淆文件（`ai_keys/*.vault`）存储，安全性等同于明文。XOR 密钥由可预测的机器指纹（`hostname + username`）派生，无密码学保护。

**解决**：将 AI API Key 存储统一迁移至操作系统原生安全存储：
- **macOS**: Keychain Services（`com.oxideterm.ai` 服务）
- **Windows**: Credential Manager
- **Linux**: Secret Service（libsecret / gnome-keyring）
- 与 SSH 密码享有同等 OS 级别加密保护

**改动文件**：
- `src-tauri/src/commands/config.rs`：5 个 `*_ai_provider_*` 命令从 `AiProviderVault` 改为 `Keychain` 调用
- `src-tauri/src/config/vault.rs`：标记为 DEPRECATED，仅保留供迁移读取
- `src-tauri/src/config/mod.rs`：更新模块文档

**迁移机制**：
- **懒迁移**：首次读取 provider key 时自动检测旧 vault 文件 → 解密 → 存入 keychain → 删除 vault 文件
- **零用户干预**：用户无需手动操作，升级后首次使用 AI 时自动完成
- **兼容性**：`has_ai_provider_api_key` 同时检查 keychain 和遗留 vault 文件

**前端**：零改动（Tauri 命令签名不变）

### 📝 文档更新
- `README.md` / `README.zh-CN.md` / `README.fr.md`：安全章节新增 AI API Key 存储说明
- `docs/AI_INLINE_CHAT.md`：API Key 存储描述从 "本地加密保险箱" 改为 "系统钥匙串"
- `docs/AI_SIDEBAR_CHAT.md`：配置表标注 keychain 存储
- `docs/SYSTEM_INVARIANTS.md`：新增 AI API Key 不变量

---

---

## 📦 Downloads

| Platform | File | Notes |
|----------|------|-------|
| macOS (Universal) | `OxideTerm_x.y.z_universal.dmg` | Requires `xattr -cr` |
| macOS (Intel) | `OxideTerm_x.y.z_x64.dmg` | Requires `xattr -cr` |
| macOS (Apple Silicon) | `OxideTerm_x.y.z_aarch64.dmg` | Requires `xattr -cr` |
| Windows (64-bit) | `OxideTerm_x.y.z_x64-setup.exe` | Installer |
| Windows (64-bit) | `OxideTerm_x.y.z_x64_en-US.msi` | MSI package |
| Linux (AppImage) | `OxideTerm_x.y.z_amd64.AppImage` | Portable |
| Linux (Debian) | `oxideterm_x.y.z_amd64.deb` | Debian/Ubuntu |

---

## 🔧 Installation Instructions

### 🍎 macOS 安装说明

> **重要**：从网络下载的 .dmg 文件会被 macOS Gatekeeper 隔离。

在终端中执行以下命令移除隔离属性：

```bash
# 对于 .dmg 文件
xattr -cr ~/Downloads/OxideTerm_*.dmg

# 或者安装后对应用执行
xattr -cr /Applications/OxideTerm.app
```

如果出现 "已损坏，无法打开" 错误，请确保执行上述命令。

---

### 🍎 macOS Installation

> **Important**: Downloaded .dmg files are quarantined by macOS Gatekeeper.

Run this command in Terminal to remove the quarantine attribute:

```bash
# For .dmg files
xattr -cr ~/Downloads/OxideTerm_*.dmg

# Or for the installed app
xattr -cr /Applications/OxideTerm.app
```

If you see "damaged and can't be opened" error, make sure to run the command above.

---

### 🪟 Windows 安装说明

1. 下载 `.msi` 或 `.exe` 安装包
2. 如果 Windows Defender SmartScreen 弹出警告，点击 "更多信息" → "仍要运行"
3. 按照安装向导完成安装

---

### 🪟 Windows Installation

1. Download the `.msi` or `.exe` installer
2. If Windows Defender SmartScreen shows a warning, click "More info" → "Run anyway"
3. Follow the installation wizard

---

### 🐧 Linux 安装说明

**AppImage (推荐)**：
```bash
chmod +x OxideTerm_*.AppImage
./OxideTerm_*.AppImage
```

**Debian/Ubuntu (.deb)**：
```bash
sudo dpkg -i oxideterm_*.deb
sudo apt-get install -f  # 安装依赖
```

---

### 🐧 Linux Installation

**AppImage (Recommended)**:
```bash
chmod +x OxideTerm_*.AppImage
./OxideTerm_*.AppImage
```

**Debian/Ubuntu (.deb)**:
```bash
sudo dpkg -i oxideterm_*.deb
sudo apt-get install -f  # Install dependencies
```

---

## 🔗 Links

- [Documentation](https://github.com/AnalyseDeCircuit/OxideTerm/tree/main/docs)
- [Report Issues](https://github.com/AnalyseDeCircuit/OxideTerm/issues)
- [Full Changelog](https://github.com/AnalyseDeCircuit/OxideTerm/tree/main/docs/changelog)