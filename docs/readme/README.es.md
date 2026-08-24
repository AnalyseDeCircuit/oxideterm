<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>Espacio de trabajo operativo nativo con IA para servidores remotos — aplicación nativa en Rust puro</strong>
  <br>
  Terminales SSH, Mosh, Telnet, serie, RDP/VNC, SFTP, reenvío de puertos y edición ligera en un espacio de trabajo nativo.
  <br>
  Renderizado GPU. Gratis. Sin necesidad de cuenta.
  <br>
  <strong>Sin Electron. Sin WebView integrado. Sin telemetría. Sin suscripción. BYOK primero. SSH puro en Rust sin OpenSSL/libssh2.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.23-blue" alt="Versión">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plataforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licencia">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange" alt="Rust 2024">
  <img src="https://img.shields.io/badge/ui-GPUI-green" alt="GPUI">
</p>

<p align="center">
  <sub>Código abierto, local primero y renderizado por GPU con GPUI.</sub>
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

<p align="center">
  <img src="../../docs/media/oxideterm-native-hero.png" alt="Resumen de funciones de OxideTerm" width="920">
</p>

---

## Qué es OxideTerm

OxideTerm es un espacio de trabajo de código abierto para SSH y operaciones remotas. Terminales, archivos, reenvío de puertos, herramientas del host y escritorios remotos permanecen en un mismo espacio.

**Qué puedes hacer:**

- Gestionar SSH, Mosh, Telnet, serie, RDP/VNC, SFTP, reenvío de puertos, shells locales y edición ligera en un solo espacio de trabajo
- Mantener el trabajo remoto durante interrupciones breves de red mediante la reconexión Grace Period
- Pedir a OxideSens que examine sesiones activas y ejecute acciones aprobadas mediante tu propio proveedor de IA

Tus conexiones y datos operativos siguen bajo tu control. OxideSens utiliza tu propio proveedor de IA y no requiere una cuenta.

---

## ¿Por qué OxideTerm?

| Si te importa… | OxideTerm te ofrece… |
|---|---|
| Un nodo remoto, muchas herramientas | Terminal, SFTP, reenvío de puertos, RDP/VNC, trzsz, IDE nativo, monitorización y OxideSens AI permanecen en el mismo espacio de trabajo |
| Una aplicación de escritorio sin Electron ni WebView incluido | GPUI dibuja la interfaz directamente en una superficie GPU, sin distribuir un runtime de navegador |
| Flujos de operación locales primero | SSH, Telnet, SFTP, reenvío, RDP/VNC, shell local, terminales serie y configuración funcionan sin registro |
| OxideSens AI con tus propias claves en lugar de créditos de plataforma | OxideSens usa tu endpoint de OpenAI, Anthropic, Gemini, Ollama o compatible con OpenAI, con MCP, RAG, controles de razonamiento según el proveedor y acciones del espacio de trabajo aprobadas |
| Estabilidad de reconexión | Grace Period prueba la conexión anterior durante 30 s antes de sustituirla, para que las aplicaciones TUI sobrevivan cortes breves de red |
| SSH en Rust puro y seguridad de credenciales | La pila SSH usa `russh` + `ring` sin OpenSSL/libssh2; las credenciales guardadas usan el llavero del sistema y los paquetes `.oxide` usan ChaCha20-Poly1305 + Argon2id |

---

## Capturas de pantalla

Las capturas muestran flujos de terminal, archivos, edición y reenvío en OxideTerm.

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

## Diseñado para operaciones remotas

OxideTerm mantiene conexiones, archivos, reenvío, herramientas del host, automatización y contexto de IA en un espacio de trabajo Rust. Las herramientas comparten la misma identidad de servidor y el mismo ciclo de sesión.

| Aspecto | Enfoque con navegador integrado | OxideTerm |
|---|---|---|
| **Renderizado** | Motor de navegador y diseño web | GPUI sobre una superficie GPU |
| **Flujo de datos del terminal** | WebSocket → bucle de eventos JS → xterm.js | Entrada Rust → mutación de `TerminalState` → renderizado GPUI |
| **Ciclo de vida de conexión** | Dividido entre frontend y backend | Un único proceso con conexión y reconexión |
| **Contexto de IA** | Copiado mediante un puente de aplicación | Creado desde el espacio de trabajo activo con aprobación del usuario |
| **Runtime de plugins** | Entorno de scripts del navegador | Rutas manifest-only, WASM con capacidades acotadas y procesos que requieren confianza |
| **CLI** | Requiere la aplicación de escritorio en ejecución | Binario independiente con enlace directo a crates |
| **Límite de runtime** | Envoltorio de escritorio más runtime de navegador | Proceso nativo sin runtime de navegador incluido |

---

## Funciones

| Categoría | Funciones |
|---|---|
| **Terminal y conexiones** | Shells locales, SSH, Mosh, Telnet, serie, paneles divididos, modo de entrada libre, registros de sesión configurables, modo de control nativo tmux -CC con distribución de paneles y separadores arrastrables, grupos de difusión con nombre, envío avanzado a varios destinos, rutas multi-hop y reconexión estable |
| **Archivos y edición remota** | SFTP, colas de transferencia, marcadores, escritura segura, árboles de proyecto y edición en pestañas |
| **Reenvío y redes** | Reenvío local, remoto y SOCKS5 dinámico, reglas guardadas y depuración de sockets |
| **Operaciones del host y escritorio remoto** | Monitorización, procesos, servicios, logs, puertos, tareas, discos, paquetes, contenedores, tmux, RDP y VNC |
| **OxideSens y automatización** | Proveedores de IA propios, MCP, RAG local, Agent Skills, acciones aprobadas, sincronización cifrada y CLI |
| **Extensiones y personalización** | Plugins manifest-only, WASM y de proceso, pestañas personalizadas, comandos rápidos, temas, fondos, atajos y 11 idiomas |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abre una terminal dentro de OxideTerm" width="920">
</a>

*OxideSens sigue una petición del usuario y abre una terminal dentro de OxideTerm.*

</div>

---

## Instalación

[Descargar la última versión](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

- macOS: elige el archivo `.dmg` para Apple Silicon o Intel.
- Windows: usa el instalador x64 o ARM64.
- Linux: elige AppImage, `.deb` o `.rpm`.
- Verifica las descargas con el archivo `sha256sums.txt` de la página de publicación.

Para compilar desde el código fuente, consulta la sección « Ejecutar desde el código fuente » más abajo.

---

## Arquitectura

OxideTerm reúne terminal, SSH, Telnet, RDP, VNC, SFTP, reenvío, IDE, IA, plugins y CLI en una arquitectura Rust. Los detalles técnicos aparecen a continuación.

<details>
<summary><strong>Arquitectura, interiores SSH, shell GPUI, reconexión, IA, plugins y más</strong></summary>
<br>

### Arquitectura — núcleo en proceso, sin puente WebView

```text
GPUI Render Loop
  WorkspaceApp / Tab surfaces / GPUI views
        │ en proceso Arc<> / async
Domain Crates
  NodeRouter → SshConnectionRegistry
  TerminalState ← SSH PTY channel
  SftpSession / ForwardingRuntime / IdeWorkspace
  Ai/ACP Entities / CloudSync / Plugin Runtimes
```

No hay frontera de serialización entre la UI y el backend SSH/terminal. Los bytes del terminal modifican `TerminalState` directamente; GPUI lee ese estado y emite draw calls GPU.

### SSH puro en Rust — russh (ring)


- **Sin OpenSSL/libssh2 en la pila SSH** — `ring` proporciona la criptografía SSH
- SSH2 completo: intercambio de claves, canales, subsistema SFTP y reenvío de puertos
- ChaCha20-Poly1305 / AES-GCM, claves Ed25519/RSA/ECDSA
- SSH Agent en Unix (`SSH_AUTH_SOCK`) y Windows (`\\.\pipe\openssh-ssh-agent`)
- ProxyJump multi-hop con autenticación independiente en cada salto

### Reconexión inteligente con Grace Period


1. Detectar timeout de SSH keepalive sin JavaScript timer throttling
2. Tomar instantánea de paneles de terminal, transferencias SFTP, reenvíos y archivos IDE
3. Probar la conexión anterior durante 30 segundos de Grace Period para que las TUI sobrevivan a cambios de red
4. Si no se recupera, reconectar, restaurar reenvíos, reanudar o reintentar transferencias según la estrategia y reabrir archivos IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → retry-or-resume-transfers → restore-ide → verify → done`

### Pool de conexiones SSH y ruteo por nodo


- En el modo predeterminado, una conexión SSH física puede servir terminales, SFTP, reenvíos de puertos e IDE; el terminal puede usar una conexión dedicada cuando la política lo requiere
- Cada conexión pasa por `connecting → active → idle → link_down → reconnecting`
- La UI envía comandos por `nodeId`; `NodeRouter` resuelve atómicamente el `connectionId` activo
- `NodeRuntimeStore` mantiene el estado de ejecución de los nodos y exporta instantáneas de topología; los helpers del workspace escriben esas instantáneas en `session_tree.json` y reconstruyen los handles vivos al iniciar
- La caída de un jump host propaga `link_down` a nodos descendientes

### OxideSens AI

OxideSens sigue siendo BYOK primero, con construcción de contexto dentro del proceso:

- Proveedores: OpenAI, Anthropic, Gemini, Ollama o cualquier punto de acceso OpenAI-compatible
- MCP: transports stdio y SSE, descubrimiento e invocación de herramientas
- RAG: texto completo BM25, índice vectorial HNSW, Reciprocal Rank Fusion, tokenizador CJK bigram
- Los mensajes enviados a proveedores pasan por una redacción de patrones de credenciales; el usuario controla el contexto y las acciones del espacio de trabajo
- Las claves API se guardan en el llavero del sistema y se excluyen expresamente de los registros estructurados y los mensajes del núcleo de escritorio

### Shell desktop GPUI

La UI se dibuja directamente con GPUI, sin pipeline DOM/CSS/JavaScript:

- Tipos de pestaña del espacio de trabajo: terminales locales, SSH, Telnet, serie, RDP, VNC, SFTP, IDE, Forwards, Settings, plugins, Topology y más
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

- Las sesiones SFTP se resuelven por `NodeRouter` y llevan una generación de conexión; reconnect puede adquirir una sesión válida de nuevo, pero una operación de una generación antigua nunca se sustituye silenciosamente por otra conexión
- Las cola de transferenciass rastrean dirección, progreso, retry state y speed limits independientemente de los file panes visibles
- Las pestañas IDE mantienen juntos dirty buffers, remote paths, conflict state y restore metadata
- Cuando el backend lo soporta, las escrituras remotas usan staged/atomic behavior para evitar partial writes en el flujo normal de edición

### Plugins, CLI y diagnósticos

La rama native mantiene extensiones y superficies de soporte dentro de límites Rust-native:

- Los plugins admiten rutas manifest-only, WASM y procesos normales. WASM usa el runtime Wasmtime/WASI integrado; los plugins de proceso son procesos locales sin sandbox del sistema operativo.
- La CLI enlaza directamente crates de dominio para doctor, settings, connections, reenvíos, bundles portables, backups y reports
- Los diagnósticos priorizan conteos, rutas, marcas de función e indicios redactados antes que cargas crudas con secretos
- Los flujos CLI que mutan estado usan dry-run plans, `--yes` guards y rollback backups cuando aplica

### Port reenvío — Lock-Free I/O


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


- **ChaCha20-Poly1305 AEAD** authenticated encryption
- **Argon2id KDF**: 256 MB memory cost, 4 iterations, sube el costo de brute force con GPU
- Cubre connections, reenvíos, settings, quick commands, ajustes de plugin y secretos portables

</details>

---

## Ejecutar desde código fuente

**Requisitos:** toolchain de Rust (edición 2024) y un entorno de escritorio capaz de ejecutar GPUI.

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build/build-cli.sh
./scripts/build/build-agent.sh
```

## CLI

La CLI sin interfaz `oxideterm` funciona sin iniciar la aplicación y resulta útil para automatización, CI y diagnósticos.

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

## Tecnologías

| Capa | Tecnología | Notas |
|---|---|---|
| Interfaz | GPUI (Zed) | Modo inmediato acelerado por GPU, íntegramente en Rust |
| Ejecución | Tokio + DashMap | Ejecución asíncrona y mapas concurrentes |
| SSH | russh (`ring`) | Sin OpenSSL/libssh2 en la pila SSH; SSH Agent |
| Terminal | portable-pty + alacritty_terminal | PTY locales, emulación de terminal y gráficos Sixel/Kitty |
| Plugins | Wasmtime/WASI y procesos | Manifest-only, llamadas de host WASM controladas y procesos locales que requieren confianza explícita |
| IA y búsqueda | SSE + BM25 + HNSW | Transmisión de proveedores, bigramas CJK y fusión RRF |
| Editor | tree-sitter (sintaxis), búfer propio | Multilenguaje, respaldado por SFTP |
| Cifrado | ChaCha20-Poly1305 + Argon2id | AEAD + KDF intensiva en memoria (256 MB) |
| i18n | oxideterm-i18n | Cargador integrado, 11 idiomas distribuidos |

## Seguridad

| Tema | Implementación |
|---|---|
| Credenciales almacenadas | macOS Keychain / Windows Credential Manager / libsecret |
| Secretos en memoria | Los tipos con secretos y búferes temporales usan `zeroize` / `Zeroizing` en los límites de propiedad compatibles |
| Diagnósticos | Los informes de soporte priorizan metadatos estructurados e indicios redactados frente a cargas con secretos |
| Contexto de IA | Los mensajes enviados a proveedores pasan por una redacción de patrones de credenciales; el usuario controla el contexto y las acciones del espacio de trabajo |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Escrituras CLI | dry-run plans, guardas `--yes`, rollback backups |
| Claves de host | TOFU con `~/.ssh/known_hosts`; rechaza cambios inesperados |
| Plugins | manifest-only, API de host WASM controlada y procesos locales que requieren confianza |

## Aviso de uso legal

OxideTerm se distribuye bajo GPL-3.0-only sin restricciones de licencia adicionales. Al usarlo, acceda únicamente a sistemas, redes y dispositivos que sean de su propiedad o para los que tenga autorización explícita, y cumpla la legislación aplicable. No utilice OxideTerm para accesos no autorizados, interrupciones de servicios ni para eludir controles de acceso.

## Contribuir

Se agradecen contribuciones de código, documentación, traducciones, plugins, pruebas e informes de errores. Comenta los cambios grandes o las correcciones bien delimitadas en un issue.

```sh
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

---

## Soporte y mantenimiento

Se priorizan los informes de errores y las regresiones reproducibles con diagnósticos redactados. Las solicitudes de funciones se evalúan según su alcance, seguridad y alineación con la dirección de OxideTerm como espacio de trabajo para servidores remotos.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
</p>

Si OxideTerm ayuda a tu workflow, una estrella, reproducción de issue, corrección de traducción o plugin hacen más fácil mantener el proyecto.

---

## Licencia

**GPL-3.0-only**. Los avisos detallados de terceros están en [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), con información adicional en [`NOTICE`](../../NOTICE).

## Agradecimientos

Gracias a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` y `tree-sitter`.
