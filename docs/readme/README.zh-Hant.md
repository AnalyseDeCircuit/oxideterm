<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <em>如果你想要一個沒有 Electron、WebView、遙測或訂閱的本地優先 SSH 工作區，請給 OxideTerm 一顆 Star，讓更多 SSH 使用者發現它。</em>
</p>

<p align="center">
  <strong>本地優先 SSH 工作區：圍繞一個遠端節點整合 shell、SFTP、連接埠轉發、trzsz、遠端編輯和 BYOK AI。</strong>
  <br>
  <strong>零 WebView。零 OpenSSL。零遙測。零訂閱。BYOK 優先。全棧純 Rust。</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="版本">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="平台">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="授權">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub><a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> 的原生 Rust 重寫 —— GPU 渲染、零 WebView，使用 <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>（Zed 的渲染框架）</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

---

## 為什麼選擇 Native？

| 如果你在意... | OxideTerm Native 提供... |
|---|---|
| SSH 工作區，而不只是 shell | 一個遠端節點同時擁有終端、SFTP、轉發、trzsz、輕量 IDE、監控和 AI 上下文 |
| 本地 shell 與遠端 SSH 共存 | zsh/bash/fish/pwsh/WSL2 與遠端 SSH 在同一工作流中運行 |
| 不需要雲端帳號 | SSH、SFTP、轉發、本地 shell 和設定都本地優先 |
| BYOK AI | 使用自己的 OpenAI、Anthropic、Gemini、Ollama 或相容端點 |
| 沒有 WebView | GPUI 直接繪製 GPU 介面，沒有 DOM、CSS、JavaScript |
| 熱路徑無序列化 | 終端位元組直接變更 Rust 狀態，無 WebSocket/JSON/Base64 開銷 |
| 無 OpenSSL 負擔 | `russh` + `ring`，純 Rust SSH |
| 重連穩定性 | Grace Period 會先探測舊連線，網路抖動時 TUI 應用更容易保活 |
| 遠端檔案工作 | 內建 SFTP 與原生 IDE 瀏覽、預覽、傳輸和編輯遠端檔案 |
| 憑證安全 | OS Keychain；`.oxide` 使用 ChaCha20-Poly1305 + Argon2id 加密 |

## 它是什麼 / 不是什麼

OxideTerm Native 是一個**純 Rust 原生桌面 SSH 工作區**。Tauri 版本中的終端、SFTP、轉發、編輯、AI、雲同步、外掛和 CLI 都在 Rust 與 GPUI UI 層中重新實作。

它不是 Electron、Tauri、網頁終端或託管服務。沒有 Chromium、WebView、JavaScript 或 CSS；所有介面都由 GPUI 直接繪製到 GPU surface。

## 與 WebView 版本的差異

| 方面 | WebView/Tauri | Native |
|---|---|---|
| 渲染 | Chromium/Safari/WebKit2GTK + CSS | GPUI GPU surface，即時模式，純 Rust |
| 終端資料流 | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC 開銷 | 每次命令都要 JSON-RPC | 進程內函式呼叫 |
| SSH keepalive | JavaScript timer | Rust async task |
| 外掛執行環境 | 瀏覽器沙箱中的 ESM | wasmtime WASM + typed Rust host API |
| CLI | 依賴桌面應用運行 | 獨立二進位，直接連結 crate |
| 發佈包體積 | 通常約 150–200 MB 安裝包 | 目前 macOS arm64：壓縮 portable/DMG 約 50–60 MB；裸 release 二進位約 132 MB |

## 功能概覽

| 分類 | 功能 |
|---|---|
| 終端 | 本地 PTY、SSH、分屏、shell integration、命令標記、asciicast 錄製/回放、trzsz、Sixel/Kitty graphics、渲染策略 |
| SSH 與認證 | 連線池、無限 ProxyJump、Grace Period 重連、Host-key TOFU、SSH Agent 轉發、密碼/金鑰/憑證/鍵盤互動認證 |
| SFTP / IDE | 雙欄瀏覽器、傳輸佇列、預覽、書籤、原子寫入、遠端檔案樹、多分頁編輯、衝突處理 |
| 轉發 | Local、Remote、Dynamic SOCKS5，保存規則，重連恢復，死亡回報，閒置逾時 |
| AI | OxideSens 支援 OpenAI、Anthropic、Gemini、Ollama/相容端點、MCP、RAG、命令審批 |
| 雲同步與 `.oxide` | push/pull/apply/resolve，S3/WebDAV/Git，rollback backup；加密匯入匯出連線、轉發、設定、快捷命令和外掛設定 |
| 外掛與 CLI | WASM 沙箱、native host API、外掛設定；CLI 含 settings、connections、forwards、quick-commands、plugins、secrets、cloud-sync、backup、report 等命令 |

## 內部實作

### 單進程，零橋接

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

UI 與 SSH/終端後端之間沒有序列化邊界。終端位元組直接修改 `TerminalState`，GPUI 讀取狀態並發出 GPU draw call。

### 純 Rust SSH、智慧重連與連線池

原生版本直接連結與 Tauri 版本同源的 `russh` stack：無 C/OpenSSL 依賴，支援 SSH2、SFTP、轉發、Agent、ProxyJump 和多種金鑰演算法。重連流程會快照終端、SFTP、轉發和 IDE 狀態，先給舊連線 30 秒 Grace Period，再必要時重建並恢復工作區。

## 快速開始

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

## 安全

| 關注點 | 實作 |
|---|---|
| 密碼與金鑰 | macOS Keychain / Windows Credential Manager / libsecret |
| 記憶體中的秘密 | `zeroize` / `Zeroizing` |
| 診斷與 AI 上下文 | 只輸出路徑、計數、旗標和 hint；送給 AI 前脫敏 |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI 寫入 | dry-run plan、`--yes` 保護和 rollback backup |
| 外掛 | wasmtime 隔離與 capability-based host API |

## 路線圖與貢獻

- [x] SSH Agent 轉發、Grace Period 重連、GPUI 桌面 shell
- [x] 無 WebSocket 的進程內終端資料流
- [x] SFTP、轉發、IDE、AI、雲同步、外掛、CLI
- [ ] 完整 ProxyCommand、審計日誌、打包發布構建

## Provider 中立性

OxideTerm 是 BYOK 優先，並保持 provider 中立。

Provider 集成是為了讓使用者連接他們已經信任的工具。它們不是排行榜，不是廣告牌，也不是獎勵那些最熱情開口者的機制。

是否寫進文件，取決於相容性、可維護性、安全性和真實使用者價值。可見度跟隨有用性，而不是熱情程度。

已有 Tauri 功能遷移到 native 時，應保持行為、標籤、互動狀態和工作流一致。新 crate 必須承擔真實職責，不能只是 re-export 或堆放函式。

## 授權與致謝

**GPL-3.0-only**。第三方聲明記錄在 `NOTICE`。感謝 `russh`、`GPUI`、`alacritty_terminal`、`portable-pty`、`wasmtime` 和 `tree-sitter`。
