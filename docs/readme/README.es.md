<h1 align="center">⚡ OxideTerm — Native</h1>

<p align="center">
  <strong>Espacio de trabajo operativo nativo con IA para servidores remotos — aplicación nativa en Rust puro</strong>
  <br>
  Terminales SSH, Telnet, serie, RDP/VNC, SFTP, reenvío de puertos, Raw TCP/UDP y edición ligera en un espacio de trabajo nativo.
  <br>
  Renderizado GPU. Gratis. Sin necesidad de cuenta.
  <br>
  <strong>Sin WebView. Sin OpenSSL. Sin telemetría. Sin suscripción. BYOK primero. SSH puro en Rust.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.0--gpui--preview.12-blue" alt="Versión">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plataforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licencia">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Próxima gran edición nativa de <a href="https://github.com/AnalyseDeCircuit/oxideterm">OxideTerm</a> — renderizada por GPU, sin WebView, usando <a href="https://github.com/zed-industries/zed/tree/main/crates/gpui">GPUI</a> (el framework de renderizado de Zed)</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="Resumen de funciones de OxideTerm Native" width="920">
</p>

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abre una terminal dentro de OxideTerm" width="920">
</a>

*OxideSens sigue una petición del usuario y abre una terminal dentro de OxideTerm.*

</div>

---

## Qué puedes hacer

- Gestionar SSH, Telnet, serie, RDP/VNC, SFTP, reenvíos de puertos, Raw TCP/UDP, shells locales y edición ligera en un espacio de trabajo nativo
- Mantener vivo el trabajo remoto ante cortes de red con la reconexión Grace Period
- Pedir a OxideSens AI que inspeccione sesiones activas y ejecute acciones aprobadas del espacio de trabajo mediante tu propio proveedor de IA

---

## ¿Por qué OxideTerm Native?

| Si te importa... | OxideTerm Native te da... |
|---|---|
| Un nodo remoto, muchas herramientas | Terminal, SFTP, reenvío de puertos, RDP/VNC, Raw TCP/UDP, trzsz, IDE nativo, monitorización y OxideSens AI permanecen unidos al mismo espacio de trabajo |
| Shell nativa sin WebView | GPUI dibuja la interfaz directamente sobre una superficie GPU, sin DOM, CSS, JavaScript, Chromium ni runtime WebKit |
| Flujos operativos locales primero | SSH, Telnet, SFTP, reenvío, RDP/VNC, Raw TCP/UDP, shell local, terminales serie y configuración funcionan sin registro |
| OxideSens AI con BYOK en vez de créditos de plataforma | OxideSens usa tu punto de acceso OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible con MCP, RAG y acciones aprobadas del espacio de trabajo |
| Reconexión estable | Grace Period sondea la conexión anterior durante 30 s antes de reemplazarla, para que las TUI sobrevivan a cortes breves |
| SSH puro en Rust y credenciales seguras | `russh` + `ring`, sin OpenSSL/libssh2; contraseñas y claves API quedan en el llavero del sistema, `.oxide` usa ChaCha20-Poly1305 + Argon2id |

## Qué es / qué no es

OxideTerm Native se centra en un **espacio de trabajo de IA local primero para servidores remotos**, reconstruido como aplicación de escritorio GPUI en Rust puro. Está pensado para usuarios que quieren mantener terminales, escritorios remotos, sockets sin procesar, archivos, puertos, transferencias, edición ligera, consolas serie y OxideSens AI alrededor de sus propias máquinas y nodos remotos.

No es una plataforma cloud de agentes. Tampoco es Electron, Tauri ni una terminal web: sin Chromium, sin WebView, sin JavaScript, sin CSS.

---

## Capturas de pantalla

La interfaz nativa sigue el mismo modelo de espacio de trabajo y el mismo lenguaje visual de OxideTerm que la línea Tauri actual.

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH con OxideSens AI" /></td>
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
| Renderizado | Chromium/Safari/WebKit2GTK + CSS | GPUI, superficie GPU, modo inmediato, Rust puro |
| Flujo terminal | WebSocket → JS event loop → xterm.js | Rust input → `TerminalState` → GPUI render |
| IPC | JSON-RPC por comando | Llamadas en proceso |
| SSH keepalive | Timer JavaScript | Tarea async Rust |
| Plugins | ESM en sandbox del navegador | WASM wasmtime + API de host Rust tipada |
| CLI | Requiere app desktop | Binario standalone |
| Límite de runtime | Runtime de navegador + puente WebView | Proceso nativo; sin runtime de navegador incluido |

## Funciones

| Categoría | Funciones |
|---|---|
| Terminal | PTY local, SSH, Telnet, terminales Raw TCP/UDP, terminales serie locales, paneles divididos, integración de shell, marcas de comandos, asciicast, trzsz, Sixel/Kitty graphics, política de renderizado |
| SSH & Auth | pool de conexiones, ProxyJump ilimitado, reconexión Grace Period, TOFU de clave de host, reenvío de SSH Agent, password/key/cert/keyboard-interactive |
| SFTP / IDE | navegador de doble panel, cola de transferencias, vista previa, marcadores, escrituras atómicas, árbol remoto de archivos, editor multipestaña, resolución de conflictos |
| Forwarding | Local, Remote, Dynamic SOCKS5, reglas guardadas, restauración tras reconexión, informe de finalización, tiempo de inactividad |
| Escritorio remoto | Pestañas RDP y VNC integradas, controles de reconexión, tamaño según viewport, teclado, ratón, portapapeles y cursor |
| Raw TCP/UDP | Terminales Raw TCP y Raw UDP para depurar servicios puntuales, protocolos de dispositivos y datagramas |
| AI | OxideSens con OpenAI, Anthropic, Gemini, Ollama/compatible, MCP, RAG y aprobación de comandos |
| Cloud Sync / `.oxide` | push/pull/apply/resolve, S3/WebDAV/Git, rollback backups, import/export cifrado |
| Plugins / CLI | sandbox WASM, API de host nativa, ajustes por plugin; CLI para settings, connections, reenvíos, plugins, secrets, cloud-sync, backup, report |

## Arquitectura

OxideTerm Native elimina el puente WebView y mantiene terminal, SSH, Telnet, RDP, VNC, Raw TCP/UDP, SFTP, reenvío, IDE, IA, plugins y CLI en una arquitectura Rust nativa. Los detalles completos se conservan abajo.

<details>
<summary><strong>Arquitectura, interiores SSH, shell GPUI, reconexión, IA, plugins y más</strong></summary>
<br>

### Arquitectura — proceso único, cero bridge

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ en proceso Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
```

No hay frontera de serialización entre la UI y el backend SSH/terminal. Los bytes del terminal modifican `TerminalState` directamente; GPUI lee ese estado y emite draw calls GPU.

### SSH puro en Rust — russh (ring)

La edición nativa enlaza el mismo stack `russh` de la línea Tauri directamente dentro del binario desktop:

- **Cero dependencias OpenSSL** mediante `ring`
- SSH2 completo: intercambio de claves, canales, subsistema SFTP y reenvío de puertos
- ChaCha20-Poly1305 / AES-GCM, claves Ed25519/RSA/ECDSA
- SSH Agent en Unix (`SSH_AUTH_SOCK`) y Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop con autenticación independiente en cada salto

### Reconexión inteligente con Grace Period

La semántica de reconexión coincide con la línea Tauri, pero la orquestación corre por completo en tareas async de Rust:

1. Detectar timeout de SSH keepalive sin JavaScript timer throttling
2. Tomar instantánea de paneles de terminal, transferencias SFTP, reenvíos y archivos IDE
3. Probar la conexión anterior durante 30 segundos de Grace Period para que las TUI sobrevivan a cambios de red
4. Si no se recupera, reconectar, restaurar reenvíos, reanudar transferencias y reabrir archivos IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### Pool de conexiones SSH y ruteo por nodo

`SshConnectionRegistry` usa `DashMap` y conserva el modelo node-first de Tauri sin el puente de ciclo de vida WebSocket:

- Una conexión SSH física puede servir paneles de terminal, SFTP, reenvíos de puertos y trabajo IDE
- Cada conexión pasa por `connecting → active → idle → link_down → reconnecting`
- La UI envía comandos por `nodeId`; `NodeRouter` resuelve atómicamente el `connectionId` activo
- `NodeRuntimeStore` persiste instantáneas de topología en `session_tree.json`
- La caída de un jump host propaga `link_down` a nodos descendientes

### OxideSens AI

OxideSens sigue siendo BYOK primero, con construcción de contexto dentro del proceso:

- Proveedores: OpenAI, Anthropic, Gemini, Ollama o cualquier punto de acceso OpenAI-compatible
- MCP: transports stdio y SSE, descubrimiento e invocación de herramientas
- RAG: texto completo BM25, índice vectorial HNSW, Reciprocal Rank Fusion, tokenizador CJK bigram
- El contexto AI viene del estado del espacio de trabajo; las credenciales se redactan antes de llamar al proveedor
- Las claves API quedan en el llavero del sistema y no entran en registros ni tramas IPC

### Shell desktop GPUI

La UI se dibuja directamente con GPUI, sin pipeline DOM/CSS/JavaScript:

- Tipos de pestaña del espacio de trabajo: terminales locales, SSH, Telnet, serie, RDP, VNC y Raw TCP/UDP, SFTP, IDE, Forwards, Settings, plugins, Topology y más
- Árbol binario de panes con divisores arrastrables, hasta cuatro panes por pestaña terminal
- Command palette, atajos globales y sidebars hechos con primitives de GPUI
- Immediate-mode rendering reacciona al estado Rust sin round-trip de serialización

### Estado del terminal y renderizado

El renderizado del terminal se modela primero como estado Rust y después GPUI lo dibuja:

- La salida PTY llega a `TerminalState`; scrollback, cursor, selección, marks y estado de búsqueda quedan en Rust
- La política de renderizado puede cambiar entre Boost, Normal e Idle sin esperar cooperación del browser event loop
- Los gráficos Sixel y Kitty se rastrean como assets propios del terminal, no como DOM nodes ni canvas overlays
- Split panes comparten el mismo modelo de estado del espacio de trabajo, por lo que la restauración de pestañas y la reconexión pueden tomar una instantánea juntos la topología del terminal

### Workspace SFTP e IDE

Los archivos remotos forman parte del mismo node espacio de trabajo, no de una función separada:

- Las sesiones SFTP se resuelven por `NodeRouter`, así reconnect puede cambiar la conexión SSH subyacente sin alterar la node address de la UI
- Las cola de transferenciass rastrean dirección, progreso, retry state y speed limits independientemente de los file panes visibles
- Las pestañas IDE mantienen juntos dirty buffers, remote paths, conflict state y restore metadata
- Cuando el backend lo soporta, las escrituras remotas usan staged/atomic behavior para evitar partial writes en el flujo normal de edición

### Plugins, CLI y diagnósticos

La rama native mantiene extensiones y superficies de soporte dentro de límites Rust-native:

- Los plugins corren en wasmtime sandbox con capacidades de host tipadas en vez de globales del navegador
- La CLI enlaza directamente crates de dominio para doctor, settings, connections, reenvíos, bundles portables, backups y reports
- Los diagnósticos priorizan conteos, rutas, marcas de función e indicios redactados antes que cargas crudas con secretos
- Los flujos CLI que mutan estado usan dry-run plans, `--yes` guards y rollback backups cuando aplica

### Port reenvío — Lock-Free I/O

Forwarding mantiene la semántica Tauri en un crate Rust independiente:

- Local `-L`, Remote `-R`, Dynamic SOCKS5 `-D`
- Un único task `ssh_io` posee cada SSH Channel y evita `Arc<Mutex<Channel>>`
- Auto-restore tras reconexión, informe de finalización e tiempo de inactividad

### trzsz — transferencia in-band

trzsz sigue usando el stream del terminal, sin puerto extra ni agent remoto:

- Upload/download por el stream de terminal existente
- Funciona a través de cadenas ProxyJump
- File pickers nativos evitan límites de memoria del navegador
- Transferencia bidireccional, soporte de directorios, límites configurables

### Export `.oxide` cifrado

El formato de bundle cifrado coincide con la línea Tauri:

- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, sube el costo de brute force con GPU
- Cubre connections, reenvíos, settings, quick commands, ajustes de plugin y secretos portables

</details>

---

## Ejecutar desde código fuente

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
| Contraseñas y claves | macOS Keychain / Windows Credential Manager / libsecret |
| Memoria secreta | `zeroize` / `Zeroizing` |
| Diagnóstico & contexto AI | secretos redactados antes de salida o llamadas a proveedores |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Escrituras CLI | dry-run plans, guardas `--yes`, rollback backups |
| Plugins | aislamiento wasmtime y basada en capacidades API de host |

## Estado de release

- [x] reenvío de SSH Agent, reconexión Grace Period, GPUI desktop shell
- [x] Flujo terminal en proceso sin WebSocket
- [x] SFTP, reenvío, IDE, AI, sincronización en la nube, plugins, CLI
- [x] Terminales serie locales
- [x] Escritorio remoto RDP/VNC y terminales Raw TCP/UDP
- [x] Full ProxyCommand
- [ ] Audit logging

## Contribuir

## Neutralidad de proveedors

OxideTerm es BYOK primero y neutral respecto a los proveedors.

Las integraciones de proveedors existen para ayudar a los usuarios a conectar las herramientas en las que ya confían. No son un ranking, un cartel publicitario ni un sistema de recompensa para quien pida atención con más entusiasmo.

La compatibilidad, mantenibilidad, seguridad y valor real para el usuario deciden qué se documenta. La visibilidad sigue a la utilidad, no al entusiasmo.

Si una función ya existe en Tauri, mantén comportamiento, textos, estados de interacción y workflows alineados salvo que haya un reemplazo documentado. Cada crate nuevo debe tener responsabilidad real de dominio.

## Soporte y mantenimiento

Se priorizan los informes de errores y regresiones reproducibles con diagnósticos redactados. Los solicitudes de funciones se evalúan según alcance, seguridad y alineación con la dirección de OxideTerm para el espacio de trabajo de servidores remotos.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Si OxideTerm ayuda a tu workflow, una estrella, reproducción de issue, corrección de traducción, plugin o pull request hacen más fácil mantener el proyecto.

---

## Licencia / Agradecimientos

**GPL-3.0-only**. Los avisos de terceros están en `NOTICE`. Gracias a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` y `tree-sitter`.
