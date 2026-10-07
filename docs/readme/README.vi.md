<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm: các máy chủ của bạn, một không gian làm việc" width="920">

# ⚡ OxideTerm

**Trình khách SSH miễn phí, chạy trực tiếp trên hệ điều hành, kết hợp không gian làm việc cho hoạt động từ xa và trợ lý AI dùng khóa của riêng bạn.**

SSH · Mosh · Telnet · Serial · RDP/VNC · SFTP · chuyển tiếp cổng · trình biên tập tích hợp, tất cả trong một ứng dụng kết xuất bằng GPU.
Không cần tài khoản. Không cần đăng ký thuê bao. Không thu thập dữ liệu đo từ xa. Không dùng Electron.

[![Bản phát hành mới nhất](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Nền tảng](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![Giấy phép](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Lượt gắn sao](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Tải xuống**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Tài liệu**](https://oxideterm.app) ·
[**Lịch sử thay đổi**](../../.github/release-notes/stable-changelog.md) ·
[**Báo cáo sự cố**](https://github.com/AnalyseDeCircuit/oxideterm/issues)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md) | [Русский](README.ru.md)

</div>

---

## Bắt đầu nhanh

1. **Cài đặt OxideTerm.** Tải gói cài đặt từ [bản phát hành mới nhất](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest); hướng dẫn cho từng nền tảng nằm trong phần [Cài đặt](#install) bên dưới.
2. **Thêm máy chủ.** Mở Trình quản lý phiên và tạo kết nối SSH hoặc nhập máy chủ từ `~/.ssh/config`.
3. **Kết nối.** Mở một terminal. Khóa máy chủ được đối chiếu với `~/.ssh/known_hosts`.
4. **Sử dụng các phần còn lại của không gian làm việc.** Mở SFTP, chuyển tiếp cổng hoặc trình biên tập tích hợp trên cùng một nút. Theo mặc định, các thành phần này dùng chung một kết nối SSH.
5. **Tùy chọn: bật AI.** Trong Cài đặt, thêm điểm cuối OpenAI, Anthropic, Gemini, Ollama hoặc tương thích OpenAI của riêng bạn để bật OxideSens.

Xem [tài liệu](https://oxideterm.app) để tìm hiểu ứng dụng qua hướng dẫn từng bước.

---

## Các tính năng

| | |
|---|---|
| **Terminal và giao thức** | Shell cục bộ, SSH, Mosh, Telnet, Serial, khung chia nhỏ, tuyến kết nối qua nhiều chặng, SSH agent và chuyển tiếp agent, thông tin xác thực 2FA và TOTP, chuyển tiếp X11, tích hợp shell, đánh dấu lệnh, nhật ký phiên có thể cấu hình, ghi lại phiên, đồ họa Sixel và Kitty, truyền tệp trzsz |
| **tmux và gửi đồng thời** | Chế độ điều khiển `tmux -CC` tích hợp, bố cục khung và đường phân chia có thể kéo, nhóm gửi đồng thời có tên, cùng công cụ gửi lệnh nâng cao đến nhiều đích với đầu vào có thể lên lịch và lặp lại |
| **Độ tin cậy** | Cơ chế kết nối lại Grace Period giữ ứng dụng TUI hoạt động qua những lần mất mạng ngắn, sau đó khôi phục chuyển tiếp cổng, truyền tệp và các tệp đang mở trong trình biên tập |
| **Tệp và biên tập** | Trình quản lý SFTP hai khung, hàng đợi truyền tệp với giới hạn tốc độ và thời gian hoàn thành dự kiến, dấu trang, trình biên tập từ xa tích hợp với thao tác ghi an toàn, xử lý xung đột và khôi phục không gian làm việc |
| **Mạng** | Chuyển tiếp cục bộ, từ xa và SOCKS5 động, quy tắc đã lưu, phát hiện cổng từ xa, sơ đồ kết nối, gỡ lỗi socket theo nhu cầu |
| **Máy tính từ xa** | RDP và VNC tích hợp, hỗ trợ bảng nhớ tạm và thao tác nhập |
| **Vận hành máy chủ** | Giám sát tiến trình, dịch vụ, nhật ký, cổng, tác vụ, ổ đĩa, gói phần mềm, container và tmux |
| **AI và tự động hóa** | OxideSens dùng khóa của bạn, MCP, RAG cục bộ, Agent Skills, thao tác được phê duyệt trong không gian làm việc, CLI độc lập |
| **Xem xét và kiểm tra hoạt động** | Không gian làm việc Thông báo và Kiểm tra hoạt động tùy chọn, cùng bản ghi phiên được mã hóa (cả hai đều tắt theo mặc định) |
| **Đồng bộ và tính di động** | Đồng bộ đám mây được mã hóa, gói `.oxide` di động |
| **Cá nhân hóa** | Chủ đề, hình nền, phím tắt có thể cấu hình, Lệnh nhanh, 11 ngôn ngữ giao diện |

---

## Vì sao chọn OxideTerm

- **Miễn phí, ưu tiên sử dụng cục bộ.** Không tài khoản, không thuê bao, không thu thập dữ liệu đo từ xa. Bạn luôn kiểm soát các kết nối và dữ liệu vận hành của mình.
- **Một không gian làm việc cho mỗi máy chủ.** Terminal, SFTP, chuyển tiếp cổng, RDP/VNC, trình biên tập, giám sát và AI cùng gắn vào một nút, thay vì hoạt động như những công cụ rời rạc.
- **Ứng dụng chạy trực tiếp trên hệ điều hành, không phải trình duyệt khoác áo ứng dụng.** Giao diện được vẽ trực tiếp bằng GPU với [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). Không dùng Electron và không kèm WebView.
- **AI theo lựa chọn của bạn.** OxideSens dùng nhà cung cấp và khóa của riêng bạn, chỉ thực hiện những thao tác bạn phê duyệt.
- **Kết nối bền bỉ.** Grace Period kiểm tra kết nối cũ trong 30 giây trước khi thay thế, giúp ứng dụng TUI vượt qua những lần mất mạng ngắn.
- **SSH thuần Rust.** Bộ thư viện SSH sử dụng `russh` với `ring`, không cần OpenSSL hay libssh2.

---

## Mức sử dụng bộ nhớ

**Bản viết lại chạy trực tiếp trên hệ điều hành đã giảm bộ nhớ khi không hoạt động xuống khoảng một phần tư so với phiên bản cũ trên macOS và một phần tám trên Windows.** Đây là các quan sát do người duy trì ghi lại khi chuyển từ Tauri 1.x sang GPUI 2.0:

| Nền tảng | Tauri 1.x (không hoạt động) | Bản 2.0 chạy trực tiếp trên hệ điều hành (không hoạt động) | Mức giảm |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | Khoảng 74% |
| Windows | 182.4 MB | 23.5 MB | Khoảng 87% |

Tổng bộ nhớ của phiên bản cũ bao gồm OxideTerm và các tiến trình WebView liên quan. Phiên bản mới không còn cần những tiến trình trình duyệt đó.

![So sánh bộ nhớ khi không hoạt động bằng ảnh chụp tiến trình hệ thống: Tauri 1.x và bản 2.0 chạy trực tiếp trên hệ điều hành](../../docs/screenshots/oxideterm-memory-comparison.png)

---

## Ảnh chụp màn hình

| Terminal SSH với OxideSens | Trình quản lý tệp SFTP |
|---|---|
| ![Terminal SSH với AI OxideSens](../../docs/screenshots/terminal/SSHTERMINAL.png) | ![Trình quản lý tệp SFTP hai khung với hàng đợi truyền tệp](../../docs/screenshots/sftp/sftp.png) |

| IDE tích hợp | Chuyển tiếp cổng thông minh |
|---|---|
| ![Chế độ IDE tích hợp](../../docs/screenshots/miniIDE/miniide.png) | ![Chuyển tiếp cổng thông minh với tính năng tự động phát hiện](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png) |

<details>
<summary><b>Xem OxideSens mở terminal từ một yêu cầu bằng ngôn ngữ tự nhiên</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens mở terminal bên trong OxideTerm" width="720">
</a>

</details>

---

## AI OxideSens

OxideSens là trợ lý tùy chọn có thể xem xét các phiên đang hoạt động và thực hiện thao tác trong không gian làm việc **chỉ sau khi bạn phê duyệt**.

- **Dùng khóa của riêng bạn.** Hỗ trợ OpenAI, Anthropic (Claude), Google Gemini, Ollama và mọi điểm cuối tương thích OpenAI, với các thiết lập suy luận phù hợp từng nhà cung cấp. Không có tín dụng do nền tảng cấp.
- **MCP và Agent Skills.** Kết nối máy chủ MCP (stdio và SSE) và nạp Agent Skills có phạm vi giới hạn.
- **Kho kiến thức cục bộ (RAG).** Tìm kiếm toàn văn BM25 kết hợp chỉ mục vector.
- **Bạn kiểm soát ngữ cảnh.** Bạn chọn ngữ cảnh và thao tác nào trong không gian làm việc được phê duyệt; các quy tắc chính sách lệnh vẫn được áp dụng.
- **Che thông tin xác thực.** Tin nhắn gửi tới nhà cung cấp được lọc để che các mẫu thông tin xác thực.
- **Khóa nằm trong kho khóa của hệ điều hành** và không xuất hiện trong nhật ký có cấu trúc.

---

<a id="plugins"></a>
## Plugin

OxideTerm hỗ trợ ba hình thức plugin:

| Loại | Cách chạy | Ranh giới |
|---|---|---|
| **Chỉ có tệp kê khai** | Phần mở rộng khai báo, không có mã | Không có mã thực thi |
| **WASM** | Wasmtime/WASI hoặc tiến trình phụ trợ | Lời gọi tới ứng dụng chủ được kiểm soát và giới hạn theo quyền |
| **Tiến trình** | Một tiến trình cục bộ thông thường | Mã cục bộ đáng tin cậy, **không** được hệ điều hành cô lập |

Plugin ESM cũ của Tauri (1.x) có thể xuất hiện trong danh sách nhưng không được ứng dụng 2.x chạy trực tiếp trên hệ điều hành thực thi. Chỉ cài plugin dạng tiến trình từ nguồn bạn tin tưởng.

---

## Bảo mật và quyền riêng tư

| Chủ đề | Cách hoạt động |
|---|---|
| **Thông tin xác thực đã lưu** | Kho khóa của hệ điều hành (macOS Keychain, Windows Credential Manager, libsecret) |
| **Bí mật trong bộ nhớ** | Các kiểu dữ liệu chứa bí mật và vùng đệm tạm sử dụng `zeroize` tại những ranh giới quyền sở hữu được hỗ trợ |
| **Khóa máy chủ** | Tin cậy lần sử dụng đầu tiên dựa trên `~/.ssh/known_hosts`; từ chối thay đổi bất ngờ |
| **Xuất gói di động** | Gói `.oxide` dùng ChaCha20-Poly1305 với Argon2id (256 MB bộ nhớ, 4 vòng lặp) |
| **Ngữ cảnh AI** | Che các mẫu thông tin xác thực trước khi gửi bất kỳ nội dung nào tới nhà cung cấp; bạn phê duyệt ngữ cảnh và thao tác |
| **Bản ghi phiên** | Tắt theo mặc định; được mã hóa và lưu trên thiết bị của bạn, không đồng bộ lên đám mây; không thu lại thao tác nhập từ bàn phím |
| **Kiểm tra hoạt động** | Tắt theo mặc định; dữ liệu nằm trên thiết bị của bạn và các chi tiết nhạy cảm được mã hóa |
| **Thay đổi qua CLI** | Kế hoạch chạy thử, yêu cầu `--yes` và bản sao lưu để hoàn tác các lệnh thay đổi trạng thái |
| **Plugin** | Xem [Plugin](#plugins) |
| **Dữ liệu đo từ xa** | Không thu thập |

**Sử dụng hợp pháp.** OxideTerm được cấp phép GPL-3.0-only, không có hạn chế bổ sung. Chỉ truy cập hệ thống, mạng và thiết bị mà bạn sở hữu hoặc được cho phép truy cập một cách rõ ràng, đồng thời tuân thủ pháp luật hiện hành. Không dùng OxideTerm để truy cập trái phép, làm gián đoạn dịch vụ hoặc vượt qua cơ chế kiểm soát truy cập.

---

## Giới hạn hiện tại

Những điều bạn nên biết trước khi cài đặt:

- Chỉ dành cho máy tính (macOS, Windows, Linux). Không có ứng dụng di động.
- Dự án phát triển nhanh và phát hành thường xuyên. Xem [lịch sử thay đổi](../../.github/release-notes/stable-changelog.md) và [các sự cố đang mở](https://github.com/AnalyseDeCircuit/oxideterm/issues).
- Kiểm tra hoạt động và ghi lại phiên cần được bật chủ động, và chỉ phản ánh những gì chính OxideTerm có thể quan sát.
- Plugin dạng tiến trình không được hệ điều hành cô lập.
- Nếu bộ kết xuất không hoạt động trên máy của bạn, hãy thử cấu hình tương thích: `OXIDETERM_RENDER_PROFILE=compatibility`.

---

<a id="for-developers"></a>
## Dành cho nhà phát triển

<details>
<summary><b>Chạy từ mã nguồn</b></summary>

**Yêu cầu:** Bộ công cụ Rust (edition 2024) và môi trường máy tính có thể chạy GPUI.

```bash
# Run the app
cargo run

# If the renderer fails on your machine
OXIDETERM_RENDER_PROFILE=compatibility cargo run

# Build the headless CLI companion
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Với Nix: `nix build .#oxideterm`, `nix run .#oxideterm` hoặc `nix develop`.

Các tệp thực thi CLI được tạo tại `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

</details>

<details>
<summary><b>Giao diện dòng lệnh</b></summary>

CLI `oxideterm` không có giao diện đồ họa hoạt động mà không cần khởi chạy ứng dụng, phù hợp cho tự động hóa, CI và chẩn đoán. CLI hỗ trợ cài đặt, kết nối, chuyển tiếp cổng, plugin, lệnh nhanh, bí mật, gói di động, chẩn đoán, báo cáo, kế hoạch theo lô, sao lưu và đồng bộ đám mây.

```bash
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- forwards list --format json
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- oxide export ./profile.oxide --connection prod --password-stdin
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
cargo run -p oxideterm-cli -- completion install zsh --force

# Path and profile isolation for CI or fixtures
cargo run -p oxideterm-cli -- --config-dir ./fixture-config doctor --strict
```

</details>

<details>
<summary><b>Kiến trúc</b></summary>

Giao diện và phần xử lý terminal/SSH dùng chung một tiến trình Rust; các agent từ xa tùy chọn và công cụ phụ trợ theo nền tảng nằm ngoài tiến trình này. Dữ liệu byte của terminal thay đổi trực tiếp `TerminalState`, và GPUI kết xuất từ trạng thái đó, không qua bước phân tích JSON, WebSocket, Base64 hay xterm.js.

```
┌─────────────────────────────────────────────────┐
│               GPUI Render Loop                  │
│   WorkspaceApp  ·  Tab surfaces  ·  GPUI views  │
└──────────────────────┬──────────────────────────┘
                       │  in-process Arc<> / async
┌──────────────────────▼──────────────────────────┐
│             Domain Crates (Rust async)          │
│  NodeRouter → SshConnectionRegistry             │
│  TerminalState ← SSH PTY channel (russh)        │
│  SftpSession · ForwardingRuntime · IdeWorkspace │
│  Ai/ACP Entities · CloudSync · Plugin Runtimes  │
└─────────────────────────────────────────────────┘
```

| Khía cạnh | Cách tiếp cận kèm trình duyệt | OxideTerm |
|---|---|---|
| Kết xuất | Bộ máy trình duyệt và bố cục web | GPUI trên bề mặt GPU |
| Luồng dữ liệu terminal | WebSocket → vòng lặp sự kiện JS → xterm.js | Đầu vào Rust → `TerminalState` → kết xuất GPUI |
| Vòng đời kết nối | Chia giữa giao diện và phần xử lý phía sau | Một kết nối và quy trình kết nối lại trong cùng tiến trình |
| Ngữ cảnh AI | Sao chép qua cầu nối của ứng dụng | Tạo từ không gian làm việc đang hoạt động, có sự phê duyệt của người dùng |
| CLI | Cần ứng dụng máy tính đang chạy | Tệp thực thi độc lập, liên kết trực tiếp với các crate |

**Nhóm kết nối.** `SshConnectionRegistry` sử dụng `DashMap` và được truy cập qua `NodeRouter`. Các khung terminal, SFTP, chuyển tiếp cổng và trình biên tập có thể dùng chung một kết nối SSH vật lý cho mỗi nút; chính sách terminal cũng cho phép chọn kết nối riêng. Mỗi kết nối đi qua các trạng thái `connecting → active → idle → link_down → reconnecting`. Khi máy chủ trung chuyển gặp lỗi, các nút phía sau được đánh dấu `link_down`. AI và plugin sử dụng tham chiếu quyền và ảnh chụp trạng thái máy chủ thay vì đăng ký làm thành phần sử dụng kết nối.

**Kết nối lại Grace Period.**

1. Phát hiện hết thời gian chờ keepalive.
2. Chụp trạng thái các khung terminal, phiên truyền SFTP, chuyển tiếp cổng và tệp trong trình biên tập.
3. Kiểm tra kết nối cũ trong 30 s để ứng dụng TUI vượt qua những lần mất mạng ngắn.
4. Mở kết nối mới, khôi phục chuyển tiếp cổng, tiếp tục truyền tệp và mở lại các tệp trong trình biên tập.

Phiên SFTP mang số thế hệ kết nối: sau khi kết nối lại, một phiên đủ điều kiện được lấy lại, nhưng thao tác thuộc thế hệ cũ không bao giờ bị âm thầm chuyển sang kết nối mới.

**Chuyển tiếp cổng.** Một crate độc lập hỗ trợ `-L`, `-R` và `-D` (SOCKS5). Mỗi kênh SSH do một tác vụ `ssh_io` duy nhất sở hữu, nên không có mutex dùng chung trên luồng xử lý thường xuyên.

**SSH thuần Rust.** `russh` với `ring`: hỗ trợ đầy đủ SSH2, ChaCha20-Poly1305 và AES-GCM, khóa Ed25519/RSA/ECDSA, SSH agent trên Unix (`SSH_AUTH_SOCK`) và Windows (`\\.\pipe\openssh-ssh-agent`), cùng chuỗi nhiều chặng với xác thực độc lập ở mỗi chặng.

**Công nghệ sử dụng**

| Lớp | Công nghệ |
|---|---|
| Giao diện | GPUI (framework giao diện của Zed sử dụng GPU) |
| Môi trường thực thi | Tokio, DashMap |
| SSH | `russh` với `ring` (không cần OpenSSL hay libssh2) |
| PTY cục bộ | `portable-pty` (ConPTY trên Windows) |
| Giả lập terminal | `alacritty_terminal` (VT100–VT500, Sixel, đồ họa Kitty) |
| Trình biên tập | Tô sáng cú pháp bằng tree-sitter, bộ đệm riêng |
| Mã hóa | ChaCha20-Poly1305, Argon2id |
| Plugin | Wasmtime/WASI, WASM qua tiến trình phụ trợ và plugin dạng tiến trình |
| Luồng dữ liệu AI | SSE (OpenAI, Anthropic, Gemini), trong cùng tiến trình |
| RAG | BM25 + chỉ mục vector HNSW với hợp nhất thứ hạng, bộ tách từ bigram CJK |
| Đa ngôn ngữ | `oxideterm-i18n` (11 ngôn ngữ) |

</details>

---

<a id="install"></a>
## Cài đặt

[**Tải bản phát hành mới nhất**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

| Hệ điều hành | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Trình cài đặt (`.exe`) | Trình cài đặt (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

Kiểm tra bản tải xuống bằng tệp `sha256sums.txt` trên trang phát hành. Các gói di động và chữ ký cũng được liệt kê tại đó.

### macOS

Nếu Gatekeeper chặn ứng dụng, hãy xóa cờ cách ly:

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

Nếu SmartScreen hiển thị cảnh báo, chọn **Thông tin thêm → Vẫn chạy**.

### Linux

```bash
# AppImage
chmod +x OxideTerm_*_linux_*.AppImage && ./OxideTerm_*_linux_*.AppImage

# Debian / Ubuntu
sudo dpkg -i OxideTerm_*_linux_*.deb && sudo apt-get install -f

# Fedora / RHEL-compatible
sudo dnf install ./OxideTerm_*_linux_*.rpm

# Nix; updates are managed by Nix
nix run github:AnalyseDeCircuit/oxideterm
```

Muốn tự biên dịch? Xem **Chạy từ mã nguồn** trong phần [Dành cho nhà phát triển](#for-developers).

---

## Đóng góp

Chúng tôi hoan nghênh mọi đóng góp: mã Rust, tài liệu, bản dịch, plugin, kiểm thử và tái hiện sự cố. Hãy mở issue trước để thảo luận những thay đổi lớn.

Báo cáo lỗi hữu ích nhất khi kèm gói chẩn đoán đã che dữ liệu nhạy cảm:

```bash
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

Lỗi có thể tái hiện và lỗi hồi quy được ưu tiên. Đề xuất tính năng được xem xét theo phạm vi, mức độ an toàn và sự phù hợp với định hướng không gian làm việc cho máy chủ từ xa của OxideTerm. Nếu OxideTerm giúp ích cho công việc của bạn, một lượt gắn sao trên GitHub, báo cáo lỗi có thể tái hiện, sửa bản dịch hoặc một plugin đều góp phần duy trì sự phát triển của dự án.

### Người đóng góp

Cảm ơn tất cả những người giúp OxideTerm ngày càng tốt hơn.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="Những người đóng góp cho OxideTerm">
  </a>
</p>

---

## Giấy phép

**GPL-3.0-only.** Thông tin ghi nhận các thư viện phụ thuộc nằm trong [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), cùng các thông báo bổ sung trong [`NOTICE`](../../NOTICE).

**Được xây dựng bằng:** [russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
