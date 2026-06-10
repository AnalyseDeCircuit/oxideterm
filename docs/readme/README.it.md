<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Workspace AI-native per server remoti.</strong>
  <br>
  Connettiti ai tuoi server via SSH e lavora con terminali, file, porte, trasferimenti, editing leggero, console seriali e OxideSens AI in un'app nativa local-first.
  <br>
  App GPUI nativa · SSH puro in Rust · OxideSens AI BYOK · nessun account per i workflow SSH principali
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust all the way down.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.6-blue" alt="Versione">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Piattaforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licenza">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Prossima grande edizione nativa di <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — renderizzata su GPU, zero-WebView, usando <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (framework di rendering di Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens apre un terminale dentro OxideTerm" width="920">
</a>

*OxideSens segue una richiesta dell’utente e apre un terminale dentro OxideTerm.*

</div>

---

## Cosa puoi fare

- Gestire terminali SSH, SFTP, port forwarding, console seriali, shell locali ed editing leggero in un workspace nativo
- Mantenere vivo il lavoro remoto durante problemi di rete con Grace Period reconnect
- Chiedere alOxideSens AI di ispezionare sessioni live ed eseguire azioni workspace approvate tramite il tuo provider AI

---

## Perché OxideTerm Native?

| Se ti interessa... | OxideTerm Native offre... |
|---|---|
| Un nodo remoto, molti strumenti | Terminale, SFTP, port forwarding, trzsz, IDE nativo, monitoraggio e OxideSens AI restano legati allo stesso workspace SSH |
| Shell nativa zero-WebView | GPUI disegna la UI desktop direttamente su una superficie GPU, senza DOM, CSS, JavaScript, Chromium o runtime WebKit |
| Workflow SSH local-first | SSH, SFTP, forwarding, shell locale, terminali seriali e configurazione funzionano senza registrazione |
| OxideSens AI BYOK invece di crediti piattaforma | OxideSens usa il tuo endpoint OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible con MCP, RAG e azioni workspace approvate |
| Riconnessione stabile | Grace Period prova la vecchia connessione per 30 s prima di sostituirla, così le TUI possono sopravvivere a brevi interruzioni |
| SSH puro Rust e credenziali sicure | `russh` + `ring`, niente OpenSSL/libssh2; password e chiavi API restano nel portachiavi OS, `.oxide` usa ChaCha20-Poly1305 + Argon2id |

## Cos'è / cosa non è

OxideTerm Native si concentra su un **workspace AI local-first per server remoti**, ricostruito come app desktop GPUI in Rust puro. È pensato per chi vuole tenere terminali, file, porte, trasferimenti, editing leggero, console seriali e una OxideSens AI attorno alle proprie macchine e nodi remoti.

Non è ancora la linea stabile di download attuale, né una piattaforma cloud Agent. Non è nemmeno Electron, Tauri o terminale web: niente Chromium, niente WebView, niente JavaScript, niente CSS.

---

## Screenshot

La UI nativa segue lo stesso modello di workspace OxideTerm e lo stesso linguaggio visivo della linea Tauri attuale.

<table>
<tr>
<td align="center"><strong>Terminale SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminale SSH con OxideSens AI" /></td>
<td align="center"><strong>Gestore file SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Gestore file SFTP a doppio pannello con coda di trasferimento" /></td>
</tr>
<tr>
<td align="center"><strong>IDE integrato</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Modalità IDE integrata" /></td>
<td align="center"><strong>Port forwarding intelligente</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Port forwarding intelligente con rilevamento automatico" /></td>
</tr>
</table>

---

## Differenze da WebView/Tauri

| Aspetto | WebView/Tauri | Native |
|---|---|---|
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI, GPU surface, immediate mode, pure Rust |
| Terminal data flow | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC per command | Chiamate in-process |
| SSH keepalive | Timer JavaScript | Rust async task |
| Plugin runtime | ESM in browser sandbox | WASM wasmtime + typed Rust host API |
| CLI | Richiede la desktop app | Binario standalone |
| Dimensione artefatto | Installer di solito ~150–200 MB | macOS arm64 attuale: portable/DMG compresso ~50–60 MB; binario release grezzo ~132 MB |

## Funzionalità

| Categoria | Funzioni |
|---|---|
| Terminal | Local PTY, SSH, local serial terminals, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens con OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG e approvazione comandi |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, import/export cifrato |
| Plugins / CLI | WASM sandbox, native host API, plugin settings; CLI per settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Architettura

OxideTerm Native rimuove il bridge WebView e mantiene terminale, SSH, SFTP, forwarding, IDE, AI, plugin e CLI in una architettura Rust-native. I dettagli completi sono conservati sotto.

<details>
<summary><strong>Architettura, internals SSH, shell GPUI, riconnessione, AI, plugin e altro</strong></summary>
<br>

### Architettura — processo singolo, zero bridge

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

Non c'è confine di serializzazione tra UI e backend SSH/terminal. I byte del terminale modificano direttamente `TerminalState`; GPUI legge lo stato ed emette draw call GPU.

### SSH puro Rust — russh (ring)

L’edizione nativa collega direttamente nel binario desktop lo stesso stack `russh` della linea Tauri:

- **Zero dipendenze C/OpenSSL** tramite `ring`
- SSH2 completo: key exchange, channels, sottosistema SFTP, port forwarding
- ChaCha20-Poly1305 / AES-GCM, chiavi Ed25519/RSA/ECDSA
- SSH Agent su Unix (`SSH_AUTH_SOCK`) e Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop con autenticazione indipendente per ogni hop

### Smart Reconnect con Grace Period

La semantica di reconnect coincide con la linea Tauri, ma l’orchestrazione gira interamente in task async Rust:

1. Rilevare SSH keepalive timeout senza JavaScript timer throttling
2. Creare snapshot di terminal panes, trasferimenti SFTP, forwards e file IDE
3. Sondare la vecchia connessione per 30 secondi di Grace Period, così le TUI possono sopravvivere ai cambi rete
4. Se il recupero fallisce, riconnettere, ripristinare forwards, riprendere transfer e riaprire file IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### Pool di connessioni SSH e routing per nodo

`SshConnectionRegistry` usa `DashMap` e conserva il modello node-first di Tauri senza WebSocket lifecycle bridge:

- Una connessione SSH fisica può servire terminal panes, SFTP, port forwards e lavoro IDE
- Ogni connessione passa per `connecting → active → idle → link_down → reconnecting`
- La UI indirizza `nodeId`; `NodeRouter` risolve atomicamente il `connectionId` attivo
- `NodeRuntimeStore` persiste snapshot della topologia in `session_tree.json`
- Il fallimento di un jump host propaga `link_down` ai nodi downstream

### OxideSens AI

OxideSens resta BYOK-first, con context building dentro il processo:

- Provider: OpenAI, Anthropic, Gemini, Ollama o qualsiasi endpoint OpenAI-compatible
- MCP: transport stdio e SSE, tool discovery e invocation
- RAG: BM25 full-text, indice vettoriale HNSW, Reciprocal Rank Fusion, tokenizer CJK bigram
- Il contesto AI arriva dallo stato del workspace; le credenziali vengono redatte prima delle chiamate provider
- Le API key restano nel keychain OS e non entrano in log o IPC frames

### Shell desktop GPUI

La UI è disegnata direttamente con GPUI, senza pipeline DOM/CSS/JavaScript:

- 17 tipi di tab workspace: terminal locale/SSH, SFTP, IDE, Forwards, Settings, Plugin, Topology e altro
- Binary pane tree con divider trascinabili, fino a quattro panes per tab terminale
- Command palette, scorciatoie globali e sidebars costruite con primitive GPUI
- Immediate-mode rendering reagisce allo stato Rust senza round-trip di serializzazione

### Stato del terminale e rendering

Il rendering del terminale viene prima modellato come stato Rust e poi disegnato da GPUI:

- L’output PTY entra in `TerminalState`; scrollback, cursore, selezione, marks e stato di ricerca restano in Rust
- La rendering policy può passare tra Boost, Normal e Idle senza aspettare un browser event loop
- Le grafiche Sixel e Kitty sono tracciate come asset del terminale, non come DOM nodes o canvas overlays
- Split panes condividono lo stesso workspace state model, quindi tab restore e reconnect possono snapshotare insieme la topologia del terminale

### Workspace SFTP e IDE

I file remoti fanno parte dello stesso node workspace, non di una funzione separata:

- Le sessioni SFTP sono risolte tramite `NodeRouter`, così reconnect può cambiare la connessione SSH sottostante senza modificare il node address della UI
- Le transfer queues tracciano direction, progress, retry state e speed limits indipendentemente dai file panes visibili
- Le tab IDE tengono insieme dirty buffers, remote paths, conflict state e restore metadata
- Quando il backend lo supporta, le scritture remote usano staged/atomic behavior per evitare partial writes nei normali edit flow

### Plugins, CLI e diagnostics

Il branch native mantiene estensioni e superfici di supporto dentro confini Rust-native:

- I plugins girano in una sandbox wasmtime con typed host capabilities invece di browser globals
- La CLI linka direttamente domain crates per doctor, settings, connections, forwards, portable bundles, backups e reports
- Diagnostics preferisce counts, paths, feature flags e redacted hints rispetto a raw payloads con segreti
- I CLI flows mutanti usano dry-run plans, `--yes` guards e rollback backups quando applicabile

### Port forwarding — Lock-Free I/O

Forwarding mantiene la semantica Tauri in un crate Rust autonomo:

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Un singolo task `ssh_io` possiede ogni SSH Channel ed evita `Arc<Mutex<Channel>>`
- Auto-restore al reconnect, death reporting e idle timeout

### trzsz — trasferimento in-band

trzsz continua a usare lo stream del terminale, senza porta extra o agent remoto:

- Upload/download attraverso lo stream terminale esistente
- Funziona attraverso catene ProxyJump
- File picker nativi evitano i limiti di memoria del browser
- Trasferimento bidirezionale, supporto directory, limiti configurabili

### Export `.oxide` cifrato

Il formato bundle cifrato coincide con la linea Tauri:

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, aumenta il costo brute-force GPU
- Copre connections, forwards, settings, quick commands, plugin settings e portable secrets

</details>

---

## Eseguire dal sorgente

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

## Sicurezza

| Tema | Implementazione |
|---|---|
| Password & keys | macOS Keychain / Windows Credential Manager / libsecret |
| Memoria segreta | `zeroize` / `Zeroizing` |
| Diagnostica & contesto AI | valori segreti redatti prima di output o provider call |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Scritture CLI | dry-run plans, guardie `--yes`, rollback backups |
| Plugins | isolamento wasmtime e capability-based host API |

## Stato release

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] Terminal data flow in-process senza WebSocket
- [x] SFTP, forwarding, IDE, AI, cloud sync, plugins, CLI
- [x] Terminali seriali locali
- [x] Full ProxyCommand
- [ ] Audit logging

## Contribuire

## Neutralità dei provider

OxideTerm è BYOK-first e neutrale rispetto ai provider.

Le integrazioni dei provider esistono per aiutare gli utenti a collegare gli strumenti di cui già si fidano. Non sono una classifica, uno spazio pubblicitario o un sistema di ricompensa per chi chiede attenzione con più entusiasmo.

Compatibilità, manutenibilità, sicurezza e valore reale per gli utenti decidono cosa viene documentato. La visibilità segue l'utilità, non l'entusiasmo.

Quando una funzione esiste già nella versione Tauri, mantieni comportamento, label, stati di interazione e workflow allineati. Ogni nuovo crate deve avere una responsabilità reale, non solo re-export.

## Supporto e manutenzione

I bug report e le regressioni riproducibili con diagnostica redatta hanno priorità. Le richieste di funzionalità vengono valutate in base ad ambito, sicurezza e allineamento con la direzione di OxideTerm per il workspace dei server remoti.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Se OxideTerm aiuta il tuo workflow, una star GitHub, una riproduzione issue, una correzione di traduzione, un plugin o una pull request aiutano il progetto a proseguire.

---

## Licenza / Ringraziamenti

**GPL-3.0-only**. Le notice di terze parti sono in `NOTICE`. Grazie a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
