<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Workspace AI-native cho máy chủ từ xa.</strong>
  <br>
  Kết nối tới server của bạn qua SSH, rồi làm việc với terminal, tệp, cổng, truyền tải, chỉnh sửa nhẹ, serial console và sidebar OxideSens tự chủ trong một app native local-first.
  <br>
  Ứng dụng GPUI native · SSH thuần Rust · BYOK AI tự chủ · không cần tài khoản cho workflow SSH chính
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust all the way down.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.1-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Next major native edition của <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — GPU-rendered, zero-WebView, dùng <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (rendering framework của Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens mở terminal bên trong OxideTerm" width="920">
</a>

*OxideSens làm theo yêu cầu của người dùng và mở một terminal bên trong OxideTerm.*

</div>

---

> **Release status:** OxideTerm Native đang được chuẩn bị làm next major release của OxideTerm. Public installer chưa được phát hành; hiện tại hãy chạy từ source. Các packaged release hiện tại vẫn ở Tauri line cho đến khi native installer sẵn sàng.

## Bạn có thể làm gì

- Quản lý SSH terminal, SFTP, port forward, serial console, local shell và chỉnh sửa nhẹ trong một native workspace
- Giữ công việc từ xa sống qua mạng chập chờn với Grace Period reconnect
- Yêu cầu sidebar OxideSens tự chủ kiểm tra live session và chạy các workspace action đã được phê duyệt qua nhà cung cấp AI của bạn

---

## Vì sao chọn OxideTerm Native?

| Nếu bạn quan tâm đến... | OxideTerm Native mang lại... |
|---|---|
| Một remote node, nhiều công cụ | Terminal, SFTP, port forwarding, trzsz, native IDE, monitoring và sidebar OxideSens tự chủ cùng gắn với một SSH workspace |
| Native shell zero-WebView | GPUI vẽ desktop UI trực tiếp lên GPU surface, không DOM, CSS, JavaScript, Chromium hay WebKit runtime |
| Workflow SSH local-first | SSH, SFTP, forwarding, local shell, serial terminal và cấu hình hoạt động không cần đăng ký |
| BYOK AI tự chủ thay vì credit nền tảng | OxideSens dùng endpoint OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible của bạn với MCP, RAG và workspace action đã được phê duyệt |
| Reconnect ổn định | Grace Period thăm dò kết nối cũ 30 giây trước khi thay thế, giúp TUI sống sót qua mất mạng ngắn |
| SSH thuần Rust và an toàn credential | `russh` + `ring`, không OpenSSL/libssh2; mật khẩu và API key ở OS keychain, `.oxide` dùng ChaCha20-Poly1305 + Argon2id |

## Nó là gì / không phải gì

OxideTerm Native tập trung vào **workspace AI local-first cho máy chủ từ xa**, được xây lại thành app desktop GPUI thuần Rust. Nó dành cho người dùng muốn terminal, tệp, cổng, truyền tải, chỉnh sửa nhẹ, serial console và sidebar BYOK AI tự chủ xoay quanh máy của họ và các node từ xa.

Nó chưa phải stable download line hiện tại, và không phải nền tảng cloud agent được host. Nó cũng không phải Electron, Tauri hay web terminal: không Chromium, không WebView, không JavaScript, không CSS.

---

## Ảnh chụp màn hình

Native UI theo cùng mô hình workspace và ngôn ngữ hình ảnh OxideTerm như Tauri line hiện tại.

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH với thanh bên OxideSens AI" /></td>
<td align="center"><strong>Trình quản lý tệp SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Trình quản lý tệp SFTP hai bảng với hàng đợi truyền tải" /></td>
</tr>
<tr>
<td align="center"><strong>IDE tích hợp</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Chế độ IDE tích hợp" /></td>
<td align="center"><strong>Chuyển tiếp cổng thông minh</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Chuyển tiếp cổng thông minh với phát hiện tự động" /></td>
</tr>
</table>

---

## Khác gì so với WebView/Tauri

| Aspect | WebView/Tauri | Native |
|---|---|---|
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI, GPU surface, immediate mode, pure Rust |
| Terminal data flow | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC mỗi command | in-process function calls |
| SSH keepalive | JavaScript timer | Rust async task |
| Plugin runtime | ESM trong browser sandbox | WASM wasmtime + typed Rust host API |
| CLI | Cần desktop app chạy | Standalone binary |
| Kích thước artifact | Trình cài đặt thường ~150–200 MB | macOS arm64 hiện tại: portable/DMG nén ~50–60 MB; release binary thô ~132 MB |

## Tính năng

| Category | Features |
|---|---|
| Terminal | Local PTY, SSH, local serial terminals, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens với OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG và command approval |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, encrypted import/export |
| Plugins / CLI | WASM sandbox, native host API, plugin settings; CLI cho settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Kiến trúc

OxideTerm Native loại bỏ WebView bridge và giữ terminal, SSH, SFTP, forwarding, IDE, AI, plugins và CLI trong một kiến trúc Rust-native. Các chi tiết triển khai đầy đủ được giữ lại bên dưới.

<details>
<summary><strong>Kiến trúc, nội bộ SSH, GPUI shell, reconnect, AI, plugins và hơn nữa</strong></summary>
<br>

### Architecture — Single-Process, Zero-Bridge

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

Không có serialization boundary giữa UI và SSH/terminal backend. Terminal bytes sửa `TerminalState` trực tiếp; GPUI đọc state và phát GPU draw calls.

### SSH Rust thuần — russh (ring)

Native edition liên kết cùng stack `russh` của Tauri line trực tiếp vào desktop binary:

- **Không phụ thuộc C/OpenSSL** nhờ `ring`
- SSH2 đầy đủ: key exchange, channels, SFTP subsystem, port forwarding
- ChaCha20-Poly1305 / AES-GCM, khóa Ed25519/RSA/ECDSA
- SSH Agent trên Unix (`SSH_AUTH_SOCK`) và Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump nhiều hop với auth độc lập ở từng hop

### Smart Reconnect với Grace Period

Reconnect semantics khớp với Tauri line, nhưng orchestration chạy hoàn toàn trong Rust async tasks:

1. Phát hiện SSH keepalive timeout mà không bị JavaScript timer throttling
2. Snapshot terminal panes, SFTP transfers, forwards và IDE files
3. Probe kết nối cũ trong 30 giây Grace Period để TUI apps có thể sống qua thay đổi mạng
4. Nếu không phục hồi được, kết nối lại, restore forwards, resume transfers và mở lại IDE files

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH connection pool và node routing

`SshConnectionRegistry` dùng `DashMap`, giữ mô hình node-first của Tauri nhưng bỏ WebSocket lifecycle bridge:

- Một SSH connection vật lý có thể phục vụ terminal panes, SFTP, port forwards và IDE work
- Mỗi connection đi qua `connecting → active → idle → link_down → reconnecting`
- UI gọi theo `nodeId`; `NodeRouter` resolve `connectionId` đang active một cách atomic
- `NodeRuntimeStore` lưu topology snapshots vào `session_tree.json`
- Jump host fail sẽ cascade `link_down` xuống downstream nodes

### OxideSens AI

OxideSens vẫn BYOK-first, với context building chạy in-process:

- Providers: OpenAI, Anthropic, Gemini, Ollama hoặc endpoint OpenAI-compatible
- MCP: stdio và SSE transports, tool discovery và invocation
- RAG: BM25 full-text, HNSW vector index, Reciprocal Rank Fusion, CJK bigram tokenizer
- AI context đến từ workspace state; credentials được redact trước khi gọi provider
- API keys ở trong OS keychain, không đi vào logs hoặc IPC frames

### GPUI Desktop Shell

UI được vẽ trực tiếp bằng GPUI, không có DOM/CSS/JavaScript rendering pipeline:

- 17 workspace tab types: local/SSH terminal, SFTP, IDE, Forwards, Settings, Plugin, Topology và hơn nữa
- Binary pane tree với dividers kéo được, tối đa bốn panes mỗi terminal tab
- Command palette, global key bindings và sidebars dùng GPUI primitives
- Immediate-mode rendering phản ứng với Rust state mà không cần serialization round-trip

### Terminal State và Rendering

Terminal rendering được mô hình hóa trước thành Rust state, rồi GPUI vẽ ra:

- PTY output đi vào `TerminalState`; scrollback, cursor, selection, marks và search state đều ở trong Rust
- Rendering policy có thể chuyển giữa Boost, Normal và Idle mà không cần browser event loop hợp tác
- Sixel và Kitty graphics được theo dõi như terminal-owned assets, không phải DOM nodes hoặc canvas overlays
- Split panes dùng chung workspace state model, nên tab restore và reconnect có thể snapshot terminal topology cùng nhau

### SFTP và IDE Workspace

Remote files là một phần của cùng node workspace, không phải tính năng tách rời:

- SFTP sessions được resolve qua `NodeRouter`, nên reconnect có thể thay underlying SSH connection mà không đổi node address của UI
- Transfer queues theo dõi direction, progress, retry state và speed limits độc lập với file panes đang hiển thị
- IDE tabs giữ chung dirty buffers, remote paths, conflict state và restore metadata
- Khi backend hỗ trợ, remote writes dùng staged/atomic behavior để tránh partial writes trong edit flow thông thường

### Plugins, CLI và Diagnostics

Native branch giữ extension và support surfaces trong Rust-native boundaries:

- Plugins chạy trong wasmtime sandbox với typed host capabilities thay vì browser globals
- CLI link trực tiếp domain crates cho doctor, settings, connections, forwards, portable bundles, backups và reports
- Diagnostics ưu tiên counts, paths, feature flags và redacted hints thay vì raw payloads có secrets
- CLI flows có thay đổi state dùng dry-run plans, `--yes` guards và rollback backups khi phù hợp

### Port Forwarding — Lock-Free I/O

Forwarding giữ semantics của Tauri trong một Rust crate độc lập:

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Một task `ssh_io` sở hữu mỗi SSH Channel, tránh `Arc<Mutex<Channel>>`
- Reconnect auto-restore, death reporting và idle timeout

### trzsz — truyền file in-band

trzsz tiếp tục dùng terminal stream, không cần port phụ hoặc remote agent:

- Upload/download qua terminal stream hiện có
- Hoạt động xuyên qua ProxyJump chains
- Native file pickers tránh giới hạn bộ nhớ của browser
- Transfer hai chiều, hỗ trợ thư mục, limits có thể cấu hình

### Export `.oxide` mã hóa

Encrypted bundle format khớp với Tauri line:

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, tăng chi phí GPU brute-force
- Bao gồm connections, forwards, settings, quick commands, plugin settings và portable secrets

</details>

---

## Chạy từ source

Public native installer chưa được phát hành. Cho đến khi packaged build sẵn sàng, hãy chạy native edition từ source.

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build-cli.sh
./scripts/build-agent.sh
```

## CLI

```sh
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

## Security

| Concern | Implementation |
|---|---|
| Passwords & keys | macOS Keychain / Windows Credential Manager / libsecret |
| Secret memory | `zeroize` / `Zeroizing` |
| Diagnostics & AI context | secret values are redacted before output or provider calls |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI writes | dry-run plans, `--yes` guards, rollback backups |
| Plugins | wasmtime isolation and capability-based host API |

## Release Status

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] In-process terminal data flow without WebSocket
- [x] SFTP, forwarding, IDE, AI, cloud sync, plugins, CLI
- [x] Local serial terminals
- [ ] Public packaged installers
- [ ] Full ProxyCommand, audit logging

## Contributing

## Provider Neutrality

OxideTerm là BYOK-first và provider-neutral.

Provider integrations tồn tại để giúp người dùng kết nối các công cụ họ đã tin tưởng. Chúng không phải leaderboard, billboard, hay hệ thống thưởng cho bên nào hỏi han nhiệt tình nhất.

Compatibility, maintainability, security và real user value quyết định nội dung nào được ghi vào documentation. Visibility đi theo usefulness, không đi theo enthusiasm.

Khi feature đã tồn tại ở bản Tauri, hãy giữ behavior, labels, interaction states và workflows tương thích trừ khi replacement được ghi rõ. Crate mới phải có trách nhiệm domain thật, không chỉ re-export.

## Hỗ trợ và bảo trì

OxideTerm Native đang được chuẩn bị làm major release tiếp theo của OxideTerm và được duy trì best-effort. Bug report có bước tái hiện và chẩn đoán đã redacted được ưu tiên; feature request có thể không được triển khai.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Nếu OxideTerm giúp workflow của bạn, GitHub star, tái hiện issue, sửa bản dịch, plugin hoặc pull request đều giúp dự án dễ tiếp tục hơn.

---

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
