<p align="center">
  <img src="../../src-tauri/icons/icon.ico" alt="OxideTerm" width="128" height="128">
</p>

<h1 align="center">⚡ OxideTerm</h1>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/stargazers">
    <img src="https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social" alt="GitHub stars">
  </a>
  <br>
  <em>¡Si te gusta OxideTerm, por favor dale una estrella en GitHub! ⭐️</em>
</p>


<p align="center">
  <strong>Cero Electron. Cero OpenSSL. Cero telemetría. Cero suscripción. SSH puro en Rust.</strong>
  <br>
  <em>Un solo binario nativo — shells locales, SSH, SFTP, IDE remoto, IA, reenvío de puertos, plugins, 30+ temas, 11 idiomas.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-1.1.0--beta.6-blue" alt="Versión">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue" alt="Plataforma">
  <img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="Licencia">
  <img src="https://img.shields.io/badge/rust-1.85+-orange" alt="Rust">
  <img src="https://img.shields.io/badge/tauri-2.0-purple" alt="Tauri">
</p>

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases/latest">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=Descargar%20última%20versión&style=for-the-badge&color=brightgreen" alt="Descargar última versión">
  </a>
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/releases">
    <img src="https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?include_prereleases&label=Descargar%20última%20beta&style=for-the-badge&color=orange" alt="Descargar última beta">
  </a>
</p>

<p align="center">
  🌐 <strong><a href="https://oxideterm.app">oxideterm.app</a></strong> — Documentation & website
</p>

<p align="center">
  <a href="../../README.md">English</a> | <a href="README.zh-Hans.md">简体中文</a> | <a href="README.zh-Hant.md">繁體中文</a> | <a href="README.ja.md">日本語</a> | <a href="README.ko.md">한국어</a> | <a href="README.fr.md">Français</a> | <a href="README.de.md">Deutsch</a> | <a href="README.es.md">Español</a> | <a href="README.it.md">Italiano</a> | <a href="README.pt-BR.md">Português</a> | <a href="README.vi.md">Tiếng Việt</a>
</p>

> [!NOTE]
> **Cambio de licencia:** A partir de la v1.0.0, OxideTerm ha cambiado su licencia de **PolyForm Noncommercial 1.0.0** a **GPL-3.0 (GNU General Public License v3.0)**. OxideTerm es ahora completamente código abierto — puede usarlo, modificarlo y distribuirlo libremente bajo los términos de la licencia GPL-3.0. Consulte el archivo [LICENSE](../../LICENSE) para más detalles.

---

<div align="center">

https://github.com/user-attachments/assets/4ba033aa-94b5-4ed4-980c-5c3f9f21db7e

*🤖 OxideSens AI — «Abre un terminal local y ejecuta echo hello, world!»*

</div>

---

## ¿Por qué OxideTerm?

| Problema | La respuesta de OxideTerm |
|---|---|
| Los clientes SSH no tienen shells locales | **Motor híbrido**: PTY local (zsh/bash/fish/pwsh/WSL2) + SSH remoto en una sola ventana |
| Reconexión = perder todo | **Reconexión con período de gracia**: sondea la conexión antigua 30 s antes de cortarla — tus vim/htop/yazi sobreviven |
| La edición remota necesita VS Code Remote | **IDE integrado**: CodeMirror 6 sobre SFTP con 30+ lenguajes, agente remoto opcional (~1 MB) en Linux |
| Sin reutilización de conexiones SSH | **Multiplexación**: terminal, SFTP, reenvíos, IDE comparten una sola conexión SSH mediante pool con conteo de referencias |
| Las bibliotecas SSH dependen de OpenSSL | **russh 0.59**: SSH puro en Rust compilado con `ring` — cero dependencias C |
| Apps Electron de 100+ MB | **Tauri 2.0**: backend Rust nativo, binario de 25–40 MB |
| IA limitada a un proveedor | **OxideSens**: 40+ herramientas, protocolo MCP, base de conocimiento RAG — funciona con OpenAI/Ollama/DeepSeek/cualquier API compatible |
| Credenciales en archivos de texto plano | **Solo llavero del sistema**: contraseñas y claves API nunca se escriben en disco; archivos `.oxide` cifrados con ChaCha20-Poly1305 + Argon2id |
| Herramientas dependientes de la nube que requieren cuenta | **Local primero**: sin cuentas, sin telemetría, sin sincronización en la nube — tus datos se quedan en tu máquina. Trae tu propia clave de IA |

---

## Capturas de pantalla

<table>
<tr>
<td align="center"><strong>Terminal SSH + OxideSens AI</strong><br/><br/><img src="../../docs/screenshots/terminal/SSHTERMINAL.png" alt="Terminal SSH con barra lateral OxideSens AI" /></td>
<td align="center"><strong>Gestor de archivos SFTP</strong><br/><br/><img src="../../docs/screenshots/sftp/sftp.png" alt="Gestor de archivos SFTP de doble panel con cola de transferencias" /></td>
</tr>
<tr>
<td align="center"><strong>IDE integrado (CodeMirror 6)</strong><br/><br/><img src="../../docs/screenshots/miniIDE/miniide.png" alt="Modo IDE integrado con editor CodeMirror 6" /></td>
<td align="center"><strong>Reenvío de puertos inteligente</strong><br/><br/><img src="../../docs/screenshots/PORTFORWARD/PORTFORWARD.png" alt="Reenvío de puertos inteligente con detección automática" /></td>
</tr>
</table>

---

## Resumen de funcionalidades

| Categoría | Funcionalidades |
|---|---|
| **Terminal** | PTY local (zsh/bash/fish/pwsh/WSL2), SSH remoto, paneles divididos, difusión de entrada, grabación/reproducción de sesiones (asciicast v2), renderizado WebGL, 30+ temas + editor personalizado, paleta de comandos (`⌘K`), modo zen |
| **SSH y autenticación** | Pool de conexiones y multiplexación, ProxyJump (saltos ilimitados) con grafo topológico, reconexión automática con período de gracia, Reenvío de agente. Auth: contraseña, clave SSH (RSA/Ed25519/ECDSA), SSH Agent, certificados, 2FA interactivo por teclado, Known Hosts TOFU |
| **SFTP** | Navegador de doble panel, arrastrar y soltar, vista previa inteligente (imágenes/vídeo/audio/código/PDF/hex/fuentes), cola de transferencias con progreso y ETA, marcadores, extracción de archivos |
| **Modo IDE** | CodeMirror 6 con 30+ lenguajes, árbol de archivos + estado Git, multi-pestaña, resolución de conflictos, terminal integrado. Agente remoto opcional para Linux (9 arquitecturas adicionales) |
| **Reenvío de puertos** | Local (-L), remoto (-R), SOCKS5 dinámico (-D), I/O por paso de mensajes sin bloqueo, restauración automática en reconexión, informe de fallos, tiempo de inactividad |
| **IA (OxideSens)** | Panel inline (`⌘I`) + chat lateral, captura de buffer del terminal (panel único/todos), contexto multi-fuente (IDE/SFTP/Git), 40+ herramientas autónomas, integración de servidores MCP, base de conocimiento RAG (búsqueda híbrida BM25 + vectores), streaming SSE |
| **Plugins** | Carga ESM en tiempo de ejecución, 18 espacios de nombres API, 24 componentes UI Kit, API congelada + ACL Proxy, disyuntor, desactivación automática en caso de errores |
| **CLI** | Companion `oxt`: JSON-RPC 2.0 vía Unix Socket / Named Pipe, `status`/`list`/`ping`, salida legible + JSON |
| **Seguridad** | Exportación cifrada .oxide (ChaCha20-Poly1305 + Argon2id 256 MB), llavero del SO, Touch ID (macOS), TOFU de clave de host, limpieza de memoria `zeroize` |
| **i18n** | 11 idiomas: EN, 简体中文, 繁體中文, 日本語, 한국어, FR, DE, ES, IT, PT-BR, VI |

---

## Bajo el capó

### Arquitectura — Comunicación de doble plano

OxideTerm separa los datos del terminal de los comandos de control en dos planos independientes:

```
┌─────────────────────────────────────┐
│        Frontend (React 19)          │
│  xterm.js 6 (WebGL) + 19 stores    │
└──────────┬──────────────┬───────────┘
           │ Tauri IPC    │ WebSocket (binario)
           │ (JSON)       │ puerto por sesión
┌──────────▼──────────────▼───────────┐
│         Backend (Rust)              │
│  NodeRouter → SshConnectionRegistry │
│  Wire Protocol v1                   │
│  [Type:1][Length:4][Payload:n]       │
└─────────────────────────────────────┘
```

- **Plano de datos (WebSocket)**: cada sesión SSH obtiene su propio puerto WebSocket. Los bytes del terminal fluyen como tramas binarias con encabezado Type-Length-Payload — sin serialización JSON, sin codificación Base64, cero sobrecarga en la ruta crítica.
- **Plano de control (Tauri IPC)**: gestión de conexiones, operaciones SFTP, reenvíos, configuración — JSON estructurado, pero fuera de la ruta crítica.
- **Direccionamiento por nodo**: el frontend nunca toca `sessionId` ni `connectionId`. Todo se direcciona mediante `nodeId`, resuelto atómicamente en el servidor por el `NodeRouter`. La reconexión SSH cambia el `connectionId` subyacente — pero SFTP, IDE y reenvíos no se ven afectados en absoluto.

### 🔩 SSH puro en Rust — russh 0.59

Toda la pila SSH es **russh 0.59** compilada contra el backend criptográfico **`ring`**:

- **Cero dependencias C/OpenSSL** — toda la pila criptográfica es Rust. No más depuración de «¿qué versión de OpenSSL?».
- Protocolo SSH2 completo: intercambio de claves, canales, subsistema SFTP, reenvío de puertos
- Suites de cifrado ChaCha20-Poly1305 y AES-GCM, claves Ed25519/RSA/ECDSA
- **`AgentSigner`** personalizado: envuelve el SSH Agent del sistema e implementa el trait `Signer` de russh, resolviendo problemas de bounds `Send` RPITIT clonando `&AgentIdentity` a un valor propio antes de cruzar `.await`

```rust
pub struct AgentSigner { /* wraps system SSH Agent */ }
impl Signer for AgentSigner { /* challenge-response via Agent IPC */ }
```

- **Soporte de plataforma**: Unix (`SSH_AUTH_SOCK`), Windows (`\\.\pipe\openssh-ssh-agent`)
- **Cadenas proxy**: cada salto usa autenticación Agent de forma independiente
- **Reconexión**: `AuthMethod::Agent` se reproduce automáticamente

### 🔄 Reconexión inteligente con período de gracia

La mayoría de los clientes SSH destruyen todo al desconectarse y empiezan de nuevo. El orquestador de reconexión de OxideTerm adopta un enfoque fundamentalmente diferente:

1. **Detección** del timeout de heartbeat WebSocket (300 s, calibrado para macOS App Nap y throttling de timers JS)
2. **Snapshot** del estado completo: paneles del terminal, transferencias SFTP en curso, reenvíos de puertos activos, archivos IDE abiertos
3. **Sondeo inteligente**: eventos `visibilitychange` + `online` disparan keepalive SSH proactivo (~2 s de detección frente a 15–30 s de timeout pasivo)
4. **Período de gracia** (30 s): sondea la conexión SSH antigua vía keepalive — si se recupera (ej.: cambio de punto de acceso WiFi), tus aplicaciones TUI (vim, htop, yazi) sobreviven completamente intactas
5. Si la recuperación falla → nueva conexión SSH → restauración automática de reenvíos → reanudación de transferencias SFTP → reapertura de archivos IDE

Pipeline: `queued → snapshot → grace-period → ssh-connect → await-terminal → restore-forwards → resume-transfers → restore-ide → verify → done`

Toda la lógica se ejecuta a través de un `ReconnectOrchestratorStore` dedicado — cero código de reconexión disperso en hooks o componentes.

### 🛡️ Pool de conexiones SSH

`SshConnectionRegistry` con conteo de referencias respaldado por `DashMap` para acceso concurrente sin bloqueo:

- **Una conexión, muchos consumidores**: terminal, SFTP, reenvíos de puertos e IDE comparten una única conexión SSH física — sin handshakes TCP redundantes
- **Máquina de estados por conexión**: `connecting → active → idle → link_down → reconnecting`
- **Gestión del ciclo de vida**: timeout de inactividad configurable (5 min / 15 min / 30 min / 1 h / nunca), intervalo keepalive de 15 s, detección de fallos de heartbeat
- **Heartbeat WsBridge**: intervalo de 30 s, timeout de 5 min — tolera macOS App Nap y throttling JS del navegador
- **Propagación en cascada**: fallo del host de salto → todos los nodos siguientes marcados automáticamente como `link_down` con sincronización de estado
- **Desconexión por inactividad**: emite `connection_status_changed` al frontend (no solo `node:state` interno), previniendo desincronización de la interfaz

### 🤖 OxideSens AI

Asistente de IA centrado en la privacidad con dos modos de interacción:

- **Panel inline** (`⌘I`): comandos rápidos de terminal, salida inyectada vía bracketed paste
- **Chat lateral**: conversaciones persistentes con historial completo
- **Captura de contexto**: el Terminal Registry recopila el buffer del panel activo o de todos los paneles divididos simultáneamente; inyección automática de archivos IDE, rutas SFTP y estado Git
- **40+ herramientas autónomas**: operaciones de archivos, gestión de procesos, diagnósticos de red, interacción con apps TUI, procesamiento de texto — la IA invoca estas herramientas sin activación manual
- **Soporte MCP**: conexión a servidores [Model Context Protocol](https://modelcontextprotocol.io) externos (stdio & SSE) para integración de herramientas de terceros
- **Base de conocimiento RAG** (v0.20): importa documentos Markdown/TXT en colecciones con alcance (global o por conexión). La búsqueda híbrida fusiona índice de palabras clave BM25 + similitud coseno vectorial vía Reciprocal Rank Fusion. Fragmentación compatible con Markdown que preserva la jerarquía de encabezados. Tokenizer de bigramas CJK para chino/japonés/coreano.
- **Proveedores**: OpenAI, Ollama, DeepSeek, OneAPI, o cualquier endpoint `/v1/chat/completions`
- **Seguridad**: claves API almacenadas en el llavero del SO; en macOS, la lectura de claves protegida por **Touch ID** vía `LAContext` — sin entitlements ni firma de código requeridos, en caché tras la primera autenticación por sesión

### 💻 Modo IDE — Edición remota

Editor CodeMirror 6 operando sobre SFTP — sin instalación del lado del servidor requerida por defecto:

- **Árbol de archivos**: carga diferida de directorios con indicadores de estado Git (modificado/sin seguimiento/añadido)
- **24 modos de lenguaje**: 14 nativos de CodeMirror + modos heredados vía `@codemirror/legacy-modes`
- **Resolución de conflictos**: bloqueo optimista por mtime — detecta cambios remotos antes de sobrescribir
- **Git basado en eventos**: actualización automática al guardar, crear, eliminar, renombrar y pulsar Enter en el terminal
- **State Gating**: todas las E/S bloqueadas cuando `readiness !== 'ready'`, Key-Driven Reset fuerza remontaje completo en reconexión
- **Agente remoto** (opcional): binario Rust de ~1 MB, desplegado automáticamente en x86_64/aarch64 Linux. 9 arquitecturas adicionales (ARMv7, RISC-V64, LoongArch64, s390x, Power64LE, i686, ARM, Android aarch64, FreeBSD x86_64) en `agents/extra/` para carga manual. Habilita árbol de archivos mejorado, búsqueda de símbolos y vigilancia de archivos.

### 🔀 Reenvío de puertos — I/O sin bloqueo

Reenvío local (-L), remoto (-R) y dinámico SOCKS5 (-D) completo:

- **Arquitectura por paso de mensajes**: el canal SSH es propiedad de una única tarea `ssh_io` — sin `Arc<Mutex<Channel>>`, eliminando la contención mutex por completo
- **Informe de fallos**: las tareas de reenvío informan activamente la razón de salida (desconexión SSH, cierre de puerto remoto, timeout) para diagnósticos claros
- **Restauración automática**: los reenvíos `Suspended` se reanudan automáticamente en reconexión sin intervención del usuario
- **Timeout de inactividad**: `FORWARD_IDLE_TIMEOUT` (300 s) previene la acumulación de conexiones zombi

### 🔌 Sistema de plugins en tiempo de ejecución

Carga ESM dinámica con una superficie API congelada y reforzada en seguridad:

- **API PluginContext**: 18 espacios de nombres — terminal, ui, commands, settings, lifecycle, events, storage, system
- **24 componentes UI Kit**: componentes React preconstruidos (botones, campos de entrada, diálogos, tablas…) inyectados en sandboxes de plugins vía `window.__OXIDE__`
- **Membrana de seguridad**: `Object.freeze` en todos los objetos de contexto, ACL basada en Proxy, lista blanca IPC, disyuntor con desactivación automática tras errores repetidos
- **Módulos compartidos**: React, ReactDOM, zustand, lucide-react expuestos para uso de plugins sin duplicar bundles

### ⚡ Renderizado adaptativo

Planificador de renderizado de tres niveles que reemplaza el batching fijo de `requestAnimationFrame`:

| Nivel | Disparador | Frecuencia | Beneficio |
|---|---|---|---|
| **Boost** | Datos de trama ≥ 4 KB | 120 Hz+ (ProMotion nativo) | Elimina el lag de desplazamiento en `cat largefile.log` |
| **Normal** | Escritura estándar | 60 Hz (RAF) | Base fluida |
| **Inactivo** | 3 s sin E/S / pestaña oculta | 1–15 Hz (decaimiento exponencial) | Carga GPU casi nula, ahorro de batería |

Las transiciones son completamente automáticas — impulsadas por el volumen de datos, la entrada del usuario y la API Page Visibility. Las pestañas en segundo plano siguen vaciando datos vía timer de inactividad sin despertar RAF.

### 🔐 Exportación cifrada .oxide

Respaldo de conexión portátil e inviolable:

- Cifrado autenticado **ChaCha20-Poly1305 AEAD**
- **KDF Argon2id**: coste de memoria de 256 MB, 4 iteraciones — resistente a fuerza bruta GPU
- Suma de verificación de integridad **SHA-256**
- **Incrustación opcional de claves**: claves privadas codificadas en base64 en la carga útil cifrada
- **Análisis previo**: desglose de tipos de autenticación, detección de claves faltantes antes de la exportación

### 📡 ProxyJump — Multi-salto con consciencia topológica

- Profundidad de cadena ilimitada: `Cliente → Salto A → Salto B → … → Destino`
- Análisis automático de `~/.ssh/config`, construcción del grafo topológico, algoritmo de Dijkstra para la ruta óptima
- Nodos de salto reutilizables como sesiones independientes
- Propagación de fallos en cascada: host de salto caído → todos los nodos siguientes marcados automáticamente como `link_down`

### ⚙️ Terminal local — PTY thread-safe

Shell local multiplataforma vía `portable-pty 0.8`, protegido por feature gate `local-terminal`:

- `MasterPty` envuelto en `std::sync::Mutex` — hilos de E/S dedicados mantienen las lecturas PTY bloqueantes fuera del bucle de eventos de Tokio
- Detección automática de shell: `zsh`, `bash`, `fish`, `pwsh`, Git Bash, WSL2
- `cargo build --no-default-features` elimina PTY para builds móviles/ligeros

### 🪟 Optimización Windows

- **ConPTY nativo**: invoca directamente la API Windows Pseudo Console — soporte completo TrueColor y ANSI, sin WinPTY obsoleto
- **Escáner de shells**: detecta automáticamente PowerShell 7, Git Bash, WSL2, CMD vía Registro y PATH

### Y más

- **Perfilador de recursos**: CPU/memoria/red en vivo vía canal SSH persistente leyendo `/proc/stat`, cálculo basado en deltas, degradación automática a solo RTT en sistemas no Linux
- **Motor de temas personalizado**: 30+ temas integrados, editor visual con vista previa en vivo, 20 campos xterm.js + 24 variables de color UI, derivación automática de colores UI desde la paleta del terminal
- **Grabación de sesiones**: formato asciicast v2, grabación y reproducción completas
- **Difusión de entrada**: escribe una vez, envía a todos los paneles divididos — operaciones de servidor por lotes
- **Galería de fondos**: imágenes de fondo por pestaña, 16 tipos de pestañas, control de opacidad/desenfoque/ajuste
- **Companion CLI** (`oxt`): binario de ~1 MB, JSON-RPC 2.0 vía Unix Socket / Named Pipe, `status`/`list`/`ping` con salida legible o `--json`
- **WSL Graphics** ⚠️ experimental: visor VNC integrado — 9 entornos de escritorio + modo aplicación única, detección WSLg, Xtigervnc + noVNC

<details>
<summary>📸 11 idiomas en acción</summary>
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

## Inicio rápido

### Requisitos previos

- **Rust** 1.85+
- **Node.js** 18+ (pnpm recomendado)
- **Herramientas de plataforma**:
  - macOS: Xcode Command Line Tools
  - Windows: Visual Studio C++ Build Tools
  - Linux: `build-essential`, `libwebkit2gtk-4.1-dev`, `libssl-dev`

### Desarrollo

```bash
git clone https://github.com/AnalyseDeCircuit/oxideterm.git
cd oxideterm && pnpm install

# Compilar el CLI companion (requerido para funciones CLI)
pnpm cli:build

# Aplicación completa (frontend + backend Rust con hot reload)
pnpm run tauri dev

# Solo frontend (Vite en el puerto 1420)
pnpm dev

# Build de producción
pnpm run tauri build
```

---

## Stack tecnológico

| Capa | Tecnología | Detalles |
|---|---|---|
| **Framework** | Tauri 2.0 | Binario nativo, 25–40 MB |
| **Runtime** | Tokio + DashMap 6 | Completamente asíncrono, maps concurrentes sin bloqueo |
| **SSH** | russh 0.59 (`ring`) | Puro Rust, cero dependencias C, SSH Agent |
| **PTY local** | portable-pty 0.8 | Feature-gated, ConPTY en Windows |
| **Frontend** | React 19.1 + TypeScript 5.8 | Vite 7, Tailwind CSS 4 |
| **Estado** | Zustand 5 | 19 stores especializados |
| **Terminal** | xterm.js 6 + WebGL | Acelerado por GPU, 60 fps+ |
| **Editor** | CodeMirror 6 | 30+ modos de lenguaje |
| **Cifrado** | ChaCha20-Poly1305 + Argon2id | AEAD + KDF con alto consumo de memoria (256 MB) |
| **Almacenamiento** | redb 2.1 | Store KV embebido |
| **i18n** | i18next 25 | 11 idiomas × 22 espacios de nombres |
| **Plugins** | ESM Runtime | PluginContext congelado + 24 UI Kit |
| **CLI** | JSON-RPC 2.0 | Unix Socket / Named Pipe |

---

## Seguridad

| Aspecto | Implementación |
|---|---|
| **Contraseñas** | Llavero del SO (macOS Keychain / Windows Credential Manager / libsecret) |
| **Claves API IA** | Llavero del SO + autenticación biométrica Touch ID en macOS |
| **Exportación** | .oxide: ChaCha20-Poly1305 + Argon2id (256 MB de memoria, 4 iteraciones) |
| **Memoria** | Seguridad de memoria de Rust + `zeroize` para limpieza de datos sensibles |
| **Claves de host** | TOFU con `~/.ssh/known_hosts`, rechaza cambios (prevención MITM) |
| **Plugins** | Object.freeze + ACL Proxy, disyuntor, lista blanca IPC |
| **WebSocket** | Tokens de un solo uso con límites de tiempo |

---

## Hoja de ruta

- [x] Reenvío de agente SSH
- [ ] Marketplace de plugins
- [ ] Búsqueda de sesiones y cambio rápido

---

## Licencia

**GPL-3.0** — este software es software libre licenciado bajo la [Licencia Pública General de GNU v3.0](https://www.gnu.org/licenses/gpl-3.0.html).

Puede usar, modificar y distribuir libremente este software bajo los términos de la GPL-3.0. Cualquier trabajo derivado también debe distribuirse bajo la misma licencia.

Texto completo: [Licencia Pública General de GNU v3.0](https://www.gnu.org/licenses/gpl-3.0.html)

---

## Agradecimientos

[russh](https://github.com/warp-tech/russh) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [Tauri](https://tauri.app/) · [xterm.js](https://xtermjs.org/) · [CodeMirror](https://codemirror.net/) · [Radix UI](https://www.radix-ui.com/)

---

<p align="center">
  <sub>236.000+ líneas de Rust y TypeScript — construido con ⚡ y ☕</sub>
</p>

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
