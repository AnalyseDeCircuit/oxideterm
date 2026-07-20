<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>面向远程服务器、带 AI 能力的原生运维工作区 —— 纯 Rust 原生应用</strong>
  <br>
  SSH、Telnet、串口、RDP/VNC、SFTP、端口转发 和轻量编辑，集中在一个原生工作区。
  <br>
  GPU 直接渲染。免费，无需注册。
  <br>
  <strong>不使用 Electron。 不捆绑 WebView。不采集遥测。无需订阅。BYOK 优先。纯 Rust SSH，不依赖 OpenSSL/libssh2。</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.8-blue" alt="版本">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="平台">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="许可证">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>开源、本地优先，使用 GPUI 进行 GPU 渲染。</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm 功能概览" width="920">
</p>

---

## OxideTerm 是什么

OxideTerm 是面向 SSH 与远程运维的开源工作区。终端、文件、端口转发、主机工具和远程桌面都集中在同一个工作区中。

**你可以做什么：**

- 在同一个工作区中管理 SSH、Telnet、串口、RDP/VNC、SFTP、端口转发、本地 Shell 与轻量编辑
- 通过 Grace Period 重连机制，应对短暂网络中断并维持远程工作
- 使用你自己的 AI 服务商，让 OxideSens 检查活动会话并执行经过批准的工作区操作

连接信息与运维数据始终由你掌控。OxideSens 使用你自己的 AI 服务商，无需注册账户。

---

## 为什么选择 OxideTerm？

- 在一个桌面应用中管理 SSH、Telnet、串口、RDP/VNC、SFTP、端口转发与本地 Shell
- Grace Period 重连机制可应对短暂网络中断
- OxideSens 使用你自己的 AI 凭据，并由你批准工作区操作
- GPUI 原生界面，不使用 Electron，也不捆绑浏览器运行时

---

## 截图

以下截图展示了 OxideTerm 的终端、文件、编辑与端口转发工作流。

<table>
<tr>
<td align="center"><strong>SSH 终端 + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="带 OxideSens AI 的 SSH 终端" /></td>
<td align="center"><strong>SFTP 文件管理器</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="SFTP 双窗格文件管理器与传输队列" /></td>
</tr>
<tr>
<td align="center"><strong>内置 IDE</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="内置 IDE 模式" /></td>
<td align="center"><strong>智能端口转发</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="带自动检测的智能端口转发" /></td>
</tr>
</table>

---

## 为远程运维而设计

OxideTerm 将连接、文件、转发、主机工具、自动化与 AI 上下文放在同一个 Rust 工作区中。各项工具共享同一台服务器的身份与会话生命周期。

---

## 功能

| 类别 | 功能 |
|---|---|
| **终端与连接** | 本地 Shell、SSH、Telnet、串口、分屏、多跳路由与稳定重连 |
| **文件与远程编辑** | SFTP、传输队列、收藏夹、安全写入、项目树与多标签编辑 |
| **转发与网络** | 本地、远程与动态 SOCKS5 转发、已保存规则与 Socket 调试 |
| **主机运维与远程桌面** | 监控、进程、服务、日志、端口、任务、磁盘、软件包、容器、tmux、RDP 与 VNC |
| **OxideSens 与自动化** | 自有 AI 服务商、MCP、本地 RAG、已批准操作、加密云同步与 CLI |
| **扩展与个性化** | WASM 插件、自定义标签页、快速命令、主题、背景图片、快捷键与 11 种界面语言 |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens 在 OxideTerm 中打开终端" width="920">
</a>

*观看 OxideSens 按照用户请求在 OxideTerm 中打开一个终端。*

</div>

---

## 内部实现

OxideTerm 将终端、SSH、Telnet、RDP、VNC、SFTP、转发、IDE、AI、插件和 CLI 整合在同一套 Rust 架构中。下方列出了面向开发者的技术细节。

<details>
<summary><strong>架构、SSH 内部、GPUI 外壳、重连、AI、插件与更多细节</strong></summary>
<br>

### 核心进程内直连，无 WebView 桥接

```text
GPUI 渲染循环
  WorkspaceApp / 标签页界面 / GPUI 视图
        │ 进程内 Arc<> / async
领域 Crate
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

界面与 SSH/终端后端之间没有序列化边界。终端字节直接修改 `TerminalState`，GPUI 读取状态并发出 GPU 绘制命令。

### 纯 Rust SSH — russh (ring)


- **SSH 栈不依赖 OpenSSL/libssh2**：SSH 密码学能力由 `ring` 提供
- 完整 SSH2：密钥交换、channel、SFTP 子系统和端口转发
- ChaCha20-Poly1305 / AES-GCM、Ed25519/RSA/ECDSA 密钥
- SSH Agent：Unix `SSH_AUTH_SOCK` 与 Windows `\\.\pipe\openssh-ssh-agent`
- 多跳 ProxyJump，每一跳独立认证

### Grace Period 智能重连


1. 通过 SSH keepalive 检测连接超时，没有 JavaScript timer throttle
2. 快照终端 pane、SFTP 传输、端口转发和 IDE 文件状态
3. **Grace Period**：先探测旧连接 30 秒，网络切换时 TUI 应用有机会原地存活
4. 旧连接无法恢复时，新 SSH 连接会恢复转发、续传并重新打开 IDE 文件

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH 连接池与节点路由


- 一个物理 SSH 连接可被 terminal、SFTP、port forward 和 IDE 同时消费
- 每条连接都有 `connecting → active → idle → link_down → reconnecting` 状态机
- UI 只按 `nodeId` 操作，`NodeRouter` 原子解析到底层 `connectionId`
- `NodeRuntimeStore` 将节点拓扑快照持久化到 `session_tree.json`
- 跳板机失效会级联标记下游节点为 `link_down`

### OxideSens AI

OxideSens 采用 BYOK 模式，并在进程内构建上下文：

- 提供商：OpenAI、Anthropic、Gemini、Ollama 或任意 OpenAI 兼容端点
- MCP：stdio 与 SSE 传输，支持工具发现与调用
- RAG：BM25 全文检索、HNSW 向量索引、RRF 融合与 CJK 双字词分词器
- 发往提供商的消息会过滤凭证模式；工作区上下文与操作仍由用户控制
- API 密钥存入系统钥匙串，并明确排除在结构化日志和桌面核心消息负载之外

### GPUI 桌面外壳

整个 UI 使用 GPUI 直接绘制，没有 DOM/CSS/JavaScript 渲染管线：

- 工作区标签类型：本地终端、SSH、Telnet、串口、RDP、VNC、SFTP、IDE、Forwards、Settings、Plugin、Topology 等
- 二叉窗格树与可拖拽分隔条，每个终端标签最多 4 个窗格
- 命令面板、全局快捷键和侧边栏都使用 GPUI primitive
- 即时模式渲染直接响应 Rust 状态变化，无需序列化往返

### 终端状态与渲染

终端渲染先建模为 Rust 状态，再由 GPUI 绘制：

- PTY 输出进入 `TerminalState`；scrollback、光标、选择区、marks 和搜索状态都保留在 Rust 中
- 渲染策略可在 Boost、Normal、Idle 之间切换，不需要等待浏览器事件循环配合
- Sixel 与 Kitty graphics 作为终端持有的资源跟踪，而不是 DOM node 或 canvas overlay
- 分屏窗格共享同一套工作区状态，标签恢复与重连可以一起快照终端拓扑

### SFTP 与 IDE 工作区

远程文件属于同一个 node 工作区，而不是割裂的附属功能：

- SFTP session 通过 `NodeRouter` 解析，重连替换底层 SSH 连接时 UI 的 node address 不变
- 传输队列独立跟踪方向、进度、重试状态和限速，不依赖当前可见文件 pane
- IDE 标签页同时保存未保存缓冲区、远程路径、冲突状态和恢复元数据
- 后端支持时，远程写入走 staged/atomic 行为，减少普通编辑流程里的半写入文件

### 插件、CLI 与诊断

原生分支把扩展和支持功能保持在 Rust 原生边界内：

- 插件运行在 wasmtime 沙箱中，使用类型化宿主能力，而不是浏览器全局对象
- CLI 直接链接领域 crate，覆盖 doctor、settings、connections、forwards、便携包、备份和报告
- 诊断优先输出计数、路径、功能标志与脱敏提示，避免暴露带秘密的原始 payload
- 会修改状态的 CLI 流程使用 dry-run 计划、`--yes` guards 和回滚备份

### 端口转发 — 无锁 I/O


- Local `-L`、Remote `-R`、Dynamic SOCKS5 `-D`
- SSH Channel 由单一 `ssh_io` task 持有，避免 `Arc<Mutex<Channel>>`
- 支持重连自动恢复、死亡上报和空闲超时

### trzsz 内联文件传输

trzsz 继续走终端数据流，不需要额外端口或远端 agent：

- 上传/下载复用已有 terminal stream
- 可穿透 ProxyJump 链路
- 原生文件选择器不受浏览器内存限制
- 支持双向传输、目录传输和可配置限制

### `.oxide` 加密导出


- **ChaCha20-Poly1305 AEAD** 认证加密
- **Argon2id KDF**：256 MB memory cost、4 iterations，提升 GPU 暴力破解成本
- 覆盖 connections、forwards、settings、快捷命令、插件设置和便携密钥

</details>

---

## 从源码运行

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build/build-cli.sh
./scripts/build/build-agent.sh
```

CLI 二进制输出到 `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`。

## CLI

```sh
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
cargo run -p oxideterm-cli -- --config-dir ./fixture-config doctor --strict
```

## 技术栈

| 层级 | 技术 | 说明 |
|---|---|---|
| 界面框架 | GPUI（Zed） | GPU 加速的即时模式界面，纯 Rust 实现 |
| 运行时 | Tokio + DashMap | 异步运行时与并发映射 |
| SSH | russh（`ring`） | SSH 栈不依赖 OpenSSL/libssh2，支持 SSH Agent |
| 终端 | portable-pty + alacritty_terminal | 本地伪终端、终端模拟与 Sixel/Kitty 图形 |
| 插件 | wasmtime | WASM 隔离与原生宿主 API |
| AI 与检索 | SSE + BM25 + HNSW | 提供商流式传输、CJK 双字词与 RRF 融合 |

## 安全

| 关注点 | 实现 |
|---|---|
| 已存储凭证 | macOS 钥匙串 / Windows Credential Manager / libsecret |
| 内存中的秘密 | 持有秘密的类型和临时缓冲区在支持的所有权边界使用 `zeroize` / `Zeroizing` |
| 诊断 | 支持报告优先输出结构化元数据和脱敏提示，避免携带秘密的原始负载 |
| AI 上下文 | 发往提供商的消息会过滤凭证模式；工作区上下文与操作仍由用户控制 |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI 写操作 | dry-run 计划、`--yes` 保护和回滚备份 |
| 插件 | wasmtime 隔离与基于能力的宿主 API |

## 合法使用提醒

OxideTerm 按 GPL-3.0-only 许可发布，不附加额外的许可证限制。使用时，请仅访问你拥有或已获得明确授权的系统、网络和设备，并遵守适用法律。请勿使用 OxideTerm 实施未经授权的访问、服务干扰或访问控制规避。

## 贡献

欢迎贡献代码、文档、翻译、插件、测试与问题复现。较大的改动请先通过 Issue 讨论，范围明确的修复可以直接提交 Pull Request。

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## 支持与维护

带有可复现步骤和脱敏诊断信息的 bug 报告与回归问题会优先处理。功能请求会根据范围、安全性以及是否符合 OxideTerm 的远程服务器工作区方向来评估。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

如果 OxideTerm 帮助了你的工作流，GitHub Star、问题复现、翻译修正、插件或 PR 都能让项目更容易继续推进。

---

## 许可证

**GPL-3.0-only**。详细的第三方声明见 [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md)，其他通知见 [`NOTICE`](../../NOTICE)。

## 致谢

感谢 `russh`、`GPUI`、`alacritty_terminal`、`portable-pty`、`wasmtime` 和 `tree-sitter`。
