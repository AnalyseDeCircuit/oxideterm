<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm: Ihre Server, ein Arbeitsbereich" width="920">

# ⚡ OxideTerm

**Ein kostenloser, nativer SSH-Client und Arbeitsbereich für die Verwaltung von Remote-Systemen mit einem KI-Assistenten für Ihren eigenen API-Schlüssel.**

SSH · Mosh · Telnet · serielle Verbindungen · RDP/VNC · SFTP · Portweiterleitung · integrierter Editor — alles in einer App mit GPU-Rendering.
Kein Konto. Kein Abo. Keine Telemetrie. Kein Electron.

[![Neueste Version](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Plattformen](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![Lizenz](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Sterne](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Herunterladen**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Dokumentation**](https://oxideterm.app) ·
[**Änderungsprotokoll**](../../.github/release-notes/stable-changelog.md) ·
[**Problem melden**](https://github.com/AnalyseDeCircuit/oxideterm/issues)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md) | [Русский](README.ru.md)

</div>

---

## Schnellstart

1. **OxideTerm installieren.** Laden Sie ein Paket der [neuesten Version](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) herunter; Hinweise zu den Plattformen finden Sie unten unter [Installation](#install).
2. **Server hinzufügen.** Öffnen Sie den Sitzungsmanager und erstellen Sie eine SSH-Verbindung oder importieren Sie Hosts aus Ihrer `~/.ssh/config`.
3. **Verbinden.** Öffnen Sie ein Terminal. Host-Schlüssel werden anhand von `~/.ssh/known_hosts` geprüft.
4. **Die weiteren Werkzeuge nutzen.** Öffnen Sie SFTP, Portweiterleitung oder den integrierten Editor am selben Knoten. Standardmäßig teilen sie sich eine SSH-Verbindung.
5. **Optional: KI aktivieren.** Fügen Sie in den Einstellungen Ihren eigenen Endpunkt für OpenAI, Anthropic, Gemini, Ollama oder einen OpenAI-kompatiblen Dienst hinzu, um OxideSens zu aktivieren.

Eine Einführung finden Sie in der [Dokumentation](https://oxideterm.app).

---

## Funktionen

| | |
|---|---|
| **Terminals und Protokolle** | Lokale Shells, SSH, Mosh, Telnet, serielle Verbindungen, geteilte Bereiche, Routen über mehrere Zwischenstationen, SSH-Agent und Agent-Weiterleitung, Zwei-Faktor-Authentifizierung (2FA) und TOTP-Zugangsdaten, X11-Weiterleitung, Shell-Integration, Befehlsmarkierungen, konfigurierbare Sitzungsprotokolle, Aufzeichnung, Sixel- und Kitty-Grafik, trzsz-Übertragungen |
| **tmux und Broadcast** | Nativer `tmux -CC`-Steuerungsmodus mit Bereichsanordnung und verschiebbaren Trennlinien, benannte Broadcast-Gruppen und ein erweitertes Werkzeug zur Befehlsübertragung an mehrere Ziele für geplante, wiederholbare Eingaben |
| **Zuverlässigkeit** | Die Grace-Period-Wiederverbindung hält TUI-Anwendungen bei kurzen Netzunterbrechungen am Leben und stellt anschließend Weiterleitungen, Übertragungen und geöffnete Editordateien wieder her |
| **Dateien und Bearbeitung** | SFTP-Dateimanager mit zwei Bereichen, Übertragungswarteschlangen mit Geschwindigkeitsbegrenzung und geschätzter Restzeit, Lesezeichen sowie integrierter Remote-Editor mit sicheren Schreibvorgängen, Konfliktbehandlung und Wiederherstellung des Arbeitsbereichs |
| **Netzwerk** | Lokale, entfernte und dynamische SOCKS5-Weiterleitung, gespeicherte Regeln, Erkennung entfernter Ports, Verbindungstopologie und gezieltes Socket-Debugging |
| **Remote-Desktop** | Integriertes RDP und VNC mit Unterstützung für Zwischenablage und Eingaben |
| **Host-Verwaltung** | Überwachung von Prozessen, Diensten, Protokollen, Ports, Aufgaben, Datenträgern, Paketen, Containern und tmux |
| **KI und Automatisierung** | OxideSens mit eigenem API-Schlüssel, MCP, lokales RAG, Agent Skills, freigegebene Arbeitsbereichsaktionen und eine eigenständige CLI |
| **Nachverfolgung und Audit** | Optionaler Arbeitsbereich für Benachrichtigungen und Audit sowie verschlüsselte Sitzungsaufzeichnungen (beides standardmäßig deaktiviert) |
| **Synchronisierung und Portabilität** | Verschlüsselte Cloud-Synchronisierung, portable `.oxide`-Pakete |
| **Personalisierung** | Themes, Hintergrundbilder, konfigurierbare Tastenkürzel, Schnellbefehle und 11 Oberflächensprachen |

---

## Warum OxideTerm

- **Kostenlos und auf lokale Nutzung ausgerichtet.** Kein Konto, kein Abo, keine Telemetrie. Ihre Verbindungen und Betriebsdaten bleiben unter Ihrer Kontrolle.
- **Ein Arbeitsbereich pro Server.** Terminal, SFTP, Weiterleitungen, RDP/VNC, Editor, Überwachung und KI sind an denselben Knoten angebunden, statt als voneinander getrennte Werkzeuge zu arbeiten.
- **Eine echte native App.** Die Oberfläche wird mit [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) direkt auf der GPU gezeichnet. Es gibt weder Electron noch eine mitgelieferte WebView.
- **KI nach Ihren Vorgaben.** OxideSens verwendet Ihren eigenen Anbieter und Schlüssel und führt nur Aktionen aus, die Sie freigeben.
- **Robuste Verbindungen.** Die Grace-Period-Wiederverbindung prüft die alte Verbindung 30 Sekunden lang, bevor sie ersetzt wird, damit TUI-Anwendungen kurze Netzunterbrechungen überstehen können.
- **SSH vollständig in Rust.** Der SSH-Stack verwendet `russh` mit `ring`, ohne OpenSSL oder libssh2.

---

## Speicherverbrauch

**Die native Neuentwicklung senkte den Speicherverbrauch im Leerlauf unter macOS auf etwa ein Viertel und unter Windows auf etwa ein Achtel des Werts der alten Version.** Dies sind die vom Maintainer festgehaltenen Beobachtungen beim Wechsel von Tauri 1.x zur nativen GPUI-Version 2.0:

| Plattform | Tauri 1.x (Leerlauf) | Native Version 2.0 (Leerlauf) | Rückgang |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | Etwa 74% |
| Windows | 182.4 MB | 23.5 MB | Etwa 87% |

Der Gesamtwert der alten Version umfasst OxideTerm und die zugehörigen WebView-Prozesse. Die native Version benötigt diese Browserprozesse nicht mehr.

![Speichervergleich im Leerlauf mit Screenshots der Systemprozesse: Tauri 1.x und native Version 2.0](../../docs/screenshots/oxideterm-memory-comparison.png)

---

## Screenshots

| SSH-Terminal mit OxideSens | SFTP-Dateimanager |
|---|---|
| ![SSH-Terminal mit OxideSens-KI](../../docs/screenshots/terminal/SSHTERMINAL.png) | ![SFTP-Dateimanager mit zwei Bereichen und Übertragungswarteschlange](../../docs/screenshots/sftp/sftp.png) |

| Integrierte IDE | Intelligente Portweiterleitung |
|---|---|
| ![Integrierter IDE-Modus](../../docs/screenshots/miniIDE/miniide.png) | ![Intelligente Portweiterleitung mit automatischer Erkennung](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png) |

<details>
<summary><b>So öffnet OxideSens ein Terminal auf eine Anfrage in natürlicher Sprache</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens öffnet ein Terminal in OxideTerm" width="720">
</a>

</details>

---

## OxideSens-KI

OxideSens ist ein optionaler Assistent, der Ihre aktiven Sitzungen prüfen und Arbeitsbereichsaktionen **erst nach Ihrer Freigabe** ausführen kann.

- **Ihren eigenen Schlüssel verwenden.** Unterstützt OpenAI, Anthropic (Claude), Google Gemini, Ollama und jeden OpenAI-kompatiblen Endpunkt, mit an den Anbieter angepassten Einstellungen für das Schlussfolgern. Es gibt kein Plattformguthaben.
- **MCP und Agent Skills.** Verbinden Sie MCP-Server (stdio und SSE) und laden Sie Agent Skills mit festgelegten Grenzen.
- **Lokale Wissensbasis (RAG).** BM25-Volltextsuche und Vektorindex.
- **Sie kontrollieren den Kontext.** Sie entscheiden, welcher Arbeitsbereichskontext und welche Aktionen freigegeben werden; dabei gelten die Regeln der Befehlsrichtlinie.
- **Maskierung von Zugangsdaten.** Nachrichten an einen Anbieter werden auf Zugangsdatenmuster geprüft und entsprechend maskiert.
- **Schlüssel bleiben im Schlüsselbund Ihres Betriebssystems** und werden aus strukturierten Protokollen ausgeschlossen.

---

<a id="plugins"></a>

## Plugins

OxideTerm unterstützt drei Plugin-Varianten:

| Typ | Ausführung | Grenzen |
|---|---|---|
| **Nur Manifest** | Deklarative Erweiterungen ohne Code | Kein ausführbarer Code |
| **WASM** | Wasmtime/WASI oder Hilfsprozess | Kontrollierte Host-Aufrufe, auf freigegebene Fähigkeiten begrenzt |
| **Prozess** | Gewöhnlicher lokaler Prozess | Vertrauenswürdiger lokaler Code, **ohne** Betriebssystem-Sandbox |

Ältere ESM-Plugins für Tauri (1.x) können aufgelistet werden, werden aber von der nativen 2.x-App nicht ausgeführt. Installieren Sie Prozess-Plugins nur aus vertrauenswürdigen Quellen.

---

## Sicherheit und Datenschutz

| Thema | Umsetzung |
|---|---|
| **Gespeicherte Zugangsdaten** | Betriebssystem-Schlüsselbund (macOS Keychain, Windows Credential Manager, libsecret) |
| **Geheimnisse im Speicher** | Typen mit sensiblen Daten und temporäre Puffer verwenden `zeroize` an den unterstützten Stellen der Speicherbesitzverwaltung |
| **Host-Schlüssel** | Vertrauen beim ersten Zugriff anhand von `~/.ssh/known_hosts`; unerwartete Änderungen werden abgelehnt |
| **Portable Exporte** | `.oxide`-Pakete verwenden ChaCha20-Poly1305 mit Argon2id (256 MB Speicher, 4 Iterationen) |
| **KI-Kontext** | Zugangsdatenmuster werden vor der Übermittlung an einen Anbieter maskiert; Sie geben Kontext und Aktionen frei |
| **Sitzungsaufzeichnungen** | Standardmäßig deaktiviert; verschlüsselt auf Ihrem Gerät gespeichert und von der Cloud-Synchronisierung ausgeschlossen; Tastatureingaben werden nicht aufgezeichnet |
| **Audit** | Standardmäßig deaktiviert; Daten bleiben auf Ihrem Gerät, sensible Details werden verschlüsselt |
| **Änderungen über die CLI** | Simulationspläne, Bestätigung durch `--yes` und Sicherungen zur Wiederherstellung bei zustandsändernden Befehlen |
| **Plugins** | Siehe [Plugins](#plugins) |
| **Telemetrie** | Keine |

**Rechtmäßige Nutzung.** OxideTerm steht unter GPL-3.0-only ohne zusätzliche Einschränkungen. Greifen Sie nur auf Systeme, Netzwerke und Geräte zu, die Ihnen gehören oder für die Sie eine ausdrückliche Zugriffsberechtigung besitzen, und beachten Sie geltendes Recht. Verwenden Sie OxideTerm nicht für unbefugte Zugriffe, Dienststörungen oder zur Umgehung von Zugriffskontrollen.

---

## Aktuelle Einschränkungen

Das sollten Sie vor der Installation wissen:

- Nur für Desktop-Systeme (macOS, Windows, Linux). Es gibt keine mobile App.
- Das Projekt entwickelt sich schnell und veröffentlicht häufig neue Versionen. Siehe [Änderungsprotokoll](../../.github/release-notes/stable-changelog.md) und [offene Issues](https://github.com/AnalyseDeCircuit/oxideterm/issues).
- Audit und Sitzungsaufzeichnung müssen ausdrücklich aktiviert werden und erfassen nur, was OxideTerm selbst beobachten kann.
- Prozess-Plugins laufen ohne Betriebssystem-Sandbox.
- Falls der Renderer auf Ihrem Rechner nicht funktioniert, versuchen Sie das Kompatibilitätsprofil: `OXIDETERM_RENDER_PROFILE=compatibility`.

---

<a id="for-developers"></a>

## Für Entwickler

<details>
<summary><b>Aus dem Quellcode ausführen</b></summary>

**Voraussetzungen:** Rust-Toolchain (Edition 2024) und eine Desktop-Umgebung, die GPUI ausführen kann.

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

Mit Nix: `nix build .#oxideterm`, `nix run .#oxideterm` oder `nix develop`.

Die CLI-Build-Ergebnisse liegen unter `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

</details>

<details>
<summary><b>Befehlszeilenschnittstelle</b></summary>

Die `oxideterm`-CLI ohne grafische Oberfläche funktioniert, ohne die App zu starten, und eignet sich für Automatisierung, CI und Diagnosen. Sie deckt Einstellungen, Verbindungen, Weiterleitungen, Plugins, Schnellbefehle, Geheimnisse, portable Pakete, Diagnosen, Berichte, Stapelpläne, Sicherungen und Cloud-Synchronisierung ab.

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
<summary><b>Architektur</b></summary>

Oberfläche und Terminal-/SSH-Backend teilen sich einen Rust-Prozess; optionale Remote-Agenten und Plattform-Hilfsprogramme laufen außerhalb dieser Grenze. Terminalbytes ändern `TerminalState` direkt, und GPUI rendert aus diesem Zustand — ohne einen Verarbeitungsschritt über JSON, WebSocket, Base64 oder xterm.js.

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

| Aspekt | Ansatz mit mitgeliefertem Browser | OxideTerm |
|---|---|---|
| Rendering | Browser-Engine und Web-Layout | GPUI auf einer GPU-Fläche |
| Terminal-Datenfluss | WebSocket → JS-Ereignisschleife → xterm.js | Rust-Eingabe → `TerminalState` → GPUI-Rendering |
| Verbindungslebenszyklus | Auf Frontend und Backend verteilt | Eine Verbindungs- und Wiederverbindungspipeline innerhalb des Prozesses |
| KI-Kontext | Über eine Anwendungsbrücke kopiert | Mit Nutzerfreigabe aus dem aktiven Arbeitsbereich aufgebaut |
| CLI | Benötigt die laufende Desktop-App | Eigenständiges Programm, direkte Einbindung der Crates |

**Verbindungspool.** `SshConnectionRegistry` basiert auf `DashMap` und wird über `NodeRouter` verwendet. Terminalbereiche, SFTP, Portweiterleitungen und Editor können sich eine physische SSH-Verbindung pro Knoten teilen; eine Terminalrichtlinie kann stattdessen eine dedizierte Verbindung vorsehen. Jede Verbindung durchläuft `connecting → active → idle → link_down → reconnecting`. Ein Ausfall des Zwischenhosts setzt nachgelagerte Knoten auf `link_down`. KI und Plugins verwenden Referenzen auf Fähigkeiten und Host-Zustandsabbilder, statt sich als Verbindungsnutzer zu registrieren.

**Grace-Period-Wiederverbindung.**

1. Einen Keepalive-Timeout erkennen.
2. Den Zustand von Terminalbereichen, SFTP-Übertragungen, Weiterleitungen und Editordateien erfassen.
3. Die alte Verbindung 30 s lang prüfen, damit TUI-Anwendungen kurze Netzunterbrechungen überstehen können.
4. Eine neue Verbindung öffnen, Weiterleitungen wiederherstellen, Übertragungen fortsetzen und Editordateien erneut öffnen.

SFTP-Sitzungen tragen eine Verbindungsgeneration: Nach einer Wiederverbindung wird eine dafür geeignete Sitzung neu bezogen; eine Operation aus einer alten Generation wird jedoch niemals stillschweigend auf die neue Verbindung übertragen.

**Portweiterleitung.** Eine eigenständige Crate unterstützt `-L`, `-R` und `-D` (SOCKS5). Jeder SSH-Kanal gehört einer einzelnen `ssh_io`-Task, sodass auf dem zeitkritischen Ausführungspfad kein gemeinsamer Mutex nötig ist.

**SSH vollständig in Rust.** `russh` mit `ring`: vollständiges SSH2, ChaCha20-Poly1305 und AES-GCM, Ed25519-/RSA-/ECDSA-Schlüssel, SSH-Agent unter Unix (`SSH_AUTH_SOCK`) und Windows (`\\.\pipe\openssh-ssh-agent`) sowie Verbindungen über mehrere Zwischenstationen mit unabhängiger Authentifizierung je Station.

**Technologien**

| Ebene | Technologie |
|---|---|
| Oberfläche | GPUI (Zeds UI-Framework mit GPU-Rendering) |
| Laufzeit | Tokio, DashMap |
| SSH | `russh` mit `ring` (ohne OpenSSL oder libssh2) |
| Lokales PTY | `portable-pty` (ConPTY unter Windows) |
| Terminalemulation | `alacritty_terminal` (VT100–VT500, Sixel- und Kitty-Grafik) |
| Editor | Syntaxhervorhebung mit tree-sitter, eigener Puffer |
| Verschlüsselung | ChaCha20-Poly1305, Argon2id |
| Plugins | Wasmtime/WASI, WASM-Hilfsprozesse und Prozess-Plugins |
| KI-Streaming | SSE (OpenAI, Anthropic, Gemini), innerhalb des Prozesses |
| RAG | BM25 + HNSW-Vektorindex mit Rangfusion, CJK-Bigramm-Tokenizer |
| i18n | `oxideterm-i18n` (11 Sprachversionen) |

</details>

---

<a id="install"></a>

## Installation

[**Neueste Version herunterladen**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

| Betriebssystem | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Installationsprogramm (`.exe`) | Installationsprogramm (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

Prüfen Sie Ihren Download anhand der Datei `sha256sums.txt` auf der Release-Seite. Dort sind auch portable Archive und Signaturen aufgeführt.

### macOS

Falls Gatekeeper die App blockiert, entfernen Sie das Quarantäneattribut:

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

Wenn SmartScreen eine Warnung anzeigt, wählen Sie **Weitere Informationen → Trotzdem ausführen**.

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

Möchten Sie selbst kompilieren? Siehe **Aus dem Quellcode ausführen** unter [Für Entwickler](#for-developers).

---

## Mitwirken

Beiträge sind willkommen: Rust-Code, Dokumentation, Übersetzungen, Plugins, Tests und die Reproduktion von Fehlern. Öffnen Sie bei größeren Änderungen zunächst ein Issue zur Abstimmung.

Fehlerberichte sind besonders hilfreich, wenn sie ein Diagnosepaket mit maskierten sensiblen Daten enthalten:

```bash
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

Reproduzierbare Fehler und Regressionen haben Vorrang. Funktionswünsche werden nach Umfang, Sicherheit und ihrer Eignung für OxideTerms Ausrichtung als Arbeitsbereich für Remote-Server bewertet. Wenn OxideTerm Ihre Arbeit unterstützt, helfen ein GitHub-Stern, ein reproduzierbarer Fehlerbericht, eine Übersetzungskorrektur oder ein Plugin dabei, das Projekt weiterzuentwickeln.

### Mitwirkende

Vielen Dank an alle, die OxideTerm verbessern.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="Mitwirkende bei OxideTerm">
  </a>
</p>

---

## Lizenz

**GPL-3.0-only.** Hinweise zu den Abhängigkeiten stehen in [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md); weitere Hinweise enthält [`NOTICE`](../../NOTICE).

**Entwickelt mit:** [russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
