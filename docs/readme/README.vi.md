<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Phiên bản zero-WebView tiếp theo của OxideTerm.</strong>
  <br>
  Kết nối tới máy từ xa một lần, rồi làm việc với shell, tệp, cổng, truyền tải, editor nhẹ, serial console và BYOK AI trong một workspace Rust native.
  <br>
  Ứng dụng GPUI native · SSH thuần Rust · không cần tài khoản cho workflow SSH chính
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust all the way down.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version">
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
- Dùng nhà cung cấp AI của bạn để xem live session và chạy các workspace action đã được phê duyệt

---

## Vì sao chọn OxideTerm Native?

| Nếu bạn quan tâm đến... | OxideTerm Native mang lại... |
|---|---|
| Một remote node, nhiều công cụ | Terminal, SFTP, port forwarding, trzsz, native IDE, monitoring và ngữ cảnh AI cùng gắn với một SSH workspace |
| Native shell zero-WebView | GPUI vẽ desktop UI trực tiếp lên GPU surface, không DOM, CSS, JavaScript, Chromium hay WebKit runtime |
| Workflow SSH local-first | SSH, SFTP, forwarding, local shell, serial terminal và cấu hình hoạt động không cần đăng ký |
| BYOK AI thay vì credit nền tảng | OxideSens dùng endpoint OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible của bạn với hỗ trợ MCP và RAG |
| Reconnect ổn định | Grace Period thăm dò kết nối cũ 30 giây trước khi thay thế, giúp TUI sống sót qua mất mạng ngắn |
| SSH thuần Rust và an toàn credential | `russh` + `ring`, không OpenSSL/libssh2; mật khẩu và API key ở OS keychain, `.oxide` dùng ChaCha20-Poly1305 + Argon2id |

## Nó là gì / không phải gì

OxideTerm Native tập trung vào cùng **local-first SSH workspace** như OxideTerm, được xây lại thành app desktop GPUI thuần Rust. Nó dành cho người dùng muốn terminal, tệp, cổng, truyền tải, chỉnh sửa nhẹ, serial console và ngữ cảnh AI xoay quanh máy của họ và các node từ xa.

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
