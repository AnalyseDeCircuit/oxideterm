<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Ứng dụng SSH có AI cho máy chủ từ xa — native app viết bằng Rust thuần</strong>
  <br>
  Terminal SSH và Telnet, SFTP, chuyển tiếp cổng, console nối tiếp và chỉnh sửa nhẹ trong một không gian làm việc native.
  <br>
  Kết xuất GPU. Miễn phí. Không cần tài khoản.
  <br>
  <strong>Không WebView. Không OpenSSL. Không thu thập telemetry. Không thuê bao. Ưu tiên BYOK. SSH thuần Rust.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--xem trước.7-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Phiên bản native lớn tiếp theo của <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — kết xuất bằng GPU, không WebView, dùng <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (framework kết xuất của Zed)</sub>
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

## Bạn có thể làm gì

- Quản lý terminal SSH và Telnet, SFTP, chuyển tiếp cổng, console nối tiếp, shell cục bộ và chỉnh sửa nhẹ trong một không gian làm việc native
- Giữ công việc từ xa tiếp tục qua mạng chập chờn với cơ chế kết nối lại Grace Period
- Yêu cầu OxideSens AI kiểm tra phiên đang chạy và thực hiện các thao tác đã được phê duyệt trong không gian làm việc qua nhà cung cấp AI của bạn

---

## Vì sao chọn OxideTerm Native?

| Nếu bạn quan tâm đến... | OxideTerm Native mang lại... |
|---|---|
| Một nút từ xa, nhiều công cụ | Terminal, SFTP, chuyển tiếp cổng, trzsz, IDE native, giám sát và OxideSens AI cùng gắn với một không gian làm việc SSH |
| Shell native không WebView | GPUI vẽ giao diện desktop trực tiếp lên bề mặt GPU, không DOM, CSS, JavaScript, Chromium hay runtime WebKit |
| Luồng làm việc SSH ưu tiên cục bộ | SSH, Telnet, SFTP, chuyển tiếp, shell cục bộ, terminal nối tiếp và cấu hình hoạt động không cần đăng ký |
| OxideSens AI dùng BYOK thay vì credit nền tảng | OxideSens dùng điểm truy cập OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible của bạn với MCP, RAG và thao tác không gian làm việc đã được phê duyệt |
| Kết nối lại ổn định | Grace Period kiểm tra kết nối cũ 30 giây trước khi thay thế, giúp TUI sống sót qua mất mạng ngắn |
| SSH thuần Rust và thông tin xác thực an toàn | `russh` + `ring`, không OpenSSL/libssh2; mật khẩu và API key ở keychain của hệ điều hành, `.oxide` dùng ChaCha20-Poly1305 + Argon2id |

## Nó là gì / không phải gì

OxideTerm Native tập trung vào **không gian làm việc AI ưu tiên cục bộ cho máy chủ từ xa**, được xây lại thành ứng dụng desktop GPUI thuần Rust. Nó dành cho người dùng muốn terminal, tệp, cổng, truyền tải, chỉnh sửa nhẹ, console nối tiếp và OxideSens AI xoay quanh máy của họ và các nút từ xa.

Nó không phải nền tảng tác nhân đám mây được lưu trữ. Nó cũng không phải Electron, Tauri hay terminal web: không Chromium, không WebView, không JavaScript, không CSS.

---

## Ảnh chụp màn hình

Giao diện native theo cùng mô hình không gian làm việc và ngôn ngữ hình ảnh OxideTerm như nhánh Tauri hiện tại.

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH với OxideSens AI" /></td>
<td align="center"><strong>Trình quản lý tệp SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Trình quản lý tệp SFTP hai bảng với hàng đợi truyền tải" /></td>
</tr>
<tr>
<td align="center"><strong>IDE tích hợp</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Chế độ IDE tích hợp" /></td>
<td align="center"><strong>Chuyển tiếp cổng thông minh</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Chuyển tiếp cổng thông minh với phát hiện tự động" /></td>
</tr>
</table>

---

## Khác gì so với WebView/Tauri

| Khía cạnh | WebView/Tauri | Native |
|---|---|---|
| Kết xuất | Chromium/Safari/WebKit2GTK + CSS | GPUI, bề mặt GPU, chế độ tức thời, Rust thuần |
| Luồng dữ liệu terminal | WebSocket → JS event loop → xterm.js | đầu vào Rust → `TerminalState` → kết xuất GPUI |
| IPC | JSON-RPC cho từng lệnh | lời gọi hàm trong tiến trình |
| SSH keepalive | JavaScript timer | Rust async task |
| Runtime plugin | ESM trong sandbox trình duyệt | WASM wasmtime + API host Rust có kiểu |
| CLI | Cần desktop app chạy | Standalone binary |
| Ranh giới runtime | Runtime trình duyệt + cầu WebView | Tiến trình native; không kèm runtime trình duyệt |

## Tính năng

| Danh mục | Tính năng |
|---|---|
| Terminal | PTY cục bộ, SSH, Telnet, terminal nối tiếp cục bộ, chia pane, tích hợp shell, đánh dấu lệnh, asciicast, trzsz, Sixel/Kitty graphics, chính sách kết xuất |
| SSH & Auth | pool kết nối, ProxyJump không giới hạn, kết nối lại Grace Period, TOFU khóa host, chuyển tiếp SSH Agent, password/key/cert/keyboard-interactive |
| SFTP / IDE | trình duyệt hai bảng, hàng đợi truyền tải, xem trước, đánh dấu, ghi nguyên tử, cây tệp từ xa, trình soạn thảo nhiều tab, giải quyết xung đột |
| Forwarding | Local, Remote, Dynamic SOCKS5, quy tắc đã lưu, khôi phục sau kết nối lại, báo cáo kết thúc, hết thời gian nhàn rỗi |
| AI | OxideSens với OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG và phê duyệt lệnh |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, encrypted import/export |
| Plugin / CLI | sandbox WASM, API host native, cài đặt plugin; CLI cho settings, connections, forwards, plugins, bí mật, cloud-sync, backup, report |

## Kiến trúc

OxideTerm Native loại bỏ cầu WebView và giữ terminal, SSH, Telnet, SFTP, chuyển tiếp, IDE, AI, plugin và CLI trong một kiến trúc Rust native. Các chi tiết triển khai đầy đủ được giữ lại bên dưới.

<details>
<summary><strong>Kiến trúc, nội bộ SSH, shell GPUI, kết nối lại, AI, plugins và hơn nữa</strong></summary>
<br>

### Architecture — Single-Process, Zero-Bridge

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ trong tiến trình Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

Không có serialization boundary giữa UI và SSH/terminal backend. Terminal bytes sửa `TerminalState` trực tiếp; GPUI đọc state và phát GPU draw calls.

### SSH Rust thuần — russh (ring)

Native edition liên kết cùng stack `russh` của Tauri line trực tiếp vào desktop binary:

- **Không phụ thuộc OpenSSL** nhờ `ring`
- SSH2 đầy đủ: trao đổi khóa, kênh, phân hệ SFTP, chuyển tiếp cổng
- ChaCha20-Poly1305 / AES-GCM, khóa Ed25519/RSA/ECDSA
- SSH Agent trên Unix (`SSH_AUTH_SOCK`) và Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump nhiều hop với auth độc lập ở từng hop

### Smart Reconnect với Grace Period

Reconnect semantics khớp với Tauri line, nhưng orchestration chạy hoàn toàn trong Rust async tasks:

1. Phát hiện SSH keepalive timeout mà không bị JavaScript timer throttling
2. Chụp lại bảng terminal, truyền tải SFTP, chuyển tiếp và tệp IDE
3. Probe kết nối cũ trong 30 giây Grace Period để TUI apps có thể sống qua thay đổi mạng
4. Nếu không phục hồi được, kết nối lại, khôi phục chuyển tiếp, tiếp tục truyền tải và mở lại tệp IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH pool kết nối và node routing

`SshConnectionRegistry` dùng `DashMap`, giữ mô hình node-first của Tauri nhưng bỏ WebSocket lifecycle bridge:

- Một SSH connection vật lý có thể phục vụ bảng terminal, SFTP, chuyển tiếp cổng và IDE work
- Mỗi connection đi qua `connecting → active → idle → link_down → kết nối lạiing`
- UI gọi theo `nodeId`; `NodeRouter` resolve `connectionId` đang active một cách atomic
- `NodeRuntimeStore` lưu topology snapshots vào `session_tree.json`
- Jump host fail sẽ cascade `link_down` xuống downstream nodes

### OxideSens AI

OxideSens vẫn ưu tiên BYOK, với xây dựng ngữ cảnh chạy trong tiến trình:

- Nhà cung cấp: OpenAI, Anthropic, Gemini, Ollama hoặc điểm truy cập OpenAI-compatible
- MCP: stdio và SSE transports, tool discovery và invocation
- RAG: BM25 full-text, HNSW vector index, Reciprocal Rank Fusion, CJK bigram tokenizer
- ngữ cảnh AI đến từ trạng thái không gian làm việc; thông tin xác thực được che trước khi gọi nhà cung cấp
- API key ở trong keychain của hệ điều hành, không đi vào log hoặc frame IPC

### GPUI Desktop Shell

UI được vẽ trực tiếp bằng GPUI, không có DOM/CSS/JavaScript rendering pipeline:

- 17 không gian làm việc loại tab: terminal cục bộ, SSH terminal, Telnet terminal, SFTP, IDE, Forwards, Settings, Plugin, Topology và hơn nữa
- Binary pane tree với dividers kéo được, tối đa bốn panes mỗi terminal tab
- Command palette, global key bindings và sidebars dùng GPUI primitives
- Immediate-mode rendering phản ứng với Rust state mà không cần serialization round-trip

### Terminal State và Kết xuất

Terminal rendering được mô hình hóa trước thành Rust state, rồi GPUI vẽ ra:

- PTY output đi vào `TerminalState`; scrollback, cursor, selection, marks và search state đều ở trong Rust
- Kết xuất policy có thể chuyển giữa Boost, Normal và Idle mà không cần browser event loop hợp tác
- Sixel và Kitty graphics được theo dõi như terminal-owned assets, không phải DOM nodes hoặc canvas overlays
- Các pane dùng chung mô hình trạng thái không gian làm việc, nên khôi phục tab và kết nối lại có thể snapshot topology terminal cùng nhau

### SFTP và IDE Workspace

Tệp từ xa là một phần của cùng không gian làm việc của nút, không phải tính năng tách rời:

- SFTP sessions được resolve qua `NodeRouter`, nên kết nối lại có thể thay underlying SSH connection mà không đổi node address của UI
- Transfer queues theo dõi direction, progress, retry state và speed limits độc lập với file panes đang hiển thị
- IDE tabs giữ chung dirty buffers, remote paths, conflict state và restore metadata
- Khi backend hỗ trợ, remote writes dùng staged/atomic behavior để tránh partial writes trong edit flow thông thường

### Plugin, CLI và chẩn đoán

Native branch giữ extension và support surfaces trong Rust native boundaries:

- Plugins chạy trong wasmtime sandbox với năng lực host có kiểu thay vì biến toàn cục trình duyệt
- CLI link trực tiếp crate miền cho doctor, settings, connections, forwards, gói di động, backups và reports
- Chẩn đoán ưu tiên số đếm, đường dẫn, cờ tính năng và gợi ý đã che thay vì payload thô có bí mật
- CLI flows có thay đổi state dùng dry-run plans, `--yes` guards và rollback backups khi phù hợp

### Port Forwarding — Lock-Free I/O

Forwarding giữ semantics của Tauri trong một Rust crate độc lập:

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Một task `ssh_io` sở hữu mỗi SSH Channel, tránh `Arc<Mutex<Channel>>`
- Reconnect auto-restore, báo cáo kết thúc và hết thời gian nhàn rỗi

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
- Bao gồm connections, forwards, settings, quick commands, cài đặt plugin và portable bí mật

</details>

---

## Chạy từ source

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
| Mật khẩu và khóa | macOS Keychain / Windows Credential Manager / libsecret |
| Secret memory | `zeroize` / `Zeroizing` |
| Chẩn đoán và ngữ cảnh AI | giá trị bí mật are cheed before output or lời gọi nhà cung cấp |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI writes | dry-run plans, `--yes` guards, rollback backups |
| Plugins | wasmtime isolation and dựa trên năng lực API host |

## Release Status

- [x] chuyển tiếp SSH Agent, kết nối lại Grace Period, GPUI desktop shell
- [x] Luồng dữ liệu terminal trong tiến trình, không dùng WebSocket
- [x] SFTP, chuyển tiếp, IDE, AI, đồng bộ đám mây, plugins, CLI
- [x] Local serial and Telnet terminals
- [x] Full ProxyCommand
- [ ] Audit logging

## Contributing

## Trung lập nhà cung cấp

OxideTerm là ưu tiên BYOK và nhà cung cấp-neutral.

Tích hợp nhà cung cấp tồn tại để giúp người dùng kết nối các công cụ họ đã tin tưởng. Chúng không phải bảng xếp hạng, biển quảng cáo, hay hệ thống thưởng cho bên nào hỏi han nhiệt tình nhất.

Compatibility, maintainability, security và real user value quyết định nội dung nào được ghi vào documentation. Visibility đi theo usefulness, không đi theo enthusiasm.

Khi feature đã tồn tại ở bản Tauri, hãy giữ behavior, labels, interaction states và workflows tương thích trừ khi replacement được ghi rõ. Crate mới phải có trách nhiệm domain thật, không chỉ re-export.

## Hỗ trợ và bảo trì

Báo cáo lỗi và hồi quy có bước tái hiện cùng chẩn đoán đã cheed được ưu tiên. Yêu cầu tính năng được đánh giá theo phạm vi, an toàn và mức độ phù hợp với định hướng của OxideTerm cho không gian làm việc máy chủ từ xa.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Nếu OxideTerm giúp workflow của bạn, GitHub star, tái hiện issue, sửa bản dịch, plugin hoặc pull request đều giúp dự án dễ tiếp tục hơn.

---

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
