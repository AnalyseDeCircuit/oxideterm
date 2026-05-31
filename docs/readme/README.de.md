<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <em>Wenn du einen local-first SSH-Arbeitsbereich ohne Electron, WebView, Telemetrie oder Abos willst, gib OxideTerm einen Star, damit mehr SSH-Nutzer es finden.</em>
</p>

<p align="center">
  <strong>Local-first SSH-Arbeitsbereich: Shell, SFTP, Port-Forwarding, trzsz, Remote-Editing und BYOK-KI rund um einen Remote-Knoten.</strong>
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust — bis ganz nach unten.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plattform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Lizenz">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Native Rust-Neufassung von <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — GPU-gerendert, zero-WebView, mit <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (Zeds Rendering-Framework)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

---

## Warum OxideTerm Native?

| Wenn dir wichtig ist... | OxideTerm Native bietet... |
|---|---|
| SSH-Arbeitsbereich statt nur Shell | Terminal, SFTP, Forwarding, trzsz, Mini-IDE, Monitoring und KI-Kontext um einen Knoten |
| Lokale Shells, serielle Konsolen und Remote-SSH | zsh/bash/fish/pwsh/WSL2, lokale serielle Terminals und SSH im selben Workflow |
| Kein Cloud-Konto | SSH, SFTP, Forwarding, lokale Shell und Config bleiben local-first |
| BYOK-KI | Eigene OpenAI-, Anthropic-, Gemini-, Ollama- oder kompatible Endpunkte |
| Kein WebView | GPUI rendert direkt auf eine GPU-Surface, ohne DOM, CSS oder JavaScript |
| Keine Serialisierung im Hot Path | Terminal-Bytes verändern Rust-State direkt, ohne WebSocket/JSON/Base64 |
| Kein OpenSSL | Reines Rust-SSH mit `russh` + `ring` |
| Stabile Reconnects | Grace Period prüft die alte Verbindung, bevor TUI-Apps beendet werden |
| Remote-Dateiarbeit | Integriertes SFTP und native IDE zum Browsen, Vorschauen, Übertragen und Bearbeiten |
| Credential-Sicherheit | OS-Keychain; `.oxide` mit ChaCha20-Poly1305 + Argon2id |

## Was es ist / was es nicht ist

OxideTerm Native ist ein **nativer Desktop-SSH-Arbeitsbereich in reinem Rust**. Terminal, SFTP, Forwarding, Editing, KI, Cloud Sync, Plugins und CLI aus der Tauri-Version werden in Rust mit GPUI-Oberfläche neu implementiert.

Es ist keine Electron-App, keine Tauri-App, kein Web-Terminal und kein gehosteter Service. Kein Chromium, kein WebView, kein JavaScript, kein CSS.

## Unterschiede zu WebView/Tauri

| Aspekt | WebView/Tauri | Native |
|---|---|---|
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI, GPU-Surface, Immediate Mode, Rust |
| Terminaldaten | WebSocket → JS Event Loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC pro Kommando | In-process Funktionsaufrufe |
| SSH keepalive | JavaScript Timer | Rust async task |
| Plugins | ESM im Browser-Sandbox | wasmtime WASM + typed Rust host API |
| CLI | Desktop-App muss laufen | Eigenständiges Binary |
| Release-Artefaktgröße | Meist ca. 150–200 MB Installer | Aktuell macOS arm64: ca. 50–60 MB komprimiertes Portable/DMG; rohes Release-Binary ca. 132 MB |

## Funktionsübersicht

| Kategorie | Funktionen |
|---|---|
| Terminal | Local PTY, SSH, local serial terminals, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | Connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | Dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| KI | OxideSens mit OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG, command approval |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, verschlüsselter Import/Export |
| Plugins / CLI | WASM-Sandbox, native host API, Plugin-Einstellungen; CLI für settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Architektur

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

Zwischen UI und SSH/Terminal-Backend gibt es keine Serialisierungsgrenze. Terminal-Bytes mutieren `TerminalState` direkt; GPUI liest den State und erzeugt GPU draw calls.

## Schnellstart

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

## Sicherheit

| Thema | Umsetzung |
|---|---|
| Passwörter & Schlüssel | macOS Keychain / Windows Credential Manager / libsecret |
| Secrets im Speicher | `zeroize` / `Zeroizing` |
| Diagnosen & KI-Kontext | Secret-Werte werden vor Ausgabe oder Provider-Aufrufen redigiert |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI-Schreibzugriffe | dry-run plans, `--yes` guards, rollback backups |
| Plugins | wasmtime isolation und capability-based host API |

## Roadmap / Beiträge

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] In-process Terminaldatenfluss ohne WebSocket
- [x] SFTP, Forwarding, IDE, KI, Cloud Sync, Plugins, CLI
- [ ] Full ProxyCommand, audit logging, packaged release builds

## Provider-Neutralität

OxideTerm ist BYOK-first und provider-neutral.

Provider-Integrationen sollen Nutzern helfen, die Werkzeuge zu verbinden, denen sie bereits vertrauen. Sie sind keine Rangliste, keine Werbefläche und kein Belohnungssystem für diejenigen, die am freundlichsten nach Aufmerksamkeit fragen.

Kompatibilität, Wartbarkeit, Sicherheit und echter Nutzwert entscheiden, was dokumentiert wird. Sichtbarkeit folgt Nützlichkeit, nicht Begeisterung.

Wenn ein Feature bereits in der Tauri-Version existiert, sollen Verhalten, Labels, Interaktionszustände und Workflows übereinstimmen. Neue Crates brauchen echte Domänenverantwortung und dürfen nicht nur Re-Exports sammeln.

## Lizenz / Danksagung

**GPL-3.0-only**. Third-party notices stehen in `NOTICE`. Danke an `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` und `tree-sitter`.
