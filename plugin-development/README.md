<!-- translated-from: plugin-development/README.zh-CN.md -->
<!-- translated-from-sha256: bf204dbf1339c1827ac161f537a72712f182e39d45d259102a05b91cfa98bb08 -->

> Applies to the current OxideTerm version (Plugin API v3)

## 1. Plugin System Overview

### 1.1 Design Philosophy

The OxideTerm plugin system follows these design principles:

- **Runtime dynamic loading**: Plugins are loaded at runtime as ESM packages through `Blob URL + dynamic import()`, without recompiling the host application
- **Membrane Pattern isolation**: Plugins communicate with the host through a `PluginContext` frozen with `Object.freeze()`, and all API objects are immutable
- **Declarative Manifest**: Plugin capabilities, including tabs, sidebar panels, and terminal hooks, must be declared in advance in `plugin.json` and are enforced at runtime
- **Fail-Open**: Exceptions in Terminal hooks do not block terminal I/O and instead fall back to the original data
- **Automatic cleanup**: Automatic resource cleanup based on the `Disposable` pattern; everything a plugin registers is automatically removed when the plugin unloads

The current PluginContext also includes two officially named namespaces for sync-oriented plugins: `ctx.sync` (encrypted export/import of saved connections plus conflict strategies) and `ctx.secrets` (plugin-scoped secure storage in the OS keychain). This means sync plugins for WebDAV, iCloud, or Syncthing no longer need to abuse `ctx.storage` or directly call host commands that have not been wrapped.

When a plugin needs to read multiple secrets in a single operation, prefer `ctx.secrets.getMany(keys)` over repeatedly calling `ctx.secrets.get()`. The host will try to combine those reads into a single keychain unlock flow, avoiding repeated Touch ID or system authentication prompts on macOS.

`ctx.sync.importOxide()` now supports four strategies: `rename`, `skip`, `replace`, and `merge`. The `merge` strategy is suitable for multi-device sync: the host preserves the existing connection ID and local metadata, updates the main connection fields from the imported side, unions `tags`, and continues reusing locally saved password / key passphrase / certificate passphrase values when they are missing from the import. Port forwarding rules in `.oxide` are also imported and exported as owner-bound saved forwards, but importing them does not directly create active forwards. In addition to the connections themselves, `.oxide` can now carry a snapshot of global OxideTerm settings and a declarative snapshot of plugin settings preferences. During export, plugins can use `ctx.sync.exportOxide({ includeAppSettings: true, selectedAppSettingsSections: ['general', 'appearance'], includePluginSettings: true, includeLocalTerminalEnvVars: false })` to precisely control which host settings sections are packaged and whether local terminal environment variables are included. Correspondingly, `ctx.sync.importOxide()` also supports `selectedAppSettingsSections`, so only part of the settings snapshot can be imported. `ctx.sync.previewImport()` returns `hasAppSettings`, `appSettingsSections`, `pluginSettingsCount`, `pluginSettingsByPlugin`, `forwardDetails`, and record-level `records`, allowing plugins to render "why this will be renamed / skipped / replaced / merged" directly and to warn users in advance which global settings, plugin preferences, and saved forwards the snapshot will restore.

`ctx.sync.getLocalSyncMetadata()` now returns not only the overall `savedConnectionsRevision`, `savedForwardsRevision`, and `settingsRevision`, but also `appSettingsSectionRevisions` and `pluginSettingsRevisions`. Sync plugins can use these revision maps for per-section / per-plugin dirty checks and incremental uploads instead of reading the host's internal stores or `localStorage` directly.

### 1.2 Architecture Model

```
┌──────────────────────────────────────────────────────────────────┐
│                       OxideTerm Host Application                 │
│                                                                  │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────────────┐ │
│  │ Rust Backend │  │  Tauri IPC   │  │     React Frontend      │ │
│  │             │  │  Control      │  │                         │ │
│  │ plugin.rs   │←→│  Plane        │←→│  ┌───────────────────┐  │ │
│  │ - list      │  │              │  │  │   pluginStore      │  │ │
│  │ - read_file │  │              │  │  │   (Zustand)        │  │ │
│  │ - config    │  │              │  │  └───────┬───────────┘  │ │
│  └─────────────┘  └──────────────┘  │          │              │ │
│                                      │  ┌───────▼───────────┐  │ │
│                                      │  │  pluginLoader      │  │ │
│                                      │  │  - discover        │  │ │
│                                      │  │  - validate        │  │ │
│                                      │  │  - load / unload   │  │ │
│                                      │  └───────┬───────────┘  │ │
│                                      │          │              │ │
│                                      │  ┌───────▼───────────┐  │ │
│                                      │  │  Context Factory   │  │ │
│                                      │  │  (buildPluginCtx)  │  │ │
│                                      │  │  → Object.freeze   │  │ │
│                                      │  └───────┬───────────┘  │ │
│                                      │          │              │ │
│                                      └──────────┼──────────────┘ │
│                                                 │                │
│              ┌──────────────────────────────────▼────────────┐   │
│              │                Plugin (ESM)                    │   │
│              │                                                │   │
│              │  activate(ctx) ←── PluginContext (frozen)      │   │
│              │    ctx.connections  ctx.events  ctx.ui         │   │
│              │    ctx.terminal    ctx.settings  ctx.i18n      │   │
│              │    ctx.storage     ctx.api      ctx.assets     │   │
│              │    ctx.sftp  ctx.forward                       │   │
│              │    ctx.sessions  ctx.transfers  ctx.profiler   │   │
│              │    ctx.eventLog  ctx.ide  ctx.ai  ctx.app      │   │
│              │                                                │   │
│              │  window.__OXIDE__                              │   │
│              │    React · ReactDOM · zustand · lucideIcons    │   │
│              └────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

**Key points**:

1. Plugins and the host run in the **same JS context** (not in an iframe or WebWorker)
2. React instances are shared through `window.__OXIDE__` to ensure hook compatibility
3. The Rust backend is responsible for file I/O, with path traversal protection, while the frontend manages lifecycle
4. The Event Bridge forwards connection state changes in `appStore` as plugin events

### 1.3 Security Model

| Layer | Mechanism | Description |
|------|------|------|
| **Membrane isolation** | `Object.freeze()` | All API objects are immutable and non-extensible |
| **Manifest declaration** | Runtime validation | Registering undeclared tabs/panels/hooks/commands throws an exception |
| **Path protection** | Rust `validate_plugin_id()` + `validate_relative_path()` + canonicalize | Prevents path traversal attacks |
| **API whitelist** | `contributes.apiCommands` | Restricts which Tauri commands the plugin can call (**Advisory**) |
| **Circuit breaker** | 10 errors / 60 seconds → auto-disable | Prevents a faulty plugin from dragging down the system |
| **Time budget** | Terminal hooks 5ms budget | Timeouts count toward the circuit breaker |

:::caution[Security Notice]
Plugins currently run in the same JS context and can theoretically bypass the API whitelist by directly `import`ing `@tauri-apps/api/core`. The whitelist is a **defense-in-depth** measure to prevent accidental misuse; real sandbox isolation requires an iframe/WebWorker architecture and is planned for the future. **Only install plugins from trusted sources.**
:::

---

## 2. Quick Start

### 2.1 Development Environment

- Developing OxideTerm plugins does not require additional build tooling
- Plugins are plain ESM JavaScript files that OxideTerm imports dynamically
- If you want to use TypeScript, compile it to ESM yourself; the project provides a standalone type definition file `plugin-api.d.ts` (see [20. Type Reference](#20-typescript-type-reference))
- If you need bundling from multiple files into a single file, you can use esbuild or rollup with `format: 'esm'`

### 2.2 Create Your First Plugin

#### Method 1: Create through Plugin Manager (Recommended)

1. Open **Plugin Manager** in OxideTerm, using the 🧩 icon in the sidebar
2. Click the **New Plugin** button in the upper-right corner, marked with a + icon
3. Enter a plugin ID, using lowercase letters, digits, and hyphens, such as `my-first-plugin`, and a display name
4. Click **Create**
5. OxideTerm will automatically generate a complete plugin scaffold under `~/.oxideterm/plugins/`:
   - `plugin.json` — a prefilled manifest file
   - `main.js` — a Hello World template with `activate()` and `deactivate()`
6. After creation, the plugin is automatically registered in Plugin Manager. Click **Reload** to load it

#### Method 2: Create manually

**Step 1: Create the plugin directory**

```bash
mkdir -p ~/.oxideterm/plugins/my-first-plugin
cd ~/.oxideterm/plugins/my-first-plugin
```

> The plugin directory name does not need to match the `id` in `plugin.json`, but keeping them the same is recommended for easier management.

**Step 2: Write plugin.json**

```json
{
  "id": "my-first-plugin",
  "name": "My First Plugin",
  "version": "0.1.0",
  "description": "A minimal OxideTerm plugin",
  "author": "Your Name",
  "main": "./main.js",
  "engines": {
    "oxideterm": ">=1.6.0"
  },
  "contributes": {
    "tabs": [
      {
        "id": "hello",
        "title": "Hello World",
        "icon": "Smile"
      }
    ]
  }
}
```

**Step 3: Write main.js**

```javascript
// Get React from the host (you must use the host's React instance)
const { React } = window.__OXIDE__;
const { createElement: h, useState } = React;

// Tab component
function HelloTab({ tabId, pluginId }) {
  const [count, setCount] = useState(0);

  return h('div', { className: 'p-6' },
    h('h1', { className: 'text-xl font-bold text-foreground mb-4' },
      'Hello from Plugin! 🧩'
    ),
    h('p', { className: 'text-muted-foreground mb-4' },
      `Plugin: ${pluginId} | Tab: ${tabId}`
    ),
    h('button', {
      onClick: () => setCount(c => c + 1),
      className: 'px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90',
    }, `Clicked ${count} times`),
  );
}

// Activation entry point
export function activate(ctx) {
  console.log(`[MyPlugin] Activating (id: ${ctx.pluginId})`);
  ctx.ui.registerTabView('hello', HelloTab);
  ctx.ui.showToast({ title: 'My Plugin Activated!', variant: 'success' });
}

// Deactivation entry point (optional)
export function deactivate() {
  console.log('[MyPlugin] Deactivating');
}
```

### 2.3 Installation and Debugging

**Method 1: Manual installation (development mode)**

1. Make sure the plugin files are placed under `~/.oxideterm/plugins/my-first-plugin/`
2. Open **Plugin Manager** in OxideTerm, using the 🧩 icon in the sidebar
3. Click **Refresh** to scan for new plugins
4. The plugin will be loaded automatically and appear in the list
5. You can see the plugin's Tab icon in the sidebar; click it to open the Tab

**Uninstall a plugin**

1. Find the plugin you want to uninstall in the **Installed** tab
2. Click the 🗑️ button on the right side of the plugin row
3. The plugin will be deactivated and deleted from disk

Debugging tips:

- Open DevTools (`Cmd+Shift+I` / `Ctrl+Shift+I`) to inspect `console.log` output
- If plugin loading fails, Plugin Manager shows a red error state together with **actionable error messages** such as "activate() must resolve within 5s" or "ensure your main.js exports an activate() function"
- Each plugin in the Plugin Manager list includes a **log viewer** (📜 icon) that shows activation, unload, error, and other lifecycle logs in real time without opening DevTools
- After editing code, click the plugin's **Reload** button in Plugin Manager for hot reload

---

## 3. Plugin Structure

### 3.1 Directory Layout

**v1 single-file bundle (default)**:

```
~/.oxideterm/plugins/
└── your-plugin-id/
    ├── plugin.json          # Required: plugin manifest
    ├── main.js              # Required: ESM entry, specified by manifest.main
    ├── locales/             # Optional: i18n translation files
    │   ├── en.json
    │   ├── zh-CN.json
    │   ├── ja.json
    │   └── ...
    └── assets/              # Optional: other asset files
        └── ...
```

**v2 multi-file package** (`format: "package"`):

```
~/.oxideterm/plugins/
└── your-plugin-id/
    ├── plugin.json          # Required: manifestVersion: 2, format: "package"
    ├── src/
    │   ├── main.js          # ESM entry, supports relative imports between modules
    │   ├── components/
    │   │   ├── Dashboard.js
    │   │   └── Charts.js
    │   └── utils/
    │       └── helpers.js
    ├── styles/
    │   ├── main.css         # Automatically loaded when declared in manifest.styles
    │   └── charts.css
    ├── assets/
    │   ├── logo.png         # Accessed through ctx.assets.getAssetUrl()
    │   └── config.json
    └── locales/
        ├── en.json
        └── zh-CN.json
```

A v2 multi-file package is loaded through the built-in local HTTP file server (`127.0.0.1`, OS-assigned port), which supports standard ES Module `import` syntax between files.

**Path constraints**:

- All file paths are relative to the plugin root
- `..` path traversal is **forbidden**
- Absolute paths are **forbidden**
- Plugin IDs **must not** contain `/`, `\`, `..`, or control characters
- The Rust backend runs `canonicalize()` on resolved paths to ensure they never escape the plugin directory

### 3.2 plugin.json Manifest

This is the core descriptor file of a plugin. OxideTerm discovers plugins by scanning `~/.oxideterm/plugins/*/plugin.json`.

```json
{
  "id": "your-plugin-id",
  "name": "Human Readable Name",
  "version": "1.0.0",
  "description": "What this plugin does",
  "author": "Your Name",
  "main": "./main.js",
  "engines": {
    "oxideterm": ">=1.6.0"
  },
  "locales": "./locales",
  "contributes": {
    "tabs": [...],
    "sidebarPanels": [...],
    "settings": [...],
    "terminalHooks": {...},
    "terminalTransports": ["telnet"],
    "connectionHooks": [...],
    "apiCommands": [...]
  }
}
```

### 3.3 Entry File (ESM)

The entry file must be a valid **ES Module** and `export` the following functions:

```javascript
/**
 * Required. Called when the plugin is activated.
 * @param {PluginContext} ctx - Frozen API context object
 */
export function activate(ctx) {
  // Register UI, hooks, event listeners, and so on
}

/**
 * Optional. Called when the plugin unloads.
 * Used to clean up global state (for example things attached to window).
 * Note: anything registered through Disposable is cleaned up automatically.
 */
export function deactivate() {
  // Clean up global references
}
```

Both functions may return a `Promise` for async activation/deactivation, but there is a **5-second timeout limit**.

**Loading mechanism (dual strategy)**:

**v1 single-file bundle (default / `format: "bundled"`)**:

```
Rust read_plugin_file(id, "main.js")
  → byte array passed to the frontend
    → new Blob([bytes], { type: 'application/javascript' })
      → URL.createObjectURL(blob)
        → import(blobUrl)
          → module.activate(frozenContext)
```

> When loaded through a Blob URL, a plugin **cannot** use relative-path `import` statements internally. Use a bundler such as esbuild or rollup to produce a single-file ESM bundle.

**v2 multi-file package** (`format: "package"`):

```
Frontend calls api.pluginStartServer()
  → Rust starts a local HTTP server (127.0.0.1:0)
    → returns an OS-assigned port

import(`http://127.0.0.1:{port}/plugins/{id}/src/main.js`)
  → browser standard ES Module loading
    → import './components/Dashboard.js' in main.js resolves automatically
      → module.activate(frozenContext)
```

> A v2 package **does** support relative-path `import` statements between files. The browser resolves them automatically through the HTTP server. The server starts on first use and supports graceful shutdown.

**Example v2 multi-file entry**:

```javascript
// src/main.js — import other modules from the same package
import { Dashboard } from './components/Dashboard.js';
import { formatBytes } from './utils/helpers.js';

export async function activate(ctx) {
  // Dynamically load additional CSS
  const cssDisposable = await ctx.assets.loadCSS('./styles/extra.css');

  // Get a blob URL for an asset file (for <img> src, etc.)
  const logoUrl = await ctx.assets.getAssetUrl('./assets/logo.png');

  ctx.ui.registerTabView('dashboard', (props) => {
    const { React } = window.__OXIDE__;
    return React.createElement(Dashboard, { ...props, logoUrl });
  });
}

export function deactivate() {
  // Disposable automatically cleans up CSS and blob URLs
}
```

---

## 4. Manifest Complete Reference

### 4.1 Top-Level Fields

| Field | Type | Required | Description |
|------|------|------|------|
| `id` | `string` | ✅ | Unique plugin identifier. May contain only letters, numbers, hyphens, and dots. `/`, `\`, `..`, and control characters are not allowed. |
| `name` | `string` | ✅ | Human-readable plugin name |
| `version` | `string` | ✅ | Semantic version, for example `"1.0.0"` |
| `description` | `string` | ⬜ | Plugin description |
| `author` | `string` | ⬜ | Author |
| `main` | `string` | ✅ | Relative path to the ESM entry file, for example `"./main.js"` or `"./src/main.js"` |
| `engines` | `object` | ⬜ | Version compatibility requirements |
| `engines.oxideterm` | `string` | ⬜ | Required minimum OxideTerm version, for example `">=1.6.0"`. Currently only `>=x.y.z` and `>x.y.z` are supported; prerelease suffixes are compared by their base version. |
| `contributes` | `object` | ⬜ | Declaration of the capabilities provided by the plugin |
| `locales` | `string` | ⬜ | Relative path to the i18n translation directory, for example `"./locales"` |

**Additional fields for v2 packages**:

| Field | Type | Required | Description |
|------|------|------|------|
| `manifestVersion` | `1 \| 2` | ⬜ | Manifest version, defaults to `1` |
| `format` | `'bundled' \| 'package'` | ⬜ | `bundled` (default) = single-file Blob URL loading; `package` = local HTTP server loading with relative imports |
| `assets` | `string` | ⬜ | Relative asset directory path, for example `"./assets"`, used together with the `ctx.assets` API |
| `styles` | `string[]` | ⬜ | CSS file list such as `["./styles/main.css"]`; automatically injected into `<head>` when loaded |
| `sharedDependencies` | `Record<string, string>` | ⬜ | Declares host-shared dependency versions. Currently supported: `react`, `react-dom`, `zustand`, `lucide-react` |
| `repository` | `string` | ⬜ | Source repository URL |
| `checksum` | `string` | ⬜ | SHA-256 checksum used for integrity verification |

**Example v2 manifest**:

```json
{
  "id": "com.example.multi-file-plugin",
  "name": "Multi-File Plugin",
  "version": "2.0.0",
  "main": "./src/main.js",
  "engines": { "oxideterm": ">=1.6.2" },
  "manifestVersion": 2,
  "format": "package",
  "styles": ["./styles/main.css"],
  "sharedDependencies": {
    "react": "^18.0.0",
    "lucide-react": "^0.300.0"
  },
  "contributes": {
    "tabs": [{ "id": "dashboard", "title": "Dashboard", "icon": "LayoutDashboard" }]
  },
  "locales": "./locales"
}
```

### 4.2 contributes.tabs

Declares the Tab views provided by the plugin.

```json
"tabs": [
  {
    "id": "dashboard",
    "title": "Plugin Dashboard",
    "icon": "LayoutDashboard"
  }
]
```

| Field | Type | Description |
|------|------|------|
| `id` | `string` | Tab identifier, unique within the plugin |
| `title` | `string` | Tab title shown in the tab bar |
| `icon` | `string` | [Lucide React](https://lucide.dev/icons/) icon name |

> After declaring it, you still need to register the component in `activate()` by calling `ctx.ui.registerTabView(id, Component)`.
>
> The `icon` field is used directly for tab bar icon rendering. Use a PascalCase Lucide icon name such as `"LayoutDashboard"`, `"Server"`, or `"Activity"`. If the name is invalid or missing, the system falls back to the `Puzzle` icon.
>
> See the full icon list at: https://lucide.dev/icons/

### 4.3 contributes.sidebarPanels

Declares the sidebar panels provided by the plugin.

```json
"sidebarPanels": [
  {
    "id": "quick-info",
    "title": "Quick Info",
    "icon": "Info",
    "position": "bottom"
  }
]
```

| Field | Type | Description |
|------|------|------|
| `id` | `string` | Panel identifier, unique within the plugin |
| `title` | `string` | Panel title |
| `icon` | `string` | Lucide React icon name |
| `position` | `"top" \| "bottom"` | Position inside the sidebar. Defaults to `"bottom"` |

> The `icon` field is used directly for activity bar icon rendering in the sidebar. Use a PascalCase Lucide icon name such as `"Info"`, `"Database"`, or `"BarChart"`. If the name is invalid or missing, the system falls back to the `Puzzle` icon.
>
> When there are many plugin panels, the middle area of the activity bar becomes scrollable automatically, while the fixed bottom buttons for local terminal, file manager, settings, and plugin manager remain visible.

### 4.4 contributes.settings

Declares configurable plugin settings. Users can inspect and modify them in Plugin Manager.

```json
"settings": [
  {
    "id": "greeting",
    "type": "string",
    "default": "Hello!",
    "title": "Greeting Message",
    "description": "The greeting shown in the dashboard"
  },
  {
    "id": "enableFeature",
    "type": "boolean",
    "default": false,
    "title": "Enable Feature",
    "description": "Toggle this feature on or off"
  },
  {
    "id": "theme",
    "type": "select",
    "default": "dark",
    "title": "Theme",
    "description": "Choose a color theme",
    "options": [
      { "label": "Dark", "value": "dark" },
      { "label": "Light", "value": "light" },
      { "label": "System", "value": "system" }
    ]
  },
  {
    "id": "maxItems",
    "type": "number",
    "default": 50,
    "title": "Max Items",
    "description": "Maximum number of items to display"
  }
]
```

| Field | Type | Description |
|------|------|------|
| `id` | `string` | Setting identifier |
| `type` | `"string" \| "number" \| "boolean" \| "select"` | Value type |
| `default` | `any` | Default value |
| `title` | `string` | Display title |
| `description` | `string?` | Description |
| `options` | `Array<{ label, value }>?` | Used only when `type: "select"` |

### 4.5 contributes.terminalHooks

Declares terminal I/O interception capabilities.

```json
"terminalHooks": {
  "inputInterceptor": true,
  "outputProcessor": true,
  "shortcuts": [
    { "key": "ctrl+shift+d", "command": "openDashboard" },
    { "key": "ctrl+shift+s", "command": "saveBuffer" }
  ]
}
```

| Field | Type | Description |
|------|------|------|
| `inputInterceptor` | `boolean?` | Whether the plugin registers an input interceptor |
| `outputProcessor` | `boolean?` | Whether the plugin registers an output processor |
| `shortcuts` | `Array<{ key, command }>?` | Terminal-local keyboard shortcut declarations |
| `shortcuts[].key` | `string` | Key combination such as `"ctrl+shift+d"` |
| `shortcuts[].command` | `string` | Command name matched by `registerShortcut()` |

**Shortcut format**:

- Modifier keys: `ctrl` (on macOS, Ctrl/Cmd are both treated as ctrl), `shift`, `alt`
- Letter keys: lowercase, such as `d` or `s`
- Combine segments with `+`: `ctrl+shift+d`
- Modifier order is normalized internally

### 4.6 contributes.terminalTransports

Declares extra terminal transports that the plugin needs to open. Currently supported:

```json
"terminalTransports": ["telnet"]
```

| Value | Description |
|------|-------------|
| `"telnet"` | Allows the plugin to call `ctx.terminal.openTelnet()` and open a Telnet terminal tab |

Telnet is a plaintext protocol intended for legacy devices, switches, serial servers, labs, and compatibility scenarios. Plugins should clearly warn users in their own UI that Telnet does not provide SSH-level encryption or host identity verification.

Calling `ctx.terminal.openTelnet()` without declaring `terminalTransports: ["telnet"]` throws.

### 4.7 contributes.connectionHooks

Declares which connection lifecycle events the plugin cares about.

```json
"connectionHooks": ["onConnect", "onDisconnect", "onReconnect", "onLinkDown"]
```

Supported values: `"onConnect"` | `"onDisconnect"` | `"onReconnect"` | `"onLinkDown"`

> Note: this field currently serves only as documentation. Actual event subscription is done through methods such as `ctx.events.onConnect()`.

### 4.8 contributes.aiTools

Declares optional metadata for AI tools that a plugin provides or plans to expose to OxideSens. This field is a **Tool Protocol v2 declaration layer**: legacy plugins can omit it completely; new plugins can use it so the host can display capability, risk, target, approval, and structured-result semantics more clearly.

OxideSens now uses a target-first task orchestrator internally. Built-in chat does not expose every low-level host tool by default, and plugin tools are not automatically shown to the model unless the user explicitly invokes a plugin participant or the host enables the relevant capability path. The manifest API remains compatible; `aiTools` is metadata, not an execution permission.

```json
"aiTools": [
  {
    "name": "router_backup",
    "description": "Back up the running configuration from a network device.",
    "parameters": {
      "type": "object",
      "required": ["targetId"],
      "properties": {
        "targetId": {
          "type": "string",
          "description": "Target terminal session or SSH node ID"
        }
      }
    },
    "capabilities": ["terminal.send", "terminal.observe", "filesystem.write"],
    "risk": "write-file",
    "targetKinds": ["terminal-session", "ssh-node"],
    "resultSchema": {
      "type": "object",
      "properties": {
        "path": { "type": "string" },
        "bytes": { "type": "number" }
      }
    }
  }
]
```

| Field | Type | Description |
|------|------|-------------|
| `name` | `string` | Plugin-local tool name. If exposed to OxideSens later, the host namespaces it to avoid conflicts with built-in tools. |
| `description` | `string` | Short explanation for both the model and the user. |
| `parameters` | `object?` | Function-calling JSON Schema. Omitted means an empty object. |
| `capabilities` | `string[]?` | Semantic capabilities such as `filesystem.read`, `terminal.send`, or `state.list`. |
| `risk` | `string?` | Explicit risk level. If omitted, the host falls back to legacy inference. |
| `targetKinds` | `string[]?` | Target kinds the tool can operate on, such as `ssh-node`, `terminal-session`, or `sftp-session`. |
| `resultSchema` | `object?` | JSON Schema for the envelope `data` field. |

Available `capabilities`:

`command.run`, `terminal.send`, `terminal.observe`, `terminal.wait`, `filesystem.read`, `filesystem.write`, `filesystem.search`, `navigation.open`, `state.list`, `network.forward`, `settings.read`, `settings.write`, `plugin.invoke`, `mcp.invoke`

Available `risk` values:

`read`, `write-file`, `execute-command`, `interactive-input`, `destructive`, `network-expose`, `settings-change`, `credential-sensitive`

Available `targetKinds`:

`local-shell`, `ssh-node`, `terminal-session`, `sftp-session`, `ide-workspace`, `app-tab`, `mcp-server`, `rag-index`

Compatibility rules:

- Plugins that omit `aiTools` continue to work as legacy plugins.
- Declaring `aiTools` does not grant additional permissions and does not bypass user approval.
- `risk` and `capabilities` only affect display, approval hints, and future plugin-tool registration semantics. Real host operations are still limited by `PluginContext` APIs and the `apiCommands` whitelist.
- If a plugin returns a legacy string or object, the host displays it as a legacy result. If it returns a Tool Protocol v2 envelope, the tool UI prefers `summary`, `data`, `warnings`, and `meta.targetId`.

Recommended result shape:

```javascript
return {
  ok: true,
  summary: 'Backed up router configuration to backups/router-1.cfg',
  data: { path: 'backups/router-1.cfg', bytes: 42192 },
  output: 'Saved 42192 bytes',
  warnings: [],
  meta: {
    toolName: 'router_backup',
    capability: 'filesystem.write',
    targetId: 'terminal-session:abc123',
    durationMs: 840
  }
};
```

### 4.9 contributes.apiCommands

Declares the whitelist of backend Tauri commands that the plugin needs to call.

```json
"apiCommands": ["list_sessions", "get_session_info"]
```

Only commands declared in this list can be called through `ctx.api.invoke()`. Calling an undeclared command throws and also logs a warning to the console.

:::tip[Tip]
Most SFTP and port-forwarding operations are already covered by the `ctx.sftp` and `ctx.forward` namespaces and do not need `apiCommands`. Only lower-level commands not covered by those namespaces need to be called through `ctx.api.invoke()`.
:::

#### Available apiCommands

| Category | Command | Description |
|------|------|------|
| **Connections** | `list_connections` | List all active connections |
| | `get_connection_health` | Get connection health metrics |
| | `quick_health_check` | Run a quick connection check |
| **SFTP** | `node_sftp_init` | Initialize an SFTP channel |
| | `node_sftp_list_dir` | List a remote directory |
| | `node_sftp_stat` | Get file or directory metadata |
| | `node_sftp_preview` | Preview file contents |
| | `node_sftp_write` | Write a file |
| | `node_sftp_mkdir` | Create a directory |
| | `node_sftp_delete` | Delete a file |
| | `node_sftp_delete_recursive` | Recursively delete a directory |
| | `node_sftp_rename` | Rename or move a file |
| | `node_sftp_download` | Download a file |
| | `node_sftp_upload` | Upload a file |
| | `node_sftp_download_dir` | Recursively download a directory |
| | `node_sftp_upload_dir` | Recursively upload a directory |
| | `node_sftp_tar_probe` | Probe remote tar support |
| | `node_sftp_tar_upload` | Stream-upload through tar |
| | `node_sftp_tar_download` | Stream-download through tar |
| **Port Forwarding** | `list_port_forwards` | List session port forwards |
| | `create_port_forward` | Create a port forward |
| | `stop_port_forward` | Stop a port forward |
| | `delete_port_forward` | Delete a saved forwarding rule |
| | `restart_port_forward` | Restart a port forward |
| | `update_port_forward` | Update forwarding parameters |
| | `get_port_forward_stats` | Get forwarding traffic statistics |
| | `stop_all_forwards` | Stop all port forwards |
| **Transfer Queue** | `sftp_cancel_transfer` | Cancel a transfer |
| | `sftp_pause_transfer` | Pause a transfer |
| | `sftp_resume_transfer` | Resume a transfer |
| | `sftp_transfer_stats` | Get transfer queue statistics |
| **System** | `get_app_version` | Get the OxideTerm version |
| | `get_system_info` | Get system information |
| **Network** | `plugin_http_request` | Issue binary-safe HTTP requests through the host Rust backend, suitable for WebDAV, object storage, or other sync scenarios affected by CORS |

> Both the request body and response body for `plugin_http_request` are transferred as base64 so the plugin can safely handle non-text payloads. You still need to explicitly declare this command in `contributes.apiCommands` before using it.

### 4.10 locales

Points to the relative path of the i18n translation directory.

```json
"locales": "./locales"
```

See [11. Internationalization (i18n)](#11-internationalization-i18n) for details.

---

## 5. Plugin Lifecycle

### 5.1 Discovery

When OxideTerm starts, or when the user clicks Refresh in Plugin Manager, the Rust backend scans `~/.oxideterm/plugins/`:

```
list_plugins()
  → iterate each child directory under plugins/
    → look for plugin.json
      → serde parse into PluginManifest
        → validate required fields (id, name, main non-empty)
          → return Vec<PluginManifest>
```

Directories without `plugin.json`, or with parse failures, are skipped with a warning in the logs.

### 5.2 Validation

After the frontend receives the manifest in `loadPlugin()`, it performs a second round of validation:

1. **Required field check**: `id`, `name`, `version`, and `main` must all be non-empty strings
2. **Version compatibility check**: if `engines.oxideterm` is declared, it is compared against the current OxideTerm version with a simple semver comparison; currently only `>=` and `>` are supported, and prerelease suffixes are folded down to the base version
3. If validation fails, the system sets `state: 'error'` and records the error information

### 5.3 Loading

```
loadPlugin(manifest)
  1. setPluginState('loading')
  2. api.pluginReadFile(id, mainPath)     // Rust reads file bytes
  3. new Blob([bytes]) → blobUrl          // Create Blob URL
  4. import(blobUrl)                      // Dynamic ESM import
  5. URL.revokeObjectURL(blobUrl)         // Reclaim Blob URL
  6. Validate module.activate is a function
  7. setPluginModule(id, module)
  8. loadPluginLocales(id, ...)           // Load i18n if declared
  9. buildPluginContext(manifest)         // Build frozen context
  10. module.activate(ctx)                // Call activate (5s timeout)
  11. setPluginState('active')
```

**Failure handling**: if any step fails during loading, the system will:
- Call `store.cleanupPlugin(id)` to clean up partial state
- Call `removePluginI18n(id)` to clear i18n resources
- Set `state: 'error'` and record the error message

### 5.4 Activation

`activate(ctx)` is the main entry point of a plugin. All registrations should be completed here:

```javascript
export function activate(ctx) {
  // 1. Register UI components
  ctx.ui.registerTabView('myTab', MyTabComponent);
  ctx.ui.registerSidebarPanel('myPanel', MyPanelComponent);

  // 2. Register terminal hooks
  ctx.terminal.registerInputInterceptor(myInterceptor);
  ctx.terminal.registerOutputProcessor(myProcessor);
  ctx.terminal.registerShortcut('myCommand', myHandler);

  // 3. Subscribe to events
  ctx.events.onConnect(handleConnect);
  ctx.events.onDisconnect(handleDisconnect);

  // 4. Read settings
  const value = ctx.settings.get('myKey');

  // 5. Read storage
  const data = ctx.storage.get('myData');
}
```

**Timeout**: if `activate()` returns a Promise, it must resolve within **5000ms**, otherwise loading is treated as failed.

### 5.5 Runtime

After activation, the plugin enters runtime:

- Registered Tab and Sidebar components are rendered through React
- Terminal hooks are called synchronously on each terminal I/O event
- Event handlers are triggered asynchronously on connection state changes via `queueMicrotask()`
- Settings and storage reads/writes take effect immediately

### 5.6 Deactivation

Triggered when the user disables or reloads the plugin in Plugin Manager:

```javascript
export function deactivate() {
  // Clean up global state
  delete window.__MY_PLUGIN_STATE__;
}
```

**Timeout**: if it returns a Promise, it must resolve within **5000ms**.

**Note**: Anything registered through `Disposable`, including event listeners, UI components, and terminal hooks, does not need to be manually cleaned up in `deactivate()`. The system handles that automatically.

### 5.7 Unloading

```
unloadPlugin(pluginId)
  1. call module.deactivate()         // 5s timeout
  2. cleanupPlugin(pluginId)          // Dispose all Disposables
  3. removePluginI18n(pluginId)       // Clear i18n resources
  4. Close all Tabs owned by the plugin
  5. Clear error trackers
  6. setPluginState('inactive')
```

### 5.8 State Machine

```
                  ┌──────────┐
                  │ inactive │ ←── initial state / after unload
                  └────┬─────┘
                       │ loadPlugin()
                  ┌────▼─────┐
                  │ loading  │
                  └────┬─────┘
               success / │ \ failure
             ┌────▼──┐   ┌──▼───┐
             │ active │   │ error│
             └────┬───┘   └──┬───┘
                  │          │ retryable
         unload / │          ▼
        disable   │    ┌──────────┐
                  │    │ disabled │ ←── disabled manually or by circuit breaker
                  │    └──────────┘
                  ▼
            ┌──────────┐
            │ inactive │
            └──────────┘
```

**PluginState** enum values:

| State | Meaning |
|------|------|
| `'inactive'` | Not loaded / unloaded |
| `'loading'` | Currently loading |
| `'active'` | Activated and running normally |
| `'error'` | An error occurred during load or runtime |
| `'disabled'` | Disabled by the user or by the circuit breaker |

---

## 6. Complete PluginContext API Reference

`PluginContext` is the only argument passed to `activate(ctx)`. It is a deeply frozen object containing 19 namespaces (`pluginId` + 18 child APIs). v3 adds 7 new read-only namespaces.

```typescript
type PluginContext = Readonly<{
  pluginId: string;
  connections: PluginConnectionsAPI;
  events: PluginEventsAPI;
  ui: PluginUIAPI;
  terminal: PluginTerminalAPI;
  settings: PluginSettingsAPI;
  i18n: PluginI18nAPI;
  storage: PluginStorageAPI;
  api: PluginBackendAPI;
  assets: PluginAssetsAPI;
  sftp: PluginSftpAPI;
  forward: PluginForwardAPI;
  // New namespaces added in v3
  sessions: PluginSessionsAPI;   // Session tree (read-only)
  transfers: PluginTransfersAPI; // SFTP transfer monitoring
  profiler: PluginProfilerAPI;   // Resource monitoring
  eventLog: PluginEventLogAPI;   // Event log
  ide: PluginIdeAPI;             // IDE mode (read-only)
  ai: PluginAiAPI;               // AI conversations (read-only)
  app: PluginAppAPI;             // Application information
}>;
```

### 6.1 ctx.pluginId

```typescript
ctx.pluginId: string
```

The unique identifier of the current plugin, matching the `id` field in `plugin.json`.

---

### 6.2 ctx.connections

Read-only connection state query API.

#### `getAll()`

```typescript
connections.getAll(): ReadonlyArray<ConnectionSnapshot>
```

Returns an immutable snapshot array of all SSH connections.

```javascript
const conns = ctx.connections.getAll();
conns.forEach(c => {
  console.log(`${c.username}@${c.host}:${c.port} [${c.state}]`);
});
```

#### `get(connectionId)`

```typescript
connections.get(connectionId: string): ConnectionSnapshot | null
```

Returns a single connection snapshot by connection ID. Returns `null` if it does not exist.

#### `getState(connectionId)`

```typescript
connections.getState(connectionId: string): SshConnectionState | null
```

Quickly returns the current state of a connection. Returns `null` if it does not exist.

#### `getByNode(nodeId)`

```typescript
connections.getByNode(nodeId: string): ConnectionSnapshot | null
```

Resolves a stable `nodeId` back to its current connection snapshot. Returns `null` if the node does not exist or is not currently bound to a connection.

Possible state values: `'idle'` | `'connecting'` | `'active'` | `'disconnecting'` | `'disconnected'` | `'reconnecting'` | `'link_down'` | `{ error: string }`

---

### 6.3 ctx.events

Event subscription and publishing API. All `on*` methods return `Disposable`. Event handlers are invoked asynchronously through `queueMicrotask()` and do not block state updates.

#### `onConnect(handler)`

```typescript
events.onConnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable
```

Triggered when a connection becomes `'active'` (a new connection or recovery from a non-active state).

#### `onDisconnect(handler)`

```typescript
events.onDisconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable
```

Triggered when a connection enters the `'disconnected'` or `'disconnecting'` state, and also when the connection is removed.

#### `onLinkDown(handler)`

```typescript
events.onLinkDown(handler: (snapshot: ConnectionSnapshot) => void): Disposable
```

Triggered when a connection enters the `'reconnecting'`, `'link_down'`, or `error` state.

#### `onReconnect(handler)`

```typescript
events.onReconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable
```

Triggered when a connection recovers from the `'reconnecting'` / `'link_down'` / `error` state back to `'active'`.

#### Current public event surface

`ctx.events` currently exposes only the 4 connection lifecycle events above. Additional node or session creation, close, or idle-state notifications are **not part of the public plugin API**.

If you need to track node or session-tree changes, use `ctx.sessions` instead:

```typescript
ctx.sessions.getActiveNodes(): ReadonlyArray<{ nodeId: string; sessionId: string | null; connectionState: string }>
ctx.sessions.onTreeChange(handler: (tree: ReadonlyArray<SessionTreeNodeSnapshot>) => void): Disposable
ctx.sessions.onNodeStateChange(nodeId: string, handler: (state: string) => void): Disposable
```

Use `onTreeChange()` for node additions, removals, or tree-structure changes. Use `onNodeStateChange()` when you already know a `nodeId` and only need to react to that node's connection-state transitions.

#### `on(name, handler)`

```typescript
events.on(name: string, handler: (data: unknown) => void): Disposable
```

Listens for custom cross-plugin events. The event name is automatically prefixed with the namespace `plugin:{pluginId}:{name}`.

**Note**: You can only listen within your own plugin namespace. For cross-plugin communication, the receiver must use another agreed mechanism.

#### `emit(name, data)`

```typescript
events.emit(name: string, data: unknown): void
```

Emits a custom event. The event name is prefixed with the same namespace automatically.

```javascript
// Emit
ctx.events.emit('data-ready', { rows: 100 });

// Listen inside the same plugin
ctx.events.on('data-ready', (data) => {
  console.log('Received:', data);
});
```

---

### 6.4 ctx.ui

UI registration and interaction API.

#### `registerTabView(tabId, component)`

```typescript
ui.registerTabView(tabId: string, component: React.ComponentType<PluginTabProps>): Disposable
```

Registers a Tab view component. `tabId` must be declared in advance in `contributes.tabs`.

**PluginTabProps**:

```typescript
type PluginTabProps = {
  tabId: string;     // Tab ID
  pluginId: string;  // Plugin ID
};
```

```javascript
function MyTab({ tabId, pluginId }) {
  return h('div', null, `Hello from ${pluginId}!`);
}
ctx.ui.registerTabView('myTab', MyTab);
```

:::caution
Using a `tabId` that is not declared in the manifest throws `Error: Tab "xxx" not declared in plugin manifest contributes.tabs`
:::

#### `registerSidebarPanel(panelId, component)`

```typescript
ui.registerSidebarPanel(panelId: string, component: React.ComponentType): Disposable
```

Registers a sidebar panel component. `panelId` must be declared in advance in `contributes.sidebarPanels`.

Panel components do not receive props, unlike Tabs.

```javascript
function MyPanel() {
  return h('div', { className: 'p-2' }, 'Sidebar content');
}
ctx.ui.registerSidebarPanel('myPanel', MyPanel);
```

#### `ctx.ui.registerCommand(id, opts, handler)`

Registers a command in the global command palette (⌘K / Ctrl+K).

```typescript
const disposable = ctx.ui.registerCommand('my-command', {
  label: 'My Plugin Action',
  icon: 'Zap',
  shortcut: '⌘⇧P',
  section: 'tools',
}, () => {
  console.log('Command executed!');
});

// Unregister when no longer needed
disposable.dispose();
```

Commands are cleaned up automatically when the plugin unloads through the Disposable mechanism.

#### `openTab(tabId)`

```typescript
ui.openTab(tabId: string): void
```

Opens a Tab programmatically. If it is already open, focus switches to that Tab; otherwise a new Tab is created.

```javascript
ctx.ui.openTab('dashboard');
```

#### `showToast(opts)`

```typescript
ui.showToast(opts: {
  title: string;
  description?: string;
  variant?: 'default' | 'success' | 'error' | 'warning';
}): void
```

Shows a toast notification.

```javascript
ctx.ui.showToast({
  title: 'File Saved',
  description: 'config.json has been updated',
  variant: 'success',
});
```

#### `showConfirm(opts)`

```typescript
ui.showConfirm(opts: {
  title: string;
  description: string;
}): Promise<boolean>
```

Shows a confirmation dialog and returns the user's choice. It is implemented with `PluginConfirmDialog` and matches the host application's visual style.

```javascript
const ok = await ctx.ui.showConfirm({
  title: 'Delete Item?',
  description: 'This action cannot be undone.',
});
if (ok) {
  // Perform deletion
}
```

#### `registerContextMenu(target, items)` <small>v3</small>

```typescript
ui.registerContextMenu(target: ContextMenuTarget, items: ContextMenuItem[]): Disposable
```

Registers context menu items for a specific target area. `target` can be `'terminal'`, `'sftp'`, `'tab'`, or `'sidebar'`.

The current host wiring is:
- `terminal`: terminal content area
- `sftp`: SFTP file panel context menu
- `tab`: tab context menu
- `sidebar`: sidebar host area context menu

```javascript
ctx.ui.registerContextMenu('terminal', [
  {
    label: 'Run Analysis',
    icon: 'BarChart',
    handler: () => console.log('Analyzing...'),
  },
  {
    label: 'Copy as Markdown',
    handler: () => { /* ... */ },
    when: () => ctx.terminal.getNodeSelection(currentNodeId) !== null,
  },
]);
```

#### `registerStatusBarItem(options)` <small>v3</small>

```typescript
ui.registerStatusBarItem(options: StatusBarItemOptions): StatusBarHandle
```

Registers a status bar item and returns a handle that can update or dispose it.

```typescript
type StatusBarItemOptions = {
  text: string;
  icon?: string;
  tooltip?: string;
  alignment: 'left' | 'right';
  priority?: number;
  onClick?: () => void;
};

type StatusBarHandle = {
  update(options: Partial<StatusBarItemOptions>): void;
  dispose(): void;
};
```

```javascript
const status = ctx.ui.registerStatusBarItem({
  text: '✔ Connected',
  icon: 'Wifi',
  alignment: 'right',
  priority: 100,
  onClick: () => ctx.ui.openTab('dashboard'),
});

// Update dynamically
status.update({ text: '⚠ Reconnecting...', icon: 'WifiOff' });

// Remove
status.dispose();
```

#### `registerKeybinding(keybinding, handler)` <small>v3</small>

```typescript
ui.registerKeybinding(keybinding: string, handler: () => void): Disposable
```

Registers a global keyboard shortcut. Unlike `registerShortcut` in Terminal Hooks, this does not need to be declared in the manifest.

The host handles these keys in the global shortcut dispatch path. Built-in shortcuts still take priority over plugin keybindings, and plugin keybindings take priority over terminal hook `registerShortcut()` handlers.

```javascript
ctx.ui.registerKeybinding('ctrl+shift+p', () => {
  console.log('Plugin action triggered!');
});
```

#### `showNotification(opts)` <small>v3</small>

```typescript
ui.showNotification(opts: {
  title: string;
  body?: string;
  severity?: 'info' | 'warning' | 'error';
}): void
```

Shows a notification message, internally mapped to the toast system. It is similar to `showToast`, but provides a more semantic `severity` parameter.

```javascript
ctx.ui.showNotification({
  title: 'Transfer Complete',
  body: '5 files uploaded successfully',
  severity: 'info',
});
```

#### `showProgress(title)` <small>v3</small>

```typescript
ui.showProgress(title: string): ProgressReporter
```

Shows a progress indicator and returns an updatable `ProgressReporter`.

The host displays a lightweight progress HUD in the upper-right corner. When `report(value, total)` reaches 100%, the progress item collapses automatically.

> Note: the current `ProgressReporter` does not provide `dispose()` or a manual close API. If an operation fails or ends early, you should still proactively report a completed state once, for example `progress.report(1, 1, 'Failed')`; otherwise the HUD will remain visible.

```typescript
type ProgressReporter = {
  report(value: number, total: number, message?: string): void;
};
```

```javascript
const progress = ctx.ui.showProgress('Deploying...');
progress.report(3, 10, 'Uploading files...');
progress.report(7, 10, 'Running scripts...');
progress.report(10, 10, 'Done!');
```

#### `getLayout()` <small>v3</small>

```typescript
ui.getLayout(): Readonly<{
  sidebarCollapsed: boolean;
  activeTabId: string | null;
  tabCount: number;
}>
```

Returns a read-only snapshot of the current layout state.

#### `onLayoutChange(handler)` <small>v3</small>

```typescript
ui.onLayoutChange(handler: (layout: Readonly<{
  sidebarCollapsed: boolean;
  activeTabId: string | null;
  tabCount: number;
}>) => void): Disposable
```

Subscribes to layout change events.

```javascript
ctx.ui.onLayoutChange((layout) => {
  console.log(`Sidebar: ${layout.sidebarCollapsed ? 'collapsed' : 'expanded'}`);
  console.log(`Active tab: ${layout.activeTabId}`);
});
```

---

### 6.5 ctx.terminal

Terminal hooks and utility API.

#### `registerInputInterceptor(handler)`

```typescript
terminal.registerInputInterceptor(handler: InputInterceptor): Disposable
```

Registers an input interceptor. It must be declared in the manifest as `contributes.terminalHooks.inputInterceptor: true`.

```typescript
type InputInterceptor = (
  data: string,
  context: { sessionId: string },
) => string | null;
```

The interceptor runs **synchronously** on the terminal I/O hot path and has a **5ms time budget**.

```javascript
ctx.terminal.registerInputInterceptor((data, { sessionId }) => {
  return data.toUpperCase();
});
```

```javascript
ctx.terminal.registerInputInterceptor((data, ctx) => {
  if (data.includes('dangerous-command')) {
    return null;
  }
  return data;
});
```

#### `registerOutputProcessor(handler)`

```typescript
terminal.registerOutputProcessor(handler: OutputProcessor): Disposable
```

Registers an output processor. It must be declared in the manifest as `contributes.terminalHooks.outputProcessor: true`.

```typescript
type OutputProcessor = (
  data: Uint8Array,
  context: { sessionId: string },
) => Uint8Array;
```

It also runs synchronously on the hot path and has a 5ms time budget.

```javascript
ctx.terminal.registerOutputProcessor((data, { sessionId }) => {
  totalBytes += data.length;
  return data;
});
```

#### `registerShortcut(command, handler)`

```typescript
terminal.registerShortcut(command: string, handler: () => void): Disposable
```

Registers a terminal shortcut. `command` must have a matching declaration in `contributes.terminalHooks.shortcuts` in the manifest.

```javascript
ctx.terminal.registerShortcut('openDashboard', () => {
  ctx.ui.openTab('dashboard');
});
```

#### `getActiveTarget()` <small>v3</small>

```typescript
terminal.getActiveTarget(): Readonly<{
  sessionId: string;
  terminalType: 'terminal' | 'local_terminal';
  nodeId: string | null;
  connectionId: string | null;
  connectionState: string | null;
  label: string | null;
}> | null
```

Returns the most recently focused terminal target. It covers both SSH terminals and local terminals, so a plugin can send commands back to the terminal the user interacted with most recently even from the plugin's own Tab. `connectionState === 'active'` means the current target is writable. `label` is the host-provided best-effort display name, usually the host for SSH and usually the shell name for local terminals.

```javascript
const target = ctx.terminal.getActiveTarget();
if (target) {
  console.log(target.terminalType, target.label, target.sessionId);
}
```

#### `writeToActive(text)` <small>v3</small>

```typescript
terminal.writeToActive(text: string): boolean
```

Writes text directly to the most recently focused terminal. This is suitable for actions such as “send to current terminal” and supports both SSH and local terminals. Returns `false` if there is no target or if the target is not in the `active` state.

```javascript
const ok = ctx.terminal.writeToActive('ls -la\n');
if (!ok) {
  ctx.ui.showToast({ title: 'No active terminal', variant: 'warning' });
}
```

#### `writeToNode(nodeId, text)`

```typescript
terminal.writeToNode(nodeId: string, text: string): void
```

Writes text to the terminal associated with a specific SSH node. `nodeId` remains stable across reconnects, so it is well suited to plugin logic bound to session tree nodes.

```javascript
ctx.terminal.writeToNode(nodeId, 'journalctl -xe\n');
```

#### `getNodeBuffer(nodeId)`

```typescript
terminal.getNodeBuffer(nodeId: string): string | null
```

Returns the terminal buffer text content for the specified SSH node.

```javascript
const buffer = ctx.terminal.getNodeBuffer(nodeId);
if (buffer) {
  const lastLine = buffer.split('\n').pop();
  console.log('Last line:', lastLine);
}
```

#### `getNodeSelection(nodeId)`

```typescript
terminal.getNodeSelection(nodeId: string): string | null
```

Returns the text currently selected by the user in the terminal for the specified SSH node.

#### `search(nodeId, query, options?)` <small>v3</small>

```typescript
terminal.search(nodeId: string, query: string, options?: {
  caseSensitive?: boolean;
  regex?: boolean;
  wholeWord?: boolean;
}): Promise<Readonly<{ matches: ReadonlyArray<unknown>; total_matches: number }>>
```

Searches text in the terminal buffer. It is executed through a backend Rust command and supports regex and case-sensitive options.

```javascript
const result = await ctx.terminal.search(nodeId, 'error', {
  caseSensitive: false,
  regex: false,
});
console.log(`Found ${result.total_matches} matches`);
```

#### `getScrollBuffer(nodeId, startLine, count)` <small>v3</small>

```typescript
terminal.getScrollBuffer(nodeId: string, startLine: number, count: number):
  Promise<ReadonlyArray<Readonly<{ text: string; lineNumber: number }>>>
```

Returns content from the scrollback buffer for the specified range of lines.

```javascript
const lines = await ctx.terminal.getScrollBuffer(nodeId, 0, 100);
lines.forEach(l => console.log(`[${l.lineNumber}] ${l.text}`));
```

#### `getBufferSize(nodeId)` <small>v3</small>

```typescript
terminal.getBufferSize(nodeId: string):
  Promise<Readonly<{ currentLines: number; totalLines: number; maxLines: number }>>
```

Returns terminal buffer size information.

```javascript
const stats = await ctx.terminal.getBufferSize(nodeId);
console.log(`Buffer: ${stats.currentLines}/${stats.maxLines} lines`);
```

#### `clearBuffer(nodeId)` <small>v3</small>

```typescript
terminal.clearBuffer(nodeId: string): Promise<void>
```

Clears the terminal buffer for the specified node.

```javascript
await ctx.terminal.clearBuffer(nodeId);
```

#### `openTelnet(options)` <small>v3</small>

```typescript
terminal.openTelnet(options: {
  host: string;
  port?: number;
  cols?: number;
  rows?: number;
}): Promise<{
  sessionId: string;
  info: LocalTerminalInfo;
}>
```

Opens a Telnet terminal tab. The plugin must first declare `contributes.terminalTransports: ["telnet"]` in its manifest; otherwise the call throws.

```json
{
  "contributes": {
    "terminalTransports": ["telnet"]
  }
}
```

```javascript
await ctx.terminal.openTelnet({
  host: '192.168.1.1',
  port: 23,
});
```

The Telnet transport is handled by the Rust core, including TCP connection setup, Telnet IAC negotiation, NAWS resize, and terminal event forwarding. The plugin is responsible only for the entry point and UI. Telnet is plaintext, so plugins should warn users before connecting that it does not provide SSH encryption or host identity verification.

---

### 6.6 ctx.settings

Plugin-scoped settings API, persisted to `localStorage`.

#### `get<T>(key)`

```typescript
settings.get<T>(key: string): T
```

Returns a setting value. If the user has not configured a value, the `default` declared in the manifest is returned.

```javascript
const greeting = ctx.settings.get('greeting');
const max = ctx.settings.get('maxItems');
```

#### `set<T>(key, value)`

```typescript
settings.set<T>(key: string, value: T): void
```

Sets a value. This triggers listeners registered through `onChange()`.

#### `onChange(key, handler)`

```typescript
settings.onChange(key: string, handler: (newValue: unknown) => void): Disposable
```

Subscribes to setting changes.

```javascript
ctx.settings.onChange('greeting', (newVal) => {
  console.log('Greeting changed to:', newVal);
});
```

#### `exportSyncableSettings()`

```typescript
settings.exportSyncableSettings(): Promise<Readonly<{
  revision: string;
  exportedAt: string;
  payload: SyncableSettingsPayload;
  warnings: ReadonlyArray<SyncableSettingsWarning>;
}>>
```

Exports the host-whitelisted subset of this plugin's settings so a sync plugin can package or upload them separately.

#### `applySyncableSettings(payload)`

```typescript
settings.applySyncableSettings(payload: SyncableSettingsPayload): Promise<Readonly<{
  revision: string;
  appliedPayload: SyncableSettingsPayload;
  warnings: ReadonlyArray<SyncableSettingsWarning>;
}>>
```

Applies a syncable settings payload with host-side validation and normalization.

**Storage key format**: `oxide-plugin-{pluginId}-setting-{settingId}`

---

### 6.7 ctx.i18n

Plugin-scoped internationalization API.

#### `t(key, params?)`

```typescript
i18n.t(key: string, params?: Record<string, string | number>): string
```

Translates the specified key. The key is automatically prefixed with `plugin.{pluginId}.`.

```javascript
const msg = ctx.i18n.t('greeting');
const hello = ctx.i18n.t('hello_user', { name: 'Alice' });
```

Corresponding translation file `locales/en.json`:

```json
{
  "greeting": "Welcome!",
  "hello_user": "Hello, {{name}}!"
}
```

#### `getLanguage()`

```typescript
i18n.getLanguage(): string
```

Returns the current language code, such as `"en"` or `"zh-CN"`.

#### `onLanguageChange(handler)`

```typescript
i18n.onLanguageChange(handler: (lang: string) => void): Disposable
```

Subscribes to language changes.

---

### 6.8 ctx.storage

Plugin-scoped persistent KV storage based on `localStorage`.

#### `get<T>(key)`

```typescript
storage.get<T>(key: string): T | null
```

Returns a value. Returns `null` if it does not exist or if parsing fails. Values are automatically JSON-deserialized.

#### `set<T>(key, value)`

```typescript
storage.set<T>(key: string, value: T): void
```

Stores a value. Values are automatically JSON-serialized.

#### `remove(key)`

```typescript
storage.remove(key: string): void
```

Removes the specified key.

```javascript
const count = (ctx.storage.get('launchCount') || 0) + 1;
ctx.storage.set('launchCount', count);
```

**Storage key format**: `oxide-plugin-{pluginId}-{key}`

---

### 6.9 ctx.api

Restricted Tauri backend command invocation API.

#### `invoke<T>(command, args?)`

```typescript
api.invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>
```

Invokes a Tauri backend command. The command must be declared in advance in `contributes.apiCommands`.

```javascript
const sessions = await ctx.api.invoke('list_sessions');
```

**Undeclared commands**:
- emit a warning in the console
- throw `Error: Command "xxx" not whitelisted in manifest contributes.apiCommands`

---

### 6.10 ctx.assets

Plugin asset file access API. Used to load CSS and obtain URLs for images, fonts, and data files.

#### `loadCSS(relativePath)`

```typescript
assets.loadCSS(relativePath: string): Promise<Disposable>
```

Reads a CSS file in the plugin directory and injects a `<style data-plugin="{pluginId}">` tag into `<head>`. Calling `dispose()` on the returned `Disposable` removes that `<style>` tag.

```javascript
const cssDisposable = await ctx.assets.loadCSS('./styles/extra.css');
cssDisposable.dispose();
```

> Note: CSS files declared in `manifest.styles` are **automatically injected** when the plugin loads, so you do not need to call `loadCSS()` manually. `loadCSS()` is intended for additional styles that are loaded on demand.

#### `getAssetUrl(relativePath)`

```typescript
assets.getAssetUrl(relativePath: string): Promise<string>
```

Reads any file in the plugin directory and returns a blob URL, which can be used in `<img src>`, `new Image()`, and similar APIs.

```javascript
const logoUrl = await ctx.assets.getAssetUrl('./assets/logo.png');
return h('img', { src: logoUrl, alt: 'Logo' });
```

**Automatic MIME type detection**:

| Extension | MIME |
|--------|------|
| `png` | `image/png` |
| `jpg`/`jpeg` | `image/jpeg` |
| `gif` | `image/gif` |
| `svg` | `image/svg+xml` |
| `webp` | `image/webp` |
| `woff`/`woff2` | `font/woff` / `font/woff2` |
| `ttf`/`otf` | `font/ttf` / `font/otf` |
| `json` | `application/json` |
| `css` | `text/css` |
| `js` | `application/javascript` |
| Other | `application/octet-stream` |

#### `revokeAssetUrl(url)`

```typescript
assets.revokeAssetUrl(url: string): void
```

Manually releases a blob URL created through `getAssetUrl()` to free memory.

```javascript
const url = await ctx.assets.getAssetUrl('./assets/large-image.png');
ctx.assets.revokeAssetUrl(url);
```

> When the plugin unloads, all blob URLs that were not manually released and all injected `<style>` tags are **cleaned up automatically**.

---

### 6.11 ctx.sftp

Remote filesystem operation API. Operates on remote files through the SFTP protocol and does not need to be declared in `contributes.apiCommands`.

All methods use `nodeId`, a stable identifier that remains valid after reconnects. The backend initializes the SFTP channel automatically.

#### `listDir(nodeId, path)`

```typescript
sftp.listDir(nodeId: string, path: string): Promise<ReadonlyArray<PluginFileInfo>>
```

Lists the contents of a remote directory. Returns a frozen array of file information.

```javascript
const files = await ctx.sftp.listDir(nodeId, '/home/user');
for (const f of files) {
  console.log(`${f.file_type} ${f.name} (${f.size} bytes)`);
}
```

#### `stat(nodeId, path)`

```typescript
sftp.stat(nodeId: string, path: string): Promise<PluginFileInfo>
```

Gets metadata for a remote file or directory.

#### `readFile(nodeId, path)`

```typescript
sftp.readFile(nodeId: string, path: string): Promise<string>
```

Reads the content of a remote text file, up to 10 MB. Encoding is detected automatically and returned as a UTF-8 string. Throws for non-text files or files that exceed the size limit.

```javascript
const content = await ctx.sftp.readFile(nodeId, '/etc/hostname');
```

#### `writeFile(nodeId, path, content)`

```typescript
sftp.writeFile(nodeId: string, path: string, content: string): Promise<void>
```

Writes text content to a remote file using atomic writes to avoid corruption.

#### `mkdir(nodeId, path)`

```typescript
sftp.mkdir(nodeId: string, path: string): Promise<void>
```

Creates a directory on the remote host.

#### `delete(nodeId, path)`

```typescript
sftp.delete(nodeId: string, path: string): Promise<void>
```

Deletes a remote file. To delete a directory recursively, use `ctx.api.invoke('node_sftp_delete_recursive', { nodeId, path })`.

#### `rename(nodeId, oldPath, newPath)`

```typescript
sftp.rename(nodeId: string, oldPath: string, newPath: string): Promise<void>
```

Renames or moves a remote file or directory.

#### PluginFileInfo type

```typescript
type PluginFileInfo = Readonly<{
  name: string;
  path: string;
  file_type: 'file' | 'directory' | 'symlink' | 'unknown';
  size: number;
  modified: number | null;
  permissions: string | null;
}>;
```

---

### 6.12 ctx.forward

Port forwarding management API. Can be used to create, query, and manage SSH port forwarding without declaring anything in `contributes.apiCommands`.

Note: port forwarding uses `sessionId` rather than `nodeId`, because forwards are bound to the SSH session lifecycle. You can obtain the `sessionId` through `ctx.connections.getByNode(nodeId)?.id`.

#### `list(sessionId)`

```typescript
forward.list(sessionId: string): Promise<ReadonlyArray<PluginForwardRule>>
```

Lists all active port forwards for a session.

```javascript
const conn = ctx.connections.getByNode(nodeId);
if (conn) {
  const forwards = await ctx.forward.list(conn.id);
  forwards.forEach(f => console.log(`${f.forward_type} ${f.bind_address}:${f.bind_port} → ${f.target_host}:${f.target_port}`));
}
```

#### `create(request)`

```typescript
forward.create(request: PluginForwardRequest): Promise<{
  success: boolean;
  forward?: PluginForwardRule;
  error?: string;
}>
```

Creates a new port forward. Supports `local`, `remote`, and `dynamic` (SOCKS5) forwarding.

```javascript
const result = await ctx.forward.create({
  sessionId: conn.id,
  forwardType: 'local',
  bindAddress: '127.0.0.1',
  bindPort: 8080,
  targetHost: 'localhost',
  targetPort: 80,
  description: 'My plugin forward',
});
if (result.success) {
  console.log('Forward created:', result.forward?.id);
}
```

#### `stop(sessionId, forwardId)`

```typescript
forward.stop(sessionId: string, forwardId: string): Promise<void>
```

Stops a port forward.

#### `stopAll(sessionId)`

```typescript
forward.stopAll(sessionId: string): Promise<void>
```

Stops all port forwards for a session.

#### `listSavedForwards()` <small>v3</small>

```typescript
forward.listSavedForwards(): ReadonlyArray<SavedForwardSnapshot>
```

Returns the current snapshot of saved forwards persisted by the host.

#### `onSavedForwardsChange(handler)` <small>v3</small>

```typescript
forward.onSavedForwardsChange(handler: (items: ReadonlyArray<SavedForwardSnapshot>) => void): Disposable
```

Subscribes to saved-forward snapshot updates.

#### `exportSavedForwardsSnapshot()` / `applySavedForwardsSnapshot()` <small>v3</small>

```typescript
forward.exportSavedForwardsSnapshot(): Promise<SavedForwardsSyncSnapshot>
forward.applySavedForwardsSnapshot(snapshot: SavedForwardsSyncSnapshot): Promise<ApplySavedForwardsSyncSnapshotResult>
```

Exports or applies the host-managed saved-forward sync snapshot.

#### `getStats(sessionId, forwardId)`

```typescript
forward.getStats(sessionId: string, forwardId: string): Promise<{
  connectionCount: number;
  activeConnections: number;
  bytesSent: number;
  bytesReceived: number;
} | null>
```

Gets traffic statistics for a port forward.

#### Related types

```typescript
type PluginForwardRequest = {
  sessionId: string;
  forwardType: 'local' | 'remote' | 'dynamic';
  bindAddress: string;
  bindPort: number;
  targetHost: string;
  targetPort: number;
  description?: string;
};

type PluginForwardRule = Readonly<{
  id: string;
  forward_type: 'local' | 'remote' | 'dynamic';
  bind_address: string;
  bind_port: number;
  target_host: string;
  target_port: number;
  status: string;
  description?: string;
}>;

type SavedForwardSnapshot = Readonly<{
  id: string;
  session_id: string;
  owner_connection_id?: string;
  forward_type: string;
  bind_address: string;
  bind_port: number;
  target_host: string;
  target_port: number;
  auto_start: boolean;
  created_at: string;
  description?: string;
}>;
```

**Complete example**:

```javascript
export async function activate(ctx) {
  // 1. CSS declared in manifest.styles loads automatically (no code needed)
  // 2. Load additional CSS on demand
  const highlightCSS = await ctx.assets.loadCSS('./styles/highlight.css');

  // 3. Get an image URL
  const iconUrl = await ctx.assets.getAssetUrl('./assets/icon.svg');

  // 4. Get JSON configuration
  const configUrl = await ctx.assets.getAssetUrl('./assets/defaults.json');
  const configResp = await fetch(configUrl);
  const defaults = await configResp.json();
  ctx.assets.revokeAssetUrl(configUrl);

  ctx.ui.registerTabView('my-tab', (props) => {
    const { React } = window.__OXIDE__;
    return React.createElement('div', null,
      React.createElement('img', { src: iconUrl, width: 32 }),
      React.createElement('pre', null, JSON.stringify(defaults, null, 2)),
    );
  });
}
```

---

### 6.13 ctx.sessions (v3)

Read-only access API for the session tree. All data is provided as frozen snapshots.

#### `getTree()`

```typescript
sessions.getTree(): ReadonlyArray<SessionTreeNodeSnapshot>
```

Gets a frozen snapshot of the entire session tree.

```typescript
type SessionTreeNodeSnapshot = Readonly<{
  id: string;
  label: string;
  host?: string;
  port?: number;
  username?: string;
  parentId: string | null;
  childIds: readonly string[];
  connectionState: string;
  connectionId: string | null;
  terminalIds: readonly string[];
  sftpSessionId: string | null;
  errorMessage?: string;
}>;
```

```javascript
const tree = ctx.sessions.getTree();
tree.forEach(node => {
  console.log(`${node.label} (${node.connectionState})`);
  if (node.host) console.log(`  → ${node.username}@${node.host}:${node.port}`);
});
```

#### `getActiveNodes()`

```typescript
sessions.getActiveNodes(): ReadonlyArray<Readonly<{
  nodeId: string;
  sessionId: string | null;
  connectionState: string;
}>>
```

Gets a list of all active, connected nodes.

#### `getNodeState(nodeId)`

```typescript
sessions.getNodeState(nodeId: string): string | null
```

Gets the connection state of a single node. Returns `null` if the node does not exist.

#### `onTreeChange(handler)`

```typescript
sessions.onTreeChange(handler: (tree: ReadonlyArray<SessionTreeNodeSnapshot>) => void): Disposable
```

Subscribes to session tree structure changes. Triggered when nodes are added or removed, or when connection state changes.

```javascript
ctx.sessions.onTreeChange((tree) => {
  const activeCount = tree.filter(n => n.connectionState === 'active').length;
  status.update({ text: `${activeCount} active` });
});
```

#### `onNodeStateChange(nodeId, handler)`

```typescript
sessions.onNodeStateChange(nodeId: string, handler: (state: string) => void): Disposable
```

Subscribes to state changes for a specific node.

---

### 6.14 ctx.transfers (v3)

SFTP transfer monitoring API. Read-only access. Progress events are throttled to 500ms intervals.

#### `getAll()`

```typescript
transfers.getAll(): ReadonlyArray<TransferSnapshot>
```

Gets all current transfer tasks.

```typescript
type TransferSnapshot = Readonly<{
  id: string;
  nodeId: string;
  name: string;
  localPath: string;
  remotePath: string;
  direction: 'upload' | 'download';
  size: number;
  transferred: number;
  state: 'pending' | 'active' | 'paused' | 'completed' | 'cancelled' | 'error';
  error?: string;
  startTime: number;
  endTime?: number;
}>;
```

```javascript
const transfers = ctx.transfers.getAll();
const active = transfers.filter(t => t.state === 'active');
console.log(`${active.length} active transfers`);
```

#### `getByNode(nodeId)`

```typescript
transfers.getByNode(nodeId: string): ReadonlyArray<TransferSnapshot>
```

Gets transfer tasks for a specific node.

#### `onProgress(handler)`

```typescript
transfers.onProgress(handler: (transfer: TransferSnapshot) => void): Disposable
```

Subscribes to transfer progress updates. Throttled to **500ms** intervals to avoid high-frequency callbacks affecting performance.

```javascript
ctx.transfers.onProgress((t) => {
  const pct = Math.round((t.transferred / t.size) * 100);
  console.log(`${t.name}: ${pct}%`);
});
```

#### `onComplete(handler)` / `onError(handler)`

```typescript
transfers.onComplete(handler: (transfer: TransferSnapshot) => void): Disposable
transfers.onError(handler: (transfer: TransferSnapshot) => void): Disposable
```

Subscribes to transfer completion and error events.

```javascript
ctx.transfers.onComplete((t) => {
  ctx.ui.showToast({ title: `${t.name} uploaded`, variant: 'success' });
});

ctx.transfers.onError((t) => {
  ctx.ui.showToast({ title: `${t.name} failed: ${t.error}`, variant: 'error' });
});
```

---

### 6.15 ctx.profiler (v3)

Resource monitoring API. Provides read-only access to system metrics such as CPU, memory, and network. Metrics are pushed with **1s** throttling.

#### `getMetrics(nodeId)`

```typescript
profiler.getMetrics(nodeId: string): ProfilerMetricsSnapshot | null
```

Gets the latest metrics snapshot for a node.

```typescript
type ProfilerMetricsSnapshot = Readonly<{
  timestampMs: number;
  cpuPercent: number | null;
  memoryUsed: number | null;
  memoryTotal: number | null;
  memoryPercent: number | null;
  loadAvg1: number | null;
  loadAvg5: number | null;
  loadAvg15: number | null;
  cpuCores: number | null;
  netRxBytesPerSec: number | null;
  netTxBytesPerSec: number | null;
  sshRttMs: number | null;
}>;
```

```javascript
const metrics = ctx.profiler.getMetrics(nodeId);
if (metrics) {
  console.log(`CPU: ${metrics.cpuPercent}%, Mem: ${metrics.memoryPercent}%`);
}
```

#### `getHistory(nodeId, maxPoints?)`

```typescript
profiler.getHistory(nodeId: string, maxPoints?: number): ReadonlyArray<ProfilerMetricsSnapshot>
```

Gets historical metrics. `maxPoints` limits the number of returned data points, starting from the newest.

#### `isRunning(nodeId)`

```typescript
profiler.isRunning(nodeId: string): boolean
```

Checks whether performance monitoring is currently running for the specified node.

#### `onMetrics(nodeId, handler)`

```typescript
profiler.onMetrics(nodeId: string, handler: (metrics: ProfilerMetricsSnapshot) => void): Disposable
```

Subscribes to real-time metric updates. Throttled to **1 second** intervals.

```javascript
ctx.profiler.onMetrics(nodeId, (m) => {
  status.update({ text: `CPU ${m.cpuPercent?.toFixed(1)}%` });
});
```

---

### 6.16 ctx.eventLog (v3)

Read-only access API for connection event logs.

#### `getEntries(filter?)`

```typescript
eventLog.getEntries(filter?: {
  severity?: 'info' | 'warn' | 'error';
  category?: 'connection' | 'reconnect' | 'node';
}): ReadonlyArray<EventLogEntrySnapshot>
```

Gets event log entries, with optional filtering by severity and category.

```typescript
type EventLogEntrySnapshot = Readonly<{
  id: number;
  timestamp: number;
  severity: 'info' | 'warn' | 'error';
  category: 'connection' | 'reconnect' | 'node';
  nodeId?: string;
  connectionId?: string;
  title: string;
  detail?: string;
  source: string;
}>;
```

```javascript
const errors = ctx.eventLog.getEntries({ severity: 'error' });
console.log(`${errors.length} errors in log`);

errors.forEach(e => {
  console.log(`[${new Date(e.timestamp).toISOString()}] ${e.title}`);
});
```

#### `onEntry(handler)`

```typescript
eventLog.onEntry(handler: (entry: EventLogEntrySnapshot) => void): Disposable
```

Subscribes to new log entries.

```javascript
ctx.eventLog.onEntry((entry) => {
  if (entry.severity === 'error') {
    ctx.ui.showNotification({
      title: entry.title,
      body: entry.detail,
      severity: 'error',
    });
  }
});
```

---

### 6.17 ctx.ide (v3)

Read-only access API for IDE mode. When OxideTerm's built-in code editor based on CodeMirror is active, plugins can read project and file information.

#### `isOpen()`

```typescript
ide.isOpen(): boolean
```

Checks whether IDE mode is active.

#### `getProject()`

```typescript
ide.getProject(): IdeProjectSnapshot | null
```

Gets information about the current project.

```typescript
type IdeProjectSnapshot = Readonly<{
  nodeId: string;
  rootPath: string;
  name: string;
  isGitRepo: boolean;
  gitBranch?: string;
}>;
```

```javascript
const project = ctx.ide.getProject();
if (project) {
  console.log(`Project: ${project.name} @ ${project.rootPath}`);
  if (project.isGitRepo) console.log(`Branch: ${project.gitBranch}`);
}
```

#### `getOpenFiles()`

```typescript
ide.getOpenFiles(): ReadonlyArray<IdeFileSnapshot>
```

Gets the list of all open files.

```typescript
type IdeFileSnapshot = Readonly<{
  path: string;
  name: string;
  language: string;
  isDirty: boolean;
  isActive: boolean;
  isPinned: boolean;
}>;
```

#### `getActiveFile()`

```typescript
ide.getActiveFile(): IdeFileSnapshot | null
```

Gets the currently active file.

#### `onFileOpen(handler)` / `onFileClose(handler)`

```typescript
ide.onFileOpen(handler: (file: IdeFileSnapshot) => void): Disposable
ide.onFileClose(handler: (path: string) => void): Disposable
```

Subscribes to file open and close events.

#### `onActiveFileChange(handler)`

```typescript
ide.onActiveFileChange(handler: (file: IdeFileSnapshot | null) => void): Disposable
```

Subscribes to active file change events.

```javascript
ctx.ide.onActiveFileChange((file) => {
  if (file) {
    console.log(`Now editing: ${file.name} (${file.language})`);
  }
});
```

---

### 6.18 ctx.ai (v3)

Read-only access API for AI conversations. Plugins can read conversation lists and messages, but cannot start conversations or send messages.

:::caution
AI messages may contain terminal buffer content and should be treated as sensitive data.
:::

#### `getConversations()`

```typescript
ai.getConversations(): ReadonlyArray<AiConversationSnapshot>
```

Gets summaries of all conversations.

```typescript
type AiConversationSnapshot = Readonly<{
  id: string;
  title: string;
  messageCount: number;
  createdAt: number;
  updatedAt: number;
}>;
```

#### `getMessages(conversationId)`

```typescript
ai.getMessages(conversationId: string): ReadonlyArray<AiMessageSnapshot>
```

Gets all messages in the specified conversation.

```typescript
type AiMessageSnapshot = Readonly<{
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
}>;
```

```javascript
const convs = ctx.ai.getConversations();
if (convs.length > 0) {
  const messages = ctx.ai.getMessages(convs[0].id);
  console.log(`Latest conversation: ${convs[0].title} (${messages.length} messages)`);
}
```

#### `getActiveProvider()` / `getAvailableModels()`

```typescript
ai.getActiveProvider(): Readonly<{ type: string; displayName: string }> | null
ai.getAvailableModels(): ReadonlyArray<string>
```

Gets information about the current AI provider and the list of available models.

```javascript
const provider = ctx.ai.getActiveProvider();
if (provider) {
  console.log(`AI Provider: ${provider.displayName} (${provider.type})`);
  const models = ctx.ai.getAvailableModels();
  console.log(`Available models: ${models.join(', ')}`);
}
```

#### `onMessage(handler)`

```typescript
ai.onMessage(handler: (info: Readonly<{
  conversationId: string;
  messageId: string;
  role: string;
}>) => void): Disposable
```

Subscribes to new message events. Message content is not included; use `getMessages()` to retrieve it.

---

### 6.19 ctx.app (v3)

Application-level read-only information API. Provides global information such as theme, settings, platform, and version.

#### `getTheme()`

```typescript
app.getTheme(): ThemeSnapshot
```

Gets the current theme information.

```typescript
type ThemeSnapshot = Readonly<{
  name: string;
  isDark: boolean;
}>;
```

```javascript
const theme = ctx.app.getTheme();
console.log(`Theme: ${theme.name} (${theme.isDark ? 'dark' : 'light'})`);
```

#### `getSettings(category)`

```typescript
app.getSettings(category: 'terminal' | 'appearance' | 'general' | 'buffer' | 'sftp' | 'reconnect'):
  Readonly<Record<string, unknown>>
```

Gets a read-only snapshot of application settings for the specified category.

```javascript
const terminalSettings = ctx.app.getSettings('terminal');
console.log('Font size:', terminalSettings.fontSize);
```

#### `getVersion()` / `getPlatform()` / `getLocale()`

```typescript
app.getVersion(): string
app.getPlatform(): 'macos' | 'windows' | 'linux'
app.getLocale(): string
```

```javascript
console.log(`OxideTerm ${ctx.app.getVersion()} on ${ctx.app.getPlatform()}`);
console.log(`Locale: ${ctx.app.getLocale()}`);
```

#### `onThemeChange(handler)`

```typescript
app.onThemeChange(handler: (theme: ThemeSnapshot) => void): Disposable
```

Subscribes to theme change events.

```javascript
ctx.app.onThemeChange((theme) => {
  console.log(`Theme changed to ${theme.name}`);
});
```

#### `onSettingsChange(category, handler)`

```typescript
app.onSettingsChange(category: string, handler: (settings: Readonly<Record<string, unknown>>) => void): Disposable
```

Subscribes to changes for the specified settings category.

#### `getPoolStats()`

```typescript
app.getPoolStats(): Promise<PoolStatsSnapshot>
```

Gets SSH connection pool statistics.

```typescript
type PoolStatsSnapshot = Readonly<{
  activeConnections: number;
  totalSessions: number;
}>;
```

```javascript
const stats = await ctx.app.getPoolStats();
console.log(`Pool: ${stats.activeConnections} connections, ${stats.totalSessions} sessions`);
```

#### `refreshAfterExternalSync(options?)`

```typescript
app.refreshAfterExternalSync(options?: {
  connections?: boolean;
  savedForwards?: boolean;
  settings?: boolean;
}): Promise<void>
```

Forces the host to refresh selected state after an external sync operation has modified saved connections, saved forwards, or settings outside the normal in-app mutation flow.

```javascript
await ctx.app.refreshAfterExternalSync({
  connections: true,
  savedForwards: true,
  settings: true,
});
```

---

### 6.20 ctx.sync

Encrypted saved-connection sync API backed by `.oxide` import/export snapshots. Use this namespace when your plugin needs host-managed conflict resolution, revision tracking, and secure import/export flows.

#### `listSavedConnections()` / `refreshSavedConnections()`

```typescript
sync.listSavedConnections(): ReadonlyArray<SavedConnectionSnapshot>
sync.refreshSavedConnections(): Promise<ReadonlyArray<SavedConnectionSnapshot>>
```

Returns the current saved-connection snapshot list. `refreshSavedConnections()` forces a fresh host read before returning the latest snapshot.

```typescript
type SavedConnectionSnapshot = Readonly<{
  id: string;
  name: string;
  group: string | null;
  host: string;
  port: number;
  username: string;
  auth_type: 'password' | 'key' | 'agent' | 'certificate';
  key_path: string | null;
  cert_path: string | null;
  created_at: string;
  last_used_at: string | null;
  color: string | null;
  tags: readonly string[];
  agent_forwarding: boolean;
  proxy_chain: readonly Readonly<{
    host: string;
    port: number;
    username: string;
    auth_type: 'password' | 'key' | 'agent' | 'certificate';
    key_path?: string;
    cert_path?: string;
    agent_forwarding?: boolean;
  }>[];
}>;
```

This snapshot is intentionally metadata-only. Plugins never receive passwords or key/certificate passphrases, but they do receive key paths, certificate paths, and proxy-chain topology so sync and audit plugins can understand certificate-auth hops without touching secrets.

#### `onSavedConnectionsChange(handler)`

```typescript
sync.onSavedConnectionsChange(handler: (connections: ReadonlyArray<SavedConnectionSnapshot>) => void): Disposable
```

Subscribes to saved-connection snapshot updates.

```javascript
ctx.sync.onSavedConnectionsChange((connections) => {
  console.log(`Saved connections updated: ${connections.length}`);
});
```

#### `exportSavedConnectionsSnapshot()` / `applySavedConnectionsSnapshot()`

```typescript
sync.exportSavedConnectionsSnapshot(): Promise<SavedConnectionsSyncSnapshot>
sync.applySavedConnectionsSnapshot(
  snapshot: SavedConnectionsSyncSnapshot,
  options?: { conflictStrategy?: 'skip' | 'replace' | 'merge' },
): Promise<ApplySavedConnectionsSyncSnapshotResult>
```

Exports or applies a lightweight sync snapshot without packaging a full `.oxide` archive.

```typescript
type SavedConnectionsSyncSnapshot = Readonly<{
  revision: string;
  exportedAt: string;
  records: readonly SavedConnectionSyncRecord[];
}>;

type ApplySavedConnectionsSyncSnapshotResult = Readonly<{
  applied: number;
  skipped: number;
  conflicts: number;
}>;
```

#### `getLocalSyncMetadata()`

```typescript
sync.getLocalSyncMetadata(): Promise<LocalSyncMetadata>
```

Returns host-maintained revision metadata so sync plugins can do dirty checks and incremental uploads.

```typescript
type LocalSyncMetadata = Readonly<{
  savedConnectionsRevision: string;
  savedConnectionsUpdatedAt: string;
  savedForwardsRevision?: string;
  settingsRevision?: string;
  appSettingsSectionRevisions?: Readonly<Partial<Record<OxideAppSettingsSectionId, string>>>;
  pluginSettingsRevisions?: Readonly<Record<string, string>>;
}>;
```

#### `preflightExport(connectionIds?, options?)`

```typescript
sync.preflightExport(
  connectionIds?: string[],
  options?: { embedKeys?: boolean },
): Promise<ExportPreflightResult>
```

Checks whether a `.oxide` export can proceed before prompting for a password.

```typescript
type ExportPreflightResult = Readonly<{
  totalConnections: number;
  missingKeys: readonly [string, string][];
  connectionsWithKeys: number;
  connectionsWithPasswords: number;
  connectionsWithAgent: number;
  totalKeyBytes: number;
  canExport: boolean;
}>;
```

#### `exportOxide(request)`

```typescript
sync.exportOxide(request: {
  connectionIds?: string[];
  password: string;
  description?: string;
  embedKeys?: boolean;
  includeAppSettings?: boolean;
  selectedAppSettingsSections?: readonly OxideAppSettingsSectionId[];
  includeLocalTerminalEnvVars?: boolean;
  includePluginSettings?: boolean;
  selectedPluginIds?: string[];
  selectedForwardIds?: string[];
  onProgress?: (progress: { stage: string; current: number; total: number }) => void;
}): Promise<Uint8Array>
```

Builds an encrypted `.oxide` archive. Besides saved connections, the export can optionally include app settings snapshots, plugin settings snapshots, and saved forwards.

Supported `selectedAppSettingsSections` values are currently:
`'general'`, `'terminalAppearance'`, `'terminalBehavior'`, `'appearance'`, `'connections'`, `'fileAndEditor'`, and `'localTerminal'`.

```javascript
const archive = await ctx.sync.exportOxide({
  password,
  description: 'Nightly sync backup',
  includeAppSettings: true,
  selectedAppSettingsSections: ['general', 'appearance'],
  includePluginSettings: true,
  onProgress: (progress) => {
    console.log(`Export ${progress.stage}: ${progress.current}/${progress.total}`);
  },
});
```

#### `validateOxide(fileData)`

```typescript
sync.validateOxide(fileData: Uint8Array): Promise<OxideMetadata>
```

Reads archive metadata without importing it.

```typescript
type OxideMetadata = Readonly<{
  exported_at: string;
  exported_by: string;
  description?: string;
  num_connections: number;
  connection_names: readonly string[];
  has_app_settings?: boolean;
  plugin_settings_count?: number;
}>;
```

#### `previewImport(fileData, password, options?)`

```typescript
sync.previewImport(
  fileData: Uint8Array,
  password: string,
  options?: {
    conflictStrategy?: 'rename' | 'skip' | 'replace' | 'merge';
    onProgress?: (progress: { stage: string; current: number; total: number }) => void;
  },
): Promise<ImportPreview>
```

Generates an import preview so the plugin can explain rename, skip, replace, and merge decisions before applying anything.

```typescript
type ImportPreview = Readonly<{
  totalConnections: number;
  unchanged: readonly string[];
  willRename: readonly [string, string][];
  willSkip: readonly string[];
  willReplace: readonly string[];
  willMerge: readonly string[];
  hasEmbeddedKeys: boolean;
  totalForwards: number;
  hasAppSettings: boolean;
  appSettingsFormat?: 'legacy' | 'sectioned';
  appSettingsKeys?: readonly string[];
  appSettingsPreview?: Readonly<Record<string, string>>;
  appSettingsSections?: ReadonlyArray<Readonly<{
    id: string;
    fieldKeys: readonly string[];
    fieldValues?: Readonly<Record<string, string>>;
    containsEnvVars?: boolean;
  }>>;
  pluginSettingsCount: number;
  pluginSettingsByPlugin: Readonly<Record<string, number>>;
  forwardDetails: ReadonlyArray<Readonly<{
    ownerConnectionName: string;
    direction: 'local' | 'remote' | 'dynamic';
    description: string;
  }>>;
  records: ReadonlyArray<Readonly<{
    resource: 'connection';
    name: string;
    action: 'import' | 'rename' | 'skip' | 'replace' | 'merge';
    reasonCode: 'new-connection' | 'name-conflict' | 'name-conflict-skipped' | 'replace-existing' | 'merge-existing';
    targetName?: string;
    targetConnectionId?: string;
    forwardCount: number;
    hasEmbeddedKeys: boolean;
  }>>;
}>;
```

#### `importOxide(fileData, password, options?)`

```typescript
sync.importOxide(
  fileData: Uint8Array,
  password: string,
  options?: {
    selectedNames?: string[];
    conflictStrategy?: 'rename' | 'skip' | 'replace' | 'merge';
    importAppSettings?: boolean;
    selectedAppSettingsSections?: readonly string[];
    importPluginSettings?: boolean;
    selectedPluginIds?: string[];
    importForwards?: boolean;
    onProgress?: (progress: { stage: string; current: number; total: number }) => void;
  },
): Promise<ImportResult>
```

Imports a `.oxide` archive using host-managed conflict resolution. The `merge` strategy is designed for multi-device sync scenarios where local connection IDs and local-only secrets should be preserved where possible.

```typescript
type ImportResult = Readonly<{
  imported: number;
  skipped: number;
  merged: number;
  replaced: number;
  renamed: number;
  errors: readonly string[];
  renames: readonly [string, string][];
  importedAppSettings: boolean;
  skippedAppSettings: boolean;
  importedPluginSettings: number;
  skippedPluginSettings: boolean;
  importedForwards: number;
  skippedForwards: number;
}>;
```

---

### 6.21 ctx.secrets

Plugin-scoped secure storage backed by the OS keychain. Use this namespace for API tokens, credentials, refresh tokens, and any value that should not be persisted in `ctx.storage`.

#### `get(key)`

```typescript
secrets.get(key: string): Promise<string | null>
```

Returns the secret value for a key, or `null` if it does not exist.

#### `getMany(keys)`

```typescript
secrets.getMany(keys: readonly string[]): Promise<Readonly<Record<string, string | null>>>
```

Fetches multiple secrets in a single call. Prefer this method when one user action needs several credentials, because the host can often collapse keychain unlocks into a single prompt.

```javascript
const secrets = await ctx.secrets.getMany(['endpoint', 'username', 'token']);
console.log(secrets.endpoint, secrets.username);
```

#### `set(key, value)`

```typescript
secrets.set(key: string, value: string): Promise<void>
```

Stores or overwrites a secret value.

#### `has(key)`

```typescript
secrets.has(key: string): Promise<boolean>
```

Checks whether a secret exists without reading its value.

#### `delete(key)`

```typescript
secrets.delete(key: string): Promise<void>
```

Removes the specified secret from the keychain.

```javascript
if (!(await ctx.secrets.has('accessToken'))) {
  await ctx.secrets.set('accessToken', token);
}
```

> Secrets are namespaced per plugin. One plugin cannot read another plugin's keychain entries through `ctx.secrets`.

---

## 7. Shared Modules (window.__OXIDE__)

### 7.1 Available Modules

Plugins **must** use the shared modules provided by the host instead of bundling their own copies of React or similar libraries. This guarantees React hook compatibility and avoids duplicate-instance problems.

```typescript
window.__OXIDE__ = {
  React: typeof import('react');
  ReactDOM: { createRoot: typeof import('react-dom/client').createRoot };
  zustand: { create: typeof import('zustand').create };
  lucideIcons: Record<string, React.FC>;  // Lucide icon name -> component mapping
  clsx: typeof import('clsx').clsx;        // Lightweight className builder
  cn: (...inputs: ClassValue[]) => string; // Tailwind-merge + clsx
  useTranslation: typeof import('react-i18next').useTranslation; // i18n hook
  ui: PluginUIKit;   // Plugin UI component library
};
```

### 7.2 Using React

```javascript
const { React } = window.__OXIDE__;
const { createElement: h, useState, useEffect, useCallback, useRef, useMemo } = React;

// Use createElement instead of JSX
function MyComponent({ name }) {
  const [count, setCount] = useState(0);

  return h('div', null,
    h('h1', null, `Hello ${name}!`),
    h('button', { onClick: () => setCount(c => c + 1) }, `Count: ${count}`),
  );
}
```

:::note
Because plugins are plain JS rather than JSX, use `React.createElement` (commonly abbreviated to `h`) instead of JSX syntax. If you use a bundler, configure a JSX transform.
:::

**All React Hooks are available**, including but not limited to:
- `useState` / `useReducer` for state management
- `useEffect` / `useLayoutEffect` for side effects
- `useCallback` / `useMemo` for performance optimization
- `useRef` for references
- `useContext` for context values, if you create your own Context

### 7.3 Using Zustand

Plugins can use the host's Zustand instance to create their own stores:

```javascript
const { zustand } = window.__OXIDE__;

const useMyStore = zustand.create((set) => ({
  items: [],
  addItem: (item) => set((s) => ({ items: [...s.items, item] })),
  clearItems: () => set({ items: [] }),
}));

// Use inside a component
function ItemList() {
  const { items, clearItems } = useMyStore();
  return h('div', null,
    h('ul', null, items.map((item, i) => h('li', { key: i }, item))),
    h('button', { onClick: clearItems }, 'Clear'),
  );
}
```

### 7.4 Using Lucide React Icons

```javascript
const { lucideIcons, lucideReact } = window.__OXIDE__;
// lucideIcons is a { name: component } mapping object
const Activity = lucideIcons['Activity'];
const Terminal = lucideIcons['Terminal'];
// lucideReact is the full module proxy with fallback; missing PascalCase icons fall back to Puzzle
const Wifi = lucideReact.Wifi;

function MyIcon() {
  return h(Activity, { className: 'h-4 w-4 text-primary' });
}
```

See the full icon list at: https://lucide.dev/icons/

> **Manifest icon resolution**: the `contributes.tabs[].icon` and `contributes.sidebarPanels[].icon` fields in `plugin.json` use icon-name strings such as `"LayoutDashboard"`. The system resolves them automatically through `resolvePluginIcon()` into the corresponding Lucide React component for tab bar and sidebar activity bar rendering. Inside plugin components, use `lucideIcons['IconName']` when indexing by string, or prefer `lucideReact.IconName` when you want automatic fallback behavior for missing icons.

### 7.5 Using the UI Kit (Recommended)

OxideTerm provides a lightweight UI component library at `window.__OXIDE__.ui` that wraps OxideTerm's theme system. **Strongly prefer the UI Kit over hand-written Tailwind class names** because it gives you:

- Automatic adaptation to all themes, including dark, light, and custom themes
- Protection against class name typos
- Much less boilerplate code
- Fewer plugin changes when the theme system evolves

```javascript
const { React, lucideIcons, ui } = window.__OXIDE__;
const { createElement: h, useState } = React;
const Activity = lucideIcons['Activity'];
const Settings = lucideIcons['Settings'];
const Terminal = lucideIcons['Terminal'];
```

**Component overview**:

| Component | Purpose | Example |
|------|------|------|
| `ui.ScrollView` | Full-height scroll container for Tab roots | `h(ui.ScrollView, null, children)` |
| `ui.Stack` | Flex layout, horizontal or vertical | `h(ui.Stack, { direction: 'horizontal', gap: 2 }, ...)` |
| `ui.Grid` | Grid layout | `h(ui.Grid, { cols: 3, gap: 4 }, ...)` |
| `ui.Card` | Card with title and icon | `h(ui.Card, { icon: Activity, title: 'Stats' }, ...)` |
| `ui.Stat` | Numeric stat card | `h(ui.Stat, { icon: Hash, label: 'Input', value: 42 })` |
| `ui.Button` | Button | `h(ui.Button, { variant: 'primary', onClick }, 'Click')` |
| `ui.Input` | Text input | `h(ui.Input, { value, onChange, placeholder: '...' })` |
| `ui.Checkbox` | Checkbox | `h(ui.Checkbox, { checked, onChange, label: 'Enable' })` |
| `ui.Select` | Dropdown select | `h(ui.Select, { value, options, onChange })` |
| `ui.Toggle` | Toggle control | `h(ui.Toggle, { checked, onChange, label: 'Auto refresh' })` |
| `ui.Text` | Semantic text | `h(ui.Text, { variant: 'heading' }, 'Title')` |
| `ui.Badge` | Status badge | `h(ui.Badge, { variant: 'success' }, 'Online')` |
| `ui.Separator` | Divider | `h(ui.Separator)` |
| `ui.IconText` | Icon + text row | `h(ui.IconText, { icon: Terminal }, 'Terminal')` |
| `ui.KV` | Key-value row | `h(ui.KV, { label: 'Host' }, '192.168.1.1')` |
| `ui.EmptyState` | Empty-state placeholder | `h(ui.EmptyState, { icon: Inbox, title: 'No Data' })` |
| `ui.ListItem` | Clickable list item | `h(ui.ListItem, { icon: Server, title: 'prod-01', onClick })` |
| `ui.Progress` | Progress bar | `h(ui.Progress, { value: 75, variant: 'success' })` |
| `ui.Alert` | Info / warning box | `h(ui.Alert, { variant: 'warning', title: 'Attention' }, '...')` |
| `ui.Spinner` | Loading indicator | `h(ui.Spinner, { label: 'Loading...' })` |
| `ui.Table` | Data table | `h(ui.Table, { columns, data, onRowClick })` |
| `ui.CodeBlock` | Code or terminal output | `h(ui.CodeBlock, null, 'ssh root@...')` |
| `ui.Tabs` | Tab switcher | `h(ui.Tabs, { tabs, activeTab, onTabChange }, content)` |
| `ui.Header` | Page-level header bar | `h(ui.Header, { icon: Layout, title: 'Dashboard' })` |

**Quick example — Tab component**:

```javascript
function MyTab({ tabId, pluginId }) {
  const [count, setCount] = useState(0);

  return h(ui.ScrollView, null,
    h(ui.Header, {
      icon: Activity,
      title: 'My Plugin',
      subtitle: `v1.0.0`,
    }),
    h(ui.Grid, { cols: 3, gap: 3 },
      h(ui.Stat, { icon: Terminal, label: 'Sessions', value: 5 }),
      h(ui.Stat, { icon: Activity, label: 'Traffic', value: '12 KB' }),
      h(ui.Stat, { icon: Clock, label: 'Uptime', value: '2h' }),
    ),
    h(ui.Card, { icon: Settings, title: 'Control Panel' },
      h(ui.Stack, { gap: 2 },
        h(ui.Text, { variant: 'muted' }, 'Click the button to increase the counter'),
        h(ui.Stack, { direction: 'horizontal', gap: 2 },
          h(ui.Button, { variant: 'primary', onClick: () => setCount(c => c + 1) }, `Count: ${count}`),
          h(ui.Button, { variant: 'ghost', onClick: () => setCount(0) }, 'Reset'),
        ),
      ),
    ),
  );
}
```

**Quick example — Sidebar panel**:

```javascript
function MySidebar() {
  return h(ui.Stack, { gap: 2, className: 'p-2' },
    h(ui.Text, { variant: 'label' }, 'My Plugin'),
    h(ui.KV, { label: 'Status', mono: true }, 'active'),
    h(ui.KV, { label: 'Connections', mono: true }, '3'),
    h(ui.Button, {
      variant: 'outline',
      size: 'sm',
      className: 'w-full',
      onClick: () => ctx.ui.openTab('myTab'),
    }, 'Open Details'),
  );
}
```

:::note
All UI Kit components accept a `className` prop, so you can append custom Tailwind classes for fine-tuning.
:::

---

## 8. UI Component Development

### 8.1 Tab View Components

Tab components receive `PluginTabProps`:

```javascript
// Recommended: use the UI Kit
function MyTabView({ tabId, pluginId }) {
  return h(ui.ScrollView, null,
    h(ui.Header, { icon: LayoutDashboard, title: 'My Plugin Tab' }),
    h(ui.Card, { title: 'Content Area' },
      h(ui.Text, { variant: 'body' }, 'This is a plugin Tab.'),
    ),
  );
}
```

**Pure createElement style** (not recommended, but also supported):

```javascript
function MyTabView({ tabId, pluginId }) {
  return h('div', { className: 'h-full overflow-auto p-6' },
    h('div', { className: 'max-w-4xl mx-auto' },
      h('h1', { className: 'text-xl font-bold text-theme-text' }, 'My Plugin Tab'),
    ),
  );
}
```

**Registration inside `activate()`**:

```javascript
ctx.ui.registerTabView('myTab', MyTabView);
```

**Open a Tab**:

```javascript
ctx.ui.openTab('myTab');
```

**Recommended Tab component structure**:

```javascript
function MyTab({ tabId, pluginId }) {
  return h(ui.ScrollView, null,
    h(ui.Header, {
      icon: SomeIcon,
      title: 'Title',
      subtitle: 'Description',
    }),
    h(ui.Grid, { cols: 3, gap: 3 },
      h(ui.Stat, { icon: Icon1, label: 'Metric', value: 42 }),
    ),
    h(ui.Card, { icon: SomeIcon, title: 'Section' },
      h(ui.Stack, { gap: 2 }, /* children */),
    ),
  );
}
```

### 8.2 Sidebar Panel Components

Sidebar panel components are function components without props:

```javascript
// Recommended: use the UI Kit
function MyPanel() {
  return h(ui.Stack, { gap: 2, className: 'p-2' },
    h(ui.Text, { variant: 'label', className: 'px-1' }, 'My Panel'),
    h(ui.KV, { label: 'Status', mono: true }, 'active'),
    h(ui.KV, { label: 'Connections', mono: true }, '3'),
    h(ui.Button, {
      variant: 'outline', size: 'sm', className: 'w-full mt-1',
      onClick: () => ctx.ui.openTab('myTab'),
    }, 'Open in Tab'),
  );
}
```

**Pure createElement style**:

```javascript
function MyPanel() {
  return h('div', { className: 'p-2 space-y-2' },
    h('div', { className: 'text-xs font-semibold text-theme-text-muted uppercase tracking-wider px-1 mb-1' },
      'My Panel'
    ),
  );
}
```

Because sidebar space is limited, the recommended approach is:
- use small text such as `text-xs`
- keep layouts compact, such as `p-2` and `space-y-1`
- provide an `Open in Tab` button that links to a more detailed view

### 8.3 UI Kit Component Reference

Below is the full API reference for all `window.__OXIDE__.ui` components.

#### Layout Components

**ScrollView** — standard root container for a Tab

| Prop | Type | Default | Description |
|------|------|--------|------|
| `maxWidth` | `string` | `'4xl'` | Tailwind max-width suffix |
| `padding` | `string` | `'6'` | Tailwind padding suffix |
| `className` | `string` | — | Additional custom classes |

```javascript
h(ui.ScrollView, null, /* all Tab content */);
h(ui.ScrollView, { maxWidth: '6xl', padding: '4' }, children);
```

**Stack** — flex layout

| Prop | Type | Default | Description |
|------|------|--------|------|
| `direction` | `'vertical' \| 'horizontal'` | `'vertical'` | Layout direction |
| `gap` | `number` | `2` | Gap value (Tailwind gap scale) |
| `align` | `'start' \| 'center' \| 'end' \| 'stretch' \| 'baseline'` | — | Cross-axis alignment |
| `justify` | `'start' \| 'center' \| 'end' \| 'between' \| 'around'` | — | Main-axis alignment |
| `wrap` | `boolean` | `false` | Whether wrapping is enabled |

```javascript
h(ui.Stack, { direction: 'horizontal', gap: 2, align: 'center' },
  h(ui.Button, null, 'A'),
  h(ui.Button, null, 'B'),
);
```

**Grid** — grid layout

| Prop | Type | Default | Description |
|------|------|--------|------|
| `cols` | `number` | `2` | Number of columns |
| `gap` | `number` | `4` | Gap size |

```javascript
h(ui.Grid, { cols: 3, gap: 3 },
  h(ui.Stat, { label: 'A', value: 1 }),
  h(ui.Stat, { label: 'B', value: 2 }),
  h(ui.Stat, { label: 'C', value: 3 }),
);
```

#### Container Components

**Card** — theme-aware card

| Prop | Type | Default | Description |
|------|------|--------|------|
| `title` | `string` | — | Card title |
| `icon` | `React.ComponentType` | — | Leading title icon, usually a Lucide component |
| `headerRight` | `React.ReactNode` | — | Custom content on the right side of the header |

```javascript
h(ui.Card, {
  icon: Settings,
  title: 'Settings',
  headerRight: h(ui.Badge, { variant: 'info' }, 'v2'),
},
  h(ui.Text, { variant: 'muted' }, 'Card content'),
);
```

**Stat** — numeric stat card

| Prop | Type | Description |
|------|------|------|
| `label` | `string` | Descriptive text |
| `value` | `string \| number` | Displayed numeric or textual value |
| `icon` | `React.ComponentType` | Optional icon |

```javascript
h(ui.Stat, { icon: Activity, label: 'Traffic', value: '12.5 KB' })
```

#### Form Components

**Button** — button

| Prop | Type | Default | Description |
|------|------|--------|------|
| `variant` | `'primary' \| 'secondary' \| 'destructive' \| 'ghost' \| 'outline'` | `'secondary'` | Style variant |
| `size` | `'sm' \| 'md' \| 'lg' \| 'icon'` | `'md'` | Size |
| `disabled` | `boolean` | `false` | Disabled state |
| `onClick` | `function` | — | Click callback |

```javascript
h(ui.Button, { variant: 'primary', onClick: handler }, 'Save');
h(ui.Button, { variant: 'destructive', size: 'sm' }, 'Delete');
h(ui.Button, { variant: 'ghost', size: 'icon' }, h(Trash2, { className: 'h-4 w-4' }));
```

**Input** — text input

| Prop | Type | Default | Description |
|------|------|--------|------|
| `value` / `defaultValue` | `string` | — | Controlled or uncontrolled value |
| `placeholder` | `string` | — | Placeholder text |
| `type` | `string` | `'text'` | HTML input type |
| `size` | `'sm' \| 'md'` | `'md'` | Size |
| `onChange` | `function` | — | Change callback |
| `onKeyDown` | `function` | — | Keyboard callback |

```javascript
h(ui.Input, {
  value: text,
  onChange: (e) => setText(e.target.value),
  placeholder: 'Enter a search keyword...',
  size: 'sm',
});
```

**Checkbox** — checkbox

| Prop | Type | Description |
|------|------|------|
| `checked` | `boolean` | Checked state |
| `onChange` | `(checked: boolean) => void` | Change callback |
| `label` | `string` | Optional label |
| `disabled` | `boolean` | Disabled state |

```javascript
h(ui.Checkbox, { checked: enabled, onChange: setEnabled, label: 'Enable feature' })
```

**Select** — dropdown select

| Prop | Type | Description |
|------|------|------|
| `value` | `string \| number` | Current value |
| `options` | `{ label: string, value: string \| number }[]` | Option list |
| `onChange` | `(value: string) => void` | Change callback |
| `placeholder` | `string` | Placeholder |
| `size` | `'sm' \| 'md'` | Size |

```javascript
h(ui.Select, {
  value: theme,
  options: [
    { label: 'Dark', value: 'dark' },
    { label: 'Light', value: 'light' },
  ],
  onChange: setTheme,
});
```

#### Typography and Presentation Components

**Text** — semantic text

| variant | Style | Typical use |
|---------|------|----------|
| `'heading'` | large bold text | page title |
| `'subheading'` | smaller bold text | section title |
| `'body'` | standard text | paragraph content |
| `'muted'` | subdued small text | descriptions / hints |
| `'mono'` | monospace text | IP addresses / code |
| `'label'` | uppercase muted text | section label |
| `'tiny'` | extra-small muted text | secondary metadata |

You can change the rendered tag through the `as` prop, for example `h(ui.Text, { variant: 'heading', as: 'h2' }, '...')`.

**Badge** — status badge

| variant | Color | Use |
|---------|------|------|
| `'default'` | gray | neutral state |
| `'success'` | green | success / online |
| `'warning'` | yellow | warning |
| `'error'` | red | error / offline |
| `'info'` | blue | information / version |

```javascript
h(ui.Badge, { variant: 'success' }, 'Active')
```

**KV** — key-value row

```javascript
h(ui.KV, { label: 'Host', mono: true }, '192.168.1.1')
```

Set `mono: true` to render the value in monospace.

**IconText** — icon + text

```javascript
h(ui.IconText, { icon: Terminal }, 'Active Sessions')
```

**Separator** — divider

```javascript
h(ui.Separator)
```

**EmptyState** — empty-state placeholder

```javascript
h(ui.EmptyState, {
  icon: Inbox,
  title: 'No Data',
  description: 'Add a new item to get started.',
  action: h(ui.Button, { variant: 'primary' }, 'Add'),
})
```

**ListItem** — list row

```javascript
h(ui.ListItem, {
  icon: Server,
  title: 'production-01',
  subtitle: 'root@10.0.1.1',
  right: h(ui.Badge, { variant: 'success' }, 'Active'),
  active: isSelected,
  onClick: () => select(item),
})
```

**Header** — page header bar

```javascript
h(ui.Header, {
  icon: LayoutDashboard,
  title: 'Dashboard',
  subtitle: 'v1.0.0',
  action: h(ui.Button, { size: 'sm' }, 'Refresh'),
})
```

**Tabs** — tab switcher

```javascript
const [tab, setTab] = useState('overview');
h(ui.Tabs, {
  tabs: [
    { id: 'overview', label: 'Overview', icon: Activity },
    { id: 'logs', label: 'Logs', icon: FileText },
  ],
  activeTab: tab,
  onTabChange: setTab,
},
  tab === 'overview' ? h(OverviewPanel) : h(LogsPanel),
)
```

| Prop | Type | Description |
|------|------|------|
| `tabs` | `{ id: string, label: string, icon?: Component }[]` | Tab definition array |
| `activeTab` | `string` | Active tab id |
| `onTabChange` | `(id: string) => void` | Tab change callback |

**Table** — data table

```javascript
h(ui.Table, {
  columns: [
    { key: 'host', header: 'Host' },
    { key: 'port', header: 'Port', align: 'right', width: '80px' },
    { key: 'status', header: 'Status', render: (v) => h(ui.Badge, { variant: v === 'active' ? 'success' : 'error' }, v) },
  ],
  data: connections,
  striped: true,
  onRowClick: (row) => select(row.id),
})
```

| Prop | Type | Default | Description |
|------|------|--------|------|
| `columns` | `{ key, header, width?, align?, render? }[]` | — | Column definitions |
| `data` | `Record<string, unknown>[]` | — | Data rows |
| `compact` | `boolean` | `false` | Compact row height |
| `striped` | `boolean` | `false` | Zebra striping |
| `emptyText` | `string` | `'No data'` | Empty-state text |
| `onRowClick` | `(row, index) => void` | — | Row click callback |

**Progress** — progress bar

```javascript
h(ui.Progress, { value: 75, max: 100, variant: 'success', showLabel: true })
```

| variant | Color |
|---------|------|
| `'default'` | theme accent color |
| `'success'` | green |
| `'warning'` | yellow |
| `'error'` | red |

**Toggle** — toggle control

```javascript
h(ui.Toggle, { checked: autoRefresh, onChange: setAutoRefresh, label: 'Auto Refresh' })
```

Unlike a checkbox, `Toggle` uses a switch-style control and is better suited to on/off scenarios.

**Alert** — info / warning box

```javascript
h(ui.Alert, { variant: 'warning', icon: AlertTriangle, title: 'Attention' },
  'This action cannot be undone.',
)
```

| variant | Color | Use |
|---------|------|------|
| `'info'` | blue | information |
| `'success'` | green | success |
| `'warning'` | yellow | warning |
| `'error'` | red | error |

**Spinner** — loading indicator

```javascript
h(ui.Spinner, { size: 'sm', label: 'Loading...' })
```

Available `size` values: `'sm'` (16px), `'md'` (24px), `'lg'` (32px)

**CodeBlock** — code or terminal output

```javascript
h(ui.CodeBlock, { maxHeight: '200px', wrap: true },
  'ssh root@192.168.1.1\nPassword: ****\nWelcome to Ubuntu 22.04',
)
```

| Prop | Type | Default | Description |
|------|------|--------|------|
| `maxHeight` | `string` | `'300px'` | Max height with scroll overflow |
| `wrap` | `boolean` | `false` | Whether to soft-wrap lines |

### 8.4 Theme CSS Variable Reference (Advanced)

If you need custom styling beyond what the UI Kit covers, you can directly use OxideTerm's semantic CSS classes.

**Text colors**:

| Class | Use |
|------|------|
| `text-theme-text` | primary text |
| `text-theme-text-muted` | secondary / muted text |
| `text-theme-accent` | accent text |

**Background colors**:

| Class | Use |
|------|------|
| `bg-theme-bg` | page background |
| `bg-theme-bg-panel` | card / panel background |
| `bg-theme-bg-hover` | hover highlight background |
| `bg-theme-accent` | accent background |

**Borders**:

| Class | Use |
|------|------|
| `border-theme-border` | standard border |

:::caution
**Do not use hard-coded colors** such as `text-white` or `bg-gray-800`. Always use semantic classes so the plugin remains compatible with all themes.
:::

### 8.5 Communication Between Components

Because Tab and Sidebar components are rendered separately, they cannot communicate directly through React props. Recommended approaches:

**Option 1: Zustand store (recommended)**

```javascript
const { zustand } = window.__OXIDE__;

const useMyStore = zustand.create((set) => ({
  data: [],
  setData: (data) => set({ data }),
}));

function MyTab() {
  const { data } = useMyStore();
  return h('div', null, `Items: ${data.length}`);
}

function MyPanel() {
  const { data } = useMyStore();
  return h('div', null, `Count: ${data.length}`);
}
```

**Option 2: Global variable + captured ctx reference**

```javascript
// In activate()
window.__MY_PLUGIN_CTX__ = ctx;

// Inside components
function MyTab() {
  const ctx = window.__MY_PLUGIN_CTX__;
  const conns = ctx?.connections.getAll() ?? [];
  // ...
}

// Cleanup in deactivate()
export function deactivate() {
  delete window.__MY_PLUGIN_CTX__;
}
```

---

## 9. Terminal Hooks Development

### 9.1 Input Interceptor

Input interceptors are called synchronously every time the user sends data to the terminal. They run directly on the terminal I/O hot path.

**Call chain**:

```
User input -> term.onData(data)
  -> runInputPipeline(data, sessionId)
    -> iterate all interceptors
      -> interceptor(data, { sessionId })
        -> return modified data or null
  -> if result is not null -> send through WebSocket to the backend
```

**Use cases**:

- input filtering and auditing
- automatic prefix insertion
- command interception and mistake prevention
- input statistics

```javascript
// Example: add an input prefix based on settings
ctx.terminal.registerInputInterceptor((data, { sessionId }) => {
  const prefix = ctx.settings.get('inputPrefix');
  if (prefix) return prefix + data;
  return data;
});
```

**Important notes**:

1. Interceptors are **synchronous** and do not support async
2. Returning `null` fully suppresses the input so nothing is sent to the server
3. Interceptors from multiple plugins are chained in registration order, where the previous output becomes the next input
4. Exceptions are silently caught and the data is passed through unchanged (fail-open)
5. There is a **5ms time budget**; see [9.4](#94-performance-budget-and-circuit-breaker)

### 9.2 Output Processor

Output processors are called synchronously each time terminal data is received from the remote server.

**Call chain**:

```
WebSocket receives MSG_TYPE_DATA
  -> runOutputPipeline(data, sessionId)
    -> iterate all processors
      -> processor(data, { sessionId })
        -> return processed Uint8Array
  -> write into xterm.js for rendering
```

**Use cases**:

- output statistics and auditing
- sensitive-data masking
- output logging

```javascript
ctx.terminal.registerOutputProcessor((data, { sessionId }) => {
  // Count bytes
  totalBytes += data.length;

  // Pass raw data through unchanged
  return data;
});
```

**Notes**:

1. The input parameter is `Uint8Array` (raw bytes), not a string
2. The return value must also be `Uint8Array`
3. Like Input Interceptors, it has a 5ms time budget
4. Fail-open on exceptions: if a processor throws, the previous step's data is used

### 9.3 Shortcuts

Registers keyboard shortcuts that are active while the terminal has focus.

**Registration**:

```javascript
// manifest:
// "shortcuts": [{ "key": "ctrl+shift+d", "command": "openDashboard" }]

ctx.terminal.registerShortcut('openDashboard', () => {
  ctx.ui.openTab('dashboard');
});
```

**Shortcut matching flow**:

```
Terminal keydown event
  -> matchPluginShortcut(event)
    -> build normalized key: parts.sort().join('+')
      example: Ctrl+Shift+D -> "ctrl+d+shift"
    -> look up in the shortcuts Map
    -> if found -> call handler and prevent default behavior
```

**Modifier key mapping**:

- `event.ctrlKey || event.metaKey` -> `"ctrl"` (on macOS, Cmd also counts as Ctrl)
- `event.shiftKey` -> `"shift"`
- `event.altKey` -> `"alt"`

### 9.4 Performance Budget and Circuit Breaker

Terminal hooks run on the terminal I/O hot path, so every keystroke or received data chunk triggers them synchronously. Because of that, the performance limits are strict:

**Time budget**: each hook invocation must complete within **5ms** (`HOOK_BUDGET_MS`)

- timeouts emit `console.warn`
- timeouts count toward the circuit breaker error total

**Circuit breaker**: **10 errors / 60 seconds** -> the plugin is automatically disabled

- the counter resets after the 60-second window expires
- once the circuit breaker trips, the plugin is unloaded immediately
- the disabled state is persisted to `plugin-config.json` so it survives restarts

**Best practices**:

```javascript
// Good: lightweight synchronous work
ctx.terminal.registerInputInterceptor((data) => {
  counter++;
  return data;
});

// Bad: expensive work
ctx.terminal.registerInputInterceptor((data) => {
  // Do not perform large-text regex work, DOM operations, and so on here
  const result = someExpensiveRegex.test(data);
  return data;
});

// Good: defer heavy work to a microtask
ctx.terminal.registerOutputProcessor((data) => {
  queueMicrotask(() => {
    // Put heavy work here
    processDataAsync(data);
  });
  return data;
});
```

---

## 10. Connection Event System

### 10.1 Connection Lifecycle Events

OxideTerm's Event Bridge turns connection-state changes in `appStore` into plugin-subscribeable events.

**Event trigger conditions**:

| Event | Trigger condition |
|------|----------|
| `connection:connect` | A new connection appears and its state is `active`; or a non-active state other than reconnecting / link_down / error changes to `active` |
| `connection:reconnect` | State changes from `reconnecting` / `link_down` / `error` to `active` |
| `connection:link_down` | Enters the `reconnecting` / `link_down` / `error` state |
| `connection:disconnect` | Enters `disconnected` / `disconnecting`, or the connection is removed from the list |

**Example usage**:

```javascript
const disposable1 = ctx.events.onConnect((snapshot) => {
  console.log(`Connected: ${snapshot.username}@${snapshot.host}`);
  console.log(`State: ${snapshot.state}, Terminals: ${snapshot.terminalIds.length}`);
});

const disposable2 = ctx.events.onDisconnect((snapshot) => {
  console.log(`Disconnected: ${snapshot.id}`);
});

const disposable3 = ctx.events.onLinkDown((snapshot) => {
  ctx.ui.showToast({
    title: 'Connection Lost',
    description: `${snapshot.host} link down`,
    variant: 'warning',
  });
});

const disposable4 = ctx.events.onReconnect((snapshot) => {
  ctx.ui.showToast({
    title: 'Reconnected',
    description: `${snapshot.host} is back`,
    variant: 'success',
  });
});
```

### 10.2 Node / Session State Tracking

```javascript
ctx.sessions.onTreeChange((tree) => {
  console.log('Session tree updated, node count:', tree.length);
});

const activeNodes = ctx.sessions.getActiveNodes();

activeNodes.forEach(({ nodeId }) => {
  ctx.sessions.onNodeStateChange(nodeId, (state) => {
    console.log(`Node ${nodeId} changed state to ${state}`);
  });
});
```

If you need to observe node additions, removals, or connection-state changes, build on the tree snapshots and node-state subscriptions exposed by `ctx.sessions` rather than relying on non-public internal event names.

### 10.3 Inter-Plugin Communication

```javascript
// Plugin A: emit an event
ctx.events.emit('data-ready', { items: [...] });

// Plugin A: listen to its own event
ctx.events.on('data-ready', (data) => {
  console.log('Received:', data.items.length);
});
```

**Namespacing rules**:

- `ctx.events.emit('foo', data)` actually emits `plugin:{pluginId}:foo`
- `ctx.events.on('foo', handler)` actually listens to `plugin:{pluginId}:foo`
- `emit` and `on` inside the same plugin automatically match each other

> Cross-plugin communication: in the current API design, every plugin's `on` and `emit` automatically prepend that plugin's own namespace. That means a plugin can only listen to its own events by default. Cross-plugin communication requires another mechanism, such as a shared store or an agreed event name through the lower-level bridge.

### 10.4 ConnectionSnapshot Structure

All connection-event handlers receive an **immutable** `ConnectionSnapshot` object:

```typescript
type ConnectionSnapshot = Readonly<{
  id: string;
  host: string;
  port: number;
  username: string;
  state: SshConnectionState;
  refCount: number;
  keepAlive: boolean;
  createdAt: string;
  lastActive: string;
  terminalIds: readonly string[];
  parentConnectionId?: string;
}>;
```

Possible values for **SshConnectionState**:

```typescript
type SshConnectionState =
  | 'idle'
  | 'connecting'
  | 'active'
  | 'disconnecting'
  | 'disconnected'
  | 'reconnecting'
  | 'link_down'
  | { error: string };
```

### 10.5 Transfer Events (v3)

v3 adds SFTP transfer-related events, exposed through the `ctx.transfers` API:

| Event method | Trigger condition |
|----------|---------|
| `transfers.onProgress(handler)` | Transfer progress updates, throttled to 500ms |
| `transfers.onComplete(handler)` | Transfer completes |
| `transfers.onError(handler)` | Transfer fails |

All handlers receive a `TransferSnapshot` object; see [6.14](#614-ctxtransfers-v3).

```javascript
ctx.transfers.onProgress((t) => {
  const pct = ((t.transferred / t.size) * 100).toFixed(1);
  console.log(`[${t.direction}] ${t.name}: ${pct}%`);
});

ctx.transfers.onComplete((t) => {
  const duration = ((t.endTime - t.startTime) / 1000).toFixed(1);
  console.log(`Done: ${t.name} in ${duration}s`);
});

ctx.transfers.onError((t) => {
  console.error(`Failed: ${t.name} — ${t.error}`);
});
```

---

## 11. Internationalization (i18n)

### 11.1 Plugin i18n Overview

OxideTerm uses **i18next** as its i18n framework. Plugin translation resources are loaded into the main i18next instance through `loadPluginI18n()`, under the namespace `plugin.{pluginId}.*`.

### 11.2 Directory Structure

```
your-plugin/
├── plugin.json           <- "locales": "./locales"
└── locales/
    ├── en.json           <- English (strongly recommended)
    ├── zh-CN.json        <- Simplified Chinese
    ├── zh-TW.json        <- Traditional Chinese
    ├── ja.json           <- Japanese
    ├── ko.json           <- Korean
    ├── de.json           <- German
    ├── es-ES.json        <- Spanish
    ├── fr-FR.json        <- French
    ├── it.json           <- Italian
    ├── pt-BR.json        <- Portuguese (Brazil)
    └── vi.json           <- Vietnamese
```

**Translation file format** (flat KV):

```json
{
  "dashboard_title": "Plugin Dashboard",
  "greeting": "Hello, {{name}}!",
  "item_count": "{{count}} items",
  "settings_saved": "Settings saved successfully"
}
```

### 11.3 Using Translations

```javascript
const title = ctx.i18n.t('dashboard_title');
const greeting = ctx.i18n.t('greeting', { name: 'Alice' });

ctx.i18n.onLanguageChange((lang) => {
  console.log('Language changed to:', lang);
  // Trigger a UI update
});
```

### 11.4 Supported Languages

OxideTerm attempts to load language files in the following order. Missing files are skipped silently.

| Language Code | Language |
|----------|------|
| `en` | English |
| `zh-CN` | Simplified Chinese |
| `zh-TW` | Traditional Chinese |
| `ja` | Japanese |
| `ko` | Korean |
| `de` | German |
| `es-ES` | Spanish |
| `fr-FR` | French |
| `it` | Italian |
| `pt-BR` | Portuguese (Brazil) |
| `vi` | Vietnamese |

---

## 12. Persistent Storage

### 12.1 KV Storage (`ctx.storage`)

Simple `localStorage`-based KV storage with automatic JSON serialization and deserialization.

```javascript
ctx.storage.set('myData', { items: [1, 2, 3], updated: Date.now() });

const data = ctx.storage.get('myData');

ctx.storage.remove('myData');
```

**Storage key format**: `oxide-plugin-{pluginId}-{key}`

**Limits**:
- constrained by `localStorage` capacity, usually 5-10 MB per origin
- failures are handled silently without throwing
- all values are serialized as JSON, so `undefined`, `function`, and `Symbol` are not supported

### 12.2 Settings Storage (`ctx.settings`)

Similar to `ctx.storage`, but with additional features:

- settings declared in the manifest have `default` values
- supports `onChange` listeners
- uses the storage key format `oxide-plugin-{pluginId}-setting-{settingId}`

### 12.3 Storage Isolation

Each plugin's storage is fully isolated:

```
localStorage key format:
  oxide-plugin-{pluginId}-{key}               <- storage
  oxide-plugin-{pluginId}-setting-{settingId} <- settings
```

Storage is **not cleared automatically** when a plugin is uninstalled. Data remains so the plugin can be reinstalled later. If you need a full wipe, call the internal `clearPluginStorage(pluginId)` helper, which is not currently exposed through `ctx`.

---

## 13. Backend API Invocation

### 13.1 Whitelist Mechanism

Plugins may call only the Tauri commands declared in `contributes.apiCommands`.

```json
{
  "contributes": {
    "apiCommands": ["list_sessions", "get_session_info"]
  }
}
```

### 13.2 Declaring and Using Commands

```javascript
try {
  const sessions = await ctx.api.invoke('list_sessions');
  console.log('Active sessions:', sessions);
} catch (err) {
  console.error('Failed to list sessions:', err);
}
```

### 13.3 Security Limits

:::caution[Advisory Whitelist]
The current whitelist is **advisory**, not a hard sandbox, because:

1. plugins run in the same JS context as the host
2. a plugin can theoretically bypass checks by directly importing `@tauri-apps/api/core`
3. the whitelist primarily helps code review catch accidental or malicious command usage

**Whitelist enforcement**:
- when a command is not declared:
  - the host emits `console.warn()`
  - it throws `Error: Command "xxx" not whitelisted...`
- the command is never actually invoked
:::

---

## 14. Circuit Breaker and Error Handling

### 14.1 Circuit Breaker Mechanism

OxideTerm's plugin system includes a built-in circuit breaker that prevents broken plugins from dragging down the entire application.

| Parameter | Value | Description |
|------|-----|------|
| `MAX_ERRORS` | 10 | Trigger threshold |
| `ERROR_WINDOW_MS` | 60,000 ms (1 minute) | Sliding window |
| `HOOK_BUDGET_MS` | 5 ms | Terminal hook time budget |

**Errors counted by the circuit breaker**:

1. exceptions thrown by terminal hooks (`inputInterceptor` / `outputProcessor`)
2. terminal hooks taking longer than 5ms
3. other runtime errors tracked through `trackPluginError()`

**Trigger flow**:

```
Plugin error
  -> trackPluginError(pluginId)
    -> accumulate errors within the 60s window
      -> reach 10 errors
        -> persistAutoDisable(pluginId)
          -> plugin-config.json: { enabled: false }
          -> store.setPluginState('disabled')
        -> unloadPlugin(pluginId)
```

### 14.2 Error Handling Best Practices

```javascript
// Defensive programming inside Terminal hooks
ctx.terminal.registerInputInterceptor((data, { sessionId }) => {
  try {
    return processInput(data);
  } catch (err) {
    console.warn('[MyPlugin] Input interceptor error:', err);
    return data;
  }
});

// Wrap event handlers with try/catch
ctx.events.onConnect((snapshot) => {
  try {
    handleConnection(snapshot);
  } catch (err) {
    console.error('[MyPlugin] onConnect error:', err);
  }
});

// Wrap API calls with try/catch
try {
  const result = await ctx.api.invoke('some_command');
} catch (err) {
  ctx.ui.showToast({
    title: 'API Error',
    description: String(err),
    variant: 'error',
  });
}
```

### 14.3 Persistent Auto-Disable

When the circuit breaker trips:

1. it reads `plugin-config.json`
2. sets `plugins[pluginId].enabled = false`
3. writes the file back
4. sets the store state to `'disabled'`

That means the plugin **remains disabled after restarting OxideTerm**. The user must re-enable it manually in Plugin Manager.

---

## 15. Disposable Pattern

### 15.1 Overview

All `register*` and `on*` methods return a `Disposable` object:

```typescript
type Disposable = {
  dispose(): void;  // becomes a no-op after the first call
};
```

### 15.2 Manual Disposal

If you need to dynamically unregister something at runtime, for example toggling a hook based on a setting:

```javascript
let interceptorDisposable = null;

function enableInterceptor() {
  interceptorDisposable = ctx.terminal.registerInputInterceptor(myHandler);
}

function disableInterceptor() {
  interceptorDisposable?.dispose();
  interceptorDisposable = null;
}

ctx.settings.onChange('enableFilter', (enabled) => {
  if (enabled) enableInterceptor();
  else disableInterceptor();
});
```

### 15.3 Automatic Cleanup

You do **not** need to manually clean up anything registered through `ctx` inside `deactivate()`. On unload, the system automatically:

1. walks all tracked `Disposable`s for the plugin
2. calls `dispose()` on each one
3. clears tab views, sidebar panels, input interceptors, output processors, and shortcuts
4. clears the disposable tracking list itself

`deactivate()` is meant for cleanup that is outside the `Disposable` model, such as global references placed on `window`.

---

## 16. Complete Example: Demo Plugin

OxideTerm ships with a complete Demo Plugin that serves as a reference implementation.

### 16.1 Directory Structure

```
~/.oxideterm/plugins/oxide-demo-plugin/
├── plugin.json
└── main.js
```

### 16.2 plugin.json

```json
{
  "id": "oxide-demo-plugin",
  "name": "OxideTerm Demo Plugin",
  "version": "1.0.0",
  "description": "A comprehensive demo plugin that exercises all plugin system APIs",
  "author": "OxideTerm Team",
  "main": "./main.js",
  "engines": {
    "oxideterm": ">=1.6.0"
  },
  "contributes": {
    "tabs": [
      { "id": "dashboard", "title": "Plugin Dashboard", "icon": "LayoutDashboard" }
    ],
    "sidebarPanels": [
      { "id": "quick-info", "title": "Quick Info", "icon": "Info", "position": "bottom" }
    ],
    "settings": [
      {
        "id": "greeting", "type": "string", "default": "Hello from Plugin!",
        "title": "Greeting Message", "description": "The greeting shown in the dashboard"
      },
      {
        "id": "inputPrefix", "type": "string", "default": "",
        "title": "Input Prefix", "description": "If set, prefix all terminal input"
      },
      {
        "id": "logOutput", "type": "boolean", "default": false,
        "title": "Log Output", "description": "Log terminal output byte counts to console"
      }
    ],
    "terminalHooks": {
      "inputInterceptor": true,
      "outputProcessor": true,
      "shortcuts": [
        { "key": "ctrl+shift+d", "command": "openDashboard" }
      ]
    },
    "connectionHooks": ["onConnect", "onDisconnect"]
  }
}
```

### 16.3 main.js Walkthrough

The Demo Plugin's `main.js` demonstrates how to use the core APIs.

**1. Get shared modules, including the UI Kit**

```javascript
const { React, ReactDOM, zustand, lucideReact, ui } = window.__OXIDE__;
const { createElement: h, useState, useEffect, useCallback, useRef } = React;
const { Activity, Wifi, Terminal, Settings } = lucideReact;
```

**2. Create a shared Zustand store**

```javascript
const useDemoStore = zustand.create((set) => ({
  eventLog: [],
  inputCount: 0,
  outputBytes: 0,
  connectionCount: 0,
  addEvent: (msg) => set((s) => ({
    eventLog: [...s.eventLog.slice(-49), { time: new Date().toLocaleTimeString(), msg }],
  })),
  incInput: () => set((s) => ({ inputCount: s.inputCount + 1 })),
  addOutputBytes: (n) => set((s) => ({ outputBytes: s.outputBytes + n })),
  setConnectionCount: (n) => set({ connectionCount: n }),
}));
```

**3. Tab component** — build the interface with `ui.*` components and read `connections`, `settings`, and `storage` through a captured `ctx` reference.

**4. Full registration inside `activate()`**

```javascript
export function activate(ctx) {
  window.__DEMO_PLUGIN_CTX__ = ctx;

  // UI registration
  ctx.ui.registerTabView('dashboard', DashboardTab);
  ctx.ui.registerSidebarPanel('quick-info', QuickInfoPanel);

  // Terminal hooks
  ctx.terminal.registerInputInterceptor((data, { sessionId }) => { /* ... */ });
  ctx.terminal.registerOutputProcessor((data, { sessionId }) => { /* ... */ });
  ctx.terminal.registerShortcut('openDashboard', () => ctx.ui.openTab('dashboard'));

  // Events
  ctx.events.onConnect((snapshot) => { /* ... */ });
  ctx.events.onDisconnect((data) => { /* ... */ });
  ctx.events.on('demo-ping', (data) => { /* ... */ });

  // Settings watch
  ctx.settings.onChange('greeting', (newVal) => { /* ... */ });

  // Storage
  const count = (ctx.storage.get('launchCount') || 0) + 1;
  ctx.storage.set('launchCount', count);

  // Toast
  ctx.ui.showToast({ title: 'Demo Plugin Activated', variant: 'success' });
}
```

**5. Cleanup in `deactivate()`**

```javascript
export function deactivate() {
  delete window.__DEMO_PLUGIN_CTX__;
}
```

---

## 17. Best Practices

### Development Rules

1. **Always use shared modules from `window.__OXIDE__`**
   - do not bundle your own copy of React
   - use `const { React } = window.__OXIDE__`

2. **Respect Manifest declarations**
   - every tab, panel, hook, shortcut, and API command must be declared first in `plugin.json`
   - registering undeclared content throws at runtime

3. **Keep `activate()` lightweight**
   - do not perform heavy computation or long network requests inside `activate()`
   - it has a 5-second timeout

4. **Keep Terminal Hooks extremely efficient**
   - they run on every keystroke or output chunk and must finish within 5ms
   - move heavy work to `queueMicrotask()` or `setTimeout()`
   - wrap them defensively in try/catch

5. **Use semantic CSS classes**
   - prefer semantic Tailwind classes like `text-foreground`, `bg-card`, and `border-border`
   - do not hard-code color values

6. **Clean up global state**
   - delete globals such as `window.__MY_GLOBAL__` in `deactivate()`
   - anything managed through `Disposable` does not need manual cleanup

### Performance Advice

1. **Cap event log size** to avoid memory leaks:

```javascript
eventLog: [...s.eventLog.slice(-49), newEntry]
```

2. **Avoid string decoding inside output processors**:

```javascript
// Bad
const text = new TextDecoder().decode(data);
const processed = text.replace(/pattern/, 'replacement');
return new TextEncoder().encode(processed);

// Good
totalBytes += data.length;
return data;
```

3. **Delay initialization**: use `useEffect` inside components to load data lazily.

### Security Advice

1. declare only the `apiCommands` you actually need
2. do not expose sensitive information on `window`
3. do not directly import `@tauri-apps/api/core`, even though it is technically possible
4. do not store passwords or private keys in `ctx.storage`, because `localStorage` is not encrypted

### v3 API Advice

1. **Snapshots are immutable**: all v3 snapshots such as `TransferSnapshot` and `ProfilerMetricsSnapshot` are frozen at runtime. Do not mutate them. Create new objects if you need derived data.
2. **Throttled events still require lightweight handlers**: `transfers.onProgress` is throttled to 500ms and `profiler.onMetrics` to 1s, but handlers should still avoid DOM-heavy or computationally expensive work.
3. **Use namespaces on demand**: v3 exposes 19 namespaces, but you only need to use the ones your plugin actually depends on.
4. **Respect Disposable lifecycles**: v3 subscriptions such as `onTreeChange`, `onProgress`, and `onMetrics` return `Disposable`s. Clean them up when appropriate, or let the framework manage them when they are registered directly from `ctx`.
5. **Treat AI data as sensitive**: `ctx.ai.getMessages()` can contain terminal buffer content, so do not log it or send it to external services casually.

---

## 18. Debugging Tips

### Built-in Log Viewer in Plugin Manager

Plugin Manager includes a per-plugin log panel. When a plugin has logs, the plugin row shows a 📜 button that opens the log panel.

The log system automatically records:
- **info**: successful activation and unload
- **error**: load failures, including the concrete reason and suggested fixes, plus circuit breaker trips

Each plugin keeps at most **200 log entries**. Use the **Clear** button in the log panel to remove them.

**Common error messages and what they mean**:

| Error message | Meaning | How to fix |
|----------|------|----------|
| `activate() must resolve within 5s` | Activation timed out | Move expensive work into `setTimeout` or `queueMicrotask` |
| `ensure your main.js exports an activate() function` | Missing activation export | Make sure `export function activate(ctx)` exists |
| `check that main.js is a valid ES module bundle` | JS syntax or import error | Check syntax and ensure the file is valid ESM |

### DevTools Console

All plugin `console.log`, `console.warn`, and `console.error` output appears in DevTools. Internal host logs use prefixes such as `[PluginLoader]`, `[PluginEventBridge]`, and `[PluginTerminalHooks]`.

**Useful debugging commands**:

```javascript
// In DevTools Console

// Show all loaded plugins
JSON.stringify([...window.__ZUSTAND_PLUGIN_STORE__?.getState?.()?.plugins?.entries?.()] ?? 'store not found');

// Show plugin store state if you exposed it globally
useDemoStore.getState()

// Trigger a toast manually
window.__DEMO_PLUGIN_CTX__?.ui.showToast({ title: 'Test', variant: 'success' });

// Inspect current connections
window.__DEMO_PLUGIN_CTX__?.connections.getAll();
```

### Plugin Manager

- **Status Badge** shows `active`, `error`, or `disabled`
- **Error Message** shows detailed load/runtime failure information
- **Reload** hot-reloads the plugin by unloading and loading it again
- **Refresh** rescans disk to discover new plugins or remove missing ones

### Troubleshooting Common Issues

| Symptom | Possible cause |
|------|----------|
| Load failure: `module must export "activate"` | The entry file does not export `activate()` |
| Load failure: `timed out after 5000ms` | `activate()` contains a Promise that never resolves |
| Tab does not appear | You forgot to call `ctx.ui.registerTabView()` in `activate()` |
| Hooks do not work | `terminalHooks.inputInterceptor: true` was not declared in the Manifest |
| Toast does not appear | Check the `variant` spelling: `default`, `success`, `error`, or `warning` |
| Shortcut does not work | Make sure the terminal pane has focus |
| Reading a setting returns `undefined` | Make sure the setting key matches `settings[].id` in the Manifest |
| The plugin was auto-disabled | The circuit breaker was triggered; inspect the Plugin Manager log viewer or DevTools for errors and timeout warnings |
| Styles look wrong or fight the theme | You used hard-coded colors instead of semantic classes |

---

## 19. Frequently Asked Questions (FAQ)

### Q: Can plugins use TypeScript?

Yes. OxideTerm provides a standalone type definition file, `plugin-api.d.ts`, so you can get full IntelliSense support without cloning the full OxideTerm source tree.

**Step 1: get the type definitions**

Copy `plugin-development/plugin-api.d.ts` from the OxideTerm repository into your plugin project.

**Step 2: configure `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "outDir": ".",
    "strict": true
  },
  "include": ["plugin-api.d.ts", "src/**/*.ts"]
}
```

**Step 3: write a typed plugin**

```typescript
import type { PluginContext } from '../plugin-api';

export function activate(ctx: PluginContext) {
  ctx.ui.showToast({ title: 'Hello!', variant: 'success' });
  ctx.events.onConnect((snapshot) => {
    console.log(`Connected to ${snapshot.host}`);
  });
}
```

**Step 4: compile to ESM**

```bash
# esbuild (recommended)
npx esbuild src/main.ts --bundle --format=esm --outfile=main.js --external:react

# or tsc
npx tsc
```

Do not bundle React. Get it from `window.__OXIDE__` at runtime.

### Q: Can a plugin have multiple files?

- **v1 single-file plugins** loaded through Blob URLs do not support internal relative imports. Use a bundler such as esbuild or rollup to collapse the plugin into one file.
- **v2 package plugins** loaded through the local HTTP server support multi-file layouts and standard relative `import` statements.

For v1 plugins, recommended options are:

1. bundle everything into one file with esbuild or rollup
2. keep all code in a single `main.js`

```bash
npx esbuild src/index.ts \
  --bundle \
  --format=esm \
  --outfile=main.js \
  --external:react \
  --external:react-dom
```

### Q: Can plugins access the filesystem?

Not directly. Plugins can only:
- call declared backend commands through `ctx.api.invoke()`
- use `ctx.storage` on top of `localStorage`

### Q: Can plugins send network requests?

Yes, but there are two distinct cases:

1. for ordinary JSON APIs, use the browser's native `fetch()` directly
2. for WebDAV, S3-compatible object storage, Dropbox, or other binary-heavy requests that often run into WebView CORS restrictions, declare `plugin_http_request` and route the request through the host Rust backend via `ctx.api.invoke()`

`plugin_http_request` allows only HTTP/HTTPS URLs. Its request body is passed as `bodyBase64`, and the response returns `{ status, headers, bodyBase64 }`. This is usually more reliable than direct plugin-side `fetch()`, especially for sync plugins.

### Q: How do I use JSX in a plugin?

By default, plugins are plain JS and should use `React.createElement`. If you want JSX:

1. use esbuild with `--jsx=automatic --jsx-import-source=react`
2. or use Babel with `@babel/plugin-transform-react-jsx`
3. mark React as external and get it from `window.__OXIDE__` at runtime

### Q: Can plugins communicate with each other?

In the current design, `ctx.events.on()` and `ctx.events.emit()` are namespace-isolated. Options for cross-plugin communication include:

1. shared globals such as `window.__SHARED_DATA__`
2. the lower-level event bridge, if you understand the internal API well enough
3. a future dedicated cross-plugin communication channel, which is still only planned

### Q: What should I do if my plugin was auto-disabled?

1. click the plugin's 📜 icon in Plugin Manager to inspect the logs and identify the concrete error and suggested fix
2. also inspect DevTools for errors and timeout warnings
3. fix the underlying performance or correctness issue
4. re-enable the plugin in Plugin Manager
5. or edit `~/.oxideterm/plugin-config.json` manually:

```json
{
  "plugins": {
    "your-plugin-id": {
      "enabled": true
    }
  }
}
```

### Q: Can plugins modify OxideTerm's interface?

Through the declarative API, plugins can:
- add Tab views
- add Sidebar panels
- show toast notifications and confirmations
- register context menu items through `ctx.ui.registerContextMenu`
- register status bar items through `ctx.ui.registerStatusBarItem`
- register keybindings through `ctx.ui.registerKeybinding`
- show notifications through `ctx.ui.showNotification`
- show progress indicators through `ctx.ui.showProgress`

They cannot:
- modify existing host UI components
- modify menus or toolbars directly

> Note: plugins may inject custom CSS through `ctx.assets.loadCSS()` or the Manifest `styles` field.

### Q: Where are plugin configuration files stored?

| File / location | Description |
|-----------|------|
| `~/.oxideterm/plugins/{id}/plugin.json` | Plugin Manifest |
| `~/.oxideterm/plugins/{id}/main.js` | Plugin code |
| `~/.oxideterm/plugin-config.json` | Global plugin enable/disable state |
| `localStorage: oxide-plugin-{id}-*` | Plugin storage data |
| `localStorage: oxide-plugin-{id}-setting-*` | Plugin setting values |

---

## 20. Type Reference (TypeScript)

> Recommended: use `plugin-development/plugin-api.d.ts` directly. It is a standalone, zero-dependency type definition file that you can copy into your plugin project for IntelliSense. See [FAQ: Can plugins use TypeScript?](#q-can-plugins-use-typescript)

Below is a practical TypeScript reference excerpt aligned with the current documentation:

```typescript
// oxideterm-plugin.d.ts

export type Disposable = {
  dispose(): void;
};

export type SshConnectionState =
  | 'idle'
  | 'connecting'
  | 'active'
  | 'disconnecting'
  | 'disconnected'
  | 'reconnecting'
  | 'link_down'
  | { error: string };

export type ConnectionSnapshot = Readonly<{
  id: string;
  host: string;
  port: number;
  username: string;
  state: SshConnectionState;
  refCount: number;
  keepAlive: boolean;
  createdAt: string;
  lastActive: string;
  terminalIds: readonly string[];
  parentConnectionId?: string;
}>;

export type PluginTabProps = {
  tabId: string;
  pluginId: string;
};

export type PluginEventsAPI = {
  onConnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onDisconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onLinkDown(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  onReconnect(handler: (snapshot: ConnectionSnapshot) => void): Disposable;
  on(name: string, handler: (data: unknown) => void): Disposable;
  emit(name: string, data: unknown): void;
};

export type ContextMenuTarget = 'terminal' | 'sftp' | 'tab' | 'sidebar';

export type ContextMenuItem = {
  label: string;
  icon?: string;
  handler: () => void;
  when?: () => boolean;
};

export type StatusBarItemOptions = {
  text: string;
  icon?: string;
  tooltip?: string;
  alignment: 'left' | 'right';
  priority?: number;
  onClick?: () => void;
};

export type StatusBarHandle = {
  update(options: Partial<StatusBarItemOptions>): void;
  dispose(): void;
};

export type ProgressReporter = {
  report(value: number, total: number, message?: string): void;
};

export type PluginUIAPI = {
  registerTabView(tabId: string, component: React.ComponentType<PluginTabProps>): Disposable;
  registerSidebarPanel(panelId: string, component: React.ComponentType): Disposable;
  registerCommand(id: string, opts: { label: string; icon?: string; shortcut?: string; section?: string }, handler: () => void): Disposable;
  openTab(tabId: string): void;
  showToast(opts: { title: string; description?: string; variant?: 'default' | 'success' | 'error' | 'warning' }): void;
  showConfirm(opts: { title: string; description: string }): Promise<boolean>;
  registerContextMenu(target: ContextMenuTarget, items: ContextMenuItem[]): Disposable;
  registerStatusBarItem(options: StatusBarItemOptions): StatusBarHandle;
  registerKeybinding(keybinding: string, handler: () => void): Disposable;
  showNotification(opts: { title: string; body?: string; severity?: 'info' | 'warning' | 'error' }): void;
  showProgress(title: string): ProgressReporter;
  getLayout(): Readonly<{ sidebarCollapsed: boolean; activeTabId: string | null; tabCount: number }>;
  onLayoutChange(handler: (layout: Readonly<{ sidebarCollapsed: boolean; activeTabId: string | null; tabCount: number }>) => void): Disposable;
};

export type PluginActiveTerminalTarget = Readonly<{
  sessionId: string;
  terminalType: 'terminal' | 'local_terminal';
  nodeId: string | null;
  connectionId: string | null;
  connectionState: string | null;
  label: string | null;
}>;

export type TerminalHookContext = {
  sessionId: string;
  nodeId: string;
};

export type InputInterceptor = (data: string, context: TerminalHookContext) => string | null;
export type OutputProcessor = (data: Uint8Array, context: TerminalHookContext) => Uint8Array;

export type PluginTerminalAPI = {
  registerInputInterceptor(handler: InputInterceptor): Disposable;
  registerOutputProcessor(handler: OutputProcessor): Disposable;
  registerShortcut(command: string, handler: () => void): Disposable;
  getActiveTarget(): PluginActiveTerminalTarget | null;
  writeToActive(text: string): boolean;
  writeToNode(nodeId: string, text: string): void;
  getNodeBuffer(nodeId: string): string | null;
  getNodeSelection(nodeId: string): string | null;
  search(nodeId: string, query: string, options?: { caseSensitive?: boolean; regex?: boolean; wholeWord?: boolean }): Promise<Readonly<{ matches: ReadonlyArray<unknown>; total_matches: number }>>;
  getScrollBuffer(nodeId: string, startLine: number, count: number): Promise<ReadonlyArray<Readonly<{ text: string; lineNumber: number }>>>;
  getBufferSize(nodeId: string): Promise<Readonly<{ currentLines: number; totalLines: number; maxLines: number }>>;
  clearBuffer(nodeId: string): Promise<void>;
};

export type PluginSettingsAPI = {
  get<T>(key: string): T;
  set<T>(key: string, value: T): void;
  onChange(key: string, handler: (newValue: unknown) => void): Disposable;
};

export type PluginI18nAPI = {
  t(key: string, params?: Record<string, string | number>): string;
  getLanguage(): string;
  onLanguageChange(handler: (lang: string) => void): Disposable;
};

export type PluginStorageAPI = {
  get<T>(key: string): T | null;
  set<T>(key: string, value: T): void;
  remove(key: string): void;
};

export type PluginBackendAPI = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
};

export type PluginAssetsAPI = {
  loadCSS(relativePath: string): Promise<Disposable>;
  getAssetUrl(relativePath: string): Promise<string>;
  revokeAssetUrl(url: string): void;
};

export type PluginContext = Readonly<{
  pluginId: string;
  connections: PluginConnectionsAPI;
  events: PluginEventsAPI;
  ui: PluginUIAPI;
  terminal: PluginTerminalAPI;
  settings: PluginSettingsAPI;
  i18n: PluginI18nAPI;
  storage: PluginStorageAPI;
  api: PluginBackendAPI;
  assets: PluginAssetsAPI;
  sftp: PluginSftpAPI;
  forward: PluginForwardAPI;
  sessions: PluginSessionsAPI;
  transfers: PluginTransfersAPI;
  profiler: PluginProfilerAPI;
  eventLog: PluginEventLogAPI;
  ide: PluginIdeAPI;
  ai: PluginAiAPI;
  app: PluginAppAPI;
}>;
```

---

## Appendix A: Complete Manifest JSON Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["id", "name", "version", "main"],
  "properties": {
    "id": {
      "type": "string",
      "pattern": "^[a-zA-Z0-9][a-zA-Z0-9_-]*$",
      "description": "Unique plugin identifier"
    },
    "name": { "type": "string", "description": "Human-readable plugin name" },
    "version": { "type": "string", "pattern": "^\\d+\\.\\d+\\.\\d+", "description": "Semver version" },
    "description": { "type": "string" },
    "author": { "type": "string" },
    "main": { "type": "string", "description": "Relative path to ESM entry file" },
    "manifestVersion": {
      "type": "integer", "enum": [1, 2], "default": 1,
      "description": "Manifest schema version; set to 2 for v2 Package format"
    },
    "format": {
      "type": "string", "enum": ["bundled", "package"], "default": "bundled",
      "description": "bundled = single-file Blob URL; package = multi-file HTTP Server"
    },
    "assets": {
      "type": "string",
      "description": "Relative path to the assets directory (v2 Package only)"
    },
    "styles": {
      "type": "array", "items": { "type": "string" },
      "description": "CSS files to auto-load on activation (v2 Package only)"
    },
    "sharedDependencies": {
      "type": "object",
      "additionalProperties": { "type": "string" },
      "description": "Dependencies provided by the host through window.__OXIDE__"
    },
    "repository": {
      "type": "string",
      "description": "Repository URL for source code"
    },
    "checksum": {
      "type": "string",
      "description": "SHA-256 hash of the main entry file for integrity verification"
    },
    "engines": {
      "type": "object",
      "properties": {
        "oxideterm": { "type": "string", "pattern": "^>=?\\d+\\.\\d+\\.\\d+" }
      }
    },
    "locales": { "type": "string", "description": "Relative path to the locales directory" },
    "contributes": {
      "type": "object",
      "properties": {
        "tabs": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["id", "title", "icon"],
            "properties": {
              "id": { "type": "string" },
              "title": { "type": "string" },
              "icon": { "type": "string", "description": "Lucide React icon name" }
            }
          }
        },
        "sidebarPanels": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["id", "title", "icon"],
            "properties": {
              "id": { "type": "string" },
              "title": { "type": "string" },
              "icon": { "type": "string" },
              "position": { "type": "string", "enum": ["top", "bottom"], "default": "bottom" }
            }
          }
        },
        "settings": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["id", "type", "default", "title"],
            "properties": {
              "id": { "type": "string" },
              "type": { "type": "string", "enum": ["string", "number", "boolean", "select"] },
              "default": {},
              "title": { "type": "string" },
              "description": { "type": "string" },
              "options": {
                "type": "array",
                "items": {
                  "type": "object",
                  "required": ["label", "value"],
                  "properties": {
                    "label": { "type": "string" },
                    "value": {}
                  }
                }
              }
            }
          }
        },
        "terminalHooks": {
          "type": "object",
          "properties": {
            "inputInterceptor": { "type": "boolean" },
            "outputProcessor": { "type": "boolean" },
            "shortcuts": {
              "type": "array",
              "items": {
                "type": "object",
                "required": ["key", "command"],
                "properties": {
                  "key": { "type": "string" },
                  "command": { "type": "string" }
                }
              }
            }
          }
        },
        "terminalTransports": {
          "type": "array",
          "items": { "type": "string", "enum": ["telnet"] }
        },
        "connectionHooks": {
          "type": "array",
          "items": { "type": "string", "enum": ["onConnect", "onDisconnect", "onReconnect", "onLinkDown"] }
        },
        "apiCommands": {
          "type": "array",
          "items": { "type": "string" }
        }
      }
    }
  }
}
```

---

## Appendix B: Internal Architecture File Quick Reference

| File | Responsibility |
|------|------|
| `src/types/plugin.ts` | All plugin type definitions |
| `src/store/pluginStore.ts` | Zustand plugin state management |
| `src/lib/plugin/pluginLoader.ts` | Lifecycle management: discovery, loading, unloading, circuit breaker |
| `src/lib/plugin/pluginContextFactory.ts` | Builds the frozen PluginContext membrane |
| `src/lib/plugin/pluginEventBridge.ts` | Event bridge from appStore to plugin events |
| `src/lib/plugin/pluginTerminalHooks.ts` | Terminal I/O hook pipeline |
| `src/lib/plugin/pluginStorage.ts` | localStorage KV wrapper |
| `src/lib/plugin/pluginSettingsManager.ts` | Setting declarations, persistence, and change notifications |
| `src/lib/plugin/pluginI18nManager.ts` | Plugin i18n wrapper around i18next |
| `src/lib/plugin/pluginUtils.ts` | Shared utilities such as path validation and safety checks |
| `src/lib/plugin/pluginUIKit.tsx` | Built-in UI Kit component library |
| `src-tauri/src/commands/plugin.rs` | Rust backend for file I/O and path safety |
| `src-tauri/src/commands/plugin_server.rs` | Plugin file server for multi-file HTTP loading |

| `src/components/plugin/PluginManagerView.tsx` | Plugin Manager UI |
| `src/components/plugin/PluginTabRenderer.tsx` | Plugin Tab renderer |
| `src/components/plugin/PluginSidebarRenderer.tsx` | Plugin Sidebar renderer |
| `src/components/plugin/PluginConfirmDialog.tsx` | Themed confirmation dialog |
| `src/lib/plugin/pluginSnapshots.ts` | v3 snapshot generation factory with freeze + deep copy |
| `src/lib/plugin/pluginThrottledEvents.ts` | v3 throttled event bridges for transfers and profiler |
