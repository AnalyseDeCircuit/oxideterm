<p align="center">
  <img src="../../src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>OxideTerm が気に入ったら、GitHub で Star を付けてください ⭐️！</em>
</p>


<p align="center">
  <strong>Electron ゼロ。OpenSSL ゼロ。テレメトリゼロ。サブスクゼロ。純粋な Rust SSH。</strong>
  <br>
  <em>ネイティブバイナリひとつで — ローカルシェル、SSH、SFTP、リモート IDE、AI、ポートフォワーディング、プラグイン、30 以上のテーマ、11 言語。</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.1.10-blue" alt="バージョン">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="プラットフォーム">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="ライセンス">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
  <img src="https://img.shields.io/github/downloads/AnalyseDeCircuit/oxideterm/total?color=brightgreen" alt="総ダウンロード数">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=%E6%9C%80%E6%96%B0%E7%89%88%E3%82%92%E3%83%80%E3%82%A6%E3%83%B3%E3%83%AD%E3%83%BC%E3%83%89&style=for-the-badge&color=brightgreen" alt="最新版をダウンロード">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=%E6%9C%80%E6%96%B0Beta%E7%89%88%E3%82%92%E3%83%80%E3%82%A6%E3%83%B3%E3%83%AD%E3%83%BC%E3%83%89&style=for-the-badge&color=orange" alt="最新Beta版をダウンロード">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

> [!NOTE]
> **ライセンス変更：** v1.0.0 より、OxideTerm のライセンスは **PolyForm Noncommercial 1.0.0** から **GPL-3.0（GNU General Public License v3.0）** に変更されました。OxideTerm は完全なオープンソースとなり、GPL-3.0 ライセンスの条件の下で自由に使用、変更、配布できます。詳しくは [LICENSE](../../LICENSE) ファイルをご覧ください。

---

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI —「ローカルターミナルを開いて echo hello, world! を実行して」*

</div>

---

## なぜ OxideTerm なのか？

| 課題 | OxideTerm の回答 |
|---|---|
| ローカルシェルが使えない SSH クライアント | **ハイブリッドエンジン**：ローカル PTY（zsh/bash/fish/pwsh/WSL2）とリモート SSH を一画面に統合 |
| 再接続するとすべて失われる | **Grace Period 再接続**：切断前に旧接続を 30 秒間プローブ — vim/htop/yazi がそのまま生き残る |
| リモートファイル編集に VS Code Remote が必要 | **内蔵 IDE**：CodeMirror 6 over SFTP、30 以上の言語対応、オプションで Linux 向け約 1 MB のリモートエージェント |
| SSH 接続の再利用ができない | **多重化**：ターミナル、SFTP、フォワード、IDE が参照カウント方式のプールで 1 本の SSH 接続を共有 |
| SSH ライブラリが OpenSSL に依存 | **russh 0.59**：`ring` でコンパイルされた純粋な Rust SSH — C 依存ゼロ |
| 100 MB 超の Electron アプリ | **Tauri 2.0**：ネイティブ Rust バックエンド、25〜40 MB のバイナリ |
| AI が特定プロバイダーにロックイン | **OxideSens**：40 以上のツール、MCP プロトコル、RAG ナレッジベース — OpenAI/Ollama/DeepSeek/互換 API に対応 |
| 認証情報が平文設定ファイルに保存 | **OS キーチェーンのみ**：パスワードと API キーはディスクに書き込まれません；`.oxide` ファイルは ChaCha20-Poly1305 + Argon2id で暗号化 |
| クラウド依存・アカウント必須のツール | **ローカルファースト**：アカウント不要・テレメトリなし・クラウド同期なし——データは手元に。AI キーは自分で用意 |

---

## スクリーンショット

<table>
<tr>
<td align="center"><strong>SSH ターミナル + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="OxideSens AI サイドバー付き SSH ターミナル" /></td>
<td align="center"><strong>SFTP ファイルマネージャー</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="転送キュー付き SFTP デュアルペインファイルマネージャー" /></td>
</tr>
<tr>
<td align="center"><strong>内蔵 IDE（CodeMirror 6）</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="CodeMirror 6 エディター搭載の内蔵 IDE モード" /></td>
<td align="center"><strong>スマートポートフォワーディング</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="自動検出付きスマートポートフォワーディング" /></td>
</tr>
</table>

---

## 機能概要

| カテゴリ | 機能 |
|---|---|
| **ターミナル** | ローカル PTY（zsh/bash/fish/pwsh/WSL2）、SSH リモート、分割ペイン、ブロードキャスト入力、セッション録画・再生（asciicast v2）、WebGL レンダリング、30 以上のテーマ + カスタムエディター、コマンドパレット（`⌘K`）、Zen モード |
| **SSH と認証** | 接続プーリングと多重化、ProxyJump（無制限ホップ）＋トポロジーグラフ、Grace Period 付き自動再接続、Agent 転送。認証：パスワード、SSH キー（RSA/Ed25519/ECDSA）、SSH Agent、証明書、keyboard-interactive 2FA、Known Hosts TOFU |
| **SFTP** | デュアルペインブラウザー、ドラッグ＆ドロップ、スマートプレビュー（画像/動画/音声/コード/PDF/Hex/フォント）、進捗・ETA 付き転送キュー、ブックマーク、アーカイブ展開 |
| **IDE モード** | CodeMirror 6、30 以上の言語、ファイルツリー + Git ステータス、マルチタブ、競合解決、統合ターミナル。Linux 向けオプションのリモートエージェント（9 種の追加アーキテクチャ） |
| **ポートフォワーディング** | Local (-L)、Remote (-R)、Dynamic SOCKS5 (-D)、ロックフリーなメッセージパッシング I/O、再接続時の自動復元、停止報告、アイドルタイムアウト |
| **AI（OxideSens）** | インラインパネル（`⌘I`）+ サイドバーチャット、ターミナルバッファキャプチャ（単一/全ペイン）、マルチソースコンテキスト（IDE/SFTP/Git）、40 以上の自律ツール、MCP サーバー統合、RAG ナレッジベース（BM25 + ベクトルハイブリッド検索）、ストリーミング SSE |
| **プラグイン** | ランタイム ESM ローディング、18 の API 名前空間、24 の UI Kit コンポーネント、凍結 API + Proxy ACL、サーキットブレーカー、エラー時の自動無効化 |
| **CLI** | `oxt` コンパニオン：JSON-RPC 2.0 over Unix Socket / Named Pipe、`status`/`list`/`ping`、ヒューマン & JSON 出力 |
| **セキュリティ** | .oxide 暗号化エクスポート（ChaCha20-Poly1305 + Argon2id 256 MB）、OS キーチェーン、Touch ID（macOS）、ホストキー TOFU、`zeroize` メモリクリア |
| **i18n** | 11 言語：EN, 简体中文, 繁體中文, 日本語, 한국어, FR, DE, ES, IT, PT-BR, VI |

---

## 内部構造

### アーキテクチャ — デュアルプレーン通信

OxideTerm はターミナルデータとコントロールコマンドを 2 つの独立したプレーンに分離しています：

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

- **データプレーン（WebSocket）**：各 SSH セッションが専用の WebSocket ポートを持ちます。ターミナルバイトは Type-Length-Payload ヘッダー付きのバイナリフレームとして転送されます — JSON シリアライズなし、Base64 エンコードなし、ホットパスのオーバーヘッドはゼロです。
- **コントロールプレーン（Tauri IPC）**：接続管理、SFTP 操作、フォワーディング、設定 — 構造化 JSON ですがクリティカルパスの外にあります。
- **ノードファーストアドレッシング**：フロントエンドは `sessionId` や `connectionId` を直接操作しません。すべてが `nodeId` で指定され、サーバーサイドの `NodeRouter` がアトミックに解決します。SSH 再接続で内部の `connectionId` が変わっても、SFTP、IDE、フォワードは一切影響を受けません。

### 🔩 純粋な Rust SSH — russh 0.59

SSH スタック全体が **`ring`** 暗号バックエンドでコンパイルされた **russh 0.59** で構成されています：

- **C/OpenSSL 依存ゼロ** — 暗号スタック全体が Rust 実装。「どのバージョンの OpenSSL？」というデバッグが不要。
- 完全な SSH2 プロトコル：鍵交換、チャネル、SFTP サブシステム、ポートフォワーディング
- ChaCha20-Poly1305 および AES-GCM 暗号スイート、Ed25519/RSA/ECDSA キー
- カスタム **`AgentSigner`**：システム SSH Agent をラップし、russh の `Signer` トレイトを実装。`.await` をまたぐ際の RPITIT `Send` バウンド問題を、`&AgentIdentity` を所有値にクローンして解決

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **プラットフォーム対応**：Unix（`SSH_AUTH_SOCK`）、Windows（`\\.\pipe\openssh-ssh-agent`）
- **プロキシチェーン**：各ホップが独立して Agent 認証を使用
- **再接続**：`AuthMethod::Agent` が自動的にリプレイ

### 🔄 Grace Period 付きスマート再接続

多くの SSH クライアントは切断時にすべてを終了して最初からやり直します。OxideTerm の再接続オーケストレーターは根本的に異なるアプローチを取ります：

1. **検出** WebSocket ハートビートタイムアウト（300 秒、macOS App Nap と JS タイマースロットリングに最適化）
2. **スナップショット** 完全な状態を取得：ターミナルペイン、転送中の SFTP、アクティブなポートフォワード、開いている IDE ファイル
3. **インテリジェントプローブ**：`visibilitychange` + `online` イベントがプロアクティブな SSH keepalive をトリガー（受動的な 15〜30 秒のタイムアウトに対し約 2 秒で検出）
4. **Grace Period**（30 秒）：旧 SSH 接続を keepalive でプローブ — 回復すれば（例：WiFi AP 切替）、TUI アプリ（vim、htop、yazi）は完全に無傷のまま生存
5. 回復に失敗した場合 → 新規 SSH 接続 → フォワードを自動復元 → SFTP 転送を再開 → IDE ファイルを再オープン

パイプライン：`queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

すべてのロジックは専用の `ReconnectOrchestratorStore` を通じて実行されます — フックやコンポーネントに再接続コードは一切散在しません。

### 🛡️ SSH 接続プール

`DashMap` をバックエンドとした参照カウント方式の `SshConnectionRegistry` でロックフリーな並行アクセスを実現：

- **1 接続、複数コンシューマー**：ターミナル、SFTP、ポートフォワード、IDE が 1 本の物理 SSH 接続を共有 — 冗長な TCP ハンドシェイク不要
- **接続ごとのステートマシン**：`connecting → active → idle → link_down → reconnecting`
- **ライフサイクル管理**：設定可能なアイドルタイムアウト（5 分 / 15 分 / 30 分 / 1 時間 / 無制限）、15 秒の keepalive 間隔、ハートビート障害検出
- **WsBridge ハートビート**：30 秒間隔、5 分タイムアウト — macOS App Nap とブラウザの JS スロットリングに対応
- **カスケード伝播**：ジャンプホスト障害 → すべての下流ノードが自動的に `link_down` に、ステータス同期
- **アイドル切断**：フロントエンドに `connection_status_changed` を発行（内部の `node:state` だけではない）、UI の非同期を防止

### 🤖 OxideSens AI

プライバシーファーストの AI アシスタント、デュアルインタラクションモード：

- **インラインパネル**（`⌘I`）：素早いターミナルコマンド、出力はブラケットペーストで挿入
- **サイドバーチャット**：完全な履歴付きの永続的な会話
- **コンテキストキャプチャ**：Terminal Registry がアクティブペインまたはすべての分割ペインからバッファを同時取得、IDE ファイル、SFTP パス、Git ステータスを自動挿入
- **40 以上の自律ツール**：ファイル操作、プロセス管理、ネットワーク診断、TUI アプリ操作、テキスト処理 — AI が手動トリガーなしでこれらを呼び出す
- **MCP サポート**：外部 [Model Context Protocol](https://modelcontextprotocol.io) サーバー（stdio & SSE）を接続してサードパーティツールを統合
- **RAG ナレッジベース**（v0.20）：Markdown/TXT ドキュメントをスコープ付きコレクション（グローバルまたは接続単位）にインポート。Reciprocal Rank Fusion で BM25 キーワードインデックス + ベクトルコサイン類似度のハイブリッド検索を融合。Markdown 対応のチャンキングで見出し階層を保持。CJK バイグラムトークナイザーで中国語/日本語/韓国語に対応。
- **プロバイダー**：OpenAI、Ollama、DeepSeek、OneAPI、または任意の `/v1/chat/completions` エンドポイント
- **セキュリティ**：API キーは OS キーチェーンに保存、macOS ではキー読み取りは `LAContext` 経由の **Touch ID** で認証ゲート — エンタイトルメントやコード署名は不要、セッションごとに初回認証後キャッシュ

### 💻 IDE モード — リモート編集

SFTP 上で動作する CodeMirror 6 エディター — デフォルトではサーバー側のインストールは不要：

- **ファイルツリー**：遅延読み込みのディレクトリ、Git ステータスインジケーター（変更/未追跡/追加）
- **24 の言語モード**：14 のネイティブ CodeMirror + `@codemirror/legacy-modes` によるレガシーモード
- **競合解決**：楽観的 mtime ロック — 上書き前にリモート変更を検出
- **イベント駆動 Git**：保存、作成、削除、リネーム、ターミナルの Enter キー押下時に自動リフレッシュ
- **State Gating**：`readiness !== 'ready'` の場合はすべての IO をブロック、Key-Driven Reset で再接続時に完全再マウントを強制
- **リモートエージェント**（オプション）：約 1 MB の Rust バイナリ、x86_64/aarch64 Linux に自動デプロイ。9 種の追加アーキテクチャ（ARMv7、RISC-V64、LoongArch64、s390x、Power64LE、i686、ARM、Android aarch64、FreeBSD x86_64）は `agents/extra/` に手動アップロード用。強化されたファイルツリー、シンボル検索、ファイル監視を有効化。

### 🔀 ポートフォワーディング — ロックフリー I/O

完全な Local (-L)、Remote (-R)、Dynamic SOCKS5 (-D) フォワーディング：

- **メッセージパッシングアーキテクチャ**：SSH Channel は単一の `ssh_io` タスクが所有 — `Arc<Mutex<Channel>>` なし、ミューテックス競合を完全に排除
- **停止報告**：フォワードタスクが終了理由（SSH 切断、リモートポートクローズ、タイムアウト）を能動的に報告し、明確な診断を提供
- **自動復元**：`Suspended` 状態のフォワードは再接続時にユーザー操作なしで自動再開
- **アイドルタイムアウト**：`FORWARD_IDLE_TIMEOUT`（300 秒）でゾンビ接続の蓄積を防止

### 🔌 ランタイムプラグインシステム

セキュリティ強化された凍結 API サーフェスを持つ動的 ESM ローディング：

- **PluginContext API**：18 の名前空間 — terminal、ui、commands、settings、lifecycle、events、storage、system
- **24 の UI Kit コンポーネント**：プラグインサンドボックスに `window.__OXIDE__` 経由で注入されるビルド済み React コンポーネント（ボタン、入力、ダイアログ、テーブル…）
- **セキュリティメンブレン**：すべてのコンテキストオブジェクトに `Object.freeze`、Proxy ベースの ACL、IPC ホワイトリスト、繰り返しエラー時の自動無効化付きサーキットブレーカー
- **共有モジュール**：React、ReactDOM、zustand、lucide-react がプラグイン用に公開され、重複バンドルを回避

### ⚡ アダプティブレンダリング

固定の `requestAnimationFrame` バッチ処理を置き換える 3 段階レンダースケジューラー：

| ティア | トリガー | レート | 効果 |
|---|---|---|---|
| **Boost** | フレームデータ ≥ 4 KB | 120 Hz+（ProMotion ネイティブ） | `cat largefile.log` でのスクロールラグを排除 |
| **Normal** | 通常のタイピング | 60 Hz（RAF） | スムーズなベースライン |
| **Idle** | 3 秒間 I/O なし / タブ非表示 | 1〜15 Hz（指数バックオフ） | GPU 負荷ほぼゼロ、バッテリー節約 |

遷移は完全に自動 — データ量、ユーザー入力、Page Visibility API に基づいて駆動。バックグラウンドタブは RAF を起動せずアイドルタイマーでデータをフラッシュし続けます。

### 🔐 .oxide 暗号化エクスポート

ポータブルで改ざん防止の接続バックアップ：

- **ChaCha20-Poly1305 AEAD** 認証付き暗号化
- **Argon2id KDF**：メモリコスト 256 MB、4 イテレーション — GPU ブルートフォース耐性
- **SHA-256** 整合性チェックサム
- **オプションの鍵埋め込み**：秘密鍵を Base64 エンコードして暗号化ペイロードに含める
- **プリフライト分析**：認証タイプの内訳、エクスポート前の不足キー検出

### 📡 ProxyJump — トポロジー対応マルチホップ

- 無制限のチェーン深度：`Client → Jump A → Jump B → … → Target`
- `~/.ssh/config` を自動解析、トポロジーグラフを構築、Dijkstra 経路探索で最適ルートを決定
- ジャンプノードを独立セッションとして再利用可能
- カスケード障害伝播：ジャンプホストのダウン → すべての下流ノードを自動的に `link_down` に設定

### ⚙️ ローカルターミナル — スレッドセーフ PTY

`portable-pty 0.8` によるクロスプラットフォームローカルシェル、`local-terminal` フィーチャーゲート：

- `MasterPty` は `std::sync::Mutex` でラップ — 専用 I/O スレッドでブロッキング PTY 読み取りを Tokio イベントループから分離
- シェル自動検出：`zsh`、`bash`、`fish`、`pwsh`、Git Bash、WSL2
- `cargo build --no-default-features` で PTY を除外し、モバイル/軽量ビルドに対応

### 🪟 Windows 最適化

- **ネイティブ ConPTY**：Windows Pseudo Console API を直接呼び出し — フル TrueColor と ANSI サポート、レガシー WinPTY 不要
- **シェルスキャナー**：レジストリと PATH から PowerShell 7、Git Bash、WSL2、CMD を自動検出

### その他の機能

- **リソースプロファイラー**：永続 SSH チャネルで `/proc/stat` を読み取り、デルタベース計算でリアルタイム CPU/メモリ/ネットワーク監視、非 Linux では RTT のみに自動縮退
- **カスタムテーマエンジン**：30 以上の内蔵テーマ、ライブプレビュー付きビジュアルエディター、20 の xterm.js フィールド + 24 の UI カラー変数、ターミナルパレットから UI カラーを自動導出
- **セッション録画**：asciicast v2 形式、完全な録画と再生
- **ブロードキャスト入力**：一度入力するとすべての分割ペインに送信 — バッチサーバー操作
- **背景ギャラリー**：タブごとの背景画像、16 のタブタイプ、不透明度/ぼかし/フィットコントロール
- **CLI コンパニオン**（`oxt`）：約 1 MB のバイナリ、JSON-RPC 2.0 over Unix Socket / Named Pipe、`status`/`list`/`ping` をヒューマンまたは `--json` 出力
- **WSL Graphics** ⚠️ 実験的：内蔵 VNC ビューア — 9 つのデスクトップ環境 + 単一アプリモード、WSLg 検出、Xtigervnc + noVNC

<details>
<summary>📸 11 言語の実動作</summary>
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

## インストール

[GitHub Releases](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) から最新版をダウンロードしてください。

| プラットフォーム | ランタイム依存関係 |
|---|---|
| **Windows** | [WebView2 ランタイム](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) — Windows 10（1803+）および Windows 11 にはプリインストール済み。**エアギャップ / イントラネット**環境では、[Evergreen スタンドアロンインストーラー](https://developer.microsoft.com/en-us/microsoft-edge/webview2/#download)（オフライン、約 170 MB）を使用するか、グループポリシーで**固定バージョン**ランタイムをデプロイしてください。 |
| **macOS** | なし（ネイティブ WebKit を使用） |
| **Linux** | `libwebkit2gtk-4.1`（モダンなデスクトップでは通常プリインストール済み） |

---

## クイックスタート

### 前提条件

- **Rust** 1.85 以上
- **Node.js** 18 以上（pnpm 推奨）
- **プラットフォームツール**：
  - macOS：Xcode コマンドラインツール
  - Windows：Visual Studio C++ ビルドツール
  - Linux：`build-essential`、`libwebkit2gtk-4.1-dev`、`libssl-dev`

### 開発

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# CLI コンパニオンをビルド（CLI 機能に必要）
pnpm cli:build

# フルアプリ（フロントエンド + Rust バックエンド、ホットリロード付き）
pnpm run tauri dev

# フロントエンドのみ（Vite、ポート 1420）
pnpm dev

# プロダクションビルド
pnpm run tauri build
```

---

## テックスタック

| レイヤー | テクノロジー | 詳細 |
|---|---|---|
| **フレームワーク** | Tauri 2.0 | ネイティブバイナリ、25〜40 MB |
| **ランタイム** | Tokio + DashMap 6 | 完全非同期、ロックフリー並行マップ |
| **SSH** | russh 0.59（`ring`） | 純粋な Rust、C 依存ゼロ、SSH Agent |
| **ローカル PTY** | portable-pty 0.8 | フィーチャーゲート、Windows は ConPTY |
| **フロントエンド** | React 19.1 + TypeScript 5.8 | Vite 7、Tailwind CSS 4 |
| **状態管理** | Zustand 5 | 19 の専用ストア |
| **ターミナル** | xterm.js 6 + WebGL | GPU アクセラレーション、60fps 以上 |
| **エディター** | CodeMirror 6 | 30 以上の言語モード |
| **暗号化** | ChaCha20-Poly1305 + Argon2id | AEAD + メモリハード KDF（256 MB） |
| **ストレージ** | redb 2.1 | 組込み KV ストア |
| **i18n** | i18next 25 | 11 言語 × 22 名前空間 |
| **プラグイン** | ESM ランタイム | 凍結 PluginContext + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## セキュリティ

| 項目 | 実装 |
|---|---|
| **パスワード** | OS キーチェーン（macOS Keychain / Windows Credential Manager / libsecret） |
| **AI API キー** | OS キーチェーン + macOS での Touch ID 生体認証ゲート |
| **エクスポート** | .oxide：ChaCha20-Poly1305 + Argon2id（メモリ 256 MB、4 イテレーション） |
| **メモリ** | Rust メモリ安全性 + 機密データの `zeroize` クリア |
| **ホストキー** | `~/.ssh/known_hosts` による TOFU、変更検出で拒否（MITM 防止） |
| **プラグイン** | Object.freeze + Proxy ACL、サーキットブレーカー、IPC ホワイトリスト |
| **WebSocket** | 時間制限付きシングルユーストークン |

---

## ロードマップ

- [x] SSH Agent フォワーディング
- [ ] プラグインマーケットプレイス
- [ ] セッション検索とクイック切替

---

## ライセンス

**GPL-3.0** — 本ソフトウェアは [GNU 一般公衆利用許諾書 v3.0](https://www.gnu.org/licenses/gpl-3.0.html) のもとで公開されているフリーソフトウェアです。

GPL-3.0 の条件のもとで、本ソフトウェアを自由に使用、修正、配布できます。派生物は同じライセンスのもとで配布する必要があります。

全文：[GNU 一般公衆利用許諾書 v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## 謝辞

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>236,000 行以上の Rust & TypeScript — ⚡ と ☕ で構築</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
