<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>La prossima edizione zero-WebView di OxideTerm.</strong>
  <br>
  Connettiti una volta a una macchina remota e lavora con shell, file, porte, trasferimenti, editor leggero, console seriali e BYOK AI da un workspace Rust nativo.
  <br>
  App GPUI nativa · SSH puro in Rust · nessun account per i workflow SSH principali
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust all the way down.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Versione">
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

> **Stato release:** OxideTerm Native è in preparazione come prossima grande versione di OxideTerm. Gli installer pubblici non sono ancora disponibili; per ora eseguilo dal sorgente. Le release pacchettizzate attuali restano sulla linea Tauri finché gli installer native non saranno pronti.

## Cosa puoi fare

- Gestire terminali SSH, SFTP, port forwarding, console seriali, shell locali ed editing leggero in un workspace nativo
- Mantenere vivo il lavoro remoto durante problemi di rete con Grace Period reconnect
- Usare il tuo provider AI per ispezionare sessioni live ed eseguire azioni workspace approvate

---

## Perché OxideTerm Native?

| Se ti interessa... | OxideTerm Native offre... |
|---|---|
| Un nodo remoto, molti strumenti | Terminale, SFTP, port forwarding, trzsz, IDE nativo, monitoraggio e contesto AI restano legati allo stesso workspace SSH |
| Shell nativa zero-WebView | GPUI disegna la UI desktop direttamente su una superficie GPU, senza DOM, CSS, JavaScript, Chromium o runtime WebKit |
| Workflow SSH local-first | SSH, SFTP, forwarding, shell locale, terminali seriali e configurazione funzionano senza registrazione |
| BYOK AI invece di crediti piattaforma | OxideSens usa il tuo endpoint OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible con supporto MCP e RAG |
| Riconnessione stabile | Grace Period prova la vecchia connessione per 30 s prima di sostituirla, così le TUI possono sopravvivere a brevi interruzioni |
| SSH puro Rust e credenziali sicure | `russh` + `ring`, niente OpenSSL/libssh2; password e chiavi API restano nel portachiavi OS, `.oxide` usa ChaCha20-Poly1305 + Argon2id |

## Cos'è / cosa non è

OxideTerm Native si concentra sullo stesso **workspace SSH local-first** di OxideTerm, ricostruito come app desktop GPUI in Rust puro. È pensato per chi vuole tenere terminale, file, porte, trasferimenti, editing leggero, console seriali e contesto AI attorno alle proprie macchine e nodi remoti.

Non è ancora la linea stabile di download attuale, né una piattaforma cloud Agent. Non è nemmeno Electron, Tauri o terminale web: niente Chromium, niente WebView, niente JavaScript, niente CSS.

---

## Screenshot

La UI nativa segue lo stesso modello di workspace OxideTerm e lo stesso linguaggio visivo della linea Tauri attuale.

<table>
<tr>
<td align="center"><strong>Terminale SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminale SSH con barra laterale OxideSens AI" /></td>
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

</details>

---

## Eseguire dal sorgente

Gli installer native pubblici non sono ancora disponibili. Finché i build pacchettizzati non saranno pronti, esegui l'edizione native dal sorgente.

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
- [ ] Installer pubblici pacchettizzati
- [ ] Full ProxyCommand, audit logging

## Contribuire

## Neutralità dei provider

OxideTerm è BYOK-first e neutrale rispetto ai provider.

Le integrazioni dei provider esistono per aiutare gli utenti a collegare gli strumenti di cui già si fidano. Non sono una classifica, uno spazio pubblicitario o un sistema di ricompensa per chi chiede attenzione con più entusiasmo.

Compatibilità, manutenibilità, sicurezza e valore reale per gli utenti decidono cosa viene documentato. La visibilità segue l'utilità, non l'entusiasmo.

Quando una funzione esiste già nella versione Tauri, mantieni comportamento, label, stati di interazione e workflow allineati. Ogni nuovo crate deve avere una responsabilità reale, non solo re-export.

## Supporto e manutenzione

OxideTerm Native è in preparazione come prossima major release di OxideTerm ed è mantenuto best-effort. I bug report con passi riproducibili e diagnostica redatta hanno priorità; le richieste di funzionalità potrebbero non essere implementate.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Se OxideTerm aiuta il tuo workflow, una star GitHub, una riproduzione issue, una correzione di traduzione, un plugin o una pull request aiutano il progetto a proseguire.

---

## Licenza / Ringraziamenti

**GPL-3.0-only**. Le notice di terze parti sono in `NOTICE`. Grazie a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
