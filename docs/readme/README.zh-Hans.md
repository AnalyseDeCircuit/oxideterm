<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>OxideTerm 的下一代零 WebView 版本。</strong>
  <br>
  连接一次远程机器，就能在一个原生 Rust 工作区里处理它的 Shell、文件、端口、传输、轻量编辑器、串口控制台和 BYOK AI。
  <br>
  原生 GPUI 应用 · 纯 Rust SSH · 核心 SSH 工作流无需账号
  <br>
  <strong>零 WebView。零 OpenSSL。零遥测。零订阅。BYOK 优先。全栈纯 Rust。</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="版本">
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

> **发布状态：** OxideTerm Native 正在作为 OxideTerm 的下一代主版本准备中。公开安装包尚未发布，目前请从源码运行；在 native 安装包准备好之前，当前打包发布仍在 Tauri 版本线上。

## 你可以做什么

- 在一个原生工作区里管理 SSH 终端、SFTP、端口转发、串口控制台、本地 Shell 和轻量编辑
- 通过宽限期重连，让远程工作更能扛住网络抖动
- 使用你自己的 AI 提供商检查实时会话，并执行经过批准的工作区操作

---

## 为什么选择 Native？

| 如果你在意... | OxideTerm Native 提供... |
|---|---|
| 一个远程节点，多种工具 | 终端、SFTP、端口转发、trzsz、原生 IDE、监控和 AI 上下文都挂在同一个 SSH 工作区上 |
| 零 WebView 原生外壳 | GPUI 直接在 GPU surface 上绘制桌面 UI，没有 DOM、CSS、JavaScript、Chromium 或 WebKit 运行时 |
| 本地优先 SSH 工作流 | SSH、SFTP、端口转发、本地 Shell、串口终端和配置管理都无需注册 |
| BYOK AI，而不是平台点数 | OxideSens 使用你自己的 OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible 端点，并支持 MCP 与 RAG |
| 重连稳定性 | 宽限期会先探测旧连接 30 秒再替换它，短暂网络中断时 TUI 应用仍有机会存活 |
| 纯 Rust SSH 与凭证安全 | `russh` + `ring`，无 OpenSSL/libssh2；密码和 API 密钥保存在 OS Keychain，`.oxide` 使用 ChaCha20-Poly1305 + Argon2id |

## 它是什么 / 不是什么

OxideTerm Native 专注于和 OxideTerm 相同的**本地优先 SSH 工作区**，只是重建为纯 Rust GPUI 桌面应用。它面向希望终端、文件、端口、传输、轻量编辑、串口控制台和 AI 上下文围绕自己的机器与远程节点展开的用户。

它还不是当前稳定下载线，也不是托管云端 Agent 平台。它也不是 Electron、Tauri 或网页终端：没有 Chromium、WebView、JavaScript 或 CSS。

---

## 截图

Native UI 遵循当前 Tauri 版本相同的 OxideTerm 工作区模型与视觉语言。

<table>
<tr>
<td align="center"><strong>SSH 终端 + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="带 OxideSens AI 侧边栏的 SSH 终端" /></td>
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

### 纯 Rust SSH、智能重连与连接池

原生版本直接链接与 Tauri 版本同源的 `russh` 栈：无 C/OpenSSL 依赖，支持 SSH2、SFTP、端口转发、Agent、ProxyJump 和多种密钥算法。重连流程会快照终端、SFTP、转发和 IDE 状态，先给旧连接 30 秒 Grace Period，再必要时重建并恢复工作区。

</details>

---

## 从源码运行

公开 native 安装包尚未发布。在打包构建准备好之前，请从源码运行 native 版本。

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
- [ ] 公开打包安装包
- [ ] 完整 ProxyCommand、审计日志

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

OxideTerm Native 正在作为下一代 OxideTerm 主版本准备中，并以**尽力而为**的方式维护。带有可复现步骤和脱敏诊断信息的 bug 报告会优先处理；功能请求不一定都会实现。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

如果 OxideTerm 帮助了你的工作流，GitHub Star、问题复现、翻译修正、插件或 PR 都能让项目更容易继续推进。

---

## 许可证与致谢

**GPL-3.0-only**。第三方声明记录在 `NOTICE`。感谢 `russh`、`GPUI`、`alacritty_terminal`、`portable-pty`、`wasmtime` 和 `tree-sitter`。
