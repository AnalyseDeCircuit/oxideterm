<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm: seus servidores, um só espaço de trabalho" width="920">

# ⚡ OxideTerm

**Um cliente SSH nativo e gratuito para trabalhar em servidores remotos.**

SSH · Mosh · Telnet · serial · RDP/VNC · SFTP · encaminhamento de portas · editor de código

Sem conta. Sem assinatura. Sem telemetria. Sem Electron.

[![Última versão](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Plataformas](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![Licença](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Estrelas](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Baixar**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Documentação**](https://oxideterm.app) ·
[**Histórico de alterações**](../../.github/release-notes/stable-changelog.md) ·
[**Relatar um problema**](https://github.com/AnalyseDeCircuit/oxideterm/issues) ·
[**Grupo no WeChat**](https://github.com/AnalyseDeCircuit/oxideterm/issues/752)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md)

</div>

---

## O que é o OxideTerm

OxideTerm é um cliente SSH para macOS, Windows e Linux. Cada servidor tem seu espaço de trabalho em uma única janela: terminais, arquivos, encaminhamentos, editor de código, monitoramento e assistente de IA opcional. É gratuito e de código aberto sob GPL-3.0.

## Por que usar o OxideTerm

- **Uma conexão para todas as ferramentas.** Terminais, SFTP, encaminhamentos e editor de um servidor compartilham uma conexão SSH por padrão, sem novo login ao abrir outra ferramenta.
- **Resiste a quedas curtas.** Quando a conexão para de responder, OxideTerm continua verificando-a por 30 segundos antes de substituí-la, permitindo que vim, htop e outros programas de tela inteira retomem seu estado.
- **Pouco uso de memória.** A interface é desenhada na GPU com [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), o framework do Zed. Em repouso, usa cerca de 81 MB no macOS e 24 MB no Windows.
- **Sua própria IA.** OxideSens usa o provedor e a chave API que você escolher. Não há créditos OxideTerm para comprar.
- **SSH inteiramente em Rust.** Baseado em russh e ring, sem OpenSSL ou libssh2.

<details>
<summary><b>Memória em comparação com OxideTerm 1.x</b></summary>

Memória em repouso antes e depois da migração de Tauri (1.x) para GPUI nativo (2.0). Os valores de 1.x incluem seus processos WebView.

| Plataforma | 1.x (Tauri) | 2.0 (nativo) | Redução |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 74% |
| Windows | 182.4 MB | 23.5 MB | 87% |

![Memória em repouso do OxideTerm 1.x e 2.0 nos monitores de processos](../../docs/screenshots/oxideterm-memory-comparison.png)

</details>

---

## Capturas de tela

### Espaço de trabalho misto

Um terminal SSH, um terminal local, notas Markdown e SFTP na mesma aba. Combine páginas de diferentes servidores e do seu computador em até 16 painéis.

![Espaço de trabalho com quatro painéis reunindo SSH, um terminal local, notas e SFTP](../../docs/screenshots/mixed-workspace.png)

### Terminal

Uma sessão SSH. A barra lateral mostra ferramentas deste servidor: mais terminais, SFTP, IDE e encaminhamento de portas.

![Terminal SSH com ferramentas do servidor na barra lateral](../../docs/screenshots/terminal/SSHTERMINAL.png)

### Arquivos

Pastas locais e remotas lado a lado, com a fila de transferências abaixo.

![Gerenciador SFTP de dois painéis com fila de transferências](../../docs/screenshots/sftp/sftp.png)

### IDE

Abra um projeto remoto e edite seus arquivos em abas.

![Projeto remoto na IDE integrada](../../docs/screenshots/miniIDE/miniide.png)

### Encaminhamento de portas

OxideTerm lista as portas em escuta no servidor. Encaminhe-as com um clique ou use um encaminhamento rápido para ferramentas como Jupyter.

![Portas detectadas no servidor na página de encaminhamentos](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png)

<details>
<summary><b>Veja o OxideSens abrir um terminal a partir de um pedido em linguagem natural</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abrindo um terminal dentro do OxideTerm" width="720">
</a>

</details>

---

## Funcionalidades

Os itens marcados como *(plugin)* são instalados pelo marketplace de plugins dentro do aplicativo.

### Espaço de trabalho

- Gerenciador de sessões com busca, ordenação, grupos, importação/exportação, grupos aninhados, etiquetas, cores, ícones e notas
- Importação de `~/.ssh/config` com blocos `Match`, cadeias `ProxyJump` e `RemoteCommand`; hosts importados mantidos atualizados
- Importação de SecureCRT, Xshell, Termius, MobaXterm, WindTerm, Electerm e FinalShell, com prévia antes de salvar
- Filtros de abas: todas, páginas globais, terminais locais ou um servidor; sessões ocultas continuam funcionando
- Até 16 painéis por aba, combinando terminais, SFTP, IDE, encaminhamentos, notas, configurações e desktops remotos
- Janelas separadas: mover uma aba para outra janela e trazê-la de volta sem interromper sessões
- Organizar de 2–16 terminais abertos em grade sem reconectar
- Paleta de comandos para ações e conexões salvas e modo Zen focado em uma página
- Perfis de terminal local com shell, diretório inicial e ícone próprios
- Restauração do espaço de trabalho, tamanho da janela, tela cheia e posição nos monitores ao iniciar

### Terminal

- Detecção de shells locais: Unix, Prompt de Comando, Windows PowerShell, PowerShell Core, Git Bash e Nushell
- Painéis com divisórias arrastáveis; novos painéis locais herdam o diretório de trabalho
- Busca com regex, diferenciação de maiúsculas e consultas multilinha, independente por painel
- Integração Bash, Zsh, Fish, Nushell e PowerShell: diretórios, códigos de saída, marcas de comandos e instalação remota pelo aplicativo
- Recolhimento da saída de comandos concluídos, preservando o status de saída
- Barra de comandos com entrada multilinha, histórico, sugestões de caminhos, completamento e comandos rápidos
- Sugestões aproximadas do histórico e histórico persistente por conexão opcional
- Modo de digitação livre para editar o comando atual com o mouse
- Edição de colagens multilinha antes do envio, com desfazer e remoção dos delimitadores de blocos de código
- Gatilhos de texto ou regex: enviar entrada, executar comando rápido, abrir programa ou tornar o resultado uma ação clicável
- Coloração semântica de saídas sem estilo: erros, estados Git, permissões, endereços e datas; destaque por palavras e regex
- URLs e caminhos clicáveis, horários por linha com milissegundos e marcas de saída não lida nas abas
- Gráficos Sixel e Kitty, incluindo animações Kitty
- Controle nativo `tmux -CC` com disposição de painéis e divisórias arrastáveis
- Grupos de entrada sincronizada e envio de texto ou hexadecimal com ritmo, intervalos e repetições
- Comandos rápidos com grupos, fixação, parâmetros de texto, escolha ou segredos e regras de visibilidade por host
- Logs de sessão com modelos de nomes, limites de tamanho e retenção, ANSI opcional; gravações `.cast` com reprodutor integrado
- Copiar, recortar e colar nas seleções do Vim, Neovim ou Emacs, localmente e em sessões SSH compatíveis
- Sequências de Backspace/Delete e codificação de texto por conexão

### Conexões

- SSH com senha, chave privada, certificado OpenSSH, chaves gerenciadas, agente SSH, autenticação interativa por teclado ou Kerberos
- Agente SSH detectado ou endpoint personalizado (`SSH_AUTH_SOCK` em macOS/Linux, `\\.\pipe\openssh-ssh-agent` no Windows), e encaminhamento de agente
- Chaves Ed25519, RSA e ECDSA, cifras ChaCha20-Poly1305 e AES-GCM, ordem ajustável dos algoritmos e compatibilidade com servidores antigos
- Geração e preenchimento TOTP em solicitações correspondentes, no servidor ou em cada host de salto
- Chaves FIDO (`ed25519-sk`, `ecdsa-sk`) com solicitações de PIN e toque *(plugin)*
- Cadeias de salto com autenticação independente, proxies HTTP CONNECT/SOCKS5 e ProxyCommand
- Reconexão com período de tolerância: restaura encaminhamentos, retoma transferências e reabre arquivos do editor na nova conexão
- Conexão compartilhada por servidor ou autenticação separada para cada terminal novo
- Encaminhamento X11 confiável ou não confiável
- Conexões temporárias sem salvar e links `ssh://`, `telnet://`, `mosh://`, `rdp://` e `vnc://` que abrem o aplicativo
- Mosh com previsão adaptativa do eco local *(plugin)*
- Telnet com negociação de opções, proxy e finais de linha configuráveis
- Serial: velocidade, bits de dados, paridade, bits de parada, controle de fluxo, entrada/saída hexadecimal, Break, DTR e RTS
- Terminais Bluetooth LE via Nordic UART Service ou características GATT personalizadas

### Arquivos e transferências

- Gerenciador local/remoto de dois painéis com favoritos, completamento de caminhos e visualização em lista ou árvore
- Favoritos emparelhados de caminhos locais e remotos por conexão
- Fila paralela com limites de velocidade, tempo restante, pausa, retomada e novas tentativas; SFTP interrompido volta pausado após reiniciar
- Sobrescrever, ignorar ou renomear em conflitos, para um arquivo ou todos
- SFTP com alternativa automática SCP quando não há subsistema SFTP
- Conexões SFTP independentes e transferências entre servidores intermediadas pelo aplicativo
- FTP e FTPS explícito
- Arraste arquivos para um terminal SSH para enviá-los ao diretório atual
- `trz`/`tsz` (trzsz) e `rz`/`sz` (ZMODEM) no shell remoto; XMODEM, YMODEM e ZMODEM em conexões seriais
- Prévias de texto, código, Markdown, imagens, áudio, vídeo, hexadecimal e fontes
- Visualizadores PDF, SQLite, certificados e binários *(plugin)*
- Ir do terminal ao mesmo diretório SFTP e enviar um diretório SFTP ao terminal

### Editor e IDE

- IDE remota por SFTP, comandos shell simples ou agente remoto com índice de símbolos, completamento e monitoramento de arquivos
- Converter terminal ocioso em IDE com o mesmo usuário, contêiner e diretório
- Abas, árvore com seleção múltipla, busca no projeto e substituição com regex e grupos de captura
- Arquivos de até 100 MiB, preservando codificação, BOM e finais de linha
- Salvamento seguro com conflitos e salvamento automático opcional ao trocar de aba
- Restauração de arquivos abertos e alterações não salvas após reiniciar
- Visualizações Git de branches, worktrees, staging, histórico, commits à frente/atrás e estados de merge/rebase
- Detecção e execução de tarefas do projeto
- Destaque de sintaxe para 15 linguagens integrado e mais de 20 adicionais via plugins *(plugin)*
- Prévia Markdown com tabelas, fórmulas e notas de rodapé

### Encaminhamento de portas e rede

- Encaminhamentos locais, remotos e dinâmicos SOCKS5 com regras salvas que podem iniciar com a conexão
- Detecção de portas em escuta e encaminhamento com um clique
- Túneis SSH independentes sem terminal, opcionalmente desde a abertura do aplicativo
- Restauração após reconectar; regras paradas manualmente continuam paradas
- Proxy global e certificados confiáveis do sistema, incluindo CAs empresariais, para solicitações de rede do aplicativo
- Topologia das conexões e depuração pontual de sockets

### Desktop remoto

- RDP e VNC com teclado, mouse, área de transferência, escala e reconexão *(plugin)*
- RDP em vários monitores com perfis de qualidade da rede *(plugin)*
- VNC via VeNCrypt TLS *(plugin)*
- RDP e VNC por hosts de salto SSH *(plugin)*
- Aplicativos WSLg em visualizador integrado (Windows)

### Ferramentas do host

- Para a máquina local e qualquer servidor SSH
- CPU, memória, swap, discos, rede, carga e latência com gráficos de tendências
- Processos (sinais, pausa/continuação, nice), serviços (systemd, launchd, BSD, Windows), tarefas agendadas e pacotes; atualizações e desinstalações winget no Windows
- Logs com predefinições e acompanhamento, portas com indicações de exposição e sistemas de arquivos com alertas de espaço
- Contêineres Docker: status, portas, iniciar, parar, reiniciar e logs
- GPU/NPU para NVIDIA, AMD ROCm, Hygon DCU, Huawei Ascend, Intel XPU, Moore Threads MUSA e Cambricon
- Sessões tmux, Zellij e Herdr: anexar, criar, dividir e renomear
- Painéis do pool e matriz de conexões; latência SSH na barra do terminal

### IA OxideSens

- OpenAI (incluindo Responses API), Anthropic, Gemini, DeepSeek, xAI Grok, Xiaomi MiMo, Ollama ou endpoint compatível OpenAI, com controle do esforço de raciocínio
- Contexto dos terminais, arquivos e ajustes selecionados; ferramentas para comandos, programas interativos, edição de arquivos e transferência de dados
- Modos somente leitura, padrão ou sem aprovação; permissões por categoria, políticas de comandos e revisão adicional de ações destrutivas ou com credenciais
- Imagens anexas, ramificações de conversas, edição de mensagens e perguntas seguintes em fila
- Subagentes paralelos, tarefas em segundo plano e retomada após interrupção
- Codex e Claude Code via ACP; inicializadores OpenCode, Antigravity, Cursor, Grok Build, GitHub Copilot CLI, Qwen Code e Kimi CLI *(plugin)*
- Servidores MCP por stdio, Streamable HTTP ou SSE legado
- Controle MCP externo: editores e outras ferramentas IA usam conexões salvas pelo OxideTerm, com permissões revogáveis por grupo
- [Agent Skills](../../docs/agent-skills.md) (`SKILL.md`) com completamento `/skill-name`
- Notas Markdown com busca BM25 e vetorial HNSW, segmentação CJK e diagramas Mermaid nas prévias

### Segurança

- Bloqueio com Touch ID (macOS) ou Windows Hello (Windows), mantendo conexões ativas
- Senhas sudo/su salvas oferecidas na solicitação, limitadas à máquina local, conexão ou padrão de host; TOTP opcional para solicitações de duas etapas
- Auditoria com filtros, retenção e exportação JSON ou CSV
- Gravações cifradas com reprodução, navegação temporal e controle de velocidade
- Revisão de permissões antes de ativar cada plugin

### Sincronização e backup

- WebDAV, armazenamento compatível S3 (incluindo Alibaba Cloud OSS), repositório Git, GitHub Gist, Dropbox, OneDrive, Google Drive ou HTTP JSON
- Escolha dos dados, mesclagem entre dispositivos e resolução individual dos conflitos
- Backups de reversão e recuperação de sincronização interrompida
- Pacotes `.oxide` cifrados para conexões, encaminhamentos, ajustes, comandos rápidos e chaves; ignorar, renomear, substituir ou mesclar ao importar
- Modo portátil a partir de pasta com armazenamento de chaves cifrado próprio
- Atualizações integradas pelos canais estável ou beta

### Personalização

- Aparência clara, escura ou do sistema, temas separados para aplicativo e terminal e editor de temas
- Fundos com imagens, animações e vídeos; efeitos de neblina, partículas, meteoros e cáusticas aquáticas, com pacotes extras *(plugin)*
- Opacidade e estilos dos cantos das janelas
- Pilhas de fontes com CJK, ligaduras e pesos separados, além de tipografia própria do editor
- Escala da interface, cursor, espaçamento interno e movimento
- Atalhos personalizados e configurações pesquisáveis e reorganizáveis
- 11 idiomas, entrada IME e texto bidirecional

### Plugins

| Tipo | Execução | Acesso |
|---|---|---|
| **Manifesto** | Extensão declarativa | Nenhum; não executa código |
| **WASM** | Wasmtime/WASI ou processo auxiliar | Apenas capacidades autorizadas pelo host |
| **Processo** | Programa local comum | Acesso de um programa local; sem sandbox do sistema operacional |

Instale plugins de processo apenas de fontes confiáveis. Plugins para OxideTerm 1.x podem aparecer, mas 2.x não os executa.

### Ferramenta de linha de comando

Instale `oxideterm` em **Configurações → Geral → CLI Companion**. Compartilha a configuração do aplicativo para verificar, automatizar e fazer backup sem janela, ou abrir abas por qualquer shell. Comandos de alteração mostram o plano com `--dry-run`, só aplicam com `--yes` e mantêm backup para reversão.

```bash
oxideterm terminal --cwd ~/project
oxideterm doctor --strict
oxideterm connections search prod
oxideterm forwards list --format json
oxideterm oxide export ./profile.oxide --connection prod --password-stdin
```

Completamento para Bash, Zsh, Fish, PowerShell e Elvish. O [guia CLI (inglês)](../../docs/user-guide/en/cli.md) lista todos os comandos.

---

<a id="install"></a>

## Instalação

Baixe OxideTerm na [versão mais recente](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest).

| Sistema operacional | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Instalador (`.exe`) | Instalador (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

A página inclui arquivos portáteis, assinaturas e `sha256sums.txt` para verificar o download.

### macOS

Se o Gatekeeper bloquear o aplicativo, remova o atributo de quarentena:

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

Se o SmartScreen exibir um aviso, escolha **Mais informações → Executar assim mesmo**.

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

### Se a janela não for desenhada

Defina `OXIDETERM_RENDER_PROFILE=compatibility` no ambiente e reinicie OxideTerm.

### Primeiros passos

1. Abra o **gerenciador de sessões** e adicione um servidor ou importe hosts de `~/.ssh/config`.
2. Conecte-se para abrir um terminal.
3. No mesmo servidor, abra SFTP, encaminhamentos ou IDE.
4. Para OxideSens, adicione um provedor IA em **Configurações**.

A [documentação](https://oxideterm.app) cobre o restante. Para compilar, veja [Compilar a partir do código-fonte](#build-from-source).

---

## Privacidade e segurança

- Senhas e chaves no chaveiro do sistema (macOS Keychain, Windows Credential Manager ou libsecret no Linux). Chaves IA não são incluídas nos logs.
- Segredos em memória são apagados com zeroize ao serem liberados.
- Chaves de host confiadas no primeiro uso e verificadas com `~/.ssh/known_hosts`; mudanças são rejeitadas.
- Texto enviado a provedores IA passa primeiro pela ocultação de credenciais.
- Gravação desativada por padrão, cifrada no dispositivo, excluída da sincronização e sem capturar o que você digita.
- Notificações e auditoria desativadas por padrão. Quando ativadas, os dados ficam no dispositivo com detalhes sensíveis cifrados e cobrem apenas ações dentro do OxideTerm.
- Pacotes `.oxide` cifrados com ChaCha20-Poly1305 e chave derivada por Argon2id (256 MB de memória, 4 iterações).

---

<a id="build-from-source"></a>

## Compilar a partir do código-fonte

É necessário Rust (rustup instala a toolchain fixada em `rust-toolchain.toml`) e um desktop capaz de executar GPUI.

```bash
# Run the app
cargo run

# Build the oxideterm CLI
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Com Nix: `nix build .#oxideterm`, `nix run .#oxideterm` ou `nix develop`. A CLI é preparada em `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

### Arquitetura

Interface, terminais e SSH compartilham um processo Rust. Agente remoto opcional e auxiliares de plataforma são separados. A saída SSH ou PTY local entra diretamente em `TerminalState`, desenhado por GPUI, sem WebView, ponte JSON ou xterm.js.

`NodeRouter` distribui conexões de `SshConnectionRegistry`, um pool baseado em DashMap. Terminais, SFTP, encaminhamentos e editor compartilham uma conexão por servidor, salvo terminal configurado com conexão própria. Estados: `connecting → active → idle → link_down → reconnecting`. Falhas no host de salto levam servidores dependentes a `link_down`. Cada regra de encaminhamento tem listener próprio. Após reconectar, as sessões SFTP são readquiridas; operações da conexão antiga nunca são transferidas silenciosamente.

```text
crates/
├── oxideterm-gpui-app/        # Desktop app: windows, workspace, settings
├── oxideterm-gpui-terminal/   # Terminal panes: input, selection, overlays
├── oxideterm-terminal/        # Terminal model and escape-sequence parsing
├── oxideterm-ssh/             # SSH connections, NodeRouter, jump hosts
├── oxideterm-sftp/            # SFTP sessions and transfers
├── oxideterm-forwarding/      # Port forwarding runtime
├── oxideterm-gpui-ide/        # Remote IDE
├── oxideterm-ai/              # OxideSens: providers, MCP, RAG
├── oxideterm-cli/             # The oxideterm command
├── oxideterm-i18n/            # Locale catalogs for the 11 interface languages
└── gpui-ce/                   # Patched copy of GPUI
```

Leituras (inglês): [desenvolvimento](../../docs/development/README.md), [mapa de crates](../../docs/development/crate-map.md) e [arquitetura](../../docs/user-guide/en/architecture.md).

---

## Como contribuir

Código, documentação, traduções, plugins, testes e reproduções de bugs são bem-vindos. Para mudanças grandes, abra uma issue primeiro. [CONTRIBUTING.md](../../docs/CONTRIBUTING.md) explica a preparação de PRs.

Um diagnóstico com dados sensíveis ocultos facilita corrigir bugs:

```bash
oxideterm report --bundle ./oxideterm-report.zip
```

A partir do código-fonte, use `cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip`.

### Colaboradores

Obrigado a todos que ajudam a tornar o OxideTerm melhor.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="Colaboradores do OxideTerm">
  </a>
</p>

---

## Agradecimentos

OxideTerm utiliza estes projetos de código aberto:

- [russh](https://github.com/warp-tech/russh) e [ring](https://github.com/briansmith/ring): SSH e criptografia
- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui): framework de interface GPU do Zed
- [alacritty_terminal](https://github.com/alacritty/alacritty): emulação de terminal (VT100–VT500)
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty): shells locais com ConPTY no Windows
- [Tokio](https://tokio.rs): runtime assíncrono
- [Wasmtime](https://wasmtime.dev/): execução de plugins WASM
- [tree-sitter](https://tree-sitter.github.io/): destaque de sintaxe no editor

---

## Licença

OxideTerm usa [GPL-3.0-only](../../LICENSE), sem restrições adicionais. Avisos de dependências: [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) e [NOTICE](../../NOTICE).

Use OxideTerm apenas em sistemas próprios ou com acesso autorizado. [LEGAL.md](../../LEGAL.md) contém o aviso legal completo.

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
