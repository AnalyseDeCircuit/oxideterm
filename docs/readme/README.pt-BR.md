<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>A próxima edição zero-WebView do OxideTerm.</strong>
  <br>
  Conecte-se uma vez a uma máquina remota e trabalhe com shell, arquivos, portas, transferências, editor leve, consoles seriais e BYOK AI em um workspace Rust nativo.
  <br>
  App GPUI nativo · SSH puro em Rust · sem conta para os workflows SSH principais
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust all the way down.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Versão">
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

> **Status da release:** OxideTerm Native está sendo preparado como a próxima grande versão do OxideTerm. Os instaladores públicos ainda não foram publicados; por enquanto, execute pelo código-fonte. As releases empacotadas atuais continuam na linha Tauri até que os instaladores native estejam prontos.

## O que você pode fazer

- Gerenciar terminais SSH, SFTP, port forwards, consoles seriais, shells locais e edição leve em um workspace nativo
- Manter o trabalho remoto vivo durante instabilidades de rede com Grace Period reconnect
- Usar seu próprio provedor de IA para inspecionar sessões ao vivo e executar ações aprovadas do workspace

---

## Por que OxideTerm Native?

| Se você se importa com... | OxideTerm Native oferece... |
|---|---|
| Um nó remoto, muitas ferramentas | Terminal, SFTP, port forwarding, trzsz, IDE nativo, monitoramento e contexto de IA ficam no mesmo workspace SSH |
| Shell nativo zero-WebView | GPUI desenha a UI desktop diretamente numa superfície GPU, sem DOM, CSS, JavaScript, Chromium ou runtime WebKit |
| Workflows SSH local-first | SSH, SFTP, forwarding, shell local, terminais seriais e configuração funcionam sem cadastro |
| BYOK AI em vez de créditos de plataforma | OxideSens usa seu endpoint OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible com suporte MCP e RAG |
| Reconexão estável | Grace Period sonda a conexão antiga por 30 s antes de substituí-la, para que TUIs sobrevivam a quedas curtas |
| SSH puro em Rust e credenciais seguras | `russh` + `ring`, sem OpenSSL/libssh2; senhas e chaves API ficam no keychain do OS, `.oxide` usa ChaCha20-Poly1305 + Argon2id |

## O que é / o que não é

OxideTerm Native foca no mesmo **workspace SSH local-first** do OxideTerm, reconstruído como app desktop GPUI em Rust puro. Ele é feito para usuários que querem terminal, arquivos, portas, transferências, edição leve, consoles seriais e contexto de IA ao redor de suas próprias máquinas e nós remotos.

Ainda não é a linha estável de download atual, nem uma plataforma cloud de agentes. Também não é Electron, Tauri ou terminal web: sem Chromium, sem WebView, sem JavaScript, sem CSS.

---

## Capturas de tela

A UI nativa segue o mesmo modelo de workspace e linguagem visual do OxideTerm da linha Tauri atual.

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH com barra lateral OxideSens AI" /></td>
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
| Tamanho do artefato | Instaladores geralmente ~150–200 MB | macOS arm64 atual: portable/DMG comprimido ~50–60 MB; binário release bruto ~132 MB |

## Funcionalidades

| Categoria | Funcionalidades |
|---|---|
| Terminal | Local PTY, SSH, local serial terminals, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens com OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG e aprovação de comandos |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, import/export criptografado |
| Plugins / CLI | WASM sandbox, native host API, plugin settings; CLI para settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Arquitetura

OxideTerm Native remove a ponte WebView e mantém terminal, SSH, SFTP, forwarding, IDE, IA, plugins e CLI em uma arquitetura Rust-native. Os detalhes completos ficam preservados abaixo.

<details>
<summary><strong>Arquitetura, internals SSH, shell GPUI, reconexão, IA, plugins e mais</strong></summary>
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

Não existe fronteira de serialização entre UI e backend SSH/terminal. Os bytes do terminal modificam `TerminalState` diretamente; GPUI lê o estado e emite draw calls GPU.

</details>

---

## Executar pelo código-fonte

Os instaladores native públicos ainda não foram publicados. Até que os builds empacotados estejam prontos, execute a edição native pelo código-fonte.

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
- [ ] Instaladores públicos empacotados
- [ ] Full ProxyCommand, audit logging

## Contribuição

## Neutralidade de providers

OxideTerm é BYOK-first e neutro em relação a providers.

Integrações de providers existem para ajudar usuários a conectar as ferramentas em que já confiam. Elas não são um ranking, um outdoor ou um sistema de recompensa para quem pede atenção com mais entusiasmo.

Compatibilidade, manutenibilidade, segurança e valor real para usuários decidem o que entra na documentação. Visibilidade segue utilidade, não entusiasmo.

Quando uma função já existir na versão Tauri, mantenha comportamento, textos, estados de interação e workflows alinhados. Cada crate novo precisa ter responsabilidade real, não apenas re-export.

## Suporte e manutenção

OxideTerm Native está sendo preparado como a próxima versão major do OxideTerm e é mantido best-effort. Bugs com passos reproduzíveis e diagnósticos redigidos são priorizados; requests de funcionalidade podem não ser implementados.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Se OxideTerm ajuda seu workflow, uma estrela no GitHub, reprodução de issue, correção de tradução, plugin ou pull request ajudam o projeto a continuar.

---

## Licença / Agradecimentos

**GPL-3.0-only**. Notices de terceiros ficam em `NOTICE`. Obrigado a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` e `tree-sitter`.
