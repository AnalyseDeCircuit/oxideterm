<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <strong>Espacio de trabajo operativo nativo con IA para servidores remotos — aplicación nativa en Rust puro</strong>
  <br>
  Terminales SSH, Telnet, serie, RDP/VNC, SFTP, reenvío de puertos y edición ligera en un espacio de trabajo nativo.
  <br>
  Renderizado GPU. Gratis. Sin necesidad de cuenta.
  <br>
  <strong>Sin Electron. Sin WebView integrado. Sin telemetría. Sin suscripción. BYOK primero. SSH puro en Rust sin OpenSSL/libssh2.</strong>
</p>


<p align="center">
  <img src="https://img.shields.io/badge/version-2.0.8-blue" alt="Versión">
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

- Gestionar SSH, Telnet, serie, RDP/VNC, SFTP, reenvío de puertos, shells locales y edición ligera en un solo espacio de trabajo
- Mantener el trabajo remoto durante interrupciones breves de red mediante la reconexión Grace Period
- Pedir a OxideSens que examine sesiones activas y ejecute acciones aprobadas mediante tu propio proveedor de IA

Tus conexiones y datos operativos siguen bajo tu control. OxideSens utiliza tu propio proveedor de IA y no requiere una cuenta.

---

## ¿Por qué OxideTerm?

- SSH, Telnet, serie, RDP/VNC, SFTP, reenvío de puertos y shells locales en una aplicación de escritorio
- Reconexión Grace Period para interrupciones breves de red
- OxideSens con tus propias credenciales de IA y acciones aprobadas
- Interfaz GPUI sin Electron ni runtime de navegador integrado

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

---

## Funciones

| Categoría | Funciones |
|---|---|
| **Terminal y conexiones** | Shells locales, SSH, Telnet, serie, paneles divididos, rutas multi-hop y reconexión estable |
| **Archivos y edición remota** | SFTP, colas de transferencia, marcadores, escritura segura, árboles de proyecto y edición en pestañas |
| **Reenvío y redes** | Reenvío local, remoto y SOCKS5 dinámico, reglas guardadas y depuración de sockets |
| **Operaciones del host y escritorio remoto** | Monitorización, procesos, servicios, logs, puertos, tareas, discos, paquetes, contenedores, tmux, RDP y VNC |
| **OxideSens y automatización** | Proveedores de IA propios, MCP, RAG local, acciones aprobadas, sincronización cifrada y CLI |
| **Extensiones y personalización** | Plugins WASM, pestañas personalizadas, comandos rápidos, temas, fondos, atajos y 11 idiomas |

---

<div align="center">

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abre una terminal dentro de OxideTerm" width="920">
</a>

*OxideSens sigue una petición del usuario y abre una terminal dentro de OxideTerm.*

</div>

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
  SftpSession / ForwardManager / IdeWorkspace
  AiProvider / CloudSyncService / PluginHost
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
4. Si no se recupera, reconectar, restaurar reenvíos, reanudar transferencias y reabrir archivos IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

### Pool de conexiones SSH y ruteo por nodo


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

```sh
cargo run
OXIDETERM_RENDER_PROFILE=compatibility cargo run
./scripts/build/build-cli.sh
./scripts/build/build-agent.sh
```

## CLI

```sh
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

## Tecnologías

| Capa | Tecnología | Notas |
|---|---|---|
| Interfaz | GPUI (Zed) | Modo inmediato acelerado por GPU, íntegramente en Rust |
| Ejecución | Tokio + DashMap | Ejecución asíncrona y mapas concurrentes |
| SSH | russh (`ring`) | Sin OpenSSL/libssh2 en la pila SSH; SSH Agent |
| Terminal | portable-pty + alacritty_terminal | PTY locales, emulación de terminal y gráficos Sixel/Kitty |
| Plugins | wasmtime | Aislamiento WASM con API de host nativa |
| IA y búsqueda | SSE + BM25 + HNSW | Transmisión de proveedores, bigramas CJK y fusión RRF |

## Seguridad

| Tema | Implementación |
|---|---|
| Credenciales almacenadas | macOS Keychain / Windows Credential Manager / libsecret |
| Secretos en memoria | Los tipos con secretos y búferes temporales usan `zeroize` / `Zeroizing` en los límites de propiedad compatibles |
| Diagnósticos | Los informes de soporte priorizan metadatos estructurados e indicios redactados frente a cargas con secretos |
| Contexto de IA | Los mensajes enviados a proveedores pasan por una redacción de patrones de credenciales; el usuario controla el contexto y las acciones del espacio de trabajo |
| `.oxide` | ChaCha20-Poly1305 + Argon2id |
| Escrituras CLI | dry-run plans, guardas `--yes`, rollback backups |
| Plugins | aislamiento wasmtime y basada en capacidades API de host |

## Aviso de uso legal

OxideTerm se distribuye bajo GPL-3.0-only sin restricciones de licencia adicionales. Al usarlo, acceda únicamente a sistemas, redes y dispositivos que sean de su propiedad o para los que tenga autorización explícita, y cumpla la legislación aplicable. No utilice OxideTerm para accesos no autorizados, interrupciones de servicios ni para eludir controles de acceso.

## Contribuir

Se agradecen contribuciones de código, documentación, traducciones, plugins, pruebas e informes de errores. Comenta los cambios grandes en un issue o envía un pull request centrado para una corrección bien delimitada.

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

Si OxideTerm ayuda a tu workflow, una estrella, reproducción de issue, corrección de traducción, plugin o pull request hacen más fácil mantener el proyecto.

---

## Licencia

**GPL-3.0-only**. Los avisos detallados de terceros están en [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), con información adicional en [`NOTICE`](../../NOTICE).

## Agradecimientos

Gracias a `russh`, `GPUI`, `alacritty_terminal`, `portable-pty`, `wasmtime` y `tree-sitter`.
