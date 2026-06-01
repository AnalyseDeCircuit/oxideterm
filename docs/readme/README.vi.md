<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>SSH workspace local-first cho người muốn workflow OxideTerm nhưng không muốn WebView.</strong>
  <br>
  Kết nối một lần tới remote machine, rồi làm việc với shell, files, ports, transfers, lightweight editor, serial consoles và BYOK AI trong một native Rust workspace.
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust xuyên suốt.</strong>
</p>

<p align="center">
  <em>Nếu bạn muốn một SSH workspace local-first không Electron, không WebView, không telemetry và không subscription, hãy star OxideTerm để nhiều người dùng SSH hơn có thể tìm thấy nó.</em>
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

---

> **Release status:** OxideTerm Native đang được chuẩn bị làm next major release của OxideTerm. Public installer chưa được phát hành; hiện tại hãy chạy từ source. Các packaged release hiện tại vẫn ở Tauri line cho đến khi native installer sẵn sàng.

## Vì sao chọn OxideTerm Native?

| Nếu bạn quan tâm... | OxideTerm Native mang lại... |
|---|---|
| SSH workspace, không chỉ shell | Terminal, SFTP, forwarding, trzsz, mini IDE, monitoring và AI context quanh một node |
| Local shell, serial console và remote SSH | zsh/bash/fish/pwsh/WSL2, local serial terminal và SSH trong cùng workflow |
| Không cần cloud account | SSH, SFTP, forwarding, local shell và config hoạt động local-first |
| BYOK AI | Endpoint OpenAI, Anthropic, Gemini, Ollama hoặc compatible của bạn |
| Không WebView | GPUI vẽ trực tiếp lên GPU surface, không DOM, CSS hoặc JavaScript |
| Không serialize trên hot path | Terminal bytes mutate Rust state trực tiếp, không WebSocket/JSON/Base64 |
| Không OpenSSL | SSH thuần Rust với `russh` + `ring` |
| Reconnect ổn định | Grace Period probe connection cũ trước khi kill TUI apps |
| Làm việc với file remote | SFTP tích hợp và native IDE để browse, preview, transfer, edit |
| An toàn credential | OS keychain; `.oxide` dùng ChaCha20-Poly1305 + Argon2id |

## Nó là gì / không phải gì

OxideTerm Native là **native desktop SSH workspace viết bằng Rust thuần** và là next major edition của OxideTerm. Nó đưa cùng product idea — terminal, SFTP, forwarding, editing, AI, cloud sync, plugins và CLI xoay quanh một remote node — sang GPUI UI layer không có browser runtime.

Nó không phải Electron, Tauri, web terminal hay hosted service. Không Chromium, WebView, JavaScript hoặc CSS.

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

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
