<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm: các máy chủ của bạn, một không gian làm việc" width="920">

# ⚡ OxideTerm

**Ứng dụng SSH gốc, miễn phí để làm việc với máy chủ từ xa.**

SSH · Mosh · Telnet · cổng nối tiếp · RDP/VNC · SFTP · chuyển tiếp cổng · trình soạn thảo mã

Không cần tài khoản. Không cần đăng ký thuê bao. Không thu thập dữ liệu đo từ xa. Không dùng Electron.

[![Bản phát hành mới nhất](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Nền tảng](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![Giấy phép](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Lượt gắn sao](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Tải xuống**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Tài liệu**](https://oxideterm.app) ·
[**Lịch sử thay đổi**](../../.github/release-notes/stable-changelog.md) ·
[**Báo cáo sự cố**](https://github.com/AnalyseDeCircuit/oxideterm/issues) ·
[**Nhóm WeChat**](https://github.com/AnalyseDeCircuit/oxideterm/issues/752)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md)

</div>

---

## OxideTerm là gì?

OxideTerm là ứng dụng SSH cho macOS, Windows và Linux. Mỗi máy chủ có không gian làm việc riêng trong cùng một cửa sổ: terminal, quản lý tệp, chuyển tiếp cổng, soạn thảo mã, giám sát và trợ lý AI tùy chọn. OxideTerm miễn phí và mã nguồn mở theo GPL-3.0.

## Vì sao chọn OxideTerm

- **Kết nối một lần, dùng nhiều công cụ.** Terminal, SFTP, chuyển tiếp cổng và trình soạn thảo của một máy chủ mặc định dùng chung kết nối SSH, không phải đăng nhập lại khi mở công cụ khác.
- **Chịu được gián đoạn ngắn.** Khi kết nối ngừng phản hồi, OxideTerm tiếp tục kiểm tra trong 30 giây trước khi thay thế, giúp vim, htop và các chương trình toàn màn hình tiếp tục từ trạng thái cũ.
- **Ít tốn bộ nhớ.** Giao diện được vẽ trên GPU bằng [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), framework của Zed. Khi rảnh dùng khoảng 81 MB trên macOS và 24 MB trên Windows.
- **AI do bạn chọn.** OxideSens dùng nhà cung cấp và khóa API bạn chọn, không cần mua tín dụng OxideTerm.
- **SSH thuần Rust.** Dựa trên russh và ring, không cần OpenSSL hay libssh2.

<details>
<summary><b>So sánh bộ nhớ với OxideTerm 1.x</b></summary>

Bộ nhớ khi rảnh trước và sau khi chuyển từ Tauri (1.x) sang GPUI gốc (2.0). Số liệu 1.x bao gồm các tiến trình WebView.

| Nền tảng | 1.x (Tauri) | 2.0 (gốc) | Mức giảm |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 74% |
| Windows | 182.4 MB | 23.5 MB | 87% |

![Bộ nhớ khi rảnh của OxideTerm 1.x và 2.0 trong trình giám sát tiến trình](../../docs/screenshots/oxideterm-memory-comparison.png)

</details>

---

## Ảnh chụp màn hình

### Terminal

Một phiên SSH. Thanh bên liệt kê công cụ có thể mở trên máy chủ này: terminal bổ sung, SFTP, IDE và chuyển tiếp cổng.

![Terminal SSH với công cụ máy chủ ở thanh bên](../../docs/screenshots/terminal/SSHTERMINAL.png)

### Không gian làm việc kết hợp

Đặt terminal SSH, terminal cục bộ, ghi chú Markdown và SFTP trong cùng một thẻ. Kết hợp các trang từ nhiều máy chủ và máy tính của bạn trong tối đa 16 ngăn.

![Không gian làm việc bốn ngăn kết hợp SSH, terminal cục bộ, ghi chú và SFTP](../../docs/screenshots/mixed-workspace.png)

### Tệp

Thư mục cục bộ và từ xa đặt cạnh nhau, hàng đợi truyền ở phía dưới.

![Trình quản lý SFTP hai ngăn với hàng đợi truyền](../../docs/screenshots/sftp/sftp.png)

### IDE

Mở dự án từ xa và chỉnh sửa các tệp trong thẻ.

![Dự án từ xa trong IDE tích hợp](../../docs/screenshots/miniIDE/miniide.png)

### Chuyển tiếp cổng

OxideTerm liệt kê cổng đang lắng nghe trên máy chủ. Chuyển tiếp bằng một cú nhấp hoặc dùng chuyển tiếp nhanh cho công cụ như Jupyter.

![Trang chuyển tiếp liệt kê cổng phát hiện trên máy chủ](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png)

<details>
<summary><b>Xem OxideSens mở terminal từ một yêu cầu bằng ngôn ngữ tự nhiên</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens mở terminal bên trong OxideTerm" width="720">
</a>

</details>

---

## Tính năng

Các mục đánh dấu *(plugin)* được cài từ chợ plugin trong ứng dụng.

### Không gian làm việc

- Quản lý phiên: tìm, sắp xếp, nhóm, nhập/xuất kết nối đã lưu, nhóm lồng nhau, thẻ, màu, biểu tượng và ghi chú
- Nhập `~/.ssh/config` gồm khối `Match`, chuỗi `ProxyJump` và `RemoteCommand`; cập nhật các máy đã nhập theo cấu hình
- Nhập từ SecureCRT, Xshell, Termius, MobaXterm, WindTerm, Electerm và FinalShell, xem trước khi lưu
- Lọc thẻ: tất cả, trang toàn cục, terminal cục bộ hoặc một máy chủ; phiên ẩn vẫn chạy
- Tối đa 16 ngăn mỗi thẻ, kết hợp terminal, SFTP, IDE, chuyển tiếp, ghi chú, cài đặt và màn hình từ xa
- Tách thẻ sang cửa sổ riêng rồi đưa lại mà không ngắt phiên
- Xếp 2–16 terminal đang mở thành lưới mà không kết nối lại
- Bảng lệnh cho thao tác và kết nối đã lưu, cùng chế độ Zen tập trung vào một trang
- Hồ sơ terminal cục bộ với shell, thư mục ban đầu và biểu tượng riêng
- Khôi phục không gian làm việc, kích thước cửa sổ, toàn màn hình và vị trí màn hình khi khởi động

### Terminal

- Tự nhận diện shell cục bộ: Unix, Command Prompt, Windows PowerShell, PowerShell Core, Git Bash và Nushell
- Chia ngăn với đường phân cách kéo được; ngăn cục bộ mới kế thừa thư mục làm việc
- Tìm bằng biểu thức chính quy, phân biệt hoa/thường và nhiều dòng, giữ trạng thái riêng từng ngăn
- Tích hợp Bash, Zsh, Fish, Nushell, PowerShell: theo dõi thư mục, mã thoát, dấu lệnh và cài trên máy chủ từ ứng dụng
- Thu gọn đầu ra lệnh đã xong nhưng giữ trạng thái thoát
- Thanh lệnh với nhập nhiều dòng, lịch sử, gợi ý đường dẫn, hoàn thành và lệnh nhanh
- Gợi ý gần đúng từ lịch sử và lịch sử theo kết nối tồn tại qua khởi động lại nếu bật
- Chế độ nhập tự do để sửa lệnh hiện tại bằng chuột
- Sửa nội dung dán nhiều dòng trước khi gửi, có hoàn tác và bỏ dấu bao khối mã
- Kích hoạt theo văn bản hoặc regex trong đầu ra: gửi dữ liệu, chạy lệnh nhanh, mở chương trình hoặc biến kết quả thành thao tác bấm được
- Tô màu ngữ nghĩa cho đầu ra chưa định dạng như lỗi, trạng thái Git, quyền, địa chỉ và ngày; tô sáng từ khóa và regex
- URL và đường dẫn bấm được, dấu thời gian mỗi dòng tới mili giây, dấu đầu ra chưa đọc trên thẻ
- Đồ họa Sixel và Kitty, gồm hoạt ảnh Kitty
- Điều khiển `tmux -CC` gốc với bố cục ngăn và đường phân cách kéo được
- Nhóm đồng bộ đầu vào và bộ gửi lệnh văn bản/hex với tốc độ, khoảng nghỉ và lặp
- Lệnh nhanh với nhóm, ghim, tham số văn bản/lựa chọn/bí mật và quy tắc hiển thị theo máy
- Nhật ký phiên với mẫu tên, giới hạn kích thước và thời gian lưu, ANSI tùy chọn; ghi `.cast` và trình phát tích hợp
- Sao chép, cắt, dán vào vùng chọn Vim, Neovim, Emacs, cục bộ và trong phiên SSH được hỗ trợ
- Chuỗi Backspace/Delete và bảng mã theo kết nối

### Kết nối

- SSH với mật khẩu, khóa riêng, chứng chỉ OpenSSH, khóa được quản lý, SSH agent, xác thực tương tác qua bàn phím hoặc Kerberos
- Tìm SSH agent hoặc đặt endpoint riêng (`SSH_AUTH_SOCK` trên macOS/Linux, `\\.\pipe\openssh-ssh-agent` trên Windows) và chuyển tiếp agent
- Khóa Ed25519/RSA/ECDSA, mã hóa ChaCha20-Poly1305/AES-GCM, thứ tự thuật toán tùy chỉnh và tùy chọn tương thích máy chủ cũ
- Tạo và điền mã TOTP khi gặp yêu cầu khớp, trên máy chủ hoặc từng máy trung gian
- Khóa FIDO (`ed25519-sk`, `ecdsa-sk`) với yêu cầu PIN và chạm *(plugin)*
- Chuỗi máy trung gian xác thực riêng từng chặng, proxy HTTP CONNECT/SOCKS5 và ProxyCommand
- Kết nối lại có thời gian chờ: khôi phục chuyển tiếp, tiếp tục truyền và mở lại tệp soạn thảo trên kết nối mới
- Chia sẻ một kết nối mỗi máy chủ hoặc xác thực riêng mỗi terminal mới
- Chuyển tiếp X11 tin cậy hoặc không tin cậy
- Kết nối tạm không lưu và liên kết `ssh://`, `telnet://`, `mosh://`, `rdp://`, `vnc://` mở trong ứng dụng
- Mosh với dự đoán phản hồi cục bộ thích ứng *(plugin)*
- Telnet với thương lượng tùy chọn, định tuyến proxy và kết thúc dòng tùy chỉnh
- Nối tiếp: baud, bit dữ liệu, parity, bit dừng, điều khiển luồng, nhập/xuất hex, Break, DTR và RTS
- Terminal Bluetooth LE qua Nordic UART Service hoặc đặc tính GATT riêng

### Tệp và truyền dữ liệu

- Quản lý tệp cục bộ/từ xa hai ngăn, mục ưa thích, hoàn thành đường dẫn, dạng danh sách hoặc cây
- Cặp dấu trang đường dẫn cục bộ/từ xa theo kết nối
- Hàng đợi song song có giới hạn tốc độ, thời gian còn lại, tạm dừng, tiếp tục và thử lại; truyền SFTP gián đoạn trở lại ở trạng thái tạm dừng sau khi khởi động lại
- Ghi đè, bỏ qua hoặc đổi tên khi xung đột, cho một tệp hay tất cả
- SFTP tự chuyển sang SCP nếu máy chủ không có phân hệ SFTP
- Kết nối SFTP độc lập và truyền giữa máy chủ qua ứng dụng
- FTP và FTPS tường minh
- Thả tệp vào terminal SSH để tải lên thư mục hiện tại
- `trz`/`tsz` (trzsz), `rz`/`sz` (ZMODEM) trong shell từ xa; XMODEM/YMODEM/ZMODEM qua nối tiếp
- Xem trước văn bản, mã, Markdown, ảnh, âm thanh, video, hex và phông chữ
- Trình xem PDF, SQLite, chứng chỉ và nhị phân *(plugin)*
- Chuyển từ terminal sang cùng thư mục SFTP và gửi thư mục SFTP về terminal

### Trình soạn thảo và IDE

- IDE từ xa qua SFTP, lệnh shell thông thường hoặc agent bổ sung chỉ mục ký hiệu, hoàn thành và theo dõi tệp
- Chuyển terminal đang rảnh thành IDE với cùng người dùng, container và thư mục
- Thẻ, cây dự án chọn nhiều, tìm trong dự án, tìm/thay bằng regex và nhóm bắt
- Tệp tới 100 MiB, giữ bảng mã, BOM và kết thúc dòng
- Lưu an toàn có xử lý xung đột và tùy chọn tự lưu khi đổi thẻ
- Khôi phục tệp mở và thay đổi chưa lưu sau khởi động lại
- Xem Git: nhánh, worktree, staging, lịch sử, số commit trước/sau và trạng thái merge/rebase
- Nhận diện và thực thi tác vụ dự án
- Tô sáng cú pháp 15 ngôn ngữ tích hợp và hơn 20 ngôn ngữ qua plugin *(plugin)*
- Xem trước Markdown với bảng, công thức và chú thích

### Chuyển tiếp cổng và mạng

- Chuyển tiếp cục bộ, từ xa và SOCKS5 động với quy tắc đã lưu có thể chạy khi kết nối
- Phát hiện cổng đang lắng nghe trên máy chủ và chuyển tiếp bằng một cú nhấp
- Đường hầm SSH độc lập không cần terminal, có thể khởi chạy cùng ứng dụng
- Khôi phục chuyển tiếp sau kết nối lại; quy tắc dừng thủ công vẫn dừng
- Proxy toàn ứng dụng và chứng chỉ tin cậy hệ thống, gồm CA doanh nghiệp, cho yêu cầu mạng
- Sơ đồ kết nối và gỡ lỗi socket tạm thời

### Màn hình từ xa

- RDP/VNC có bàn phím, chuột, clipboard, tỷ lệ hiển thị và kết nối lại *(plugin)*
- RDP nhiều màn hình với cấu hình chất lượng mạng *(plugin)*
- VNC qua VeNCrypt TLS *(plugin)*
- RDP/VNC qua máy trung gian SSH *(plugin)*
- Ứng dụng WSLg trong trình xem tích hợp (Windows)

### Công cụ máy chủ

- Dùng trên máy cục bộ và mọi máy chủ SSH
- CPU, bộ nhớ, swap, đĩa, mạng, tải và độ trễ với đồ thị xu hướng
- Tiến trình (tín hiệu, dừng/tiếp tục, nice), dịch vụ (systemd, launchd, BSD, Windows), tác vụ định kỳ và gói; nâng cấp/gỡ cài đặt winget trên Windows
- Nhật ký với mẫu sẵn và theo dõi liên tục, cổng lắng nghe có cảnh báo phơi bày và hệ thống tệp cảnh báo thiếu dung lượng
- Container Docker: trạng thái, cổng, khởi động, dừng, khởi động lại và nhật ký
- Giám sát GPU/NPU: NVIDIA, AMD ROCm, Hygon DCU, Huawei Ascend, Intel XPU, Moore Threads MUSA và Cambricon
- Phiên tmux, Zellij, Herdr: gắn, tạo, chia và đổi tên
- Bảng pool/ma trận kết nối và độ trễ SSH trên thanh công cụ terminal

### AI OxideSens

- OpenAI (gồm Responses API), Anthropic, Gemini, DeepSeek, xAI Grok, Xiaomi MiMo, Ollama hoặc endpoint tương thích OpenAI, với điều chỉnh mức suy luận
- Ngữ cảnh từ terminal, tệp và cài đặt bạn chọn; công cụ chạy lệnh, điều khiển chương trình tương tác, sửa tệp và truyền dữ liệu
- Chế độ chỉ đọc, mặc định hoặc không cần duyệt; quyền theo loại công cụ, chính sách lệnh và kiểm tra thêm với lệnh phá hủy hoặc liên quan thông tin xác thực
- Đính kèm ảnh, phân nhánh hội thoại, sửa tin nhắn và xếp hàng câu hỏi tiếp theo
- Agent con chạy song song, tác vụ nền và tiếp tục sau gián đoạn
- Codex/Claude Code qua ACP và bộ khởi chạy OpenCode, Antigravity, Cursor, Grok Build, GitHub Copilot CLI, Qwen Code, Kimi CLI *(plugin)*
- Máy chủ MCP qua stdio, Streamable HTTP hoặc SSE cũ
- Điều khiển MCP bên ngoài: trình soạn thảo và công cụ AI dùng kết nối đã lưu qua OxideTerm, với quyền cấp/thu hồi theo nhóm công cụ
- [Agent Skills](../../docs/agent-skills.md) (`SKILL.md`) với hoàn thành `/skill-name`
- Kho ghi chú Markdown với tìm BM25 và vector HNSW, tách từ CJK, sơ đồ Mermaid trong bản xem trước

### Bảo mật

- Khóa ứng dụng bằng Touch ID (macOS) hoặc Windows Hello (Windows), kết nối vẫn hoạt động
- Gợi ý mật khẩu sudo/su đã lưu ở lời nhắc, giới hạn theo máy cục bộ, kết nối hoặc mẫu máy; TOTP tùy chọn cho lời nhắc hai bước
- Kiểm toán thao tác có lọc, thời hạn lưu và xuất JSON/CSV
- Ghi phiên được mã hóa, phát lại, tua và điều chỉnh tốc độ
- Xem xét quyền trước khi kích hoạt plugin

### Đồng bộ và sao lưu

- WebDAV, lưu trữ tương thích S3 (gồm Alibaba Cloud OSS), kho Git, GitHub Gist, Dropbox, OneDrive, Google Drive hoặc HTTP JSON
- Chọn dữ liệu đồng bộ, hợp nhất thay đổi nhiều thiết bị và xử lý từng xung đột
- Sao lưu hoàn tác và phục hồi đồng bộ bị gián đoạn
- Gói `.oxide` mã hóa cho kết nối, chuyển tiếp, cài đặt, lệnh nhanh và khóa; bỏ qua, đổi tên, thay thế hoặc hợp nhất khi nhập
- Chế độ di động chạy từ thư mục với kho khóa mã hóa riêng
- Cập nhật trong ứng dụng qua kênh ổn định hoặc beta

### Tùy chỉnh

- Giao diện sáng/tối/theo hệ thống, chủ đề ứng dụng và terminal riêng cùng trình sửa chủ đề
- Hình nền, hoạt ảnh, video; hiệu ứng sương, hạt, sao băng và vân sáng dưới nước, cùng gói nội dung bổ sung *(plugin)*
- Độ trong suốt và kiểu góc cửa sổ
- Danh sách phông với CJK, chữ ghép và độ đậm riêng; kiểu chữ trình soạn thảo độc lập
- Tỷ lệ giao diện, kiểu con trỏ, khoảng đệm và chuyển động
- Phím tắt tùy chỉnh và cài đặt tìm kiếm/sắp xếp lại được
- 11 ngôn ngữ giao diện, nhập IME và văn bản hai chiều

### Plugin

| Loại | Cách chạy | Quyền truy cập |
|---|---|---|
| **Manifest** | Tiện ích khai báo | Không có; không chạy mã |
| **WASM** | Wasmtime/WASI hoặc tiến trình phụ | Chỉ khả năng được máy chủ cấp |
| **Tiến trình** | Chương trình cục bộ thông thường | Như chương trình cục bộ; không có sandbox hệ điều hành |

Chỉ cài plugin tiến trình từ nguồn đáng tin. Plugin 1.x có thể còn trong danh sách nhưng 2.x không thực thi.

### Công cụ dòng lệnh

Cài `oxideterm` từ **Cài đặt → Chung → CLI Companion**. Dùng chung cấu hình để kiểm tra, chạy kịch bản, sao lưu không cần cửa sổ hoặc mở thẻ từ shell. Lệnh thay đổi có kế hoạch `--dry-run`, chỉ áp dụng với `--yes` và giữ bản sao để hoàn tác.

```bash
oxideterm terminal --cwd ~/project
oxideterm doctor --strict
oxideterm connections search prod
oxideterm forwards list --format json
oxideterm oxide export ./profile.oxide --connection prod --password-stdin
```

Hoàn thành lệnh cho Bash, Zsh, Fish, PowerShell, Elvish. [Hướng dẫn CLI (tiếng Anh)](../../docs/user-guide/en/cli.md) liệt kê toàn bộ lệnh.

---

<a id="install"></a>

## Cài đặt

Tải OxideTerm từ [bản phát hành mới nhất](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest).

| Hệ điều hành | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Trình cài đặt (`.exe`) | Trình cài đặt (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

Trang phát hành còn có bản nén di động, chữ ký và `sha256sums.txt` để kiểm tra tải xuống.

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

### Nếu cửa sổ không hiển thị

Đặt `OXIDETERM_RENDER_PROFILE=compatibility` trong môi trường rồi khởi động lại OxideTerm.

### Bước đầu

1. Mở **quản lý phiên**, thêm máy chủ hoặc nhập từ `~/.ssh/config`.
2. Kết nối để mở terminal.
3. Từ cùng máy chủ, mở SFTP, chuyển tiếp cổng hoặc IDE.
4. Để dùng OxideSens, thêm nhà cung cấp AI trong **Cài đặt**.

[Tài liệu](https://oxideterm.app) giới thiệu phần còn lại. Để tự biên dịch, xem [Biên dịch từ mã nguồn](#build-from-source).

---

## Quyền riêng tư và bảo mật

- Mật khẩu và khóa nằm trong kho khóa hệ thống (macOS Keychain, Windows Credential Manager, libsecret trên Linux). Khóa AI không được ghi vào nhật ký.
- Bí mật trong bộ nhớ được xóa bằng zeroize khi giải phóng.
- Khóa máy chủ được tin ở lần đầu rồi kiểm tra với `~/.ssh/known_hosts`; thay đổi khóa sẽ bị từ chối.
- Văn bản gửi cho AI được che thông tin xác thực trước.
- Ghi phiên mặc định tắt, mã hóa trên thiết bị, không đồng bộ đám mây và không thu phím bạn nhập.
- Thông báo và kiểm toán mặc định tắt. Khi bật, dữ liệu vẫn ở thiết bị, chi tiết nhạy cảm được mã hóa và chỉ bao gồm thao tác trong OxideTerm.
- Gói `.oxide` mã hóa ChaCha20-Poly1305 với khóa Argon2id (256 MB bộ nhớ, 4 vòng).

---

<a id="build-from-source"></a>

## Biên dịch từ mã nguồn

Cần Rust (rustup cài toolchain cố định trong `rust-toolchain.toml`) và môi trường desktop chạy được GPUI.

```bash
# Run the app
cargo run

# Build the oxideterm CLI
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Với Nix: `nix build .#oxideterm`, `nix run .#oxideterm` hoặc `nix develop`. CLI được đặt ở `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

### Kiến trúc tổng thể

Giao diện, terminal và SSH chạy cùng một tiến trình Rust. Agent từ xa tùy chọn và công cụ nền tảng chạy riêng. Đầu ra SSH/PTY cục bộ vào trực tiếp `TerminalState` rồi GPUI vẽ, không qua WebView, cầu JSON hay xterm.js.

`NodeRouter` cấp kết nối từ pool DashMap `SshConnectionRegistry`. Terminal, SFTP, chuyển tiếp và trình soạn thảo chia sẻ kết nối theo máy chủ, trừ terminal đặt dùng kết nối riêng. Trạng thái: `connecting → active → idle → link_down → reconnecting`. Máy trung gian lỗi đưa máy phía sau vào `link_down`. Mỗi quy tắc chuyển tiếp có bộ lắng nghe riêng. Sau kết nối lại, SFTP được lấy lại trên kết nối mới; thao tác cũ không bao giờ âm thầm chuyển sang kết nối khác.

```text
crates/
├── oxideterm-gpui-app/        # Desktop app: windows, workspace, settings
├── oxideterm-gpui-terminal/   # Terminal panes: input, selection, overlays
├── oxideterm-terminal/        # Terminal model and escape-sequence parsing
├── oxideterm-ssh/             # SSH connections, NodeRouter, jump hosts
├── oxideterm-sftp/            # SFTP sessions and transfers
├── oxideterm-forwarding/      # Port forwarding runtime
├── oxideterm-gpui-ide/        # Remote IDE
├── oxideterm-ai/              # OxideSens: providers, MCP, RAG
├── oxideterm-cli/             # The oxideterm command
├── oxideterm-i18n/            # Locale catalogs for the 11 interface languages
└── gpui-ce/                   # Patched copy of GPUI
```

Đọc thêm (tiếng Anh): [phát triển](../../docs/development/README.md), [bản đồ crate](../../docs/development/crate-map.md), [kiến trúc](../../docs/user-guide/en/architecture.md).

---

## Đóng góp

Hoan nghênh mã, tài liệu, dịch, plugin, thử nghiệm và tái hiện lỗi. Thay đổi lớn nên mở issue trước. [CONTRIBUTING.md](../../docs/CONTRIBUTING.md) hướng dẫn chuẩn bị PR.

Gói chẩn đoán đã che dữ liệu nhạy cảm giúp sửa lỗi dễ hơn:

```bash
oxideterm report --bundle ./oxideterm-report.zip
```

Khi chạy từ mã nguồn, dùng `cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip`.

### Người đóng góp

Cảm ơn tất cả những người giúp OxideTerm ngày càng tốt hơn.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="Những người đóng góp cho OxideTerm">
  </a>
</p>

---

## Ghi nhận

OxideTerm sử dụng các dự án mã nguồn mở sau:

- [russh](https://github.com/warp-tech/russh) và [ring](https://github.com/briansmith/ring): SSH và mật mã
- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui): framework giao diện GPU của Zed
- [alacritty_terminal](https://github.com/alacritty/alacritty): mô phỏng terminal (VT100–VT500)
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty): shell cục bộ, dùng ConPTY trên Windows
- [Tokio](https://tokio.rs): runtime bất đồng bộ
- [Wasmtime](https://wasmtime.dev/): chạy plugin WASM
- [tree-sitter](https://tree-sitter.github.io/): tô sáng cú pháp trình soạn thảo

---

## Giấy phép

OxideTerm dùng [GPL-3.0-only](../../LICENSE), không có hạn chế bổ sung. Thông báo phụ thuộc nằm trong [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) và [NOTICE](../../NOTICE).

Chỉ dùng OxideTerm trên hệ thống bạn sở hữu hoặc được phép truy cập. [LEGAL.md](../../LEGAL.md) chứa toàn bộ thông báo pháp lý.

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
