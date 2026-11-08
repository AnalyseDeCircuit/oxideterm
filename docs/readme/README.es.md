<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm: tus servidores, un solo espacio de trabajo" width="920">

# ⚡ OxideTerm

**Un cliente SSH nativo y gratuito para trabajar con servidores remotos.**

SSH · Mosh · Telnet · serie · RDP/VNC · SFTP · reenvío de puertos · editor de código

Sin cuenta. Sin suscripción. Sin telemetría. Sin Electron.

[![Última versión](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Plataformas](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![Licencia](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Estrellas](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Descargar**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Documentación**](https://oxideterm.app) ·
[**Historial de cambios**](../../.github/release-notes/stable-changelog.md) ·
[**Informar de un problema**](https://github.com/AnalyseDeCircuit/oxideterm/issues) ·
[**Grupo de WeChat**](https://github.com/AnalyseDeCircuit/oxideterm/issues/752)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md)

</div>

---

## Qué es OxideTerm

OxideTerm es un cliente SSH para macOS, Windows y Linux. Cada servidor tiene su espacio de trabajo en una misma ventana: terminales, archivos, reenvíos, editor de código, supervisión y un asistente de IA opcional. Es gratuito y de código abierto bajo GPL-3.0.

## Por qué elegir OxideTerm

- **Una conexión para todas las herramientas.** Los terminales, SFTP, reenvíos y editor de un servidor comparten una conexión SSH de forma predeterminada, sin volver a iniciar sesión al abrir otra herramienta.
- **Resiste cortes breves.** Si una conexión deja de responder, OxideTerm la comprueba durante 30 segundos antes de sustituirla, para que vim, htop y otros programas a pantalla completa puedan continuar donde estaban.
- **Poca memoria.** La interfaz se dibuja en la GPU con [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), el framework de Zed. En reposo consume unos 81 MB en macOS y 24 MB en Windows.
- **Tu propia IA.** OxideSens usa el proveedor y la clave API que elijas, sin créditos de OxideTerm que comprar.
- **SSH íntegramente en Rust.** Basado en russh y ring, sin OpenSSL ni libssh2.

<details>
<summary><b>Memoria comparada con OxideTerm 1.x</b></summary>

Memoria en reposo antes y después del cambio de Tauri (1.x) a GPUI nativo (2.0). Los valores de 1.x incluyen sus procesos WebView.

| Plataforma | 1.x (Tauri) | 2.0 (nativo) | Reducción |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | 74% |
| Windows | 182.4 MB | 23.5 MB | 87% |

![Memoria en reposo de OxideTerm 1.x y 2.0 en los monitores de procesos](../../docs/screenshots/oxideterm-memory-comparison.png)

</details>

---

## Capturas de pantalla

### Terminal

Una sesión SSH. La barra lateral muestra las herramientas del servidor: más terminales, SFTP, IDE y reenvío de puertos.

![Terminal SSH con herramientas del servidor en la barra lateral](../../docs/screenshots/terminal/SSHTERMINAL.png)

### Archivos

Carpetas locales y remotas en paralelo, con la cola de transferencias debajo.

![Gestor SFTP de dos paneles con cola de transferencias](../../docs/screenshots/sftp/sftp.png)

### IDE

Abre un proyecto remoto y edita sus archivos en pestañas.

![Proyecto remoto abierto en el IDE integrado](../../docs/screenshots/miniIDE/miniide.png)

### Reenvío de puertos

OxideTerm enumera los puertos en escucha del servidor. Reenvíalos con un clic o usa un reenvío rápido para herramientas como Jupyter.

![Puertos detectados en el servidor en la página de reenvíos](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png)

<details>
<summary><b>Mira cómo OxideSens abre un terminal a partir de una petición en lenguaje natural</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abre un terminal dentro de OxideTerm" width="720">
</a>

</details>

---

## Funcionalidades

Las funciones marcadas como *(plugin)* se instalan desde el mercado de plugins de la aplicación.

### Espacio de trabajo

- Gestor de sesiones: buscar, ordenar, agrupar, importar y exportar conexiones guardadas, con grupos anidados, etiquetas, colores, iconos y notas
- Importación de `~/.ssh/config`, incluidos bloques `Match`, cadenas `ProxyJump` y `RemoteCommand`; los equipos importados se mantienen actualizados
- Importación de SecureCRT, Xshell, Termius, MobaXterm, WindTerm, Electerm y FinalShell, con vista previa antes de guardar
- Filtros de pestañas: todas, páginas globales, terminales locales o un servidor; las sesiones ocultas siguen ejecutándose
- Hasta 16 paneles por pestaña, combinando terminales, SFTP, IDE, reenvíos, notas, ajustes y escritorios remotos
- Ventanas independientes: mover una pestaña a otra ventana y devolverla sin interrumpir sesiones
- Organizar de 2–16 terminales abiertos en una cuadrícula sin reconectar
- Paleta de comandos para acciones y conexiones guardadas, y modo Zen centrado en una página
- Perfiles de terminal local con intérprete, directorio inicial e icono propios
- Restauración del espacio de trabajo, tamaño de ventana, pantalla completa y ubicación en los monitores

### Terminal

- Detección de intérpretes locales: Unix, Símbolo del sistema, Windows PowerShell, PowerShell Core, Git Bash y Nushell
- Paneles con separadores arrastrables; los nuevos paneles locales heredan el directorio de trabajo
- Búsqueda con expresiones regulares, distinción de mayúsculas y consultas multilínea, independiente por panel
- Integración de Bash, Zsh, Fish, Nushell y PowerShell: directorios, códigos de salida y marcas de comandos; instalación remota desde la aplicación
- Plegado de la salida de comandos terminados, conservando su estado de salida
- Barra de comandos con entrada multilínea, historial, sugerencias de rutas, autocompletado y comandos rápidos
- Sugerencias aproximadas del historial y, opcionalmente, historial persistente por conexión
- Modo de escritura libre para editar el comando actual con el ratón
- Edición de pegados multilínea antes de enviarlos, con deshacer y retirada de delimitadores de bloques de código
- Disparadores por texto o expresiones regulares: enviar entrada, ejecutar un comando rápido, iniciar un programa o crear una acción clicable
- Color semántico para salida sin formato: errores, estados Git, permisos, direcciones y fechas; resaltado por palabras y expresiones regulares
- URL y rutas clicables, marcas de tiempo por línea con milisegundos e indicadores de salida sin leer en las pestañas
- Gráficos Sixel y Kitty, incluidas animaciones Kitty
- Control nativo `tmux -CC` con distribución de paneles y separadores arrastrables
- Grupos de entrada sincronizada y envío de texto o hexadecimal con ritmo, intervalos y repeticiones
- Comandos rápidos con grupos, fijación, parámetros de texto, elección o secretos y reglas de visibilidad por equipo
- Registros de sesión con plantillas de nombres, límites de tamaño y retención, y ANSI opcional; grabaciones `.cast` con reproductor
- Copiar, cortar y pegar sobre selecciones de Vim, Neovim o Emacs, localmente y en sesiones SSH compatibles
- Secuencias de Backspace y Delete y codificación de texto por conexión

### Conexiones

- SSH con contraseña, clave privada, certificados OpenSSH, claves gestionadas, agente SSH, autenticación interactiva por teclado o Kerberos
- Agente SSH detectado o endpoint personalizado (`SSH_AUTH_SOCK` en macOS/Linux, `\\.\pipe\openssh-ssh-agent` en Windows), y reenvío del agente
- Claves Ed25519, RSA y ECDSA; ChaCha20-Poly1305 y AES-GCM; orden de algoritmos ajustable y compatibilidad con servidores antiguos
- Generación y relleno de TOTP ante una solicitud coincidente, en el servidor o cualquier equipo de salto
- Claves FIDO (`ed25519-sk`, `ecdsa-sk`), con solicitudes de PIN y contacto *(plugin)*
- Cadenas de salto con autenticación independiente, proxies HTTP CONNECT y SOCKS5 y ProxyCommand
- Reconexión con periodo de gracia: restaura reenvíos, reanuda transferencias y reabre archivos del editor en la nueva conexión
- Una conexión compartida por servidor o autenticación independiente para cada terminal nuevo
- Reenvío X11 de confianza o no confiable
- Conexiones temporales sin guardar; enlaces `ssh://`, `telnet://`, `mosh://`, `rdp://` y `vnc://` que abren la aplicación
- Mosh con predicción adaptativa del eco local *(plugin)*
- Telnet con negociación, rutas por proxy y finales de línea configurables
- Puertos serie: velocidad, bits de datos, paridad, bits de parada, control de flujo, entrada/salida hexadecimal, Break, DTR y RTS
- Terminales Bluetooth LE mediante Nordic UART Service o características GATT personalizadas

### Archivos y transferencias

- Gestor local/remoto de dos paneles con favoritos, autocompletado de rutas y vistas de lista o árbol
- Marcadores emparejados de rutas locales y remotas por conexión
- Cola paralela con límites de velocidad, tiempo restante, pausa, reanudación y reintento; transferencias SFTP interrumpidas recuperadas en pausa tras reiniciar
- Sobrescribir, omitir o renombrar ante conflictos, para un archivo o todos
- SFTP con alternativa automática SCP si falta el subsistema SFTP
- Conexiones SFTP independientes y transferencias entre servidores a través de la aplicación
- FTP y FTPS explícito
- Soltar archivos en un terminal SSH para subirlos a su directorio actual
- `trz`/`tsz` (trzsz) y `rz`/`sz` (ZMODEM) desde el intérprete remoto; XMODEM, YMODEM y ZMODEM por serie
- Vistas previas de texto, código, Markdown, imágenes, audio, vídeo, hexadecimal y fuentes
- Visores PDF, SQLite, certificados y binarios *(plugin)*
- Ir del terminal al mismo directorio SFTP y enviar un directorio SFTP al terminal

### Editor e IDE

- IDE remoto por SFTP, comandos de shell o agente remoto con índice de símbolos, autocompletado y vigilancia de archivos
- Conversión de terminal inactivo a IDE con el mismo usuario, contenedor y directorio
- Pestañas, árbol con selección múltiple, búsqueda de proyecto y sustituciones con expresiones regulares y grupos de captura
- Archivos de hasta 100 MiB, conservando codificación, BOM y finales de línea
- Guardado seguro con conflictos y guardado automático opcional al cambiar de pestaña
- Recuperación de archivos abiertos y cambios sin guardar tras reiniciar
- Vistas Git de ramas, worktrees, área de preparación, historial, diferencias de commits y estados de fusión o rebase
- Detección y ejecución de tareas del proyecto
- Resaltado de 15 lenguajes integrado y más de 20 adicionales mediante plugins *(plugin)*
- Vista previa Markdown con tablas, fórmulas y notas al pie

### Reenvío de puertos y redes

- Reenvíos locales, remotos y dinámicos SOCKS5 con reglas guardadas que pueden iniciarse con la conexión
- Detección de puertos en escucha y reenvío con un clic
- Túneles SSH independientes sin terminal, opcionalmente al iniciar la aplicación
- Restauración de reenvíos tras reconectar; las reglas detenidas manualmente siguen detenidas
- Proxy global y certificados de confianza del sistema, incluidas CA corporativas, para las peticiones de la aplicación
- Topología de conexiones y depuración puntual de sockets

### Escritorio remoto

- RDP y VNC con teclado, ratón, portapapeles, escala y reconexión *(plugin)*
- RDP multimonitor con perfiles de calidad de red *(plugin)*
- VNC mediante VeNCrypt TLS *(plugin)*
- RDP y VNC a través de equipos de salto SSH *(plugin)*
- Aplicaciones WSLg en un visor integrado (Windows)

### Herramientas del equipo

- Para el equipo local y cualquier servidor SSH
- CPU, memoria, swap, discos, red, carga y latencia con gráficas históricas
- Procesos (señales, pausa, continuación, nice), servicios (systemd, launchd, BSD, Windows), tareas y paquetes; actualizaciones y desinstalaciones winget en Windows
- Registros con preajustes y seguimiento, puertos en escucha con avisos de exposición y sistemas de archivos con alertas de espacio
- Contenedores Docker: estado, puertos, inicio, parada, reinicio y registros
- GPU/NPU: NVIDIA, AMD ROCm, Hygon DCU, Huawei Ascend, Intel XPU, Moore Threads MUSA y Cambricon
- Sesiones tmux, Zellij y Herdr: conectar, crear, dividir y renombrar
- Paneles de grupo y matriz de conexiones; latencia SSH en la barra del terminal

### IA OxideSens

- OpenAI (incluida Responses API), Anthropic, Gemini, DeepSeek, xAI Grok, Xiaomi MiMo, Ollama o endpoints compatibles con OpenAI, con ajuste del esfuerzo de razonamiento
- Contexto de los terminales, archivos y ajustes seleccionados; herramientas para ejecutar comandos, manejar programas interactivos, editar y transferir archivos
- Modos de solo lectura, predeterminado o sin aprobación; permisos por categoría, políticas de comandos y revisión adicional de acciones destructivas o con credenciales
- Imágenes adjuntas, ramas de conversación, edición de mensajes y consultas en cola
- Subagentes paralelos, tareas en segundo plano y reanudación tras interrupciones
- Codex y Claude Code mediante ACP; lanzadores OpenCode, Antigravity, Cursor, Grok Build, GitHub Copilot CLI, Qwen Code y Kimi CLI *(plugin)*
- Servidores MCP mediante stdio, Streamable HTTP o SSE heredado
- Control MCP externo para usar conexiones guardadas desde editores y otras herramientas IA, con permisos revocables por grupo
- [Agent Skills](../../docs/agent-skills.md) (`SKILL.md`) con autocompletado `/skill-name`
- Notas Markdown con búsqueda BM25 y vectores HNSW, segmentación CJK y diagramas Mermaid en la vista previa

### Seguridad

- Bloqueo con Touch ID (macOS) o Windows Hello (Windows), sin detener conexiones
- Contraseñas sudo/su guardadas ofrecidas en la solicitud, limitadas al equipo local, una conexión o patrón de equipo; TOTP opcional para solicitudes de dos pasos
- Auditoría con filtros, retención y exportación JSON o CSV
- Grabaciones cifradas con reproducción, búsqueda temporal y velocidad ajustable
- Revisión de permisos antes de activar cada plugin

### Sincronización y copias de seguridad

- WebDAV, almacenamiento compatible S3 (incluido Alibaba Cloud OSS), repositorio Git, GitHub Gist, Dropbox, OneDrive, Google Drive o HTTP JSON
- Selección de datos, fusión de cambios entre dispositivos y resolución individual de conflictos
- Copias de restauración y recuperación de sincronizaciones interrumpidas
- Paquetes `.oxide` cifrados para conexiones, reenvíos, ajustes, comandos rápidos y claves; omitir, renombrar, sustituir o fusionar al importar
- Modo portátil desde una carpeta con almacén de claves cifrado propio
- Actualizaciones integradas en los canales estable o beta

### Personalización

- Apariencia clara, oscura o del sistema, temas separados de aplicación y terminal, y editor de temas
- Fondos con imágenes, animaciones o vídeo; efectos de niebla, partículas, meteoros y cáusticas acuáticas, con paquetes adicionales *(plugin)*
- Opacidad y estilo de esquinas de las ventanas
- Pilas de fuentes con CJK, ligaduras y grosores independientes, y tipografía propia del editor
- Escala de interfaz, estilo de cursor, relleno y animaciones
- Atajos personalizados y ajustes con búsqueda y organización modificable
- 11 idiomas, entrada IME y texto bidireccional

### Plugins

| Tipo | Ejecución | Acceso |
|---|---|---|
| **Manifiesto** | Extensión declarativa | Ninguno; no ejecuta código |
| **WASM** | Wasmtime/WASI o proceso auxiliar | Solo capacidades concedidas por el anfitrión |
| **Proceso** | Programa local normal | Acceso de un programa local, sin aislamiento del sistema operativo |

Instala plugins de proceso solo de fuentes fiables. Los de OxideTerm 1.x pueden seguir en la lista, pero 2.x no los ejecuta.

### Herramienta de línea de comandos

Instala `oxideterm` desde **Ajustes → General → CLI Companion**. Comparte la configuración de la aplicación para comprobar, automatizar y hacer copias sin ventana, o abrir una pestaña desde cualquier intérprete. Los cambios se previsualizan con `--dry-run`, se aplican solo con `--yes` y guardan una copia reversible.

```bash
oxideterm terminal --cwd ~/project
oxideterm doctor --strict
oxideterm connections search prod
oxideterm forwards list --format json
oxideterm oxide export ./profile.oxide --connection prod --password-stdin
```

Autocompletado para Bash, Zsh, Fish, PowerShell y Elvish. La [guía CLI (inglés)](../../docs/user-guide/en/cli.md) enumera todos los comandos.

---

<a id="install"></a>

## Instalación

Descarga OxideTerm desde la [última versión](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest).

| Sistema operativo | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Instalador (`.exe`) | Instalador (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

La página de versiones incluye archivos portátiles, firmas y `sha256sums.txt` para verificar la descarga.

### macOS

Si Gatekeeper bloquea la aplicación, elimina el indicador de cuarentena:

```bash
xattr -cr /Applications/OxideTerm.app
```

### Windows

Si SmartScreen muestra una advertencia, selecciona **Más información → Ejecutar de todas formas**.

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

### Si la ventana no se dibuja

Define `OXIDETERM_RENDER_PROFILE=compatibility` en el entorno y reinicia OxideTerm.

### Primeros pasos

1. Abre el **gestor de sesiones** y añade un servidor, o importa equipos desde `~/.ssh/config`.
2. Conéctate para abrir un terminal.
3. Desde el mismo servidor, abre SFTP, reenvíos o el IDE.
4. Para usar OxideSens, añade un proveedor IA en **Ajustes**.

La [documentación](https://oxideterm.app) cubre el resto. Para compilar, consulta [Compilar desde el código fuente](#build-from-source).

---

## Privacidad y seguridad

- Contraseñas y claves guardadas en el llavero del sistema (macOS Keychain, Windows Credential Manager o libsecret en Linux). Las claves de IA no se incluyen en los registros.
- Los secretos en memoria se borran mediante zeroize al liberarse.
- Las claves del servidor se confían al primer uso y se verifican con `~/.ssh/known_hosts`; se rechazan si cambian.
- El texto enviado a proveedores IA pasa primero por el filtro de credenciales.
- Grabación desactivada por defecto: cifrada en el dispositivo, excluida de la sincronización y sin capturar lo que escribes.
- Notificaciones y auditoría desactivadas por defecto. Al activarlas, los datos quedan en el dispositivo con detalles sensibles cifrados y solo cubren acciones dentro de OxideTerm.
- Paquetes `.oxide` cifrados con ChaCha20-Poly1305 y una clave Argon2id (256 MB de memoria, 4 iteraciones).

---

<a id="build-from-source"></a>

## Compilar desde el código fuente

Necesitas Rust (rustup instala la versión fijada en `rust-toolchain.toml`) y un escritorio que pueda ejecutar GPUI.

```bash
# Run the app
cargo run

# Build the oxideterm CLI
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Con Nix: `nix build .#oxideterm`, `nix run .#oxideterm` o `nix develop`. La CLI se prepara en `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

### Arquitectura

Interfaz, terminales y SSH comparten un proceso Rust. El agente remoto opcional y los auxiliares de plataforma son procesos independientes. La salida SSH o PTY local entra directamente en `TerminalState`, dibujado por GPUI, sin WebView, puente JSON ni xterm.js.

`NodeRouter` distribuye conexiones desde `SshConnectionRegistry`, un grupo basado en DashMap. Terminales, SFTP, reenvíos y editor comparten una conexión por servidor salvo que un terminal use una propia. Los estados son `connecting → active → idle → link_down → reconnecting`. Si falla un salto, sus servidores descendientes pasan a `link_down`. Cada regla de reenvío tiene su escucha. Tras reconectar se vuelven a adquirir sesiones SFTP; una operación de la conexión anterior nunca se traslada silenciosamente.

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

Lecturas (inglés): [desarrollo](../../docs/development/README.md), [mapa de crates](../../docs/development/crate-map.md) y [arquitectura](../../docs/user-guide/en/architecture.md).

---

## Contribuir

Se agradecen código, documentación, traducciones, plugins, pruebas y reproducciones de errores. Para cambios grandes, abre primero una incidencia. [CONTRIBUTING.md](../../docs/CONTRIBUTING.md) explica cómo preparar un PR.

Un diagnóstico con datos sensibles ocultos facilita corregir errores:

```bash
oxideterm report --bundle ./oxideterm-report.zip
```

Desde el código fuente, usa `cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip`.

### Colaboradores

Gracias a todas las personas que ayudan a mejorar OxideTerm.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="Colaboradores de OxideTerm">
  </a>
</p>

---

## Agradecimientos

OxideTerm utiliza estos proyectos de código abierto:

- [russh](https://github.com/warp-tech/russh) y [ring](https://github.com/briansmith/ring): SSH y criptografía
- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui): interfaz acelerada por GPU de Zed
- [alacritty_terminal](https://github.com/alacritty/alacritty): emulación de terminal (VT100–VT500)
- [portable-pty](https://github.com/wez/wezterm/tree/main/pty): intérpretes locales con ConPTY en Windows
- [Tokio](https://tokio.rs): ejecución asíncrona
- [Wasmtime](https://wasmtime.dev/): ejecución de plugins WASM
- [tree-sitter](https://tree-sitter.github.io/): resaltado de sintaxis del editor

---

## Licencia

OxideTerm usa [GPL-3.0-only](../../LICENSE), sin restricciones adicionales. Los avisos de dependencias están en [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) y [NOTICE](../../NOTICE).

Usa OxideTerm solo en sistemas propios o con autorización. [LEGAL.md](../../LEGAL.md) contiene el aviso legal completo.

## Star History

<a href="https://www.star-history.com/?repos=AnalyseDeCircuit%2Foxideterm&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/chart?repos=AnalyseDeCircuit/oxideterm&type=date&legend=top-left" />
 </picture>
</a>
