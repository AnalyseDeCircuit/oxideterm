<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>Không gian làm việc vận hành máy chủ từ xa có AI — ứng dụng gốc viết hoàn toàn bằng Rust</strong>
  <br>
  Terminal SSH, Telnet, nối tiếp, RDP/VNC, SFTP, chuyển tiếp cổng và chỉnh sửa nhẹ trong một không gian làm việc gốc.
  <br>
  Kết xuất GPU. Miễn phí. Không cần tài khoản.
  <br>
  <strong>Không dùng Electron. Không đóng gói WebView. Không thu thập dữ liệu đo từ xa. Không thuê bao. Ưu tiên BYOK. SSH thuần Rust không dùng OpenSSL/libssh2.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.9-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Mã nguồn mở, ưu tiên cục bộ và kết xuất GPU bằng GPUI.</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="Tổng quan tính năng của OxideTerm" width="920">
</p>

---

## OxideTerm là gì

OxideTerm là không gian làm việc mã nguồn mở cho SSH và vận hành từ xa. Terminal, tệp, chuyển tiếp cổng, công cụ máy chủ và màn hình từ xa được tập trung trong một nơi.

**Bạn có thể làm gì:**

- Quản lý SSH, Telnet, serial, RDP/VNC, SFTP, chuyển tiếp cổng, shell cục bộ và chỉnh sửa nhẹ trong một không gian làm việc
- Duy trì công việc từ xa qua gián đoạn mạng ngắn bằng cơ chế kết nối lại Grace Period
- Yêu cầu OxideSens kiểm tra phiên đang hoạt động và thực hiện các hành động đã được phê duyệt qua nhà cung cấp AI của bạn

Kết nối và dữ liệu vận hành vẫn do bạn kiểm soát. OxideSens dùng nhà cung cấp AI của bạn và không yêu cầu tài khoản.

---

## Vì sao chọn OxideTerm?

- SSH, Telnet, serial, RDP/VNC, SFTP, chuyển tiếp cổng và shell cục bộ trong một ứng dụng desktop
- Kết nối lại Grace Period cho gián đoạn mạng ngắn
- OxideSens dùng thông tin AI của bạn và các hành động được phê duyệt
- Giao diện GPUI không dùng Electron hay runtime trình duyệt đóng gói

---

## Ảnh chụp màn hình

Các ảnh dưới đây thể hiện quy trình terminal, tệp, chỉnh sửa và chuyển tiếp của OxideTerm.

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

## Thiết kế cho vận hành từ xa

OxideTerm giữ kết nối, tệp, chuyển tiếp, công cụ máy chủ, tự động hóa và ngữ cảnh AI trong một không gian Rust. Các công cụ dùng chung danh tính máy chủ và vòng đời phiên.

---

## Tính năng

| Danh mục | Tính năng |
|---|---|
| **Terminal và kết nối** | Shell cục bộ, SSH, Telnet, serial, khung chia, multi-hop và kết nối lại ổn định |
| **Tệp và chỉnh sửa từ xa** | SFTP, hàng đợi truyền, dấu trang, ghi an toàn, cây dự án và chỉnh sửa theo tab |
| **Chuyển tiếp và mạng** | Chuyển tiếp cục bộ, từ xa và SOCKS5 động, quy tắc đã lưu và gỡ lỗi socket |
| **Vận hành máy chủ và màn hình từ xa** | Giám sát, tiến trình, dịch vụ, log, cổng, tác vụ, đĩa, gói, container, tmux, RDP và VNC |
| **OxideSens và tự động hóa** | Nhà cung cấp AI riêng, MCP, RAG cục bộ, hành động được duyệt, đồng bộ mã hóa và CLI |
| **Mở rộng và cá nhân hóa** | Plugin WASM, tab tùy chỉnh, lệnh nhanh, chủ đề, hình nền, phím tắt và 11 ngôn ngữ |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens mở terminal bên trong OxideTerm" width="920">
</a>

*OxideSens làm theo yêu cầu của người dùng và mở một terminal bên trong OxideTerm.*

</div>

---

## Kiến trúc

OxideTerm hợp nhất terminal, SSH, Telnet, RDP, VNC, SFTP, chuyển tiếp, IDE, AI, plugin và CLI trong một kiến trúc Rust. Chi tiết kỹ thuật được trình bày bên dưới.

<details>
<summary><strong>Kiến trúc, nội bộ SSH, shell GPUI, kết nối lại, AI, plugins và hơn nữa</strong></summary>
<br>

### Kiến trúc — lõi cùng tiến trình, không cầu nối WebView

```text
GPUI Render Loop
  WorkspaceApp / bề mặt tab / khung nhìn GPUI
        │ trong tiến trình Arc<> / async
Các crate miền
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

Không có ranh giới tuần tự hóa giữa giao diện và phần nền SSH/terminal. Dữ liệu terminal sửa `TerminalState` trực tiếp; GPUI đọc trạng thái và phát lệnh vẽ GPU.

### SSH Rust thuần — russh (ring)


- **Ngăn xếp SSH không dùng OpenSSL/libssh2** — `ring` cung cấp mật mã SSH
- SSH2 đầy đủ: trao đổi khóa, kênh, phân hệ SFTP, chuyển tiếp cổng
- ChaCha20-Poly1305 / AES-GCM, khóa Ed25519/RSA/ECDSA
- SSH Agent trên Unix (`SSH_AUTH_SOCK`) và Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump nhiều bước nhảy với xác thực độc lập ở từng bước

### Kết nối lại thông minh với Grace Period


1. Phát hiện SSH keepalive timeout mà không bị JavaScript timer throttling
2. Chụp lại bảng terminal, truyền tải SFTP, chuyển tiếp và tệp IDE
3. Probe kết nối cũ trong 30 giây Grace Period để TUI apps có thể sống qua thay đổi mạng
4. Nếu không phục hồi được, kết nối lại, khôi phục chuyển tiếp, tiếp tục truyền tải và mở lại tệp IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH pool kết nối và node routing


- Một SSH connection vật lý có thể phục vụ bảng terminal, SFTP, chuyển tiếp cổng và IDE work
- Mỗi kết nối đi qua `connecting → active → idle → link_down → reconnecting`
- UI gọi theo `nodeId`; `NodeRouter` resolve `connectionId` đang active một cách atomic
- `NodeRuntimeStore` lưu topology snapshots vào `session_tree.json`
- Lỗi máy chủ trung chuyển sẽ truyền trạng thái `link_down` tới các nút phía sau

### OxideSens AI

OxideSens vẫn ưu tiên BYOK, với xây dựng ngữ cảnh chạy trong tiến trình:

- Nhà cung cấp: OpenAI, Anthropic, Gemini, Ollama hoặc điểm truy cập OpenAI-compatible
- MCP: truyền qua stdio và SSE, khám phá và gọi công cụ
- RAG: tìm kiếm toàn văn BM25, chỉ mục vector HNSW, Reciprocal Rank Fusion và bộ tách từ bigram CJK
- Thông điệp gửi tới nhà cung cấp được lọc mẫu thông tin xác thực; ngữ cảnh không gian làm việc và hành động vẫn do người dùng kiểm soát
- Khóa API được lưu trong kho khóa hệ điều hành và chủ động loại khỏi nhật ký có cấu trúc cùng thông điệp của lõi desktop

### Giao diện desktop GPUI

UI được vẽ trực tiếp bằng GPUI, không có DOM/CSS/JavaScript rendering pipeline:

- Các loại tab trong không gian làm việc: terminal cục bộ, SSH, Telnet, nối tiếp, RDP, VNC, SFTP, IDE, Forwards, Settings, Plugin, Topology và hơn nữa
- Binary pane tree với dividers kéo được, tối đa bốn panes mỗi terminal tab
- Command palette, global key bindings và sidebars dùng GPUI primitives
- Kết xuất chế độ tức thời phản ứng với trạng thái Rust mà không cần tuần tự hóa khứ hồi

### Trạng thái terminal và kết xuất

Kết xuất terminal được mô hình hóa trước thành trạng thái Rust, rồi GPUI vẽ ra:

- PTY output đi vào `TerminalState`; scrollback, cursor, selection, marks và search state đều ở trong Rust
- Kết xuất policy có thể chuyển giữa Boost, Normal và Idle mà không cần browser event loop hợp tác
- Sixel và Kitty graphics được theo dõi như terminal-owned assets, không phải DOM nodes hoặc canvas overlays
- Các pane dùng chung mô hình trạng thái không gian làm việc, nên khôi phục tab và kết nối lại có thể snapshot topology terminal cùng nhau

### Không gian làm việc SFTP và IDE

Tệp từ xa là một phần của cùng không gian làm việc của nút, không phải tính năng tách rời:

- SFTP sessions được resolve qua `NodeRouter`, nên kết nối lại có thể thay underlying SSH connection mà không đổi node address của UI
- Hàng đợi truyền theo dõi hướng, tiến độ, trạng thái thử lại và giới hạn tốc độ độc lập với các khung tệp đang hiển thị
- Tab IDE lưu cùng bộ đệm chưa lưu, đường dẫn từ xa, trạng thái xung đột và siêu dữ liệu khôi phục
- Khi backend hỗ trợ, remote writes dùng staged/atomic behavior để tránh partial writes trong edit flow thông thường

### Plugin, CLI và chẩn đoán

Nhánh gốc giữ phần mở rộng và bề mặt hỗ trợ trong ranh giới Rust gốc:

- Plugins chạy trong wasmtime sandbox với năng lực host có kiểu thay vì biến toàn cục trình duyệt
- CLI link trực tiếp crate miền cho doctor, settings, connections, forwards, gói di động, backups và reports
- Chẩn đoán ưu tiên số đếm, đường dẫn, cờ tính năng và gợi ý đã che thay vì payload thô có bí mật
- CLI flows có thay đổi state dùng dry-run plans, `--yes` guards và rollback backups khi phù hợp

### Chuyển tiếp cổng — I/O không khóa


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


- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, tăng chi phí GPU brute-force
- Bao gồm connections, forwards, settings, quick commands, cài đặt plugin và portable bí mật

</details>

---

## Chạy từ mã nguồn

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build/build-cli.sh
./scripts/build/build-agent.sh
```

## CLI

```sh
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

## Ngăn xếp công nghệ

| Lớp | Công nghệ | Ghi chú |
|---|---|---|
| Giao diện | GPUI (Zed) | Chế độ tức thời dùng GPU, thuần Rust |
| Môi trường chạy | Tokio + DashMap | Xử lý bất đồng bộ và bản đồ đồng thời |
| SSH | russh (`ring`) | Không dùng OpenSSL/libssh2 trong ngăn xếp SSH; hỗ trợ SSH Agent |
| Terminal | portable-pty + alacritty_terminal | PTY cục bộ, mô phỏng terminal và đồ họa Sixel/Kitty |
| Plugin | wasmtime | Cách ly WASM với API máy chủ gốc |
| AI và tìm kiếm | SSE + BM25 + HNSW | Truyền dữ liệu nhà cung cấp, bigram CJK và hợp nhất RRF |

## Bảo mật

| Concern | Implementation |
|---|---|
| Thông tin xác thực đã lưu | macOS Keychain / Windows Credential Manager / libsecret |
| Bí mật trong bộ nhớ | Kiểu dữ liệu chứa bí mật và bộ đệm tạm dùng `zeroize` / `Zeroizing` tại các ranh giới sở hữu được hỗ trợ |
| Chẩn đoán | Báo cáo hỗ trợ ưu tiên siêu dữ liệu có cấu trúc và gợi ý đã che thay cho dữ liệu chứa bí mật |
| Ngữ cảnh AI | Thông điệp gửi tới nhà cung cấp được lọc mẫu thông tin xác thực; ngữ cảnh workspace và hành động vẫn do người dùng kiểm soát |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Ghi bằng CLI | Kế hoạch chạy thử, bảo vệ `--yes`, bản sao lưu khôi phục |
| Plugin | Cách ly wasmtime và API máy chủ dựa trên năng lực |

## Lưu ý về sử dụng hợp pháp

OxideTerm được cấp phép theo GPL-3.0-only và không kèm hạn chế giấy phép bổ sung. Khi sử dụng, chỉ truy cập các hệ thống, mạng và thiết bị thuộc sở hữu của bạn hoặc bạn được cấp quyền rõ ràng, đồng thời tuân thủ pháp luật hiện hành. Không sử dụng OxideTerm để truy cập trái phép, gây gián đoạn dịch vụ hoặc vượt qua cơ chế kiểm soát truy cập.

## Đóng góp

Chúng tôi hoan nghênh đóng góp về mã nguồn, tài liệu, bản dịch, plugin, kiểm thử và tái hiện lỗi. Hãy thảo luận thay đổi lớn trong issue hoặc gửi pull request tập trung cho một bản sửa lỗi có phạm vi rõ ràng.

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## Hỗ trợ và bảo trì

Báo cáo lỗi và hồi quy có bước tái hiện cùng chẩn đoán đã che được ưu tiên. Yêu cầu tính năng được đánh giá theo phạm vi, an toàn và mức độ phù hợp với định hướng không gian làm việc máy chủ từ xa của OxideTerm.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Nếu OxideTerm hỗ trợ quy trình làm việc của bạn, một ngôi sao GitHub, báo cáo tái hiện lỗi, bản sửa dịch thuật, plugin hoặc pull request đều giúp dự án tiếp tục phát triển.

---

## Giấy phép

**GPL-3.0-only**. Thông báo chi tiết về bên thứ ba nằm trong [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), cùng thông tin bổ sung trong [`NOTICE`](../../NOTICE).

## Lời cảm ơn

Xin cảm ơn `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` và `tree-sitter`.
