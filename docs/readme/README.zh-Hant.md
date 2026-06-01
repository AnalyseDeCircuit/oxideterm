<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>OxideTerm 的下一代零 WebView 版本。</strong>
  <br>
  連接一次遠端機器，就能在一個原生 Rust 工作區裡處理它的 Shell、檔案、連接埠、傳輸、輕量編輯器、序列埠主控台和 BYOK AI。
  <br>
  原生 GPUI 應用 · 純 Rust SSH · 核心 SSH 工作流無需帳號
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
  <sub><a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> 的下一代原生版本 —— GPU 渲染、零 WebView，使用 <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>（Zed 的渲染框架）</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens 在 OxideTerm 中開啟終端" width="920">
</a>

*觀看 OxideSens 依照使用者請求，在 OxideTerm 中開啟一個終端。*

</div>

---

> **發布狀態：** OxideTerm Native 正在作為 OxideTerm 的下一代主版本準備中。公開安裝包尚未發布，目前請從原始碼執行；在 native 安裝包準備好之前，當前打包發布仍在 Tauri 版本線上。

## 你可以做什麼

- 在一個原生工作區裡管理 SSH 終端、SFTP、連接埠轉發、序列埠主控台、本地 Shell 和輕量編輯
- 透過寬限期重連，讓遠端工作更能承受網路抖動
- 使用你自己的 AI 提供商檢查即時工作階段，並執行經批准的工作區操作

---

## 為什麼選擇 Native？

| 如果你在意... | OxideTerm Native 提供... |
|---|---|
| 一個遠端節點，多種工具 | 終端、SFTP、連接埠轉發、trzsz、原生 IDE、監控和 AI 上下文都掛在同一個 SSH 工作區上 |
| 零 WebView 原生外殼 | GPUI 直接在 GPU surface 上繪製桌面 UI，沒有 DOM、CSS、JavaScript、Chromium 或 WebKit 執行階段 |
| 本地優先 SSH 工作流 | SSH、SFTP、連接埠轉發、本地 Shell、序列埠終端和設定管理都無需註冊 |
| BYOK AI，而不是平台點數 | OxideSens 使用你自己的 OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible 端點，並支援 MCP 與 RAG |
| 重連穩定性 | 寬限期會先探測舊連線 30 秒再替換它，短暫網路中斷時 TUI 應用仍有機會存活 |
| 純 Rust SSH 與憑證安全 | `russh` + `ring`，無 OpenSSL/libssh2；密碼和 API 金鑰保存在 OS Keychain，`.oxide` 使用 ChaCha20-Poly1305 + Argon2id |

## 它是什麼 / 不是什麼

OxideTerm Native 專注於和 OxideTerm 相同的**本地優先 SSH 工作區**，只是重建為純 Rust GPUI 桌面應用。它面向希望終端、檔案、連接埠、傳輸、輕量編輯、序列埠主控台和 AI 上下文圍繞自己的機器與遠端節點展開的使用者。

它還不是目前穩定下載線，也不是託管雲端 Agent 平台。它也不是 Electron、Tauri 或網頁終端：沒有 Chromium、WebView、JavaScript 或 CSS。

---

## 截圖

Native UI 遵循目前 Tauri 版本相同的 OxideTerm 工作區模型與視覺語言。

<table>
<tr>
<td align="center"><strong>SSH 終端 + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="帶有 OxideSens AI 側邊欄的 SSH 終端" /></td>
<td align="center"><strong>SFTP 檔案管理員</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="SFTP 雙窗格檔案管理員與傳輸佇列" /></td>
</tr>
<tr>
<td align="center"><strong>內建 IDE</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="內建 IDE 模式" /></td>
<td align="center"><strong>智慧連接埠轉發</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="帶自動偵測的智慧連接埠轉發" /></td>
</tr>
</table>

---

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
| 終端 | 本地 PTY、SSH、本地序列埠終端、分屏、shell integration、命令標記、asciicast 錄製/回放、trzsz、Sixel/Kitty graphics、渲染策略 |
| SSH 與認證 | 連線池、無限 ProxyJump、Grace Period 重連、Host-key TOFU、SSH Agent 轉發、密碼/金鑰/憑證/鍵盤互動認證 |
| SFTP / IDE | 雙欄瀏覽器、傳輸佇列、預覽、書籤、原子寫入、遠端檔案樹、多分頁編輯、衝突處理 |
| 轉發 | Local、Remote、Dynamic SOCKS5，保存規則，重連恢復，死亡回報，閒置逾時 |
| AI | OxideSens 支援 OpenAI、Anthropic、Gemini、Ollama/相容端點、MCP、RAG、命令審批 |
| 雲同步與 `.oxide` | push/pull/apply/resolve，S3/WebDAV/Git，rollback backup；加密匯入匯出連線、轉發、設定、快捷命令和外掛設定 |
| 外掛與 CLI | WASM 沙箱、native host API、外掛設定；CLI 含 settings、connections、forwards、quick-commands、plugins、secrets、cloud-sync、backup、report 等命令 |

## 內部實作

OxideTerm Native 移除了 WebView 橋接，並把終端、SSH、SFTP、轉發、IDE、AI、插件和 CLI 保持在一套 Rust 原生架構中。完整實作細節保留在下方，方便需要工程細節的讀者展開查看。

<details>
<summary><strong>架構、SSH 內部、GPUI 外殼、重連、AI、插件與更多細節</strong></summary>
<br>

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

</details>

---

## 從原始碼執行

公開 native 安裝包尚未發布。在打包構建準備好之前，請從原始碼執行 native 版本。

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

## 發布狀態

- [x] SSH Agent 轉發、Grace Period 重連、GPUI 桌面 shell
- [x] 無 WebSocket 的進程內終端資料流
- [x] SFTP、轉發、IDE、AI、雲同步、外掛、CLI
- [x] 本地序列埠終端
- [ ] 公開打包安裝包
- [ ] 完整 ProxyCommand、審計日誌

## 貢獻

## Provider 中立性

OxideTerm 是 BYOK 優先，並保持 provider 中立。

Provider 集成是為了讓使用者連接他們已經信任的工具。它們不是排行榜，不是廣告牌，也不是獎勵那些最熱情開口者的機制。

是否寫進文件，取決於相容性、可維護性、安全性和真實使用者價值。可見度跟隨有用性，而不是熱情程度。

已有 Tauri 功能遷移到 native 時，應保持行為、標籤、互動狀態和工作流一致。新 crate 必須承擔真實職責，不能只是 re-export 或堆放函式。

## 支援與維護

OxideTerm Native 正在作為下一代 OxideTerm 主版本準備中，並以**盡力而為**的方式維護。帶有可重現步驟和脫敏診斷資訊的 bug 報告會優先處理；功能請求不一定都會實作。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

如果 OxideTerm 幫助了你的工作流，GitHub Star、問題重現、翻譯修正、插件或 PR 都能讓專案更容易繼續推進。

---

## 授權與致謝

**GPL-3.0-only**。第三方聲明記錄在 `NOTICE`。感謝 `russh`、`GPUI`、`alacritty_terminal`、`portable-pty`、`wasmtime` 和 `tree-sitter`。
