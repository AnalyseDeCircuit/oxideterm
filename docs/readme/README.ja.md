<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm：サーバー管理をひとつのワークスペースに" width="920">

# ⚡ OxideTerm

**リモートサーバーでの作業に使える、無料のネイティブ SSH クライアント。**

SSH · Mosh · Telnet · シリアル · RDP/VNC · SFTP · ポート転送 · コードエディター

アカウント不要。サブスクリプションなし。テレメトリーなし。Electron 不使用。

[![最新リリース](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![対応プラットフォーム](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![ライセンス](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![スター](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**ダウンロード**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**ドキュメント**](https://oxideterm.app) ·
[**変更履歴**](../../.github/release-notes/stable-changelog.md) ·
[**問題を報告**](https://github.com/AnalyseDeCircuit/oxideterm/issues) ·
[**WeChat 交流グループ**](https://github.com/AnalyseDeCircuit/oxideterm/issues/752)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md)

</div>

---

## OxideTerm とは

OxideTerm は macOS、Windows、Linux 向けの SSH クライアントです。サーバーごとにワークスペースを持ち、ターミナル、ファイル管理、ポート転送、コード編集、ホスト監視、任意の AI アシスタントを一つのウィンドウで使えます。GPL-3.0 の無料オープンソースソフトウェアです。

## OxideTerm を選ぶ理由

- **一度の接続で各ツールを利用。** 同じサーバーのターミナル、SFTP、ポート転送、エディターは標準で SSH 接続を共有するため、ツールを開くたびに再認証する必要はありません。
- **短い通信断にも対応。** 接続が応答しなくなると、置き換える前に 30 秒間元の接続を確認し続け、vim や htop などの全画面プログラムが元の状態から再開できるようにします。
- **少ないメモリーで動作。** Zed の UI フレームワーク [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) で GPU に直接描画します。アイドル時は macOS で約 81 MB、Windows で約 24 MB です。
- **AI は自分で選択。** OxideSens は選んだプロバイダーと API キーを使用します。OxideTerm 専用クレジットの購入は不要です。
- **Rust 製の SSH。** russh と ring を使用し、OpenSSL や libssh2 に依存しません。

<details>
<summary><b>OxideTerm 1.x とのメモリー比較</b></summary>

Tauri（1.x）からネイティブ GPUI（2.0）への移行前後のアイドル時メモリーです。1.x の値には WebView プロセスを含みます。

| プラットフォーム | 1.x（Tauri） | 2.0（ネイティブ） | 削減率 |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 74% |
| Windows | 182.4 MB | 23.5 MB | 87% |

![システムのプロセス監視で比較した OxideTerm 1.x と 2.0 のアイドル時メモリー](../../docs/screenshots/oxideterm-memory-comparison.png)

</details>

---

## スクリーンショット

### ターミナル

SSH セッションの画面です。サイドバーから同じサーバーの追加ターミナル、SFTP、IDE、ポート転送を開けます。

![サイドバーにサーバーツールを表示した SSH ターミナル](../../docs/screenshots/terminal/SSHTERMINAL.png)

### 混合ワークスペース

SSH ターミナル、ローカルターミナル、Markdown ノート、SFTP を 1 つのタブに配置できます。ローカルと複数のサーバーのページを、最大 16 ペインで自由に組み合わせられます。

![SSH、ローカルターミナル、ノート、SFTP を組み合わせた 4 ペインのワークスペース](../../docs/screenshots/mixed-workspace.png)

### ファイル

ローカルとリモートのフォルダーを左右に並べ、下部に転送キューを表示します。

![転送キュー付きの 2 ペイン SFTP ファイルマネージャー](../../docs/screenshots/sftp/sftp.png)

### IDE

リモートプロジェクトを開き、ファイルをタブで編集できます。

![内蔵 IDE で開いたリモートプロジェクト](../../docs/screenshots/miniIDE/miniide.png)

### ポート転送

サーバーで待ち受け中のポートを一覧表示し、クリックで転送できます。Jupyter などにはクイック転送も使えます。

![検出したサーバーポートを表示するポート転送画面](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png)

<details>
<summary><b>自然な言葉での依頼から OxideSens がターミナルを開く様子を見る</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens が OxideTerm 内でターミナルを開く様子" width="720">
</a>

</details>

---

## 機能

*（プラグイン）* と記載された機能は、アプリ内のプラグインマーケットからインストールします。

### ワークスペース

- セッション管理：保存接続の検索、並べ替え、グループ化、インポート／エクスポート、入れ子のグループ、タグ、色、アイコン、メモ
- `~/.ssh/config` から `Match` ブロック、`ProxyJump` チェーン、`RemoteCommand` を取り込み、取り込んだホストを最新に維持
- SecureCRT、Xshell、Termius、MobaXterm、WindTerm、Electerm、FinalShell から保存前のプレビュー付きで取り込み
- 全タブ、グローバルページ、ローカルターミナル、サーバー単位でタブを絞り込み；非表示のセッションも実行を継続
- 1 タブに最大 16 ペイン；ターミナル、SFTP、IDE、転送、ノート、設定、リモートデスクトップを混在可能
- セッションを切断せずタブを独立ウィンドウへ移動し、元に戻す
- 開いている 2–16 個のターミナルを再接続せずグリッド配置
- 操作と保存接続のコマンドパレット、1 ページに集中する Zen モード
- シェル、開始ディレクトリ、アイコンを持つローカルターミナルプロファイル
- 起動時にワークスペース、ウィンドウサイズ、全画面状態、モニター位置を復元

### ターミナル

- Unix シェル、コマンドプロンプト、Windows PowerShell、PowerShell Core、Git Bash、Nushell を自動検出
- ドラッグできる区切り線で分割；新しいローカルペインは作業ディレクトリを引き継ぐ
- 正規表現、大文字小文字、複数行検索；検索状態はペインごとに保持
- Bash、Zsh、Fish、Nushell、PowerShell のシェル統合：ディレクトリ、終了コード、コマンドマークを追跡し、アプリからリモートに導入
- 完了したコマンド出力を折りたたみ、終了状態を保持
- 複数行入力、履歴、パス候補、補完、クイックコマンドを備えたコマンドバー
- あいまい検索による履歴候補と、再起動後も残る接続別履歴（任意）
- マウスで現在のコマンドを編集できる自由入力モード
- 複数行の貼り付けを送信前に編集；元に戻す操作とコードフェンスの除去に対応
- 文字列や正規表現で出力を検知し、入力送信、クイックコマンド、プログラム起動、クリック可能な操作への変換を実行
- エラー、Git 状態、権限、アドレス、日付など無着色の出力を意味に応じて着色；キーワード／正規表現の強調も可能
- クリック可能な URL とファイルパス、ミリ秒精度の行タイムスタンプ、タブの未読出力マーク
- Sixel／Kitty グラフィックスと Kitty アニメーション
- ペイン配置とドラッグ可能な区切り線を備えたネイティブ `tmux -CC` 制御モード
- 複数端末への同期入力グループ、速度・間隔・反復を指定できるテキスト／16 進コマンド送信
- クイックコマンドのグループ、固定表示、テキスト／選択／秘密パラメーター、ホスト別表示ルール
- セッションログのファイル名テンプレート、サイズ・保存期間制限、任意の ANSI；`.cast` 録画と内蔵プレーヤー
- ローカルと対応 SSH セッションで、Vim、Neovim、Emacs の選択範囲をコピー、切り取り、貼り付け
- 接続別の Backspace／Delete シーケンスと文字エンコーディング

### 接続

- パスワード、秘密鍵、OpenSSH 証明書、管理鍵、SSH Agent、キーボード対話認証、Kerberos に対応
- SSH Agent の自動検出と独自エンドポイント（macOS/Linux：`SSH_AUTH_SOCK`、Windows：`\\.\pipe\openssh-ssh-agent`）、Agent 転送
- Ed25519、RSA、ECDSA 鍵と ChaCha20-Poly1305／AES-GCM；アルゴリズム順序の調整と旧サーバー互換設定
- サーバーや各踏み台で一致する認証プロンプトに TOTP を生成・自動入力
- PIN とタッチ確認付き FIDO 鍵（`ed25519-sk`、`ecdsa-sk`）*（プラグイン）*
- 各段で個別認証する踏み台チェーン、HTTP CONNECT／SOCKS5 上流プロキシ、ProxyCommand
- 猶予期間付き再接続で新接続に転送を復元し、転送を再開してエディターのファイルを再度開く
- サーバーごとに接続共有、または新規ターミナルごとに個別認証
- 信頼済み／非信頼の X11 転送
- 保存しない一時接続と、アプリを開く `ssh://`、`telnet://`、`mosh://`、`rdp://`、`vnc://` リンク
- 適応型ローカルエコー予測付き Mosh *（プラグイン）*
- オプション交渉、プロキシ経由接続、改行設定に対応した Telnet
- シリアルのボーレート、データビット、パリティ、ストップビット、フロー制御、16 進入出力、Break／DTR／RTS
- Nordic UART Service または独自 GATT 特性による Bluetooth LE ターミナル

### ファイルと転送

- ローカル／リモートの 2 ペイン管理、ブックマーク、パス補完、リスト／ツリー表示
- 接続ごとにローカル／リモートのパスを組で保存
- 並列転送、速度制限、残り時間、一時停止、再開、再試行；中断した SFTP 転送は再起動後に一時停止状態で復元
- 競合時は上書き、スキップ、名前変更を 1 ファイルまたは全体に適用
- SFTP サブシステムのないホストでは SCP に自動切り替え
- 独立 SFTP 接続と、アプリを経由するサーバー間転送
- FTP と明示的 FTPS
- SSH ターミナルにファイルをドロップし、現在のディレクトリへアップロード
- リモートシェルで `trz`/`tsz`（trzsz）、`rz`/`sz`（ZMODEM）；シリアルで XMODEM、YMODEM、ZMODEM
- テキスト、コード、Markdown、画像、音声、動画、16 進、フォントのプレビュー
- PDF、SQLite、証明書、バイナリのビューアー *（プラグイン）*
- ターミナルと SFTP で同じディレクトリを開き、SFTP のディレクトリをターミナルに送信

### エディターと IDE

- SFTP、単純なシェルコマンド、またはシンボル索引・補完・ファイル監視を提供するリモート Agent 経由の IDE
- アイドル状態のターミナルを同じユーザー、コンテナー、ディレクトリで IDE に変換
- タブ、複数選択可能なプロジェクトツリー、プロジェクト検索、正規表現・キャプチャーグループによる置換
- 最大 100 MiB のファイルを扱い、エンコーディング、BOM、改行を保持
- 競合を扱う安全な保存、タブ切り替え時の任意の自動保存
- 開いたファイルと未保存の変更を再起動後に復元
- Git のブランチ、worktree、ステージ、履歴、先行／遅延コミット数、マージ／リベース状態の表示
- プロジェクトタスクの検出と実行
- 内蔵 15 言語、追加で 20 以上の言語プラグインによる構文強調 *（プラグイン）*
- 表、数式、脚注に対応した Markdown プレビュー

### ポート転送とネットワーク

- ローカル／リモート／動的 SOCKS5 転送と、接続時に自動開始できる保存ルール
- サーバーの待ち受けポートを検出してワンクリック転送
- ターミナル不要の独立 SSH トンネル、任意でアプリ起動時に開始
- 再接続で転送を復元；手動停止したルールは停止を維持
- アプリ全体のプロキシと、企業 CA を含むシステム信頼証明書によるネットワーク要求
- 接続トポロジー表示と一時的なソケットデバッグ

### リモートデスクトップ

- キーボード、マウス、クリップボード、拡大縮小、再接続対応の RDP／VNC *（プラグイン）*
- 複数モニターとネットワーク品質プロファイルに対応する RDP *（プラグイン）*
- VeNCrypt TLS 経由の VNC *（プラグイン）*
- SSH 踏み台経由の RDP／VNC *（プラグイン）*
- 内蔵ビューアーで WSLg アプリを表示（Windows）

### ホストツール

- ローカルマシンと任意の SSH サーバーに対応
- CPU、メモリー、スワップ、ディスク、ネットワーク、負荷、遅延と履歴グラフ
- プロセス（シグナル、停止・再開、nice）、サービス（systemd、launchd、BSD、Windows）、予定タスク、パッケージ；Windows の winget 更新・削除
- プリセットと追跡モード付きログ、公開状態を示す待ち受けポート、空き容量警告付きファイルシステム
- Docker の状態、ポート、開始、停止、再起動、ログ
- NVIDIA、AMD ROCm、Hygon DCU、Huawei Ascend、Intel XPU、Moore Threads MUSA、Cambricon の GPU／NPU 監視
- tmux、Zellij、Herdr セッションの接続、作成、分割、名前変更
- 接続プール／接続マトリックスのダッシュボード、ターミナルバーの SSH 遅延表示

### OxideSens AI

- OpenAI（Responses API 含む）、Anthropic、Gemini、DeepSeek、xAI Grok、Xiaomi MiMo、Ollama、OpenAI 互換エンドポイントと推論強度設定
- 選択したターミナル、ファイル、設定をコンテキストに使用し、コマンド、対話プログラム操作、ファイル編集、データ転送を実行
- 読み取り専用、標準、承認不要のモード、ツール分類別権限、コマンドポリシー、破壊的操作や認証情報に関する追加審査
- 画像添付、会話分岐、メッセージ編集、続く質問のキュー
- 並列サブエージェント、バックグラウンドタスク、中断後のタスク再開
- ACP 経由の Codex／Claude Code、OpenCode、Antigravity、Cursor、Grok Build、GitHub Copilot CLI、Qwen Code、Kimi CLI のランチャー *（プラグイン）*
- stdio、Streamable HTTP、従来の SSE 経由で MCP サーバーに接続
- 外部 MCP 制御：エディターや他の AI ツールから保存接続を利用；ツールグループ単位の権限付与・取り消し
- [Agent Skills](../../docs/agent-skills.md)（`SKILL.md`）と `/skill-name` 補完
- BM25・HNSW ベクトル検索、CJK 分かち書き、Mermaid プレビュー付き Markdown ノート知識ベース

### セキュリティー

- Touch ID（macOS）／Windows Hello（Windows）でアプリをロックし、接続は維持
- sudo／su のプロンプトに保存パスワードを提示；ローカル、単一接続、ホストパターンに限定でき、2 段階プロンプトの TOTP にも対応
- フィルター、保存期間、JSON／CSV 出力付き操作監査
- 暗号化セッション録画、再生位置移動と再生速度調整
- プラグイン有効化前の権限確認

### 同期とバックアップ

- WebDAV、S3 互換ストレージ（Alibaba Cloud OSS 含む）、Git リポジトリー、GitHub Gist、Dropbox、OneDrive、Google Drive、HTTP JSON によるクラウド同期
- 同期対象を選び、複数端末の変更をマージして競合を個別解決
- ロールバック用バックアップと、中断した同期からの復旧
- 接続、転送、設定、クイックコマンド、鍵を暗号化 `.oxide` に格納；インポート時にスキップ、名前変更、置換、マージ
- 専用暗号化キーストアを持つフォルダーからのポータブル動作
- Stable／Beta チャンネルによるアプリ内更新

### カスタマイズ

- ライト／ダーク／システム外観、アプリとターミナル別テーマ、テーマエディター
- 背景画像、アニメーション、動画、霧・粒子・流星・水面コースティクスなどの生成効果；追加素材パック *（プラグイン）*
- ウィンドウの不透明度と角のスタイル
- 独立 CJK フォント、合字、ウェイトのフォントスタックと、エディター固有の組版
- UI 倍率、カーソル、余白、動きの設定
- 独自ショートカット、検索・並べ替え可能な設定
- 11 言語、IME 入力、双方向テキスト

### プラグイン

| 種類 | 実行方式 | アクセス範囲 |
|---|---|---|
| **マニフェスト** | 宣言的拡張 | なし；コード実行なし |
| **WASM** | Wasmtime/WASI または補助プロセス | 許可されたホスト機能のみ |
| **プロセス** | 通常のローカルプロセス | ローカルプログラムと同じ；OS サンドボックスなし |

プロセス型プラグインは信頼できる配布元からのみ導入してください。1.x 向けは一覧に出る場合がありますが、2.x では実行しません。

### コマンドラインツール

**設定 → 一般 → CLI Companion** から `oxideterm` を導入します。アプリと設定を共有し、ウィンドウなしの検証、スクリプト、バックアップやシェルからのタブ起動に使えます。変更コマンドは `--dry-run` で計画を確認でき、`--yes` 指定時のみ適用され、ロールバック用バックアップを保持します。

```bash
oxideterm terminal --cwd ~/project
oxideterm doctor --strict
oxideterm connections search prod
oxideterm forwards list --format json
oxideterm oxide export ./profile.oxide --connection prod --password-stdin
```

Bash、Zsh、Fish、PowerShell、Elvish の補完に対応。全コマンドは [CLI ガイド（英語）](../../docs/user-guide/en/cli.md) を参照してください。

---

<a id="install"></a>

## インストール

[最新リリース](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)から OxideTerm をダウンロードしてください。

| OS | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG（Intel） | DMG（Apple Silicon） |
| **Windows** | インストーラー（`.exe`） | インストーラー（`.exe`） |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

リリースページにはポータブル版、署名、検証用 `sha256sums.txt` もあります。

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

### ウィンドウが描画されない場合

環境変数 `OXIDETERM_RENDER_PROFILE=compatibility` を設定し、OxideTerm を再起動してください。

### はじめに

1. **セッションマネージャー**でサーバーを追加するか、`~/.ssh/config` から取り込みます。
2. 接続してターミナルを開きます。
3. 同じサーバーから SFTP、ポート転送、IDE を開きます。
4. OxideSens を使うには、**設定**で AI プロバイダーを追加します。

その他の機能は[ドキュメント](https://oxideterm.app)にあります。自分でビルドする場合は[ソースからビルド](#build-from-source)を参照してください。

---

## プライバシーとセキュリティー

- パスワードと鍵は OS のキーチェーン（macOS Keychain、Windows Credential Manager、Linux の libsecret）に保存し、AI キーはログに残しません。
- メモリー内の秘密情報は解放時に zeroize で消去します。
- ホスト鍵は初回信頼後に `~/.ssh/known_hosts` と照合し、変更された鍵を拒否します。
- AI に送信するテキストには、先に認証情報のマスキングを適用します。
- セッション録画は標準で無効です。端末上で暗号化し、クラウド同期から除外し、キー入力を記録しません。
- 通知と監査は標準で無効です。有効時もデータは端末内に留まり、機密部分を暗号化し、OxideTerm 内の操作のみを対象にします。
- `.oxide` は ChaCha20-Poly1305 と Argon2id 派生鍵（256 MB、4 回）で暗号化します。

---

<a id="build-from-source"></a>

## ソースからビルド

Rust（rustup が `rust-toolchain.toml` 指定版を導入）と、GPUI を実行できるデスクトップ環境が必要です。

```bash
# Run the app
cargo run

# Build the oxideterm CLI
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Nix では `nix build .#oxideterm`、`nix run .#oxideterm`、`nix develop` を使用します。CLI は `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm` に配置されます。

### 全体の構成

UI、ターミナル、SSH は一つの Rust プロセスで動作し、任意のリモート Agent とプラットフォーム補助プログラムは別プロセスです。SSH／ローカル PTY 出力は直接 `TerminalState` に入り、GPUI が WebView、JSON ブリッジ、xterm.js を介さず描画します。

`NodeRouter` は DashMap ベースの `SshConnectionRegistry` から接続を配布します。個別接続を選ぶターミナルを除き、ターミナル、SFTP、転送、エディターはサーバーごとに接続を共有します。状態は `connecting → active → idle → link_down → reconnecting` です。踏み台障害時は配下サーバーが `link_down` になり、転送ルールはそれぞれリスナーを持ちます。再接続後は SFTP セッションを取得し直し、旧接続の操作を新接続へ黙って移しません。

```text
crates/
├── oxideterm-gpui-app/        # Desktop app: windows, workspace, settings
├── oxideterm-gpui-terminal/   # Terminal panes: input, selection, overlays
├── oxideterm-terminal/        # Terminal model and escape-sequence parsing
├── oxideterm-ssh/             # SSH connections, NodeRouter, jump hosts
├── oxideterm-sftp/            # SFTP sessions and transfers
├── oxideterm-forwarding/      # Port forwarding runtime
├── oxideterm-gpui-ide/        # Remote IDE
├── oxideterm-ai/              # OxideSens: providers, MCP, RAG
├── oxideterm-cli/             # The oxideterm command
├── oxideterm-i18n/            # Locale catalogs for the 11 interface languages
└── gpui-ce/                   # Patched copy of GPUI
```

関連資料（英語）：[開発ガイド](../../docs/development/README.md)、[crate マップ](../../docs/development/crate-map.md)、[アーキテクチャー](../../docs/user-guide/en/architecture.md)。

---

## コントリビュート

コード、文書、翻訳、プラグイン、テスト、不具合再現の貢献を歓迎します。大きな変更は先に issue で相談してください。PR の準備は [CONTRIBUTING.md](../../docs/CONTRIBUTING.md) を参照してください。

機密情報をマスキングした診断パッケージがあると、不具合を修正しやすくなります：

```bash
oxideterm report --bundle ./oxideterm-report.zip
```

ソースから実行する場合は `cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip` を使用してください。

### 貢献者

OxideTerm をより良くするために協力してくださる皆さんに感謝します。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="OxideTerm の貢献者">
  </a>
</p>

---

## 謝辞

OxideTerm は以下のオープンソースプロジェクトを利用しています：

- [russh](https://github.com/warp-tech/russh) と [ring](https://github.com/briansmith/ring)：SSH と暗号
- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui)：Zed の GPU UI フレームワーク
- [alacritty_terminal](https://github.com/alacritty/alacritty)：ターミナルエミュレーション（VT100–VT500）
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty)：ローカルシェル、Windows では ConPTY
- [Tokio](https://tokio.rs)：非同期ランタイム
- [Wasmtime](https://wasmtime.dev/)：WASM プラグイン実行
- [tree-sitter](https://tree-sitter.github.io/)：エディターの構文強調

---

## ライセンス

OxideTerm は追加制限のない [GPL-3.0-only](../../LICENSE) です。依存関係の告知は [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) と [NOTICE](../../NOTICE) にあります。

所有する、またはアクセスを許可されたシステムでのみ使用してください。法的告知の全文は [LEGAL.md](../../LEGAL.md) にあります。

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
