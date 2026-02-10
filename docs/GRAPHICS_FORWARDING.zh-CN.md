# WSL 图形转发

> **版本**: v0.3.0 | **状态**: ⚠️ 实验性 | **平台**: 仅限 Windows

OxideTerm 内置 WSL 图形查看器，让你可以在终端标签页中运行 Linux GUI 桌面——无需外部 VNC 客户端。

---

## 概述

WSL Graphics 模块在 WSL 发行版内启动 **Xtigervnc** 独立 X 服务器，启动桌面会话（D-Bus + 桌面环境），并通过 localhost WebSocket 桥接以 **noVNC** 在应用内渲染。

```
WSL (Ubuntu)                    OxideTerm (Tauri)
┌────────────────┐              ┌──────────────────────────────┐
│ Xtigervnc :10  │──TCP──▶     │  Rust Bridge (WS↔TCP)        │
│  └─ Desktop    │  localhost   │       │                      │
│    (D-Bus)     │              │       ▼                      │
└────────────────┘              │  GraphicsView.tsx (noVNC)    │
                                │  ┌──────────────────────┐    │
                                │  │ Canvas (RFB)         │    │
                                │  │ Toolbar (auto-hide)  │    │
                                │  └──────────────────────┘    │
                                └──────────────────────────────┘
```

**关键特性**：
- 与终端数据平面零耦合——不修改 Wire Protocol
- WebSocket 仅绑定 `127.0.0.1`——永远不对外暴露
- 每个桥接连接使用一次性 CSPRNG Token
- Feature-gated：`wsl-graphics`（未启用的平台注册返回错误的桩命令）

---

## 支持的桌面环境

| 桌面环境 | 安装包 | 状态 | 备注 |
|---------|---------|------|------|
| **Xfce** | `xfce4` | ✅ 稳定（已测试） | 推荐。已在 Ubuntu 24.04 上测试。 |
| **GNOME** | `ubuntu-desktop` | ⚠️ **实验性** | 使用 `gnome-session --session=gnome-xorg`。强制 X11 模式（`XDG_SESSION_TYPE=x11`、`GDK_BACKEND=x11`）。深层进程树——清理时递归 kill。**不提供保证。** |
| **KDE Plasma** | `kde-plasma-desktop` | ⚠️ **实验性** | 使用 `startplasma-x11`。强制 Qt X11 模式（`QT_QPA_PLATFORM=xcb`、`DESKTOP_SESSION=plasma`、`KWIN_COMPOSE=N`）。深层进程树——清理时递归 kill。**不提供保证。** |
| **MATE** | `mate-desktop-environment` | 未测试 | 轻量级 GTK 桌面，理论上可用。 |
| **LXDE** | `lxde` | 未测试 | 非常轻量，理论上可用。 |
| **Cinnamon** | `cinnamon-desktop-environment` | 未测试 | Mint 的默认桌面，理论上可用。 |
| **Openbox** | `openbox` | 未测试 | 极简窗口管理器，无桌面会话。 |
| **Fluxbox** | `fluxbox` | 未测试 | 极简窗口管理器。 |
| **IceWM** | `icewm` | 未测试 | 极简窗口管理器。 |

> **仅 Xfce（Ubuntu 24.04）经过开发者验证。** 其他桌面环境可能可用，但以"按原样"提供。GNOME 和 KDE Plasma 在 UI 中明确标记为实验性并显示警告徽章。

---

## 前置要求

在 WSL 发行版中安装：

```bash
# 核心：Xtigervnc + D-Bus
sudo apt update && sudo apt install tigervnc-standalone-server dbus-x11 -y

# 桌面环境（选择一个）：
sudo apt install xfce4 -y                  # ✅ 推荐
sudo apt install ubuntu-desktop -y          # ⚠️ GNOME（实验性）
sudo apt install kde-plasma-desktop -y      # ⚠️ KDE Plasma（实验性）
```

---

## 架构

### 后端（Rust）

```
src-tauri/src/graphics/
├── mod.rs        # 类型、状态、错误、Feature Gate、桩命令
├── wsl.rs        # WSL 检测、Xtigervnc、桌面引导、清理
├── bridge.rs     # WebSocket ↔ VNC TCP 透明代理
└── commands.rs   # 5 个 Tauri IPC 命令
```

**Feature Gate**：`#[cfg(all(feature = "wsl-graphics", target_os = "windows"))]`

在非 Windows 平台或未启用 Feature 时，同名的 5 个命令注册为桩函数，返回 `"WSL Graphics is only available on Windows"`。

### 前端（React）

`src/components/graphics/GraphicsView.tsx` — 内置标签页组件：

- **发行版选择器**：通过 `wsl_graphics_list_distros` 列出 WSL 发行版
- **noVNC 查看器**：`@novnc/novnc` RFB 类，启用 `scaleViewport + resizeSession`
- **自动隐藏工具栏**：显示发行版名称、桌面名称、Experimental 徽章、重连、全屏、停止
- **状态叠加层**：启动加载动画、断连警告、错误提示

### Tauri 命令

| 命令 | 描述 |
|-----|------|
| `wsl_graphics_list_distros` | 列出 WSL 发行版（解析 `wsl.exe --list --verbose`，处理 UTF-16LE） |
| `wsl_graphics_start` | 检查前置条件 → 启动 Xtigervnc → 桌面会话 → WebSocket 桥接 |
| `wsl_graphics_stop` | 终止桥接、VNC、桌面；通过 PID 文件递归清理进程树 |
| `wsl_graphics_reconnect` | 仅重建 WebSocket 桥接（VNC + 桌面保持运行） |
| `wsl_graphics_list_sessions` | 列出活跃的图形会话 |

---

## 会话生命周期

### 启动流程

1. 用户从选择器中选择一个 WSL 发行版
2. 后端检查前置条件：`Xtigervnc`、桌面环境、D-Bus
3. 查找空闲 X Display（从 `:10` 开始）和 TCP 端口（OS 分配）
4. 启动 Xtigervnc：`-SecurityTypes None -localhost=0 -geometry 1920x1080 -depth 24`
5. 等待 RFB 握手（12 字节 `"RFB 003.0xx\n"`）
6. 生成引导 shell 脚本 → 通过 stdin 管道传输到 `wsl.exe -d {distro} -- bash -s`
7. 引导脚本：清除 WSLg 环境变量、设置 `XDG_RUNTIME_DIR`、注入桌面环境特定环境变量、启动 D-Bus + 桌面、写入 PID 文件
8. 在随机端口上启动 WebSocket ↔ TCP 桥接，附带 CSPRNG Token
9. 向前端返回 `{ id, wsPort, wsToken, distro, desktopName }`
10. 前端通过 `ws://127.0.0.1:{port}?token={token}` 连接 noVNC

### 停止流程

1. 前端断开 noVNC RFB 连接
2. 后端中止桥接任务，终止 VNC 子进程，终止桌面子进程
3. 执行 `cleanup_wsl_session`：读取 PID 文件 → 递归 `kill_tree`（遍历 `pgrep -P`，从叶到根）→ 强制终止残留进程 → 删除临时目录

### 重连（仅桥接）

VNC 服务器和桌面会话保持运行。仅重建 WebSocket 桥接，使用新端口和 Token。适用于浏览器标签页失去连接但桌面未崩溃的情况。

### 应用退出

`WslGraphicsState::shutdown()` 遍历所有活跃会话并执行完整清理。

---

## 桌面环境特定的环境变量

`DesktopCandidate` 结构体携带 `extra_env` 字段——启动桌面前注入引导脚本的 shell export：

| 桌面环境 | `extra_env` |
|---------|------------|
| Xfce、MATE、LXDE 等 | *（无）* |
| GNOME | `export XDG_SESSION_TYPE=x11` + `export GDK_BACKEND=x11` |
| KDE Plasma | `export QT_QPA_PLATFORM=xcb` + `export DESKTOP_SESSION=plasma` + `export KWIN_COMPOSE=N` |

如果不设置这些变量，GNOME 会尝试 Wayland，KDE 会启用 OpenGL 合成——两者在无头 Xtigervnc 中都会失败。

---

## 进程清理

GNOME 和 KDE Plasma 会产生深层进程树（gnome-session → gnome-shell → gnome-settings-daemon → …，或 startplasma-x11 → kwin_x11 → kded5 → …）。简单的 `pkill -P` 只能终止直接子进程。

清理使用递归 `kill_tree()` 函数：

```bash
kill_tree() {
    local pid=$1
    local children
    children=$(pgrep -P "$pid" 2>/dev/null) || true
    for child in $children; do
        kill_tree "$child"
    done
    kill -TERM "$pid" 2>/dev/null || true
}
```

该函数从叶节点到根节点遍历进程树，自底向上发送 `SIGTERM`，然后对残留进程强制执行 `SIGKILL`。

---

## 已知问题与注意事项

| 问题 | 缓解措施 |
|-----|---------|
| **noVNC 静默断连** | `accept_hdr_async` 必须在响应中包含 `Sec-WebSocket-Protocol: binary` 头——否则浏览器会静默关闭 WebSocket |
| **wsl.exe UTF-16LE 输出** | `wsl --list --verbose` 在某些 Windows 版本上输出 UTF-16LE（带或不带 BOM）——通过启发式方式解码 |
| **端口 TOCTOU 竞争** | `find_free_port()` 绑定 `:0`、读取端口、释放——通过 `wait_for_vnc_ready()` 超时缓解 |
| **GNOME/KDE 崩溃** | 实验性桌面——复杂的合成器可能在所有 WSL 配置中无法正常工作。Xfce 是最安全的选择。 |
| **Xtigervnc `-randr` 参数** | Xtigervnc 1.13.x 内置 RANDR（`AcceptSetDesktopSize=1` 为默认值）——**不要**传递 `-randr` |

---

## 安全性

- WebSocket 桥接绑定 `127.0.0.1:0`——仅限 localhost
- CSPRNG 32 字节 Token，常量时间比较
- VNC `-SecurityTypes None` 是安全的，因为只有 localhost 可以访问
- Token 通过 URL 查询字符串传递，在 `accept_hdr_async` 中验证
- 所有端口均由 OS 随机分配（绝不硬编码 5900）

---

## 国际化

图形 UI 字符串位于 `src/locales/{lang}/graphics.json`（11 种语言）。关键字符串包括 `desktop_experimental` 用于显示警告徽章。

---

*文档版本：v0.3.0 | 最后更新：2026-02-11*
