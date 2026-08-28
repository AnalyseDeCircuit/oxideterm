<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>Espaço de trabalho operacional nativo com IA para servidores remotos — app nativo em Rust puro</strong>
  <br>
  Terminais SSH, Mosh, Telnet, seriais, RDP/VNC, SFTP, encaminhamento de portas e edição leve em um espaço de trabalho nativo.
  <br>
  Renderização GPU. Grátis. Sem necessidade de conta.
  <br>
  <strong>Sem Electron. Sem WebView incorporada. Sem telemetria. Sem assinatura. BYOK primeiro. SSH puro em Rust sem OpenSSL/libssh2.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.25-blue" alt="Versão">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plataforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licença">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Código aberto, local-first e renderizado por GPU com GPUI.</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="Visão geral dos recursos do OxideTerm" width="920">
</p>

---

## O que é o OxideTerm

OxideTerm é um espaço de trabalho de código aberto para SSH e operações remotas. Terminais, arquivos, encaminhamentos, ferramentas do host e desktops remotos ficam em um só lugar.

**O que você pode fazer:**

- Gerenciar SSH, Mosh, Telnet, serial, RDP/VNC, SFTP, encaminhamento de portas, shells locais e edição leve em um único espaço de trabalho
- Manter o trabalho remoto durante breves interrupções de rede com a reconexão Grace Period
- Pedir ao OxideSens que examine sessões ativas e execute ações aprovadas usando seu próprio provedor de IA

Suas conexões e dados operacionais permanecem sob seu controle. OxideSens usa seu próprio provedor de IA e não exige conta.

---

## Por que OxideTerm?

| Se você se importa com… | O OxideTerm oferece… |
|---|---|
| Um nó remoto, muitas ferramentas | Terminal, SFTP, encaminhamento de portas, RDP/VNC, trzsz, IDE nativa, monitoramento e OxideSens AI permanecem no mesmo workspace |
| Um app desktop sem Electron ou WebView incluído | A GPUI desenha a interface diretamente em uma superfície de GPU, sem distribuir um runtime de navegador |
| Fluxos operacionais local-first | SSH, Telnet, SFTP, encaminhamento, RDP/VNC, shell local, terminais seriais e configuração funcionam sem cadastro |
| OxideSens AI com suas próprias chaves em vez de créditos de plataforma | O OxideSens usa seu endpoint OpenAI, Anthropic, Gemini, Ollama ou compatível com OpenAI, com MCP, RAG, controles de raciocínio por provedor e ações aprovadas no workspace |
| Estabilidade de reconexão | Grace Period verifica a conexão anterior por 30 s antes de substituí-la, para que apps TUI sobrevivam a quedas breves de rede |
| SSH em Rust puro e segurança de credenciais | A pilha SSH usa `russh` + `ring` sem OpenSSL/libssh2; as credenciais salvas usam o chaveiro do sistema e os pacotes `.oxide` usam ChaCha20-Poly1305 + Argon2id |

---

## Capturas de tela

As capturas mostram os fluxos de terminal, arquivos, edição e encaminhamento do OxideTerm.

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

## Feito para operações remotas

OxideTerm mantém conexões, arquivos, encaminhamentos, ferramentas do host, automação e contexto de IA em um espaço Rust. As ferramentas compartilham a mesma identidade de servidor e o mesmo ciclo de sessão.

| Aspecto | Abordagem com navegador incluído | OxideTerm |
|---|---|---|
| **Renderização** | Motor de navegador e layout web | GPUI em uma superfície de GPU |
| **Fluxo de dados do terminal** | WebSocket → loop de eventos JS → xterm.js | Entrada Rust → mutação de `TerminalState` → renderização GPUI |
| **Ciclo de vida da conexão** | Dividido entre frontend e backend | Um único pipeline no processo para conexão e reconexão |
| **Contexto de IA** | Copiado por uma ponte da aplicação | Criado do workspace ativo com aprovação do usuário |
| **Runtime de plugins** | Ambiente de scripts do navegador | Caminhos manifest-only, WASM com capacidades delimitadas e processos que exigem confiança explícita |
| **CLI** | Exige o app desktop em execução | Binário independente com ligação direta a crates |
| **Limite de runtime** | Wrapper desktop mais runtime de navegador | Processo nativo sem runtime de navegador incluído |

---

## Recursos

| Categoria | Recursos |
|---|---|
| **Terminal e conexões** | Shells locais, SSH, Mosh, Telnet, serial, painéis, modo de entrada livre, logs de sessão configuráveis, modo de controle nativo tmux -CC com layout de painéis e divisores arrastáveis, grupos de transmissão nomeados, envio avançado para vários destinos, rotas multi-hop e reconexão estável |
| **Arquivos e edição remota** | SFTP, filas de transferência, favoritos, gravação segura, árvores de projeto e edição em abas |
| **Encaminhamento e rede** | Encaminhamento local, remoto e SOCKS5 dinâmico, regras salvas e depuração de sockets |
| **Operações do host e desktop remoto** | Monitoramento, processos, serviços, logs, portas, tarefas, discos, pacotes, contêineres, tmux, RDP e VNC |
| **OxideSens e automação** | Provedores de IA próprios, MCP, RAG local, Agent Skills, ações aprovadas, sincronização criptografada e CLI |
| **Extensões e personalização** | Plugins manifest-only, WASM e de processo, abas personalizadas, comandos rápidos, temas, fundos, atalhos e 11 idiomas |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abre um terminal dentro do OxideTerm" width="920">
</a>

*OxideSens segue um pedido do usuário e abre um terminal dentro do OxideTerm.*

</div>

---

## Instalação

[Baixar a versão mais recente](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

- macOS: escolha o arquivo `.dmg` para Apple Silicon ou Intel.
- Windows: use o instalador x64 ou ARM64.
- Linux: escolha AppImage, `.deb` ou `.rpm`.
- Verifique os downloads com o arquivo `sha256sums.txt` na página da versão.

Para compilar a partir do código-fonte, consulte a seção « Executar a partir do código-fonte » abaixo.

---

## Arquitetura

OxideTerm remove a ponte WebView e mantém terminal, SSH, Telnet, RDP, VNC, SFTP, forwarding, IDE, IA, plugins e CLI em uma arquitetura Rust-native. Os detalhes completos ficam preservados abaixo.

<details>
<summary><strong>Arquitetura, internos SSH, shell GPUI, reconexão, IA, plugins e mais</strong></summary>
<br>

### Arquitetura — núcleo no mesmo processo, sem ponte WebView

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ dentro do processo Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardingRuntime / IdeWorkspace
  Ai/ACP Entities / CloudSync / Plugin Runtimes
```

Não existe fronteira de serialização entre UI e backend SSH/terminal. Os bytes do terminal modificam `TerminalState` diretamente; GPUI lê o estado e emite draw calls GPU.

### SSH puro em Rust — russh (ring)


- **Sem OpenSSL/libssh2 na pilha SSH** — `ring` fornece a criptografia SSH
- SSH2 completo: troca de chaves, canais, subsistema SFTP, encaminhamento de portas
- ChaCha20-Poly1305 / AES-GCM, chaves Ed25519/RSA/ECDSA
- SSH Agent no Unix (`SSH_AUTH_SOCK`) e Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop com autenticação independente em cada salto

### Smart Reconnect com Grace Period


1. Detectar SSH keepalive timeout sem JavaScript timer throttling
2. Fazer instantâneo de painéis de terminal, transferências SFTP, encaminhamentos e arquivos IDE
3. Testar a conexão antiga por 30 segundos de Grace Period para que TUIs sobrevivam a trocas de rede
4. Se a recuperação falhar, reconectar, restaurar encaminhamentos, retomar ou repetir transferências conforme a estratégia e reabrir arquivos IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → retry-or-resume-transfers → restore-ide → verify → done`

### Pool de conexões SSH e roteamento por nó


- No modo padrão, uma conexão SSH física pode atender terminais, SFTP, encaminhamentos de portas e IDE; o terminal pode usar uma conexão dedicada quando a política exigir
- Cada conexão passa por `connecting → active → idle → link_down → reconnecting`
- A UI endereça `nodeId`; `NodeRouter` resolve atomicamente o `connectionId` ativo
- `NodeRuntimeStore` mantém o estado de execução dos nós e exporta instantâneos de topologia; helpers do workspace gravam esses instantâneos em `session_tree.json` e reconstroem os handles ativos na inicialização
- Falha em jump host propaga `link_down` para nós downstream

### OxideSens AI

OxideSens continua BYOK primeiro, com construção de contexto dentro do processo:

- Provedores: OpenAI, Anthropic, Gemini, Ollama ou qualquer ponto de acesso OpenAI-compatible
- MCP: transports stdio e SSE, descoberta e invocação de ferramentas
- RAG: BM25 full-text, índice vetorial HNSW, Reciprocal Rank Fusion, tokenizer CJK bigram
- Mensagens enviadas a provedores passam pela remoção de padrões de credenciais; contexto e ações do espaço de trabalho permanecem sob controle do usuário
- As chaves API ficam no chaveiro do sistema e são excluídas de propósito dos registros estruturados e das mensagens do núcleo desktop

### Shell desktop GPUI

A UI é desenhada diretamente com GPUI, sem pipeline DOM/CSS/JavaScript:

- Tipos de abas do espaço de trabalho: terminais locais, SSH, Telnet, serial, RDP, VNC, SFTP, IDE, Forwards, Settings, plugins, Topology e mais
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

- Sessões SFTP são resolvidas pelo `NodeRouter` com uma geração de conexão; o reconnect pode adquirir uma sessão válida novamente, mas uma operação da geração antiga nunca é substituída silenciosamente por uma conexão nova
- Transfer queues rastreiam direction, progress, retry state e speed limits independentemente dos file panes visíveis
- Abas IDE mantêm juntos dirty buffers, remote paths, conflict state e restore metadata
- Quando o backend suporta, remote writes usam staged/atomic behavior para evitar partial writes no fluxo normal de edição

### Plugins, CLI e diagnósticos

Extensões e superfícies de suporte seguem limites explícitos definidos em Rust:

- Plugins suportam caminhos manifest-only, WASM e processos comuns. WASM usa o runtime Wasmtime/WASI integrado; plugins de processo são processos locais sem sandbox do sistema operacional.
- A CLI linka diretamente crates de domínio para doctor, settings, connections, encaminhamentos, portable bundles, backups e reports
- Diagnósticos priorizam contagens, caminhos, flags de recurso e dicas redigidas em vez de cargas brutas com segredos
- Fluxos CLI que alteram estado usam dry-run plans, `--yes` guards e rollback backups quando aplicável

### Port forwarding — Lock-Free I/O


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


- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, eleva o custo de brute force por GPU
- Cobre connections, encaminhamentos, settings, quick commands, configurações por plugin e segredos portáteis

</details>

---

## Executar pelo código-fonte

**Requisitos:** toolchain Rust (edição 2024) e um ambiente de desktop capaz de executar GPUI.

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build/build-cli.sh
./scripts/build/build-agent.sh
```

## CLI

A CLI sem interface `oxideterm` funciona sem iniciar o app e é útil para automação, CI e diagnósticos.

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

## Tecnologias

| Camada | Tecnologia | Observações |
|---|---|---|
| Interface | GPUI (Zed) | Modo imediato acelerado por GPU, inteiramente em Rust |
| Execução | Tokio + DashMap | Execução assíncrona e mapas concorrentes |
| SSH | russh (`ring`) | Sem OpenSSL/libssh2 na pilha SSH; SSH Agent |
| Terminal | portable-pty + alacritty_terminal | PTYs locais, emulação de terminal e gráficos Sixel/Kitty |
| Plugins | Wasmtime/WASI e processos | Manifest-only, chamadas de host WASM controladas e processos locais que exigem confiança explícita |
| IA e busca | SSE + BM25 + HNSW | Streaming de provedores, bigramas CJK e fusão RRF |
| Editor | tree-sitter (sintaxe), buffer próprio | Multilíngue, com suporte a SFTP |
| Criptografia | ChaCha20-Poly1305 + Argon2id | AEAD + KDF intensiva em memória (256 MB) |
| i18n | oxideterm-i18n | Carregador integrado, 11 idiomas distribuídos |

## Segurança

| Tema | Implementação |
|---|---|
| Credenciais armazenadas | macOS Keychain / Windows Credential Manager / libsecret |
| Segredos na memória | Tipos que contêm segredos e buffers temporários usam `zeroize` / `Zeroizing` nos limites de propriedade compatíveis |
| Diagnósticos | Relatórios de suporte priorizam metadados estruturados e indícios ocultados em vez de dados que contenham segredos |
| Contexto de IA | Mensagens enviadas a provedores passam pela remoção de padrões de credenciais; contexto e ações do espaço de trabalho permanecem sob controle do usuário |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Escritas CLI | dry-run plans, proteções `--yes`, rollback backups |
| Chaves de host | TOFU com `~/.ssh/known_hosts`, rejeita alterações inesperadas |
| Plugins | manifest-only, API de host WASM controlada e processos locais que exigem confiança explícita |

## Aviso de uso legítimo

O OxideTerm é licenciado sob a GPL-3.0-only, sem restrições adicionais de licença. Ao utilizá-lo, acesse somente sistemas, redes e dispositivos que sejam de sua propriedade ou para os quais você tenha autorização explícita, e cumpra a legislação aplicável. Não use o OxideTerm para acesso não autorizado, interrupção de serviços ou para contornar controles de acesso.

## Contribuição

Contribuições de código, documentação, traduções, plugins, testes e relatos de erros são bem-vindas. Discuta mudanças maiores ou correções bem delimitadas em uma issue.

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## Suporte e manutenção

Bugs e regressões reproduzíveis com diagnósticos ocultados são priorizados. Solicitações de recursos são avaliadas por escopo, segurança e alinhamento com a direção do OxideTerm como espaço de trabalho para servidores remotos.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Se OxideTerm ajuda seu workflow, uma estrela no GitHub, reprodução de issue, correção de tradução ou plugin ajudam o projeto a continuar.

---

## Licença

**GPL-3.0-only**. Os avisos detalhados de terceiros estão em [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), com informações adicionais em [`NOTICE`](../../NOTICE).

## Agradecimentos

Obrigado a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
