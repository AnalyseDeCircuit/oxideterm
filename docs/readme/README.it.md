<p align="center">
  <img src="../../src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>Se ti piace OxideTerm, per favore metti una stella su GitHub! ⭐️</em>
</p>


<p align="center">
  <strong>Zero Electron. Zero OpenSSL. SSH puro in Rust.</strong>
  <br>
  <em>Un unico binario nativo — shell locali, SSH, SFTP, IDE remoto, IA, port forwarding, plugin, 30+ temi, 11 lingue.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.1.0--beta.5-blue" alt="Versione">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Piattaforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licenza">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=Scarica%20ultima%20versione&style=for-the-badge&color=brightgreen" alt="Scarica ultima versione">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=Scarica%20ultima%20beta&style=for-the-badge&color=orange" alt="Scarica ultima beta">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

> [!NOTE]
> **Cambio di licenza:** A partire dalla v1.0.0, OxideTerm ha cambiato la sua licenza da **PolyForm Noncommercial 1.0.0** a **GPL-3.0 (GNU General Public License v3.0)**. OxideTerm è ora completamente open source — puoi usarlo, modificarlo e distribuirlo liberamente secondo i termini della licenza GPL-3.0. Vedi il file [LICENSE](../../LICENSE) per i dettagli.

---

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI — «Apri un terminale locale ed esegui echo hello, world!»*

</div>

---

## Perché OxideTerm?

| Problema | La risposta di OxideTerm |
|---|---|
| I client SSH non supportano shell locali | **Motore ibrido**: PTY locale (zsh/bash/fish/pwsh/WSL2) + SSH remoto in un'unica finestra |
| Riconnessione = perdere tutto | **Riconnessione con periodo di grazia**: sonda la vecchia connessione per 30 s prima di chiuderla — i tuoi vim/htop/yazi sopravvivono |
| L'editing remoto richiede VS Code Remote | **IDE integrato**: CodeMirror 6 su SFTP con 30+ linguaggi, agente remoto opzionale (~1 MB) su Linux |
| Nessun riutilizzo delle connessioni SSH | **Multiplexing**: terminale, SFTP, inoltri, IDE condividono una singola connessione SSH tramite pool con conteggio dei riferimenti |
| Le librerie SSH dipendono da OpenSSL | **russh 0.59**: SSH puro in Rust compilato con `ring` — zero dipendenze C |
| App Electron da 100+ MB | **Tauri 2.0**: backend Rust nativo, binario da 25–40 MB |
| IA vincolata a un provider | **OxideSens**: 40+ strumenti, protocollo MCP, knowledge base RAG — funziona con OpenAI/Ollama/DeepSeek/qualsiasi API compatibile |
| Credenziali in file di configurazione in chiaro | **Solo portachiavi di sistema**: password e chiavi API mai scritte su disco; file `.oxide` cifrati con ChaCha20-Poly1305 + Argon2id |
| Strumenti dipendenti dal cloud che richiedono un account | **Local-first**: zero account, zero telemetria, zero sincronizzazione cloud — i tuoi dati restano sul tuo dispositivo. Porta la tua chiave AI |

---

## Screenshot

<table>
<tr>
<td align="center"><strong>Terminale SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminale SSH con barra laterale OxideSens AI" /></td>
<td align="center"><strong>Gestore file SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Gestore file SFTP a doppio pannello con coda di trasferimento" /></td>
</tr>
<tr>
<td align="center"><strong>IDE integrato (CodeMirror 6)</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Modalità IDE integrata con editor CodeMirror 6" /></td>
<td align="center"><strong>Port forwarding intelligente</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Port forwarding intelligente con rilevamento automatico" /></td>
</tr>
</table>

---

## Panoramica delle funzionalità

| Categoria | Funzionalità |
|---|---|
| **Terminale** | PTY locale (zsh/bash/fish/pwsh/WSL2), SSH remoto, pannelli divisi, broadcast input, registrazione/riproduzione sessioni (asciicast v2), rendering WebGL, 30+ temi + editor personalizzato, palette comandi (`⌘K`), modalità zen |
| **SSH e autenticazione** | Pool di connessioni e multiplexing, ProxyJump (salti illimitati) con grafo topologico, riconnessione automatica con periodo di grazia, Inoltro agente. Auth: password, chiave SSH (RSA/Ed25519/ECDSA), SSH Agent, certificati, 2FA interattivo da tastiera, Known Hosts TOFU |
| **SFTP** | Browser a doppio pannello, drag-and-drop, anteprima intelligente (immagini/video/audio/codice/PDF/hex/font), coda di trasferimento con progresso ed ETA, segnalibri, estrazione archivi |
| **Modalità IDE** | CodeMirror 6 con 30+ linguaggi, albero file + stato Git, multi-tab, risoluzione conflitti, terminale integrato. Agente remoto opzionale per Linux (9 architetture aggiuntive) |
| **Port forwarding** | Locale (-L), remoto (-R), SOCKS5 dinamico (-D), I/O message-passing senza lock, ripristino automatico alla riconnessione, report di terminazione, timeout di inattività |
| **IA (OxideSens)** | Pannello inline (`⌘I`) + chat laterale, cattura buffer terminale (pannello singolo/tutti), contesto multi-sorgente (IDE/SFTP/Git), 40+ strumenti autonomi, integrazione server MCP, knowledge base RAG (ricerca ibrida BM25 + vettori), streaming SSE |
| **Plugin** | Caricamento ESM runtime, 18 namespace API, 24 componenti UI Kit, API congelata + ACL Proxy, circuit breaker, disattivazione automatica in caso di errori |
| **CLI** | Companion `oxt`: JSON-RPC 2.0 tramite Unix Socket / Named Pipe, `status`/`list`/`ping`, output leggibile + JSON |
| **Sicurezza** | Export crittografato .oxide (ChaCha20-Poly1305 + Argon2id 256 MB), portachiavi OS, Touch ID (macOS), TOFU chiave host, pulizia memoria `zeroize` |
| **i18n** | 11 lingue: EN, 简体中文, 繁體中文, 日本語, 한국어, FR, DE, ES, IT, PT-BR, VI |

---

## Sotto il cofano

### Architettura — Comunicazione a doppio piano

OxideTerm separa i dati del terminale dai comandi di controllo in due piani indipendenti:

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│  xterm.js 6 (WebGL) + 19 stores    │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binario)
           │ (JSON)       │ porta per sessione
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│  NodeRouter → SshConnectionRegistry │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]       │
└─────────────────────────────────────┘
```

- **Piano dati (WebSocket)**: ogni sessione SSH ottiene la propria porta WebSocket. I byte del terminale fluiscono come frame binari con header Type-Length-Payload — nessuna serializzazione JSON, nessuna codifica Base64, zero overhead nel percorso critico.
- **Piano di controllo (Tauri IPC)**: gestione connessioni, operazioni SFTP, inoltri, configurazione — JSON strutturato, ma fuori dal percorso critico.
- **Indirizzamento per nodo**: il frontend non tocca mai `sessionId` né `connectionId`. Tutto viene indirizzato tramite `nodeId`, risolto atomicamente lato server dal `NodeRouter`. La riconnessione SSH modifica il `connectionId` sottostante — ma SFTP, IDE e inoltri non sono minimamente coinvolti.

### 🔩 SSH puro in Rust — russh 0.59

L'intero stack SSH è **russh 0.59** compilato con il backend crittografico **`ring`**:

- **Zero dipendenze C/OpenSSL** — l'intero stack crittografico è in Rust. Niente più debug «quale versione di OpenSSL?».
- Protocollo SSH2 completo: scambio chiavi, canali, sottosistema SFTP, port forwarding
- Suite crittografiche ChaCha20-Poly1305 e AES-GCM, chiavi Ed25519/RSA/ECDSA
- **`AgentSigner`** personalizzato: avvolge il SSH Agent di sistema e implementa il trait `Signer` di russh, risolvendo problemi di bound `Send` RPITIT clonando `&AgentIdentity` in un valore posseduto prima di attraversare `.await`

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **Supporto piattaforma**: Unix (`SSH_AUTH_SOCK`), Windows (`\\.\pipe\openssh-ssh-agent`)
- **Catene proxy**: ogni salto utilizza l'autenticazione Agent in modo indipendente
- **Riconnessione**: `AuthMethod::Agent` riprodotto automaticamente

### 🔄 Riconnessione intelligente con periodo di grazia

La maggior parte dei client SSH distrugge tutto alla disconnessione e riparte da zero. L'orchestratore di riconnessione di OxideTerm adotta un approccio fondamentalmente diverso:

1. **Rilevamento** del timeout heartbeat WebSocket (300 s, calibrato per macOS App Nap e throttling dei timer JS)
2. **Snapshot** dello stato completo: pannelli terminale, trasferimenti SFTP in corso, port forward attivi, file IDE aperti
3. **Sondaggio intelligente**: eventi `visibilitychange` + `online` attivano keepalive SSH proattivo (~2 s di rilevamento contro 15–30 s di timeout passivo)
4. **Periodo di grazia** (30 s): sonda la vecchia connessione SSH via keepalive — se si ripristina (es.: cambio di access point WiFi), le tue app TUI (vim, htop, yazi) sopravvivono completamente intatte
5. Se il recupero fallisce → nuova connessione SSH → ripristino automatico degli inoltri → ripresa dei trasferimenti SFTP → riapertura dei file IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

Tutta la logica passa attraverso un `ReconnectOrchestratorStore` dedicato — zero codice di riconnessione sparso in hook o componenti.

### 🛡️ Pool di connessioni SSH

`SshConnectionRegistry` con conteggio dei riferimenti supportato da `DashMap` per accesso concorrente senza lock:

- **Una connessione, molti consumatori**: terminale, SFTP, port forward e IDE condividono una singola connessione SSH fisica — nessun handshake TCP ridondante
- **Macchina a stati per connessione**: `connecting → active → idle → link_down → reconnecting`
- **Gestione del ciclo di vita**: timeout di inattività configurabile (5 min / 15 min / 30 min / 1 h / mai), intervallo keepalive di 15 s, rilevamento guasti heartbeat
- **Heartbeat WsBridge**: intervallo di 30 s, timeout di 5 min — tollera macOS App Nap e throttling JS del browser
- **Propagazione a cascata**: guasto dell'host di salto → tutti i nodi a valle automaticamente marcati come `link_down` con sincronizzazione dello stato
- **Disconnessione per inattività**: emette `connection_status_changed` al frontend (non solo `node:state` interno), prevenendo desincronizzazione dell'interfaccia

### 🤖 OxideSens AI

Assistente IA incentrato sulla privacy con due modalità di interazione:

- **Pannello inline** (`⌘I`): comandi terminale rapidi, output iniettato tramite bracketed paste
- **Chat laterale**: conversazioni persistenti con cronologia completa
- **Cattura del contesto**: il Terminal Registry raccoglie il buffer dal pannello attivo o da tutti i pannelli divisi simultaneamente; iniezione automatica di file IDE, percorsi SFTP e stato Git
- **40+ strumenti autonomi**: operazioni sui file, gestione processi, diagnostica di rete, interazione con app TUI, elaborazione testi — l'IA invoca questi strumenti senza attivazione manuale
- **Supporto MCP**: connessione a server [Model Context Protocol](https://modelcontextprotocol.io) esterni (stdio e SSE) per integrazione di strumenti di terze parti
- **Knowledge base RAG** (v0.20): importa documenti Markdown/TXT in collezioni con scope (globale o per connessione). La ricerca ibrida fonde indice di keyword BM25 + similarità coseno vettoriale tramite Reciprocal Rank Fusion. Chunking consapevole del Markdown che preserva la gerarchia dei titoli. Tokenizer a bigrammi CJK per cinese/giapponese/coreano.
- **Provider**: OpenAI, Ollama, DeepSeek, OneAPI, o qualsiasi endpoint `/v1/chat/completions`
- **Sicurezza**: chiavi API conservate nel portachiavi OS; su macOS, la lettura delle chiavi è protetta da **Touch ID** tramite `LAContext` — nessun entitlement o firma del codice richiesti, in cache dopo la prima autenticazione per sessione

### 💻 Modalità IDE — Editing remoto

Editor CodeMirror 6 che opera su SFTP — nessuna installazione lato server richiesta di default:

- **Albero file**: caricamento lazy delle directory con indicatori di stato Git (modificato/non tracciato/aggiunto)
- **24 modalità linguaggio**: 14 native CodeMirror + modalità legacy tramite `@codemirror/legacy-modes`
- **Risoluzione conflitti**: locking ottimistico per mtime — rileva modifiche remote prima della sovrascrittura
- **Git event-driven**: aggiornamento automatico al salvataggio, creazione, eliminazione, rinominazione e pressione del tasto Invio nel terminale
- **State Gating**: tutte le I/O bloccate quando `readiness !== 'ready'`, Key-Driven Reset forza il remount completo alla riconnessione
- **Agente remoto** (opzionale): binario Rust di ~1 MB, deployment automatico su x86_64/aarch64 Linux. 9 architetture aggiuntive (ARMv7, RISC-V64, LoongArch64, s390x, Power64LE, i686, ARM, Android aarch64, FreeBSD x86_64) in `agents/extra/` per upload manuale. Abilita albero file migliorato, ricerca simboli e sorveglianza file.

### 🔀 Port Forwarding — I/O senza lock

Inoltro locale (-L), remoto (-R) e SOCKS5 dinamico (-D) completo:

- **Architettura message-passing**: il canale SSH è posseduto da un singolo task `ssh_io` — nessun `Arc<Mutex<Channel>>`, eliminando completamente la contesa mutex
- **Report di terminazione**: i task di inoltro segnalano attivamente il motivo di uscita (disconnessione SSH, chiusura porta remota, timeout) per diagnostica chiara
- **Ripristino automatico**: gli inoltri `Suspended` riprendono automaticamente alla riconnessione senza intervento dell'utente
- **Timeout di inattività**: `FORWARD_IDLE_TIMEOUT` (300 s) previene l'accumulo di connessioni zombie

### 🔌 Sistema di plugin runtime

Caricamento ESM dinamico con superficie API congelata e rinforzata in sicurezza:

- **API PluginContext**: 18 namespace — terminal, ui, commands, settings, lifecycle, events, storage, system
- **24 componenti UI Kit**: componenti React precostruiti (pulsanti, campi di input, dialoghi, tabelle…) iniettati nelle sandbox dei plugin tramite `window.__OXIDE__`
- **Membrana di sicurezza**: `Object.freeze` su tutti gli oggetti di contesto, ACL basata su Proxy, whitelist IPC, circuit breaker con disattivazione automatica dopo errori ripetuti
- **Moduli condivisi**: React, ReactDOM, zustand, lucide-react esposti per l'uso dei plugin senza duplicazione dei bundle

### ⚡ Rendering adattivo

Scheduler di rendering a tre livelli che sostituisce il batching fisso di `requestAnimationFrame`:

| Livello | Trigger | Frequenza | Beneficio |
|---|---|---|---|
| **Boost** | Dati frame ≥ 4 KB | 120 Hz+ (ProMotion nativo) | Elimina il lag di scorrimento su `cat largefile.log` |
| **Normale** | Digitazione standard | 60 Hz (RAF) | Base fluida |
| **Inattivo** | 3 s senza I/O / tab nascosto | 1–15 Hz (decadimento esponenziale) | Carico GPU quasi nullo, risparmio batteria |

Le transizioni sono completamente automatiche — guidate dal volume dei dati, dall'input utente e dall'API Page Visibility. I tab in background continuano a svuotare i dati tramite timer di inattività senza svegliare RAF.

### 🔐 Export crittografato .oxide

Backup di connessione portatile e a prova di manomissione:

- Crittografia autenticata **ChaCha20-Poly1305 AEAD**
- **KDF Argon2id**: costo memoria di 256 MB, 4 iterazioni — resistente al brute-force GPU
- Checksum di integrità **SHA-256**
- **Embedding opzionale delle chiavi**: chiavi private codificate in base64 nel payload crittografato
- **Analisi preliminare**: breakdown dei tipi di autenticazione, rilevamento chiavi mancanti prima dell'export

### 📡 ProxyJump — Multi-hop con consapevolezza topologica

- Profondità della catena illimitata: `Client → Salto A → Salto B → … → Destinazione`
- Parsing automatico di `~/.ssh/config`, costruzione del grafo topologico, pathfinding Dijkstra per la rotta ottimale
- Nodi di salto riutilizzabili come sessioni indipendenti
- Propagazione di guasti a cascata: host di salto down → tutti i nodi a valle automaticamente marcati come `link_down`

### ⚙️ Terminale locale — PTY thread-safe

Shell locale multipiattaforma tramite `portable-pty 0.8`, protetto dal feature gate `local-terminal`:

- `MasterPty` avvolto in `std::sync::Mutex` — thread I/O dedicati mantengono le letture PTY bloccanti fuori dall'event loop di Tokio
- Rilevamento automatico della shell: `zsh`, `bash`, `fish`, `pwsh`, Git Bash, WSL2
- `cargo build --no-default-features` rimuove PTY per build mobile/leggeri

### 🪟 Ottimizzazione Windows

- **ConPTY nativo**: invoca direttamente l'API Windows Pseudo Console — supporto completo TrueColor e ANSI, nessun WinPTY legacy
- **Scanner shell**: rileva automaticamente PowerShell 7, Git Bash, WSL2, CMD tramite Registro e PATH

### E altro ancora

- **Profiler risorse**: CPU/memoria/rete in tempo reale tramite canale SSH persistente che legge `/proc/stat`, calcolo basato su delta, degradazione automatica a solo RTT su sistemi non-Linux
- **Motore temi personalizzato**: 30+ temi integrati, editor visuale con anteprima live, 20 campi xterm.js + 24 variabili colore UI, derivazione automatica dei colori UI dalla palette del terminale
- **Registrazione sessioni**: formato asciicast v2, registrazione e riproduzione complete
- **Broadcast input**: digita una volta, invia a tutti i pannelli divisi — operazioni batch sui server
- **Galleria sfondi**: immagini di sfondo per tab, 16 tipi di tab, controllo opacità/sfocatura/adattamento
- **Companion CLI** (`oxt`): binario di ~1 MB, JSON-RPC 2.0 tramite Unix Socket / Named Pipe, `status`/`list`/`ping` con output leggibile o `--json`
- **WSL Graphics** ⚠️ sperimentale: visualizzatore VNC integrato — 9 ambienti desktop + modalità singola applicazione, rilevamento WSLg, Xtigervnc + noVNC

<details>
<summary>📸 11 lingue in azione</summary>
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

## Avvio rapido

### Prerequisiti

- **Rust** 1.85+
- **Node.js** 18+ (pnpm consigliato)
- **Strumenti piattaforma**:
  - macOS: Xcode Command Line Tools
  - Windows: Visual Studio C++ Build Tools
  - Linux: `build-essential`, `libwebkit2gtk-4.1-dev`, `libssl-dev`

### Sviluppo

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# Compilare il CLI companion (necessario per le funzionalità CLI)
pnpm cli:build

# App completa (frontend + backend Rust con hot reload)
pnpm run tauri dev

# Solo frontend (Vite sulla porta 1420)
pnpm dev

# Build di produzione
pnpm run tauri build
```

---

## Stack tecnologico

| Livello | Tecnologia | Dettagli |
|---|---|---|
| **Framework** | Tauri 2.0 | Binario nativo, 25–40 MB |
| **Runtime** | Tokio + DashMap 6 | Completamente asincrono, mappe concorrenti senza lock |
| **SSH** | russh 0.59 (`ring`) | Puro Rust, zero dipendenze C, SSH Agent |
| **PTY locale** | portable-pty 0.8 | Feature-gated, ConPTY su Windows |
| **Frontend** | React 19.1 + TypeScript 5.8 | Vite 7, Tailwind CSS 4 |
| **Stato** | Zustand 5 | 19 store specializzati |
| **Terminale** | xterm.js 6 + WebGL | Accelerato da GPU, 60 fps+ |
| **Editor** | CodeMirror 6 | 30+ modalità linguaggio |
| **Crittografia** | ChaCha20-Poly1305 + Argon2id | AEAD + KDF ad alto consumo di memoria (256 MB) |
| **Storage** | redb 2.1 | Store KV embedded |
| **i18n** | i18next 25 | 11 lingue × 22 namespace |
| **Plugin** | ESM Runtime | PluginContext congelato + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## Sicurezza

| Aspetto | Implementazione |
|---|---|
| **Password** | Portachiavi OS (macOS Keychain / Windows Credential Manager / libsecret) |
| **Chiavi API IA** | Portachiavi OS + autenticazione biometrica Touch ID su macOS |
| **Export** | .oxide: ChaCha20-Poly1305 + Argon2id (256 MB di memoria, 4 iterazioni) |
| **Memoria** | Sicurezza della memoria di Rust + `zeroize` per la pulizia dei dati sensibili |
| **Chiavi host** | TOFU con `~/.ssh/known_hosts`, rifiuta le modifiche (prevenzione MITM) |
| **Plugin** | Object.freeze + ACL Proxy, circuit breaker, whitelist IPC |
| **WebSocket** | Token monouso con limiti di tempo |

---

## Roadmap

- [x] Forwarding dell'agente SSH
- [ ] Marketplace dei plugin
- [ ] Ricerca sessioni e cambio rapido

---

## Licenza

**GPL-3.0** — questo software è software libero rilasciato sotto la [Licenza Pubblica Generale GNU v3.0](https://www.gnu.org/licenses/gpl-3.0.html).

È possibile utilizzare, modificare e distribuire liberamente questo software secondo i termini della GPL-3.0. Qualsiasi opera derivata deve essere distribuita sotto la stessa licenza.

Testo completo: [Licenza Pubblica Generale GNU v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## Ringraziamenti

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>236.000+ righe di Rust e TypeScript — costruito con ⚡ e ☕</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
