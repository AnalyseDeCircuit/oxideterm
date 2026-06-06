<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>リモートサーバー向けの AI-native ワークスペース。</strong>
  <br>
  SSH でサーバーに接続し、terminal、ファイル、ポート、転送、軽量編集、serial console、OxideSens AIを local-first なネイティブアプリで扱えます。
  <br>
  ネイティブ GPUI アプリ · 純粋な Rust SSH · BYOK OxideSens AI · コア SSH ワークフローにアカウント不要
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. 徹底して Pure Rust。</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.3-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Platform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub><a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> の次期メジャー native edition — GPU レンダリング、ゼロ WebView、<a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a>（Zed のレンダリングフレームワーク）を使用</sub>
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

## できること

- SSH terminal、SFTP、port forwarding、serial console、local shell、軽量編集を 1 つの native workspace で管理
- Grace Period reconnect により、ネットワークが揺れてもリモート作業を維持
- OxideSens AIに、自分の AI provider 経由で live session の確認と承認済み workspace action の実行を任せる

---

## なぜ Native 版か

| 重視すること | OxideTerm Native が提供するもの |
|---|---|
| 1 つの remote node、多数のツール | Terminal、SFTP、port forwarding、trzsz、native IDE、monitoring、OxideSens AIが同じ SSH workspace に結び付きます |
| ゼロ WebView の native shell | GPUI が GPU surface に desktop UI を直接描画し、DOM、CSS、JavaScript、Chromium、WebKit runtime はありません |
| Local-first SSH workflow | SSH、SFTP、forwarding、local shell、serial terminals、設定管理はサインアップ不要です |
| Platform credit ではなく BYOK OxideSens AI | OxideSens は OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible endpoint を使い、MCP、RAG、承認済み workspace action に対応します |
| 再接続の安定性 | Grace Period が旧接続を 30 秒 probe してから置き換えるため、短いネットワーク断でも TUI が生き残れます |
| 純粋な Rust SSH と認証情報の安全性 | `russh` + `ring`、OpenSSL/libssh2 なし。パスワードと API key は OS keychain、`.oxide` は ChaCha20-Poly1305 + Argon2id |

## これは何か / 何ではないか

OxideTerm Native は **リモートサーバー向け local-first AI workspace** に集中し、それを pure Rust GPUI desktop app として作り直したものです。Terminal、file、port、transfer、軽量編集、serial console、OxideSens AIを自分の machine と remote node 中心に扱いたいユーザー向けです。

これはまだ現在の stable download line ではなく、hosted cloud agent platform でもありません。また Electron、Tauri、web terminal でもありません。Chromium、WebView、JavaScript、CSS はありません。

---

## スクリーンショット

Native UI は現在の Tauri line と同じ OxideTerm workspace model と visual language を踏襲します。

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
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI GPU surface、immediate mode、pure Rust |
| Terminal data flow | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | command ごとに JSON-RPC | in-process function calls |
| SSH keepalive | JavaScript timer | Rust async task |
| Plugin runtime | browser sandbox の ESM | wasmtime WASM + typed Rust host API |
| CLI | desktop app の起動が必要 | standalone binary |
| 配布アーティファクトサイズ | 通常 ~150–200 MB の installer | 現在の macOS arm64: 圧縮 portable/DMG は約 50–60 MB、未圧縮 release binary は約 132 MB |

## 機能概要

| カテゴリ | 機能 |
|---|---|
| Terminal | Local PTY、SSH、local serial terminals、split panes、shell integration、command marks、asciicast、trzsz、Sixel/Kitty graphics、rendering policy |
| SSH & Auth | connection pool、unlimited ProxyJump、Grace Period reconnect、Host-key TOFU、SSH Agent forwarding、password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser、transfer queue、preview、bookmarks、atomic writes、remote file tree、multi-tab editor、conflict resolution |
| Port Forwarding | Local、Remote、Dynamic SOCKS5、saved rules、reconnect restore、death reporting、idle timeout |
| AI | OxideSens: OpenAI、Anthropic、Gemini、Ollama/compatible、MCP、RAG、command approval |
| Cloud Sync / `.oxide` | push/pull/apply/resolve、S3/WebDAV/Git、rollback backup、encrypted import/export |
| Plugins / CLI | WASM sandbox、native host API、per-plugin settings；CLI は settings、connections、forwards、plugins、secrets、cloud-sync、backup、report など |

## 内部構造

OxideTerm Native は WebView bridge を取り除き、terminal、SSH、SFTP、forwarding、IDE、AI、plugins、CLI を 1 つの Rust-native architecture に保ちます。実装詳細は下に残しています。

<details>
<summary><strong>Architecture, SSH internals, GPUI shell, reconnect, AI, plugins など</strong></summary>
<br>

### Architecture — Single-Process, Zero-Bridge

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

UI と SSH/terminal backend の間にシリアライズ境界はありません。Terminal bytes は `TerminalState` を直接変更し、GPUI が state を読んで GPU draw call を発行します。

### 純粋な Rust SSH — russh (ring)

Native edition は Tauri line と同じ `russh` stack を desktop binary に直接 link します。

- `ring` により **C/OpenSSL 依存なし**
- 完整 SSH2: key exchange、channels、SFTP subsystem、port forwarding
- ChaCha20-Poly1305 / AES-GCM、Ed25519/RSA/ECDSA keys
- SSH Agent: Unix (`SSH_AUTH_SOCK`) と Windows (`\\.\pipe\openssh-ssh-agent`)
- hop ごとに独立認証する multi-hop ProxyJump

### Grace Period 付き Smart Reconnect

Reconnect semantics は Tauri line と同じですが、orchestration は Rust async task 内で完結します。

1. JavaScript timer throttling なしで SSH keepalive timeout を検出
2. terminal panes、SFTP transfers、forwards、IDE files を snapshot
3. Grace Period 中に旧接続を 30 秒 probe し、ネットワーク切替時も TUI apps を残せるようにする
4. 復旧できない場合は再接続し、forwards 復元、transfers 再開、IDE files 再オープン

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH Connection Pool と Node Routing

`SshConnectionRegistry` は `DashMap` backed で、WebSocket lifecycle bridge なしに Tauri の node-first model を維持します。

- 1 つの物理 SSH connection が terminal panes、SFTP、port forwards、IDE work を共有
- 各 connection は `connecting → active → idle → link_down → reconnecting` を遷移
- UI は `nodeId` で command を出し、`NodeRouter` が active `connectionId` を atomic に解決
- `NodeRuntimeStore` が topology snapshots を `session_tree.json` に永続化
- jump host failure は downstream nodes に `link_down` を cascade

### OxideSens AI

OxideSens は BYOK-first のまま、context building は in-process で行います。

- Providers: OpenAI、Anthropic、Gemini、Ollama、任意の OpenAI-compatible endpoint
- MCP: stdio / SSE transports、tool discovery、invocation
- RAG: BM25 full-text、HNSW vector index、Reciprocal Rank Fusion、CJK bigram tokenizer
- AI context は workspace state から作られ、credentials は provider call 前に redact
- API keys は OS keychain に保存され、logs や IPC frames には入りません

### GPUI Desktop Shell

UI は GPUI で直接描画され、DOM/CSS/JavaScript rendering pipeline はありません。

- 17 workspace tab types: local/SSH terminal、SFTP、IDE、Forwards、Settings、Plugin、Topology など
- draggable dividers 付き binary pane tree、terminal tab ごとに最大 4 panes
- Command palette、global key bindings、sidebars は GPUI primitives
- Immediate-mode rendering は serialization round-trip なしで Rust state に反応

### Terminal State と Rendering

Terminal rendering はまず Rust state としてモデル化され、その後 GPUI が描画します。

- PTY output は `TerminalState` に入り、scrollback、cursor、selection、marks、search state は Rust 側に保持されます
- Rendering policy は Boost、Normal、Idle の間で切り替えられ、browser event loop の協調を待ちません
- Sixel と Kitty graphics は DOM nodes や canvas overlays ではなく、terminal-owned assets として追跡されます
- Split panes は同じ workspace state model を共有し、tab restore と reconnect が terminal topology をまとめて snapshot できます

### SFTP と IDE Workspace

Remote files は分離された付属機能ではなく、同じ node workspace の一部です。

- SFTP sessions は `NodeRouter` 経由で解決され、reconnect が underlying SSH connection を差し替えても UI の node address は変わりません
- Transfer queues は visible file panes から独立して direction、progress、retry state、speed limits を追跡します
- IDE tabs は dirty buffers、remote paths、conflict state、restore metadata をまとめて保持します
- Backend が対応する場合、remote writes は staged/atomic behavior を使い、通常の edit flow に partial writes を出しにくくします

### Plugins、CLI、Diagnostics

Native branch は extension と support surfaces を Rust-native boundaries に保ちます。

- Plugins は browser globals ではなく typed host capabilities を使い、wasmtime sandbox で実行されます
- CLI は domain crates に直接 link し、doctor、settings、connections、forwards、portable bundles、backups、reports を扱います
- Diagnostics は raw secret-bearing payloads ではなく counts、paths、feature flags、redacted hints を優先します
- 状態を変更する CLI flows は dry-run plans、`--yes` guards、rollback backups を使います

### Port Forwarding — Lock-Free I/O

Forwarding は Tauri semantics を standalone Rust crate で保持します。

- Local `-L`、Remote `-R`、Dynamic SOCKS5 `-D`
- 単一の `ssh_io` task が各 SSH Channel を所有し、`Arc<Mutex<Channel>>` を避ける
- reconnect auto-restore、death reporting、idle timeout

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
- connections、forwards、settings、quick commands、plugin settings、portable secrets を含む

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
| Passwords & keys | macOS Keychain / Windows Credential Manager / libsecret |
| Secret memory | `zeroize` / `Zeroizing` |
| Diagnostics & AI context | secret values are redacted before output or provider calls |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI writes | dry-run plans、`--yes` guards、rollback backups |
| Plugins | wasmtime isolation and capability-based host API |

## リリース状況

- [x] SSH Agent forwarding、Grace Period reconnect、GPUI desktop shell
- [x] in-process terminal data flow without WebSocket
- [x] SFTP、port forwarding、IDE、AI、cloud sync、plugins、CLI
- [x] ローカルシリアル端末
- [x] Full ProxyCommand
- [ ] Audit logging

## Contributing

## Provider Neutrality

OxideTerm は BYOK-first であり、provider-neutral です。

Provider integration は、ユーザーがすでに信頼しているツールへ接続するためのものです。ランキング、広告枠、あるいは最も熱心に声をかけてきた相手への報酬ではありません。

何をドキュメントに載せるかは、compatibility、maintainability、security、そして実際の user value で決まります。Visibility は usefulness に従うもので、enthusiasm に従うものではありません。

Tauri 版に存在する機能を移植する場合は、明示的な置き換えがない限り、挙動、ラベル、interaction state、workflow を合わせてください。新しい crate は re-export だけではなく、明確な責務を持つ必要があります。

## サポートとメンテナンス

再現手順と redacted diagnostics を含む bug report と regression を優先します。feature request は範囲、安全性、OxideTerm の remote-server workspace の方向性との整合性に基づいて検討します。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

OxideTerm があなたの workflow に役立つなら、GitHub star、issue reproduction、translation fix、plugin、pull request がプロジェクトの継続を助けます。

---

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
