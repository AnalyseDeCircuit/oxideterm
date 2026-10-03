<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm: i tuoi server, un unico spazio di lavoro" width="920">

# ⚡ OxideTerm

**Un client SSH nativo e gratuito e uno spazio di lavoro per le operazioni remote, con un assistente IA che usa la tua chiave.**

SSH · Mosh · Telnet · Seriale · RDP/VNC · SFTP · inoltro delle porte · editor integrato, in un'unica applicazione con rendering sulla GPU.
Nessun account. Nessun abbonamento. Nessuna telemetria. Niente Electron.

[![Ultima versione](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Piattaforme](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![Licenza](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Stelle](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Scarica**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Documentazione**](https://oxideterm.app) ·
[**Registro delle modifiche**](../../.github/release-notes/stable-changelog.md) ·
[**Segnala un problema**](https://github.com/AnalyseDeCircuit/oxideterm/issues)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md)

</div>

---

## Per iniziare

1. **Installa OxideTerm.** Scarica un pacchetto dall'[ultima versione](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest); trovi le indicazioni per ogni piattaforma nella sezione [Installazione](#install) più avanti.
2. **Aggiungi un server.** Apri il gestore delle sessioni e crea una connessione SSH, oppure importa gli host dal tuo `~/.ssh/config`.
3. **Connettiti.** Apri un terminale. Le chiavi degli host vengono verificate rispetto a `~/.ssh/known_hosts`.
4. **Usa il resto dello spazio di lavoro.** Apri SFTP, l'inoltro delle porte o l'editor integrato sullo stesso nodo. Per impostazione predefinita condividono una connessione SSH.
5. **Facoltativo: attiva l'IA.** Nelle impostazioni aggiungi il tuo servizio OpenAI, Anthropic, Gemini, Ollama o un endpoint compatibile con OpenAI per abilitare OxideSens.

Per una visita guidata, consulta la [documentazione](https://oxideterm.app).

---

## Funzionalità

| | |
|---|---|
| **Terminali e protocolli** | Shell locali, SSH, Mosh, Telnet, seriale, riquadri divisi, percorsi a più salti, agente SSH e inoltro dell'agente, autenticazione a due fattori e credenziali TOTP, inoltro X11, integrazione con la shell, marcatori dei comandi, log di sessione configurabili, registrazione, grafica Sixel e Kitty, trasferimenti trzsz |
| **tmux e invio simultaneo** | Modalità di controllo nativa `tmux -CC` con disposizione dei riquadri e divisori trascinabili, gruppi di invio simultaneo con nome e invio avanzato di comandi a più destinazioni per input programmabili e ripetibili |
| **Affidabilità** | La riconnessione con periodo di tolleranza mantiene attive le applicazioni TUI durante brevi interruzioni di rete, poi ripristina inoltri, trasferimenti e file aperti nell'editor |
| **File e modifica** | Gestore SFTP a due pannelli, code di trasferimento con limiti di velocità e tempo stimato, segnalibri ed editor remoto integrato con scritture sicure, gestione dei conflitti e ripristino dello spazio di lavoro |
| **Rete** | Inoltro locale, remoto e dinamico SOCKS5, regole salvate, rilevamento delle porte remote, topologia delle connessioni e diagnostica occasionale dei socket |
| **Desktop remoto** | RDP e VNC integrati con supporto per appunti e input |
| **Operazioni sugli host** | Monitoraggio di processi, servizi, log, porte, attività, dischi, pacchetti, container e tmux |
| **IA e automazione** | OxideSens con la tua chiave, MCP, RAG locale, Agent Skills, azioni approvate nello spazio di lavoro e CLI autonoma |
| **Revisione e audit** | Spazio facoltativo per notifiche e audit e registrazioni di sessione cifrate; entrambe le funzioni sono disattivate per impostazione predefinita |
| **Sincronizzazione e portabilità** | Sincronizzazione cloud cifrata e pacchetti portabili `.oxide` |
| **Personalizzazione** | Temi, immagini di sfondo, scorciatoie configurabili, comandi rapidi e 11 lingue dell'interfaccia |

---

## Perché scegliere OxideTerm

- **Gratuito, con priorità all'uso locale.** Nessun account, abbonamento o telemetria. Le tue connessioni e i tuoi dati operativi restano sotto il tuo controllo.
- **Uno spazio di lavoro per ogni server.** Terminale, SFTP, inoltri, RDP/VNC, editor, monitoraggio e IA si collegano allo stesso nodo, invece di comportarsi come strumenti separati.
- **Nativo, non un browser mascherato.** L'interfaccia viene disegnata direttamente sulla GPU con [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). Non usa Electron né include un WebView.
- **IA alle tue condizioni.** OxideSens usa il tuo fornitore e la tua chiave ed esegue solo le azioni che approvi.
- **Connessioni resistenti.** La riconnessione con periodo di tolleranza verifica la connessione precedente per 30 secondi prima di sostituirla, così le applicazioni TUI possono sopravvivere a brevi interruzioni di rete.
- **SSH interamente in Rust.** Lo stack SSH usa `russh` con `ring`, senza OpenSSL o libssh2.

---

## Uso della memoria

**La riscrittura nativa ha ridotto la memoria a riposo a circa un quarto della versione precedente su macOS e a circa un ottavo su Windows.** Queste sono le osservazioni registrate dal manutentore nel passaggio da Tauri 1.x a GPUI nativo 2.0:

| Piattaforma | Tauri 1.x (a riposo) | Nativo 2.0 (a riposo) | Riduzione |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | Circa 74% |
| Windows | 182.4 MB | 23.5 MB | Circa 87% |

Il totale della versione precedente comprende OxideTerm e i relativi processi WebView. La versione nativa non ha più bisogno di quei processi del browser.

![Confronto della memoria a riposo con schermate dei processi di sistema: Tauri 1.x e versione nativa 2.0](../../docs/screenshots/oxideterm-memory-comparison.png)

---

## Schermate

| Terminale SSH con OxideSens | Gestore di file SFTP |
|---|---|
| ![Terminale SSH con l'IA OxideSens](../../docs/screenshots/terminal/SSHTERMINAL.png) | ![Gestore SFTP a due pannelli con coda dei trasferimenti](../../docs/screenshots/sftp/sftp.png) |

| IDE integrato | Inoltro intelligente delle porte |
|---|---|
| ![Modalità IDE integrato](../../docs/screenshots/miniIDE/miniide.png) | ![Inoltro intelligente delle porte con rilevamento automatico](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png) |

<details>
<summary><b>Guarda OxideSens aprire un terminale da una richiesta in linguaggio naturale</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens apre un terminale all’interno di OxideTerm" width="720">
</a>

</details>

---

## IA OxideSens

OxideSens è un assistente facoltativo che può esaminare le sessioni attive ed eseguire azioni nello spazio di lavoro **solo dopo la tua approvazione**.

- **Usa la tua chiave.** Funziona con OpenAI, Anthropic (Claude), Google Gemini, Ollama e qualsiasi endpoint compatibile con OpenAI, con controlli del ragionamento adattati al fornitore. Non ci sono crediti della piattaforma.
- **MCP e Agent Skills.** Collega server MCP tramite stdio e SSE e carica Agent Skills con un ambito delimitato.
- **Base di conoscenza locale (RAG).** Ricerca a testo completo con BM25 e indice vettoriale.
- **Il contesto lo controlli tu.** Scegli quale contesto dello spazio di lavoro e quali azioni autorizzare; si applicano le regole della politica dei comandi.
- **Oscuramento delle credenziali.** I messaggi inviati a un fornitore passano attraverso un filtro che oscura i pattern delle credenziali.
- **Le chiavi restano nel portachiavi del sistema operativo** e sono escluse dai log strutturati.

---

<a id="plugins"></a>

## Plugin

OxideTerm supporta tre modalità per i plugin:

| Tipo | Esecuzione | Limiti |
|---|---|---|
| **Solo manifesto** | Estensioni dichiarative, senza codice | Nessun codice eseguibile |
| **WASM** | Wasmtime/WASI o processo ausiliario | Chiamate all'host controllate e limitate alle capacità concesse |
| **Processo** | Un normale processo locale | Codice locale attendibile, **senza** isolamento del sistema operativo |

I vecchi plugin ESM di Tauri (1.x) possono comparire nell'elenco, ma non vengono eseguiti dall'app nativa 2.x. Installa plugin di processo solo da fonti di cui ti fidi.

---

## Sicurezza e privacy

| Argomento | Funzionamento |
|---|---|
| **Credenziali salvate** | Portachiavi del sistema operativo: macOS Keychain, Windows Credential Manager, libsecret |
| **Segreti in memoria** | I tipi che contengono segreti e i buffer temporanei usano `zeroize` nei punti supportati di passaggio della proprietà |
| **Chiavi degli host** | Fiducia al primo utilizzo tramite `~/.ssh/known_hosts`; le modifiche inattese vengono rifiutate |
| **Esportazioni portabili** | I pacchetti `.oxide` usano ChaCha20-Poly1305 con Argon2id: 256 MB di memoria e 4 iterazioni |
| **Contesto IA** | Oscuramento dei pattern delle credenziali prima dell'invio a un fornitore; approvi tu il contesto e le azioni |
| **Registrazioni delle sessioni** | Disattivate per impostazione predefinita; conservate cifrate sul dispositivo ed escluse dalla sincronizzazione cloud; l'input da tastiera non viene acquisito |
| **Audit** | Disattivato per impostazione predefinita; i dati restano sul dispositivo e i dettagli sensibili sono cifrati |
| **Modifiche tramite CLI** | Piani di simulazione, conferme con `--yes` e backup per annullare i comandi che modificano lo stato |
| **Plugin** | Vedi [Plugin](#plugins) |
| **Telemetria** | Nessuna |

**Uso lecito.** OxideTerm è distribuito con licenza GPL-3.0-only, senza restrizioni aggiuntive. Accedi solo a sistemi, reti e dispositivi di tua proprietà o per i quali disponi di un'autorizzazione esplicita, e rispetta le leggi applicabili. Non usare OxideTerm per accessi non autorizzati, interruzioni dei servizi o aggiramento dei controlli di accesso.

---

## Limiti attuali

Preferiamo che tu li conosca prima dell'installazione:

- Solo per desktop: macOS, Windows e Linux. Non esiste un'app mobile.
- Il progetto si evolve rapidamente, con versioni frequenti. Consulta il [registro delle modifiche](../../.github/release-notes/stable-changelog.md) e i [problemi aperti](https://github.com/AnalyseDeCircuit/oxideterm/issues).
- Audit e registrazione delle sessioni richiedono un'attivazione esplicita e riflettono solo ciò che OxideTerm può osservare.
- I plugin di processo non sono isolati dal sistema operativo.
- Se il rendering non funziona sul tuo computer, prova il profilo di compatibilità: `OXIDETERM_RENDER_PROFILE=compatibility`.

---

<a id="for-developers"></a>

## Per gli sviluppatori

<details>
<summary><b>Eseguire dal codice sorgente</b></summary>

**Requisiti:** toolchain Rust (edizione 2024) e un ambiente desktop in grado di eseguire GPUI.

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

Con Nix: `nix build .#oxideterm`, `nix run .#oxideterm` oppure `nix develop`.

Gli eseguibili della CLI vengono generati in `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

</details>

<details>
<summary><b>Interfaccia a riga di comando</b></summary>

La CLI `oxideterm`, senza interfaccia grafica, funziona senza avviare l'app ed è utile per automazione, CI e diagnostica. Comprende impostazioni, connessioni, inoltri, plugin, comandi rapidi, segreti, pacchetti portabili, diagnostica, rapporti, piani batch, backup e sincronizzazione cloud.

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
<summary><b>Architettura</b></summary>

L'interfaccia e il backend terminale/SSH condividono un processo Rust; gli agenti remoti facoltativi e i processi ausiliari di piattaforma operano al di fuori di questo confine. I byte del terminale modificano direttamente `TerminalState` e GPUI esegue il rendering da quello stato, senza passaggi di analisi JSON, WebSocket, Base64 o xterm.js.

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

| Aspetto | Approccio con browser integrato | OxideTerm |
|---|---|---|
| Rendering | Motore del browser e layout web | GPUI su una superficie GPU |
| Flusso dei dati del terminale | WebSocket → ciclo degli eventi JS → xterm.js | Input Rust → `TerminalState` → rendering GPUI |
| Ciclo di vita della connessione | Diviso tra frontend e backend | Un unico flusso di connessione e riconnessione nel processo |
| Contesto IA | Copiato attraverso un ponte applicativo | Creato dallo spazio di lavoro attivo, con l'approvazione dell'utente |
| CLI | Richiede l'app desktop in esecuzione | Eseguibile autonomo, collegamento diretto ai crate |

**Pool di connessioni.** `SshConnectionRegistry` si basa su `DashMap` e viene usato tramite `NodeRouter`. Riquadri del terminale, SFTP, inoltri delle porte ed editor possono condividere una connessione SSH fisica per nodo; una politica del terminale permette invece di scegliere una connessione dedicata. Ogni connessione segue `connecting → active → idle → link_down → reconnecting`. Un guasto di un host di salto contrassegna i nodi successivi come `link_down`. IA e plugin usano riferimenti alle capacità e istantanee degli host, invece di registrarsi come utilizzatori delle connessioni.

**Riconnessione con periodo di tolleranza.**

1. Rilevare la scadenza del timeout di keepalive.
2. Salvare un'istantanea dei riquadri del terminale, dei trasferimenti SFTP, degli inoltri e dei file dell'editor.
3. Verificare la connessione precedente per 30 s, affinché le applicazioni TUI sopravvivano a brevi interruzioni di rete.
4. Aprire una nuova connessione, ripristinare gli inoltri, riprendere i trasferimenti e riaprire i file dell'editor.

Le sessioni SFTP hanno una generazione di connessione: dopo una riconnessione viene riacquisita una sessione idonea, ma un'operazione di una generazione precedente non viene mai spostata silenziosamente sulla nuova connessione.

**Inoltro delle porte.** Un crate autonomo con supporto per `-L`, `-R` e `-D` (SOCKS5). Un'unica attività `ssh_io` possiede ciascun canale SSH, quindi non c'è un mutex condiviso nel percorso critico.

**SSH interamente in Rust.** `russh` con `ring`: SSH2 completo, ChaCha20-Poly1305 e AES-GCM, chiavi Ed25519/RSA/ECDSA, agente SSH su Unix (`SSH_AUTH_SOCK`) e Windows (`\\.\pipe\openssh-ssh-agent`), e catene a più salti con autenticazione indipendente per ciascun salto.

**Tecnologie utilizzate**

| Livello | Tecnologia |
|---|---|
| Interfaccia | GPUI: framework di interfaccia di Zed basato sulla GPU |
| Ambiente di esecuzione | Tokio, DashMap |
| SSH | `russh` con `ring`, senza OpenSSL o libssh2 |
| PTY locale | `portable-pty`: ConPTY su Windows |
| Emulazione del terminale | `alacritty_terminal`: VT100–VT500, Sixel, grafica Kitty |
| Editor | Evidenziazione della sintassi con tree-sitter e buffer personalizzato |
| Cifratura | ChaCha20-Poly1305, Argon2id |
| Plugin | Wasmtime/WASI, WASM in processi ausiliari e plugin di processo |
| Risposte IA in streaming | SSE per OpenAI, Anthropic e Gemini, nel processo |
| RAG | BM25 e indice vettoriale HNSW con fusione delle classifiche e tokenizzazione a bigrammi CJK |
| Internazionalizzazione | `oxideterm-i18n`: 11 lingue |

</details>

---

<a id="install"></a>

## Installazione

[**Scarica l'ultima versione**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

| Sistema operativo | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Programma di installazione (`.exe`) | Programma di installazione (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

Verifica il download con il file `sha256sums.txt` nella pagina della versione. Vi trovi anche gli archivi portabili e le firme.

### macOS

Se Gatekeeper blocca l'app, rimuovi il flag di quarantena:

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

Se SmartScreen mostra un avviso, scegli **Ulteriori informazioni → Esegui comunque**.

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

Preferisci compilare il programma? Vedi **Eseguire dal codice sorgente** nella sezione [Per gli sviluppatori](#for-developers).

---

## Contribuire

Sono benvenuti contributi al codice Rust, alla documentazione, alle traduzioni, ai plugin, ai test e alla riproduzione dei problemi. Prima di proporre modifiche importanti, apri una segnalazione per discuterne.

Le segnalazioni di bug sono più utili se includono un pacchetto diagnostico con i dati sensibili oscurati:

```bash
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

Bug riproducibili e regressioni hanno la priorità. Le richieste di funzionalità vengono valutate per ambito, sicurezza e coerenza con lo spazio di lavoro per server remoti di OxideTerm. Se OxideTerm ti aiuta nel lavoro, una stella su GitHub, una segnalazione riproducibile, una correzione di traduzione o un plugin aiutano il progetto a progredire.

### Collaboratori

Grazie a tutte le persone che contribuiscono a migliorare OxideTerm.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="Collaboratori di OxideTerm">
  </a>
</p>

---

## Licenza

**GPL-3.0-only.** Le attribuzioni delle dipendenze sono in [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), con ulteriori avvisi in [`NOTICE`](../../NOTICE).

**Realizzato con:** [russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
