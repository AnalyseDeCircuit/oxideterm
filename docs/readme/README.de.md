<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>AI-native Workspace für Remote-Server.</strong>
  <br>
  Verbinden Sie sich per SSH mit Ihren Servern und arbeiten Sie dann mit Terminals, Dateien, Ports, Transfers, schlanker Bearbeitung, seriellen Konsolen und der OxideSens AI in einer local-first nativen App.
  <br>
  Native GPUI-App · reines Rust-SSH · BYOK OxideSens AI · kein Konto für zentrale SSH-Workflows erforderlich
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust — bis ganz nach unten.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.3-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plattform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Lizenz">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Nächste große native Edition von <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — GPU-gerendert, zero-WebView, mit <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (Zeds Rendering-Framework)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens öffnet ein Terminal in OxideTerm" width="920">
</a>

*OxideSens folgt einer Nutzeranfrage und öffnet ein Terminal in OxideTerm.*

</div>

---

## Was Sie damit tun können

- SSH-Terminals, SFTP, Portweiterleitungen, serielle Konsolen, lokale Shells und leichtes Editieren in einem nativen Workspace verwalten
- Remote-Arbeit mit Grace-Period-Reconnect bei kurzen Netzwerkaussetzern am Leben halten
- OxideSens AI kann mit Ihrem eigenen KI-Anbieter Live-Sessions prüfen und genehmigte Workspace-Aktionen ausführen

---

## Warum OxideTerm Native?

| Wenn Ihnen wichtig ist... | OxideTerm Native bietet... |
|---|---|
| Ein Remote-Node, viele Werkzeuge | Terminal, SFTP, Portweiterleitung, trzsz, native IDE, Monitoring und OxideSens AI bleiben am selben SSH-Workspace |
| Zero-WebView native Shell | GPUI zeichnet die Desktop-UI direkt auf eine GPU-Surface — ohne DOM, CSS, JavaScript, Chromium oder WebKit-Runtime |
| Local-first SSH-Workflows | SSH, SFTP, Forwarding, lokale Shell, serielle Terminals und Konfiguration funktionieren ohne Registrierung |
| Autonome OxideSens AI statt Plattform-Credits | OxideSens nutzt Ihren OpenAI/Anthropic/Gemini/Ollama/OpenAI-kompatiblen Endpoint mit MCP, RAG und genehmigten Workspace-Aktionen |
| Stabile Wiederverbindung | Grace Period prüft die alte Verbindung 30 s lang, bevor sie ersetzt wird, damit TUI-Apps kurze Aussetzer überstehen können |
| Reines Rust-SSH und sichere Zugangsdaten | `russh` + `ring`, kein OpenSSL/libssh2; Passwörter und API-Schlüssel bleiben im OS-Keychain, `.oxide` nutzt ChaCha20-Poly1305 + Argon2id |

## Was es ist / was es nicht ist

OxideTerm Native konzentriert sich auf einen **local-first KI-Workspace für Remote-Server**, neu aufgebaut als pure Rust GPUI desktop app. Es richtet sich an Nutzer, die Terminal, Dateien, Ports, Transfers, leichtes Editieren, serielle Konsolen und eine OxideSens AI um ihre eigenen Maschinen und Remote-Nodes herum halten wollen.

Es ist noch nicht die aktuelle stabile Download-Linie und keine gehostete Cloud-Agent-Plattform. Es ist auch keine Electron-App, keine Tauri-App und kein Web-Terminal: kein Chromium, kein WebView, kein JavaScript, kein CSS.

---

## Screenshots

Die native UI folgt demselben OxideTerm-Workspace-Modell und derselben visuellen Sprache wie die aktuelle Tauri-Linie.

<table>
<tr>
<td align="center"><strong>SSH-Terminal + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="SSH-Terminal mit OxideSens AI" /></td>
<td align="center"><strong>SFTP-Dateimanager</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="SFTP Dual-Pane-Dateimanager mit Transfer-Warteschlange" /></td>
</tr>
<tr>
<td align="center"><strong>Integrierte IDE</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Integrierter IDE-Modus" /></td>
<td align="center"><strong>Intelligente Portweiterleitung</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Intelligente Portweiterleitung mit Auto-Erkennung" /></td>
</tr>
</table>

---

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

OxideTerm Native entfernt die WebView-Bridge und hält Terminal, SSH, SFTP, Forwarding, IDE, KI, Plugins und CLI in einer Rust-nativen Architektur. Die vollständigen Implementierungsdetails bleiben unten erhalten.

<details>
<summary><strong>Architektur, SSH-Internals, GPUI-Shell, Reconnect, KI, Plugins und mehr</strong></summary>
<br>

### Architektur — Single-Process, Zero-Bridge

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

### Reines Rust-SSH — russh (ring)

Die native Edition linkt denselben `russh`-Stack wie die Tauri-Linie direkt in das Desktop-Binary:

- **Keine C/OpenSSL-Abhängigkeiten** durch `ring`
- Vollständiges SSH2: Key Exchange, Channels, SFTP-Subsystem, Portweiterleitung
- ChaCha20-Poly1305 / AES-GCM, Ed25519/RSA/ECDSA-Schlüssel
- SSH Agent unter Unix (`SSH_AUTH_SOCK`) und Windows (`\\.\pipe\openssh-ssh-agent`)
- Mehrstufiges ProxyJump mit unabhängiger Authentifizierung pro Hop

### Smart Reconnect mit Grace Period

Die Reconnect-Semantik entspricht der Tauri-Linie, aber die Orchestrierung läuft vollständig in Rust-async-Tasks:

1. SSH-keepalive timeout erkennen, ohne JavaScript timer throttling
2. Terminal-Panes, SFTP-Transfers, Forwards und IDE-Dateien snapshotten
3. Die alte Verbindung 30 Sekunden lang während der Grace Period prüfen, damit TUI-Apps Netzwerkwechsel überstehen können
4. Wenn die Wiederherstellung scheitert: neu verbinden, Forwards wiederherstellen, Transfers fortsetzen und IDE-Dateien erneut öffnen

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH-Verbindungspool und Node-Routing

`SshConnectionRegistry` basiert auf `DashMap` und behält das node-first Modell der Tauri-Version ohne WebSocket-Lifecycle-Bridge bei:

- Eine physische SSH-Verbindung kann Terminal-Panes, SFTP, Port-Forwards und IDE-Arbeit bedienen
- Jede Verbindung durchläuft `connecting → active → idle → link_down → reconnecting`
- UI-Kommandos adressieren `nodeId`; `NodeRouter` löst die aktive `connectionId` atomar auf
- `NodeRuntimeStore` persistiert Topologie-Snapshots in `session_tree.json`
- Jump-Host-Ausfälle propagieren `link_down` auf nachgelagerte Nodes

### OxideSens KI

OxideSens bleibt BYOK-first, mit Kontextaufbau direkt im Prozess:

- Provider: OpenAI, Anthropic, Gemini, Ollama oder jeder OpenAI-compatible Endpoint
- MCP: stdio- und SSE-Transports, Tool Discovery und Invocation
- RAG: BM25-Volltext, HNSW-Vektorindex, Reciprocal Rank Fusion, CJK-Bigram-Tokenizer
- KI-Kontext stammt aus dem Workspace-State; Zugangsdaten werden vor Provider-Aufrufen redigiert
- API-Schlüssel bleiben im OS-Keychain und gelangen nicht in Logs oder IPC-Frames

### GPUI Desktop-Shell

Die UI wird direkt mit GPUI gezeichnet, ohne DOM/CSS/JavaScript-Rendering-Pipeline:

- 17 Workspace-Tab-Typen: lokale/SSH-Terminals, SFTP, IDE, Forwards, Settings, Plugin, Topology und mehr
- Binärer Pane-Tree mit ziehbaren Dividern, bis zu vier Panes pro Terminal-Tab
- Command Palette, globale Tastenkürzel und Sidebars bestehen aus GPUI-Primitives
- Immediate-mode Rendering reagiert auf Rust-State ohne Serialisierungs-Roundtrip

### Terminalzustand und Rendering

Terminal-Rendering wird zuerst als Rust-State modelliert und anschließend von GPUI gezeichnet:

- PTY-Ausgabe landet in `TerminalState`; Scrollback, Cursor, Auswahl, Marks und Suchzustand bleiben in Rust
- Die Rendering Policy kann zwischen Boost, Normal und Idle wechseln, ohne auf einen Browser Event Loop zu warten
- Sixel- und Kitty-Grafiken werden als terminal-eigene Assets verfolgt, nicht als DOM-Nodes oder Canvas-Overlays
- Split Panes teilen dasselbe Workspace-State-Modell, sodass Tab-Restore und Reconnect die Terminal-Topologie gemeinsam snapshotten können

### SFTP- und IDE-Workspace

Remote-Dateien sind Teil desselben Node-Workspace und keine getrennte Nebenfunktion:

- SFTP-Sessions werden über `NodeRouter` aufgelöst, sodass Reconnect die darunterliegende SSH-Verbindung tauschen kann, ohne die Node-Adresse der UI zu ändern
- Transfer Queues verfolgen Richtung, Fortschritt, Retry-Zustand und Speed Limits unabhängig von den sichtbaren Datei-Panes
- IDE-Tabs halten Dirty Buffers, Remote-Pfade, Conflict State und Restore-Metadaten zusammen
- Remote Writes nutzen staged/atomic behavior, wo das Backend es unterstützt, damit normale Edit-Flows keine Partial Writes sehen

### Plugins, CLI und Diagnosen

Die native Branch hält Erweiterungen und Support-Flächen innerhalb Rust-nativer Grenzen:

- Plugins laufen in einer wasmtime Sandbox mit typed host capabilities statt Browser-Globals
- Die CLI linkt direkt gegen Domain Crates für doctor, settings, connections, forwards, portable bundles, backups und reports
- Diagnosen bevorzugen Counts, Paths, Feature Flags und redacted Hints statt roher payloads mit Secrets
- Mutierende CLI-Flows nutzen dry-run plans, `--yes` guards und rollback backups, wo anwendbar

### Portweiterleitung — Lock-Free I/O

Forwarding behält die Tauri-Semantik in einer eigenständigen Rust-Crate:

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Ein einzelner `ssh_io`-Task besitzt jeden SSH Channel und vermeidet `Arc<Mutex<Channel>>`
- Reconnect Auto-Restore, Death Reporting und Idle Timeout

### trzsz — In-Band-Dateitransfer

trzsz nutzt weiterhin den Terminal-Stream, ohne zusätzlichen Port oder Remote-Agent:

- Upload/download über den bestehenden Terminal-Stream
- Funktioniert durch ProxyJump-Ketten
- Native Dateiauswahl vermeidet Browser-Speichergrenzen
- Bidirektional, Verzeichnis-Support, konfigurierbare Limits

### `.oxide` verschlüsselter Export

Das verschlüsselte Bundle-Format entspricht der Tauri-Linie:

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, erhöht die Kosten für GPU-Bruteforce
- Enthält connections, forwards, settings, quick commands, plugin settings und portable secrets

</details>

---

## Aus dem Quellcode starten

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

## Release-Status

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] In-process Terminaldatenfluss ohne WebSocket
- [x] SFTP, Forwarding, IDE, KI, Cloud Sync, Plugins, CLI
- [x] Lokale serielle Terminals
- [x] Full ProxyCommand
- [ ] Audit logging

## Beiträge

## Provider-Neutralität

OxideTerm ist BYOK-first und provider-neutral.

Provider-Integrationen sollen Nutzern helfen, die Werkzeuge zu verbinden, denen sie bereits vertrauen. Sie sind keine Rangliste, keine Werbefläche und kein Belohnungssystem für diejenigen, die am freundlichsten nach Aufmerksamkeit fragen.

Kompatibilität, Wartbarkeit, Sicherheit und echter Nutzwert entscheiden, was dokumentiert wird. Sichtbarkeit folgt Nützlichkeit, nicht Begeisterung.

Wenn ein Feature bereits in der Tauri-Version existiert, sollen Verhalten, Labels, Interaktionszustände und Workflows übereinstimmen. Neue Crates brauchen echte Domänenverantwortung und dürfen nicht nur Re-Exports sammeln.

## Support und Wartung

Reproduzierbare Bug Reports und Regressionen mit redigierten Diagnosen werden priorisiert. Feature Requests werden nach Umfang, Sicherheit und Ausrichtung auf OxideTerms Remote-Server-Workspace-Richtung bewertet.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Wenn OxideTerm Ihrem Workflow hilft, machen GitHub Star, Reproduktion, Übersetzungskorrektur, Plugin oder Pull Request das Projekt leichter weiterzuführen.

---

## Lizenz / Danksagung

**GPL-3.0-only**. Third-party notices stehen in `NOTICE`. Danke an `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` und `tree-sitter`.
