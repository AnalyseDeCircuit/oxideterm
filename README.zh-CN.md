<p align="center">
  <img src="src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>Rust 驱动的终端引擎 — 不止于 SSH</strong>
  <br>
  <em>95,000+ 行 Rust &amp; TypeScript 代码。零 Electron。SSH 栈零 C 依赖。</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.13.5-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-PolyForm%20Noncommercial-blueviolet" alt="License">
  <img src="https://img.shields.io/badge/rust-1.75+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-CN.md">简体中文</a> | <a href="README.fr.md">Français</a>
</p>

---

## OxideTerm 是什么？

OxideTerm 是一款**跨平台终端应用**，将本地 Shell、远程 SSH 会话、文件管理、代码编辑和 AI 助手整合进一个 Rust 原生二进制文件中。它**不是** Electron 套壳——后端完全由 Rust 编写，通过 Tauri 2.0 打包为约 20-35 MB 的原生可执行文件。

### 为什么需要 OxideTerm？

| 痛点 | OxideTerm 的解答 |
|---|---|
| SSH 客户端不支持本地 Shell | 混合引擎：本地 PTY + 远程 SSH 在同一窗口 |
| 断线重连 = 丢失一切 | **Node-first 架构**：自动重连带宽限期保护 TUI 应用；恢复转发、传输、IDE 状态 |
| 远程编辑需要 VS Code Remote | **内置 IDE 模式**：CodeMirror 6 基于 SFTP，默认零安装；Linux 可选部署远端 Agent 增强体验 |
| SSH 连接不可复用 | **SSH 多路复用**：终端、SFTP、转发共享一条连接 |
| SSH 库依赖 OpenSSL | **russh 0.54**：纯 Rust SSH，`ring` 密码学后端，无 C 依赖 |

---

## 架构概览

```
┌─────────────────────────────────────┐
│        前端 (React 19)              │
│                                     │
│  SessionTreeStore ──► AppStore      │    10 个 Zustand Store
│  IdeStore    LocalTerminalStore     │    17 个组件目录
│  ReconnectOrchestratorStore         │    11 种语言 × 18 命名空间
│  PluginStore  AiChatStore  ...      │
│                                     │
│        xterm.js 6 + WebGL           │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (二进制)
┌──────────▼──────────────▼───────────┐
│         后端 (Rust)                 │
│                                     │
│  NodeRouter ── resolve(nodeId) ──►  │    22 个 IPC 命令模块
│  ├─ SshConnectionRegistry          │    DashMap 并发状态
│  ├─ SessionRegistry                │    Feature-gated 本地 PTY
│  ├─ ForwardingManager              │    ChaCha20-Poly1305 保险库
│  ├─ SftpSession (连接级)            │    russh 0.54 (ring 后端)
│  └─ LocalTerminalRegistry          │    SSH Agent (AgentSigner)
│                                     │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]       │
└─────────────────────────────────────┘
```

**双平面通信**：WebSocket 二进制帧承载终端 I/O（零序列化开销），Tauri IPC 承载结构化命令和事件。前端从不接触 `sessionId` 或 `connectionId`——一切通过 `nodeId` 寻址，由后端 `NodeRouter` 解析。

---

## 核心技术亮点

### 🔩 纯 Rust SSH — russh 0.54

OxideTerm 搭载 **russh 0.54**，编译使用 `ring` 密码学后端：
- SSH 路径中**零 C/OpenSSL 依赖**——整个密码学栈纯 Rust 实现
- 完整 SSH2 协议：密钥交换、通道、SFTP 子系统、端口转发
- ChaCha20-Poly1305 和 AES-GCM 密码套件，Ed25519/RSA/ECDSA 密钥

### 🔑 SSH Agent 认证 (AgentSigner)

自研 `AgentSigner` 封装系统 SSH Agent，满足 russh 的 `Signer` trait：

```rust
// 通过将 &PublicKey 克隆为 owned 值，解决 russh 0.54 中
// RPITIT Send bound 跨 .await 借用问题
pub struct AgentSigner { /* ... */ }
impl Signer for AgentSigner { /* 通过 Agent IPC 完成挑战-响应签名 */ }
```

- **平台支持**：Unix (`SSH_AUTH_SOCK`)、Windows (`\\.\pipe\openssh-ssh-agent`)
- **代理链支持**：每一跳可独立使用 Agent 认证
- **重连韧性**：重连时自动重放 `AuthMethod::Agent`

### 🧭 Node-First 架构 (NodeRouter)

**Oxide-Next 节点抽象**消灭了一整类竞态条件：

```
前端: useNodeState(nodeId) → { readiness, sftpReady, error }
后端: NodeRouter.resolve(nodeId) → ConnectionEntry → SftpSession
```

- 前端 SFTP/IDE 操作只传 `nodeId`——不传 `sessionId`，不传 `connectionId`
- 后端原子解析 `nodeId → ConnectionEntry`
- SSH 重连导致 `connectionId` 变化——SFTP/IDE **无感知**
- `NodeEventEmitter` 推送带 generation 计数器的类型化事件，保证有序性

### ⚙️ 本地终端 — 线程安全 PTY

基于 `portable-pty 0.8` 的跨平台本地 Shell，通过 `local-terminal` Feature Gate 控制：

- **线程安全**：`std::sync::Mutex` 封装 `MasterPty` + `unsafe impl Sync`
- **专用 I/O 线程**：阻塞式 PTY 读取不干扰 Tokio 事件循环
- **Shell 探测**：自动识别 `zsh`、`bash`、`fish`、`pwsh`、Git Bash、WSL2
- **Feature Gate**：`cargo build --no-default-features` 可剥离 PTY，为移动端铺路

### 🔌 运行时插件系统 (v1.6.2+)

动态插件加载，冻结 API，安全加固：

- **PluginContext API**：8 个命名空间（terminal, ui, commands, settings, lifecycle, events, storage, system）
- **24 个 UI Kit 组件**：预构建 React 组件注入插件沙箱
- **安全模型**：`Object.freeze` + Proxy 访问控制、熔断器机制、IPC 白名单
- **Membrane 架构**：插件在隔离 ESM 上下文中运行，通过受控桥接访问宿主

### 🛡️ SSH 智能连接池

基于引用计数的 `SshConnectionRegistry`，底层 DashMap：

- 多终端、SFTP、端口转发共享**同一条物理 SSH 连接**
- 每连接独立状态机（connecting → active → idle → link_down → reconnecting）
- 空闲超时 (30 分钟)、心跳保活 (15 秒)、心跳驱动的故障检测
- WsBridge 本地心跳：30 秒间隔、5 分钟超时（容忍 App Nap）
- 空闲超时断连发 `connection_status_changed` 事件通知前端
- 级联传播：跳板机断连 → 所有下游节点标记 `link_down`
- **智能感知**：`visibilitychange` + `online` 事件 → 主动 SSH 探测（~2 秒 vs 被动 15-30 秒）
- **宽限期**：30 秒窗口尝试恢复现有连接，避免破坏性重连杀死 TUI 应用（yazi/vim/htop）

### 🔀 端口转发 — 无锁 I/O

完整的本地 (-L)、远程 (-R) 和动态 SOCKS5 (-D) 转发：

- **消息传递架构**：SSH Channel 由单一 `ssh_io` 任务持有，无 `Arc<Mutex<Channel>>`
- **死亡报告**：转发任务在 SSH 断开时主动上报退出原因
- **自动恢复**：`Suspended` 状态的转发规则在重连后自动恢复
- **空闲超时**：`FORWARD_IDLE_TIMEOUT` (300 秒) 防止僵尸连接

### 🤖 AI 终端助手

双模式 AI，隐私优先：

- **内联面板** (`⌘I`)：快速命令，通过 Bracketed Paste 注入终端
- **侧边栏聊天**：持久化对话，支持历史记录
- **上下文捕获**：Terminal Registry 从活动或全部分屏面板采集缓冲区
- **广泛兼容**：OpenAI、Ollama、DeepSeek、OneAPI，任意 `/v1/chat/completions` 端点
- **安全存储**：API Key 存于系统钥匙串（macOS Keychain / Windows Credential Manager）

### 💻 IDE 模式 — 远程编辑

CodeMirror 6 编辑器通过 SFTP 操作远程文件——默认无需服务器端安装，Linux 上可选部署轻量远端 Agent 以获得增强体验：

- **文件树**：SFTP 懒加载 + Git 状态指示器
- **30+ 语言模式**：16 个原生 CodeMirror 语言包 + legacy modes
- **冲突解决**：基于 `mtime` 的乐观锁
- **事件驱动 Git**：保存/创建/删除/重命名/终端回车后自动刷新状态
- **状态门禁**：`readiness !== 'ready'` 时阻断所有 IO，重连时 Key-Driven Reset
- **Linux 远端 Agent（可选）**：~1 MB Rust 二进制，x86_64/aarch64 自动部署；ARMv7、RISC-V64、LoongArch64、s390x 等额外架构可从 `agents/extra/` 手动下载上传

### 🔐 .oxide 加密导出

可移植的连接备份格式：

- **ChaCha20-Poly1305 AEAD** 认证加密
- **Argon2id KDF**（256 MB 内存成本，4 迭代）——抗 GPU 暴力破解
- **SHA-256** 完整性校验
- **可选密钥内嵌**：私钥以 base64 编码嵌入加密载荷
- **导出前体检**：认证类型统计、缺失密钥检测

### 📡 ProxyJump — 拓扑感知的多跳连接

- 无限链式深度：`Client → Jump A → Jump B → … → Target`
- 自动解析 SSH Config，构建拓扑图，Dijkstra 最优路径计算
- 跳板机节点可复用为独立会话
- 级联故障传播，下游节点状态自动同步

### 📊 资源监控器

通过持久化 SSH Shell 通道实时采集远程主机指标：

- 读取 `/proc/stat`、`/proc/meminfo`、`/proc/loadavg`、`/proc/net/dev`
- 基于 Delta 的 CPU% 和网络吞吐量计算
- 单通道设计——不触发 MaxSessions 限制
- 非 Linux 主机或连续失败时自动降级为 RTT-Only 模式
### 🖼️ 背景图片画廊

多图背景系统，支持按标签页透明度控制：

- **画廊管理**：上传多张图片，点击缩略图切换，单张删除或一键清除
- **总开关**：全局启用/禁用背景图，不会删除已上传图片
- **按标签页控制**：13 种标签页类型可独立开关（终端、SFTP、IDE、设置、拓扑等）
- **自定义**：透明度 (3–50%)、模糊 (0–20px)、填充模式 (覆盖/适应/拉伸/平铺)
- **平台感知**：macOS 透明支持；Windows WSLg 路径排除（VNC 画布不支持透明）
- **安全**：路径规范化删除防止目录穿越；Rust 后端完整错误传播

### 🎨 自定义主题引擎

超越预设配色方案的全深度主题定制：

- **30+ 内置主题**：Oxide、Dracula、Nord、Catppuccin、Spring Rice、Tokyo Night 等
- **可视化编辑器**：颜色选取器 + RGB 十六进制输入，覆盖每个字段
- **终端配色**：xterm.js 全部 22 个字段（背景、前景、光标、选区、16 ANSI 色）
- **UI 界面色**：19 个 CSS 变量，分 5 大类——背景(5)、文字(3)、边框(3)、强调(4)、语义状态色(4)
- **自动推导**：一键从终端配色生成全套 UI 颜色
- **实时预览**：编辑时即时展示迷你终端 + UI 界面效果
- **复制 & 扩展**：基于任意内置或自定义主题创建新主题
- **持久存储**：自定义主题保存至 localStorage，跨更新保留

### 🪟 Windows 深度优化

- **原生 ConPTY 集成**：直接调用 Windows Pseudo Console (ConPTY) API，完美支持 TrueColor 和 ANSI 转义序列——告别过时的 WinPTY。
- **智能 Shell 探测**：内置扫描引擎自动检测 **PowerShell 7 (pwsh)**、**Git Bash**、**WSL2** 和传统 CMD，通过注册表和 PATH 扫描。
- **原生体验**：Rust 直接处理窗口事件——响应速度远超 Electron 应用。

### 📊 后端滚动缓冲区

- **大容量持久化**：默认 **100,000 行**终端输出，可序列化到磁盘（MessagePack 格式）。
- **高性能搜索**：`spawn_blocking` 隔离正则搜索任务，避免阻塞 Tokio 运行时。
- **内存高效**：环形缓冲区设计自动淘汰最旧数据，内存用量可控。

### ⚛️ 多 Store 状态架构

前端采用 **Multi-Store** 模式（10 个 Store）应对差异化的状态管理需求：

| Store | 职责 |
|---|---|
| **SessionTreeStore** | 用户意图层 — 树形结构、连接流、会话组织 |
| **AppStore** | 事实层 — 通过 `connections` Map 管理实际 SSH 连接状态，从 SessionTreeStore 同步 |
| **IdeStore** | IDE 模式 — 远程文件编辑、Git 状态跟踪、多标签编辑器 |
| **LocalTerminalStore** | 本地 PTY 生命周期、Shell 进程监控、独立 I/O |
| **ReconnectOrchestratorStore** | 自动重连管道（snapshot → grace-period → ssh-connect → await-terminal → restore） |
| **TransferStore** | SFTP 传输队列与进度 |
| **PluginStore** | 插件运行时状态和 UI 注册表 |
| **ProfilerStore** | 资源监控指标 |
| **AiChatStore** | AI 对话状态 |
| **SettingsStore** | 应用设置 |

尽管状态来源不同，渲染逻辑通过 `TerminalView` 和 `IdeView` 统一视图层。
---

## 技术栈

| 层级 | 技术 | 说明 |
|---|---|---|
| **框架** | Tauri 2.0 | 原生二进制，~15 MB，零 Electron |
| **运行时** | Tokio + DashMap 6 | 全异步 + 无锁并发映射 |
| **SSH** | russh 0.54 (`ring`) | 纯 Rust，零 C 依赖，SSH Agent |
| **本地 PTY** | portable-pty 0.8 | Feature-gated，Windows ConPTY |
| **前端** | React 19.1 + TypeScript 5.8 | Vite 7，Tailwind CSS 4 |
| **状态管理** | Zustand 5 | 10 个专用 Store，事件驱动同步 |
| **终端渲染** | xterm.js 6 + WebGL | GPU 加速，60fps+ |
| **编辑器** | CodeMirror 6 | 16 语言包 + legacy modes |
| **加密** | ChaCha20-Poly1305 + Argon2id | AEAD 认证加密 + 内存硬化 KDF |
| **存储** | redb 2.1 | 嵌入式数据库（会话、转发、传输） |
| **序列化** | MessagePack (rmp-serde) | 二进制缓冲区/状态持久化 |
| **国际化** | i18next 25 | 11 种语言 × 18 命名空间 |
| **SFTP** | russh-sftp 2.0 | SSH 文件传输协议 |
| **WebSocket** | tokio-tungstenite 0.24 | 异步 WebSocket，终端数据平面 |
| **协议** | Wire Protocol v1 | 二进制 `[Type:1][Length:4][Payload:n]` 基于 WebSocket |
| **插件** | ESM Runtime | 冻结 PluginContext + 24 UI Kit 组件 |

---

## 功能矩阵

| 分类 | 功能 |
|---|---|
| **终端** | 本地 PTY、SSH 远程、分屏 (水平/垂直)、会话录制/回放 (asciicast v2)、跨分屏 AI 上下文、WebGL 渲染、背景图片画廊、30+ 主题 + 自定义主题编辑器 |
| **SSH** | 连接池、多路复用、ProxyJump (∞ 跳)、拓扑图、自动重连管道 |
| **认证** | 密码、SSH 密钥 (RSA/Ed25519/ECDSA)、SSH Agent、证书、Keyboard-Interactive (2FA)、Known Hosts |
| **文件** | 双面板 SFTP 浏览器、拖放传输、预览 (图片/视频/音频/PDF/代码/Hex)、传输队列 |
| **IDE** | 文件树、CodeMirror 编辑器、多标签、Git 状态、冲突解决、集成终端 |
| **转发** | 本地 (-L)、远程 (-R)、动态 SOCKS5 (-D)、自动恢复、死亡报告、无锁 I/O |
| **AI** | 内联面板 + 侧边栏聊天、流式 SSE、命令插入、OpenAI/Ollama/DeepSeek |
| **插件** | ESM 运行时加载、8 API 命名空间、24 UI Kit、沙箱执行、熔断器 |
| **WSL 图形** ⚠️ | 内置 VNC 查看器（实验性）：桌面模式（9 种桌面环境）+ 应用模式（单 GUI 应用），WSLg 检测，Xtigervnc + noVNC，支持重连，Feature-gated |
| **安全** | .oxide 加密导出、系统钥匙串、`zeroize` 内存擦除、主机密钥 TOFU |
| **国际化** | EN, 简体中文, 繁體中文, 日本語, FR, DE, ES, IT, 한국어, PT-BR, VI |

---

## 功能特性介绍

### 🚀 混合终端体验
- **零延迟本地 Shell**：直接 IPC 与本地进程交互，近零延迟。
- **高性能远程 SSH**：WebSocket 二进制流传输，跃过传统 HTTP 开销。
- **完整环境继承**：继承 PATH、HOME 等全部环境变量，与系统终端体验一致。

### 🔐 多元化认证方式
- **密码认证**：安全存储于系统钥匙串。
- **密钥认证**：支持 RSA / Ed25519 / ECDSA，自动扫描 `~/.ssh/id_*`。
- **SSH Agent**：通过 `AgentSigner` 访问系统 Agent（macOS/Linux/Windows）。
- **证书认证**：OpenSSH Certificates。
- **2FA/MFA**：Keyboard-Interactive 认证。
- **Known Hosts**：主机密钥 TOFU 验证 + `~/.ssh/known_hosts`。

### 🔍 全文搜索
项目级文件内容搜索，智能缓存：
- **实时搜索**：300ms 防抖输入，即时返回结果。
- **结果缓存**：60 秒 TTL 缓存，避免重复扫描。
- **分组展示**：按文件分组，带行号定位。
- **高亮匹配**：搜索词在预览中高亮显示。
- **自动失效**：文件变更时自动清除缓存。

### 📦 高级文件管理
- **SFTP v3 协议**：完整双面板文件管理器。
- **拖放传输**：支持多文件和文件夹批量操作。
- **智能预览**：
  - 🎨 图片 (JPEG/PNG/GIF/WebP)
  - 🎬 视频 (MP4/WebM) 内置播放器
  - 🎵 音频 (MP3/WAV/OGG/FLAC) 含元数据展示
  - 💻 代码高亮 (30+ 语言)
  - 📄 PDF 文档
  - 🔍 Hex 查看器（二进制文件）
- **进度跟踪**：实时速度、进度条、预计完成时间。

### 🌍 国际化 (i18n)
- **11 种语言**：English、简体中文、繁體中文、日本語、Français、Deutsch、Español、Italiano、한국어、Português、Tiếng Việt。
- **动态加载**：通过 i18next 按需加载语言包。
- **类型安全**：所有翻译键均有 TypeScript 类型定义。

### 🌐 网络优化
- **双平面架构**：数据平面（WebSocket 直连）与控制平面（Tauri IPC）分离。
- **自定义二进制协议**：`[Type:1][Length:4][Payload:n]`，无 JSON 序列化开销。
- **背压控制**：突发流量时防止内存溢出。
- **自动重连**：指数退避重试，最多 5 次。

### 🖥️ WSL 图形（⚠️ 实验性）
- **桌面模式**：在终端标签页内运行完整 Linux GUI 桌面——支持 9 种桌面环境（Xfce / GNOME / KDE Plasma / MATE / LXDE / Cinnamon / Openbox / Fluxbox / IceWM），自动检测。
- **应用模式**：无需完整桌面，直接启动单个 GUI 应用（如 `gedit`、`firefox`）——轻量 Xtigervnc + 可选 Openbox WM，应用退出时自动清理。
- **WSLg 检测**：自动检测每个发行版的 WSLg 可用性（Wayland / X11 socket），UI 中显示状态徽章。
- **Xtigervnc + noVNC**：独立 X 服务器，通过应用内 `<canvas>` 渲染，支持 `scaleViewport` 和 `resizeSession`。
- **安全性**：`argv` 数组注入（无 shell 解析），`env_clear()` + 最小白名单，`validate_argv()` 6 层防御，并发限制（每发行版 4 个应用会话，全局 8 个）。
- **重连**：WebSocket 桥接可在不终止 VNC 会话的情况下重新建立。
- **Feature-gated**：`wsl-graphics` Cargo Feature，非 Windows 平台注册桩命令。

---

## 快速开始

### 前置要求

- **Rust** 1.75+
- **Node.js** 18+（推荐 pnpm）
- **平台工具**：
  - macOS: Xcode Command Line Tools
  - Windows: Visual Studio C++ Build Tools
  - Linux: `build-essential`、`libwebkit2gtk-4.1-dev`、`libssl-dev`

### 开发构建

```bash
git clone https://github.com/AnalyseDeCircuit/OxideTerm.git
cd OxideTerm && pnpm install

# 完整应用（前端 + Rust 后端 + 本地 PTY）
pnpm tauri dev

# 仅前端（端口 1420 热更新）
pnpm dev

# 生产构建
pnpm tauri build

# 轻量内核——剥离本地 PTY，适配移动端
cd src-tauri && cargo build --no-default-features --release
```

---

## 项目结构

```
OxideTerm/
├── src/                            # 前端 — 56K 行 TypeScript
│   ├── components/                 # 17 个目录
│   │   ├── terminal/               #   终端视图、分屏、搜索
│   │   ├── sftp/                   #   双面板文件浏览器
│   │   ├── ide/                    #   编辑器、文件树、Git 对话框
│   │   ├── ai/                     #   内联 + 侧边栏聊天
│   │   ├── plugin/                 #   插件管理 & 运行时 UI
│   │   ├── forwards/               #   端口转发管理
│   │   ├── connections/            #   连接增删改查 & 导入
│   │   ├── topology/               #   网络拓扑图
│   │   ├── layout/                 #   侧边栏、头部、分屏布局
│   │   └── ...                     #   sessions, settings, modals 等
│   ├── store/                      # 10 个 Zustand Store
│   ├── lib/                        # API 层、AI 提供者、插件运行时
│   ├── hooks/                      # React Hooks (事件、键盘、Toast)
│   ├── types/                      # TypeScript 类型定义
│   └── locales/                    # 11 种语言 × 18 命名空间
│
├── src-tauri/                      # 后端 — 39K 行 Rust
│   └── src/
│       ├── router/                 #   NodeRouter (nodeId → 资源)
│       ├── ssh/                    #   SSH 客户端 (12 模块含 Agent)
│       ├── local/                  #   本地 PTY (feature-gated)
│       ├── graphics/               #   WSL 图形 (feature-gated)
│       ├── bridge/                 #   WebSocket 桥接 & Wire Protocol v1
│       ├── session/                #   会话管理 (16 模块)
│       ├── forwarding/             #   端口转发 (6 模块)
│       ├── sftp/                   #   SFTP 实现
│       ├── config/                 #   保险库、钥匙串、SSH Config
│       ├── oxide_file/             #   .oxide 加密 (ChaCha20)
│       ├── commands/               #   22 个 Tauri IPC 命令模块
│       └── state/                  #   全局状态类型
│
└── docs/                           # 28+ 架构与功能文档
```

---

## 路线图

### ✅ 已发布 (v1.8.0)

- [x] 本地终端 (PTY) + Feature Gating
- [x] SSH 连接池 & 多路复用
- [x] SSH Agent 认证 (AgentSigner)
- [x] Node-first 架构 (NodeRouter + 事件)
- [x] 自动重连编排器 (8 阶段管道，含宽限期)
- [x] ProxyJump 无限跳板机链
- [x] 端口转发 — 本地 / 远程 / 动态 SOCKS5
- [x] SFTP 双面板文件管理 + 预览
- [x] IDE 模式 (CodeMirror 6 + Git 状态)
- [x] .oxide 加密导出 + 密钥内嵌
- [x] AI 终端助手 (内联 + 侧边栏)
- [x] 运行时插件系统 (PluginContext + UI Kit)
- [x] 终端分屏 + 快捷键
- [x] 资源监控器 (CPU / 内存 / 网络)
- [x] 国际化 — 11 种语言 × 18 命名空间
- [x] Keyboard-Interactive 认证 (2FA/MFA)
- [x] 深度历史搜索 (30K 行，Rust Regex)
- [x] WSL 图形 — 桌面模式 + 应用模式 VNC 查看器（⚠️ 实验性）
- [x] 背景图片画廊 — 多图上传、按标签页控制、总开关
- [x] 增强媒体预览 — SFTP 浏览器内音频/视频播放
- [x] 会话录制 & 回放
- [x] 自定义主题引擎 — 30+ 内置主题、可视化编辑器支持十六进制输入、22 终端 + 19 UI 颜色字段

### 🚧 进行中

- [ ] 命令面板 (`⌘K`)
- [ ] 会话搜索 & 快速切换

### 📋 计划中

- [ ] SSH Agent 转发

---

## 安全设计

| 关注点 | 实现 |
|---|---|
| **密码** | 系统钥匙串 (macOS Keychain / Windows Credential Manager / Linux libsecret) |
| **AI API Key** | 系统钥匙串 `com.oxideterm.ai` 服务 |
| **配置文件** | `~/.oxideterm/connections.json` — 仅存储钥匙串引用 ID |
| **导出** | .oxide: ChaCha20-Poly1305 + Argon2id，可选密钥内嵌 |
| **内存** | `zeroize` 擦除敏感数据；Rust 编译器保证内存安全 |
| **主机密钥** | TOFU 模式 + `~/.ssh/known_hosts` |
| **插件** | Object.freeze + Proxy ACL、熔断器、IPC 白名单 |

---

## 许可证

**PolyForm Noncommercial 1.0.0**

- ✅ 个人 / 非营利使用：免费
- 🚫 商业使用：需获取商业授权
- ⚖️ 专利防御条款 (Nuclear Clause)

完整协议：https://polyformproject.org/licenses/noncommercial/1.0.0/

---

## 致谢

- [russh](https://github.com/warp-tech/russh) — 纯 Rust SSH 实现
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty) — 跨平台 PTY 抽象
- [Tauri](https://tauri.app/) — 原生应用框架
- [xterm.js](https://xtermjs.org/) — 终端模拟器
- [CodeMirror](https://codemirror.net/) — 代码编辑器
- [Radix UI](https://www.radix-ui.com/) — 无障碍 UI 基元

---

<p align="center">
  <sub>以 Rust 和 Tauri 构建 — 95,000+ 行代码</sub>
</p>
