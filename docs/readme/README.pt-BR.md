<p align="center">
  <img src="../../src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>Se você gosta do OxideTerm, por favor dê uma estrela no GitHub! ⭐️</em>
</p>


<p align="center">
  <strong>Zero Electron. Zero OpenSSL. SSH puro em Rust.</strong>
  <br>
  <em>Um único binário nativo — shells locais, SSH, SFTP, IDE remoto, IA, encaminhamento de portas, plugins, 30+ temas, 11 idiomas.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.1.0--beta.5-blue" alt="Versão">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plataforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licença">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=Baixar%20última%20versão&style=for-the-badge&color=brightgreen" alt="Baixar última versão">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=Baixar%20última%20beta&style=for-the-badge&color=orange" alt="Baixar última beta">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

> [!NOTE]
> **Mudança de licença:** A partir da v1.0.0, o OxideTerm mudou sua licença de **PolyForm Noncommercial 1.0.0** para **GPL-3.0 (GNU General Public License v3.0)**. O OxideTerm agora é totalmente código aberto — você pode usá-lo, modificá-lo e distribuí-lo livremente sob os termos da licença GPL-3.0. Veja o arquivo [LICENSE](../../LICENSE) para detalhes.

---

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI — "Abra um terminal local e execute echo hello, world!"*

</div>

---

## Por que OxideTerm?

| Problema | A resposta do OxideTerm |
|---|---|
| Clientes SSH não fazem shells locais | **Motor híbrido**: PTY local (zsh/bash/fish/pwsh/WSL2) + SSH remoto em uma janela |
| Reconexão = perder tudo | **Reconexão com período de carência**: sonda a conexão antiga por 30 s antes de encerrá-la — seus vim/htop/yazi sobrevivem |
| Edição remota precisa do VS Code Remote | **IDE integrado**: CodeMirror 6 sobre SFTP com 30+ linguagens, agente remoto opcional (~1 MB) no Linux |
| Sem reutilização de conexão SSH | **Multiplexação**: terminal, SFTP, encaminhamentos, IDE compartilham uma única conexão SSH via pool com contagem de referências |
| Bibliotecas SSH dependem do OpenSSL | **russh 0.59**: SSH puro em Rust compilado com `ring` — zero dependências C |
| Apps Electron de 100+ MB | **Tauri 2.0**: backend Rust nativo, binário de 25–40 MB |
| IA presa a um provedor | **OxideSens**: 40+ ferramentas, protocolo MCP, base de conhecimento RAG — funciona com OpenAI/Ollama/DeepSeek/qualquer API compatível |
| Credenciais em arquivos de configuração em texto simples | **Apenas chaveiro do sistema**: senhas e chaves de API nunca são gravadas em disco; arquivos `.oxide` criptografados com ChaCha20-Poly1305 + Argon2id |
| Ferramentas dependentes da nuvem que exigem conta | **Local-first**: zero contas, zero telemetria, zero sincronização em nuvem — seus dados ficam no seu dispositivo. Traga sua própria chave de IA |

---

## Capturas de tela

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH com barra lateral OxideSens AI" /></td>
<td align="center"><strong>Gerenciador de arquivos SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Gerenciador de arquivos SFTP de painel duplo com fila de transferência" /></td>
</tr>
<tr>
<td align="center"><strong>IDE integrado (CodeMirror 6)</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Modo IDE integrado com editor CodeMirror 6" /></td>
<td align="center"><strong>Encaminhamento de portas inteligente</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Encaminhamento de portas inteligente com detecção automática" /></td>
</tr>
</table>

---

## Visão geral das funcionalidades

| Categoria | Funcionalidades |
|---|---|
| **Terminal** | PTY local (zsh/bash/fish/pwsh/WSL2), SSH remoto, painéis divididos, broadcast de entrada, gravação/reprodução de sessões (asciicast v2), renderização WebGL, 30+ temas + editor personalizado, paleta de comandos (`⌘K`), modo zen |
| **SSH e autenticação** | Pool de conexões e multiplexação, ProxyJump (saltos ilimitados) com grafo topológico, reconexão automática com período de carência, Encaminhamento de agente. Auth: senha, chave SSH (RSA/Ed25519/ECDSA), SSH Agent, certificados, 2FA interativo por teclado, Known Hosts TOFU |
| **SFTP** | Navegador de painel duplo, arrastar e soltar, pré-visualização inteligente (imagens/vídeo/áudio/código/PDF/hex/fontes), fila de transferência com progresso e ETA, favoritos, extração de arquivos |
| **Modo IDE** | CodeMirror 6 com 30+ linguagens, árvore de arquivos + status Git, multi-abas, resolução de conflitos, terminal integrado. Agente remoto opcional para Linux (9 arquiteturas adicionais) |
| **Encaminhamento de portas** | Local (-L), remoto (-R), SOCKS5 dinâmico (-D), I/O por passagem de mensagens sem lock, restauração automática na reconexão, relatório de falhas, timeout de inatividade |
| **IA (OxideSens)** | Painel inline (`⌘I`) + chat lateral, captura de buffer do terminal (painel único/todos), contexto multi-fonte (IDE/SFTP/Git), 40+ ferramentas autônomas, integração com servidores MCP, base de conhecimento RAG (busca híbrida BM25 + vetores), streaming SSE |
| **Plugins** | Carregamento ESM em runtime, 18 namespaces de API, 24 componentes UI Kit, API congelada + ACL Proxy, circuit breaker, desativação automática em caso de erros |
| **CLI** | Companion `oxt`: JSON-RPC 2.0 via Unix Socket / Named Pipe, `status`/`list`/`ping`, saída legível + JSON |
| **Segurança** | Exportação criptografada .oxide (ChaCha20-Poly1305 + Argon2id 256 MB), chaveiro do SO, Touch ID (macOS), TOFU de chave do host, limpeza de memória `zeroize` |
| **i18n** | 11 idiomas: EN, 简体中文, 繁體中文, 日本語, 한국어, FR, DE, ES, IT, PT-BR, VI |

---

## Sob o capô

### Arquitetura — Comunicação de plano duplo

OxideTerm separa dados do terminal dos comandos de controle em dois planos independentes:

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│  xterm.js 6 (WebGL) + 19 stores    │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binário)
           │ (JSON)       │ porta por sessão
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│  NodeRouter → SshConnectionRegistry │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]       │
└─────────────────────────────────────┘
```

- **Plano de dados (WebSocket)**: cada sessão SSH obtém sua própria porta WebSocket. Os bytes do terminal fluem como frames binários com cabeçalho Type-Length-Payload — sem serialização JSON, sem codificação Base64, zero overhead no caminho crítico.
- **Plano de controle (Tauri IPC)**: gerenciamento de conexões, operações SFTP, encaminhamentos, configuração — JSON estruturado, mas fora do caminho crítico.
- **Endereçamento por nó**: o frontend nunca toca `sessionId` nem `connectionId`. Tudo é endereçado por `nodeId`, resolvido atomicamente no servidor pelo `NodeRouter`. A reconexão SSH altera o `connectionId` subjacente — mas SFTP, IDE e encaminhamentos não são afetados de forma alguma.

### 🔩 SSH puro em Rust — russh 0.59

Toda a pilha SSH é **russh 0.59** compilada com o backend criptográfico **`ring`**:

- **Zero dependências C/OpenSSL** — toda a pilha criptográfica é Rust. Sem mais debug de "qual versão do OpenSSL?".
- Protocolo SSH2 completo: troca de chaves, canais, subsistema SFTP, encaminhamento de portas
- Suítes de cifragem ChaCha20-Poly1305 e AES-GCM, chaves Ed25519/RSA/ECDSA
- **`AgentSigner`** personalizado: encapsula o SSH Agent do sistema e implementa o trait `Signer` do russh, resolvendo problemas de bounds `Send` RPITIT ao clonar `&AgentIdentity` para um valor owned antes de cruzar `.await`

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **Suporte de plataforma**: Unix (`SSH_AUTH_SOCK`), Windows (`\\.\pipe\openssh-ssh-agent`)
- **Cadeias proxy**: cada salto usa autenticação Agent de forma independente
- **Reconexão**: `AuthMethod::Agent` reproduzido automaticamente

### 🔄 Reconexão inteligente com período de carência

A maioria dos clientes SSH destrói tudo na desconexão e recomeça do zero. O orquestrador de reconexão do OxideTerm adota uma abordagem fundamentalmente diferente:

1. **Detecção** do timeout de heartbeat WebSocket (300 s, calibrado para macOS App Nap e throttling de timers JS)
2. **Snapshot** do estado completo: painéis do terminal, transferências SFTP em andamento, encaminhamentos de portas ativos, arquivos IDE abertos
3. **Sondagem inteligente**: eventos `visibilitychange` + `online` disparam keepalive SSH proativo (~2 s de detecção contra 15–30 s de timeout passivo)
4. **Período de carência** (30 s): sonda a conexão SSH antiga via keepalive — se ela se recuperar (ex.: troca de ponto de acesso WiFi), seus apps TUI (vim, htop, yazi) sobrevivem completamente intactos
5. Se a recuperação falhar → nova conexão SSH → restauração automática dos encaminhamentos → retomada das transferências SFTP → reabertura dos arquivos IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

Toda a lógica passa por um `ReconnectOrchestratorStore` dedicado — zero código de reconexão espalhado em hooks ou componentes.

### 🛡️ Pool de conexões SSH

`SshConnectionRegistry` com contagem de referências respaldado por `DashMap` para acesso concorrente sem lock:

- **Uma conexão, muitos consumidores**: terminal, SFTP, encaminhamentos de portas e IDE compartilham uma única conexão SSH física — sem handshakes TCP redundantes
- **Máquina de estados por conexão**: `connecting → active → idle → link_down → reconnecting`
- **Gerenciamento de ciclo de vida**: timeout de inatividade configurável (5 min / 15 min / 30 min / 1 h / nunca), intervalo keepalive de 15 s, detecção de falhas de heartbeat
- **Heartbeat WsBridge**: intervalo de 30 s, timeout de 5 min — tolera macOS App Nap e throttling JS do navegador
- **Propagação em cascata**: falha do host de salto → todos os nós downstream automaticamente marcados como `link_down` com sincronização de status
- **Desconexão por inatividade**: emite `connection_status_changed` para o frontend (não apenas `node:state` interno), prevenindo dessincronização da interface

### 🤖 OxideSens AI

Assistente IA focado em privacidade com dois modos de interação:

- **Painel inline** (`⌘I`): comandos rápidos de terminal, saída injetada via bracketed paste
- **Chat lateral**: conversas persistentes com histórico completo
- **Captura de contexto**: o Terminal Registry coleta o buffer do painel ativo ou de todos os painéis divididos simultaneamente; injeção automática de arquivos IDE, caminhos SFTP e status Git
- **40+ ferramentas autônomas**: operações de arquivo, gerenciamento de processos, diagnósticos de rede, interação com apps TUI, processamento de texto — a IA invoca essas ferramentas sem acionamento manual
- **Suporte MCP**: conexão a servidores [Model Context Protocol](https://modelcontextprotocol.io) externos (stdio & SSE) para integração de ferramentas de terceiros
- **Base de conhecimento RAG** (v0.20): importe documentos Markdown/TXT em coleções com escopo (global ou por conexão). A busca híbrida funde índice de palavras-chave BM25 + similaridade cosseno vetorial via Reciprocal Rank Fusion. Chunking com reconhecimento de Markdown que preserva a hierarquia de cabeçalhos. Tokenizer de bigramas CJK para chinês/japonês/coreano.
- **Provedores**: OpenAI, Ollama, DeepSeek, OneAPI, ou qualquer endpoint `/v1/chat/completions`
- **Segurança**: chaves API armazenadas no chaveiro do SO; no macOS, a leitura de chaves é protegida por **Touch ID** via `LAContext` — sem entitlements ou assinatura de código necessários, em cache após a primeira autenticação por sessão

### 💻 Modo IDE — Edição remota

Editor CodeMirror 6 operando sobre SFTP — nenhuma instalação no lado do servidor necessária por padrão:

- **Árvore de arquivos**: carregamento lazy de diretórios com indicadores de status Git (modificado/não rastreado/adicionado)
- **24 modos de linguagem**: 14 nativos CodeMirror + modos legacy via `@codemirror/legacy-modes`
- **Resolução de conflitos**: bloqueio otimista por mtime — detecta alterações remotas antes de sobrescrever
- **Git orientado a eventos**: atualização automática ao salvar, criar, excluir, renomear e pressionar Enter no terminal
- **State Gating**: todas as I/O bloqueadas quando `readiness !== 'ready'`, Key-Driven Reset força remontagem completa na reconexão
- **Agente remoto** (opcional): binário Rust de ~1 MB, implantação automática em x86_64/aarch64 Linux. 9 arquiteturas adicionais (ARMv7, RISC-V64, LoongArch64, s390x, Power64LE, i686, ARM, Android aarch64, FreeBSD x86_64) em `agents/extra/` para upload manual. Habilita árvore de arquivos aprimorada, busca de símbolos e observação de arquivos.

### 🔀 Encaminhamento de portas — I/O sem lock

Encaminhamento local (-L), remoto (-R) e SOCKS5 dinâmico (-D) completo:

- **Arquitetura por passagem de mensagens**: o canal SSH é de propriedade de uma única task `ssh_io` — sem `Arc<Mutex<Channel>>`, eliminando a contenção mutex completamente
- **Relatório de falhas**: as tasks de encaminhamento reportam ativamente o motivo de saída (desconexão SSH, fechamento de porta remota, timeout) para diagnósticos claros
- **Restauração automática**: encaminhamentos `Suspended` retomados automaticamente na reconexão sem intervenção do usuário
- **Timeout de inatividade**: `FORWARD_IDLE_TIMEOUT` (300 s) previne o acúmulo de conexões zumbi

### 🔌 Sistema de plugins em runtime

Carregamento ESM dinâmico com superfície API congelada e reforçada em segurança:

- **API PluginContext**: 18 namespaces — terminal, ui, commands, settings, lifecycle, events, storage, system
- **24 componentes UI Kit**: componentes React pré-construídos (botões, campos de entrada, diálogos, tabelas…) injetados em sandboxes de plugins via `window.__OXIDE__`
- **Membrana de segurança**: `Object.freeze` em todos os objetos de contexto, ACL baseada em Proxy, whitelist IPC, circuit breaker com desativação automática após erros repetidos
- **Módulos compartilhados**: React, ReactDOM, zustand, lucide-react expostos para uso dos plugins sem duplicação de bundles

### ⚡ Renderização adaptativa

Agendador de renderização de três níveis que substitui o batching fixo de `requestAnimationFrame`:

| Nível | Gatilho | Frequência | Benefício |
|---|---|---|---|
| **Boost** | Dados de frame ≥ 4 KB | 120 Hz+ (ProMotion nativo) | Elimina lag de rolagem em `cat largefile.log` |
| **Normal** | Digitação padrão | 60 Hz (RAF) | Base fluida |
| **Inativo** | 3 s sem I/O / aba oculta | 1–15 Hz (decaimento exponencial) | Carga GPU quase nula, economia de bateria |

As transições são completamente automáticas — impulsionadas pelo volume de dados, entrada do usuário e API Page Visibility. Abas em segundo plano continuam esvaziando dados via timer de inatividade sem despertar RAF.

### 🔐 Exportação criptografada .oxide

Backup de conexão portátil e à prova de adulteração:

- Criptografia autenticada **ChaCha20-Poly1305 AEAD**
- **KDF Argon2id**: custo de memória de 256 MB, 4 iterações — resistente a brute-force GPU
- Checksum de integridade **SHA-256**
- **Incorporação opcional de chaves**: chaves privadas codificadas em base64 no payload criptografado
- **Análise prévia**: detalhamento dos tipos de autenticação, detecção de chaves ausentes antes da exportação

### 📡 ProxyJump — Multi-salto com consciência topológica

- Profundidade de cadeia ilimitada: `Cliente → Salto A → Salto B → … → Destino`
- Parsing automático de `~/.ssh/config`, construção do grafo topológico, pathfinding Dijkstra para a rota ótima
- Nós de salto reutilizáveis como sessões independentes
- Propagação de falhas em cascata: host de salto down → todos os nós downstream automaticamente marcados como `link_down`

### ⚙️ Terminal local — PTY thread-safe

Shell local multiplataforma via `portable-pty 0.8`, protegido pelo feature gate `local-terminal`:

- `MasterPty` envolvido em `std::sync::Mutex` — threads de I/O dedicados mantêm as leituras PTY bloqueantes fora do event loop do Tokio
- Detecção automática de shell: `zsh`, `bash`, `fish`, `pwsh`, Git Bash, WSL2
- `cargo build --no-default-features` remove PTY para builds mobile/leves

### 🪟 Otimização Windows

- **ConPTY nativo**: invoca diretamente a API Windows Pseudo Console — suporte completo TrueColor e ANSI, sem WinPTY legado
- **Scanner de shells**: detecta automaticamente PowerShell 7, Git Bash, WSL2, CMD via Registro e PATH

### E muito mais

- **Profiler de recursos**: CPU/memória/rede em tempo real via canal SSH persistente lendo `/proc/stat`, cálculo baseado em deltas, degradação automática para RTT-only em sistemas não-Linux
- **Motor de temas personalizado**: 30+ temas integrados, editor visual com pré-visualização ao vivo, 20 campos xterm.js + 24 variáveis de cor UI, derivação automática de cores da UI a partir da paleta do terminal
- **Gravação de sessões**: formato asciicast v2, gravação e reprodução completas
- **Broadcast de entrada**: digite uma vez, envie para todos os painéis divididos — operações de servidor em lote
- **Galeria de fundos**: imagens de fundo por aba, 16 tipos de abas, controle de opacidade/desfoque/ajuste
- **Companion CLI** (`oxt`): binário de ~1 MB, JSON-RPC 2.0 via Unix Socket / Named Pipe, `status`/`list`/`ping` com saída legível ou `--json`
- **WSL Graphics** ⚠️ experimental: visualizador VNC integrado — 9 ambientes desktop + modo de aplicação única, detecção WSLg, Xtigervnc + noVNC

<details>
<summary>📸 11 idiomas em ação</summary>
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

## Início rápido

### Pré-requisitos

- **Rust** 1.85+
- **Node.js** 18+ (pnpm recomendado)
- **Ferramentas de plataforma**:
  - macOS: Xcode Command Line Tools
  - Windows: Visual Studio C++ Build Tools
  - Linux: `build-essential`, `libwebkit2gtk-4.1-dev`, `libssl-dev`

### Desenvolvimento

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# Compilar o CLI companion (necessário para recursos CLI)
pnpm cli:build

# App completa (frontend + backend Rust com hot reload)
pnpm run tauri dev

# Apenas frontend (Vite na porta 1420)
pnpm dev

# Build de produção
pnpm run tauri build
```

---

## Stack tecnológico

| Camada | Tecnologia | Detalhes |
|---|---|---|
| **Framework** | Tauri 2.0 | Binário nativo, 25–40 MB |
| **Runtime** | Tokio + DashMap 6 | Totalmente assíncrono, mapas concorrentes sem lock |
| **SSH** | russh 0.59 (`ring`) | Puro Rust, zero dependências C, SSH Agent |
| **PTY local** | portable-pty 0.8 | Feature-gated, ConPTY no Windows |
| **Frontend** | React 19.1 + TypeScript 5.8 | Vite 7, Tailwind CSS 4 |
| **Estado** | Zustand 5 | 19 stores especializados |
| **Terminal** | xterm.js 6 + WebGL | Acelerado por GPU, 60 fps+ |
| **Editor** | CodeMirror 6 | 30+ modos de linguagem |
| **Criptografia** | ChaCha20-Poly1305 + Argon2id | AEAD + KDF com alto consumo de memória (256 MB) |
| **Armazenamento** | redb 2.1 | Store KV embarcado |
| **i18n** | i18next 25 | 11 idiomas × 22 namespaces |
| **Plugins** | ESM Runtime | PluginContext congelado + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## Segurança

| Aspecto | Implementação |
|---|---|
| **Senhas** | Chaveiro do SO (macOS Keychain / Windows Credential Manager / libsecret) |
| **Chaves API IA** | Chaveiro do SO + autenticação biométrica Touch ID no macOS |
| **Exportação** | .oxide: ChaCha20-Poly1305 + Argon2id (256 MB de memória, 4 iterações) |
| **Memória** | Segurança de memória do Rust + `zeroize` para limpeza de dados sensíveis |
| **Chaves do host** | TOFU com `~/.ssh/known_hosts`, rejeita alterações (prevenção MITM) |
| **Plugins** | Object.freeze + ACL Proxy, circuit breaker, whitelist IPC |
| **WebSocket** | Tokens de uso único com limites de tempo |

---

## Roteiro

- [x] Encaminhamento de agente SSH
- [ ] Marketplace de plugins
- [ ] Busca de sessões e troca rápida

---

## Licença

**GPL-3.0** — este software é software livre licenciado sob a [Licença Pública Geral GNU v3.0](https://www.gnu.org/licenses/gpl-3.0.html).

Você é livre para usar, modificar e distribuir este software sob os termos da GPL-3.0. Qualquer trabalho derivado também deve ser distribuído sob a mesma licença.

Texto completo: [Licença Pública Geral GNU v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## Agradecimentos

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>236.000+ linhas de Rust e TypeScript — construído com ⚡ e ☕</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
