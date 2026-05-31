<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <em>如果你想要一个没有 Electron、WebView、遥测或订阅的本地优先 SSH 工作区，请给 OxideTerm 点个 Star，让更多 SSH 用户发现它。</em>
</p>

<p align="center">
  <strong>本地优先 SSH 工作区：围绕一个远程节点整合 shell、SFTP、端口转发、trzsz、远程编辑和 BYOK AI。</strong>
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
  <sub><a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> 的原生 Rust 重写 —— GPU 渲染、零 WebView，使用 <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>（Zed 的渲染框架）</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

---

## 为什么选择 Native？

| 如果你在意... | OxideTerm Native 提供... |
|---|---|
| SSH 工作区，而不只是 shell | 一个远程节点同时拥有终端、SFTP、端口转发、trzsz、轻量 IDE、监控和 AI 上下文 |
| 本地 shell、串口控制台与远程 SSH 共存 | zsh/bash/fish/pwsh/WSL2、本地串口终端与远程 SSH 在同一工作流中运行 |
| 不需要云账号 | SSH、SFTP、转发、本地 shell 和配置都本地优先 |
| BYOK AI | 使用你自己的 OpenAI、Anthropic、Gemini、Ollama 或兼容端点 |
| 没有 WebView | GPUI 直接绘制 GPU 界面，没有 DOM、CSS、JavaScript |
| 热路径无序列化 | 终端字节直接变更 Rust 状态，无 WebSocket/JSON/Base64 开销 |
| 无 OpenSSL 负担 | `russh` + `ring`，纯 Rust SSH |
| 重连稳定性 | Grace Period 会先探测旧连接，网络抖动时 TUI 应用更容易保活 |
| 远程文件工作 | 内置 SFTP 与原生 IDE 浏览、预览、传输和编辑远程文件 |
| 凭据安全 | OS Keychain；`.oxide` 使用 ChaCha20-Poly1305 + Argon2id 加密 |

## 它是什么 / 不是什么

OxideTerm Native 是一个**纯 Rust 原生桌面 SSH 工作区**。Tauri 版本中的终端、SFTP、转发、编辑、AI、云同步、插件和 CLI 都在 Rust 与 GPUI UI 层中重新实现。

它不是 Electron、Tauri、网页终端或托管服务。没有 Chromium、WebView、JavaScript 或 CSS；所有界面都由 GPUI 直接绘制到 GPU surface。

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

## 快速开始

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

## 路线图

- [x] SSH Agent 转发、Grace Period 重连、GPUI 桌面 shell
- [x] 无 WebSocket 的进程内终端数据流
- [x] SFTP、端口转发、IDE、AI、云同步、插件、CLI
- [ ] 完整 ProxyCommand、审计日志、打包发布构建

## Provider 中立性

OxideTerm 是 BYOK 优先，并保持 provider 中立。

Provider 集成是为了让用户连接他们已经信任的工具。它们不是排行榜，不是广告牌，也不是奖励那些最热情开口者的机制。

是否写进文档，取决于兼容性、可维护性、安全性和真实用户价值。可见度跟随有用性，而不是热情程度。

## 贡献

已有 Tauri 功能迁移到 native 时，应保持行为、标签、交互状态和工作流一致，除非明确记录替代设计。新 crate 必须承担真实职责，不能只是 re-export 或堆放函数；按 DTO、验证、持久化、view model、协议 adapter、展示 builder 等能力拆分。

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

## 许可证与致谢

**GPL-3.0-only**。第三方声明记录在 `NOTICE`。感谢 `russh`、`GPUI`、`alacritty_terminal`、`portable-pty`、`wasmtime` 和 `tree-sitter`。
