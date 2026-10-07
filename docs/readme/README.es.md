<div align="center">

<img src="../../docs/media/oxideterm-native-hero.png" alt="OxideTerm: tus servidores, un solo espacio de trabajo" width="920">

# ⚡ OxideTerm

**Un cliente SSH nativo y gratuito, y un espacio de trabajo para operaciones remotas con un asistente de IA que utiliza tu propia clave.**

SSH · Mosh · Telnet · Serie · RDP/VNC · SFTP · reenvío de puertos · editor integrado, todo en una aplicación renderizada por GPU.
Sin cuenta. Sin suscripción. Sin telemetría. Sin Electron.

[![Última versión](https://img.shields.io/github/v/release/AnalyseDeCircuit/oxideterm?label=release)](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)
[![Plataformas](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-blue)](#install)
[![Licencia](https://img.shields.io/badge/license-GPL--3.0-blue)](../../LICENSE)
[![Estrellas](https://img.shields.io/github/stars/AnalyseDeCircuit/oxideterm?style=social)](https://github.com/AnalyseDeCircuit/oxideterm/stargazers)

[**Descargar**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest) ·
[**Documentación**](https://oxideterm.app) ·
[**Historial de cambios**](../../.github/release-notes/stable-changelog.md) ·
[**Informar de un problema**](https://github.com/AnalyseDeCircuit/oxideterm/issues)

[English](../../README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md) | [Italiano](README.it.md) | [Português](README.pt-BR.md) | [Tiếng Việt](README.vi.md) | [Русский](README.ru.md)

</div>

---

## Primeros pasos

1. **Instala OxideTerm.** Descarga un paquete de la [última versión](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest); encontrarás las indicaciones para cada plataforma en [Instalación](#install), más abajo.
2. **Añade un servidor.** Abre el gestor de sesiones y crea una conexión SSH, o importa los equipos de tu archivo `~/.ssh/config`.
3. **Conéctate.** Abre un terminal. Las claves del servidor se comprueban con `~/.ssh/known_hosts`.
4. **Usa el resto del espacio de trabajo.** Abre SFTP, el reenvío de puertos o el editor integrado en el mismo nodo. De forma predeterminada, comparten una conexión SSH.
5. **Opcional: activa la IA.** En Ajustes, añade tu propio servicio de OpenAI, Anthropic, Gemini, Ollama o un endpoint compatible con OpenAI para activar OxideSens.

Consulta la [documentación](https://oxideterm.app) para conocer la aplicación paso a paso.

---

## Qué incluye

| | |
|---|---|
| **Terminales y protocolos** | Intérpretes de comandos locales, SSH, Mosh, Telnet, serie, paneles divididos, rutas con varios saltos, agente SSH y reenvío del agente, autenticación de dos factores y credenciales TOTP, reenvío X11, integración con el intérprete, marcadores de comandos, registros de sesión configurables, grabación, gráficos Sixel y Kitty, transferencias trzsz |
| **tmux y envío simultáneo** | Modo de control nativo `tmux -CC` con distribuciones de paneles y separadores arrastrables, grupos de envío simultáneo con nombre y un emisor avanzado de comandos a varios destinos para entradas programadas y repetibles |
| **Fiabilidad** | La reconexión con periodo de gracia mantiene las aplicaciones TUI activas durante cortes breves de red y después restaura los reenvíos, las transferencias y los archivos abiertos en el editor |
| **Archivos y edición** | Gestor SFTP de dos paneles, colas de transferencia con límites de velocidad y tiempo estimado, marcadores y editor remoto integrado con escrituras seguras, gestión de conflictos y restauración del espacio de trabajo |
| **Redes** | Reenvío local, remoto y dinámico SOCKS5, reglas guardadas, detección de puertos remotos, topología de conexiones y depuración puntual de sockets |
| **Escritorio remoto** | RDP y VNC integrados con compatibilidad para portapapeles y entrada |
| **Operaciones del servidor** | Supervisión de procesos, servicios, registros, puertos, tareas, discos, paquetes, contenedores y tmux |
| **IA y automatización** | OxideSens con tu propia clave, MCP, RAG local, Agent Skills, acciones autorizadas en el espacio de trabajo y una CLI independiente |
| **Revisión y auditoría** | Espacio opcional de notificaciones y auditoría, y grabaciones de sesión cifradas; ambas funciones están desactivadas de forma predeterminada |
| **Sincronización y portabilidad** | Sincronización cifrada en la nube y paquetes portátiles `.oxide` |
| **Personalización** | Temas, imágenes de fondo, atajos configurables, comandos rápidos y 11 idiomas de interfaz |

---

## Por qué elegir OxideTerm

- **Gratuito y centrado en el uso local.** Sin cuenta, suscripción ni telemetría. Tus conexiones y datos operativos permanecen bajo tu control.
- **Un espacio de trabajo por servidor.** Terminal, SFTP, reenvíos, RDP/VNC, editor, supervisión e IA se vinculan al mismo nodo, en lugar de funcionar como herramientas aisladas.
- **Una aplicación nativa, no un navegador disfrazado.** La interfaz se dibuja directamente en la GPU con [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui). No utiliza Electron ni incluye un WebView.
- **IA a tu manera.** OxideSens utiliza tu proveedor y tu clave, y solo realiza las acciones que apruebas.
- **Conexiones resistentes.** La reconexión con periodo de gracia comprueba la conexión anterior durante 30 segundos antes de sustituirla, para que las aplicaciones TUI puedan sobrevivir a cortes breves de red.
- **SSH íntegramente en Rust.** La implementación SSH utiliza `russh` con `ring`, sin OpenSSL ni libssh2.

---

## Uso de memoria

**La reescritura nativa redujo la memoria en reposo a aproximadamente una cuarta parte de la versión anterior en macOS y una octava parte en Windows.** Estas son las observaciones registradas por el mantenedor al pasar de Tauri 1.x a GPUI nativo 2.0:

| Plataforma | Tauri 1.x (en reposo) | Nativa 2.0 (en reposo) | Reducción |
|---|---:|---:|---:|
| macOS | 318.7 MB | 81.3 MB | Aproximadamente 74% |
| Windows | 182.4 MB | 23.5 MB | Aproximadamente 87% |

El total de la versión anterior incluye OxideTerm y sus procesos WebView asociados. La versión nativa ya no necesita esos procesos de navegador.

![Comparación de memoria en reposo con capturas de los procesos del sistema: Tauri 1.x frente a la versión nativa 2.0](../../docs/screenshots/oxideterm-memory-comparison.png)

---

## Capturas de pantalla

| Terminal SSH con OxideSens | Gestor de archivos SFTP |
|---|---|
| ![Terminal SSH con la IA de OxideSens](../../docs/screenshots/terminal/SSHTERMINAL.png) | ![Gestor SFTP de dos paneles con cola de transferencias](../../docs/screenshots/sftp/sftp.png) |

| IDE integrado | Reenvío inteligente de puertos |
|---|---|
| ![Modo IDE integrado](../../docs/screenshots/miniIDE/miniide.png) | ![Reenvío inteligente de puertos con detección automática](../../docs/screenshots/PORTFORWARD/PORTFORWARD.png) |

<details>
<summary><b>Mira cómo OxideSens abre un terminal a partir de una petición en lenguaje natural</b></summary>

<a href="../../docs/media/ai-terminal-demo.mp4">
  <img src="../../docs/media/ai-terminal-demo.gif" alt="OxideSens abre un terminal dentro de OxideTerm" width="720">
</a>

</details>

---

## IA OxideSens

OxideSens es un asistente opcional que puede examinar tus sesiones activas y realizar acciones en el espacio de trabajo **solo después de que las apruebes**.

- **Utiliza tu propia clave.** Compatible con OpenAI, Anthropic (Claude), Google Gemini, Ollama y cualquier endpoint compatible con OpenAI, con controles de razonamiento adaptados al proveedor. No hay créditos de la plataforma.
- **MCP y Agent Skills.** Conecta servidores MCP mediante stdio y SSE, y carga Agent Skills con un alcance delimitado.
- **Base de conocimiento local (RAG).** Búsqueda de texto completo con BM25 y un índice vectorial.
- **Tú controlas el contexto.** Tú decides qué contexto del espacio de trabajo y qué acciones autorizas; se aplican las reglas de la política de comandos.
- **Ocultación de credenciales.** Los mensajes enviados a un proveedor pasan por un filtro que oculta patrones de credenciales.
- **Las claves permanecen en el llavero del sistema operativo** y no aparecen en los registros estructurados.

---

<a id="plugins"></a>

## Complementos

OxideTerm admite tres modalidades de complementos:

| Tipo | Ejecución | Límites |
|---|---|---|
| **Solo manifiesto** | Extensiones declarativas, sin código | Sin código ejecutable |
| **WASM** | Wasmtime/WASI o un proceso auxiliar | Llamadas al anfitrión controladas y limitadas a las capacidades concedidas |
| **Proceso** | Un proceso local convencional | Código local de confianza, **sin** aislamiento del sistema operativo |

Los complementos ESM antiguos de Tauri (1.x) pueden aparecer en la lista, pero la aplicación nativa 2.x no los ejecuta. Instala complementos de proceso únicamente de fuentes de confianza.

---

## Seguridad y privacidad

| Tema | Funcionamiento |
|---|---|
| **Credenciales guardadas** | Llavero del sistema operativo: macOS Keychain, Windows Credential Manager y libsecret |
| **Secretos en memoria** | Los tipos que contienen secretos y los búferes temporales utilizan `zeroize` en los límites de propiedad compatibles |
| **Claves del servidor** | Confianza en el primer uso mediante `~/.ssh/known_hosts`; se rechazan los cambios inesperados |
| **Exportaciones portátiles** | Los paquetes `.oxide` utilizan ChaCha20-Poly1305 con Argon2id: 256 MB de memoria y 4 iteraciones |
| **Contexto de IA** | Ocultación de patrones de credenciales antes de enviar información a un proveedor; tú apruebas el contexto y las acciones |
| **Grabaciones de sesión** | Desactivadas de forma predeterminada; se guardan cifradas en tu dispositivo y se excluyen de la sincronización en la nube; no se captura la entrada del teclado |
| **Auditoría** | Desactivada de forma predeterminada; los datos permanecen en tu dispositivo y los detalles sensibles están cifrados |
| **Cambios mediante CLI** | Planes de simulación, confirmaciones con `--yes` y copias para revertir comandos que modifican el estado |
| **Complementos** | Consulta [Complementos](#plugins) |
| **Telemetría** | Ninguna |

**Uso legal.** OxideTerm se distribuye bajo GPL-3.0-only, sin restricciones adicionales. Accede únicamente a sistemas, redes y dispositivos de tu propiedad o para los que tengas autorización explícita, y cumple la legislación aplicable. No utilices OxideTerm para acceder sin autorización, interrumpir servicios ni eludir controles de acceso.

---

## Limitaciones actuales

Preferimos que las conozcas antes de instalar:

- Solo para equipos de escritorio: macOS, Windows y Linux. No hay aplicación móvil.
- El proyecto avanza rápidamente y publica versiones con frecuencia. Consulta el [historial de cambios](../../.github/release-notes/stable-changelog.md) y los [problemas abiertos](https://github.com/AnalyseDeCircuit/oxideterm/issues).
- La auditoría y la grabación de sesiones requieren activación expresa y solo reflejan lo que OxideTerm puede observar.
- Los complementos de proceso no están aislados por el sistema operativo.
- Si el renderizador falla en tu equipo, prueba el perfil de compatibilidad: `OXIDETERM_RENDER_PROFILE=compatibility`.

---

<a id="for-developers"></a>

## Para desarrolladores

<details>
<summary><b>Ejecutar desde el código fuente</b></summary>

**Requisitos:** herramientas de Rust (edición 2024) y un entorno de escritorio capaz de ejecutar GPUI.

```bash
# Run the app
cargo run

# If the renderer fails on your machine
OXIDETERM_RENDER_PROFILE=compatibility cargo run

# Build the headless CLI companion
./scripts/build/build-cli.sh

# Build the optional Linux remote agent
./scripts/build/build-agent.sh
```

Con Nix: `nix build .#oxideterm`, `nix run .#oxideterm` o `nix develop`.

Los ejecutables de la CLI se generan en `crates/oxideterm-gpui-app/resources/cli-bin/<target-triple>/oxideterm`.

</details>

<details>
<summary><b>Interfaz de línea de comandos</b></summary>

La CLI `oxideterm`, sin interfaz gráfica, funciona sin iniciar la aplicación y resulta útil para automatización, CI y diagnóstico. Incluye ajustes, conexiones, reenvíos, complementos, comandos rápidos, secretos, paquetes portátiles, diagnóstico, informes, planes por lotes, copias de seguridad y sincronización en la nube.

```bash
cargo run -p oxideterm-cli -- doctor --strict
cargo run -p oxideterm-cli -- settings validate --strict --json
cargo run -p oxideterm-cli -- connections search prod
cargo run -p oxideterm-cli -- forwards list --format json
cargo run -p oxideterm-cli -- cloud-sync push --dry-run --json
cargo run -p oxideterm-cli -- oxide export ./profile.oxide --connection prod --password-stdin
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
cargo run -p oxideterm-cli -- completion install zsh --force

# Path and profile isolation for CI or fixtures
cargo run -p oxideterm-cli -- --config-dir ./fixture-config doctor --strict
```

</details>

<details>
<summary><b>Arquitectura</b></summary>

La interfaz y el backend de terminal/SSH comparten un proceso Rust; los agentes remotos opcionales y los procesos auxiliares de cada plataforma se ejecutan fuera de él. Los bytes del terminal modifican `TerminalState` directamente y GPUI renderiza ese estado, sin pasos de análisis de JSON, WebSocket, Base64 o xterm.js.

```
┌─────────────────────────────────────────────────┐
│               GPUI Render Loop                  │
│   WorkspaceApp  ·  Tab surfaces  ·  GPUI views  │
└──────────────────────┬──────────────────────────┘
                       │  in-process Arc<> / async
┌──────────────────────▼──────────────────────────┐
│             Domain Crates (Rust async)          │
│  NodeRouter → SshConnectionRegistry             │
│  TerminalState ← SSH PTY channel (russh)        │
│  SftpSession · ForwardingRuntime · IdeWorkspace │
│  Ai/ACP Entities · CloudSync · Plugin Runtimes  │
└─────────────────────────────────────────────────┘
```

| Aspecto | Enfoque con navegador integrado | OxideTerm |
|---|---|---|
| Renderizado | Motor de navegador y diseño web | GPUI sobre una superficie de GPU |
| Flujo de datos del terminal | WebSocket → bucle de eventos JS → xterm.js | Entrada Rust → `TerminalState` → renderizado GPUI |
| Ciclo de vida de la conexión | Dividido entre frontend y backend | Una sola cadena de conexión y reconexión dentro del proceso |
| Contexto de IA | Copiado a través de un puente de la aplicación | Creado a partir del espacio de trabajo activo, con aprobación del usuario |
| CLI | Requiere la aplicación de escritorio en ejecución | Ejecutable independiente, enlazado directamente con los crates |

**Grupo de conexiones.** `SshConnectionRegistry` utiliza `DashMap` y se accede a él mediante `NodeRouter`. Los paneles de terminal, SFTP, los reenvíos de puertos y el editor pueden compartir una conexión SSH física por nodo; una política de terminal permite elegir una conexión dedicada. Cada conexión sigue `connecting → active → idle → link_down → reconnecting`. El fallo de un servidor de salto marca los nodos posteriores como `link_down`. La IA y los complementos utilizan referencias de capacidades e instantáneas del servidor, en lugar de registrarse como consumidores de conexiones.

**Reconexión con periodo de gracia.**

1. Detectar que se ha agotado el tiempo de espera de keepalive.
2. Guardar una instantánea de los paneles de terminal, las transferencias SFTP, los reenvíos y los archivos del editor.
3. Comprobar la conexión anterior durante 30 s para que las aplicaciones TUI sobrevivan a cortes breves de red.
4. Abrir una nueva conexión, restaurar los reenvíos, reanudar las transferencias y volver a abrir los archivos del editor.

Las sesiones SFTP llevan una generación de conexión: tras una reconexión, se vuelve a obtener una sesión apta, pero una operación de una generación anterior nunca se traslada silenciosamente a la nueva conexión.

**Reenvío de puertos.** Un crate independiente compatible con `-L`, `-R` y `-D` (SOCKS5). Una única tarea `ssh_io` es propietaria de cada canal SSH, por lo que no hay un mutex compartido en la ruta crítica.

**SSH íntegramente en Rust.** `russh` con `ring`: SSH2 completo, ChaCha20-Poly1305 y AES-GCM, claves Ed25519/RSA/ECDSA, agente SSH en Unix (`SSH_AUTH_SOCK`) y Windows (`\\.\pipe\openssh-ssh-agent`), y cadenas de varios saltos con autenticación independiente en cada salto.

**Tecnologías utilizadas**

| Capa | Tecnología |
|---|---|
| Interfaz | GPUI: framework de interfaz de Zed basado en GPU |
| Entorno de ejecución | Tokio, DashMap |
| SSH | `russh` con `ring`, sin OpenSSL ni libssh2 |
| PTY local | `portable-pty`: ConPTY en Windows |
| Emulación de terminal | `alacritty_terminal`: VT100–VT500, Sixel y gráficos Kitty |
| Editor | Resaltado de sintaxis con tree-sitter y búfer propio |
| Cifrado | ChaCha20-Poly1305, Argon2id |
| Complementos | Wasmtime/WASI, WASM en un proceso auxiliar y complementos de proceso |
| Respuestas de IA en streaming | SSE para OpenAI, Anthropic y Gemini, dentro del proceso |
| RAG | BM25 e índice vectorial HNSW con fusión de resultados y tokenización por bigramas CJK |
| Internacionalización | `oxideterm-i18n`: 11 idiomas |

</details>

---

<a id="install"></a>

## Instalación

[**Descargar la última versión**](https://github.com/AnalyseDeCircuit/oxideterm/releases/latest)

| Sistema operativo | x64 | ARM64 |
|---|---|---|
| **macOS** | DMG (Intel) | DMG (Apple Silicon) |
| **Windows** | Instalador (`.exe`) | Instalador (`.exe`) |
| **Linux** | AppImage · `.deb` · `.rpm` | AppImage · `.deb` · `.rpm` |

Comprueba la descarga con el archivo `sha256sums.txt` de la página de la versión. Allí también se publican los archivos portátiles y las firmas.

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

¿Prefieres compilarla? Consulta **Ejecutar desde el código fuente** en [Para desarrolladores](#for-developers).

---

## Contribuir

Las contribuciones son bienvenidas: código Rust, documentación, traducciones, complementos, pruebas y reproducción de problemas. Abre primero un problema para comentar cambios importantes.

Los informes de errores son más útiles si incluyen un paquete de diagnóstico con los datos sensibles ocultos:

```bash
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.zip
```

Se da prioridad a los errores reproducibles y las regresiones. Las propuestas de funciones se evalúan por su alcance, seguridad y encaje con el espacio de trabajo para servidores remotos de OxideTerm. Si OxideTerm te ayuda en tu trabajo, una estrella en GitHub, un informe reproducible, una corrección de traducción o un complemento contribuyen a que el proyecto siga avanzando.

### Colaboradores

Gracias a todas las personas que ayudan a mejorar OxideTerm.

<p align="center">
  <a href="https://github.com/AnalyseDeCircuit/oxideterm/graphs/contributors">
    <img src="https://contrib.rocks/image?repo=AnalyseDeCircuit/oxideterm" alt="Colaboradores de OxideTerm">
  </a>
</p>

---

## Licencia

**GPL-3.0-only.** Las atribuciones de las dependencias están en [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md), con avisos adicionales en [`NOTICE`](../../NOTICE).

**Desarrollado con:** [russh](https://github.com/warp-tech/russh) · [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) · [alacritty_terminal](https://github.com/alacritty/alacritty) · [portable-pty](https://github.com/wez/wezterm/tree/main/pty) · [wasmtime](https://wasmtime.dev/) · [tree-sitter](https://tree-sitter.github.io/)
