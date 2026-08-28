<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>Spazio di lavoro operativo nativo con IA per server remoti — app nativa in Rust puro</strong>
  <br>
  Terminali SSH, Mosh, Telnet, seriali, RDP/VNC, SFTP, inoltro porte e modifica leggera in uno spazio di lavoro nativo.
  <br>
  Rendering su GPU. Gratis. Nessun account necessario.
  <br>
  <strong>Senza Electron. Senza WebView incorporata. Senza telemetria. Senza abbonamento. BYOK prima di tutto. SSH puro in Rust senza OpenSSL/libssh2.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.25-blue" alt="Versione">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Piattaforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licenza">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Open source, local-first e renderizzato su GPU con GPUI.</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="Panoramica delle funzioni di OxideTerm" width="920">
</p>

---

## Cos’è OxideTerm

OxideTerm è uno spazio di lavoro open source per SSH e operazioni remote. Terminali, file, port forwarding, strumenti host e desktop remoti restano in un unico spazio.

**Cosa puoi fare:**

- Gestire SSH, Mosh, Telnet, seriale, RDP/VNC, SFTP, port forwarding, shell locali e modifica leggera in un unico spazio di lavoro
- Mantenere attivo il lavoro remoto durante brevi interruzioni di rete con la riconnessione Grace Period
- Chiedere a OxideSens di esaminare le sessioni attive ed eseguire azioni approvate tramite il tuo provider AI

Connessioni e dati operativi rimangono sotto il tuo controllo. OxideSens usa il tuo provider AI e non richiede un account.

---

## Perché OxideTerm?

| Se per te conta… | OxideTerm offre… |
|---|---|
| Un nodo remoto, molti strumenti | Terminale, SFTP, port forwarding, RDP/VNC, trzsz, IDE nativo, monitoraggio e OxideSens AI restano nello stesso workspace |
| Un’app desktop senza Electron né WebView incluso | GPUI disegna l’interfaccia direttamente su una superficie GPU, senza distribuire un runtime browser |
| Flussi operativi local-first | SSH, Telnet, SFTP, forwarding, RDP/VNC, shell locale, terminali seriali e configurazione funzionano senza registrazione |
| OxideSens AI con le tue chiavi invece di crediti di piattaforma | OxideSens usa il tuo endpoint OpenAI, Anthropic, Gemini, Ollama o compatibile OpenAI, con MCP, RAG, controlli di ragionamento specifici per provider e azioni approvate del workspace |
| Stabilità della riconnessione | Grace Period verifica la vecchia connessione per 30 s prima di sostituirla, così le applicazioni TUI resistono a brevi cadute di rete |
| SSH Rust puro e sicurezza delle credenziali | Lo stack SSH usa `russh` + `ring` senza OpenSSL/libssh2; le credenziali salvate usano il portachiavi di sistema e i pacchetti `.oxide` usano ChaCha20-Poly1305 + Argon2id |

---

## Screenshot

Le schermate mostrano i flussi di terminale, file, modifica e forwarding di OxideTerm.

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

## Pensato per le operazioni remote

OxideTerm mantiene connessioni, file, forwarding, strumenti host, automazione e contesto AI in uno spazio Rust. Gli strumenti condividono la stessa identità server e lo stesso ciclo di sessione.

| Aspetto | Approccio con browser incluso | OxideTerm |
|---|---|---|
| **Rendering** | Motore browser e layout web | GPUI su una superficie GPU |
| **Flusso dati del terminale** | WebSocket → ciclo eventi JS → xterm.js | Input Rust → mutazione di `TerminalState` → rendering GPUI |
| **Ciclo di vita della connessione** | Diviso tra frontend e backend | Un’unica pipeline nel processo per connessione e riconnessione |
| **Contesto AI** | Copiato tramite un bridge applicativo | Creato dal workspace attivo con approvazione dell’utente |
| **Runtime dei plugin** | Ambiente di script del browser | Percorsi manifest-only, WASM con capacità limitate e processi che richiedono fiducia esplicita |
| **CLI** | Richiede l’app desktop in esecuzione | Binario autonomo con collegamento diretto ai crate |
| **Confine di runtime** | Wrapper desktop più runtime browser | Processo nativo senza runtime browser incorporato |

---

## Funzioni

| Categoria | Funzioni |
|---|---|
| **Terminale e connessioni** | Shell locali, SSH, Mosh, Telnet, seriale, pannelli, modalità di input libero, registri di sessione configurabili, modalità di controllo nativa tmux -CC con layout dei pannelli e separatori trascinabili, gruppi di broadcast con nome, invio avanzato a più destinazioni, percorsi multi-hop e riconnessione stabile |
| **File e modifica remota** | SFTP, code di trasferimento, preferiti, scritture sicure, alberi di progetto e modifica a schede |
| **Forwarding e rete** | Forwarding locale, remoto e SOCKS5 dinamico, regole salvate e debug dei socket |
| **Operazioni host e desktop remoto** | Monitoraggio, processi, servizi, log, porte, attività, dischi, pacchetti, container, tmux, RDP e VNC |
| **OxideSens e automazione** | Provider AI propri, MCP, RAG locale, Agent Skills, azioni approvate, sincronizzazione cifrata e CLI |
| **Estensioni e personalizzazione** | Plugin manifest-only, WASM e di processo, schede personalizzate, comandi rapidi, temi, sfondi, scorciatoie e 11 lingue |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens apre un terminale dentro OxideTerm" width="920">
</a>

*OxideSens segue una richiesta dell’utente e apre un terminale dentro OxideTerm.*

</div>

---

## Installazione

[Scarica l’ultima versione](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

- macOS: scegli il file `.dmg` per Apple Silicon o Intel.
- Windows: usa il programma di installazione x64 o ARM64.
- Linux: scegli AppImage, `.deb` o `.rpm`.
- Verifica i download con il file `sha256sums.txt` nella pagina della release.

Per compilare dal codice sorgente, consulta la sezione « Esegui dal codice sorgente » qui sotto.

---

## Architettura

OxideTerm rimuove il bridge WebView e mantiene terminale, SSH, Telnet, RDP, VNC, SFTP, forwarding, IDE, AI, plugin e CLI in una architettura Rust-native. I dettagli completi sono conservati sotto.

<details>
<summary><strong>Architettura, internals SSH, shell GPUI, riconnessione, AI, plugin e altro</strong></summary>
<br>

### Architettura — nucleo nello stesso processo, senza bridge WebView

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ nel processo Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardingRuntime / IdeWorkspace
  Ai/ACP Entities / CloudSync / Plugin Runtimes
```

Non c'è confine di serializzazione tra UI e backend SSH/terminal. I byte del terminale modificano direttamente `TerminalState`; GPUI legge lo stato ed emette draw call GPU.

### SSH puro Rust — russh (ring)


- **Niente OpenSSL/libssh2 nello stack SSH** — `ring` fornisce la crittografia SSH
- SSH2 completo: key exchange, channels, sottosistema SFTP, inoltro porte
- ChaCha20-Poly1305 / AES-GCM, chiavi Ed25519/RSA/ECDSA
- SSH Agent su Unix (`SSH_AUTH_SOCK`) e Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop con autenticazione indipendente per ogni hop

### Smart Reconnect con Grace Period


1. Rilevare SSH keepalive timeout senza JavaScript timer throttling
2. Creare snapshot di pannelli terminale, trasferimenti SFTP, forwards e file IDE
3. Sondare la vecchia connessione per 30 secondi di Grace Period, così le TUI possono sopravvivere ai cambi rete
4. Se il recupero fallisce, riconnettere, ripristinare i forwards, riprendere o ritentare i trasferimenti secondo la strategia e riaprire i file IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → retry-or-resume-transfers → restore-ide → verify → done`

### Pool di connessioni SSH e routing per nodo


- In modalità predefinita, una connessione SSH fisica può servire terminali, SFTP, inoltri porte e IDE; il terminale può usare una connessione dedicata quando la policy lo richiede
- Ogni connessione passa per `connecting → active → idle → link_down → reconnecting`
- La UI indirizza `nodeId`; `NodeRouter` risolve atomicamente il `connectionId` attivo
- `NodeRuntimeStore` conserva lo stato runtime dei nodi ed esporta snapshot della topologia; gli helper del workspace scrivono gli snapshot in `session_tree.json` e ricostruiscono gli handle attivi all’avvio
- Il fallimento di un jump host propaga `link_down` ai nodi downstream

### OxideSens AI

OxideSens resta BYOK prima di tutto, con costruzione del contesto dentro il processo:

- Fornitore: OpenAI, Anthropic, Gemini, Ollama o qualsiasi punto di accesso OpenAI-compatible
- MCP: transport stdio e SSE, tool discovery e invocation
- RAG: BM25 full-text, indice vettoriale HNSW, Reciprocal Rank Fusion, tokenizer CJK bigram
- I messaggi inviati ai provider passano attraverso la rimozione dei pattern di credenziali; contesto e azioni del workspace restano sotto il controllo dell’utente
- Le chiavi API sono conservate nel portachiavi di sistema ed escluse intenzionalmente dai log strutturati e dai messaggi del nucleo desktop

### Shell desktop GPUI

La UI è disegnata direttamente con GPUI, senza pipeline DOM/CSS/JavaScript:

- Tipi di tab dello spazio di lavoro: terminali locali, SSH, Telnet, seriali, RDP, VNC, SFTP, IDE, Forwards, Settings, plugin, Topology e altro
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

- Le sessioni SFTP sono risolte tramite `NodeRouter` e portano una generazione di connessione; reconnect può acquisire di nuovo una sessione valida, ma un’operazione della vecchia generazione non viene mai sostituita silenziosamente da una nuova connessione
- Le coda trasferimentis tracciano direction, progress, retry state e speed limits indipendentemente dai file panes visibili
- Le tab IDE tengono insieme dirty buffers, remote paths, conflict state e restore metadata
- Quando il backend lo supporta, le scritture remote usano staged/atomic behavior per evitare partial writes nei normali edit flow

### Plugins, CLI e diagnostics

Estensioni e superfici di supporto seguono confini espliciti definiti in Rust:

- I plugin supportano percorsi manifest-only, WASM e processi normali. WASM usa il runtime Wasmtime/WASI integrato; i plugin di processo sono processi locali senza sandbox del sistema operativo.
- La CLI linka direttamente crate di dominio per doctor, settings, connections, forwards, portable bundles, backups e reports
- Diagnostica preferisce conteggi, percorsi, flag funzionali e indizi redatti rispetto a payload grezzi con segreti
- I CLI flows mutanti usano dry-run plans, `--yes` guards e rollback backups quando applicabile

### Port forwarding — Lock-Free I/O


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


- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, aumenta il costo brute-force GPU
- Copre connections, forwards, settings, comandi rapidi, impostazioni plugin e segreti portabili

</details>

---

## Eseguire dal sorgente

**Requisiti:** toolchain Rust (edizione 2024) e un ambiente desktop in grado di eseguire GPUI.

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build/build-cli.sh
./scripts/build/build-agent.sh
```

## CLI

La CLI senza interfaccia `oxideterm` funziona senza avviare l’app ed è utile per automazione, CI e diagnostica.

```sh
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- forwards list --format json
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- oxide export ./profile.oxide --connection prod --password-stdin
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
cargo run -p oxideterm-cli -- completion install zsh --force
```

## Tecnologie

| Livello | Tecnologia | Note |
|---|---|---|
| Interfaccia | GPUI (Zed) | Modalità immediata accelerata dalla GPU, interamente in Rust |
| Runtime | Tokio + DashMap | Esecuzione asincrona e mappe concorrenti |
| SSH | russh (`ring`) | Niente OpenSSL/libssh2 nello stack SSH; SSH Agent |
| Terminale | portable-pty + alacritty_terminal | PTY locali, emulazione terminale e grafica Sixel/Kitty |
| Plugin | Wasmtime/WASI e processi | Manifest-only, chiamate host WASM controllate e processi locali che richiedono fiducia esplicita |
| AI e ricerca | SSE + BM25 + HNSW | Streaming dei provider, bigrammi CJK e fusione RRF |
| Editor | tree-sitter (sintassi), buffer personalizzato | Multilingua, supportato da SFTP |
| Crittografia | ChaCha20-Poly1305 + Argon2id | AEAD + KDF ad alta intensità di memoria (256 MB) |
| i18n | oxideterm-i18n | Caricatore integrato, 11 lingue distribuite |

## Sicurezza

| Tema | Implementazione |
|---|---|
| Credenziali memorizzate | macOS Keychain / Windows Credential Manager / libsecret |
| Segreti in memoria | I tipi che contengono segreti e i buffer temporanei usano `zeroize` / `Zeroizing` ai confini di proprietà supportati |
| Diagnostica | I report di supporto preferiscono metadati strutturati e indizi oscurati rispetto a payload contenenti segreti |
| Contesto AI | I messaggi inviati ai provider passano attraverso la rimozione dei pattern di credenziali; contesto e azioni del workspace restano sotto il controllo dell’utente |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Scritture CLI | dry-run plans, guardie `--yes`, rollback backups |
| Chiavi host | TOFU con `~/.ssh/known_hosts`, rifiuta modifiche inattese |
| Plugins | manifest-only, API host WASM controllata e processi locali che richiedono fiducia esplicita |

## Avviso sull’uso legittimo

OxideTerm è distribuito con licenza GPL-3.0-only senza ulteriori restrizioni di licenza. Durante l’utilizzo, accedere esclusivamente a sistemi, reti e dispositivi di proprietà dell’utente o per i quali si dispone di un’autorizzazione esplicita, nel rispetto delle leggi applicabili. Non utilizzare OxideTerm per accessi non autorizzati, interruzioni di servizi o per aggirare i controlli di accesso.

## Contribuire

Sono benvenuti contributi a codice, documentazione, traduzioni, plugin, test e segnalazioni. Discuti le modifiche più ampie o le correzioni ben delimitate in una issue.

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## Supporto e manutenzione

Le segnalazioni di bug e le regressioni riproducibili con diagnostica oscurata hanno priorità. Le richieste di funzionalità vengono valutate in base ad ambito, sicurezza e coerenza con la direzione di OxideTerm come spazio di lavoro per server remoti.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Se OxideTerm aiuta il tuo workflow, una star GitHub, una riproduzione issue, una correzione di traduzione o un plugin aiutano il progetto a proseguire.

---

## Licenza

**GPL-3.0-only**. Le informazioni dettagliate sui componenti di terze parti sono in [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), con ulteriori avvisi in [`NOTICE`](../../NOTICE).

## Ringraziamenti

Grazie a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
