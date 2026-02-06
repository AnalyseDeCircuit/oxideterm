# OxideTerm v1.5.0 Release Notes

## 📋 What's Changed


### ✨ 新特性

#### 1. 资源监控器 (Resource Profiler)

实时采样远程 Linux 主机的 CPU、内存、负载和网络指标。

**后端** (`session/profiler.rs`, ~760 行)：
- **持久化 Shell 通道**：整个生命周期仅打开 1 个 Shell Channel，避免 MaxSessions 耗尽
- **轻量采样**：精简命令输出 ~500-1.5KB（`head -1 /proc/stat` + `grep MemTotal|MemAvailable`），10s 间隔
- **Delta 计算**：CPU% 和网络速率基于两次采样差值，首次返回 `None`
- **优雅降级**：非 Linux 主机或连续 3 次失败后自动降级到 RTT-Only 模式
- **自动生命周期**：通过 `subscribe_disconnect()` 绑定，SSH 断连自动停止
- **std::sync::RwLock**：极短临界区避免 async 调度开销，减少终端 PTY I/O 竞争
- **ProfilerRegistry**：DashMap 注册表 + 4 个 Tauri 命令 + 应用退出统一清理
- **8+ 单元测试**：覆盖 `/proc` 解析、delta 计算、首采空值、空输出降级

**前端**：
- `profilerStore.ts`：Zustand Store，per-connection 状态，Tauri Event 订阅
- `api.ts`：4 个 API 包装函数
- `types/index.ts`：`ResourceMetrics` / `MetricsSource` 类型定义
- 11 种语言 i18n 支持（`src/locales/*/profiler.json`）

**性能影响**：~6-12 KB/min 额外 SSH 带宽，内存 ~30 KB/连接

> 详见 [docs/RESOURCE_PROFILER.md](../RESOURCE_PROFILER.md)

### 🔧 修复

#### 1. 文件预览窗口模式溢出修复
- **问题**：QuickLook 预览窗口在窗口化模式下超出应用边界被裁剪
- **原因**：`fixed inset-0 z-50` 定位在 `absolute inset-0 z-10` 的 tab wrapper 内部，受祖先 `overflow: hidden` 裁剪
- **解决**：使用 `createPortal(…, document.body)` 将预览 overlay 渲染到 `<body>`，脱离 stacking context
- **额外优化**：
  - 背景层添加 `overflow-auto`，面板添加 `m-auto shrink-0`
  - `minWidth`/`minHeight` 用 `min()` 函数钳位到视口尺寸，防止小窗口溢出

#### 2. `opener:allow-open-path` 权限错误
- **问题**：文件管理器中"打开方式"调用 `openPath()` 报错 `opener.open_path not allowed`
- **解决**：在 `capabilities/default.json` 中添加 `opener:allow-open-path` scope 配置，允许所有路径 (`"path": "**"`)

#### 3. Dotfile 路径无法用外部程序打开
- **问题**：`.bashrc`、`.ssh` 等以点开头的路径不匹配 `**` 通配符
- **解决**：在 `tauri.conf.json` 的 `plugins` 中为 `opener` 添加 `"requireLiteralLeadingDot": false`

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