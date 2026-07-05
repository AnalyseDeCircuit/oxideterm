<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Spazio di lavoro operativo nativo con IA per server remoti — app nativa in Rust puro</strong>
  <br>
  Terminali SSH, Telnet, seriali, RDP/VNC, SFTP, inoltro porte, Raw TCP/UDP e modifica leggera in uno spazio di lavoro nativo.
  <br>
  Rendering su GPU. Gratis. Nessun account necessario.
  <br>
  <strong>Senza WebView. Senza OpenSSL. Senza telemetria. Senza abbonamento. BYOK prima di tutto. SSH puro in Rust.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.13-blue" alt="Versione">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Piattaforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licenza">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Prossima grande edizione nativa di <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — renderizzata su GPU, senza WebView, usando <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (framework di rendering di Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="Panoramica delle funzioni di OxideTerm Native" width="920">
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens apre un terminale dentro OxideTerm" width="920">
</a>

*OxideSens segue una richiesta dell’utente e apre un terminale dentro OxideTerm.*

</div>

---

## Cosa puoi fare

- Gestire SSH, Telnet, seriale, RDP/VNC, SFTP, inoltro porte, Raw TCP/UDP, shell locali e modifica leggera in uno spazio di lavoro nativo
- Mantenere vivo il lavoro remoto durante problemi di rete con la riconnessione Grace Period
- Chiedere a OxideSens AI di ispezionare sessioni attive ed eseguire azioni approvate nello spazio di lavoro tramite il tuo fornitore IA

---

## Perché OxideTerm Native?

| Se ti interessa... | OxideTerm Native offre... |
|---|---|
| Un nodo remoto, molti strumenti | Terminale, SFTP, inoltro porte, RDP/VNC, Raw TCP/UDP, trzsz, IDE nativo, monitoraggio e OxideSens AI restano legati allo stesso spazio di lavoro |
| Shell nativa senza WebView | GPUI disegna l’interfaccia desktop direttamente su una superficie GPU, senza DOM, CSS, JavaScript, Chromium o runtime WebKit |
| Flussi operativi locali prima di tutto | SSH, Telnet, SFTP, inoltro, RDP/VNC, Raw TCP/UDP, shell locale, terminali seriali e configurazione funzionano senza registrazione |
| OxideSens AI con BYOK invece di crediti piattaforma | OxideSens usa il tuo punto di accesso OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible con MCP, RAG e azioni approvate nello spazio di lavoro |
| Riconnessione stabile | Grace Period prova la vecchia connessione per 30 s prima di sostituirla, così le TUI possono sopravvivere a brevi interruzioni |
| SSH puro Rust e credenziali sicure | `russh` + `ring`, niente OpenSSL/libssh2; password e chiavi API restano nel portachiavi OS, `.oxide` usa ChaCha20-Poly1305 + Argon2id |

## Cos'è / cosa non è

OxideTerm Native si concentra su uno **spazio di lavoro IA locale prima di tutto per server remoti**, ricostruito come app desktop GPUI in Rust puro. È pensato per chi vuole tenere terminali, desktop remoti, socket grezzi, file, porte, trasferimenti, modifica leggera, console seriali e OxideSens AI attorno alle proprie macchine e nodi remoti.

Non è una piattaforma di agenti ospitata nel cloud. Non è nemmeno Electron, Tauri o un terminale web: niente Chromium, niente WebView, niente JavaScript, niente CSS.

---

## Screenshot

L’interfaccia nativa segue lo stesso modello di spazio di lavoro OxideTerm e lo stesso linguaggio visivo della linea Tauri attuale.

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
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI, superficie GPU, modalità immediata, Rust puro |
| Flusso dati del terminale | WebSocket → JS ciclo eventi → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC per comando | Chiamate nel processo |
| SSH keepalive | Timer JavaScript | Rust async task |
| Ambiente plugin | ESM in sandbox del browser | WASM wasmtime + API host Rust tipizzata |
| CLI | Richiede la desktop app | Binario standalone |
| Confine runtime | Runtime browser + bridge WebView | Processo nativo; nessun runtime browser incluso |

## Funzionalità

| Categoria | Funzioni |
|---|---|
| Terminal | PTY locale, SSH, Telnet, terminali Raw TCP/UDP, terminali seriali locali, pannelli divisi, integrazione shell, comando marks, asciicast, trzsz, Sixel/Kitty graphics, politica di rendering |
| SSH & Auth | pool di connessioni, ProxyJump illimitato, riconnessione Grace Period, TOFU della chiave host, inoltro SSH Agent, password/key/cert/keyboard-interactive |
| SFTP / IDE | browser a due pannelli, coda trasferimenti, anteprima, segnalibri, scritture atomiche, albero file remoto, editor multi-tab, risoluzione conflitti |
| Forwarding | Local, Remote, Dynamic SOCKS5, regole salvate, ripristino alla riconnessione, notifica di terminazione, timeout inattività |
| Desktop remoto | Tab RDP e VNC integrati, controlli di riconnessione, dimensioni in base al viewport, tastiera, mouse, appunti e cursore |
| Raw TCP/UDP | Terminali Raw TCP e Raw UDP per debug di servizi temporanei, protocolli di dispositivi e datagrammi |
| AI | OxideSens con OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG e approvazione comandi |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, import/export cifrato |
| Plugins / CLI | WASM sandbox, API host nativa, impostazioni plugin; CLI per settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Architettura

OxideTerm Native rimuove il bridge WebView e mantiene terminale, SSH, Telnet, RDP, VNC, Raw TCP/UDP, SFTP, forwarding, IDE, AI, plugin e CLI in una architettura Rust-native. I dettagli completi sono conservati sotto.

<details>
<summary><strong>Architettura, internals SSH, shell GPUI, riconnessione, AI, plugin e altro</strong></summary>
<br>

### Architettura — processo singolo, zero bridge

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ nel processo Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

Non c'è confine di serializzazione tra UI e backend SSH/terminal. I byte del terminale modificano direttamente `TerminalState`; GPUI legge lo stato ed emette draw call GPU.

### SSH puro Rust — russh (ring)

L’edizione nativa collega direttamente nel binario desktop lo stesso stack `russh` della linea Tauri:

- **Zero dipendenze OpenSSL** tramite `ring`
- SSH2 completo: key exchange, channels, sottosistema SFTP, inoltro porte
- ChaCha20-Poly1305 / AES-GCM, chiavi Ed25519/RSA/ECDSA
- SSH Agent su Unix (`SSH_AUTH_SOCK`) e Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop con autenticazione indipendente per ogni hop

### Smart Reconnect con Grace Period

La semantica di reconnect coincide con la linea Tauri, ma l’orchestrazione gira interamente in task async Rust:

1. Rilevare SSH keepalive timeout senza JavaScript timer throttling
2. Creare snapshot di pannelli terminale, trasferimenti SFTP, forwards e file IDE
3. Sondare la vecchia connessione per 30 secondi di Grace Period, così le TUI possono sopravvivere ai cambi rete
4. Se il recupero fallisce, riconnettere, ripristinare forwards, riprendere transfer e riaprire file IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### Pool di connessioni SSH e routing per nodo

`SshConnectionRegistry` usa `DashMap` e conserva il modello node-first di Tauri senza WebSocket lifecycle bridge:

- Una connessione SSH fisica può servire pannelli terminale, SFTP, inoltri porte e lavoro IDE
- Ogni connessione passa per `connecting → active → idle → link_down → reconnecting`
- La UI indirizza `nodeId`; `NodeRouter` risolve atomicamente il `connectionId` attivo
- `NodeRuntimeStore` persiste snapshot della topologia in `session_tree.json`
- Il fallimento di un jump host propaga `link_down` ai nodi downstream

### OxideSens AI

OxideSens resta BYOK prima di tutto, con costruzione del contesto dentro il processo:

- Fornitore: OpenAI, Anthropic, Gemini, Ollama o qualsiasi punto di accesso OpenAI-compatible
- MCP: transport stdio e SSE, tool discovery e invocation
- RAG: BM25 full-text, indice vettoriale HNSW, Reciprocal Rank Fusion, tokenizer CJK bigram
- Il contesto AI arriva dallo stato del spazio di lavoro; le credenziali vengono redatte prima delle chiamate fornitore
- Le API key restano nel portachiavi del sistema operativo e non entrano in log o frame IPC

### Shell desktop GPUI

La UI è disegnata direttamente con GPUI, senza pipeline DOM/CSS/JavaScript:

- Tipi di tab dello spazio di lavoro: terminali locali, SSH, Telnet, seriali, RDP, VNC e Raw TCP/UDP, SFTP, IDE, Forwards, Settings, plugin, Topology e altro
- Binary pane tree con divider trascinabili, fino a quattro panes per tab terminale
- Command palette, scorciatoie globali e sidebars costruite con primitive GPUI
- Immediate-mode rendering reagisce allo stato Rust senza round-trip di serializzazione

### Stato del terminale e rendering

Il rendering del terminale viene prima modellato come stato Rust e poi disegnato da GPUI:

- L’output PTY entra in `TerminalState`; scrollback, cursore, selezione, marks e stato di ricerca restano in Rust
- La politica di rendering può passare tra Boost, Normal e Idle senza aspettare un browser ciclo eventi
- Le grafiche Sixel e Kitty sono tracciate come asset del terminale, non come DOM nodes o canvas overlays
- Pannelli divisi condividono lo stesso spazio di lavoro modello di stato, quindi ripristino scheda e reconnect possono snapshotare insieme la topologia del terminale

### Workspace SFTP e IDE

I file remoti fanno parte dello stesso node spazio di lavoro, non di una funzione separata:

- Le sessioni SFTP sono risolte tramite `NodeRouter`, così reconnect può cambiare la connessione SSH sottostante senza modificare il node address della UI
- Le coda trasferimentis tracciano direction, progress, retry state e speed limits indipendentemente dai file panes visibili
- Le tab IDE tengono insieme dirty buffers, remote paths, conflict state e restore metadata
- Quando il backend lo supporta, le scritture remote usano staged/atomic behavior per evitare partial writes nei normali edit flow

### Plugins, CLI e diagnostics

Il branch native mantiene estensioni e superfici di supporto dentro confini Rust-native:

- I plugin girano in una sandbox wasmtime con capacità host tipizzate invece dei globali del browser
- La CLI linka direttamente crate di dominio per doctor, settings, connections, forwards, portable bundles, backups e reports
- Diagnostica preferisce conteggi, percorsi, flag funzionali e indizi redatti rispetto a payload grezzi con segreti
- I CLI flows mutanti usano dry-run plans, `--yes` guards e rollback backups quando applicabile

### Port forwarding — Lock-Free I/O

Forwarding mantiene la semantica Tauri in un crate Rust autonomo:

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Un singolo task `ssh_io` possiede ogni SSH Channel ed evita `Arc<Mutex<Channel>>`
- Auto-restore al reconnect, notifica di terminazione e timeout inattività

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
- Copre connections, forwards, settings, comandi rapidi, impostazioni plugin e segreti portabili

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
| Password e chiavi | macOS Keychain / Windows Credential Manager / libsecret |
| Memoria segreta | `zeroize` / `Zeroizing` |
| Diagnostica & contesto AI | valori segreti redatti prima di output o chiamate al fornitore |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Scritture CLI | dry-run plans, guardie `--yes`, rollback backups |
| Plugins | isolamento wasmtime e basata su capacità API host |

## Stato release

- [x] inoltro SSH Agent, riconnessione Grace Period, GPUI desktop shell
- [x] Flusso dati del terminale nel processo senza WebSocket
- [x] SFTP, forwarding, IDE, AI, sincronizzazione cloud, plugins, CLI
- [x] Terminali seriali locali e Telnet
- [x] Desktop remoto RDP/VNC e terminali Raw TCP/UDP
- [x] Full ProxyCommand
- [ ] Audit logging

## Contribuire

## Neutralità dei fornitore

OxideTerm è BYOK prima di tutto e neutrale rispetto ai fornitore.

Le integrazioni dei fornitore esistono per aiutare gli utenti a collegare gli strumenti di cui già si fidano. Non sono una classifica, uno spazio pubblicitario o un sistema di ricompensa per chi chiede attenzione con più entusiasmo.

Compatibilità, manutenibilità, sicurezza e valore reale per gli utenti decidono cosa viene documentato. La visibilità segue l'utilità, non l'entusiasmo.

Quando una funzione esiste già nella versione Tauri, mantieni comportamento, label, stati di interazione e workflow allineati. Ogni nuovo crate deve avere una responsabilità reale, non solo re-export.

## Supporto e manutenzione

I segnalazioni di bug e le regressioni riproducibili con diagnostica redatta hanno priorità. Le richieste di funzionalità vengono valutate in base ad ambito, sicurezza e allineamento con la direzione di OxideTerm per il spazio di lavoro dei server remoti.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Se OxideTerm aiuta il tuo workflow, una star GitHub, una riproduzione issue, una correzione di traduzione, un plugin o una pull request aiutano il progetto a proseguire.

---

## Licenza / Ringraziamenti

**GPL-3.0-only**. Le notice di terze parti sono in `NOTICE`. Grazie a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
