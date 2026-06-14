<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Terminal SSH com IA · Navegador SFTP · Encaminhamento de portas · Console serial · mini IDE —— App nativa Pure Rust </strong>
  <br>
  Renderização GPU. Grátis. Sem necessidade de conta.
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero telemetria. Zero assinatura. BYOK-first. SSH puro em Rust.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.7-blue" alt="Versão">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plataforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licença">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Próxima grande edição nativa do <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — renderizada por GPU, zero-WebView, usando <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (framework de renderização do Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abre um terminal dentro do OxideTerm" width="920">
</a>

*OxideSens segue um pedido do usuário e abre um terminal dentro do OxideTerm.*

</div>

---

## O que você pode fazer

- Gerenciar terminais SSH e Telnet, SFTP, port forwards, consoles seriais, shells locais e edição leve em um workspace nativo
- Manter o trabalho remoto vivo durante instabilidades de rede com Grace Period reconnect
- Pedir à OxideSens AI para inspecionar sessões ao vivo e executar ações aprovadas do workspace usando seu próprio provedor de IA

---

## Por que OxideTerm Native?

| Se você se importa com... | OxideTerm Native oferece... |
|---|---|
| Um nó remoto, muitas ferramentas | Terminal, SFTP, port forwarding, trzsz, IDE nativo, monitoramento e OxideSens AI ficam no mesmo workspace SSH |
| Shell nativo zero-WebView | GPUI desenha a UI desktop diretamente numa superfície GPU, sem DOM, CSS, JavaScript, Chromium ou runtime WebKit |
| Workflows SSH local-first | SSH, Telnet, SFTP, forwarding, shell local, terminais seriais e configuração funcionam sem cadastro |
| OxideSens AI BYOK em vez de créditos de plataforma | OxideSens usa seu endpoint OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible com MCP, RAG e ações aprovadas do workspace |
| Reconexão estável | Grace Period sonda a conexão antiga por 30 s antes de substituí-la, para que TUIs sobrevivam a quedas curtas |
| SSH puro em Rust e credenciais seguras | `russh` + `ring`, sem OpenSSL/libssh2; senhas e chaves API ficam no keychain do OS, `.oxide` usa ChaCha20-Poly1305 + Argon2id |

## O que é / o que não é

OxideTerm Native foca em um **workspace AI local-first para servidores remotos**, reconstruído como app desktop GPUI em Rust puro. Ele é feito para usuários que querem terminais, arquivos, portas, transferências, edição leve, consoles seriais e uma OxideSens AI ao redor de suas próprias máquinas e nós remotos.

Não é uma plataforma cloud de agentes. Também não é Electron, Tauri ou terminal web: sem Chromium, sem WebView, sem JavaScript, sem CSS.

---

## Capturas de tela

A UI nativa segue o mesmo modelo de workspace e linguagem visual do OxideTerm da linha Tauri atual.

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH com OxideSens AI" /></td>
<td align="center"><strong>Gerenciador de arquivos SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Gerenciador de arquivos SFTP de painel duplo com fila de transferência" /></td>
</tr>
<tr>
<td align="center"><strong>IDE integrado</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Modo IDE integrado" /></td>
<td align="center"><strong>Encaminhamento de portas inteligente</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Encaminhamento de portas inteligente com detecção automática" /></td>
</tr>
</table>

---

## Diferenças em relação ao WebView/Tauri

| Aspecto | WebView/Tauri | Native |
|---|---|---|
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI, GPU surface, immediate mode, Rust puro |
| Fluxo terminal | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC por comando | Chamadas in-process |
| SSH keepalive | Timer JavaScript | Rust async task |
| Plugins | ESM em sandbox do navegador | WASM wasmtime + typed Rust host API |
| CLI | Requer app desktop | Binário standalone |
| Limite de runtime | Runtime de navegador + ponte WebView | Processo nativo; sem runtime de navegador embutido |

## Funcionalidades

| Categoria | Funcionalidades |
|---|---|
| Terminal | Local PTY, SSH, Telnet, local serial terminals, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens com OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG e aprovação de comandos |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, import/export criptografado |
| Plugins / CLI | WASM sandbox, native host API, plugin settings; CLI para settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Arquitetura

OxideTerm Native remove a ponte WebView e mantém terminal, SSH, Telnet, SFTP, forwarding, IDE, IA, plugins e CLI em uma arquitetura Rust-native. Os detalhes completos ficam preservados abaixo.

<details>
<summary><strong>Arquitetura, internals SSH, shell GPUI, reconexão, IA, plugins e mais</strong></summary>
<br>

### Arquitetura — processo único, zero bridge

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

Não existe fronteira de serialização entre UI e backend SSH/terminal. Os bytes do terminal modificam `TerminalState` diretamente; GPUI lê o estado e emite draw calls GPU.

### SSH puro em Rust — russh (ring)

A edição nativa vincula o mesmo stack `russh` da linha Tauri diretamente no binário desktop:

- **Zero dependências OpenSSL** via `ring`
- SSH2 completo: key exchange, channels, subsistema SFTP, port forwarding
- ChaCha20-Poly1305 / AES-GCM, chaves Ed25519/RSA/ECDSA
- SSH Agent no Unix (`SSH_AUTH_SOCK`) e Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop com autenticação independente em cada salto

### Smart Reconnect com Grace Period

A semântica de reconnect corresponde à linha Tauri, mas a orquestração roda inteiramente em tarefas async Rust:

1. Detectar SSH keepalive timeout sem JavaScript timer throttling
2. Fazer snapshot de terminal panes, transferências SFTP, forwards e arquivos IDE
3. Testar a conexão antiga por 30 segundos de Grace Period para que TUIs sobrevivam a trocas de rede
4. Se a recuperação falhar, reconectar, restaurar forwards, retomar transferências e reabrir arquivos IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### Pool de conexões SSH e roteamento por nó

`SshConnectionRegistry` usa `DashMap` e preserva o modelo node-first do Tauri sem a ponte de lifecycle WebSocket:

- Uma conexão SSH física pode servir terminal panes, SFTP, port forwards e trabalho IDE
- Cada conexão passa por `connecting → active → idle → link_down → reconnecting`
- A UI endereça `nodeId`; `NodeRouter` resolve atomicamente o `connectionId` ativo
- `NodeRuntimeStore` persiste snapshots de topologia em `session_tree.json`
- Falha em jump host propaga `link_down` para nós downstream

### OxideSens AI

OxideSens continua BYOK-first, com construção de contexto dentro do processo:

- Providers: OpenAI, Anthropic, Gemini, Ollama ou qualquer endpoint OpenAI-compatible
- MCP: transports stdio e SSE, descoberta e invocação de ferramentas
- RAG: BM25 full-text, índice vetorial HNSW, Reciprocal Rank Fusion, tokenizer CJK bigram
- O contexto AI vem do estado do workspace; credenciais são mascaradas antes de chamadas ao provider
- API keys ficam no keychain do sistema e nunca entram em logs ou frames IPC

### Shell desktop GPUI

A UI é desenhada diretamente com GPUI, sem pipeline DOM/CSS/JavaScript:

- 17 tipos de abas workspace: terminais locais, SSH e Telnet, SFTP, IDE, Forwards, Settings, Plugin, Topology e mais
- Binary pane tree com divisores arrastáveis, até quatro panes por aba terminal
- Command palette, atalhos globais e sidebars feitos com primitives GPUI
- Immediate-mode rendering reage ao estado Rust sem round-trip de serialização

### Estado do terminal e renderização

A renderização do terminal é modelada primeiro como estado Rust e depois desenhada pelo GPUI:

- A saída PTY entra em `TerminalState`; scrollback, cursor, seleção, marks e estado de busca ficam em Rust
- A rendering policy pode alternar entre Boost, Normal e Idle sem esperar cooperação de um browser event loop
- Gráficos Sixel e Kitty são rastreados como assets do terminal, não como DOM nodes ou canvas overlays
- Split panes compartilham o mesmo workspace state model, então tab restore e reconnect podem snapshotar juntos a topologia do terminal

### Workspace SFTP e IDE

Arquivos remotos fazem parte do mesmo node workspace, não de uma função separada:

- Sessões SFTP são resolvidas pelo `NodeRouter`, então reconnect pode trocar a conexão SSH subjacente sem mudar o node address da UI
- Transfer queues rastreiam direction, progress, retry state e speed limits independentemente dos file panes visíveis
- Abas IDE mantêm juntos dirty buffers, remote paths, conflict state e restore metadata
- Quando o backend suporta, remote writes usam staged/atomic behavior para evitar partial writes no fluxo normal de edição

### Plugins, CLI e diagnósticos

A branch native mantém extensões e superfícies de suporte dentro de limites Rust-native:

- Plugins rodam em wasmtime sandbox com typed host capabilities em vez de browser globals
- A CLI linka diretamente domain crates para doctor, settings, connections, forwards, portable bundles, backups e reports
- Diagnósticos priorizam counts, paths, feature flags e redacted hints em vez de payloads crus com segredos
- Fluxos CLI que alteram estado usam dry-run plans, `--yes` guards e rollback backups quando aplicável

### Port forwarding — Lock-Free I/O

Forwarding mantém a semântica Tauri em um crate Rust independente:

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Um único task `ssh_io` possui cada SSH Channel e evita `Arc<Mutex<Channel>>`
- Auto-restore após reconnect, death reporting e idle timeout

### trzsz — transferência in-band

trzsz continua usando o stream do terminal, sem porta extra ou agent remoto:

- Upload/download pelo stream terminal existente
- Funciona através de cadeias ProxyJump
- File pickers nativos evitam limites de memória do navegador
- Transferência bidirecional, suporte a diretórios, limites configuráveis

### Export `.oxide` criptografado

O formato de bundle criptografado corresponde à linha Tauri:

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, eleva o custo de brute force por GPU
- Cobre connections, forwards, settings, quick commands, plugin settings e portable secrets

</details>

---

## Executar pelo código-fonte

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

## Segurança

| Tema | Implementação |
|---|---|
| Passwords & keys | macOS Keychain / Windows Credential Manager / libsecret |
| Memória secreta | `zeroize` / `Zeroizing` |
| Diagnóstico & contexto AI | valores secretos são redigidos antes de saída ou provider calls |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Escritas CLI | dry-run plans, proteções `--yes`, rollback backups |
| Plugins | isolamento wasmtime e capability-based host API |

## Status da release

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] Fluxo terminal in-process sem WebSocket
- [x] SFTP, forwarding, IDE, AI, cloud sync, plugins, CLI
- [x] Terminais seriais locais
- [x] Full ProxyCommand
- [ ] Audit logging

## Contribuição

## Neutralidade de providers

OxideTerm é BYOK-first e neutro em relação a providers.

Integrações de providers existem para ajudar usuários a conectar as ferramentas em que já confiam. Elas não são um ranking, um outdoor ou um sistema de recompensa para quem pede atenção com mais entusiasmo.

Compatibilidade, manutenibilidade, segurança e valor real para usuários decidem o que entra na documentação. Visibilidade segue utilidade, não entusiasmo.

Quando uma função já existir na versão Tauri, mantenha comportamento, textos, estados de interação e workflows alinhados. Cada crate novo precisa ter responsabilidade real, não apenas re-export.

## Suporte e manutenção

Bugs e regressões reproduzíveis com diagnósticos redigidos são priorizados. Requests de funcionalidade são avaliados por escopo, segurança e alinhamento com a direção do OxideTerm para o workspace de servidores remotos.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Se OxideTerm ajuda seu workflow, uma estrela no GitHub, reprodução de issue, correção de tradução, plugin ou pull request ajudam o projeto a continuar.

---

## Licença / Agradecimentos

**GPL-3.0-only**. Notices de terceiros ficam em `NOTICE`. Obrigado a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
