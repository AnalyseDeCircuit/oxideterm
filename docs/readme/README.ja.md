<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>リモートサーバー向け AI 搭載 SSH クライアント — 純粋 Rust ネイティブアプリ</strong>
  <br>
  SSH と Telnet のターミナル、SFTP、ポート転送、シリアルコンソール、軽量編集を 1 つのネイティブワークスペースに。
  <br>
  GPU 直接レンダリング。無料、アカウント不要。
  <br>
  <strong>ゼロ WebView。ゼロ OpenSSL。ゼロテレメトリ。ゼロサブスク。BYOK ファースト。純粋 Rust SSH。</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.10-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub><a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> の次期メジャーなネイティブ版 — GPU レンダリング、WebView なし、<a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>（Zed のレンダリングフレームワーク）を使用</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens が OxideTerm 内でターミナルを開くデモ" width="920">
</a>

*OxideSens がユーザーの依頼に従い、OxideTerm 内でターミナルを開く様子です。*

</div>

---

## OxideTerm Native とは

OxideTerm Native は**純粋 Rust GPUI デスクトップアプリ**——Termius & SecureCRT のオープンソース代替です。

**できること：**

- SSH と Telnet のターミナル、SFTP、ポート転送、シリアルコンソール、ローカルシェル、軽量編集を 1 つのネイティブワークスペースで管理
- Grace Period による再接続で、ネットワークが揺れてもリモート作業を維持
- OxideSens AI に、自分の AI プロバイダー経由で実行中のセッション確認と承認済みワークスペース操作の実行を任せる

ホスト型のクラウドエージェント基盤ではありません。Electron、Tauri、Web ターミナルでもありません。Chromium、WebView、JavaScript、CSS はありません。

---

## なぜ OxideTerm Native か？

| 重視すること... | OxideTerm Native が提供すること... |
|---|---|
| 1 つのリモートノード、多数のツール | ターミナル、SFTP、ポート転送、trzsz、ネイティブ IDE、監視、OxideSens AI が同じ SSH ワークスペースに接続 |
| ゼロ WebView ネイティブシェル | GPUI が GPU サーフェスに直接デスクトップ UI を描画 — DOM、CSS、JavaScript、Chromium、WebKit ランタイムなし |
| ローカルファースト SSH ワークフロー | SSH、Telnet、SFTP、転送、ローカルシェル、シリアル端末、設定はサインアップ不要 |
| BYOK OxideSens AI（プラットフォームクレジット不要） | OxideSens はあなたの OpenAI/Anthropic/Gemini/Ollama/互換エンドポイントを MCP、RAG、承認済みワークスペース操作と共に使用 |
| 再接続の安定性 | Grace Period が 30 秒間古い接続を確認 — TUI アプリが短いネットワーク切断でも生き残る |
| 純粋 Rust SSH と認証情報の安全性 | `russh` + `ring`、OpenSSL/libssh2 なし。パスワードと API キーは OS のキーチェーンに保存、`.oxide` バンドルは ChaCha20-Poly1305 + Argon2id を使用 |

---

## スクリーンショット

ネイティブ UI は、現在の Tauri 系列と同じ OxideTerm のワークスペースモデルとビジュアル言語を踏襲します。

<table>
<tr>
<td align="center"><strong>SSH ターミナル + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="OxideSens AI 付き SSH ターミナル" /></td>
<td align="center"><strong>SFTP ファイルマネージャー</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="転送キュー付き SFTP デュアルペインファイルマネージャー" /></td>
</tr>
<tr>
<td align="center"><strong>内蔵 IDE</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="内蔵 IDE モード" /></td>
<td align="center"><strong>スマートポートフォワーディング</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="自動検出付きスマートポートフォワーディング" /></td>
</tr>
</table>

---

## WebView 版との違い

| 項目 | WebView/Tauri | Native |
|---|---|---|
| 描画 | Chromium/Safari/WebKit2GTK + CSS | GPUI GPU サーフェス、即時モード、純粋 Rust |
| ターミナルデータの流れ | WebSocket → JS event loop → xterm.js | Rust 入力 → `TerminalState` → GPUI 描画 |
| IPC | コマンドごとに JSON-RPC | プロセス内関数呼び出し |
| SSH keepalive | JavaScript timer | Rust async task |
| プラグイン実行環境 | ブラウザーサンドボックスの ESM | wasmtime WASM + typed Rust ホスト API |
| CLI | desktop app の起動が必要 | standalone binary |
| ランタイム境界 | ブラウザランタイム + WebView ブリッジ | ネイティブプロセス。ブラウザランタイムを同梱しません |

## 機能概要

| カテゴリ | 機能 |
|---|---|
| ターミナル | ローカル PTY、SSH、Telnet、ローカルシリアル端末、分割ペイン、シェル連携、コマンドマーク、asciicast、trzsz、Sixel/Kitty graphics、描画ポリシー |
| SSH と認証 | 接続プール、無制限 ProxyJump、Grace Period 再接続、ホスト鍵 TOFU、SSH Agent 転送、password/key/cert/keyboard-interactive |
| SFTP / IDE | 2 ペインブラウザー、転送キュー、プレビュー、ブックマーク、アトミック書き込み、リモートファイルツリー、マルチタブエディター、競合解決 |
| ポート転送 | Local、Remote、Dynamic SOCKS5、保存済みルール、再接続時の復元、停止報告、アイドルタイムアウト |
| AI | OxideSens: OpenAI、Anthropic、Gemini、Ollama/compatible、MCP、RAG、コマンド承認 |
| クラウド同期 / `.oxide` | push/pull/apply/resolve、S3/WebDAV/Git、ロールバックバックアップ、暗号化インポート/エクスポート |
| プラグイン / CLI | WASM サンドボックス、ネイティブホスト API、プラグイン別設定；CLI は settings、connections、転送、plugins、secrets、cloud-sync、backup、report など |

## 内部構造

OxideTerm Native は WebView ブリッジを取り除き、ターミナル、SSH、Telnet、SFTP、ポート転送、IDE、AI、プラグイン、CLI を 1 つの Rust ネイティブアーキテクチャに保ちます。実装詳細は下に残しています。

<details>
<summary><strong>アーキテクチャ, SSH 内部, GPUI シェル, 再接続, AI, プラグインなど</strong></summary>
<br>

### アーキテクチャ — 単一プロセス, ブリッジなし

```text
GPUI 描画ループ
  WorkspaceApp / Tab surfaces / GPUI views
        │ プロセス内 Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

UI と SSH/terminal backend の間にシリアライズ境界はありません。Terminal bytes は `TerminalState` を直接変更し、GPUI が state を読んで GPU draw call を発行します。

### 純粋な Rust SSH — russh (ring)

Native edition は Tauri line と同じ `russh` stack を desktop binary に直接 link します。

- `ring` により **OpenSSL 依存なし**
- 完整 SSH2: 鍵交換、チャネル、SFTP サブシステム、ポート転送
- ChaCha20-Poly1305 / AES-GCM、Ed25519/RSA/ECDSA 鍵
- SSH Agent: Unix (`SSH_AUTH_SOCK`) と Windows (`\\.\pipe\openssh-ssh-agent`)
- hop ごとに独立認証する multi-hop ProxyJump

### Grace Period 付き Smart Reconnect

Reconnect semantics は Tauri line と同じですが、orchestration は Rust async task 内で完結します。

1. JavaScript timer throttling なしで SSH keepalive timeout を検出
2. ターミナルペイン、SFTP 転送、転送、IDE ファイルをスナップショット
3. Grace Period 中に旧接続を 30 秒 probe し、ネットワーク切替時も TUI apps を残せるようにする
4. 復旧できない場合は再接続し、転送を復元し、転送を再開して IDE ファイルを再オープン

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH Connection Pool と Node Routing

`SshConnectionRegistry` は `DashMap` backed で、WebSocket lifecycle bridge なしに Tauri の node-first model を維持します。

- 1 つの物理 SSH connection がターミナルペイン、SFTP、ポート転送、IDE 作業を共有
- 各 connection は `connecting → active → idle → link_down → reconnecting` を遷移
- UI は `nodeId` でコマンドを出し、`NodeRouter` がアクティブな `connectionId` をアトミックに解決
- `NodeRuntimeStore` がトポロジースナップショットを `session_tree.json` に永続化
- jump host failure は downstream nodes に `link_down` を cascade

### OxideSens AI

OxideSens は BYOK 優先のまま、コンテキスト構築はプロセス内で行います。

- プロバイダー: OpenAI、Anthropic、Gemini、Ollama、任意の OpenAI 互換エンドポイント
- MCP: stdio / SSE transports、tool discovery、invocation
- RAG: BM25 full-text、HNSW vector index、Reciprocal Rank Fusion、CJK bigram tokenizer
- AI コンテキストはワークスペース状態から作られ、認証情報はプロバイダー呼び出し前にマスクされます
- API キーは OS のキーチェーンに保存され、ログや IPC フレームには入りません

### GPUI Desktop Shell

UI は GPUI で直接描画され、DOM/CSS/JavaScript rendering pipeline はありません。

- 17 種類のワークスペースタブ: local terminal、SSH terminal、Telnet terminal、SFTP、IDE、Forwards、Settings、Plugin、Topology など
- draggable dividers 付き binary pane tree、terminal tab ごとに最大 4 panes
- Command palette、global key bindings、sidebars は GPUI primitives
- Immediate-mode rendering は serialization round-trip なしで Rust state に反応

### ターミナル状態と描画

Terminal rendering はまず Rust state としてモデル化され、その後 GPUI が描画します。

- PTY 出力は `TerminalState` に入り、scrollback、cursor、selection、marks、検索状態は Rust 側に保持されます
- 描画ポリシーは Boost、Normal、Idle の間で切り替えられ、ブラウザーイベントループの協調を待ちません
- Sixel と Kitty graphics は DOM nodes や canvas overlays ではなく、terminal-owned assets として追跡されます
- 分割ペインは同じワークスペース状態モデルを共有し、タブ復元と再接続でターミナル構成をまとめてスナップショットできます

### SFTP と IDE Workspace

リモートファイルは分離された付属機能ではなく、同じノードワークスペースの一部です。

- SFTP sessions は `NodeRouter` 経由で解決され、再接続が基盤の SSH 接続を差し替えても UI のノードアドレスは変わりません
- Transfer queues は visible file panes から独立して direction、progress、retry state、speed limits を追跡します
- IDE tabs は dirty buffers、remote paths、conflict state、restore metadata をまとめて保持します
- Backend が対応する場合、remote writes は staged/atomic behavior を使い、通常の edit flow に partial writes を出しにくくします

### プラグイン、CLI、診断

Native branch は extension と support surfaces を Rust-native boundaries に保ちます。

- Plugins はブラウザーのグローバルではなく型付きホスト機能を使い、wasmtime サンドボックスで実行されます
- CLI はドメイン crate に直接リンクし、doctor、settings、connections、転送、ポータブルバンドル、バックアップ、レポートを扱います
- 診断は秘密を含む生ペイロードではなく、件数、パス、機能フラグ、マスク済みヒントを優先します
- 状態を変更する CLI flows は dry-run plans、`--yes` guards、ロールバックバックアップを使います

### Port Forwarding — Lock-Free I/O

Forwarding は Tauri semantics を standalone Rust crate で保持します。

- Local `-L`、Remote `-R`、Dynamic SOCKS5 `-D`
- 単一の `ssh_io` task が各 SSH Channel を所有し、`Arc<Mutex<Channel>>` を避ける
- 再接続 auto-restore、停止報告、アイドルタイムアウト

### trzsz — In-Band File Transfer

trzsz は引き続き terminal stream を使い、追加 port や remote agent は不要です。

- 既存 terminal stream 経由の upload/download
- ProxyJump chains 越しに動作
- Native file pickers により browser memory limits を回避
- 双方向 transfer、directory support、configurable limits

### `.oxide` Encrypted Export

Encrypted bundle format は Tauri line と同じです。

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost、4 iterations により GPU brute-force cost を上げる
- connections、転送、settings、クイックコマンド、プラグイン設定、ポータブルシークレットを含む

</details>

---

## ソースから実行

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
| パスワードと鍵 | macOS Keychain / Windows Credential Manager / libsecret |
| Secret memory | `zeroize` / `Zeroizing` |
| 診断と AI コンテキスト | 秘密値は出力またはプロバイダー呼び出しの前にマスクされます |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI writes | dry-run plans、`--yes` guards、ロールバックバックアップ |
| Plugins | wasmtime isolation and 能力ベースのホスト API |

## リリース状況

- [x] SSH Agent 転送、Grace Period 再接続、GPUI desktop shell
- [x] WebSocket を使わないプロセス内のターミナルデータフロー
- [x] SFTP、ポート転送、IDE、AI、クラウド同期、plugins、CLI
- [x] ローカルシリアル端末
- [x] Full ProxyCommand
- [ ] Audit logging

## Contributing

## プロバイダー中立性

OxideTerm は BYOK 優先であり、プロバイダー中立です。

プロバイダー連携は、ユーザーがすでに信頼しているツールへ接続するためのものです。ランキング、広告枠、あるいは最も熱心に声をかけてきた相手への報酬ではありません。

何をドキュメントに載せるかは、compatibility、maintainability、security、そして実際の user value で決まります。Visibility は usefulness に従うもので、enthusiasm に従うものではありません。

Tauri 版に存在する機能を移植する場合は、明示的な置き換えがない限り、挙動、ラベル、interaction state、workflow を合わせてください。新しい crate は re-export だけではなく、明確な責務を持つ必要があります。

## サポートとメンテナンス

再現手順とマスク済み診断を含むバグ報告と回帰を優先します。機能リクエストは範囲、安全性、OxideTerm のリモートサーバー向けワークスペースという方向性との整合性に基づいて検討します。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

OxideTerm があなたの workflow に役立つなら、GitHub star、issue reproduction、translation fix、plugin、pull request がプロジェクトの継続を助けます。

---

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
