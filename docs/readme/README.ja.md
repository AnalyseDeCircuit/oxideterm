<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>OxideTerm のワークフローを WebView なしで使いたい人のためのローカルファースト SSH ワークスペース。</strong>
  <br>
  リモートマシンへ一度接続すれば、shell、ファイル、ポート、転送、軽量エディタ、シリアルコンソール、BYOK AI を 1 つのネイティブ Rust ワークスペースで扱えます。
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. 徹底して Pure Rust。</strong>
</p>

<p align="center">
  <em>Electron、WebView、テレメトリ、サブスクリプションなしのローカルファースト SSH ワークスペースが必要なら、OxideTerm に Star を付けて、より多くの SSH ユーザーに届けてください。</em>
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

---

> **リリース状況:** OxideTerm Native は OxideTerm の次期メジャーリリースとして準備中です。公開インストーラはまだ提供されていないため、現時点ではソースから実行してください。native インストーラが準備できるまで、現在のパッケージ版リリースは Tauri 系列のままです。

## なぜ Native 版か

| 重視すること | OxideTerm Native が提供するもの |
|---|---|
| 単なる shell ではなく SSH ワークスペース | terminal、SFTP、forwarding、trzsz、軽量 IDE、monitoring、AI context を 1 つのノードに統合 |
| ローカル shell、シリアルコンソール、リモート SSH | zsh/bash/fish/pwsh/WSL2、ローカルシリアル端末、SSH を同じワークフローで扱える |
| クラウドアカウント不要 | SSH、SFTP、forwarding、local shell、設定はローカルファースト |
| BYOK AI | OpenAI、Anthropic、Gemini、Ollama、OpenAI-compatible endpoint を利用 |
| WebView なし | DOM/CSS/JavaScript なしで GPUI が GPU surface に直接描画 |
| ホットパスでのシリアライズなし | terminal bytes が Rust state を直接更新し、WebSocket/JSON/Base64 を通らない |
| OpenSSL 依存なし | `russh` + `ring` による純 Rust SSH |
| 安定した再接続 | Grace Period が旧接続を先に probe し、ネットワーク揺れで TUI を守る |
| リモートファイル作業 | 内蔵 SFTP と native IDE で browse、preview、transfer、edit |
| 認証情報の安全性 | OS keychain と `.oxide` の ChaCha20-Poly1305 + Argon2id 暗号化 |

## これは何か / 何ではないか

OxideTerm Native は**純 Rust のネイティブデスクトップ SSH ワークスペース**であり、OxideTerm の次期メジャー edition です。Terminal、SFTP、forwarding、editing、AI、cloud sync、plugins、CLI を 1 つのリモートノード中心にまとめる同じ製品思想を、browser runtime のない GPUI UI layer へ移します。

Electron、Tauri、Web terminal、hosted service ではありません。Chromium、WebView、JavaScript、CSS はなく、すべての UI は GPUI が GPU surface に直接描画します。

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

## License / Acknowledgments

**GPL-3.0-only**. Third-party notices are recorded in `NOTICE`. Thanks to `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime`, and `tree-sitter`.
