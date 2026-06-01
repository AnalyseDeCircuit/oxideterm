<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>OxideTerm の次期ゼロ WebView edition。</strong>
  <br>
  リモートマシンへ一度接続すれば、shell、ファイル、ポート、転送、軽量エディタ、シリアルコンソール、BYOK AI を 1 つのネイティブ Rust ワークスペースで扱えます。
  <br>
  ネイティブ GPUI アプリ · 純粋な Rust SSH · コア SSH ワークフローにアカウント不要
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. 徹底して Pure Rust。</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version">
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

> **リリース状況:** OxideTerm Native は OxideTerm の次期メジャーリリースとして準備中です。公開インストーラはまだ提供されていないため、現時点ではソースから実行してください。native インストーラが準備できるまで、現在のパッケージ版リリースは Tauri 系列のままです。

## できること

- SSH terminal、SFTP、port forwarding、serial console、local shell、軽量編集を 1 つの native workspace で管理
- Grace Period reconnect により、ネットワークが揺れてもリモート作業を維持
- 自分の AI provider で live session を確認し、承認済みの workspace action を実行

---

## なぜ Native 版か

| 重視すること | OxideTerm Native が提供するもの |
|---|---|
| 1 つの remote node、多数のツール | Terminal、SFTP、port forwarding、trzsz、native IDE、monitoring、AI context が同じ SSH workspace に結び付きます |
| ゼロ WebView の native shell | GPUI が GPU surface に desktop UI を直接描画し、DOM、CSS、JavaScript、Chromium、WebKit runtime はありません |
| Local-first SSH workflow | SSH、SFTP、forwarding、local shell、serial terminals、設定管理はサインアップ不要です |
| Platform credit ではなく BYOK AI | OxideSens は OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible endpoint を使い、MCP と RAG に対応します |
| 再接続の安定性 | Grace Period が旧接続を 30 秒 probe してから置き換えるため、短いネットワーク断でも TUI が生き残れます |
| 純粋な Rust SSH と認証情報の安全性 | `russh` + `ring`、OpenSSL/libssh2 なし。パスワードと API key は OS keychain、`.oxide` は ChaCha20-Poly1305 + Argon2id |

## これは何か / 何ではないか

OxideTerm Native は OxideTerm と同じ **local-first SSH workspace** に集中し、それを pure Rust GPUI desktop app として作り直したものです。Terminal、file、port、transfer、軽量編集、serial console、AI context を自分の machine と remote node 中心に扱いたいユーザー向けです。

これはまだ現在の stable download line ではなく、hosted cloud agent platform でもありません。また Electron、Tauri、web terminal でもありません。Chromium、WebView、JavaScript、CSS はありません。

---

## スクリーンショット

Native UI は現在の Tauri line と同じ OxideTerm workspace model と visual language を踏襲します。

<table>
<tr>
<td align="center"><strong>SSH ターミナル + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="OxideSens AI サイドバー付き SSH ターミナル" /></td>
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

</details>

---

## ソースから実行

公開 native インストーラはまだ提供されていません。パッケージ版 build が準備できるまでは、native edition をソースから実行してください。

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
- [ ] 公開パッケージ版インストーラ
- [ ] Full ProxyCommand、audit logging

## Contributing

## Provider Neutrality

OxideTerm は BYOK-first であり、provider-neutral です。

Provider integration は、ユーザーがすでに信頼しているツールへ接続するためのものです。ランキング、広告枠、あるいは最も熱心に声をかけてきた相手への報酬ではありません。

何をドキュメントに載せるかは、compatibility、maintainability、security、そして実際の user value で決まります。Visibility は usefulness に従うもので、enthusiasm に従うものではありません。

Tauri 版に存在する機能を移植する場合は、明示的な置き換えがない限り、挙動、ラベル、interaction state、workflow を合わせてください。新しい crate は re-export だけではなく、明確な責務を持つ必要があります。

## サポートとメンテナンス

OxideTerm Native は OxideTerm の次期 major release として準備中で、best-effort でメンテナンスされています。再現手順と redacted diagnostics を含む bug report を優先します。feature request は常に実装されるとは限りません。

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

OxideTerm があなたの workflow に役立つなら、GitHub star、issue reproduction、translation fix、plugin、pull request がプロジェクトの継続を助けます。

---

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
