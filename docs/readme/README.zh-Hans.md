<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>面向远程服务器的 AI-native 工作区。</strong>
  <br>
  通过 SSH 连接你的服务器，然后在一个本地优先的原生应用里使用终端、文件、端口、传输、轻量编辑、串口控制台和 OxideSens AI。
  <br>
  原生 GPUI 应用 · 纯 Rust SSH · BYOK OxideSens AI · 核心 SSH 工作流无需账号
  <br>
  <strong>零 WebView。零 OpenSSL。零遥测。零订阅。BYOK 优先。全栈纯 Rust。</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.7-blue" alt="版本">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="平台">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="许可证">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub><a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> 的下一代原生版本 —— GPU 渲染、零 WebView，使用 <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>（Zed 的渲染框架）</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens 在 OxideTerm 中打开终端" width="920">
</a>

*观看 OxideSens 按照用户请求在 OxideTerm 中打开一个终端。*

</div>

---

## 你可以做什么

- 在一个原生工作区里管理 SSH 终端、SFTP、端口转发、串口控制台、本地 Shell 和轻量编辑
- 通过宽限期重连，让远程工作更能扛住网络抖动
- 让 OxideSens AI通过你自己的 AI 提供商检查实时会话，并执行经过批准的工作区操作

---

## 为什么选择 Native？

| 如果你在意... | OxideTerm Native 提供... |
|---|---|
| 一个远程节点，多种工具 | 终端、SFTP、端口转发、trzsz、原生 IDE、监控和 OxideSens AI都挂在同一个 SSH 工作区上 |
| 零 WebView 原生外壳 | GPUI 直接在 GPU surface 上绘制桌面 UI，没有 DOM、CSS、JavaScript、Chromium 或 WebKit 运行时 |
| 本地优先 SSH 工作流 | SSH、SFTP、端口转发、本地 Shell、串口终端和配置管理都无需注册 |
| BYOK OxideSens AI，而不是平台点数 | OxideSens 使用你自己的 OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible 端点，支持 MCP、RAG 和经过批准的工作区操作 |
| 重连稳定性 | 宽限期会先探测旧连接 30 秒再替换它，短暂网络中断时 TUI 应用仍有机会存活 |
| 纯 Rust SSH 与凭证安全 | `russh` + `ring`，无 OpenSSL/libssh2；密码和 API 密钥保存在 OS Keychain，`.oxide` 使用 ChaCha20-Poly1305 + Argon2id |

## 它是什么 / 不是什么

OxideTerm Native 专注于**面向远程服务器的本地优先 AI 工作区**，并重建为纯 Rust GPUI 桌面应用。它面向希望终端、文件、端口、传输、轻量编辑、串口控制台和 BYOK OxideSens AI围绕自己的机器与远程节点展开的用户。

它还不是当前稳定下载线，也不是托管云端 Agent 平台。它也不是 Electron、Tauri 或网页终端：没有 Chromium、WebView、JavaScript 或 CSS。

---

## 截图

Native UI 遵循当前 Tauri 版本相同的 OxideTerm 工作区模型与视觉语言。

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

## 与 WebView 版本的区别

| 方面 | WebView/Tauri | Native |
|---|---|---|
| 渲染 | Chromium/Safari/WebKit2GTK + CSS | GPUI GPU surface，即时模式，纯 Rust |
| 终端数据流 | WebSocket → JS 事件循环 → xterm.js | Rust 输入 → `TerminalState` → GPUI 渲染 |
| IPC 开销 | 每次命令都要 JSON-RPC | 进程内函数调用 |
| SSH keepalive | JavaScript 定时器 | Rust async task |
| 插件运行时 | 浏览器沙箱中的 ESM | wasmtime WASM + 类型化 Rust Host API |
| CLI | 依赖桌面应用运行 | 独立二进制，直接链接 crate |
| 分发包体积 | 通常约 150–200 MB 安装包 | 当前 macOS arm64：压缩 portable/DMG 约 50–60 MB；裸 release 二进制约 132 MB |

## 功能概览

| 分类 | 功能 |
|---|---|
| 终端 | 本地 PTY、SSH、本地串口终端、分屏、shell integration、命令标记、asciicast 录制/回放、trzsz、Sixel/Kitty 图形、渲染策略 |
| SSH 与认证 | 连接池、无限 ProxyJump、Grace Period 重连、Host-key TOFU、SSH Agent 转发、密码/密钥/证书/键盘交互认证 |
| SFTP / IDE | 双栏浏览器、传输队列、预览、书签、原子写入、远程文件树、多标签编辑、冲突处理 |
| 转发 | Local、Remote、Dynamic SOCKS5，保存规则，重连恢复，死亡报告，空闲超时 |
| AI | OxideSens 支持 OpenAI、Anthropic、Gemini、Ollama/兼容端点、MCP、RAG、命令审批 |
| 云同步与 `.oxide` | push/pull/apply/resolve，S3/WebDAV/Git，回滚备份；加密导入导出连接、转发、设置、快捷命令和插件设置 |
| 插件与 CLI | WASM 沙箱、native host API、插件设置；CLI 含 settings、connections、forwards、quick-commands、plugins、secrets、cloud-sync、backup、report 等命令 |

## 内部实现

OxideTerm Native 移除了 WebView 桥接，并把终端、SSH、SFTP、转发、IDE、AI、插件和 CLI 保持在一套 Rust 原生架构中。完整实现细节保留在下方，方便需要工程细节的读者展开查看。

<details>
<summary><strong>架构、SSH 内部、GPUI 外壳、重连、AI、插件与更多细节</strong></summary>
<br>

### 单进程，零桥接

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ in-process Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

UI 与 SSH/终端后端之间没有序列化边界。终端字节直接修改 `TerminalState`，GPUI 读取状态并发出 GPU draw call。

### 纯 Rust SSH — russh (ring)

原生版本把与 Tauri 版本同源的 `russh` 栈直接链接进桌面应用二进制：

- **零 C/OpenSSL 依赖**：密码学实现通过 `ring` 完成
- 完整 SSH2：密钥交换、channel、SFTP 子系统和端口转发
- ChaCha20-Poly1305 / AES-GCM、Ed25519/RSA/ECDSA 密钥
- SSH Agent：Unix `SSH_AUTH_SOCK` 与 Windows `\\.\pipe\openssh-ssh-agent`
- 多跳 ProxyJump，每一跳独立认证

### Grace Period 智能重连

重连语义与 Tauri 版本保持一致，但 orchestration 全部在 Rust async 任务内完成：

1. 通过 SSH keepalive 检测连接超时，没有 JavaScript timer throttle
2. 快照终端 pane、SFTP 传输、端口转发和 IDE 文件状态
3. **Grace Period**：先探测旧连接 30 秒，网络切换时 TUI 应用有机会原地存活
4. 旧连接无法恢复时，新 SSH 连接会恢复转发、续传并重新打开 IDE 文件

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH 连接池与节点路由

`SshConnectionRegistry` 使用 `DashMap` 管理连接，沿用 Tauri 的 node-first 架构，但没有 WebSocket 生命周期桥接：

- 一个物理 SSH 连接可被 terminal、SFTP、port forward 和 IDE 同时消费
- 每条连接有 `connecting → active → idle → link_down → reconnecting` 状态机
- UI 只按 `nodeId` 操作，`NodeRouter` 原子解析到底层 `connectionId`
- `NodeRuntimeStore` 将节点拓扑快照持久化到 `session_tree.json`
- Jump host 失败会级联标记下游节点为 `link_down`

### OxideSens AI

OxideSens 仍然是 BYOK-first，native 版本把上下文构建放在进程内完成：

- Provider：OpenAI、Anthropic、Gemini、Ollama 或任意 OpenAI-compatible endpoint
- MCP：stdio 与 SSE transport，支持工具发现与调用
- RAG：BM25 全文检索、HNSW 向量索引、RRF 融合与 CJK bigram tokenizer
- AI 上下文来自当前 workspace state；凭据在发送给 provider 前会被脱敏
- API key 存入 OS Keychain，不写入日志，也不会进入 IPC frame

### GPUI 桌面外壳

整个 UI 使用 GPUI 直接绘制，没有 DOM/CSS/JavaScript 渲染管线：

- 17 类 workspace tab：本地/SSH 终端、SFTP、IDE、Forwards、Settings、Plugin、Topology 等
- 二叉 pane tree 与可拖拽 divider，终端 tab 最多 4 个 pane
- 命令面板、全局快捷键和侧边栏都使用 GPUI primitive
- Immediate-mode rendering 直接响应 Rust state 变化，无序列化 round-trip

### 终端状态与渲染

终端渲染先建模为 Rust 状态，再由 GPUI 绘制：

- PTY 输出进入 `TerminalState`；scrollback、光标、选择区、marks 和搜索状态都保留在 Rust 中
- 渲染策略可在 Boost、Normal、Idle 之间切换，不需要等待浏览器事件循环配合
- Sixel 与 Kitty graphics 作为终端持有的资源跟踪，而不是 DOM node 或 canvas overlay
- Split panes 共享同一套 workspace state，tab restore 与 reconnect 可以一起快照终端拓扑

### SFTP 与 IDE 工作区

远程文件属于同一个 node workspace，而不是割裂的附属功能：

- SFTP session 通过 `NodeRouter` 解析，重连替换底层 SSH 连接时 UI 的 node address 不变
- 传输队列独立跟踪方向、进度、重试状态和限速，不依赖当前可见文件 pane
- IDE tab 同时保存 dirty buffer、remote path、conflict state 和 restore metadata
- 后端支持时，远程写入走 staged/atomic 行为，减少普通编辑流程里的半写入文件

### 插件、CLI 与诊断

Native 分支把扩展和支持面保持在 Rust-native 边界内：

- 插件运行在 wasmtime sandbox 中，使用 typed host capabilities，而不是 browser globals
- CLI 直接链接 domain crates，覆盖 doctor、settings、connections、forwards、portable bundles、backups 和 reports
- 诊断优先输出 counts、paths、feature flags 与 redacted hints，避免暴露带秘密的原始 payload
- 会修改状态的 CLI 流程使用 dry-run plans、`--yes` guards 和 rollback backups

### 端口转发 — Lock-Free I/O

端口转发语义与 Tauri 相同，但独立为 Rust crate：

- Local `-L`、Remote `-R`、Dynamic SOCKS5 `-D`
- SSH Channel 由单一 `ssh_io` task 持有，避免 `Arc<Mutex<Channel>>`
- 支持重连自动恢复、死亡上报和 idle timeout

### trzsz 内联文件传输

trzsz 继续走终端数据流，不需要额外端口或远端 agent：

- 上传/下载复用已有 terminal stream
- 可穿透 ProxyJump 链路
- native 文件选择器不受浏览器内存限制
- 支持双向传输、目录传输和可配置限制

### `.oxide` 加密导出

加密格式与 Tauri 版本保持一致：

- **ChaCha20-Poly1305 AEAD** 认证加密
- **Argon2id KDF**：256 MB memory cost、4 iterations，提升 GPU 暴力破解成本
- 覆盖 connections、forwards、settings、quick commands、plugin settings 和 portable secrets

</details>

---

## 从源码运行

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build-cli.sh
./scripts/build-agent.sh
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

## 安全

| 关注点 | 实现 |
|---|---|
| 密码与密钥 | macOS Keychain / Windows Credential Manager / libsecret |
| 内存中的秘密 | `zeroize` / `Zeroizing` |
| 诊断与 AI 上下文 | 只输出路径、计数、标志和 hint；发送给 AI 前脱敏 |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI 写操作 | dry-run plan、`--yes` 保护和 rollback backup |
| 插件 | wasmtime 隔离与 capability-based host API |

## 发布状态

- [x] SSH Agent 转发、Grace Period 重连、GPUI 桌面 shell
- [x] 无 WebSocket 的进程内终端数据流
- [x] SFTP、端口转发、IDE、AI、云同步、插件、CLI
- [x] 本地串口终端
- [x] 完整 ProxyCommand
- [ ] 审计日志

## Provider 中立性

OxideTerm 是 BYOK 优先，并保持 provider 中立。

Provider 集成是为了让用户连接他们已经信任的工具。它们不是排行榜，不是广告牌，也不是奖励那些最热情开口者的机制。

是否写进文档，取决于兼容性、可维护性、安全性和真实用户价值。可见度跟随有用性，而不是热情程度。

## 贡献

已有 Tauri 功能迁移到 native 时，应保持行为、标签、交互状态和工作流一致，除非明确记录替代设计。新 crate 必须承担真实职责，不能只是 re-export 或堆放函数；按 DTO、验证、持久化、view model、协议 adapter、展示 builder 等能力拆分。

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

## 支持与维护

带有可复现步骤和脱敏诊断信息的 bug 报告与回归问题会优先处理。功能请求会根据范围、安全性以及是否符合 OxideTerm 的远程服务器工作区方向来评估。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

如果 OxideTerm 帮助了你的工作流，GitHub Star、问题复现、翻译修正、插件或 PR 都能让项目更容易继续推进。

---

## 许可证与致谢

**GPL-3.0-only**。第三方声明记录在 `NOTICE`。感谢 `russh`、`GPUI`、`alacritty_terminal`、`portable-pty`、`wasmtime` 和 `tree-sitter`。
