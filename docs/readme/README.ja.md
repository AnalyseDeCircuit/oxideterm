<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm：サーバー管理をひとつのワークスペースに" width="920">

# ⚡ OxideTerm

**無料のネイティブ SSH クライアントとリモート運用ワークスペース。自分の API キーで使える AI アシスタントも搭載。**

SSH · Mosh · Telnet · シリアル · RDP/VNC · SFTP · ポート転送 · 内蔵エディターを、GPU 描画のアプリひとつに。
アカウント不要。サブスクリプションなし。テレメトリーなし。Electron 不使用。

[![最新リリース](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![対応プラットフォーム](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![ライセンス](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![スター](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**ダウンロード**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**ドキュメント**](https://oxideterm.app) ·
[**変更履歴**](../../.github/release-notes/stable-changelog.md) ·
[**問題を報告**](https://github.com/AnalyseDeCircuit/oxideterm/issues)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md) | [Русский](README.ru.md)

</div>

---

## クイックスタート

1. **OxideTerm をインストール。** [最新リリース](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)からパッケージをダウンロードしてください。プラットフォームごとの注意事項は、下の[インストール](#install)にあります。
2. **サーバーを追加。** セッションマネージャーで SSH 接続を作成するか、`~/.ssh/config` からホストをインポートします。
3. **接続。** ターミナルを開きます。ホスト鍵は `~/.ssh/known_hosts` と照合されます。
4. **ほかの機能も活用。** 同じノードで SFTP、ポート転送、内蔵エディターを開けます。既定では、これらの機能はひとつの SSH 接続を共有します。
5. **必要に応じて AI を有効化。** 設定で、自分の OpenAI、Anthropic、Gemini、Ollama、または OpenAI 互換エンドポイントを追加すると、OxideSens を使えるようになります。

詳しい使い方は[ドキュメント](https://oxideterm.app)をご覧ください。

---

## 主な機能

| | |
|---|---|
| **ターミナルとプロトコル** | ローカルシェル、SSH、Mosh、Telnet、シリアル、ペイン分割、多段接続、SSH エージェントとエージェント転送、2FA と TOTP 認証情報、X11 転送、シェル統合、コマンドマーク、設定可能なセッションログ、録画、Sixel と Kitty グラフィックス、trzsz 転送 |
| **tmux と一括入力** | ペインレイアウトとドラッグ可能な区切り線に対応したネイティブの `tmux -CC` 制御モード、名前付きの一括入力グループ、予約・繰り返し入力に対応した高度な複数接続向けコマンド送信 |
| **接続の安定性** | Grace Period 再接続で短いネットワーク断の間も TUI アプリを維持し、その後ポート転送、転送処理、開いていた編集ファイルを復元 |
| **ファイルと編集** | SFTP の 2 ペインファイルマネージャー、速度制限と残り時間表示に対応した転送キュー、ブックマーク、安全な書き込み・競合処理・ワークスペース復元に対応した内蔵リモートエディター |
| **ネットワーク** | ローカル・リモート・動的 SOCKS5 ポート転送、ルールの保存、リモートポート検出、接続トポロジー、その場でのソケットデバッグ |
| **リモートデスクトップ** | クリップボードと入力に対応した内蔵 RDP・VNC |
| **ホスト運用** | プロセス、サービス、ログ、ポート、タスク、ディスク、パッケージ、コンテナー、tmux の監視 |
| **AI と自動化** | 自分の API キーで使う OxideSens、MCP、ローカル RAG、Agent Skills、承認済みワークスペース操作、独立した CLI |
| **確認と監査** | 任意で有効化できる通知・監査ワークスペースと暗号化されたセッション録画（どちらも既定では無効） |
| **同期と持ち運び** | 暗号化されたクラウド同期、持ち運び可能な `.oxide` バンドル |
| **カスタマイズ** | テーマ、背景画像、ショートカット設定、クイックコマンド、11 言語のインターフェース |

---

## OxideTerm を選ぶ理由

- **無料で、ローカルを優先。** アカウント、サブスクリプション、テレメトリーは不要です。接続情報と運用データは自分で管理できます。
- **サーバーごとにひとつのワークスペース。** ターミナル、SFTP、ポート転送、RDP/VNC、エディター、監視、AI が同じノードに接続し、ひとまとまりの作業環境として機能します。
- **ブラウザーに頼らないネイティブアプリ。** [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) が GPU 上にインターフェースを直接描画します。Electron や同梱の WebView は使用しません。
- **AI の使い方は自分で決める。** OxideSens は自分のプロバイダーとキーを使い、承認した操作だけを実行します。
- **途切れにくい接続。** Grace Period 再接続は既存の接続を 30 秒間確認してから置き換えるため、短いネットワーク断でも TUI アプリを維持できます。
- **純 Rust の SSH。** SSH スタックは `russh` と `ring` を使い、OpenSSL や libssh2 に依存しません。

---

## メモリー使用量

**ネイティブ版への書き直しにより、待機時のメモリー使用量は macOS で旧版の約 4 分の 1、Windows で約 8 分の 1 になりました。** 以下は、Tauri 1.x からネイティブ GPUI 2.0 への移行時にメンテナーが記録した測定結果です。

| プラットフォーム | Tauri 1.x（待機時） | ネイティブ 2.0（待機時） | 削減率 |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 約 74% |
| Windows | 182.4 MB | 23.5 MB | 約 87% |

旧版の合計には、OxideTerm と関連する WebView プロセスが含まれます。ネイティブ版では、これらのブラウザープロセスは不要になりました。

![システムのプロセス画面による待機時メモリー比較：Tauri 1.x とネイティブ 2.0](../../docs/screenshots/oxideterm-memory-comparison.png)

---

## スクリーンショット

| OxideSens を搭載した SSH ターミナル | SFTP ファイルマネージャー |
|---|---|
| ![OxideSens AI を搭載した SSH ターミナル](../../docs/screenshots/terminal/SSHTERMINAL.png) | ![転送キュー付き SFTP 2 ペインファイルマネージャー](../../docs/screenshots/sftp/sftp.png) |

| 内蔵 IDE | スマートポート転送 |
|---|---|
| ![内蔵 IDE モード](../../docs/screenshots/miniIDE/miniide.png) | ![自動検出に対応したスマートポート転送](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png) |

<details>
<summary><b>自然な言葉での依頼から OxideSens がターミナルを開く様子を見る</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens が OxideTerm 内でターミナルを開く様子" width="720">
</a>

</details>

---

## OxideSens AI

OxideSens は任意で有効化できるアシスタントです。実行中のセッションを確認し、**承認を得た後に限り**ワークスペース操作を実行できます。

- **自分のキーを使用。** OpenAI、Anthropic（Claude）、Google Gemini、Ollama、任意の OpenAI 互換エンドポイントに対応し、プロバイダーごとの推論設定を利用できます。プラットフォーム側の利用クレジットはありません。
- **MCP と Agent Skills。** MCP サーバー（stdio・SSE）への接続と、範囲を限定した Agent Skills の読み込みに対応します。
- **ローカル知識ベース（RAG）。** BM25 全文検索とベクトル索引を組み合わせます。
- **コンテキストを自分で管理。** どのワークスペース情報と操作を承認するかを自分で選択し、コマンドポリシーのルールも適用されます。
- **認証情報のマスキング。** プロバイダーに送るメッセージには、認証情報のパターンに基づくマスキングを適用します。
- **キーは OS のキーチェーンに保管**し、構造化ログには出力しません。

---

<a id="plugins"></a>

## プラグイン

OxideTerm は次の 3 種類のプラグインに対応します。

| 種類 | 実行方法 | 実行範囲と制約 |
|---|---|---|
| **マニフェストのみ** | 宣言型の拡張で、コードは含まない | 実行可能なコードなし |
| **WASM** | Wasmtime/WASI またはサイドカー | ホスト呼び出しを制御し、許可された機能の範囲に限定 |
| **プロセス** | 通常のローカルプロセス | 信頼されたローカルコードとして実行し、OS のサンドボックスでは**隔離されない** |

旧 Tauri（1.x）向け ESM プラグインは一覧に表示される場合がありますが、ネイティブ 2.x アプリでは実行されません。プロセスプラグインは、信頼できる提供元からのみインストールしてください。

---

## セキュリティーとプライバシー

| 項目 | 仕組み |
|---|---|
| **認証情報の保管** | OS キーチェーン（macOS Keychain、Windows Credential Manager、libsecret） |
| **メモリー内の秘密情報** | 秘密情報を持つ型と一時バッファーは、対応している所有権の境界で `zeroize` により消去 |
| **ホスト鍵** | `~/.ssh/known_hosts` を使う初回利用時の信頼方式。予期しない変更は拒否 |
| **持ち運び用のエクスポート** | `.oxide` バンドルは ChaCha20-Poly1305 と Argon2id（メモリー 256 MB、反復 4 回）を使用 |
| **AI コンテキスト** | プロバイダーに送る前に認証情報のパターンをマスキング。コンテキストと操作は利用者が承認 |
| **セッション録画** | 既定では無効。端末上に暗号化して保存し、クラウド同期から除外。キーボード入力は記録しない |
| **監査** | 既定では無効。データは端末上に保存し、機密情報は暗号化 |
| **CLI による変更** | 状態を変更するコマンドには、ドライランの計画、`--yes` による確認、復旧用バックアップを用意 |
| **プラグイン** | [プラグイン](#plugins)を参照 |
| **テレメトリー** | なし |

**適法な利用。** OxideTerm は GPL-3.0-only ライセンスで提供され、追加の制限はありません。自分が所有する、または明示的にアクセスを許可されたシステム、ネットワーク、機器のみを利用し、適用される法律を遵守してください。不正アクセス、サービスの妨害、アクセス制御の回避には使用しないでください。

---

## 現在の制約

インストール前に、次の点をご確認ください。

- デスクトップ専用です（macOS、Windows、Linux）。モバイルアプリはありません。
- 開発は速いペースで進んでおり、頻繁にリリースされます。[変更履歴](../../.github/release-notes/stable-changelog.md)と[未解決の課題](https://github.com/AnalyseDeCircuit/oxideterm/issues)をご覧ください。
- 監査とセッション録画は任意で有効化する機能であり、OxideTerm 自身が観測できる範囲だけを記録します。
- プロセスプラグインは OS のサンドボックスで隔離されません。
- 描画に問題がある場合は、互換プロファイルを試してください：`OXIDETERM_RENDER_PROFILE=compatibility`。

---

<a id="for-developers"></a>

## 開発者向け

<details>
<summary><b>ソースから実行</b></summary>

**必要な環境：** Rust ツールチェーン（edition 2024）と、GPUI を実行できるデスクトップ環境。

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

Nix を使う場合：`nix build .#oxideterm`、`nix run .#oxideterm`、または `nix develop`。

CLI の成果物は `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm` に出力されます。

</details>

<details>
<summary><b>コマンドラインインターフェース</b></summary>

画面を持たない `oxideterm` CLI はアプリを起動せずに使えるため、自動化、CI、診断に便利です。設定、接続、ポート転送、プラグイン、クイックコマンド、秘密情報、持ち運び用バンドル、診断、レポート、一括実行の計画、バックアップ、クラウド同期に対応します。

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
<summary><b>アーキテクチャー</b></summary>

UI とターミナル・SSH バックエンドはひとつの Rust プロセスを共有し、任意のリモートエージェントとプラットフォーム用ヘルパーはその外側で動作します。ターミナルのバイト列は `TerminalState` を直接更新し、GPUI はその状態から描画します。JSON、WebSocket、Base64、xterm.js による解析処理は介在しません。

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

| 項目 | ブラウザー同梱方式 | OxideTerm |
|---|---|---|
| 描画 | ブラウザーエンジンと Web レイアウト | GPU 上の GPUI 描画 |
| ターミナルのデータ処理 | WebSocket → JS イベントループ → xterm.js | Rust 入力 → `TerminalState` → GPUI 描画 |
| 接続のライフサイクル | フロントエンドとバックエンドに分散 | プロセス内で完結する接続・再接続処理 |
| AI コンテキスト | アプリ内の橋渡し処理を介してコピー | 利用者の承認のもと、現在のワークスペースから生成 |
| CLI | デスクトップアプリの起動が必要 | 独立した実行ファイルで、クレートを直接利用 |

**接続プール。** `SshConnectionRegistry` は `DashMap` を基盤とし、`NodeRouter` を通じて利用します。ターミナルペイン、SFTP、ポート転送、エディターは、ノードごとにひとつの物理 SSH 接続を共有できます。ターミナルのポリシー設定で専用接続を選ぶことも可能です。各接続は `connecting → active → idle → link_down → reconnecting` の状態をたどります。踏み台ホストに障害が発生すると、配下のノードは `link_down` になります。AI とプラグインは接続の利用者として登録されるのではなく、許可された機能のハンドルとホストのスナップショットを利用します。

**Grace Period 再接続。**

1. キープアライブのタイムアウトを検出。
2. ターミナルペイン、SFTP 転送、ポート転送、編集ファイルのスナップショットを保存。
3. 既存の接続を 30 秒間確認し、短いネットワーク断でも TUI アプリを維持。
4. 新しい接続を開き、ポート転送を復元し、転送を再開して編集ファイルを開き直す。

SFTP セッションには接続の世代情報が含まれます。再接続後は条件を満たすセッションを再取得しますが、古い世代の操作を新しい接続へ暗黙に移すことはありません。

**ポート転送。** `-L`、`-R`、`-D`（SOCKS5）に対応する独立したクレートです。各 SSH チャネルはひとつの `ssh_io` タスクが所有し、頻繁に実行される処理で共有ミューテックスを使いません。

**純 Rust の SSH。** `russh` と `ring` により、完全な SSH2、ChaCha20-Poly1305 と AES-GCM、Ed25519・RSA・ECDSA 鍵、Unix（`SSH_AUTH_SOCK`）と Windows（`\\.\pipe\openssh-ssh-agent`）の SSH エージェント、各段で独立した認証を行う多段接続に対応します。

**技術スタック**

| 層 | 技術 |
|---|---|
| UI | GPUI（Zed の GPU 描画 UI フレームワーク） |
| ランタイム | Tokio、DashMap |
| SSH | `russh` と `ring`（OpenSSL・libssh2 不使用） |
| ローカル PTY | `portable-pty`（Windows では ConPTY） |
| ターミナルエミュレーション | `alacritty_terminal`（VT100–VT500、Sixel、Kitty グラフィックス） |
| エディター | tree-sitter による構文強調、独自バッファー |
| 暗号化 | ChaCha20-Poly1305、Argon2id |
| プラグイン | Wasmtime/WASI、サイドカー WASM、プロセス方式 |
| AI ストリーミング | SSE（OpenAI、Anthropic、Gemini）、プロセス内で処理 |
| RAG | BM25 と HNSW ベクトル索引をランキング融合で組み合わせ、CJK バイグラム分かち書きを使用 |
| 国際化 | `oxideterm-i18n`（11 言語） |

</details>

---

<a id="install"></a>

## インストール

[**最新リリースをダウンロード**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

| OS | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG（Intel） | DMG（Apple Silicon） |
| **Windows** | インストーラー（`.exe`） | インストーラー（`.exe`） |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

リリースページの `sha256sums.txt` ファイルでダウンロード内容を検証してください。ポータブルアーカイブと署名も同じページに掲載されています。

### macOS

Gatekeeper にブロックされた場合は、隔離属性を解除してください。

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

SmartScreen の警告が表示された場合は、**詳細情報 → 実行**を選択してください。

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

自分でビルドする場合は、[開発者向け](#for-developers)の**ソースから実行**をご覧ください。

---

## コントリビュート

Rust コード、ドキュメント、翻訳、プラグイン、テスト、不具合の再現など、さまざまな貢献を歓迎します。大きな変更は、まずイシューを開いて相談してください。

不具合の報告には、機密情報をマスキングした診断バンドルを添えると役立ちます。

```bash
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

再現可能な不具合とリグレッションを優先して対応します。機能の要望は、範囲、安全性、リモートサーバー用ワークスペースという OxideTerm の方向性との相性を検討します。OxideTerm が役に立ったら、GitHub のスター、再現可能な不具合報告、翻訳の修正、プラグインの提供などで開発を支援していただけます。

### 貢献者

OxideTerm をより良くするために協力してくださる皆さんに感謝します。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="OxideTerm の貢献者">
  </a>
</p>

---

## ライセンス

**GPL-3.0-only。** 依存ライブラリーの帰属表示は [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md)、追加の告知は [`NOTICE`](../../NOTICE) にあります。

**使用技術：** [russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
