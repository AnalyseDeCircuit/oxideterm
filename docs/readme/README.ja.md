<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>リモートサーバー向け AI 搭載ネイティブ運用ワークスペース — 純粋 Rust ネイティブアプリ</strong>
  <br>
  SSH、Telnet、シリアル、RDP/VNC、SFTP、ポート転送、軽量編集を 1 つのネイティブワークスペースに。
  <br>
  GPU 直接レンダリング。無料、アカウント不要。
  <br>
  <strong>Electron 不使用。 WebView の同梱なし。テレメトリなし。サブスクリプションなし。BYOK 優先。OpenSSL/libssh2 に依存しない純 Rust SSH。</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.6-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>オープンソース、ローカルファースト、GPUI による GPU レンダリング。</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm の機能概要" width="920">
</p>

---

## OxideTerm とは

OxideTerm は SSH とリモート運用のためのオープンソースワークスペースです。ターミナル、ファイル、ポート転送、ホストツール、リモートデスクトップを一つの画面で扱えます。

**できること：**

- SSH、Telnet、シリアル、RDP/VNC、SFTP、ポート転送、ローカルシェル、軽量編集を一つのワークスペースで管理
- Grace Period 再接続により、短いネットワーク断の間もリモート作業を維持
- 自分の AI プロバイダーを使い、OxideSens に実行中のセッションの確認と承認済みワークスペース操作を依頼

接続情報と運用データは利用者が管理します。OxideSens は利用者自身の AI プロバイダーを使用し、アカウント登録は不要です。

---

## なぜ OxideTerm か？

- SSH、Telnet、シリアル、RDP/VNC、SFTP、ポート転送、ローカルシェルを一つのデスクトップアプリに統合
- 短いネットワーク断に対応する Grace Period 再接続
- 自分の AI 認証情報と承認済み操作を使う OxideSens
- Electron と組み込みブラウザーランタイムを使わない GPUI インターフェース

---

## スクリーンショット

以下のスクリーンショットでは、OxideTerm のターミナル、ファイル、編集、転送ワークフローを紹介します。

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

## リモート運用のための設計

OxideTerm は接続、ファイル、転送、ホストツール、自動化、AI コンテキストを一つの Rust ワークスペースにまとめます。各ツールは同じサーバー識別情報とセッションライフサイクルを共有します。

---

## 機能

| カテゴリ | 機能 |
|---|---|
| **ターミナルと接続** | ローカルシェル、SSH、Telnet、シリアル、分割ペイン、マルチホップ、安定した再接続 |
| **ファイルとリモート編集** | SFTP、転送キュー、ブックマーク、安全な書き込み、プロジェクトツリー、タブ編集 |
| **転送とネットワーク** | ローカル・リモート・動的 SOCKS5 転送、保存ルール、ソケットデバッグ |
| **ホスト運用とリモートデスクトップ** | 監視、プロセス、サービス、ログ、ポート、タスク、ディスク、パッケージ、コンテナ、tmux、RDP、VNC |
| **OxideSens と自動化** | 自分の AI プロバイダー、MCP、ローカル RAG、承認済み操作、暗号化同期、CLI |
| **拡張とカスタマイズ** | WASM プラグイン、カスタムタブ、クイックコマンド、テーマ、背景、ショートカット、11 言語 |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens が OxideTerm 内でターミナルを開くデモ" width="920">
</a>

*OxideSens がユーザーの依頼に従い、OxideTerm 内でターミナルを開く様子です。*

</div>

---

## 内部構造

OxideTerm はターミナル、SSH、Telnet、RDP、VNC、SFTP、ポート転送、IDE、AI、プラグイン、CLI を一つの Rust アーキテクチャにまとめています。技術的な詳細を以下に示します。

<details>
<summary><strong>アーキテクチャ, SSH 内部, GPUI シェル, 再接続, AI, プラグインなど</strong></summary>
<br>

### アーキテクチャ — プロセス内コア、WebView ブリッジなし

```text
GPUI 描画ループ
  WorkspaceApp / タブ画面 / GPUI ビュー
        │ プロセス内 Arc<> / async
ドメイン Crate
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

UI と SSH/ターミナルバックエンドの間にシリアライズ境界はありません。ターミナルのバイト列は `TerminalState` を直接変更し、GPUI が状態を読み取って GPU 描画命令を発行します。

### 純粋な Rust SSH — russh (ring)


- **SSH スタックは OpenSSL/libssh2 に非依存** — SSH 暗号処理には `ring` を使用
- 完整 SSH2: 鍵交換、チャネル、SFTP サブシステム、ポート転送
- ChaCha20-Poly1305 / AES-GCM、Ed25519/RSA/ECDSA 鍵
- SSH Agent: Unix (`SSH_AUTH_SOCK`) と Windows (`\\.\pipe\openssh-ssh-agent`)
- 各ホップで独立して認証する多段 ProxyJump

### Grace Period 付きスマート再接続


1. JavaScript timer throttling なしで SSH keepalive timeout を検出
2. ターミナルペイン、SFTP 転送、転送、IDE ファイルをスナップショット
3. Grace Period 中に旧接続を 30 秒 probe し、ネットワーク切替時も TUI apps を残せるようにする
4. 復旧できない場合は再接続し、転送を復元し、転送を再開して IDE ファイルを再オープン

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH 接続プールとノードルーティング


- 1 つの物理 SSH connection がターミナルペイン、SFTP、ポート転送、IDE 作業を共有
- UI は `nodeId` でコマンドを出し、`NodeRouter` がアクティブな `connectionId` をアトミックに解決
- `NodeRuntimeStore` がトポロジースナップショットを `session_tree.json` に永続化
- ジャンプホスト障害は下流ノードへ `link_down` を連鎖的に伝播

### OxideSens AI

OxideSens は BYOK 優先のまま、コンテキスト構築はプロセス内で行います。

- プロバイダー: OpenAI、Anthropic、Gemini、Ollama、任意の OpenAI 互換エンドポイント
- MCP: stdio / SSE トランスポート、ツールの検出と呼び出し
- RAG: BM25 全文検索、HNSW ベクトル索引、Reciprocal Rank Fusion、CJK バイグラムトークナイザー
- プロバイダーへ送るメッセージは認証情報パターンをマスクし、ワークスペースのコンテキストと操作はユーザーが管理します
- API キーは OS のキーチェーンに保存し、構造化ログとデスクトップコアのメッセージ対象から明示的に除外します

### GPUI デスクトップシェル

UI は GPUI で直接描画され、DOM/CSS/JavaScript rendering pipeline はありません。

- ワークスペースタブの種類: local terminal、SSH、Telnet、Serial、RDP、VNC、SFTP、IDE、Forwards、Settings、Plugin、Topology など
- draggable dividers 付き binary pane tree、terminal tab ごとに最大 4 panes
- Command palette、global key bindings、sidebars は GPUI primitives
- 即時モード描画はシリアライズの往復なしで Rust の状態変化に反応

### ターミナル状態と描画

ターミナル描画はまず Rust の状態としてモデル化され、その後 GPUI が描画します。

- PTY 出力は `TerminalState` に入り、scrollback、cursor、selection、marks、検索状態は Rust 側に保持されます
- 描画ポリシーは Boost、Normal、Idle の間で切り替えられ、ブラウザーイベントループの協調を待ちません
- Sixel と Kitty graphics は DOM nodes や canvas overlays ではなく、terminal-owned assets として追跡されます
- 分割ペインは同じワークスペース状態モデルを共有し、タブ復元と再接続でターミナル構成をまとめてスナップショットできます

### SFTP と IDE ワークスペース

リモートファイルは分離された付属機能ではなく、同じノードワークスペースの一部です。

- SFTP sessions は `NodeRouter` 経由で解決され、再接続が基盤の SSH 接続を差し替えても UI のノードアドレスは変わりません
- 転送キューは表示中のファイルペインから独立して方向、進捗、再試行状態、速度制限を追跡します
- IDE タブは未保存バッファ、リモートパス、競合状態、復元メタデータをまとめて保持します
- Backend が対応する場合、remote writes は staged/atomic behavior を使い、通常の edit flow に partial writes を出しにくくします

### プラグイン、CLI、診断

拡張機能とサポート機能は、Rust が所有する明確な境界内で動作します。

- Plugins はブラウザーのグローバルではなく型付きホスト機能を使い、wasmtime サンドボックスで実行されます
- CLI はドメイン crate に直接リンクし、doctor、settings、connections、転送、ポータブルバンドル、バックアップ、レポートを扱います
- 診断は秘密を含む生ペイロードではなく、件数、パス、機能フラグ、マスク済みヒントを優先します
- 状態を変更する CLI 処理はドライラン計画、`--yes` 保護、ロールバック用バックアップを使います

### ポート転送 — ロックフリー I/O


- Local `-L`、Remote `-R`、Dynamic SOCKS5 `-D`
- 単一の `ssh_io` task が各 SSH Channel を所有し、`Arc<Mutex<Channel>>` を避ける
- 再接続 auto-restore、停止報告、アイドルタイムアウト

### trzsz — 帯域内ファイル転送

trzsz は引き続き terminal stream を使い、追加 port や remote agent は不要です。

- 既存 terminal stream 経由の upload/download
- ProxyJump chains 越しに動作
- Native file pickers により browser memory limits を回避
- 双方向 transfer、directory support、configurable limits

### `.oxide` 暗号化エクスポート


- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost、4 iterations により GPU brute-force cost を上げる
- connections、転送、settings、クイックコマンド、プラグイン設定、ポータブルシークレットを含む

</details>

---

## ソースから実行

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

## 技術スタック

| レイヤー | 技術 | 補足 |
|---|---|---|
| UI | GPUI (Zed) | GPU 対応の即時モード、純 Rust |
| ランタイム | Tokio + DashMap | 非同期処理と並行マップ |
| SSH | russh (`ring`) | SSH スタックは OpenSSL/libssh2 に非依存、SSH Agent 対応 |
| ターミナル | portable-pty + alacritty_terminal | ローカル PTY、端末エミュレーション、Sixel/Kitty グラフィックス |
| プラグイン | wasmtime | ネイティブホスト API を備えた WASM 分離 |
| AI と検索 | SSE + BM25 + HNSW | プロバイダー配信、CJK バイグラム、RRF 統合 |

## セキュリティ

| Concern | Implementation |
|---|---|
| 保存済み認証情報 | macOS Keychain / Windows Credential Manager / libsecret |
| メモリ上の秘密情報 | 秘密情報を保持する型と一時バッファは、対応する所有権境界で `zeroize` / `Zeroizing` を使用 |
| 診断 | サポート出力は秘密情報を含むデータより、構造化メタデータとマスク済み情報を優先 |
| AI コンテキスト | プロバイダーへ送るメッセージは認証情報パターンをマスクし、ワークスペースのコンテキストと操作はユーザーが管理 |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI 書き込み | ドライラン計画、`--yes` 保護、ロールバック用バックアップ |
| プラグイン | wasmtime による分離と能力ベースのホスト API |

## 適法な利用に関する注意

OxideTerm は、追加のライセンス制限を設けず GPL-3.0-only の下で提供されます。利用にあたっては、自ら所有するか明示的なアクセス許可を得ているシステム、ネットワーク、デバイスだけにアクセスし、適用法令を遵守してください。不正アクセス、サービス妨害、アクセス制御の回避に OxideTerm を使用しないでください。

## コントリビューション

コード、ドキュメント、翻訳、プラグイン、テスト、不具合報告への貢献を歓迎します。大きな変更は Issue で相談し、範囲が明確な修正はプルリクエストで送ってください。

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## サポートとメンテナンス

再現手順とマスク済み診断を含むバグ報告と回帰を優先します。機能リクエストは範囲、安全性、OxideTerm のリモートサーバー向けワークスペースという方向性との整合性に基づいて検討します。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

OxideTerm が役立ったなら、GitHub のスター、問題の再現報告、翻訳の修正、プラグイン、プルリクエストがプロジェクトの継続を支えます。

---

## ライセンス

**GPL-3.0-only**。詳細な第三者ライセンス情報は [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md) に、追加の通知は [`NOTICE`](../../NOTICE) に記載しています。

## 謝辞

`russh`、`GPUI`、`alacritty_terminal`、`portable-pty`、`wasmtime`、`tree-sitter` に感謝します。
