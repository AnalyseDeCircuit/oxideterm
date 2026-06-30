<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>面向遠端伺服器、具備 AI 能力的 SSH 用戶端 —— 純 Rust 原生應用</strong>
  <br>
  SSH 與 Telnet 終端、SFTP、連接埠轉發、序列埠主控台和輕量編輯，集中在一個原生工作區。
  <br>
  GPU 直接渲染。免費，無需註冊。
  <br>
  <strong>零 WebView。零 OpenSSL。零遙測。零訂閱。BYOK 優先。純 Rust SSH。</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.11-blue" alt="版本">
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

## OxideTerm Native 是什麼

OxideTerm Native 是一個**純 Rust GPUI 桌面應用**——Termius 與 SecureCRT 的開源替代，用於透過 SSH 原生連接遠端伺服器。

**你可以做什麼：**

- 在一個原生工作區裡管理 SSH 與 Telnet 終端、SFTP、連接埠轉發、序列埠主控台、本地 Shell 和輕量編輯
- 透過寬限期重新連線，讓遠端工作更能扛住網路抖動
- 讓 OxideSens AI 透過你自己的 AI 提供商檢查即時會話，並執行經過批准的工作區操作

它**不是**託管雲端 Agent 平台，也不是 Electron、Tauri 或網頁終端：沒有 Chromium、WebView、JavaScript 或 CSS。

---

## 為什麼選擇 Native？

| 如果你在意... | OxideTerm Native 提供... |
|---|---|
| 一個遠端節點，多種工具 | 終端、SFTP、連接埠轉發、trzsz、原生 IDE、監控和 OxideSens AI 都掛在同一個 SSH 工作區上 |
| 零 WebView 原生外殼 | GPUI 直接在 GPU 表面繪製桌面介面 — 沒有 DOM、CSS、JavaScript、Chromium 或 WebKit 執行時 |
| 本地優先 SSH 工作流 | SSH、Telnet、SFTP、轉發、本地 Shell、序列埠終端和配置管理都無需註冊 |
| BYOK OxideSens AI，而不是平台點數 | OxideSens 使用你自己的 OpenAI/Anthropic/Gemini/Ollama/OpenAI 相容端點，支援 MCP、RAG 和經過批准的工作區操作 |
| 重連穩定性 | 寬限期會先探測舊連線 30 秒再替換它，短暫網路中斷時 TUI 應用仍有機會存活 |
| 純 Rust SSH 與憑證安全 | `russh` + `ring`，無 OpenSSL/libssh2；密碼和 API 金鑰保存在系統鑰匙圈，`.oxide` 使用 ChaCha20-Poly1305 + Argon2id |

---

## 截圖

原生介面遵循目前 Tauri 版本相同的 OxideTerm 工作區模型與視覺語言。

<table>
<tr>
<td align="center"><strong>SSH 終端 + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="帶有 OxideSens AI 的 SSH 終端" /></td>
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
| 渲染 | Chromium/Safari/WebKit2GTK + CSS | GPUI GPU 表面，即時模式，純 Rust |
| 終端資料流 | WebSocket → JS 事件迴圈 → xterm.js | Rust 輸入 → `TerminalState` → GPUI 渲染 |
| IPC 開銷 | 每次命令都要 JSON-RPC | 進程內函式呼叫 |
| SSH keepalive | JavaScript timer | Rust async task |
| 外掛執行環境 | 瀏覽器沙箱中的 ESM | wasmtime WASM + 型別化 Rust 宿主 API |
| CLI | 依賴桌面應用運行 | 獨立二進位，直接連結 crate |
| 執行環境邊界 | 瀏覽器執行環境 + WebView 橋接 | 原生進程；不捆綁瀏覽器執行環境 |

## 功能概覽

| 分類 | 功能 |
|---|---|
| 終端 | 本地 PTY、SSH、Telnet、本地序列埠終端、分屏、shell integration、命令標記、asciicast 錄製/回放、trzsz、Sixel/Kitty graphics、渲染策略 |
| SSH 與認證 | 連線池、無限 ProxyJump、Grace Period 重連、Host-key TOFU、SSH Agent 轉發、密碼/金鑰/憑證/鍵盤互動認證 |
| SFTP / IDE | 雙欄瀏覽器、傳輸佇列、預覽、書籤、原子寫入、遠端檔案樹、多分頁編輯、衝突處理 |
| 轉發 | Local、Remote、Dynamic SOCKS5，保存規則，重連恢復，死亡回報，閒置逾時 |
| AI | OxideSens 支援 OpenAI、Anthropic、Gemini、Ollama/相容端點、MCP、RAG、命令審批 |
| 雲同步與 `.oxide` | push/pull/apply/resolve，S3/WebDAV/Git，回滾備份；加密匯入匯出連線、轉發、設定、快捷命令和外掛設定 |
| 外掛與 CLI | WASM 沙箱、原生宿主 API、外掛設定；CLI 含 settings、connections、forwards、quick-commands、plugins、密鑰、cloud-sync、backup、report 等命令 |

## 內部實作

OxideTerm Native 移除了 WebView 橋接，並把終端、SSH、Telnet、SFTP、轉發、IDE、AI、插件和 CLI 保持在一套 Rust 原生架構中。完整實作細節保留在下方，方便需要工程細節的讀者展開查看。

<details>
<summary><strong>架構、SSH 內部、GPUI 外殼、重連、AI、插件與更多細節</strong></summary>
<br>

### 單進程，零橋接

```text
GPUI 渲染迴圈
  WorkspaceApp / Tab surfaces / GPUI views
        │ in-程序 Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

UI 與 SSH/終端後端之間沒有序列化邊界。終端位元組直接修改 `TerminalState`，GPUI 讀取狀態並發出 GPU draw call。

### 純 Rust SSH — russh (ring)

原生版本把與 Tauri 版本同源的 `russh` stack 直接連結進桌面應用二進位：

- **零 OpenSSL 依賴**：密碼學實作透過 `ring` 完成
- 完整 SSH2：金鑰交換、通道、SFTP 子系統與連接埠轉發
- ChaCha20-Poly1305 / AES-GCM、Ed25519/RSA/ECDSA 金鑰
- SSH Agent：Unix `SSH_AUTH_SOCK` 與 Windows `\\.\pipe\openssh-ssh-agent`
- 多跳 ProxyJump，每一跳獨立認證

### Grace Period 智慧重連

重連語義與 Tauri 版本一致，但 orchestration 全部在 Rust async task 內完成：

1. 透過 SSH keepalive 偵測連線逾時，沒有 JavaScript timer throttle
2. 快照 terminal pane、SFTP transfer、port forward 與 IDE file state
3. **Grace Period**：先探測舊連線 30 秒，網路切換時 TUI 應用有機會原地存活
4. 舊連線無法恢復時，新 SSH 連線會恢復轉發、續傳並重新開啟 IDE 檔案

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH 連線池與節點路由

`SshConnectionRegistry` 使用 `DashMap` 管理連線，沿用 Tauri 的 node-first 架構，但沒有 WebSocket lifecycle bridge：

- 一個實體 SSH 連線可被 terminal、SFTP、port forward 和 IDE 同時使用
- 每條連線有 `connecting → active → idle → link_down → 重連ing` state machine
- UI 只按 `nodeId` 操作，`NodeRouter` 原子解析到底層 `connectionId`
- `NodeRuntimeStore` 將節點拓撲快照持久化到 `session_tree.json`
- Jump host 失敗會級聯標記下游節點為 `link_down`

### OxideSens AI

OxideSens 仍然是 BYOK 優先，native 版本把上下文構建放在程序內完成：

- 提供商：OpenAI、Anthropic、Gemini、Ollama 或任何 OpenAI 相容端點
- MCP：stdio 與 SSE transport，支援 tool discovery 與 invocation
- RAG：BM25 full-text、HNSW vector index、RRF fusion 與 CJK bigram tokenizer
- AI 上下文來自目前工作區狀態；憑證在送往提供商前會被遮蔽
- API 金鑰存入系統鑰匙圈，不寫入 log，也不會進入 IPC frame

### GPUI 桌面外殼

整個 UI 使用 GPUI 直接繪製，沒有 DOM/CSS/JavaScript rendering pipeline：

- 17 類工作區分頁：本地終端、SSH 終端、Telnet 終端、SFTP、Forwards、Settings、Plugin、Topology 等
- Binary pane tree 與可拖曳 divider，每個 terminal tab 最多 4 個 pane
- Command palette、global key bindings 與 sidebar 都使用 GPUI primitive
- Immediate-mode rendering 直接回應 Rust state 變化，無 serialization round-trip

### 終端狀態與渲染

終端渲染先建模為 Rust state，再由 GPUI 繪製：

- PTY 輸出進入 `TerminalState`；scrollback、cursor、selection、marks 與搜尋狀態都留在 Rust 中
- 渲染策略可在 Boost、Normal、Idle 之間切換，不需要等待瀏覽器事件迴圈配合
- Sixel 與 Kitty graphics 作為 terminal-owned assets 追蹤，而不是 DOM node 或 canvas overlay
- 分割窗格共享同一套工作區狀態，分頁恢復與重連可以一起快照終端拓撲

### SFTP 與 IDE 工作區

遠端檔案屬於同一個 node 工作區，而不是割裂的附屬功能：

- SFTP session 透過 `NodeRouter` 解析，重連替換底層 SSH connection 時 UI 的 node address 不變
- Transfer queues 獨立追蹤 direction、progress、retry state 與 speed limits，不依賴目前可見 file panes
- IDE tabs 同時保存 dirty buffers、remote paths、conflict state 與 restore metadata
- Backend 支援時，remote writes 使用 staged/atomic behavior，避免普通 editing flow 出現 partial writes

### 外掛、CLI 與診斷

Native 分支把 extension 與 support surfaces 保持在 Rust-native boundaries：

- 外掛在 wasmtime 沙箱中執行，使用型別化宿主能力，而不是瀏覽器全域物件
- CLI 直接連結領域 crate，涵蓋 doctor、settings、connections、forwards、便攜包、備份與報告
- 診斷優先輸出計數、路徑、功能旗標與脫敏提示，避免暴露含密鑰的原始負載
- 會修改狀態的 CLI flows 使用 dry-run 計畫、`--yes` guards 與回滾備份

### Port Forwarding — Lock-Free I/O

Port 轉發語義與 Tauri 相同，但獨立為 Rust crate：

- Local `-L`、Remote `-R`、Dynamic SOCKS5 `-D`
- SSH Channel 由單一 `ssh_io` task 持有，避免 `Arc<Mutex<Channel>>`
- 支援重連自動恢復、終止回報與閒置逾時

### trzsz 內聯檔案傳輸

trzsz 繼續走 terminal stream，不需要額外 port 或 remote agent：

- Upload/download 復用既有 terminal stream
- 可穿透 ProxyJump chain
- Native file picker 不受 browser memory 限制
- 支援 bidirectional、directory transfer 與 configurable limits

### `.oxide` 加密匯出

加密格式與 Tauri 版本保持一致：

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**：256 MB memory cost、4 iterations，提高 GPU brute-force 成本
- 覆蓋 connections、forwards、settings、快捷命令、外掛設定與便攜密鑰

</details>

---

## 從原始碼執行

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
| 密碼與金鑰 | mac系統鑰匙圈 / Windows Credential Manager / libsecret |
| 記憶體中的秘密 | `zeroize` / `Zeroizing` |
| 診斷與 AI 上下文 | 只輸出路徑、計數、旗標和 hint；送給 AI 前脫敏 |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI 寫入 | dry-run 計畫、`--yes` 保護和回滾備份 |
| 外掛 | wasmtime 隔離與基於能力的宿主 API |

## 發布狀態

- [x] SSH Agent 轉發、Grace Period 重連、GPUI 桌面 shell
- [x] 無 WebSocket 的進程內終端資料流
- [x] SFTP、轉發、IDE、AI、雲同步、外掛、CLI
- [x] 本地序列埠與 Telnet 終端
- [x] 完整 ProxyCommand
- [ ] 審計日誌

## 貢獻

## 提供商中立性

OxideTerm 是 BYOK 優先，並保持提供商中立。

提供商整合是為了讓使用者連接他們已經信任的工具。它們不是排行榜，不是廣告牌，也不是獎勵那些最熱情開口者的機制。

是否寫進文件，取決於相容性、可維護性、安全性和真實使用者價值。可見度跟隨有用性，而不是熱情程度。

已有 Tauri 功能遷移到 native 時，應保持行為、標籤、互動狀態和工作流一致。新 crate 必須承擔真實職責，不能只是 re-export 或堆放函式。

## 支援與維護

帶有可重現步驟和脫敏診斷資訊的 bug 回報與回歸問題會優先處理。功能請求會根據範圍、安全性以及是否符合 OxideTerm 的遠端伺服器工作區方向來評估。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

如果 OxideTerm 幫助了你的工作流，GitHub Star、問題重現、翻譯修正、插件或 PR 都能讓專案更容易繼續推進。

---

## 授權與致謝

**GPL-3.0-only**。第三方聲明記錄在 `NOTICE`。感謝 `russh`、`GPUI`、`alacritty_terminal`、`portable-pty`、`wasmtime` 和 `tree-sitter`。
