<p align="center">
  <img src="../../src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>如果您喜欢 OxideTerm，请在 GitHub 上点个 Star ⭐️！</em>
</p>


<p align="center">
  <strong>零 Electron。零 OpenSSL。零遥测。零订阅。纯 Rust SSH。</strong>
  <br>
  <em>一个原生二进制——本地 Shell、SSH、SFTP、远程 IDE、AI、端口转发、插件、30+ 主题、11 种语言。</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.1.0--beta.6-blue" alt="版本">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="平台">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="许可证">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
  <img src="https://img.shields.io/github/downloads/AnalyseDeCircuit/oxideterm/total?color=brightgreen" alt="总下载量">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=%E4%B8%8B%E8%BD%BD%E6%9C%80%E6%96%B0%E7%89%88&style=for-the-badge&color=brightgreen" alt="下载最新版">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=%E4%B8%8B%E8%BD%BD%E6%9C%80%E6%96%B0Beta%E7%89%88&style=for-the-badge&color=orange" alt="下载最新Beta版">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

> [!NOTE]
> **许可证变更：** 从 v1.0.0 起，OxideTerm 已将许可证从 **PolyForm Noncommercial 1.0.0** 变更为 **GPL-3.0（GNU 通用公共许可证 v3.0）**。这意味着 OxideTerm 现在是完全开源的——您可以在 GPL-3.0 许可证条款下自由使用、修改和分发。详见 [LICENSE](../../LICENSE) 文件。

---

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI —「打开一个本地终端并运行 echo hello, world!」*

</div>

---

## 为什么选择 OxideTerm？

| 痛点 | OxideTerm 的解决方案 |
|---|---|
| SSH 客户端无法使用本地 Shell | **混合引擎**：本地 PTY（zsh/bash/fish/pwsh/WSL2）+ 远程 SSH 同窗共存 |
| 断线重连 = 丢失一切 | **宽限期重连**：断开前探测旧连接 30 秒——你的 vim/htop/yazi 安然无恙 |
| 远程编辑需要 VS Code Remote | **内置 IDE**：CodeMirror 6 基于 SFTP，支持 30+ 语言，可选 ~1 MB Linux 远程 Agent |
| SSH 连接无法复用 | **多路复用**：终端、SFTP、转发、IDE 通过引用计数连接池共享同一 SSH 连接 |
| SSH 库依赖 OpenSSL | **russh 0.59**：基于 `ring` 编译的纯 Rust SSH——零 C 依赖 |
| 100+ MB 的 Electron 应用 | **Tauri 2.0**：原生 Rust 后端，25–40 MB 二进制文件 |
| AI 被锁定在单一供应商 | **OxideSens**：40+ 工具、MCP 协议、RAG 知识库——支持 OpenAI/Ollama/DeepSeek 及任何兼容 API |
| 凭证存储在明文配置文件中 | **仅系统钥匙串**：密码和 API 密钥绝不落盘；`.oxide` 文件使用 ChaCha20-Poly1305 + Argon2id 加密 |
| 依赖云端、需要注册账号 | **本地优先**：零账号、零遥测、零云同步——数据留在你的设备上。AI 密钥自行提供 |

---

## 截图

<table>
<tr>
<td align="center"><strong>SSH 终端 + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="带 OxideSens AI 侧边栏的 SSH 终端" /></td>
<td align="center"><strong>SFTP 文件管理器</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="SFTP 双窗格文件管理器与传输队列" /></td>
</tr>
<tr>
<td align="center"><strong>内置 IDE（CodeMirror 6）</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="基于 CodeMirror 6 编辑器的内置 IDE 模式" /></td>
<td align="center"><strong>智能端口转发</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="带自动检测的智能端口转发" /></td>
</tr>
</table>

---

## 功能概览

| 分类 | 功能 |
|---|---|
| **终端** | 本地 PTY（zsh/bash/fish/pwsh/WSL2）、SSH 远程、分屏窗格、广播输入、会话录制/回放（asciicast v2）、WebGL 渲染、30+ 主题 + 自定义编辑器、命令面板（`⌘K`）、禅模式 |
| **SSH 与认证** | 连接池与多路复用、ProxyJump（无限跳数）拓扑图、宽限期自动重连、Agent 转发。认证方式：密码、SSH 密钥（RSA/Ed25519/ECDSA）、SSH Agent、证书、keyboard-interactive 2FA、Known Hosts TOFU |
| **SFTP** | 双窗格浏览器、拖放操作、智能预览（图片/视频/音频/代码/PDF/十六进制/字体）、带进度和预计到达时间的传输队列、书签、压缩包解压 |
| **IDE 模式** | CodeMirror 6 支持 30+ 语言、文件树 + Git 状态、多标签页、冲突解决、集成终端。可选 Linux 远程 Agent（9 种额外架构） |
| **端口转发** | 本地（-L）、远程（-R）、动态 SOCKS5（-D）、无锁消息传递 I/O、重连自动恢复、终止报告、空闲超时 |
| **AI（OxideSens）** | 内联面板（`⌘I`）+ 侧边栏聊天、终端缓冲区捕获（单窗格/所有窗格）、多源上下文（IDE/SFTP/Git）、40+ 自主工具、MCP 服务器集成、RAG 知识库（BM25 + 向量混合搜索）、SSE 流式输出 |
| **插件** | 运行时 ESM 加载、18 个 API 命名空间、24 个 UI Kit 组件、冻结 API + Proxy ACL、熔断器、错误时自动禁用 |
| **CLI** | `oxt` 伴侣工具：JSON-RPC 2.0 基于 Unix Socket / Named Pipe、`status`/`list`/`ping`、人类可读 + JSON 输出 |
| **安全** | .oxide 加密导出（ChaCha20-Poly1305 + Argon2id 256 MB）、OS 密钥链、Touch ID（macOS）、主机密钥 TOFU、`zeroize` 内存清除 |
| **国际化** | 11 种语言：EN、简体中文、繁體中文、日本語、한국어、FR、DE、ES、IT、PT-BR、VI |

---

## 技术内幕

### 架构——双平面通信

OxideTerm 将终端数据与控制命令分离为两个独立平面：

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│  xterm.js 6 (WebGL) + 19 stores    │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binary)
           │ (JSON)       │ per-session port
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│  NodeRouter → SshConnectionRegistry │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]       │
└─────────────────────────────────────┘
```

- **数据平面（WebSocket）**：每个 SSH 会话获得独立的 WebSocket 端口。终端字节以带有 Type-Length-Payload 头的二进制帧传输——无 JSON 序列化、无 Base64 编码，热路径零开销。
- **控制平面（Tauri IPC）**：连接管理、SFTP 操作、转发、配置——结构化 JSON，但不在关键路径上。
- **Node 优先寻址**：前端从不直接触及 `sessionId` 或 `connectionId`。一切通过 `nodeId` 寻址，由 `NodeRouter` 在服务端原子解析。SSH 重连会更换底层 `connectionId`——但 SFTP、IDE 和转发完全不受影响。

### 🔩 纯 Rust SSH — russh 0.59

整个 SSH 协议栈使用 **russh 0.59**，基于 **`ring`** 加密后端编译：

- **零 C/OpenSSL 依赖**——完整的加密栈由 Rust 实现，告别"哪个 OpenSSL 版本？"的调试噩梦。
- 完整的 SSH2 协议：密钥交换、通道、SFTP 子系统、端口转发
- ChaCha20-Poly1305 和 AES-GCM 加密套件，Ed25519/RSA/ECDSA 密钥
- 自定义 **`AgentSigner`**：封装系统 SSH Agent 并实现 russh 的 `Signer` trait，通过在 `.await` 前将 `&AgentIdentity` 克隆为 owned 值，解决 RPITIT `Send` 约束问题

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **平台支持**：Unix（`SSH_AUTH_SOCK`）、Windows（`\\.\pipe\openssh-ssh-agent`）
- **代理链**：每一跳独立使用 Agent 认证
- **重连**：`AuthMethod::Agent` 自动重放

### 🔄 智能重连与宽限期

大多数 SSH 客户端在断线时会销毁一切然后从头开始。OxideTerm 的重连编排器采用了截然不同的策略：

1. **检测**：WebSocket 心跳超时（300 秒，针对 macOS App Nap 和 JS 定时器节流优化）
2. **快照**：完整状态——终端窗格、进行中的 SFTP 传输、活动端口转发、打开的 IDE 文件
3. **智能探测**：`visibilitychange` + `online` 事件触发主动 SSH keepalive（~2 秒检测 vs 被动超时的 15-30 秒）
4. **宽限期**（30 秒）：通过 keepalive 探测旧 SSH 连接——如果恢复成功（例如 WiFi AP 切换），你的 TUI 应用（vim、htop、yazi）完全不受影响
5. 恢复失败 → 建立新 SSH 连接 → 自动恢复转发 → 恢复 SFTP 传输 → 重新打开 IDE 文件

管线流程：`queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

所有逻辑运行在专用的 `ReconnectOrchestratorStore` 中——零重连代码散落在 hooks 或组件中。

### 🛡️ SSH 连接池

引用计数的 `SshConnectionRegistry`，以 `DashMap` 为底层实现无锁并发访问：

- **一个连接，多个消费者**：终端、SFTP、端口转发和 IDE 共享同一物理 SSH 连接——无冗余 TCP 握手
- **每连接状态机**：`connecting → active → idle → link_down → reconnecting`
- **生命周期管理**：可配置的空闲超时（5 分钟 / 15 分钟 / 30 分钟 / 1 小时 / 永不）、15 秒 keepalive 间隔、心跳故障检测
- **WsBridge 心跳**：30 秒间隔、5 分钟超时——兼容 macOS App Nap 和浏览器 JS 节流
- **级联传播**：跳板机故障 → 所有下游节点自动标记为 `link_down` 并同步状态
- **空闲断开**：向前端发送 `connection_status_changed`（而非仅内部 `node:state`），防止 UI 状态不同步

### 🤖 OxideSens AI

隐私优先的 AI 助手，提供双重交互模式：

- **内联面板**（`⌘I`）：快速终端命令，通过 bracketed paste 注入输出
- **侧边栏聊天**：持久对话，完整历史记录
- **上下文捕获**：Terminal Registry 从活动窗格或所有分屏窗格同时采集缓冲区；自动注入 IDE 文件、SFTP 路径和 Git 状态
- **40+ 自主工具**：文件操作、进程管理、网络诊断、TUI 应用交互、文本处理——AI 无需手动触发即可调用
- **MCP 支持**：连接外部 [Model Context Protocol](https://modelcontextprotocol.io) 服务器（stdio & SSE）进行第三方工具集成
- **RAG 知识库**（v0.20）：将 Markdown/TXT 文档导入作用域集合（全局或按连接）。混合搜索通过 Reciprocal Rank Fusion 融合 BM25 关键词索引 + 向量余弦相似度。Markdown 感知分块保留标题层级。CJK 双字符分词器支持中文/日文/韩文。
- **供应商**：OpenAI、Ollama、DeepSeek、OneAPI 或任何 `/v1/chat/completions` 端点
- **安全**：API 密钥存储在 OS 密钥链；macOS 上密钥读取受 **Touch ID** 通过 `LAContext` 保护——无需授权签名或代码签名，每次会话首次认证后缓存

### 💻 IDE 模式——远程编辑

CodeMirror 6 编辑器基于 SFTP 运行——默认无需服务端安装：

- **文件树**：延迟加载目录，带 Git 状态指示器（已修改/未跟踪/已添加）
- **24 语言模式**：14 种原生 CodeMirror + 通过 `@codemirror/legacy-modes` 提供的传统模式
- **冲突解决**：乐观 mtime 锁定——覆盖前检测远端变更
- **事件驱动 Git**：保存、创建、删除、重命名及终端回车按键时自动刷新
- **状态门控**：当 `readiness !== 'ready'` 时阻止所有 IO，Key-Driven Reset 在重连时强制完整重载
- **远程 Agent**（可选）：~1 MB Rust 二进制文件，在 x86_64/aarch64 Linux 上自动部署。9 种额外架构（ARMv7、RISC-V64、LoongArch64、s390x、Power64LE、i686、ARM、Android aarch64、FreeBSD x86_64）位于 `agents/extra/`，可手动上传。提供增强文件树、符号搜索和文件监视功能。

### 🔀 端口转发——无锁 I/O

完整的本地（-L）、远程（-R）和动态 SOCKS5（-D）转发：

- **消息传递架构**：SSH Channel 由单一 `ssh_io` 任务拥有——无 `Arc<Mutex<Channel>>`，彻底消除互斥锁竞争
- **终止报告**：转发任务主动报告退出原因（SSH 断开、远端端口关闭、超时），提供清晰的诊断信息
- **自动恢复**：`Suspended` 状态的转发在重连时自动恢复，无需用户干预
- **空闲超时**：`FORWARD_IDLE_TIMEOUT`（300 秒）防止僵尸连接堆积

### 🔌 运行时插件系统

动态 ESM 加载，安全加固的冻结 API 表面：

- **PluginContext API**：18 个命名空间——terminal、ui、commands、settings、lifecycle、events、storage、system
- **24 个 UI Kit 组件**：预构建的 React 组件（按钮、输入框、对话框、表格……）通过 `window.__OXIDE__` 注入插件沙箱
- **安全膜**：对所有上下文对象使用 `Object.freeze`，基于 Proxy 的 ACL，IPC 白名单，熔断器在重复错误后自动禁用
- **共享模块**：React、ReactDOM、zustand、lucide-react 对外暴露供插件使用，无需重复打包

### ⚡ 自适应渲染

三级渲染调度器，替代固定的 `requestAnimationFrame` 批处理：

| 级别 | 触发条件 | 帧率 | 收益 |
|---|---|---|---|
| **加速** | 帧数据 ≥ 4 KB | 120 Hz+（ProMotion 原生） | 消除 `cat largefile.log` 时的滚动卡顿 |
| **正常** | 常规打字 | 60 Hz（RAF） | 平稳的基准表现 |
| **空闲** | 3 秒无 I/O / 标签页隐藏 | 1–15 Hz（指数退避） | 接近零 GPU 负载，节省电量 |

级别切换完全自动——由数据量、用户输入和 Page Visibility API 驱动。后台标签页通过空闲定时器持续刷新数据，无需唤醒 RAF。

### 🔐 .oxide 加密导出

便携、防篡改的连接备份：

- **ChaCha20-Poly1305 AEAD** 认证加密
- **Argon2id KDF**：256 MB 内存开销、4 次迭代——抵御 GPU 暴力破解
- **SHA-256** 完整性校验
- **可选密钥嵌入**：私钥 base64 编码嵌入加密载荷
- **导出前分析**：认证类型分类、缺失密钥检测

### 📡 ProxyJump——拓扑感知多跳

- 无限链深度：`Client → Jump A → Jump B → … → Target`
- 自动解析 `~/.ssh/config`，构建拓扑图，Dijkstra 最短路径寻路
- 跳板节点可作为独立会话复用
- 级联故障传播：跳板机宕机 → 所有下游节点自动标记为 `link_down`

### ⚙️ 本地终端——线程安全 PTY

跨平台本地 Shell，基于 `portable-pty 0.8`，通过 `local-terminal` feature flag 控制：

- `MasterPty` 封装在 `std::sync::Mutex` 中——专用 I/O 线程将阻塞式 PTY 读取隔离在 Tokio 事件循环之外
- Shell 自动检测：`zsh`、`bash`、`fish`、`pwsh`、Git Bash、WSL2
- `cargo build --no-default-features` 可剥离 PTY 功能用于移动端/轻量构建

### 🪟 Windows 优化

- **原生 ConPTY**：直接调用 Windows Pseudo Console API——完整 TrueColor 和 ANSI 支持，无传统 WinPTY
- **Shell 扫描器**：通过注册表和 PATH 自动检测 PowerShell 7、Git Bash、WSL2、CMD

### 更多功能

- **资源分析器**：通过持久 SSH 通道读取 `/proc/stat` 获取实时 CPU/内存/网络数据，基于增量计算，非 Linux 环境自动降级为仅 RTT
- **自定义主题引擎**：30+ 内置主题，可视化编辑器实时预览，20 个 xterm.js 字段 + 24 个 UI 颜色变量，从终端调色板自动推导 UI 颜色
- **会话录制**：asciicast v2 格式，完整录制和回放
- **广播输入**：输入一次，发送到所有分屏窗格——批量服务器操作
- **背景画廊**：每标签页背景图片，16 种标签类型，透明度/模糊/适配控制
- **CLI 伴侣工具**（`oxt`）：~1 MB 二进制文件，JSON-RPC 2.0 基于 Unix Socket / Named Pipe，`status`/`list`/`ping` 支持人类可读或 `--json` 输出
- **WSL Graphics** ⚠️ 实验性：内置 VNC 查看器——9 种桌面环境 + 单应用模式，WSLg 检测，Xtigervnc + noVNC

<details>
<summary>📸 11 种语言实际展示</summary>
<br>
<table>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/en.png" width="280"><br><b>English</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/zhHans.png" width="280"><br><b>简体中文</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/zhHant.png" width="280"><br><b>繁體中文</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/ja.png" width="280"><br><b>日本語</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/ko.png" width="280"><br><b>한국어</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/fr.png" width="280"><br><b>Français</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/de.png" width="280"><br><b>Deutsch</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/es.png" width="280"><br><b>Español</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/it.png" width="280"><br><b>Italiano</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/pt-BR.png" width="280"><br><b>Português</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/vi.png" width="280"><br><b>Tiếng Việt</b></td>
    <td></td>
  </tr>
</table>
</details>

---

## 快速开始

### 前置要求

- **Rust** 1.85+
- **Node.js** 18+（推荐 pnpm）
- **平台工具**：
  - macOS：Xcode Command Line Tools
  - Windows：Visual Studio C++ Build Tools
  - Linux：`build-essential`、`libwebkit2gtk-4.1-dev`、`libssl-dev`

### 开发

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# 构建 CLI 伴侣工具（CLI 功能必需）
pnpm cli:build

# 完整应用（前端 + Rust 后端，热重载）
pnpm run tauri dev

# 仅前端（Vite 运行在端口 1420）
pnpm dev

# 生产构建
pnpm run tauri build
```

---

## 技术栈

| 层级 | 技术 | 详情 |
|---|---|---|
| **框架** | Tauri 2.0 | 原生二进制，25–40 MB |
| **运行时** | Tokio + DashMap 6 | 全异步，无锁并发映射 |
| **SSH** | russh 0.59（`ring`） | 纯 Rust，零 C 依赖，SSH Agent |
| **本地 PTY** | portable-pty 0.8 | Feature 门控，Windows 上使用 ConPTY |
| **前端** | React 19.1 + TypeScript 5.8 | Vite 7，Tailwind CSS 4 |
| **状态** | Zustand 5 | 19 个专用 Store |
| **终端** | xterm.js 6 + WebGL | GPU 加速，60fps+ |
| **编辑器** | CodeMirror 6 | 30+ 语言模式 |
| **加密** | ChaCha20-Poly1305 + Argon2id | AEAD + 内存硬化 KDF（256 MB） |
| **存储** | redb 2.1 | 嵌入式 KV 存储 |
| **国际化** | i18next 25 | 11 种语言 × 22 个命名空间 |
| **插件** | ESM 运行时 | 冻结 PluginContext + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## 安全

| 关注点 | 实现方式 |
|---|---|
| **密码** | OS 密钥链（macOS Keychain / Windows Credential Manager / libsecret） |
| **AI API 密钥** | OS 密钥链 + macOS 上的 Touch ID 生物识别保护 |
| **导出** | .oxide：ChaCha20-Poly1305 + Argon2id（256 MB 内存，4 次迭代） |
| **内存** | Rust 内存安全 + `zeroize` 敏感数据清除 |
| **主机密钥** | TOFU 验证 `~/.ssh/known_hosts`，拒绝变更（防中间人攻击） |
| **插件** | Object.freeze + Proxy ACL，熔断器，IPC 白名单 |
| **WebSocket** | 一次性令牌，带时间限制 |

---

## 路线图

- [x] SSH Agent 转发
- [ ] 插件市场
- [ ] 会话搜索与快速切换

---

## 许可证

**GPL-3.0** — 本软件是按照 [GNU 通用公共许可证 v3.0](https://www.gnu.org/licenses/gpl-3.0.html) 发布的自由软件。

您可以在 GPL-3.0 条款下自由地使用、修改和分发本软件。任何衍生作品也必须在由同一许可证下发布。

完整文本：[GNU 通用公共许可证 v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## 致谢

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>236,000+ 行 Rust 与 TypeScript 代码——以 ⚡ 和 ☕ 构建</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
