<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm：在一個工作區中管理你的伺服器" width="920">

# ⚡ OxideTerm

**免費的原生 SSH 用戶端與遠端維運工作區，內建使用自備金鑰的 AI 助手。**

SSH · Mosh · Telnet · 序列埠 · RDP/VNC · SFTP · 連接埠轉送 · 內建編輯器，整合在同一個 GPU 繪製的應用程式中。
無需帳號，無需訂閱，不蒐集遙測，不使用 Electron。

[![最新版本](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![平台](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![授權條款](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Stars](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**下載**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**文件**](https://oxideterm.app) ·
[**更新紀錄**](../../.github/release-notes/stable-changelog.md) ·
[**報告問題**](https://github.com/AnalyseDeCircuit/oxideterm/issues)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md) | [Русский](README.ru.md)

</div>

---

## 快速上手

1. **安裝 OxideTerm。** 從[最新發布頁面](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)下載安裝套件；各平台的說明見下方[安裝](#install)一節。
2. **新增伺服器。** 開啟工作階段管理器，新增 SSH 連線，或從 `~/.ssh/config` 匯入主機。
3. **連線伺服器。** 開啟終端。主機金鑰會與 `~/.ssh/known_hosts` 中的記錄核對。
4. **使用工作區中的其他功能。** 在同一節點上開啟 SFTP、連接埠轉送或內建編輯器。預設情況下，它們共用一個 SSH 連線。
5. **可選：啟用 AI。** 在設定中新增你自己的 OpenAI、Anthropic、Gemini、Ollama 或相容 OpenAI 的服務端點，即可啟用 OxideSens。

需要詳細導覽，請參閱[文件](https://oxideterm.app)。

---

## 功能一覽

| | |
|---|---|
| **終端與通訊協定** | 本機 Shell、SSH、Mosh、Telnet、序列埠、分割窗格、多跳連線、SSH Agent 與 Agent 轉送、雙因素認證和 TOTP 認證資料、X11 轉送、Shell 整合、指令標記、可設定的工作階段日誌、錄製、Sixel 和 Kitty 圖形、trzsz 傳輸 |
| **tmux 與廣播** | 原生 `tmux -CC` 控制模式，支援窗格配置和拖動分隔線；具名廣播群組；可按計畫重複傳送輸入的進階多目標指令傳送工具 |
| **連線可靠性** | 寬限期重新連線讓 TUI 應用程式在短暫斷網期間繼續執行，隨後恢復連接埠轉送、傳輸和已開啟的編輯器檔案 |
| **檔案與編輯** | SFTP 雙欄檔案管理器、帶限速和預計完成時間的傳輸佇列、書籤；內建遠端編輯器，支援安全寫入、衝突處理和工作區恢復 |
| **網路功能** | 本機、遠端和動態 SOCKS5 轉送、已保存的規則、遠端連接埠偵測、連線拓樸、臨時 Socket 除錯 |
| **遠端桌面** | 內建 RDP 和 VNC，支援剪貼簿與輸入 |
| **主機維運** | 監控行程、服務、日誌、連接埠、任務、磁碟、套件、容器和 tmux |
| **AI 與自動化** | 使用自備金鑰的 OxideSens、MCP、本機 RAG、Agent Skills、經批准的工作區操作、獨立命令列工具 |
| **回顧與稽核** | 可選的通知與稽核工作區，以及加密工作階段錄製（兩者預設均關閉） |
| **同步與可攜** | 加密雲同步、可攜式 `.oxide` 檔案包 |
| **個性化** | 主題、背景圖片、可設定的快速鍵、快速指令、11 種介面語言 |

---

## 為什麼選擇 OxideTerm

- **免費，本機優先。** 無需帳號，無需訂閱，不蒐集遙測。連線與維運資料由你掌控。
- **每台伺服器，一個工作區。** 終端、SFTP、連接埠轉送、RDP/VNC、編輯器、監控和 AI 都連線到同一節點，不再像各自獨立的工具。
- **原生應用程式，而非瀏覽器套殼。** 介面透過 [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) 直接在 GPU 上繪製，不使用 Electron，也不捆綁 WebView。
- **AI 由你決定。** OxideSens 使用你自己的服務供應商和金鑰，只執行你批准的操作。
- **能應對短暫斷網。** 寬限期重新連線會先探測原連線 30 秒，再決定是否替換，讓 TUI 應用程式有機會在短暫斷網後繼續執行。
- **純 Rust SSH。** SSH 使用 `russh` 與 `ring`，不依賴 OpenSSL 或 libssh2。

---

## 記憶體佔用

**原生重寫後，macOS 上的閒置記憶體約為舊版的四分之一，Windows 上約為八分之一。** 以下是維護者在從 Tauri 1.x 遷移到原生 GPUI 2.0 時記錄的觀察結果：

| 平台 | Tauri 1.x（閒置） | 原生 2.0（閒置） | 降幅 |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 約 74% |
| Windows | 182.4 MB | 23.5 MB | 約 87% |

舊版的總佔用包含 OxideTerm 及其關聯的 WebView 行程。原生版不再需要這些瀏覽器行程。

![附系統行程螢幕截圖的閒置記憶體對比：Tauri 1.x 與原生 2.0](../../docs/screenshots/oxideterm-memory-comparison.png)

---

## 螢幕截圖

| 帶 OxideSens 的 SSH 終端 | SFTP 檔案管理器 |
|---|---|
| ![帶 OxideSens AI 的 SSH 終端](../../docs/screenshots/terminal/SSHTERMINAL.png) | ![帶傳輸佇列的 SFTP 雙欄檔案管理器](../../docs/screenshots/sftp/sftp.png) |

| 內建 IDE | 智慧連接埠轉送 |
|---|---|
| ![內建 IDE 模式](../../docs/screenshots/miniIDE/miniide.png) | ![支援自動偵測的智慧連接埠轉送](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png) |

<details>
<summary><b>觀看 OxideSens 根據自然語言請求開啟終端</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens 在 OxideTerm 中開啟終端" width="720">
</a>

</details>

---

## OxideSens AI

OxideSens 是可選的助手，可查看正在執行的工作階段，並且**僅在你批准後**執行工作區操作。

- **自備金鑰。** 支援 OpenAI、Anthropic（Claude）、Google Gemini、Ollama 及相容 OpenAI 的服務端點，並提供適配各服務供應商的推理控制選項。不使用平台點數。
- **MCP 與 Agent Skills。** 可連線 MCP 伺服器（stdio 和 SSE），並載入有明確使用範圍的 Agent Skills。
- **本機知識庫（RAG）。** BM25 全文檢索與向量索引相結合。
- **上下文由你控制。** 由你決定批准哪些工作區上下文和操作，指令策略規則仍然適用。
- **認證資料遮蔽。** 傳送給服務供應商的訊息會先依認證資料特徵遮蔽敏感內容。
- **金鑰保存在作業系統鑰匙圈中**，不會寫入結構化日誌。

---

<a id="plugins"></a>

## 外掛

OxideTerm 支援三種外掛方式：

| 類型 | 執行方式 | 邊界 |
|---|---|---|
| **僅清單** | 宣告式擴充，不含程式碼 | 不含可執行程式碼 |
| **WASM** | Wasmtime/WASI 或獨立輔助行程 | 主機呼叫受控，依能力範圍限制權限 |
| **行程** | 普通本機行程 | 受信任的本機程式碼，**不受**作業系統沙箱隔離 |

舊版 Tauri（1.x）的 ESM 外掛可能仍會顯示在列表中，但原生 2.x 應用程式不會執行它們。請只安裝來自可信來源的行程外掛。

---

## 安全與隱私

| 主題 | 實現方式 |
|---|---|
| **儲存的認證資料** | 作業系統鑰匙圈（macOS Keychain、Windows Credential Manager、libsecret） |
| **記憶體中的秘密資料** | 含秘密資料的類型和臨時緩衝區在支援的所有權邊界使用 `zeroize` 清除 |
| **主機金鑰** | 首次使用時依據 `~/.ssh/known_hosts` 建立信任；拒絕未預期的金鑰變更 |
| **可攜匯出** | `.oxide` 檔案包使用 ChaCha20-Poly1305 與 Argon2id（256 MB 記憶體，4 次迭代） |
| **AI 上下文** | 內容送達服務供應商前，先依認證資料特徵遮蔽敏感內容；上下文和操作由你批准 |
| **工作階段錄製** | 預設關閉；加密儲存在你的裝置上，不參與雲同步；不錄製鍵盤輸入 |
| **稽核** | 預設關閉；資料保留在你的裝置上，敏感細節加密儲存 |
| **命令列修改操作** | 改變狀態的指令提供試執行計畫、`--yes` 確認保護和回復備份 |
| **外掛** | 見[外掛](#plugins) |
| **遙測** | 無 |

**合法使用。** OxideTerm 採用 GPL-3.0-only 授權條款，無額外限制。請僅存取你擁有或獲明確授權存取的系統、網路和裝置，並遵守適用法律。不得使用 OxideTerm 進行未授權存取、干擾服務或繞過存取控制。

---

## 目前限制

安裝前，請瞭解以下情況：

- 僅支援桌面平台（macOS、Windows、Linux），沒有行動版應用程式。
- 專案更新較快，發布頻繁。請參閱[更新紀錄](../../.github/release-notes/stable-changelog.md)和[未解決的問題](https://github.com/AnalyseDeCircuit/oxideterm/issues)。
- 稽核與工作階段錄製需主動啟用，且只反映 OxideTerm 自身能夠觀察到的活動。
- 行程外掛不受作業系統沙箱隔離。
- 如果繪製器在你的機器上無法正常執行，可嘗試相容模式：`OXIDETERM_RENDER_PROFILE=compatibility`。

---

<a id="for-developers"></a>

## 開發者資訊

<details>
<summary><b>從原始碼執行</b></summary>

**要求：** Rust 工具鏈（edition 2024），以及能夠執行 GPUI 的桌面環境。

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

使用 Nix：`nix build .#oxideterm`、`nix run .#oxideterm` 或 `nix develop`。

命令列工具的建置產物位於 `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`。

</details>

<details>
<summary><b>命令列介面</b></summary>

無介面的 `oxideterm` 命令列工具無需啟動應用程式即可執行，適用於自動化、CI 和診斷。它涵蓋設定、連線、連接埠轉送、外掛、快速指令、秘密資料、可攜檔案包、診斷、報告、批次計畫、備份和雲同步。

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
<summary><b>架構</b></summary>

介面與終端及 SSH 後端共用一個 Rust 行程；可選的遠端 Agent 和平台輔助程式位於該行程之外。終端位元組直接更新 `TerminalState`，GPUI 根據該狀態繪製，無需經過 JSON、WebSocket、Base64 或 xterm.js 解析。

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

| 方面 | 捆綁瀏覽器的方案 | OxideTerm |
|---|---|---|
| 繪製 | 瀏覽器引擎與網頁配置 | GPU 繪圖表面上的 GPUI |
| 終端資料流 | WebSocket → JS 事件循環 → xterm.js | Rust 輸入 → `TerminalState` → GPUI 繪製 |
| 連線生命週期 | 分散在前端和後端 | 同一行程內統一處理連線與重新連線 |
| AI 上下文 | 透過應用程式橋接層複製 | 根據目前工作區建立，並由使用者批准 |
| 命令列工具 | 需要桌面應用程式保持執行 | 獨立程式，直接連結各 crate |

**連線池。** `SshConnectionRegistry` 以 `DashMap` 為基礎，透過 `NodeRouter` 使用。終端窗格、SFTP、連接埠轉送和編輯器可共用每個節點上的一個物理 SSH 連線，終端策略也允許改用專用連線。每個連線遵循 `connecting → active → idle → link_down → reconnecting` 的狀態變化。跳板機故障會將下游節點標記為 `link_down`。AI 和外掛透過能力控制代碼與主機快照使用相關功能，不註冊為連線消費者。

**寬限期重新連線。**

1. 偵測連線保活逾時。
2. 保存終端窗格、SFTP 傳輸、連接埠轉送和編輯器檔案的快照。
3. 探測原連線 30 秒，讓 TUI 應用程式有機會在短暫斷網後繼續執行。
4. 建立新連線，恢復連接埠轉送、繼續傳輸並重新開啟編輯器檔案。

SFTP 工作階段帶有連線世代識別碼：重新連線後，符合條件的工作階段會重新取得，但舊世代的操作絕不會被悄悄轉移到新連線。

**連接埠轉送。** 獨立 crate，支援 `-L`、`-R` 和 `-D`（SOCKS5）。每個 SSH 通道由一個 `ssh_io` 任務獨立管理，關鍵執行路徑上沒有共用互斥鎖。

**純 Rust SSH。** `russh` 與 `ring`：完整 SSH2、ChaCha20-Poly1305 和 AES-GCM、Ed25519/RSA/ECDSA 金鑰、Unix（`SSH_AUTH_SOCK`）與 Windows（`\\.\pipe\openssh-ssh-agent`）上的 SSH Agent，以及每跳獨立認證的多跳鏈路。

**技術堆疊**

| 層級 | 技術 |
|---|---|
| 介面 | GPUI（Zed 的 GPU 介面框架） |
| 執行環境 | Tokio、DashMap |
| SSH | `russh` 與 `ring`（不依賴 OpenSSL 或 libssh2） |
| 本機 PTY | `portable-pty`（Windows 上使用 ConPTY） |
| 終端模擬 | `alacritty_terminal`（VT100–VT500、Sixel、Kitty 圖形） |
| 編輯器 | tree-sitter 語法高亮、自訂緩衝區 |
| 加密 | ChaCha20-Poly1305、Argon2id |
| 外掛 | Wasmtime/WASI、獨立輔助行程中的 WASM，以及行程方式 |
| AI 流式響應 | SSE（OpenAI、Anthropic、Gemini），在行程內處理 |
| RAG | BM25 與 HNSW 向量索引相結合，融合排序，使用 CJK 雙字分詞器 |
| 國際化 | `oxideterm-i18n`（11 種語言） |

</details>

---

<a id="install"></a>

## 安裝

[**下載最新版本**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

| 作業系統 | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG（Intel） | DMG（Apple Silicon） |
| **Windows** | 安裝程式（`.exe`） | 安裝程式（`.exe`） |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

請使用發布頁面中的 `sha256sums.txt` 檔案驗證下載內容。可攜版壓縮包和簽名也列在該頁面。

### macOS

如果 Gatekeeper 阻止執行應用程式，請移除隔離標記：

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

如果 SmartScreen 顯示警告，請選擇**其他資訊 → 仍要執行**。

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

想自行建置？請查看[開發者資訊](#for-developers)中的**從原始碼執行**。

---

## 參與貢獻

歡迎貢獻 Rust 程式碼、文件、翻譯、外掛、測試，以及重現問題。較大的改動請先提交 issue 討論。

附上已遮蔽敏感資訊的診斷包，能讓問題報告更有幫助：

```bash
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

可重現的缺陷與回歸問題會優先處理。功能請求會根據範圍、安全性，以及是否符合 OxideTerm 遠端伺服器工作區的方向進行評估。如果 OxideTerm 對你的工作有幫助，GitHub Star、可重現的問題報告、翻譯修正或外掛，都能幫助專案繼續發展。

### 貢獻者

感謝每一位幫助 OxideTerm 變得更好的貢獻者。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="OxideTerm 貢獻者">
  </a>
</p>

---

## 授權條款

**GPL-3.0-only。** 相依套件署名見 [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md)，其他聲明見 [`NOTICE`](../../NOTICE)。

**使用以下專案打造：** [russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
