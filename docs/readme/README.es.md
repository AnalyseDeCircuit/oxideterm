<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>La próxima edición zero-WebView de OxideTerm.</strong>
  <br>
  Conéctate una vez a una máquina remota y trabaja con shell, archivos, puertos, transferencias, editor ligero, consolas serie y BYOK AI desde un workspace Rust nativo.
  <br>
  App GPUI nativa · SSH puro en Rust · sin cuenta para los workflows SSH principales
  <br>
  <strong>Zero WebView. Zero OpenSSL. Zero Telemetry. Zero Subscription. BYOK-first. Pure Rust all the way down.</strong>
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

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abre una terminal dentro de OxideTerm" width="920">
</a>

*OxideSens sigue una petición del usuario y abre una terminal dentro de OxideTerm.*

</div>

---

> **Estado de release:** OxideTerm Native se está preparando como la próxima gran versión de OxideTerm. Los instaladores públicos aún no están publicados; por ahora ejecútalo desde el código fuente. Las releases empaquetadas actuales siguen en la línea Tauri hasta que los instaladores native estén listos.

## Qué puedes hacer

- Gestionar terminales SSH, SFTP, port forwards, consolas serie, shells locales y edición ligera en un workspace nativo
- Mantener vivo el trabajo remoto ante cortes de red con Grace Period reconnect
- Usar tu propio proveedor de IA para inspeccionar sesiones en vivo y ejecutar acciones aprobadas del workspace

---

## ¿Por qué OxideTerm Native?

| Si te importa... | OxideTerm Native te da... |
|---|---|
| Un nodo remoto, muchas herramientas | Terminal, SFTP, port forwarding, trzsz, IDE nativo, monitorización y contexto de IA permanecen unidos al mismo workspace SSH |
| Shell nativa zero-WebView | GPUI dibuja la UI directamente sobre una superficie GPU, sin DOM, CSS, JavaScript, Chromium ni WebKit runtime |
| Workflows SSH local-first | SSH, SFTP, forwarding, shell local, terminales serie y configuración funcionan sin registro |
| BYOK AI en vez de créditos de plataforma | OxideSens usa tu endpoint OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible con soporte MCP y RAG |
| Reconexión estable | Grace Period sondea la conexión anterior durante 30 s antes de reemplazarla, para que las TUI sobrevivan a cortes breves |
| SSH puro en Rust y credenciales seguras | `russh` + `ring`, sin OpenSSL/libssh2; contraseñas y claves API quedan en el llavero del sistema, `.oxide` usa ChaCha20-Poly1305 + Argon2id |

## Qué es / qué no es

OxideTerm Native se centra en el mismo **workspace SSH local-first** que OxideTerm, reconstruido como app de escritorio GPUI en Rust puro. Está pensado para usuarios que quieren mantener terminal, archivos, puertos, transferencias, edición ligera, consolas serie y contexto de IA alrededor de sus propias máquinas y nodos remotos.

Todavía no es la línea estable de descarga actual, ni una plataforma cloud de agentes. Tampoco es Electron, Tauri ni una terminal web: sin Chromium, sin WebView, sin JavaScript, sin CSS.

---

## Capturas de pantalla

La UI nativa sigue el mismo modelo de workspace y lenguaje visual de OxideTerm que la línea Tauri actual.

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH con barra lateral OxideSens AI" /></td>
<td align="center"><strong>Gestor de archivos SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Gestor de archivos SFTP de doble panel con cola de transferencias" /></td>
</tr>
<tr>
<td align="center"><strong>IDE integrado</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Modo IDE integrado" /></td>
<td align="center"><strong>Reenvío de puertos inteligente</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Reenvío de puertos inteligente con detección automática" /></td>
</tr>
</table>

---

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

OxideTerm Native elimina el puente WebView y mantiene terminal, SSH, SFTP, forwarding, IDE, IA, plugins y CLI en una arquitectura Rust nativa. Los detalles completos se conservan abajo.

<details>
<summary><strong>Arquitectura, internals SSH, shell GPUI, reconexión, IA, plugins y más</strong></summary>
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

No hay frontera de serialización entre la UI y el backend SSH/terminal. Los bytes del terminal modifican `TerminalState` directamente; GPUI lee ese estado y emite draw calls GPU.

</details>

---

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

## Soporte y mantenimiento

OxideTerm Native se prepara como la próxima versión mayor de OxideTerm y se mantiene best-effort. Se priorizan bug reports con pasos reproducibles y diagnósticos redactados; los feature requests no siempre se implementarán.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Si OxideTerm ayuda a tu workflow, una estrella, reproducción de issue, corrección de traducción, plugin o pull request hacen más fácil mantener el proyecto.

---

## Licencia / Agradecimientos

**GPL-3.0-only**. Los avisos de terceros están en `NOTICE`. Gracias a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` y `tree-sitter`.
