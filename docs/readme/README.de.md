<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>KI-gestützter nativer Betriebsarbeitsbereich für Remote-Server — native App aus reinem Rust</strong>
  <br>
  SSH, Telnet, serielle Terminals, RDP/VNC, SFTP, Portweiterleitung und leichtes Editieren in einem nativen Arbeitsbereich.
  <br>
  GPU-gerendert. Kostenlos. Kein Konto nötig.
  <br>
  <strong>Kein Electron. Kein gebündeltes WebView. Keine Telemetrie. Kein Abo. BYOK zuerst. Reines Rust-SSH ohne OpenSSL/libssh2.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.3-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plattform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Lizenz">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Open Source, lokal orientiert und GPU-gerendert mit GPUI.</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm Funktionsübersicht" width="920">
</p>

---

## Was OxideTerm ist

OxideTerm ist ein Open-Source-Arbeitsbereich für SSH und Remote-Betrieb. Terminal, Dateien, Portweiterleitungen, Host-Werkzeuge und Remote-Desktops bleiben in einem gemeinsamen Arbeitsbereich.

**Was Sie tun können:**

- SSH, Telnet, serielle Verbindungen, RDP/VNC, SFTP, Portweiterleitungen, lokale Shells und leichtes Editieren in einem Arbeitsbereich verwalten
- Remote-Arbeit mit Grace-Period-Wiederverbindung über kurze Netzwerkausfälle hinweg aufrechterhalten
- OxideSens aktive Sitzungen prüfen und freigegebene Arbeitsbereichsaktionen über Ihren eigenen KI-Anbieter ausführen lassen

Verbindungen und Betriebsdaten bleiben unter Ihrer Kontrolle. Für OxideSens verwenden Sie Ihren eigenen KI-Anbieter; ein Konto ist nicht erforderlich.

---

## Warum OxideTerm?

- SSH, Telnet, seriell, RDP/VNC, SFTP, Portweiterleitungen und lokale Shells in einer Desktop-App
- Grace-Period-Wiederverbindung für kurze Netzwerkausfälle
- OxideSens mit eigenen KI-Zugangsdaten und freigegebenen Arbeitsbereichsaktionen
- GPUI-Oberfläche ohne Electron oder gebündelte Browser-Runtime

---

## Screenshots

Die folgenden Screenshots zeigen Terminal-, Datei-, Editor- und Weiterleitungsabläufe in OxideTerm.

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

## Für Remote-Betrieb entwickelt

OxideTerm hält Verbindungen, Dateien, Weiterleitungen, Host-Werkzeuge, Automatisierung und KI-Kontext in einem Rust-Arbeitsbereich. Werkzeuge teilen dieselbe Serveridentität und denselben Sitzungslebenszyklus.

---

## Funktionen

| Kategorie | Funktionen |
|---|---|
| **Terminal und Verbindungen** | Lokale Shells, SSH, Telnet, seriell, geteilte Bereiche, Multi-Hop-Routen und stabile Wiederverbindung |
| **Dateien und Remote-Bearbeitung** | SFTP, Übertragungswarteschlangen, Lesezeichen, sichere Schreibvorgänge, Projektbäume und Mehrfachbearbeitung |
| **Weiterleitung und Netzwerk** | Lokale, entfernte und dynamische SOCKS5-Weiterleitung, gespeicherte Regeln und Socket-Debugging |
| **Host-Betrieb und Remote-Desktop** | Überwachung, Prozesse, Dienste, Logs, Ports, Aufgaben, Datenträger, Pakete, Container, tmux, RDP und VNC |
| **OxideSens und Automatisierung** | Eigene KI-Anbieter, MCP, lokales RAG, freigegebene Aktionen, verschlüsselte Cloud-Synchronisierung und CLI |
| **Erweiterungen und Personalisierung** | WASM-Plugins, eigene Tabs, Schnellbefehle, Themes, Hintergrundbilder, Tastenkürzel und 11 Sprachen |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens öffnet ein Terminal in OxideTerm" width="920">
</a>

*OxideSens folgt einer Nutzeranfrage und öffnet ein Terminal in OxideTerm.*

</div>

---

## Architektur

OxideTerm vereint Terminal, SSH, Telnet, RDP, VNC, SFTP, Forwarding, IDE, KI, Plugins und CLI in einer Rust-Architektur. Die technischen Details folgen unten.

<details>
<summary><strong>Architektur, SSH-Internals, GPUI-Shell, Reconnect, KI, Plugins und mehr</strong></summary>
<br>

### Architektur — Kern in einem Prozess, keine WebView-Bridge

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


- **Kein OpenSSL/libssh2 im SSH-Stack** — die SSH-Kryptografie wird von `ring` bereitgestellt
- Vollständiges SSH2: Key Exchange, Channels, SFTP-Subsystem, Portweiterleitung
- ChaCha20-Poly1305 / AES-GCM, Ed25519/RSA/ECDSA-Schlüssel
- SSH Agent unter Unix (`SSH_AUTH_SOCK`) und Windows (`\\.\pipe\openssh-ssh-agent`)
- Mehrstufiges ProxyJump mit unabhängiger Authentifizierung pro Hop

### Smart Reconnect mit Grace Period


1. SSH-keepalive timeout erkennen, ohne JavaScript timer throttling
2. Terminal-Panes, SFTP-Transfers, Forwards und IDE-Dateien snapshotten
3. Die alte Verbindung 30 Sekunden lang während der Grace Period prüfen, damit TUI-Apps Netzwerkwechsel überstehen können
4. Wenn die Wiederherstellung scheitert: neu verbinden, Forwards wiederherstellen, Transfers fortsetzen und IDE-Dateien erneut öffnen

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### SSH-Verbindungspool und Node-Routing


- Eine physische SSH-Verbindung kann Terminal-Panes, SFTP, Port-Forwards und IDE-Arbeit bedienen
- Jede Verbindung durchläuft `connecting → active → idle → link_down → reconnecting`
- UI-Kommandos adressieren `nodeId`; `NodeRouter` löst die aktive `connectionId` atomar auf
- `NodeRuntimeStore` persistiert Topologie-Snapshots in `session_tree.json`
- Jump-Host-Ausfälle propagieren `link_down` auf nachgelagerte Nodes

### OxideSens KI

OxideSens bleibt BYOK zuerst, mit Kontextaufbau direkt im Prozess:

- Anbieter: OpenAI, Anthropic, Gemini, Ollama oder jeder OpenAI-kompatible Endpunkt
- MCP: stdio- und SSE-Transports, Tool Discovery und Invocation
- RAG: BM25-Volltext, HNSW-Vektorindex, Reciprocal Rank Fusion, CJK-Bigram-Tokenizer
- Nachrichten an Anbieter durchlaufen eine Redigierung für Zugangsdatenmuster; Arbeitsbereichskontext und Aktionen bleiben unter Nutzerkontrolle
- API-Schlüssel liegen im Systemschlüsselbund und werden bewusst aus strukturierten Logs und Nachrichten des Desktop-Kerns ausgeschlossen

### GPUI Desktop-Shell

Die UI wird direkt mit GPUI gezeichnet, ohne DOM/CSS/JavaScript-Rendering-Pipeline:

- Workspace-Tab-Typen: lokale, SSH-, Telnet-, serielle, RDP-, VNC-, SFTP, IDE, Forwards, Settings, Plugins, Topology und mehr
- Binärer Pane-Tree mit ziehbaren Dividern, bis zu vier Panes pro Terminal-Tab
- Command Palette, globale Tastenkürzel und Sidebars bestehen aus GPUI-Primitives
- Immediate-mode Rendering reagiert auf Rust-State ohne Serialisierungs-Roundtrip

### Terminalzustand und Rendering

Terminal-Rendering wird zuerst als Rust-State modelliert und anschließend von GPUI gezeichnet:

- PTY-Ausgabe landet in `TerminalState`; Scrollback, Cursor, Auswahl, Marks und Suchzustand bleiben in Rust
- Die Rendering Policy kann zwischen Boost, Normal und Idle wechseln, ohne auf einen Browser Event Loop zu warten
- Sixel- und Kitty-Grafiken werden als terminal-eigene Assets verfolgt, nicht als DOM-Nodes oder Canvas-Overlays
- Split Panes teilen dasselbe Arbeitsbereichsstatus-Modell, sodass Tab-Restore und Reconnect die Terminal-Topologie gemeinsam snapshotten können

### SFTP- und IDE-Workspace

Remote-Dateien sind Teil desselben Node-Workspace und keine getrennte Nebenfunktion:

- SFTP-Sessions werden über `NodeRouter` aufgelöst, sodass Reconnect die darunterliegende SSH-Verbindung tauschen kann, ohne die Node-Adresse der UI zu ändern
- Transfer Queues verfolgen Richtung, Fortschritt, Retry-Zustand und Speed Limits unabhängig von den sichtbaren Datei-Panes
- IDE-Tabs halten Dirty Buffers, Remote-Pfade, Conflict State und Restore-Metadaten zusammen
- Remote Writes nutzen staged/atomic behavior, wo das Backend es unterstützt, damit normale Edit-Flows keine Partial Writes sehen

### Plugins, CLI und Diagnosen

Erweiterungen und Support-Flächen folgen klaren Rust-eigenen Grenzen:

- Plugins laufen in einer wasmtime-Sandbox mit typisierten Host-Fähigkeiten statt Browser-Globals
- Die CLI linkt direkt gegen Domain Crates für doctor, settings, connections, forwards, portable bundles, backups und reports
- Diagnosen bevorzugen Zähler, Pfade, Feature-Flags und redigierte Hinweise statt roher payloads mit Geheimnisse
- Mutierende CLI-Flows nutzen dry-run plans, `--yes` guards und rollback backups, wo anwendbar

### Portweiterleitung — Lock-Free I/O


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


- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, erhöht die Kosten für GPU-Bruteforce
- Enthält connections, forwards, settings, quick commands, plugin settings und portable secrets

</details>

---

## Aus dem Quellcode starten

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

## Technik

| Ebene | Technologie | Hinweise |
|---|---|---|
| Benutzeroberfläche | GPUI (Zed) | GPU-beschleunigter Immediate Mode, vollständig in Rust |
| Laufzeit | Tokio + DashMap | Asynchrone Laufzeit und nebenläufige Maps |
| SSH | russh (`ring`) | Kein OpenSSL/libssh2 im SSH-Stack; SSH Agent |
| Terminal | portable-pty + alacritty_terminal | Lokale PTYs, Terminalemulation, Sixel- und Kitty-Grafik |
| Plugins | wasmtime | WASM-Isolation mit nativer Host-API |
| KI und Suche | SSE + BM25 + HNSW | Anbieter-Streaming, CJK-Bigramme und RRF-Fusion |

## Sicherheit

| Thema | Umsetzung |
|---|---|
| Gespeicherte Zugangsdaten | macOS Keychain / Windows Credential Manager / libsecret |
| Geheimnisse im Speicher | Geheimnistragende Typen und temporäre Puffer verwenden an unterstützten Besitzgrenzen `zeroize` / `Zeroizing` |
| Diagnosen | Support-Ausgaben bevorzugen strukturierte Metadaten und redigierte Hinweise statt geheimnistragender Nutzdaten |
| KI-Kontext | Nachrichten an Anbieter durchlaufen eine Redigierung für Zugangsdatenmuster; Workspace-Kontext und Aktionen bleiben unter Nutzerkontrolle |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| CLI-Schreibzugriffe | dry-run plans, `--yes` guards, rollback backups |
| Plugins | wasmtime-Isolation und fähigkeitsbasierte Host-API |

## Hinweis zur rechtmäßigen Nutzung

OxideTerm ist unter GPL-3.0-only ohne zusätzliche Lizenzbeschränkungen lizenziert. Greifen Sie bei der Nutzung nur auf Systeme, Netzwerke und Geräte zu, die Ihnen gehören oder für die Sie eine ausdrückliche Zugriffsberechtigung besitzen, und beachten Sie das geltende Recht. Verwenden Sie OxideTerm nicht für unbefugte Zugriffe, Dienststörungen oder zur Umgehung von Zugriffskontrollen.

## Beiträge

Beiträge zu Code, Dokumentation, Übersetzungen, Plugins, Tests und Fehlerberichten sind willkommen. Größere Änderungen sollten zuerst in einem Issue abgestimmt werden; klar begrenzte Korrekturen können direkt als Pull Request eingereicht werden.

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## Support und Wartung

Reproduzierbare Bug Reports und Regressionen mit redigierten Diagnosen werden priorisiert. Feature Requests werden nach Umfang, Sicherheit und Ausrichtung auf OxideTerms Remote-Server-Workspace-Richtung bewertet.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Wenn OxideTerm Ihrem Workflow hilft, machen GitHub Star, Reproduktion, Übersetzungskorrektur, Plugin oder Pull Request das Projekt leichter weiterzuführen.

---

## Lizenz

**GPL-3.0-only**. Ausführliche Hinweise zu Drittanbietern stehen in [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md); zusätzliche Hinweise enthält [`NOTICE`](../../NOTICE).

## Danksagung

Danke an `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` und `tree-sitter`.
