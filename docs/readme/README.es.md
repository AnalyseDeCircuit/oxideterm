<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Workspace SSH local-first para quienes quieren el flujo de OxideTerm sin WebView.</strong>
  <br>
  Conéctate una vez a una máquina remota y trabaja con shell, archivos, puertos, transferencias, editor ligero, consolas serie y AI BYOK desde un workspace Rust nativo.
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Rust puro, de arriba abajo.</strong>
</p>

<p align="center">
  <em>Si quieres un workspace SSH local-first sin Electron, WebView, telemetría ni suscripciones, dale una estrella a OxideTerm para que más usuarios de SSH puedan encontrarlo.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-0.1.0-blue" alt="Versión">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plataforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licencia">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Próxima gran edición nativa de <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — renderizada por GPU, zero-WebView, usando <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (framework de renderizado de Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

---

> **Estado de release:** OxideTerm Native se está preparando como la próxima gran versión de OxideTerm. Los instaladores públicos aún no están publicados; por ahora ejecútalo desde el código fuente. Las releases empaquetadas actuales siguen en la línea Tauri hasta que los instaladores native estén listos.

## ¿Por qué OxideTerm Native?

| Si te importa... | OxideTerm Native ofrece... |
|---|---|
| Un workspace SSH, no solo shell | Terminal, SFTP, forwarding, trzsz, mini IDE, monitoring y contexto AI alrededor de un nodo |
| Shell local, consolas serie y SSH remoto juntos | zsh/bash/fish/pwsh/WSL2, terminales serie locales y SSH en el mismo flujo |
| Sin cuenta cloud | SSH, SFTP, forwarding, shell local y config funcionan local-first |
| AI BYOK | Tus propios endpoints OpenAI, Anthropic, Gemini, Ollama o compatibles |
| Sin WebView | GPUI dibuja directamente en una GPU surface, sin DOM, CSS ni JavaScript |
| Sin serialización en el hot path | Los bytes del terminal mutan estado Rust directamente, sin WebSocket/JSON/Base64 |
| Sin OpenSSL | SSH puro Rust con `russh` + `ring` |
| Reconexión estable | Grace Period prueba la conexión antigua antes de matar apps TUI |
| Archivos remotos | SFTP integrado e IDE nativo para navegar, previsualizar, transferir y editar |
| Seguridad de credenciales | Keychain del SO; `.oxide` con ChaCha20-Poly1305 + Argon2id |

## Qué es / qué no es

OxideTerm Native es un **workspace SSH de escritorio nativo en Rust puro** y la próxima gran edición de OxideTerm. Lleva la misma idea de producto — terminal, SFTP, forwarding, edición, AI, cloud sync, plugins y CLI alrededor de un nodo remoto — a una UI GPUI sin runtime de navegador.

No es Electron, Tauri, un terminal web ni un servicio hospedado. No hay Chromium, WebView, JavaScript ni CSS.

## Diferencias frente a WebView/Tauri

| Aspecto | WebView/Tauri | Native |
|---|---|---|
| Render | Chromium/Safari/WebKit2GTK + CSS | GPUI, GPU surface, immediate mode, Rust puro |
| Flujo terminal | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC por comando | Llamadas in-process |
| SSH keepalive | Timer JavaScript | Tarea async Rust |
| Plugins | ESM en sandbox browser | WASM wasmtime + typed Rust host API |
| CLI | Requiere app desktop | Binario standalone |
| Tamaño del artefacto | Instaladores de ~150–200 MB normalmente | macOS arm64 actual: portable/DMG comprimido de ~50–60 MB; binario release sin comprimir de ~132 MB |

## Funciones

| Categoría | Funciones |
|---|---|
| Terminal | Local PTY, SSH, local serial terminals, split panes, shell integration, command marks, asciicast, trzsz, Sixel/Kitty graphics, rendering policy |
| SSH & Auth | connection pool, unlimited ProxyJump, Grace Period reconnect, Host-key TOFU, SSH Agent forwarding, password/key/cert/keyboard-interactive |
| SFTP / IDE | dual-pane browser, transfer queue, preview, bookmarks, atomic writes, remote file tree, multi-tab editor, conflict resolution |
| Forwarding | Local, Remote, Dynamic SOCKS5, saved rules, reconnect restore, death reporting, idle timeout |
| AI | OxideSens con OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG y aprobación de comandos |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, import/export cifrado |
| Plugins / CLI | sandbox WASM, native host API, settings por plugin; CLI para settings, connections, forwards, plugins, secrets, cloud-sync, backup, report |

## Arquitectura

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

No hay frontera de serialización entre la UI y el backend SSH/terminal. Los bytes del terminal modifican `TerminalState` directamente; GPUI lee ese estado y emite draw calls GPU.

## Ejecutar desde código fuente

Los instaladores native públicos aún no están publicados. Hasta que los builds empaquetados estén listos, ejecuta la edición native desde el código fuente.

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

## Seguridad

| Tema | Implementación |
|---|---|
| Passwords & keys | macOS Keychain / Windows Credential Manager / libsecret |
| Memoria secreta | `zeroize` / `Zeroizing` |
| Diagnóstico & contexto AI | secretos redactados antes de salida o llamadas a proveedores |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Escrituras CLI | dry-run plans, guardas `--yes`, rollback backups |
| Plugins | aislamiento wasmtime y capability-based host API |

## Estado de release

- [x] SSH Agent forwarding, Grace Period reconnect, GPUI desktop shell
- [x] Flujo terminal in-process sin WebSocket
- [x] SFTP, forwarding, IDE, AI, cloud sync, plugins, CLI
- [x] Terminales serie locales
- [ ] Instaladores públicos empaquetados
- [ ] Full ProxyCommand, audit logging

## Contribuir

## Neutralidad de providers

OxideTerm es BYOK-first y neutral respecto a los providers.

Las integraciones de providers existen para ayudar a los usuarios a conectar las herramientas en las que ya confían. No son un ranking, un cartel publicitario ni un sistema de recompensa para quien pida atención con más entusiasmo.

La compatibilidad, mantenibilidad, seguridad y valor real para el usuario deciden qué se documenta. La visibilidad sigue a la utilidad, no al entusiasmo.

Si una función ya existe en Tauri, mantén comportamiento, textos, estados de interacción y workflows alineados salvo que haya un reemplazo documentado. Cada crate nuevo debe tener responsabilidad real de dominio.

## Licencia / Agradecimientos

**GPL-3.0-only**. Los avisos de terceros están en `NOTICE`. Gracias a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` y `tree-sitter`.
