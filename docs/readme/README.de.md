<p align="center">
  <img src="../../src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>Wenn dir OxideTerm gefällt, gib uns bitte einen Stern auf GitHub! ⭐️</em>
</p>


<p align="center">
  <strong>OxideTerm ist ein local-first SSH-Workspace, nicht nur ein Terminal.</strong>
  <br>
  <em>Öffnen Sie einmal einen Remote-Knoten (Serververbindung) und arbeiten Sie darum herum: Shell, SFTP, Portweiterleitung, trzsz, leichtgewichtige Bearbeitung und BYOK-KI.</em>
  <br>
  <strong>Kein Electron. Kein OpenSSL. Keine Telemetrie. Kein Abo. BYOK-first. Reines Rust-SSH.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.4.0--beta.5-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plattform">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Lizenz">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
  <img src="https://img.shields.io/github/downloads/AnalyseDeCircuit/oxideterm/total?color=brightgreen" alt="Downloads gesamt">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=Neueste%20Version%20herunterladen&style=for-the-badge&color=brightgreen" alt="Neueste Version herunterladen">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=Neueste%20Beta%20herunterladen&style=for-the-badge&color=orange" alt="Neueste Beta herunterladen">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI — steuern Sie Live-Terminals und Workspace-Werkzeuge über einen Assistenten.*

</div>

---

## Warum OxideTerm?

| Wenn Ihnen wichtig ist... | OxideTerm bietet... |
|---|---|
| SSH-Workspace, nicht nur eine Shell | **Remote-Knoten-Workspace**: ein Knoten mit Terminal, SFTP, Portweiterleitung, trzsz, leichtgewichtiger IDE, Monitoring und KI-Kontext |
| Lokale Shells im selben Workflow | **Hybrid-Engine**: lokales PTY (zsh/bash/fish/pwsh/WSL2) und Remote-SSH laufen nebeneinander, damit lokale und entfernte Arbeit in einem Workspace bleiben |
| Kein Cloud-Konto für SSH-Workflows | **Local-first-Kern**: SSH, SFTP, Portweiterleitung, lokale Shell und Konfiguration funktionieren ohne Registrierung |
| BYOK-KI statt Plattform-Credits | **OxideSens**: verwenden Sie Ihren eigenen OpenAI/Ollama/DeepSeek/OpenAI-kompatiblen Endpoint mit MCP- und RAG-Unterstützung |
| Kein Electron-Runtime | **Tauri 2.0**: natives Rust-Backend, 25–40 MB Binärdatei |
| Kein OpenSSL-Ballast | **russh 0.59**: reines Rust-SSH kompiliert mit `ring` — keine OpenSSL/libssh2-Abhängigkeit |
| Keine Telemetrie oder App-Abos | **Kein Tracking, kein App-Abo für zentrale SSH-Workflows**: SSH/SFTP/Portweiterleitung/lokale Shell benötigen kein Konto und kein App-Abo; Ihre Daten bleiben standardmäßig auf Ihrem Gerät; Cloud-Synchronisation ist optional via [offiziellem Plugin](#offizielle-plugins) |
| Stabile Verbindungen | **Grace-Period-Reconnect**: prüft die alte Verbindung 30 s lang, bevor sie getrennt wird — vim/htop/yazi können Netzwerkruckler überleben |
| Remote-Dateiarbeit ohne VS Code Remote | **Integriertes SFTP + leichtgewichtige IDE**: entfernte Dateien im selben SSH-Workspace durchsuchen, ansehen, übertragen und bearbeiten |
| Sichere Zugangsdaten | **Verschlüsselt im Ruhezustand**: Passwörter und API-Schlüssel bleiben im OS-Schlüsselbund, Metadaten gespeicherter Verbindungen werden lokal versiegelt, und `.oxide`-Dateien sind mit ChaCha20-Poly1305 + Argon2id verschlüsselt |

## Was es ist / nicht ist

OxideTerm ist ein **local-first SSH-Workspace**: Öffnen Sie einmal einen Remote-Knoten und bedienen Sie Shell, Dateien, Ports, Terminal-interne Transfers, leichtgewichtige Bearbeitung und KI-Kontext an einem Ort.

OxideTerm ist **keine** Cloud-KI-Plattform, kein gehosteter Agent-Dienst, kein generischer Remote-Protokoll-Werkzeugkasten und kein Projekt, dessen Hauptargument Terminal-Rendering-Benchmarks sind. Viele moderne Terminals entwickeln sich um lokale Shells, KI-Panels oder Cloud-Agent-Plattformen; OxideTerm konzentriert sich auf den local-first SSH-Workspace.

---

## Screenshots

<table>
<tr>
<td align="center"><strong>SSH-Terminal + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="SSH-Terminal mit OxideSens AI-Seitenleiste" /></td>
<td align="center"><strong>SFTP-Dateimanager</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="SFTP Dual-Pane-Dateimanager mit Transfer-Warteschlange" /></td>
</tr>
<tr>
<td align="center"><strong>Integrierte IDE (CodeMirror 6)</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Integrierter IDE-Modus mit CodeMirror 6-Editor" /></td>
<td align="center"><strong>Intelligente Portweiterleitung</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Intelligente Portweiterleitung mit Auto-Erkennung" /></td>
</tr>
</table>

---

## Download

Laden Sie die neueste Version von [GitHub Releases](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) herunter.

---

## Funktionsübersicht

| Kategorie | Funktionen |
|---|---|
| **Terminal** | Lokales PTY (zsh/bash/fish/pwsh/WSL2), SSH Remote, geteilte Fenster, Broadcast-Eingabe, Sitzungsaufzeichnung/-wiedergabe (asciicast v2), WebGL-Rendering, 30+ Designs + benutzerdefinierter Editor, Befehlspalette (`⌘K`), Zen-Modus, **trzsz** In-Band-Dateiübertragung |
| **SSH & Authentifizierung** | Verbindungspool & Multiplexing, ProxyJump (unbegrenzte Hops) mit Topologiegraph, Auto-Reconnect mit Grace Period, Agent-Weiterleitung. Auth: Passwort, SSH-Schlüssel (RSA/Ed25519/ECDSA), SSH Agent, Zertifikate, Keyboard-Interactive 2FA, Known Hosts TOFU |
| **SFTP** | Dual-Pane-Browser, Drag-and-Drop, intelligente Vorschau (Bilder/Video/Audio/Code/PDF/Hex/Schriftarten), Transfer-Warteschlange mit Fortschritt & ETA, Lesezeichen, Archivextraktion |
| **IDE-Modus** | CodeMirror 6 mit 30+ Sprachen, Dateibaum + Git-Status, Multi-Tab, Konfliktlösung, integriertes Terminal. Optionaler Remote-Agent für Linux (9 zusätzliche Architekturen) |
| **Portweiterleitung** | Lokal (-L), Remote (-R), dynamisches SOCKS5 (-D), lock-freie Message-Passing-I/O, automatische Wiederherstellung bei Reconnect, Ausfallberichterstattung, Leerlauf-Timeout |
| **KI (OxideSens)** | Zielorientierter Assistent für gespeicherte Verbindungen, Live-SSH-Sitzungen, Terminal-Buffer, SFTP-Pfade, Einstellungen und Wissensdatenbank-Einträge; kann Remote-Ausgaben diagnostizieren, genehmigte Befehle ausführen, Dateien prüfen und Fehler erklären, ohne ein OxideTerm-Konto zu benötigen |
| **Plugins** | Laufzeit-ESM-Laden, 18 API-Namensräume, 24 UI-Kit-Komponenten, eingefrorene API + Proxy-ACL, Circuit Breaker, automatische Deaktivierung bei Fehlern |
| **CLI** | `oxt`-Companion: JSON-RPC 2.0 über Unix Socket / Named Pipe, status/health/list/forward/config/connect/focus/attach/SFTP/import/AI, menschenlesbare + JSON-Ausgabe |
| **Sicherheit** | .oxide-verschlüsselter Export (ChaCha20-Poly1305 + Argon2id 256 MB), lokal verschlüsselte Konfiguration im Ruhezustand, Betriebssystem-Schlüsselbund, Touch ID (macOS), portabler verschlüsselter Schlüsselspeicher, Host-Key-TOFU, `zeroize`-Speicherbereinigung |
| **i18n** | 11 Sprachen: EN, 简体中文, 繁體中文, 日本語, 한국어, FR, DE, ES, IT, PT-BR, VI |

---

## Unter der Haube

### Architektur — Dual-Plane-Kommunikation

OxideTerm trennt Terminaldaten von Steuerbefehlen in zwei unabhängige Ebenen:

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│  xterm.js 6 (WebGL) + 19 stores     │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binary)
           │ (JSON)       │ per-session port
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│  NodeRouter → SshConnectionRegistry │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]      │
└─────────────────────────────────────┘
```

- **Datenebene (WebSocket)**: Jede SSH-Sitzung erhält ihren eigenen WebSocket-Port. Terminal-Bytes fließen als Binärframes mit Type-Length-Payload-Header — keine JSON-Serialisierung, kein Base64-Encoding, null Overhead auf dem kritischen Pfad.
- **Steuerungsebene (Tauri IPC)**: Verbindungsverwaltung, SFTP-Operationen, Weiterleitungen, Konfiguration — strukturiertes JSON, aber abseits des kritischen Pfads.
- **Knoten-basierte Adressierung**: Das Frontend berührt niemals `sessionId` oder `connectionId`. Alles wird über `nodeId` adressiert, serverseitig atomar vom `NodeRouter` aufgelöst. SSH-Reconnect ändert die zugrunde liegende `connectionId` — aber SFTP, IDE und Weiterleitungen sind davon völlig unberührt.

### 🔩 Reines Rust-SSH — russh 0.59

Der gesamte SSH-Stack ist **russh 0.59**, kompiliert gegen das **`ring`**-Crypto-Backend:

- **Null C/OpenSSL-Abhängigkeiten** — der gesamte Crypto-Stack ist Rust. Keine „Welche OpenSSL-Version?"-Debugging-Sessions mehr.
- Vollständiges SSH2-Protokoll: Schlüsselaustausch, Kanäle, SFTP-Subsystem, Portweiterleitung
- ChaCha20-Poly1305 und AES-GCM Cipher Suites, Ed25519/RSA/ECDSA-Schlüssel
- Benutzerdefinierter **`AgentSigner`**: kapselt den System-SSH-Agent und implementiert das `Signer`-Trait von russh, löst RPITIT-`Send`-Bound-Probleme durch Klonen von `&AgentIdentity` zu einem eigenen Wert vor dem `.await`

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **Plattformunterstützung**: Unix (`SSH_AUTH_SOCK`), Windows (`\\.\pipe\openssh-ssh-agent`)
- **Proxy-Ketten**: Jeder Hop verwendet unabhängig Agent-Authentifizierung
- **Reconnect**: `AuthMethod::Agent` wird automatisch wiederholt

### 🔄 Intelligenter Reconnect mit Grace Period

Die meisten SSH-Clients beenden alles bei einer Unterbrechung und starten neu. OxideTerms Reconnect-Orchestrator verfolgt einen grundlegend anderen Ansatz:

1. **Erkennung** des WebSocket-Heartbeat-Timeouts (300 s, kalibriert für macOS App Nap und JS-Timer-Throttling)
2. **Snapshot** des vollständigen Zustands: Terminalfenster, laufende SFTP-Transfers, aktive Portweiterleitungen, geöffnete IDE-Dateien
3. **Intelligente Prüfung**: `visibilitychange` + `online`-Events lösen proaktives SSH-Keepalive aus (~2 s Erkennung gegenüber 15–30 s passivem Timeout)
4. **Grace Period** (30 s): prüft die alte SSH-Verbindung per Keepalive — falls sie sich erholt (z. B. WLAN-Accesspoint-Wechsel), überleben Ihre TUI-Apps (vim, htop, yazi) vollständig unberührt
5. Falls die Wiederherstellung fehlschlägt → neue SSH-Verbindung → automatische Wiederherstellung der Weiterleitungen → Wiederaufnahme der SFTP-Transfers → Wiedereröffnung der IDE-Dateien

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

Die gesamte Logik läuft über einen dedizierten `ReconnectOrchestratorStore` — kein Reconnect-Code verstreut in Hooks oder Komponenten.

### 🛡️ SSH-Verbindungspool

Referenzgezählte `SshConnectionRegistry` mit `DashMap` für lock-freien nebenläufigen Zugriff:

- **Eine Verbindung, viele Konsumenten**: Terminal, SFTP, Portweiterleitungen und IDE teilen sich eine einzige physische SSH-Verbindung — keine redundanten TCP-Handshakes
- **Zustandsmaschine pro Verbindung**: `connecting → active → idle → link_down → reconnecting`
- **Lebenszyklus-Management**: konfigurierbares Leerlauf-Timeout (5 Min. / 15 Min. / 30 Min. / 1 Std. / nie), 15 s Keepalive-Intervall, Heartbeat-Fehlererkennung
- **WsBridge-Heartbeat**: 30 s Intervall, 5 Min. Timeout — toleriert macOS App Nap und Browser-JS-Throttling
- **Kaskadenpropagation**: Ausfall des Jump-Hosts → alle nachgelagerten Knoten automatisch als `link_down` markiert mit Statussynchronisation
- **Leerlauf-Trennung**: sendet `connection_status_changed` an das Frontend (nicht nur internes `node:state`), verhindert UI-Desynchronisation

### 🤖 OxideSens AI

Datenschutzorientierter KI-Assistent mit zwei Interaktionsmodi:

- **Inline-Panel** (`⌘I`): schnelle Terminalbefehle, Ausgabe per Bracketed Paste eingefügt
- **Seitenleisten-Chat**: persistente Konversationen mit vollständigem Verlauf
- **Zielorientierter Workspace-Kontext**: sieht gespeicherte Verbindungen, Live-SSH-Sitzungen, Terminal-Buffer, SFTP-Pfade, Einstellungen und Wissensdatenbank-Einträge als Workspace-Ziele
- **Genehmigte Aktionen**: kann Remote-Ausgaben diagnostizieren, genehmigte Befehle ausführen, Dateien prüfen und Fehler erklären, ohne ein OxideTerm-Konto zu benötigen
- **MCP-Unterstützung**: externe [Model Context Protocol](https://modelcontextprotocol.io)-Server (stdio & SSE) für Drittanbieter-Werkzeugintegration verbinden
- **RAG-Wissensdatenbank** (v0.20): Importieren Sie Markdown/TXT-Dokumente in bereichsbezogene Sammlungen (global oder pro Verbindung). Hybridsuche fusioniert BM25-Schlüsselwortindex + Vektor-Kosinusähnlichkeit über Reciprocal Rank Fusion. Markdown-bewusstes Chunking erhält die Überschriftenhierarchie. CJK-Bigramm-Tokenizer für Chinesisch/Japanisch/Koreanisch.
- **Anbieter**: OpenAI, Ollama, DeepSeek, OneAPI oder jeder `/v1/chat/completions`-Endpunkt
- **Sicherheit**: API-Schlüssel im Betriebssystem-Schlüsselbund gespeichert; unter macOS wird der Schlüsselzugriff durch **Touch ID** über `LAContext` geschützt — keine Entitlements oder Code-Signierung erforderlich, nach der ersten Authentifizierung pro Sitzung zwischengespeichert

###  Portweiterleitung — Lock-freie I/O

Vollständige lokale (-L), Remote- (-R) und dynamische SOCKS5-Weiterleitung (-D):

- **Message-Passing-Architektur**: SSH Channel wird von einer einzelnen `ssh_io`-Task gehalten — kein `Arc<Mutex<Channel>>`, eliminiert Mutex-Contention vollständig
- **Ausfallberichterstattung**: Weiterleitungs-Tasks melden aktiv den Beendigungsgrund (SSH-Trennung, Remote-Port-Schließung, Timeout) für klare Diagnose
- **Automatische Wiederherstellung**: `Suspended`-Weiterleitungen werden bei Reconnect automatisch fortgesetzt, ohne Benutzereingriff
- **Leerlauf-Timeout**: `FORWARD_IDLE_TIMEOUT` (300 s) verhindert die Ansammlung von Zombie-Verbindungen

### 📦 trzsz — In-Band-Dateiübertragung

Dateien direkt über die SSH-Terminalsitzung hoch- und herunterladen — ohne SFTP-Verbindung:

- **In-Band-Protokoll**: Dateien werden als Base64-kodierte Frames im bestehenden Terminalstream übertragen — funktioniert transparent durch ProxyJump-Ketten und tmux ohne zusätzliche Ports oder Agenten
- **Bidirektional**: der Server führt `tsz <Datei>` aus, um Dateien an den Client zu senden; `trz` startet den Client-Upload; Drag-and-Drop unterstützt
- **Verzeichnis-Unterstützung**: rekursive Übertragungen via `trz -d` / `tsz -d`
- **Übertragungslimits**: konfigurierbare Limits pro Sitzung für Chunk-Größe, Dateianzahl und Gesamtbytes
- **Natives Tauri I/O**: Dateilese- und -schreibvorgänge über native Tauri-Dateidialoge und Rust I/O — keine Browser-Speicherbeschränkungen
- **Live-Benachrichtigungen**: Toast-Benachrichtigungen für Start, Abschluss, Abbruch und Fehler — einschließlich eines Hinweises, wenn trzsz erkannt, aber die Funktion deaktiviert ist
- Aktivieren unter **Einstellungen → Terminal → In-Band-Übertragung**

### 🔌 Laufzeit-Plugin-System

Dynamisches ESM-Laden mit sicherheitsgehärteter, eingefrorener API-Oberfläche:

- **PluginContext-API**: 18 Namensräume — terminal, ui, commands, settings, lifecycle, events, storage, system
- **24 UI-Kit-Komponenten**: vorgefertigte React-Komponenten (Buttons, Eingabefelder, Dialoge, Tabellen…), in Plugin-Sandboxen über `window.__OXIDE__` injiziert
- **Sicherheitsmembran**: `Object.freeze` auf allen Kontextobjekten, Proxy-basierte ACL, IPC-Whitelist, Circuit Breaker mit automatischer Deaktivierung nach wiederholten Fehlern
- **Geteilte Module**: React, ReactDOM, zustand, lucide-react für Plugins bereitgestellt, ohne Bundle-Duplikation

### ⚡ Adaptives Rendering

Dreistufiger Render-Scheduler, der festes `requestAnimationFrame`-Batching ersetzt:

| Stufe | Auslöser | Rate | Vorteil |
|---|---|---|---|
| **Boost** | Frame-Daten ≥ 4 KB | 120 Hz+ (natives ProMotion) | Eliminiert Scroll-Lag bei `cat largefile.log` |
| **Normal** | Standard-Eingabe | 60 Hz (RAF) | Flüssige Basislinie |
| **Idle** | 3 s ohne I/O / Tab verborgen | 1–15 Hz (exponentielle Verlangsamung) | Nahezu null GPU-Last, Batterieersparnis |

Übergänge sind vollautomatisch — gesteuert durch Datenvolumen, Benutzereingaben und Page Visibility API. Hintergrund-Tabs leeren Daten weiterhin über den Idle-Timer, ohne RAF zu wecken.

### 🔐 .oxide-verschlüsselter Export

Portables, manipulationssicheres Verbindungs-Backup:

- Authentifizierte Verschlüsselung mit **ChaCha20-Poly1305 AEAD**
- **Argon2id KDF**: 256 MB Speicherkosten, 4 Iterationen — GPU-Brute-Force-resistent
- **SHA-256**-Integritätsprüfsumme
- **Optionale Schlüsseleinbettung**: private Schlüssel base64-kodiert in der verschlüsselten Nutzlast
- **Vorab-Analyse**: Aufschlüsselung der Auth-Typen, Erkennung fehlender Schlüssel vor dem Export

### 📡 ProxyJump — Topologie-bewusstes Multi-Hop

- Unbegrenzte Kettentiefe: `Client → Sprung A → Sprung B → … → Ziel`
- Automatisches Parsen von `~/.ssh/config`, Aufbau des Topologiegraphen, Dijkstra-Pfadfindung für optimale Route
- Sprungknoten als unabhängige Sitzungen wiederverwendbar
- Kaskadierende Fehlerpropagation: Jump-Host ausgefallen → alle nachgelagerten Knoten automatisch als `link_down` markiert

### ⚙️ Lokales Terminal — Thread-sicheres PTY

Plattformübergreifende lokale Shell über `portable-pty 0.8`, Feature-gated hinter `local-terminal`:

- `MasterPty` in `std::sync::Mutex` gekapselt — dedizierte I/O-Threads halten blockierende PTY-Lesevorgänge von der Tokio-Event-Loop fern
- Automatische Shell-Erkennung: `zsh`, `bash`, `fish`, `pwsh`, Git Bash, WSL2
- `cargo build --no-default-features` entfernt PTY für Mobile-/Leichtgewicht-Builds

### 🪟 Windows-Optimierung

- **Natives ConPTY**: ruft direkt die Windows Pseudo Console API auf — volle TrueColor- und ANSI-Unterstützung, kein veraltetes WinPTY
- **Shell-Scanner**: erkennt automatisch PowerShell 7, Git Bash, WSL2, CMD über Registry und PATH

### Und mehr

- **IDE-Modus**: CodeMirror 6 über SFTP, 24 Sprachen, Dateibaum mit Git-Status, Multi-Tab, Konflikterkennung — optionaler Remote-Agent (~1 MB) für erweiterte Funktionen unter Linux
- **Ressourcen-Profiler**: Live CPU/Speicher/Netzwerk über persistenten SSH-Kanal, liest `/proc/stat`, deltabasierte Berechnung, automatischer Fallback auf RTT-only bei Nicht-Linux-Systemen
- **Benutzerdefinierte Design-Engine**: 30+ integrierte Designs, visueller Editor mit Live-Vorschau, 20 xterm.js-Felder + 24 UI-Farbvariablen, automatische Ableitung der UI-Farben aus der Terminal-Palette
- **Sitzungsaufzeichnung**: asciicast v2-Format, vollständige Aufzeichnung und Wiedergabe
- **Broadcast-Eingabe**: einmal tippen, an alle geteilten Fenster senden — Batch-Server-Operationen
- **Hintergrund-Galerie**: Hintergrundbilder pro Tab, 16 Tab-Typen, Steuerung von Deckkraft/Unschärfe/Anpassung
- **CLI-Companion** (`oxt`): ~1 MB Binärdatei, JSON-RPC 2.0 über Unix Socket / Named Pipe, status/health/list/forward/config/connect/focus/attach/SFTP/import/AI mit menschenlesbarer oder `--json`-Ausgabe
- **WSL Graphics** ⚠️ experimentell: integrierter VNC-Viewer — 9 Desktop-Umgebungen + Einzelanwendungsmodus, WSLg-Erkennung, Xtigervnc + noVNC

#### Offizielle Plugins

| Plugin | Beschreibung | Repository |
|---|---|---|
| **Cloud Sync** | Verschlüsselte selbstgehostete Synchronisation — `.oxide`-Snapshots über WebDAV, HTTP JSON, Dropbox, Git oder S3 hochladen und importieren | [oxideterm.cloud-sync](https://github.com/AnalyseDeCircuit/oxideterm.cloud-sync) |
| **Telnet Client** | Nativer Telnet-Client für Router, Switches und Legacy-Geräte — keine externe Binary erforderlich | [oxideterm.telnet](https://github.com/AnalyseDeCircuit/oxideterm.telnet) |

<details>
<summary>📸 11 Sprachen in Aktion</summary>
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

## Laufzeitanforderungen

OxideTerm verwendet die native WebView-Laufzeit des Betriebssystems. Die meisten Nutzer haben sie bereits installiert; installieren Sie sie manuell nur, wenn die App nicht startet oder Ihre Umgebung air-gapped ist.

| Plattform | Laufzeitabhängigkeit |
|---|---|
| **Windows** | [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) — vorinstalliert auf Windows 10 (1803+) und Windows 11. Für **Air-Gap- / Intranet**-Umgebungen verwenden Sie den [Evergreen Standalone Installer](https://developer.microsoft.com/en-us/microsoft-edge/webview2/#download) (offline, ~170 MB) oder stellen Sie die **Fixed Version** Runtime per Gruppenrichtlinie bereit. |
| **macOS** | Keine (verwendet natives WebKit) |
| **Linux** | `libwebkit2gtk-4.1` (normalerweise auf modernen Desktops vorinstalliert) |

---

## Portabler Modus

OxideTerm unterstützt einen vollständig eigenständigen portablen Modus — alle Daten (Verbindungen, Geheimnisse, Einstellungen) werden neben der Anwendungsdatei gespeichert, ideal für USB-Sticks oder Offline-Umgebungen.

### Aktivierung

**Option A — Markierungsdatei** (am einfachsten): Erstellen Sie eine leere Datei namens `portable` (ohne Erweiterung) neben der Anwendung.

| Plattform | Wo die `portable`-Datei platziert wird |
|---|---|
| **macOS** | Neben `OxideTerm.app` (gleiches Verzeichnis) |
| **Windows** | Neben `OxideTerm.exe` |
| **Linux (AppImage)** | Neben der `.AppImage`-Datei |

```
/my-usb/
├── OxideTerm.app   (or .exe / .AppImage)
├── portable        ← von Ihnen erstellte leere Datei
└── data/           ← wird beim ersten Start automatisch erstellt
```

**Option B — `portable.json`** (benutzerdefiniertes Datenverzeichnis): Platzieren Sie eine `portable.json` am selben Ort:

```json
{
  "enabled": true,
  "dataDir": "my-data"
}
```

- `enabled` ist standardmäßig `true`, wenn weggelassen
- `dataDir` muss ein **relativer Pfad** sein (`..` nicht erlaubt); Standard ist `data`

### Funktionsweise

1. **Erster Start** — Der Bootstrap-Bildschirm fordert Sie auf, ein portables Passwort zu erstellen. Dieses Passwort verschlüsselt den lokalen Schlüsselspeicher (ChaCha20-Poly1305 + Argon2id) und schützt alle gespeicherten Geheimnisse.
2. **Weitere Starts** — Geben Sie das Passwort zum Entsperren ein. Auf macOS mit Touch ID können Sie die biometrische Entsperrung unter **Settings → General → Portable Runtime** optional aktivieren.
3. **Instanzsperre** — Es kann jeweils nur eine OxideTerm-Instanz das portable Datenverzeichnis verwenden (`data/.portable.lock`).
4. **Verwaltung** — Ändern Sie das portable Passwort oder wechseln Sie die biometrische Entsperrung unter **Settings → General → Portable Runtime**.
5. **Portabilität** — Kopieren Sie den gesamten Ordner (Anwendung + `portable`-Markierung + `data/`) auf einen anderen Computer. Das Passwort reist mit dem Schlüsselspeicher.

> [!TIP]
> Im portablen Modus sind automatische Updates deaktiviert. Zum Aktualisieren ersetzen Sie die Anwendungsdatei und behalten das `data/`-Verzeichnis bei.

---

## Schnellstart

### Voraussetzungen

- **Rust** 1.85+
- **Node.js** 18+ (pnpm empfohlen)
- **Plattform-Tools**:
  - macOS: Xcode Command Line Tools
  - Windows: Visual Studio C++ Build Tools
  - Linux: `build-essential`, `libwebkit2gtk-4.1-dev`, `libssl-dev`

### Entwicklung

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# CLI-Companion bauen (erforderlich für CLI-Funktionen)
pnpm cli:build

# Vollständige App (Frontend + Rust-Backend mit Hot Reload)
pnpm run tauri dev

# Nur Frontend (Vite auf Port 1420)
pnpm dev

# Produktions-Build
pnpm run tauri build
```

---

## Technologie-Stack

| Schicht | Technologie | Details |
|---|---|---|
| **Framework** | Tauri 2.0 | Native Binärdatei, 25–40 MB |
| **Runtime** | Tokio + DashMap 6 | Vollständig asynchron, lock-freie nebenläufige Maps |
| **SSH** | russh 0.59 (`ring`) | Reines Rust, null C-Abhängigkeiten, SSH Agent |
| **Lokales PTY** | portable-pty 0.8 | Feature-gated, ConPTY unter Windows |
| **Frontend** | React 19.1 + TypeScript 5.8 | Vite 7, Tailwind CSS 4 |
| **Zustand** | Zustand 5 | 19 spezialisierte Stores |
| **Terminal** | xterm.js 6 + WebGL | GPU-beschleunigt, 60 fps+ |
| **Editor** | CodeMirror 6 | 30+ Sprachmodi |
| **Verschlüsselung** | ChaCha20-Poly1305 + Argon2id | AEAD + speicherintensive KDF (256 MB) |
| **Speicher** | redb 2.1 | Eingebetteter KV-Store |
| **i18n** | i18next 25 | 11 Sprachen × 22 Namensräume |
| **Plugins** | ESM Runtime | Eingefrorener PluginContext + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## Projektgröße

Gemessen mit `tokei`, ohne Abhängigkeiten und Build-Artefakte.

| Metrik | Aktuelle Größe |
|---|---:|
| Gesamter Code | 286K+ |
| TypeScript / TSX | 130K+ |
| Rust | 100K+ |
| Frontend-Testcode | 24K+ |
| Frontend-Testdateien | 128 |
| Quelldateien (`src` + `src-tauri/src`) | 664 |

---

## Sicherheit

| Bereich | Implementierung |
|---|---|
| **Passwörter** | Betriebssystem-Schlüsselbund (macOS Keychain / Windows Credential Manager / libsecret) |
| **Portabler Schlüsselspeicher** | ChaCha20-Poly1305-verschlüsselter Tresor neben der Anwendung, optionale biometrische Bindung über den OS-Schlüsselbund |
| **KI-API-Schlüssel** | Betriebssystem-Schlüsselbund + biometrische Touch ID-Authentifizierung unter macOS |
| **Export** | .oxide: ChaCha20-Poly1305 + Argon2id (256 MB Speicher, 4 Iterationen) |
| **Speicher** | Rust-Speichersicherheit + `zeroize` zur Bereinigung sensibler Daten |
| **Host-Schlüssel** | TOFU mit `~/.ssh/known_hosts`, lehnt Änderungen ab (MITM-Prävention) |
| **Plugins** | Object.freeze + Proxy-ACL, Circuit Breaker, IPC-Whitelist |
| **WebSocket** | Einmal-Token mit Zeitlimits |

---

## Roadmap

- [x] SSH-Agent-Forwarding
- [ ] Vollständige ProxyCommand-Unterstützung
- [ ] Audit-Protokollierung
- [ ] Agent-Verbesserungen
- [ ] Sitzungssuche & Schnellwechsel

---

## Support und Wartung

OxideTerm wird von einem einzelnen Entwickler nach dem **Best-Effort-Prinzip** gepflegt. Fehlerberichte und reproduzierbare Regressionen haben Priorität; Feature-Anfragen sind willkommen, werden aber nicht immer umgesetzt.

Wenn OxideTerm Ihren Workflow verbessert, helfen ein GitHub-Stern, eine Issue-Reproduktion, eine Übersetzungskorrektur, ein Plugin oder ein Pull Request dabei, das Projekt voranzubringen.

---

## Lizenz

**GPL-3.0** — diese Software ist freie Software, lizenziert unter der [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.html).

Sie dürfen diese Software gemäß den Bedingungen der GPL-3.0 frei nutzen, ändern und verteilen. Abgeleitete Werke müssen ebenfalls unter derselben Lizenz verteilt werden.

OxideTerm wechselte ab v1.0.0 von **PolyForm Noncommercial 1.0.0** zu **GPL-3.0**. Wir haben diesen Wechsel bewusst vorgenommen: kein „Open Source“-Theater mit Noncommercial-Fallen oder No-Competition-Zusatzklauseln, sondern klare Copyleft-Freiheit für Nutzer, Forks, Weiterverbreiter und kommerzielle Betreiber.

Öffentlich einsehbarer Code ist nicht automatisch Open Source. Wenn ein Projekt eine bekannte Open-Source-Lizenz zeigt, aber zusätzlich Klauseln wie „keine Weiterverbreitung“, „kein Repackaging“, „keine konkurrierenden Produkte“ oder „keine nicht autorisierten Vertriebsplattformen“ anhängt, ist das eher Source-Available-Marketing als die Freiheit, die Nutzer von Open Source erwarten. OxideTerm fügt keine No-Compete- oder Anti-Redistribution-Klauseln hinzu: Die GPL-3.0-Bedingungen sind die vollständigen Bedingungen.

Vollständiger Text: [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## Danksagungen

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>271.000+ Zeilen Rust & TypeScript — gebaut mit ⚡ und ☕</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
