<p align="center">
  <img src="../../src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>如果您喜歡 OxideTerm，請在 GitHub 上點個 Star ⭐️！</em>
</p>


<p align="center">
  <strong>零 Electron。零 OpenSSL。零遙測。零訂閱。純 Rust SSH。</strong>
  <br>
  <em>一個原生二進位檔——本機 Shell、SSH、SFTP、遠端 IDE、AI、連接埠轉發、外掛、30+ 主題、11 種語言。</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.1.10-blue" alt="版本">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="平台">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="授權條款">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
  <img src="https://img.shields.io/github/downloads/AnalyseDeCircuit/oxideterm/total?color=brightgreen" alt="總下載量">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=%E4%B8%8B%E8%BC%89%E6%9C%80%E6%96%B0%E7%89%88&style=for-the-badge&color=brightgreen" alt="下載最新版">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=%E4%B8%8B%E8%BC%89%E6%9C%80%E6%96%B0Beta%E7%89%88&style=for-the-badge&color=orange" alt="下載最新Beta版">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

> [!NOTE]
> **授權變更：** 從 v1.0.0 起，OxideTerm 已將授權從 **PolyForm Noncommercial 1.0.0** 變更為 **GPL-3.0（GNU 通用公共授權條款 v3.0）**。這意味著 OxideTerm 現在是完全開源的——您可以在 GPL-3.0 授權條款下自由使用、修改和散布。詳見 [LICENSE](../../LICENSE) 檔案。

---

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI —「開啟一個本機終端並執行 echo hello, world!」*

</div>

---

## 為什麼選擇 OxideTerm？

| 痛點 | OxideTerm 的解決方案 |
|---|---|
| SSH 用戶端無法使用本機 Shell | **混合引擎**：本機 PTY（zsh/bash/fish/pwsh/WSL2）+ 遠端 SSH 同窗共存 |
| 斷線重連 = 遺失一切 | **寬限期重連**：斷開前探測舊連線 30 秒——您的 vim/htop/yazi 安然無恙 |
| 遠端編輯需要 VS Code Remote | **內建 IDE**：CodeMirror 6 基於 SFTP，支援 30+ 語言，可選 ~1 MB Linux 遠端 Agent |
| SSH 連線無法複用 | **多工複用**：終端、SFTP、轉發、IDE 透過參考計數連線池共用同一 SSH 連線 |
| SSH 函式庫依賴 OpenSSL | **russh 0.59**：基於 `ring` 編譯的純 Rust SSH——零 C 依賴 |
| 100+ MB 的 Electron 應用 | **Tauri 2.0**：原生 Rust 後端，25–40 MB 二進位檔 |
| AI 被鎖定在單一供應商 | **OxideSens**：40+ 工具、MCP 協定、RAG 知識庫——支援 OpenAI/Ollama/DeepSeek 及任何相容 API |
| 憑證存放在明文設定檔中 | **僅系統鑰匙圈**：密碼和 API 金鑰絕不落地；`.oxide` 檔案使用 ChaCha20-Poly1305 + Argon2id 加密 |
| 依賴雲端、需要註冊帳號 | **本地優先**：零帳號、零遙測、零雲端同步——資料留在你的裝置上。AI 金鑰自行提供 |

---

## 螢幕截圖

<table>
<tr>
<td align="center"><strong>SSH 終端 + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="帶有 OxideSens AI 側邊欄的 SSH 終端" /></td>
<td align="center"><strong>SFTP 檔案管理員</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="SFTP 雙窗格檔案管理員與傳輸佇列" /></td>
</tr>
<tr>
<td align="center"><strong>內建 IDE（CodeMirror 6）</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="基於 CodeMirror 6 編輯器的內建 IDE 模式" /></td>
<td align="center"><strong>智慧連接埠轉發</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="帶自動偵測的智慧連接埠轉發" /></td>
</tr>
</table>

---

## 功能概覽

| 分類 | 功能 |
|---|---|
| **終端** | 本機 PTY（zsh/bash/fish/pwsh/WSL2）、SSH 遠端、分割窗格、廣播輸入、工作階段錄製/回放（asciicast v2）、WebGL 算繪、30+ 主題 + 自訂編輯器、命令面板（`⌘K`）、禪模式 |
| **SSH 與驗證** | 連線池與多工複用、ProxyJump（無限跳數）拓撲圖、寬限期自動重連、Agent 轉發。驗證方式：密碼、SSH 金鑰（RSA/Ed25519/ECDSA）、SSH Agent、憑證、keyboard-interactive 2FA、Known Hosts TOFU |
| **SFTP** | 雙窗格瀏覽器、拖放操作、智慧預覽（圖片/影片/音訊/程式碼/PDF/十六進位/字型）、帶進度和預估時間的傳輸佇列、書籤、壓縮檔解壓 |
| **IDE 模式** | CodeMirror 6 支援 30+ 語言、檔案樹 + Git 狀態、多分頁、衝突解決、整合終端。可選 Linux 遠端 Agent（9 種額外架構） |
| **連接埠轉發** | 本機（-L）、遠端（-R）、動態 SOCKS5（-D）、無鎖訊息傳遞 I/O、重連自動恢復、終止報告、閒置逾時 |
| **AI（OxideSens）** | 內嵌面板（`⌘I`）+ 側邊欄聊天、終端緩衝區擷取（單窗格/所有窗格）、多來源上下文（IDE/SFTP/Git）、40+ 自主工具、MCP 伺服器整合、RAG 知識庫（BM25 + 向量混合搜尋）、SSE 串流輸出 |
| **外掛** | 執行階段 ESM 載入、18 個 API 命名空間、24 個 UI Kit 元件、凍結 API + Proxy ACL、斷路器、錯誤時自動停用 |
| **CLI** | `oxt` 伴隨工具：JSON-RPC 2.0 基於 Unix Socket / Named Pipe、`status`/`list`/`ping`、人類可讀 + JSON 輸出 |
| **安全** | .oxide 加密匯出（ChaCha20-Poly1305 + Argon2id 256 MB）、OS 鑰匙圈、Touch ID（macOS）、主機金鑰 TOFU、`zeroize` 記憶體清除 |
| **國際化** | 11 種語言：EN、简体中文、繁體中文、日本語、한국어、FR、DE、ES、IT、PT-BR、VI |

---

## 技術內幕

### 架構——雙平面通訊

OxideTerm 將終端資料與控制命令分離為兩個獨立平面：

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│  xterm.js 6 (WebGL) + 19 stores    │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binary)
           │ (JSON)       │ per-session port
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│  NodeRouter → SshConnectionRegistry │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]       │
└─────────────────────────────────────┘
```

- **資料平面（WebSocket）**：每個 SSH 工作階段取得獨立的 WebSocket 連接埠。終端位元組以帶有 Type-Length-Payload 標頭的二進位幀傳輸——無 JSON 序列化、無 Base64 編碼，熱路徑零開銷。
- **控制平面（Tauri IPC）**：連線管理、SFTP 操作、轉發、組態——結構化 JSON，但不在關鍵路徑上。
- **Node 優先定址**：前端從不直接觸及 `sessionId` 或 `connectionId`。一切透過 `nodeId` 定址，由 `NodeRouter` 在伺服端原子解析。SSH 重連會更換底層 `connectionId`——但 SFTP、IDE 和轉發完全不受影響。

### 🔩 純 Rust SSH — russh 0.59

整個 SSH 協定棧使用 **russh 0.59**，基於 **`ring`** 加密後端編譯：

- **零 C/OpenSSL 依賴**——完整的加密棧由 Rust 實作，告別「哪個 OpenSSL 版本？」的除錯噩夢。
- 完整的 SSH2 協定：金鑰交換、通道、SFTP 子系統、連接埠轉發
- ChaCha20-Poly1305 和 AES-GCM 加密套件，Ed25519/RSA/ECDSA 金鑰
- 自訂 **`AgentSigner`**：封裝系統 SSH Agent 並實作 russh 的 `Signer` trait，透過在 `.await` 前將 `&AgentIdentity` 複製為 owned 值，解決 RPITIT `Send` 約束問題

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **平台支援**：Unix（`SSH_AUTH_SOCK`）、Windows（`\\.\pipe\openssh-ssh-agent`）
- **代理鏈**：每一跳獨立使用 Agent 驗證
- **重連**：`AuthMethod::Agent` 自動重放

### 🔄 智慧重連與寬限期

大多數 SSH 用戶端在斷線時會銷毀一切然後從頭開始。OxideTerm 的重連編排器採用了截然不同的策略：

1. **偵測**：WebSocket 心跳逾時（300 秒，針對 macOS App Nap 和 JS 計時器節流最佳化）
2. **快照**：完整狀態——終端窗格、進行中的 SFTP 傳輸、活動連接埠轉發、開啟的 IDE 檔案
3. **智慧探測**：`visibilitychange` + `online` 事件觸發主動 SSH keepalive（~2 秒偵測 vs 被動逾時的 15-30 秒）
4. **寬限期**（30 秒）：透過 keepalive 探測舊 SSH 連線——如果恢復成功（例如 WiFi AP 切換），您的 TUI 應用（vim、htop、yazi）完全不受影響
5. 恢復失敗 → 建立新 SSH 連線 → 自動恢復轉發 → 恢復 SFTP 傳輸 → 重新開啟 IDE 檔案

管線流程：`queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

所有邏輯執行於專用的 `ReconnectOrchestratorStore` 中——零重連程式碼散落在 hooks 或元件中。

### 🛡️ SSH 連線池

參考計數的 `SshConnectionRegistry`，以 `DashMap` 為底層實作無鎖並行存取：

- **一個連線，多個消費者**：終端、SFTP、連接埠轉發和 IDE 共用同一實體 SSH 連線——無冗餘 TCP 交握
- **每連線狀態機**：`connecting → active → idle → link_down → reconnecting`
- **生命週期管理**：可設定的閒置逾時（5 分鐘 / 15 分鐘 / 30 分鐘 / 1 小時 / 永不）、15 秒 keepalive 間隔、心跳故障偵測
- **WsBridge 心跳**：30 秒間隔、5 分鐘逾時——相容 macOS App Nap 和瀏覽器 JS 節流
- **級聯傳播**：跳板機故障 → 所有下游節點自動標記為 `link_down` 並同步狀態
- **閒置斷開**：向前端發送 `connection_status_changed`（而非僅內部 `node:state`），防止 UI 狀態不同步

### 🤖 OxideSens AI

隱私優先的 AI 助理，提供雙重互動模式：

- **內嵌面板**（`⌘I`）：快速終端命令，透過 bracketed paste 注入輸出
- **側邊欄聊天**：持久對話，完整歷史紀錄
- **上下文擷取**：Terminal Registry 從活動窗格或所有分割窗格同時擷取緩衝區；自動注入 IDE 檔案、SFTP 路徑和 Git 狀態
- **40+ 自主工具**：檔案操作、程序管理、網路診斷、TUI 應用互動、文字處理——AI 無需手動觸發即可呼叫
- **MCP 支援**：連接外部 [Model Context Protocol](https://modelcontextprotocol.io) 伺服器（stdio & SSE）進行第三方工具整合
- **RAG 知識庫**（v0.20）：將 Markdown/TXT 文件匯入作用域集合（全域或按連線）。混合搜尋透過 Reciprocal Rank Fusion 融合 BM25 關鍵字索引 + 向量餘弦相似度。Markdown 感知分塊保留標題層級。CJK 雙字元分詞器支援中文/日文/韓文。
- **供應商**：OpenAI、Ollama、DeepSeek、OneAPI 或任何 `/v1/chat/completions` 端點
- **安全**：API 金鑰儲存於 OS 鑰匙圈；macOS 上金鑰讀取受 **Touch ID** 透過 `LAContext` 保護——無需授權簽章或程式碼簽署，每次工作階段首次驗證後快取

### 💻 IDE 模式——遠端編輯

CodeMirror 6 編輯器基於 SFTP 運作——預設無需伺服端安裝：

- **檔案樹**：延遲載入目錄，帶 Git 狀態指示器（已修改/未追蹤/已新增）
- **24 語言模式**：14 種原生 CodeMirror + 透過 `@codemirror/legacy-modes` 提供的傳統模式
- **衝突解決**：樂觀 mtime 鎖定——覆寫前偵測遠端變更
- **事件驅動 Git**：儲存、建立、刪除、重新命名及終端 Enter 按鍵時自動重新整理
- **狀態閘控**：當 `readiness !== 'ready'` 時阻止所有 IO，Key-Driven Reset 在重連時強制完整重新掛載
- **遠端 Agent**（可選）：~1 MB Rust 二進位檔，在 x86_64/aarch64 Linux 上自動部署。9 種額外架構（ARMv7、RISC-V64、LoongArch64、s390x、Power64LE、i686、ARM、Android aarch64、FreeBSD x86_64）位於 `agents/extra/`，可手動上傳。提供增強檔案樹、符號搜尋和檔案監視功能。

### 🔀 連接埠轉發——無鎖 I/O

完整的本機（-L）、遠端（-R）和動態 SOCKS5（-D）轉發：

- **訊息傳遞架構**：SSH Channel 由單一 `ssh_io` 任務擁有——無 `Arc<Mutex<Channel>>`，徹底消除互斥鎖競爭
- **終止報告**：轉發任務主動報告結束原因（SSH 斷開、遠端連接埠關閉、逾時），提供清晰的診斷資訊
- **自動恢復**：`Suspended` 狀態的轉發在重連時自動恢復，無需使用者介入
- **閒置逾時**：`FORWARD_IDLE_TIMEOUT`（300 秒）防止殭屍連線堆積

### 🔌 執行階段外掛系統

動態 ESM 載入，安全強化的凍結 API 表面：

- **PluginContext API**：18 個命名空間——terminal、ui、commands、settings、lifecycle、events、storage、system
- **24 個 UI Kit 元件**：預建的 React 元件（按鈕、輸入框、對話方塊、表格……）透過 `window.__OXIDE__` 注入外掛沙箱
- **安全膜**：對所有上下文物件使用 `Object.freeze`，基於 Proxy 的 ACL，IPC 白名單，斷路器在重複錯誤後自動停用
- **共用模組**：React、ReactDOM、zustand、lucide-react 對外暴露供外掛使用，無需重複打包

### ⚡ 自適應算繪

三級算繪排程器，取代固定的 `requestAnimationFrame` 批次處理：

| 級別 | 觸發條件 | 幀率 | 效益 |
|---|---|---|---|
| **加速** | 幀資料 ≥ 4 KB | 120 Hz+（ProMotion 原生） | 消除 `cat largefile.log` 時的捲動卡頓 |
| **正常** | 一般打字 | 60 Hz（RAF） | 平穩的基準表現 |
| **閒置** | 3 秒無 I/O / 分頁隱藏 | 1–15 Hz（指數退避） | 接近零 GPU 負載，節省電量 |

級別切換完全自動——由資料量、使用者輸入和 Page Visibility API 驅動。背景分頁透過閒置計時器持續刷新資料，無需喚醒 RAF。

### 🔐 .oxide 加密匯出

可攜式、防竄改的連線備份：

- **ChaCha20-Poly1305 AEAD** 認證加密
- **Argon2id KDF**：256 MB 記憶體成本、4 次迭代——抵禦 GPU 暴力破解
- **SHA-256** 完整性校驗
- **可選金鑰嵌入**：私鑰 base64 編碼嵌入加密酬載
- **匯出前分析**：驗證類型分類、遺失金鑰偵測

### 📡 ProxyJump——拓撲感知多跳

- 無限鏈深度：`Client → Jump A → Jump B → … → Target`
- 自動解析 `~/.ssh/config`，建構拓撲圖，Dijkstra 最短路徑尋路
- 跳板節點可作為獨立工作階段複用
- 級聯故障傳播：跳板機當機 → 所有下游節點自動標記為 `link_down`

### ⚙️ 本機終端——執行緒安全 PTY

跨平台本機 Shell，基於 `portable-pty 0.8`，透過 `local-terminal` feature flag 控制：

- `MasterPty` 封裝在 `std::sync::Mutex` 中——專用 I/O 執行緒將阻塞式 PTY 讀取隔離在 Tokio 事件迴圈之外
- Shell 自動偵測：`zsh`、`bash`、`fish`、`pwsh`、Git Bash、WSL2
- `cargo build --no-default-features` 可剝離 PTY 功能用於行動裝置/輕量建置

### 🪟 Windows 最佳化

- **原生 ConPTY**：直接呼叫 Windows Pseudo Console API——完整 TrueColor 和 ANSI 支援，無傳統 WinPTY
- **Shell 掃描器**：透過登錄檔和 PATH 自動偵測 PowerShell 7、Git Bash、WSL2、CMD

### 更多功能

- **資源分析器**：透過持久 SSH 通道讀取 `/proc/stat` 取得即時 CPU/記憶體/網路資料，基於增量計算，非 Linux 環境自動降級為僅 RTT
- **自訂主題引擎**：30+ 內建主題，視覺化編輯器即時預覽，20 個 xterm.js 欄位 + 24 個 UI 顏色變數，從終端調色盤自動推導 UI 顏色
- **工作階段錄製**：asciicast v2 格式，完整錄製和回放
- **廣播輸入**：輸入一次，傳送至所有分割窗格——批次伺服器操作
- **背景相簿**：每分頁背景圖片，16 種分頁類型，透明度/模糊/適配控制
- **CLI 伴隨工具**（`oxt`）：~1 MB 二進位檔，JSON-RPC 2.0 基於 Unix Socket / Named Pipe，`status`/`list`/`ping` 支援人類可讀或 `--json` 輸出
- **WSL Graphics** ⚠️ 實驗性：內建 VNC 檢視器——9 種桌面環境 + 單應用模式，WSLg 偵測，Xtigervnc + noVNC

<details>
<summary>📸 11 種語言實際展示</summary>
<br>
<table>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/en.png" width="280"><br><b>English</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/zhHans.png" width="280"><br><b>简体中文</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/zhHant.png" width="280"><br><b>繁體中文</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/ja.png" width="280"><br><b>日本語</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/ko.png" width="280"><br><b>한국어</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/fr.png" width="280"><br><b>Français</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/de.png" width="280"><br><b>Deutsch</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/es.png" width="280"><br><b>Español</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/it.png" width="280"><br><b>Italiano</b></td>
  </tr>
  <tr>
    <td align="center"><img src="../../docs/screenshots/overview/pt-BR.png" width="280"><br><b>Português</b></td>
    <td align="center"><img src="../../docs/screenshots/overview/vi.png" width="280"><br><b>Tiếng Việt</b></td>
    <td></td>
  </tr>
</table>
</details>

---

## 安裝

從 [GitHub Releases](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) 下載最新版本。

| 平台 | 執行階段相依性 |
|---|---|
| **Windows** | [WebView2 執行階段](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) — 已預先安裝於 Windows 10（1803+）和 Windows 11。對於**離線 / 內網**環境，請使用 [Evergreen 獨立安裝程式](https://developer.microsoft.com/en-us/microsoft-edge/webview2/#download)（離線安裝，約 170 MB），或透過群組原則部署**固定版本**執行階段。 |
| **macOS** | 無（使用原生 WebKit） |
| **Linux** | `libwebkit2gtk-4.1`（現代桌面環境通常已預先安裝） |

---

## 快速開始

### 先決條件

- **Rust** 1.85+
- **Node.js** 18+（推薦 pnpm）
- **平台工具**：
  - macOS：Xcode Command Line Tools
  - Windows：Visual Studio C++ Build Tools
  - Linux：`build-essential`、`libwebkit2gtk-4.1-dev`、`libssl-dev`

### 開發

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# 建構 CLI 伴侶工具（CLI 功能必需）
pnpm cli:build

# 完整應用（前端 + Rust 後端，熱重載）
pnpm run tauri dev

# 僅前端（Vite 執行於連接埠 1420）
pnpm dev

# 正式建置
pnpm run tauri build
```

---

## 技術棧

| 層級 | 技術 | 詳情 |
|---|---|---|
| **框架** | Tauri 2.0 | 原生二進位檔，25–40 MB |
| **執行環境** | Tokio + DashMap 6 | 全非同步，無鎖並行對映 |
| **SSH** | russh 0.59（`ring`） | 純 Rust，零 C 依賴，SSH Agent |
| **本機 PTY** | portable-pty 0.8 | Feature 閘控，Windows 上使用 ConPTY |
| **前端** | React 19.1 + TypeScript 5.8 | Vite 7，Tailwind CSS 4 |
| **狀態** | Zustand 5 | 19 個專用 Store |
| **終端** | xterm.js 6 + WebGL | GPU 加速，60fps+ |
| **編輯器** | CodeMirror 6 | 30+ 語言模式 |
| **加密** | ChaCha20-Poly1305 + Argon2id | AEAD + 記憶體硬化 KDF（256 MB） |
| **儲存** | redb 2.1 | 嵌入式 KV 儲存 |
| **國際化** | i18next 25 | 11 種語言 × 22 個命名空間 |
| **外掛** | ESM 執行階段 | 凍結 PluginContext + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## 安全

| 關注點 | 實作方式 |
|---|---|
| **密碼** | OS 鑰匙圈（macOS Keychain / Windows Credential Manager / libsecret） |
| **AI API 金鑰** | OS 鑰匙圈 + macOS 上的 Touch ID 生物辨識保護 |
| **匯出** | .oxide：ChaCha20-Poly1305 + Argon2id（256 MB 記憶體，4 次迭代） |
| **記憶體** | Rust 記憶體安全 + `zeroize` 敏感資料清除 |
| **主機金鑰** | TOFU 驗證 `~/.ssh/known_hosts`，拒絕變更（防中間人攻擊） |
| **外掛** | Object.freeze + Proxy ACL，斷路器，IPC 白名單 |
| **WebSocket** | 一次性權杖，帶時間限制 |

---

## 路線圖

- [x] SSH Agent 轉發
- [ ] 外掛市集
- [ ] 工作階段搜尋與快速切換

---

## 授權條款

**GPL-3.0** — 本軟體是按照 [GNU 通用公共授權條款 v3.0](https://www.gnu.org/licenses/gpl-3.0.html) 發布的自由軟體。

您可以在 GPL-3.0 條款下自由地使用、修改和散布本軟體。任何衍生作品也必須在同一授權條款下發布。

完整文本：[GNU 通用公共授權條款 v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## 致謝

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>236,000+ 行 Rust 與 TypeScript 程式碼——以 ⚡ 和 ☕ 建構</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
