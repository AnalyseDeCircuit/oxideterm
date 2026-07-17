<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>面向遠端伺服器、具備 AI 能力的原生維運工作區 —— 純 Rust 原生應用</strong>
  <br>
  SSH、Telnet、序列埠、RDP/VNC、SFTP、連接埠轉發 和輕量編輯，集中在一個原生工作區。
  <br>
  GPU 直接渲染。免費，無需註冊。
  <br>
  <strong>不使用 Electron。 不捆綁 WebView。不蒐集遙測。無需訂閱。BYOK 優先。純 Rust SSH，不依賴 OpenSSL/libssh2。</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.3-blue" alt="版本">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="平台">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="授權">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>開源、本機優先，使用 GPUI 進行 GPU 繪製。</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm 功能概覽" width="920">
</p>

---

## OxideTerm 是什麼

OxideTerm 是面向 SSH 與遠端維運的開源工作區。終端、檔案、連接埠轉發、主機工具和遠端桌面都集中在同一個工作區中。

**你可以做什麼：**

- 在同一個工作區中管理 SSH、Telnet、序列埠、RDP/VNC、SFTP、連接埠轉發、本機 Shell 與輕量編輯
- 透過 Grace Period 重連機制，應對短暫網路中斷並維持遠端工作
- 使用你自己的 AI 服務商，讓 OxideSens 檢查作用中的工作階段並執行經過核准的工作區操作

連線資訊與維運資料始終由你掌控。OxideSens 使用你自己的 AI 服務商，無需註冊帳號。

---

## 為什麼選擇 OxideTerm？

- 在一個桌面應用程式中管理 SSH、Telnet、序列埠、RDP/VNC、SFTP、連接埠轉發與本機 Shell
- Grace Period 重連機制可應對短暫網路中斷
- OxideSens 使用你自己的 AI 憑證，並由你核准工作區操作
- GPUI 原生介面，不使用 Electron，也不捆綁瀏覽器執行環境

---

## 截圖

以下截圖展示了 OxideTerm 的終端、檔案、編輯與連接埠轉發工作流程。

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

## 為遠端維運而設計

OxideTerm 將連線、檔案、轉發、主機工具、自動化與 AI 上下文放在同一個 Rust 工作區中。各項工具共享同一台伺服器的身分與工作階段生命週期。

---

## 功能

| 類別 | 功能 |
|---|---|
| **終端與連線** | 本機 Shell、SSH、Telnet、序列埠、分割窗格、多跳路由與穩定重連 |
| **檔案與遠端編輯** | SFTP、傳輸佇列、收藏夾、安全寫入、專案樹與多分頁編輯 |
| **轉發與網路** | 本機、遠端與動態 SOCKS5 轉發、已儲存規則與 Socket 除錯 |
| **主機維運與遠端桌面** | 監控、行程、服務、日誌、連接埠、工作、磁碟、套件、容器、tmux、RDP 與 VNC |
| **OxideSens 與自動化** | 自有 AI 服務商、MCP、本機 RAG、已核准操作、加密雲端同步與 CLI |
| **擴充與個人化** | WASM 插件、自訂分頁、快速命令、主題、背景圖片、快捷鍵與 11 種介面語言 |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens 在 OxideTerm 中開啟終端" width="920">
</a>

*觀看 OxideSens 依照使用者請求，在 OxideTerm 中開啟一個終端。*

</div>

---

## 內部實作

OxideTerm 將終端、SSH、Telnet、RDP、VNC、SFTP、轉發、IDE、AI、插件和 CLI 整合在同一套 Rust 架構中。下方列出了面向開發者的技術細節。

<details>
<summary><strong>架構、SSH 內部、GPUI 外殼、重連、AI、插件與更多細節</strong></summary>
<br>

### 核心行程內直連，無 WebView 橋接

```text
GPUI 渲染迴圈
  WorkspaceApp / 分頁介面 / GPUI 檢視
        │ in-程序 Arc<> / async
領域 Crate
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

介面與 SSH/終端後端之間沒有序列化邊界。終端位元組直接修改 `TerminalState`，GPUI 讀取狀態並發出 GPU 繪製命令。

### 純 Rust SSH — russh (ring)


- **SSH 堆疊不依賴 OpenSSL/libssh2**：SSH 密碼學能力由 `ring` 提供
- 完整 SSH2：金鑰交換、通道、SFTP 子系統與連接埠轉發
- ChaCha20-Poly1305 / AES-GCM、Ed25519/RSA/ECDSA 金鑰
- SSH Agent：Unix `SSH_AUTH_SOCK` 與 Windows `\\.\pipe\openssh-ssh-agent`
- 多跳 ProxyJump，每一跳獨立認證

### Grace Period 智慧重連


1. 透過 SSH keepalive 偵測連線逾時，沒有 JavaScript timer throttle
2. 快照終端分割窗格、SFTP 傳輸、連接埠轉發與 IDE 檔案狀態
3. **Grace Period**：先探測舊連線 30 秒，網路切換時 TUI 應用有機會原地存活
4. 舊連線無法恢復時，新 SSH 連線會恢復轉發、續傳並重新開啟 IDE 檔案

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH 連線池與節點路由


- 一個實體 SSH 連線可被 terminal、SFTP、port forward 和 IDE 同時使用
- 每條連線都有 `connecting → active → idle → link_down → reconnecting` 狀態機
- UI 只按 `nodeId` 操作，`NodeRouter` 原子解析到底層 `connectionId`
- `NodeRuntimeStore` 將節點拓撲快照持久化到 `session_tree.json`
- 跳板機失效會級聯標記下游節點為 `link_down`

### OxideSens AI

OxideSens 採用 BYOK 模式，並在行程內建立上下文：

- 提供商：OpenAI、Anthropic、Gemini、Ollama 或任何 OpenAI 相容端點
- MCP：stdio 與 SSE 傳輸，支援工具探索與呼叫
- RAG：BM25 全文檢索、HNSW 向量索引、RRF 融合與 CJK 雙字詞斷詞器
- 傳送至提供商的訊息會過濾憑證模式；工作區上下文與操作仍由使用者控制
- API 金鑰存入系統鑰匙圈，並明確排除在結構化日誌和桌面核心訊息內容之外

### GPUI 桌面外殼

整個 UI 使用 GPUI 直接繪製，沒有 DOM/CSS/JavaScript rendering pipeline：

- 工作區分頁類型：本地終端、SSH、Telnet、序列埠、RDP、VNC、SFTP、IDE、Forwards、Settings、Plugin、Topology 等
- Binary pane tree 與可拖曳 divider，每個 terminal tab 最多 4 個 pane
- Command palette、global key bindings 與 sidebar 都使用 GPUI primitive
- 即時模式渲染直接回應 Rust 狀態變化，無需序列化往返

### 終端狀態與渲染

終端渲染先建模為 Rust 狀態，再由 GPUI 繪製：

- PTY 輸出進入 `TerminalState`；scrollback、cursor、selection、marks 與搜尋狀態都留在 Rust 中
- 渲染策略可在 Boost、Normal、Idle 之間切換，不需要等待瀏覽器事件迴圈配合
- Sixel 與 Kitty graphics 作為 terminal-owned assets 追蹤，而不是 DOM node 或 canvas overlay
- 分割窗格共享同一套工作區狀態，分頁恢復與重連可以一起快照終端拓撲

### SFTP 與 IDE 工作區

遠端檔案屬於同一個 node 工作區，而不是割裂的附屬功能：

- SFTP session 透過 `NodeRouter` 解析，重連替換底層 SSH connection 時 UI 的 node address 不變
- 傳輸佇列獨立追蹤方向、進度、重試狀態與速度限制，不依賴目前可見的檔案窗格
- IDE 分頁同時保存未儲存緩衝區、遠端路徑、衝突狀態與復原中繼資料
- Backend 支援時，remote writes 使用 staged/atomic behavior，避免普通 editing flow 出現 partial writes

### 外掛、CLI 與診斷

原生分支把擴充功能與支援介面保持在 Rust 原生邊界內：

- 外掛在 wasmtime 沙箱中執行，使用型別化宿主能力，而不是瀏覽器全域物件
- CLI 直接連結領域 crate，涵蓋 doctor、settings、connections、forwards、便攜包、備份與報告
- 診斷優先輸出計數、路徑、功能旗標與脫敏提示，避免暴露含密鑰的原始負載
- 會修改狀態的 CLI flows 使用 dry-run 計畫、`--yes` guards 與回滾備份

### 連接埠轉發 — 無鎖 I/O


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


- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**：256 MB memory cost、4 iterations，提高 GPU brute-force 成本
- 覆蓋 connections、forwards、settings、快捷命令、外掛設定與便攜密鑰

</details>

---

## 從原始碼執行

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

## 技術棧

| 層級 | 技術 | 說明 |
|---|---|---|
| 介面框架 | GPUI（Zed） | GPU 加速的即時模式介面，純 Rust 實作 |
| 執行環境 | Tokio + DashMap | 非同步執行環境與並行映射 |
| SSH | russh（`ring`） | SSH 堆疊不依賴 OpenSSL/libssh2，支援 SSH Agent |
| 終端 | portable-pty + alacritty_terminal | 本機偽終端、終端模擬與 Sixel/Kitty 圖形 |
| 外掛 | wasmtime | WASM 隔離與原生主機 API |
| AI 與檢索 | SSE + BM25 + HNSW | 提供商串流、CJK 雙字詞與 RRF 融合 |

## 安全

| 關注點 | 實作 |
|---|---|
| 已儲存憑證 | macOS 鑰匙圈 / Windows Credential Manager / libsecret |
| 記憶體中的秘密 | 持有秘密的型別和暫存緩衝區在支援的所有權邊界使用 `zeroize` / `Zeroizing` |
| 診斷 | 支援報告優先輸出結構化中繼資料和脫敏提示，避免攜帶秘密的原始內容 |
| AI 上下文 | 傳送至提供商的訊息會過濾憑證模式；工作區上下文與操作仍由使用者控制 |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI 寫入 | dry-run 計畫、`--yes` 保護和回滾備份 |
| 外掛 | wasmtime 隔離與基於能力的宿主 API |

## 合法使用提醒

OxideTerm 依 GPL-3.0-only 授權發布，不附加額外的授權限制。使用時，請僅存取你擁有或已取得明確授權的系統、網路和裝置，並遵守適用法律。請勿使用 OxideTerm 從事未經授權的存取、服務干擾或規避存取控制。

## 貢獻

歡迎貢獻程式碼、文件、翻譯、插件、測試與問題重現。較大的改動請先透過 Issue 討論，範圍明確的修正可以直接提交 Pull Request。

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## 支援與維護

帶有可重現步驟和脫敏診斷資訊的 bug 回報與回歸問題會優先處理。功能請求會根據範圍、安全性以及是否符合 OxideTerm 的遠端伺服器工作區方向來評估。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

如果 OxideTerm 幫助了你的工作流，GitHub Star、問題重現、翻譯修正、插件或 PR 都能讓專案更容易繼續推進。

---

## 授權

**GPL-3.0-only**。詳細的第三方聲明請見 [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md)，其他通知請見 [`NOTICE`](../../NOTICE)。

## 致謝

感謝 `russh`、`GPUI`、`alacritty_terminal`、`portable-pty`、`wasmtime` 和 `tree-sitter`。
