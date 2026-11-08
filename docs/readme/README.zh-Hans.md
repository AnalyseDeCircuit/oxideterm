<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm：在一个工作区里管理你的服务器" width="920">

# ⚡ OxideTerm

**免费、原生的 SSH 客户端，专为远程服务器工作打造。**

SSH · Mosh · Telnet · 串口 · RDP/VNC · SFTP · 端口转发 · 代码编辑器

无需账号。无需订阅。没有遥测。没有 Electron。

[![最新版本](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![平台](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![许可证](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Stars](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**下载**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**文档**](https://oxideterm.app) ·
[**更新日志**](../../.github/release-notes/stable-changelog.md) ·
[**报告问题**](https://github.com/AnalyseDeCircuit/oxideterm/issues) ·
[**微信交流群**](https://github.com/AnalyseDeCircuit/oxideterm/issues/752)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md)

</div>

---

## OxideTerm 是什么

OxideTerm 是一款适用于 macOS、Windows 和 Linux 的 SSH 客户端。你连接的每台服务器都有自己的工作区：终端、文件管理器、端口转发、代码编辑器、主机监控和可选的 AI 助手，都在同一个窗口里。OxideTerm 免费开源，采用 GPL-3.0 许可证。

## 为什么选择 OxideTerm

- **连接一次，处处可用。** 同一台服务器的终端、SFTP、端口转发和编辑器默认共用一条 SSH 连接，打开其他工具时不用再登录一遍。
- **扛得住短暂断网。** 连接失去响应时，OxideTerm 会先持续探测原连接 30 秒，再决定是否替换，vim、htop 等全屏程序因此能接着原来的状态运行。
- **内存占用低。** 界面由 [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)（Zed 编辑器背后的界面框架）直接在 GPU 上绘制。空闲时在 macOS 上约占 81 MB 内存，在 Windows 上约占 24 MB。
- **AI 用你自己的。** OxideSens 助手使用你选择的服务商和 API 密钥，没有需要另外购买的 OxideTerm 额度。
- **纯 Rust SSH。** SSH 协议栈基于 russh 和 ring，不依赖 OpenSSL 或 libssh2。

<details>
<summary><b>与 OxideTerm 1.x 的内存对比</b></summary>

从 Tauri（1.x）迁移到原生 GPUI（2.0）前后的空闲内存。1.x 的数值包含其 WebView 进程。

| 平台 | 1.x（Tauri） | 2.0（原生） | 降幅 |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 74% |
| Windows | 182.4 MB | 23.5 MB | 87% |

![系统进程监视器中 OxideTerm 1.x 与 2.0 的空闲内存](../../docs/screenshots/oxideterm-memory-comparison.png)

</details>

---

## 截图

### 终端

一个 SSH 会话。侧边栏列出了这台服务器上可以打开的内容：更多终端、SFTP、IDE 和端口转发。

![侧边栏列出服务器工具的 SSH 终端](../../docs/screenshots/terminal/SSHTERMINAL.png)

### 文件

本地和远程文件夹左右并排，传输队列在下方。

![带传输队列的 SFTP 双栏文件管理器](../../docs/screenshots/sftp/sftp.png)

### IDE

打开远程项目，在标签页中编辑文件。

![在内置 IDE 中打开的远程项目](../../docs/screenshots/miniIDE/miniide.png)

### 端口转发

OxideTerm 会列出服务器上正在监听的端口。点一下就能转发其中任意一个，也可以用快速转发打开 Jupyter 这类常用工具。

![列出服务器上已检测端口的端口转发页面](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png)

<details>
<summary><b>观看 OxideSens 根据一句自然语言打开终端</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens 在 OxideTerm 中打开终端" width="720">
</a>

</details>

---

## 功能

标有 *（插件）* 的功能需要在应用内的插件市场安装。

### 工作区

- 会话管理器：搜索、排序、分组、导入和导出已保存的连接，支持嵌套分组、标签、颜色、图标和备注
- 从 `~/.ssh/config` 导入，支持 `Match` 块、`ProxyJump` 链和 `RemoteCommand`，导入的主机会随配置文件保持更新
- 从 SecureCRT、Xshell、Termius、MobaXterm、WindTerm、Electerm 和 FinalShell 导入，保存前可以先预览
- 标签页范围：只显示全部标签页、全局页面、本地终端或某一台服务器，隐藏的会话照常运行
- 一个标签页最多容纳 16 个窗格，终端、SFTP、IDE、端口转发、笔记、设置和远程桌面可以混排
- 分离窗口：把标签页移到独立窗口再移回来，会话不会中断
- 把 2–16 个已打开的终端平铺成网格，无需重新连接
- 命令面板可查找操作和已保存的连接；禅模式只保留当前页面，方便专注
- 已保存的本地终端配置，各自带有 Shell、起始目录和图标
- 启动时恢复工作区、窗口大小、全屏状态和所在显示器

### 终端

- 自动检测本地 Shell：常见的 Unix Shell、命令提示符、Windows PowerShell、PowerShell Core、Git Bash 和 Nushell
- 分屏带可拖动的分隔线，新的本地分屏会沿用原窗格的工作目录
- 搜索支持正则表达式、大小写匹配和多行查询，每个窗格各自保留搜索状态
- Shell 集成支持 Bash、Zsh、Fish、Nushell 和 PowerShell，记录工作目录、退出码和命令标记，并可从应用内安装到远程服务器
- 命令折叠：收起已执行完毕的命令输出，同时保留它的退出状态
- 底部命令栏支持多行输入、历史记录、路径提示、补全和快捷命令
- 模糊匹配的历史建议，以及可选的按连接保存、重启后依然保留的历史记录
- 自由输入模式：用鼠标编辑当前命令
- 多行粘贴在发送前可以先编辑，支持撤销和去除代码围栏
- 终端触发器：输出中匹配到文本或正则表达式后，发送输入、运行快捷命令、启动程序，或把匹配内容变成可点击的操作
- 语义着色会给没有颜色的输出上色，例如错误、Git 状态、权限、地址和日期；另有关键词与正则高亮
- 可点击的 URL 和文件路径、精确到毫秒的逐行时间戳，以及标签页上的未读输出标记
- Sixel 和 Kitty 图形，包括 Kitty 动画
- 原生 `tmux -CC` 控制模式，支持窗格布局和可拖动的分隔线
- 同步组可以把你的输入同时发到多个终端；命令发送器能按节奏、间隔和重复次数发送文本或十六进制数据
- 快捷命令支持分组、置顶、文本/选项/保密参数，以及控制在哪些主机上显示的规则
- 会话日志支持文件名模板、大小和保留期限以及可选的 ANSI 控制码；另有 `.cast` 录制和内置播放器
- 复制、剪切和粘贴可以直接作用于 Vim、Neovim 或 Emacs 中的选区，本地和受支持的 SSH 会话都能用
- 按连接设置 Backspace 和 Delete 键发送的序列以及文本编码

### 连接

- SSH 登录方式：密码、私钥、OpenSSH 证书、托管密钥、SSH Agent、键盘交互式认证或 Kerberos
- SSH Agent 支持自动发现或自定义端点（macOS 和 Linux 上为 `SSH_AUTH_SOCK`，Windows 上为 `\\.\pipe\openssh-ssh-agent`），也支持 Agent 转发
- 支持 Ed25519、RSA 和 ECDSA 密钥以及 ChaCha20-Poly1305 和 AES-GCM 加密算法，可以调整算法顺序，并提供旧版服务器兼容选项
- 出现匹配的验证提示时，自动生成并填入 TOTP 验证码，服务器和每台跳板机都适用
- FIDO 安全密钥（`ed25519-sk`、`ecdsa-sk`），带 PIN 和触摸提示 *（插件）*
- 多级跳板机链路，每一跳单独认证；支持 HTTP CONNECT 和 SOCKS5 上游代理以及 ProxyCommand
- 宽限期重连：在新连接上恢复端口转发、继续传输并重新打开编辑器中的文件
- 每台服务器共用一条连接，或让每个新终端单独认证
- X11 转发，可选受信任或不受信任模式
- 不保存也能临时连接；`ssh://`、`telnet://`、`mosh://`、`rdp://` 和 `vnc://` 链接可以直接在应用中打开
- Mosh，支持自适应的本地回显预测 *（插件）*
- Telnet，支持选项协商、代理路由和可配置的换行符
- 串口：可设置波特率、数据位、校验位、停止位和流控，支持十六进制收发以及 Break、DTR 和 RTS 控制
- 蓝牙 BLE 终端，通过 Nordic UART Service 或自定义 GATT 特征通信

### 文件与传输

- 本地与远程双栏文件管理器，带收藏夹、路径补全，以及列表或树形视图
- 按连接保存成对的本地与远程路径书签
- 传输队列支持并行传输、限速、剩余时间、暂停、继续和重试；中断的 SFTP 传输在重启后会以暂停状态恢复
- 遇到冲突时可以选择覆盖、跳过或重命名，既可只针对一个文件，也可应用到全部
- 主机缺少 SFTP 子系统时，SFTP 会自动回退到 SCP
- 独立的 SFTP 连接，以及经由应用中转的服务器间传输
- FTP 和显式 FTPS
- 把文件拖到 SSH 终端上，即可上传到该终端的当前目录
- 在远程 Shell 中使用 `trz`/`tsz`（trzsz）和 `rz`/`sz`（ZMODEM）；串口连接支持 XMODEM、YMODEM 和 ZMODEM
- 预览文本、代码、Markdown、图片、音频、视频、十六进制数据和字体
- PDF、SQLite、证书和二进制文件查看器 *（插件）*
- 从终端跳到 SFTP 中的同一目录，也能把 SFTP 目录发回终端

### 编辑器与 IDE

- 通过 SFTP、纯 Shell 命令或远程 Agent 打开远程 IDE，远程 Agent 还提供符号索引、补全和文件监视
- 把空闲的终端转换成 IDE，沿用同样的用户、容器和目录
- 多标签页、支持多选的项目文件树、项目搜索，以及支持正则表达式和捕获组的查找替换
- 可编辑最大 100 MiB 的文件，并保留编码、BOM 和换行符
- 安全保存与冲突处理，可选在切换标签页时自动保存
- 重启后恢复已打开的文件和未保存的修改
- Git 视图：分支、工作树、暂存、历史、领先/落后提交数，以及合并或变基状态
- 识别并运行项目任务
- 内置 15 种语言的语法高亮，另有 20 多种语言可通过插件添加 *（插件）*
- Markdown 预览，支持表格、公式和脚注

### 端口转发与网络

- 本地、远程和动态（SOCKS5）转发，规则可以保存，并能随连接自动启动
- 自动检测服务器上正在监听的端口，一键转发
- 独立的 SSH 隧道，无需打开终端就能运行，也可以随应用启动
- 重连后自动恢复端口转发，手动停止的规则保持停止
- 应用级代理设置；应用的网络请求使用系统信任的证书（包括企业 CA）
- 连接拓扑视图和临时 Socket 调试

### 远程桌面

- RDP 和 VNC，支持键盘、鼠标、剪贴板、缩放和重连 *（插件）*
- RDP 多显示器布局和网络质量配置 *（插件）*
- 基于 VeNCrypt TLS 的 VNC *（插件）*
- 通过 SSH 跳板机连接 RDP 和 VNC *（插件）*
- 用内置查看器运行 WSLg 图形应用（Windows）

### 主机工具

- 适用于本机和任意 SSH 服务器
- CPU、内存、交换分区、磁盘、网络、负载和延迟，带趋势图
- 进程（发送信号、暂停与继续、调整 nice 值）、服务（systemd、launchd、BSD 和 Windows 服务）、计划任务和软件包；Windows 上还可以用 winget 升级和卸载
- 支持预设和跟随模式的日志、带公网暴露提示的监听端口，以及带空间不足警告的文件系统
- Docker 容器：状态、端口、启动、停止、重启和日志
- GPU 和 NPU 监控，支持 NVIDIA、AMD ROCm、海光 DCU、华为昇腾、Intel XPU、摩尔线程 MUSA 和寒武纪
- tmux、Zellij 和 Herdr 会话：附加、新建、拆分和重命名
- 连接池和连接矩阵面板，以及终端工具栏中的 SSH 延迟显示

### OxideSens AI

- 支持 OpenAI（包括 Responses API）、Anthropic、Gemini、DeepSeek、xAI Grok、小米 MiMo、Ollama 以及任何兼容 OpenAI 的端点，并可调节思考强度
- 读取你选定的终端、文件和设置作为上下文，并通过工具执行命令、操作交互式程序、编辑文件和传输数据
- 只读、默认和免审批三种模式，可按类别设置工具权限和命令策略规则，破坏性或涉及凭据的命令会被额外审查
- 图片附件、对话分支、消息编辑和排队追问
- 并行工作的子代理、后台任务，以及中断后能继续执行的任务
- 通过 ACP 接入 Codex 和 Claude Code，并提供 OpenCode、Antigravity、Cursor、Grok Build、GitHub Copilot CLI、Qwen Code 和 Kimi CLI 的启动器 *（插件）*
- 通过 stdio、Streamable HTTP 或旧版 SSE 连接 MCP 服务器
- 外部 MCP 控制：让编辑器和其他 AI 工具通过 OxideTerm 使用你保存的连接，权限按工具组授予，随时可以撤销
- [Agent Skills](../../docs/agent-skills.md)（`SKILL.md`），支持 `/skill-name` 补全
- Markdown 笔记知识库，结合 BM25 关键词检索和 HNSW 向量检索，支持中日韩分词，预览中可显示 Mermaid 图表

### 安全

- 应用锁，支持 Touch ID（macOS）或 Windows Hello（Windows），锁定期间连接照常运行
- 提权凭据：sudo 或 su 提示出现时提供已保存的密码，可限定到本机、单个连接或主机匹配规则，两步验证提示还可附带 TOTP
- 操作审计，支持筛选、保留期限，以及导出为 JSON 或 CSV
- 加密的会话录制，支持回放、拖动进度和倍速
- 插件启用前需要先审查它的权限

### 同步与备份

- 云同步支持 WebDAV、S3 兼容存储（包括阿里云 OSS）、Git 仓库、GitHub Gist、Dropbox、OneDrive、Google Drive 或 HTTP JSON
- 自选同步内容，合并多台设备上的修改，并逐一处理冲突
- 回滚备份，以及从中断的同步中恢复
- 加密的 `.oxide` 便携包，可迁移连接、端口转发、设置、快捷命令和密钥，导入时可以选择跳过、重命名、替换或合并
- 便携模式：直接从文件夹运行，带独立的加密密钥库
- 应用内更新，可选稳定版或 Beta 通道

### 个性化

- 浅色、深色或跟随系统的外观，应用主题和终端主题分开设置，并带主题编辑器
- 背景支持图片、动图和视频，可以叠加雾光、粒子、流星、水面焦散等生成特效，另有额外的素材包 *（插件）*
- 窗口透明度和圆角样式
- 字体栈，支持单独的中日韩字体、连字和字重；编辑器排版可以单独设置
- 界面缩放、光标样式、内边距和动效设置
- 自定义快捷键；设置项可以搜索，页面布局可以重新排列
- 11 种界面语言，支持输入法和双向文本

### 插件

| 类型 | 运行方式 | 能访问的范围 |
|---|---|---|
| **清单** | 声明式扩展 | 无；不运行任何代码 |
| **WASM** | Wasmtime/WASI 或独立辅助进程 | 仅限被授予的主机能力 |
| **进程** | 普通本地进程 | 与本地程序相同，不受操作系统沙箱限制 |

请只安装来自可信来源的进程插件。为 OxideTerm 1.x 编写的插件可能仍会出现在列表中，但 2.x 不会运行它们。

### 命令行工具

在 **设置 → 常规 → CLI 伴侣工具** 中安装 `oxideterm` 命令。它和应用共用同一份配置，不打开窗口也能检查、编写脚本和备份你的配置，还能从任意 Shell 打开新的标签页。会修改内容的命令可以先用 `--dry-run` 查看计划，加上 `--yes` 才会真正执行，并保留可回滚的备份。

```bash
oxideterm terminal --cwd ~/project
oxideterm doctor --strict
oxideterm connections search prod
oxideterm forwards list --format json
oxideterm oxide export ./profile.oxide --connection prod --password-stdin
```

提供 Bash、Zsh、Fish、PowerShell 和 Elvish 的 Shell 补全。全部命令见 [CLI 指南](../../docs/user-guide/zh-Hans/cli.md)。

---

<a id="install"></a>

## 安装

从[最新版本](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)下载 OxideTerm。

| 操作系统 | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG（Intel） | DMG（Apple Silicon） |
| **Windows** | 安装程序（`.exe`） | 安装程序（`.exe`） |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

发布页面还提供便携版压缩包、签名，以及用于校验下载的 `sha256sums.txt` 文件。

### macOS

如果 Gatekeeper 阻止打开应用，请移除隔离标记：

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

如果 SmartScreen 显示警告，请选择 **更多信息 → 仍要运行**。

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

### 窗口无法正常绘制时

在环境变量中设置 `OXIDETERM_RENDER_PROFILE=compatibility`，然后重新启动 OxideTerm。

### 快速上手

1. 打开 **会话管理器** 添加服务器，或从 `~/.ssh/config` 导入主机。
2. 连接服务器，打开终端。
3. 在同一台服务器上打开 SFTP、端口转发或 IDE。
4. 如需使用 OxideSens，请在 **设置** 中添加 AI 服务商。

[文档](https://oxideterm.app)介绍了应用的其余功能。想自己构建 OxideTerm，请参阅[从源码构建](#build-from-source)。

---

## 隐私与安全

- 保存的密码和密钥存放在系统钥匙串中（macOS 钥匙串、Windows 凭据管理器，或 Linux 上的 libsecret）。AI 服务商的密钥不会写入应用日志。
- 内存中的机密数据在释放时会被清零（zeroize）。
- 主机密钥在首次连接时信任，之后都与 `~/.ssh/known_hosts` 核对；密钥发生变化时会拒绝连接。
- 发送给 AI 服务商的文本会先经过凭据脱敏。
- 会话录制默认关闭。录制内容加密保存在你的设备上，不参与云同步，也不会记录你的键盘输入。
- 通知与审计默认关闭。开启后，数据保存在你的设备上，敏感细节会加密，且只记录 OxideTerm 内部发生的操作。
- `.oxide` 便携包使用 ChaCha20-Poly1305 加密，密钥由 Argon2id 派生（256 MB 内存，4 次迭代）。

---

<a id="build-from-source"></a>

## 从源码构建

需要 Rust（rustup 会自动安装 `rust-toolchain.toml` 中固定的工具链版本）和能运行 GPUI 的桌面环境。

```bash
# Run the app
cargo run

# Build the oxideterm CLI
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

使用 Nix 时，可以运行 `nix build .#oxideterm`、`nix run .#oxideterm` 或 `nix develop`。CLI 的构建产物位于 `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`。

### 整体结构

界面、终端和 SSH 运行在同一个 Rust 进程中，可选的远程 Agent 和平台辅助程序则作为独立进程运行。终端输出从 SSH 通道或本地 PTY 直接写入 `TerminalState`，再由 GPUI 绘制，中间没有 WebView、JSON 桥接或 xterm.js。

`NodeRouter` 从基于 DashMap 的连接池 `SshConnectionRegistry` 中分配连接，所以终端窗格、SFTP、端口转发和编辑器会共用每台服务器的一条 SSH 连接，除非某个终端设置为使用独立连接。每条连接都会经历 `connecting → active → idle → link_down → reconnecting` 这些状态。跳板机出现故障时，它后面的服务器会进入 `link_down`。每条转发规则都有自己的监听器。重连后，SFTP 会话会在新连接上重新获取，而在旧连接上发起的操作绝不会被悄悄转移到新连接。

```text
crates/
├── oxideterm-gpui-app/        # Desktop app: windows, workspace, settings
├── oxideterm-gpui-terminal/   # Terminal panes: input, selection, overlays
├── oxideterm-terminal/        # Terminal model and escape-sequence parsing
├── oxideterm-ssh/             # SSH connections, NodeRouter, jump hosts
├── oxideterm-sftp/            # SFTP sessions and transfers
├── oxideterm-forwarding/      # Port forwarding runtime
├── oxideterm-gpui-ide/        # Remote IDE
├── oxideterm-ai/              # OxideSens: providers, MCP, RAG
├── oxideterm-cli/             # The oxideterm command
├── oxideterm-i18n/            # Locale catalogs for the 11 interface languages
└── gpui-ce/                   # Patched copy of GPUI
```

继续阅读：[开发指南](../../docs/development/README.md)、[crate 地图](../../docs/development/crate-map.md)和[架构概览](../../docs/user-guide/zh-Hans/architecture.md)。

---

## 参与贡献

欢迎贡献代码、文档、翻译、插件、测试和问题复现。较大的改动请先开 issue 讨论。[CONTRIBUTING.md](../../docs/CONTRIBUTING.md) 介绍了如何准备拉取请求。

附上脱敏后的诊断包，问题能更快得到修复：

```bash
oxideterm report --bundle ./oxideterm-report.zip
```

如果是从源码运行，请改用 `cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip`。

### 贡献者

感谢每一位让 OxideTerm 变得更好的贡献者。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="OxideTerm 贡献者">
  </a>
</p>

---

## 致谢

OxideTerm 基于以下开源项目构建：

- [russh](https://github.com/warp-tech/russh) 和 [ring](https://github.com/briansmith/ring)：SSH 协议与加密
- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)：来自 Zed 编辑器的 GPU 加速界面框架
- [alacritty_terminal](https://github.com/alacritty/alacritty)：终端模拟（VT100–VT500）
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty)：本地 Shell，在 Windows 上使用 ConPTY
- [Tokio](https://tokio.rs)：异步运行时
- [Wasmtime](https://wasmtime.dev/)：运行 WASM 插件
- [tree-sitter](https://tree-sitter.github.io/)：编辑器的语法高亮

---

## 许可证

OxideTerm 采用 [GPL-3.0-only](../../LICENSE) 许可证，没有附加限制。依赖项声明见 [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) 和 [NOTICE](../../NOTICE)。

请只在你拥有或已获授权访问的系统上使用 OxideTerm。完整的法律声明见 [LEGAL.md](../../LEGAL.md)。

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
