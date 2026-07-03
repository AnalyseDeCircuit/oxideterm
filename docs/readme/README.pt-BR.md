<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Espaço de trabalho operacional nativo com IA para servidores remotos — app nativo em Rust puro</strong>
  <br>
  Terminais SSH, Telnet, seriais, RDP/VNC, SFTP, encaminhamento de portas, Raw TCP/UDP e edição leve em um espaço de trabalho nativo.
  <br>
  Renderização GPU. Grátis. Sem necessidade de conta.
  <br>
  <strong>Sem WebView. Sem OpenSSL. Sem telemetria. Sem assinatura. BYOK primeiro. SSH puro em Rust.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.11-blue" alt="Versão">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plataforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licença">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Próxima grande edição nativa do <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — renderizada por GPU, sem WebView, usando <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (framework de renderização do Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="Visão geral dos recursos do OxideTerm Native" width="920">
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abre um terminal dentro do OxideTerm" width="920">
</a>

*OxideSens segue um pedido do usuário e abre um terminal dentro do OxideTerm.*

</div>

---

## O que você pode fazer

- Gerenciar SSH, Telnet, serial, RDP/VNC, SFTP, encaminhamentos de portas, Raw TCP/UDP, shells locais e edição leve em um espaço de trabalho nativo
- Manter o trabalho remoto vivo durante instabilidades de rede com a reconexão Grace Period
- Pedir à OxideSens AI para inspecionar sessões ativas e executar ações aprovadas do espaço de trabalho usando seu próprio provedor de IA

---

## Por que OxideTerm Native?

| Se você se importa com... | OxideTerm Native oferece... |
|---|---|
| Um nó remoto, muitas ferramentas | Terminal, SFTP, encaminhamento de portas, RDP/VNC, Raw TCP/UDP, trzsz, IDE nativo, monitoramento e OxideSens AI ficam no mesmo espaço de trabalho |
| Shell nativo sem WebView | GPUI desenha a interface desktop diretamente numa superfície GPU, sem DOM, CSS, JavaScript, Chromium ou runtime WebKit |
| Fluxos operacionais locais primeiro | SSH, Telnet, SFTP, encaminhamento, RDP/VNC, Raw TCP/UDP, shell local, terminais seriais e configuração funcionam sem cadastro |
| OxideSens AI com BYOK em vez de créditos de plataforma | OxideSens usa seu ponto de acesso OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible com MCP, RAG e ações aprovadas do espaço de trabalho |
| Reconexão estável | Grace Period sonda a conexão antiga por 30 s antes de substituí-la, para que TUIs sobrevivam a quedas curtas |
| SSH puro em Rust e credenciais seguras | `russh` + `ring`, sem OpenSSL/libssh2; senhas e chaves API ficam no chaveiro do OS, `.oxide` usa ChaCha20-Poly1305 + Argon2id |

## O que é / o que não é

OxideTerm Native foca em um **espaço de trabalho de IA local primeiro para servidores remotos**, reconstruído como app desktop GPUI em Rust puro. Ele é feito para usuários que querem terminais, áreas de trabalho remotas, sockets brutos, arquivos, portas, transferências, edição leve, consoles seriais e OxideSens AI ao redor de suas próprias máquinas e nós remotos.

Não é uma plataforma de agentes hospedada na nuvem. Também não é Electron, Tauri ou terminal web: sem Chromium, sem WebView, sem JavaScript, sem CSS.

---

## Capturas de tela

A interface nativa segue o mesmo modelo de espaço de trabalho e a mesma linguagem visual do OxideTerm da linha Tauri atual.

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
| Rendering | Chromium/Safari/WebKit2GTK + CSS | GPUI, superfície GPU, modo imediato, Rust puro |
| Fluxo terminal | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC por comando | Chamadas dentro do processo |
| SSH keepalive | Timer JavaScript | Rust async task |
| Plugins | ESM em sandbox do navegador | WASM wasmtime + API de host Rust tipada |
| CLI | Requer app desktop | Binário standalone |
| Limite de runtime | Runtime de navegador + ponte WebView | Processo nativo; sem runtime de navegador embutido |

## Funcionalidades

| Categoria | Funcionalidades |
|---|---|
| Terminal | PTY local, SSH, Telnet, terminais Raw TCP/UDP, terminais seriais locais, painéis divididos, integração com shell, marcas de comando, asciicast, trzsz, Sixel/Kitty graphics, política de renderização |
| SSH & Auth | pool de conexões, ProxyJump ilimitado, reconexão Grace Period, TOFU de chave de host, encaminhamento de SSH Agent, password/key/cert/keyboard-interactive |
| SFTP / IDE | navegador de dois painéis, fila de transferências, prévia, favoritos, escritas atômicas, árvore remota de arquivos, editor multiaba, resolução de conflitos |
| Forwarding | Local, Remote, Dynamic SOCKS5, regras salvas, restauração após reconexão, relatório de encerramento, tempo limite de inatividade |
| Área de trabalho remota | Abas RDP e VNC integradas, controles de reconexão, tamanho conforme viewport, teclado, mouse, área de transferência e cursor |
| Raw TCP/UDP | Terminais Raw TCP e Raw UDP para depurar serviços pontuais, protocolos de dispositivos e datagramas |
| AI | OxideSens com OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG e aprovação de comandos |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, import/export criptografado |
| Plugins / CLI | WASM sandbox, API de host nativa, configurações por plugin; CLI para settings, connections, encaminhamentos, plugins, secrets, cloud-sync, backup, report |

## Arquitetura

OxideTerm Native remove a ponte WebView e mantém terminal, SSH, Telnet, RDP, VNC, Raw TCP/UDP, SFTP, forwarding, IDE, IA, plugins e CLI em uma arquitetura Rust-native. Os detalhes completos ficam preservados abaixo.

<details>
<summary><strong>Arquitetura, internos SSH, shell GPUI, reconexão, IA, plugins e mais</strong></summary>
<br>

### Arquitetura — processo único, zero bridge

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ dentro do processo Arc<> / async
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
- SSH2 completo: troca de chaves, canais, subsistema SFTP, encaminhamento de portas
- ChaCha20-Poly1305 / AES-GCM, chaves Ed25519/RSA/ECDSA
- SSH Agent no Unix (`SSH_AUTH_SOCK`) e Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop com autenticação independente em cada salto

### Smart Reconnect com Grace Period

A semântica de reconnect corresponde à linha Tauri, mas a orquestração roda inteiramente em tarefas async Rust:

1. Detectar SSH keepalive timeout sem JavaScript timer throttling
2. Fazer instantâneo de painéis de terminal, transferências SFTP, encaminhamentos e arquivos IDE
3. Testar a conexão antiga por 30 segundos de Grace Period para que TUIs sobrevivam a trocas de rede
4. Se a recuperação falhar, reconectar, restaurar encaminhamentos, retomar transferências e reabrir arquivos IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### Pool de conexões SSH e roteamento por nó

`SshConnectionRegistry` usa `DashMap` e preserva o modelo node-first do Tauri sem a ponte de lifecycle WebSocket:

- Uma conexão SSH física pode servir painéis de terminal, SFTP, encaminhamentos de portas e trabalho IDE
- Cada conexão passa por `connecting → active → idle → link_down → reconnecting`
- A UI endereça `nodeId`; `NodeRouter` resolve atomicamente o `connectionId` ativo
- `NodeRuntimeStore` persiste instantâneos de topologia em `session_tree.json`
- Falha em jump host propaga `link_down` para nós downstream

### OxideSens AI

OxideSens continua BYOK primeiro, com construção de contexto dentro do processo:

- Provedores: OpenAI, Anthropic, Gemini, Ollama ou qualquer ponto de acesso OpenAI-compatible
- MCP: transports stdio e SSE, descoberta e invocação de ferramentas
- RAG: BM25 full-text, índice vetorial HNSW, Reciprocal Rank Fusion, tokenizer CJK bigram
- O contexto AI vem do estado do espaço de trabalho; credenciais são mascaradas antes de chamadas ao provedor
- chaves API ficam no chaveiro do sistema e nunca entram em registros ou quadros IPC

### Shell desktop GPUI

A UI é desenhada diretamente com GPUI, sem pipeline DOM/CSS/JavaScript:

- Tipos de abas do espaço de trabalho: terminais locais, SSH, Telnet, serial, RDP, VNC e Raw TCP/UDP, SFTP, IDE, Forwards, Settings, plugins, Topology e mais
- Binary pane tree com divisores arrastáveis, até quatro panes por aba terminal
- Command palette, atalhos globais e sidebars feitos com primitives GPUI
- Immediate-mode rendering reage ao estado Rust sem round-trip de serialização

### Estado do terminal e renderização

A renderização do terminal é modelada primeiro como estado Rust e depois desenhada pelo GPUI:

- A saída PTY entra em `TerminalState`; scrollback, cursor, seleção, marks e estado de busca ficam em Rust
- A política de renderização pode alternar entre Boost, Normal e Idle sem esperar cooperação de um browser event loop
- Gráficos Sixel e Kitty são rastreados como assets do terminal, não como DOM nodes ou canvas overlays
- Painéis divididos compartilham o mesmo espaço de trabalho modelo de estado, então restauração de aba e reconnect podem instantâneoar juntos a topologia do terminal

### Workspace SFTP e IDE

Arquivos remotos fazem parte do mesmo node espaço de trabalho, não de uma função separada:

- Sessões SFTP são resolvidas pelo `NodeRouter`, então reconnect pode trocar a conexão SSH subjacente sem mudar o node address da UI
- Transfer queues rastreiam direction, progress, retry state e speed limits independentemente dos file panes visíveis
- Abas IDE mantêm juntos dirty buffers, remote paths, conflict state e restore metadata
- Quando o backend suporta, remote writes usam staged/atomic behavior para evitar partial writes no fluxo normal de edição

### Plugins, CLI e diagnósticos

A branch native mantém extensões e superfícies de suporte dentro de limites Rust-native:

- Plugins rodam em wasmtime sandbox com capacidades de host tipadas em vez de globais do navegador
- A CLI linka diretamente crates de domínio para doctor, settings, connections, encaminhamentos, portable bundles, backups e reports
- Diagnósticos priorizam contagens, caminhos, flags de recurso e dicas redigidas em vez de cargas brutas com segredos
- Fluxos CLI que alteram estado usam dry-run plans, `--yes` guards e rollback backups quando aplicável

### Port forwarding — Lock-Free I/O

Forwarding mantém a semântica Tauri em um crate Rust independente:

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Um único task `ssh_io` possui cada SSH Channel e evita `Arc<Mutex<Channel>>`
- Auto-restore após reconnect, relatório de encerramento e tempo limite de inatividade

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
- Cobre connections, encaminhamentos, settings, quick commands, configurações por plugin e segredos portáteis

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
| Senhas e chaves | macOS Keychain / Windows Credential Manager / libsecret |
| Memória secreta | `zeroize` / `Zeroizing` |
| Diagnóstico & contexto AI | valores secretos são redigidos antes de saída ou provedor calls |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Escritas CLI | dry-run plans, proteções `--yes`, rollback backups |
| Plugins | isolamento wasmtime e baseada em capacidades API de host |

## Status da release

- [x] encaminhamento de SSH Agent, reconexão Grace Period, GPUI desktop shell
- [x] Fluxo terminal dentro do processo sem WebSocket
- [x] SFTP, forwarding, IDE, AI, sincronização em nuvem, plugins, CLI
- [x] Terminais seriais locais
- [x] Área de trabalho remota RDP/VNC e terminais Raw TCP/UDP
- [x] Full ProxyCommand
- [ ] Audit logging

## Contribuição

## Neutralidade de provedors

OxideTerm é BYOK primeiro e neutro em relação a provedors.

Integrações de provedors existem para ajudar usuários a conectar as ferramentas em que já confiam. Elas não são um ranking, um outdoor ou um sistema de recompensa para quem pede atenção com mais entusiasmo.

Compatibilidade, manutenibilidade, segurança e valor real para usuários decidem o que entra na documentação. Visibilidade segue utilidade, não entusiasmo.

Quando uma função já existir na versão Tauri, mantenha comportamento, textos, estados de interação e workflows alinhados. Cada crate novo precisa ter responsabilidade real, não apenas re-export.

## Suporte e manutenção

Bugs e regressões reproduzíveis com diagnósticos redigidos são priorizados. Requests de funcionalidade são avaliados por escopo, segurança e alinhamento com a direção do OxideTerm para o espaço de trabalho de servidores remotos.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Se OxideTerm ajuda seu workflow, uma estrela no GitHub, reprodução de issue, correção de tradução, plugin ou pull request ajudam o projeto a continuar.

---

## Licença / Agradecimentos

**GPL-3.0-only**. Notices de terceiros ficam em `NOTICE`. Obrigado a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
