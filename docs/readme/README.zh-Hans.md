<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm：在一个工作区中管理你的服务器" width="920">

# ⚡ OxideTerm

**免费的原生 SSH 客户端与远程运维工作区，内置使用自备密钥的 AI 助手。**

SSH · Mosh · Telnet · 串口 · RDP/VNC · SFTP · 端口转发 · 内置编辑器，集成在同一个 GPU 渲染的应用中。
无需账号，无需订阅，不采集遥测，不使用 Electron。

[![最新版本](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![平台](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![许可证](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Stars](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**下载**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**文档**](https://oxideterm.app) ·
[**更新日志**](../../.github/release-notes/stable-changelog.md) ·
[**报告问题**](https://github.com/AnalyseDeCircuit/oxideterm/issues)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md) | [Русский](README.ru.md)

</div>

---

## 快速上手

1. **安装 OxideTerm。** 从[最新发布页面](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)下载安装包；各平台的说明见下方[安装](#install)一节。
2. **添加服务器。** 打开会话管理器，新建 SSH 连接，或从 `~/.ssh/config` 导入主机。
3. **连接服务器。** 打开终端。主机密钥会与 `~/.ssh/known_hosts` 中的记录核对。
4. **使用工作区中的其他功能。** 在同一节点上打开 SFTP、端口转发或内置编辑器。默认情况下，它们共享一个 SSH 连接。
5. **可选：启用 AI。** 在设置中添加你自己的 OpenAI、Anthropic、Gemini、Ollama 或兼容 OpenAI 的服务端点，即可启用 OxideSens。

需要详细导览，请参阅[文档](https://oxideterm.app)。

---

## 功能一览

| | |
|---|---|
| **终端与协议** | 本地 Shell、SSH、Mosh、Telnet、串口、分屏、多跳连接、SSH Agent 与 Agent 转发、双因素认证和 TOTP 凭据、X11 转发、Shell 集成、命令标记、可配置的会话日志、录制、Sixel 和 Kitty 图形、trzsz 传输 |
| **tmux 与广播** | 原生 `tmux -CC` 控制模式，支持窗格布局和拖动分隔线；命名广播组；可按计划重复发送输入的高级多目标命令发送器 |
| **连接可靠性** | 宽限期重连让 TUI 应用在短暂断网期间继续运行，随后恢复端口转发、传输和已打开的编辑器文件 |
| **文件与编辑** | SFTP 双栏文件管理器、带限速和预计完成时间的传输队列、书签；内置远程编辑器，支持安全写入、冲突处理和工作区恢复 |
| **网络功能** | 本地、远程和动态 SOCKS5 转发、已保存的规则、远程端口检测、连接拓扑、临时 Socket 调试 |
| **远程桌面** | 内置 RDP 和 VNC，支持剪贴板与输入 |
| **主机运维** | 监控进程、服务、日志、端口、任务、磁盘、软件包、容器和 tmux |
| **AI 与自动化** | 使用自备密钥的 OxideSens、MCP、本地 RAG、Agent Skills、经批准的工作区操作、独立命令行工具 |
| **回顾与审计** | 可选的通知与审计工作区，以及加密会话录制（两者默认均关闭） |
| **同步与便携** | 加密云同步、便携式 `.oxide` 文件包 |
| **个性化** | 主题、背景图片、可配置的快捷键、快捷命令、11 种界面语言 |

---

## 为什么选择 OxideTerm

- **免费，本地优先。** 无需账号，无需订阅，不采集遥测。连接与运维数据由你掌控。
- **每台服务器，一个工作区。** 终端、SFTP、端口转发、RDP/VNC、编辑器、监控和 AI 都连接到同一节点，不再像各自独立的工具。
- **原生应用，而非浏览器套壳。** 界面通过 [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) 直接在 GPU 上绘制，不使用 Electron，也不捆绑 WebView。
- **AI 由你决定。** OxideSens 使用你自己的服务商和密钥，只执行你批准的操作。
- **能应对短暂断网。** 宽限期重连会先探测原连接 30 秒，再决定是否替换，让 TUI 应用有机会在短暂断网后继续运行。
- **纯 Rust SSH。** SSH 使用 `russh` 与 `ring`，不依赖 OpenSSL 或 libssh2。

---

## 内存占用

**原生重写后，macOS 上的空闲内存约为旧版的四分之一，Windows 上约为八分之一。** 以下是维护者在从 Tauri 1.x 迁移到原生 GPUI 2.0 时记录的观察结果：

| 平台 | Tauri 1.x（空闲） | 原生 2.0（空闲） | 降幅 |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 约 74% |
| Windows | 182.4 MB | 23.5 MB | 约 87% |

旧版的总占用包含 OxideTerm 及其关联的 WebView 进程。原生版不再需要这些浏览器进程。

![附系统进程截图的空闲内存对比：Tauri 1.x 与原生 2.0](../../docs/screenshots/oxideterm-memory-comparison.png)

---

## 截图

| 带 OxideSens 的 SSH 终端 | SFTP 文件管理器 |
|---|---|
| ![带 OxideSens AI 的 SSH 终端](../../docs/screenshots/terminal/SSHTERMINAL.png) | ![带传输队列的 SFTP 双栏文件管理器](../../docs/screenshots/sftp/sftp.png) |

| 内置 IDE | 智能端口转发 |
|---|---|
| ![内置 IDE 模式](../../docs/screenshots/miniIDE/miniide.png) | ![支持自动检测的智能端口转发](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png) |

<details>
<summary><b>观看 OxideSens 根据自然语言请求打开终端</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens 在 OxideTerm 中打开终端" width="720">
</a>

</details>

---

## OxideSens AI

OxideSens 是可选的助手，可查看正在运行的会话，并且**仅在你批准后**执行工作区操作。

- **自备密钥。** 支持 OpenAI、Anthropic（Claude）、Google Gemini、Ollama 及兼容 OpenAI 的服务端点，并提供适配各服务商的推理控制选项。不使用平台积分。
- **MCP 与 Agent Skills。** 可连接 MCP 服务器（stdio 和 SSE），并加载有明确使用范围的 Agent Skills。
- **本地知识库（RAG）。** BM25 全文检索与向量索引相结合。
- **上下文由你控制。** 由你决定批准哪些工作区上下文和操作，命令策略规则仍然适用。
- **凭据脱敏。** 发给服务商的消息会先根据凭据特征进行脱敏。
- **密钥保存在操作系统钥匙串中**，不会写入结构化日志。

---

<a id="plugins"></a>

## 插件

OxideTerm 支持三种插件方式：

| 类型 | 运行方式 | 边界 |
|---|---|---|
| **仅清单** | 声明式扩展，不含代码 | 不含可执行代码 |
| **WASM** | Wasmtime/WASI 或独立辅助进程 | 主机调用受控，权限按能力限定 |
| **进程** | 普通本地进程 | 受信任的本地代码，**不受**操作系统沙箱隔离 |

旧版 Tauri（1.x）的 ESM 插件可能仍会显示在列表中，但原生 2.x 应用不会执行它们。请只安装来自可信来源的进程插件。

---

## 安全与隐私

| 项目 | 实现方式 |
|---|---|
| **存储的凭据** | 操作系统钥匙串（macOS Keychain、Windows Credential Manager、libsecret） |
| **内存中的秘密数据** | 含秘密数据的类型和临时缓冲区在支持的所有权边界使用 `zeroize` 清除 |
| **主机密钥** | 首次使用时依据 `~/.ssh/known_hosts` 建立信任；拒绝未预期的密钥变更 |
| **便携导出** | `.oxide` 文件包使用 ChaCha20-Poly1305 与 Argon2id（256 MB 内存，4 次迭代） |
| **AI 上下文** | 内容到达服务商前，先根据凭据特征进行脱敏；上下文和操作由你批准 |
| **会话录制** | 默认关闭；加密存储在你的设备上，不参与云同步；不录制键盘输入 |
| **审计** | 默认关闭；数据保留在你的设备上，敏感细节加密存储 |
| **命令行修改操作** | 改变状态的命令提供试运行计划、`--yes` 确认保护和回滚备份 |
| **插件** | 见[插件](#plugins) |
| **遥测** | 无 |

**合法使用。** OxideTerm 采用 GPL-3.0-only 许可证，无额外限制。请仅访问你拥有或获明确授权访问的系统、网络和设备，并遵守适用法律。不得使用 OxideTerm 进行未授权访问、干扰服务或绕过访问控制。

---

## 当前限制

安装前，请了解以下情况：

- 仅支持桌面平台（macOS、Windows、Linux），没有移动应用。
- 项目迭代较快，发布频繁。请参阅[更新日志](../../.github/release-notes/stable-changelog.md)和[未解决的问题](https://github.com/AnalyseDeCircuit/oxideterm/issues)。
- 审计与会话录制需主动启用，且只反映 OxideTerm 自身能够观察到的活动。
- 进程插件不受操作系统沙箱隔离。
- 如果渲染器在你的机器上无法正常运行，可尝试兼容模式：`OXIDETERM_RENDER_PROFILE=compatibility`。

---

<a id="for-developers"></a>

## 开发者信息

<details>
<summary><b>从源码运行</b></summary>

**要求：** Rust 工具链（edition 2024），以及能够运行 GPUI 的桌面环境。

```bash
# Run the app
cargo run

# If the renderer fails on your machine
OXIDETERM_RENDER_PROFILE=compatibility cargo run

# Build the headless CLI companion
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

使用 Nix：`nix build .#oxideterm`、`nix run .#oxideterm` 或 `nix develop`。

命令行工具的构建产物位于 `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`。

</details>

<details>
<summary><b>命令行接口</b></summary>

无界面的 `oxideterm` 命令行工具无需启动应用即可运行，适用于自动化、CI 和诊断。它涵盖设置、连接、端口转发、插件、快捷命令、秘密数据、便携文件包、诊断、报告、批量计划、备份和云同步。

```bash
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- forwards list --format json
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- oxide export ./profile.oxide --connection prod --password-stdin
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
cargo run -p oxideterm-cli -- completion install zsh --force

# Path and profile isolation for CI or fixtures
cargo run -p oxideterm-cli -- --config-dir ./fixture-config doctor --strict
```

</details>

<details>
<summary><b>架构</b></summary>

界面与终端及 SSH 后端共用一个 Rust 进程；可选的远程 Agent 和平台辅助程序位于该进程之外。终端字节直接更新 `TerminalState`，GPUI 根据该状态渲染，无需经过 JSON、WebSocket、Base64 或 xterm.js 解析。

```
┌─────────────────────────────────────────────────┐
│               GPUI Render Loop                  │
│   WorkspaceApp  ·  Tab surfaces  ·  GPUI views  │
└──────────────────────┬──────────────────────────┘
                       │  in-process Arc<> / async
┌──────────────────────▼──────────────────────────┐
│             Domain Crates (Rust async)          │
│  NodeRouter → SshConnectionRegistry             │
│  TerminalState ← SSH PTY channel (russh)        │
│  SftpSession · ForwardingRuntime · IdeWorkspace │
│  Ai/ACP Entities · CloudSync · Plugin Runtimes  │
└─────────────────────────────────────────────────┘
```

| 方面 | 捆绑浏览器的方案 | OxideTerm |
|---|---|---|
| 渲染 | 浏览器引擎与网页布局 | 在 GPU 画面上使用 GPUI |
| 终端数据流 | WebSocket → JS 事件循环 → xterm.js | Rust 输入 → `TerminalState` → GPUI 渲染 |
| 连接生命周期 | 分散在前端和后端 | 同一进程内统一处理连接与重连 |
| AI 上下文 | 通过应用桥接层复制 | 根据当前工作区构建，并由用户批准 |
| 命令行工具 | 需要桌面应用保持运行 | 独立程序，直接链接各 crate |

**连接池。** `SshConnectionRegistry` 以 `DashMap` 为基础，通过 `NodeRouter` 使用。终端窗格、SFTP、端口转发和编辑器可共享每个节点上的一个物理 SSH 连接，终端策略也允许改用专用连接。每个连接遵循 `connecting → active → idle → link_down → reconnecting` 的状态变化。跳板机故障会将下游节点标记为 `link_down`。AI 和插件通过能力句柄与主机快照使用相关功能，不注册为连接消费者。

**宽限期重连。**

1. 检测保活超时。
2. 保存终端窗格、SFTP 传输、端口转发和编辑器文件的快照。
3. 探测原连接 30 秒，让 TUI 应用有机会在短暂断网后继续运行。
4. 建立新连接，恢复端口转发、继续传输并重新打开编辑器文件。

SFTP 会话带有连接世代标识：重连后，符合条件的会话会重新获取，但旧世代的操作绝不会被悄悄转移到新连接。

**端口转发。** 独立 crate，支持 `-L`、`-R` 和 `-D`（SOCKS5）。每个 SSH 通道由一个 `ssh_io` 任务独立管理，关键执行路径上没有共享互斥锁。

**纯 Rust SSH。** `russh` 与 `ring`：完整 SSH2、ChaCha20-Poly1305 和 AES-GCM、Ed25519/RSA/ECDSA 密钥、Unix（`SSH_AUTH_SOCK`）与 Windows（`\\.\pipe\openssh-ssh-agent`）上的 SSH Agent，以及每跳独立认证的多跳链路。

**技术栈**

| 层级 | 技术 |
|---|---|
| 界面 | GPUI（Zed 的 GPU 界面框架） |
| 运行时 | Tokio、DashMap |
| SSH | `russh` 与 `ring`（不依赖 OpenSSL 或 libssh2） |
| 本地 PTY | `portable-pty`（Windows 上使用 ConPTY） |
| 终端模拟 | `alacritty_terminal`（VT100–VT500、Sixel、Kitty 图形） |
| 编辑器 | tree-sitter 语法高亮、自定义缓冲区 |
| 加密 | ChaCha20-Poly1305、Argon2id |
| 插件 | Wasmtime/WASI、独立辅助进程中的 WASM，以及进程方式 |
| AI 流式响应 | SSE（OpenAI、Anthropic、Gemini），在进程内处理 |
| RAG | BM25 与 HNSW 向量索引相结合，融合排序，使用 CJK 双字分词器 |
| 国际化 | `oxideterm-i18n`（11 种语言） |

</details>

---

<a id="install"></a>

## 安装

[**下载最新版本**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

| 操作系统 | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG（Intel） | DMG（Apple Silicon） |
| **Windows** | 安装程序（`.exe`） | 安装程序（`.exe`） |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

请使用发布页面中的 `sha256sums.txt` 文件验证下载内容。便携版压缩包和签名也列在该页面。

### macOS

如果 Gatekeeper 阻止运行应用，请移除隔离标记：

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

如果 SmartScreen 显示警告，请选择**更多信息 → 仍要运行**。

### Linux

```bash
# AppImage
chmod +x OxideTerm_*_linux_*.AppImage && ./OxideTerm_*_linux_*.AppImage

# Debian / Ubuntu
sudo dpkg -i OxideTerm_*_linux_*.deb && sudo apt-get install -f

# Fedora / RHEL-compatible
sudo dnf install ./OxideTerm_*_linux_*.rpm

# Nix; updates are managed by Nix
nix run github:AnalyseDeCircuit/oxideterm
```

想自己构建？请查看[开发者信息](#for-developers)中的**从源码运行**。

---

## 参与贡献

欢迎贡献 Rust 代码、文档、翻译、插件、测试，以及问题复现。较大的改动请先提交 issue 讨论。

附上已脱敏的诊断包，能让问题报告更有帮助：

```bash
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

可复现的缺陷与回归问题会优先处理。功能请求会根据范围、安全性，以及是否符合 OxideTerm 远程服务器工作区的方向进行评估。如果 OxideTerm 对你的工作有帮助，GitHub Star、可复现的问题报告、翻译修正或插件，都能帮助项目继续发展。

### 贡献者

感谢每一位帮助 OxideTerm 变得更好的贡献者。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="OxideTerm 贡献者">
  </a>
</p>

---

## 许可证

**GPL-3.0-only。** 依赖署名见 [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md)，其他声明见 [`NOTICE`](../../NOTICE)。

**基于以下项目构建：** [russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
