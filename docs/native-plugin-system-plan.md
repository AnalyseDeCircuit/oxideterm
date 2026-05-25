# OxideTerm Native Plugin System Plan

This document is the executable plan for bringing the Tauri plugin system into
the GPUI native application without bringing back WebView, DOM, React component
execution, or JavaScript evaluation.

The source of truth for the public plugin contract is:

- `/Users/dominical/Documents/oxideterm-main/plugin-api.d.ts`
- `/Users/dominical/Documents/oxideterm-main/plugin-development/plugin-api.d.ts`
- `/Users/dominical/Documents/oxideterm-main/src/types/plugin.ts`
- `/Users/dominical/Documents/oxideterm-main/src/store/pluginStore.ts`
- `/Users/dominical/Documents/oxideterm-main/src/lib/plugin/pluginLoader.ts`
- `/Users/dominical/Documents/oxideterm-main/src/lib/plugin/pluginContextFactory.ts`
- `/Users/dominical/Documents/oxideterm-main/src/components/plugin/PluginManagerView.tsx`
- `/Users/dominical/Documents/oxideterm-main/src-tauri/src/commands/plugin.rs`
- `/Users/dominical/Documents/oxideterm-main/src-tauri/src/commands/plugin_registry.rs`
- `/Users/dominical/Documents/oxideterm-main/src-tauri/src/commands/plugin_server.rs`

Tauri's public API shape should be preserved wherever it makes sense, but the
runtime implementation must be native:

```text
Tauri:
plugin.json -> ESM main.js -> dynamic import -> activate(ctx) -> React/UI hooks

Native:
plugin.json -> native runtime plan -> WASM/process/manifest-only host RPC
           -> native contribution registry -> GPUI-owned UI and host APIs
```

## Hard Rules

- Do not use WebView for the plugin runtime.
- Do not evaluate plugin JavaScript.
- Do not create a plugin-local HTTP module server in native.
- Do not import or copy implementation from Zed.
- Do not let plugins provide raw GPUI elements, HTML, CSS, React components, or
  arbitrary renderer code.
- Do not expose secrets, terminal input, filesystem writes, backend commands, or
  network forwarding without manifest-declared capability and host-side checks.
- Do not claim Tauri plugin parity until every namespace in `plugin-api.d.ts` is
  classified and the relevant verification gate passes.

## Current Native Baseline

Already present in native:

- Plugin Manager tab and sidebar entry.
- `.oxide` import/export support for plugin settings payloads.
- `workspace/plugin_host.rs` discovery scaffold:
  - scans `~/.oxideterm/plugins/*/plugin.json`;
  - parses Tauri-compatible manifest fields;
  - validates plugin ids and relative paths;
  - classifies `main.js` plugins as `UnsupportedLegacyJs`;
  - accepts explicit native `runtime.kind = wasm | process | manifest-only`.

This baseline is intentionally not a runtime. It is a registry foundation.

## API Classification Table

Every type/member in `plugin-api.d.ts` must be classified before implementation.

| Area | Tauri API | Native Strategy | First Milestone |
| --- | --- | --- | --- |
| Manifest | `PluginManifest`, `contributes` | Direct Rust manifest model | Phase 1 |
| Lifecycle | `activate`, `deactivate` | WASM/process RPC lifecycle | Phase 3 |
| Disposables | `dispose()` | Host-owned registration ids | Phase 3 |
| Connections | `ctx.connections.*` | Read-only snapshots from SSH registry/node runtime | Phase 5 |
| Events | `ctx.events.*` | Host event bus with plugin-scoped names | Phase 4 |
| UI commands | `registerCommand`, `registerKeybinding` | Native command/keybinding registry | Phase 4 |
| UI views | `registerTabView`, `registerSidebarPanel` | Declarative native UI schema, not components | Phase 7 |
| UI feedback | toast/confirm/notification/progress | Native workspace overlay/toast hosts | Phase 4 |
| Context menu | `registerContextMenu` | Native context menu item registry | Phase 4 |
| Status bar | `registerStatusBarItem` | Native status bar contribution registry | Phase 4 |
| Terminal hooks | input/output/shortcut | Host hook pipeline with timeout and ordering | Phase 5 |
| Terminal utilities | buffer/selection/search/write/telnet | NodeRouter/terminal registry adapters | Phase 5 |
| Settings | `ctx.settings.*` | Plugin-scoped typed settings store | Phase 4 |
| i18n | `ctx.i18n.*` | Plugin locale bundle loaded into native i18n facade | Phase 4 |
| Storage | `ctx.storage.*` | Plugin-scoped JSON KV store | Phase 4 |
| Sync | `ctx.sync.*` | Existing `.oxide` and Cloud Sync services | Phase 6 |
| Secrets | `ctx.secrets.*` | OS keychain, plugin-scoped account ids | Phase 6 |
| Backend invoke | `ctx.api.invoke` | Whitelisted host commands only | Phase 6 |
| Assets | `ctx.assets.*` | Safe binary asset read URLs or host image handles; no CSS injection | Phase 7 |
| SFTP | `ctx.sftp.*` | NodeRouter-backed SFTP sessions | Phase 6 |
| Forwarding | `ctx.forward.*` | Forwarding registry adapters | Phase 6 |
| Sessions | `ctx.sessions.*` | Node tree snapshots and subscriptions | Phase 5 |
| Transfers | `ctx.transfers.*` | SFTP transfer manager snapshots | Phase 6 |
| Profiler | `ctx.profiler.*` | Connection monitor/profiler registry snapshots | Phase 6 |
| Event log | `ctx.eventLog.*` | Native event log store | Phase 5 |
| IDE | `ctx.ide.*` | IDE state snapshots | Phase 6 |
| AI | `ctx.ai.*` | Read-only AI chat/provider snapshots | Phase 6 |
| App | `ctx.app.*` | Theme/settings/platform/layout snapshots | Phase 4 |
| Shared modules | `window.__OXIDE__` | Unsupported in native | Phase 0 |

## Phase 0: Parity Ledger

Status: [x] implemented.

Goal: create the tracking ledger before adding more runtime behavior.

Tracking artifact:

- `/Users/dominical/Documents/OxideTerm/docs/native-plugin-api-ledger.md`

Tasks:

- Add `docs/native-plugin-api-ledger.md`.
- Copy the top-level groups from `plugin-api.d.ts`.
- For each type and method, record:
  - source declaration line or section;
  - native owner module;
  - capability requirement;
  - implementation state;
  - test requirement;
  - Tauri behavior notes.
- Explicitly mark `window.__OXIDE__`, React component registration, CSS loading,
  and JavaScript ESM loading as native-incompatible.

Exit criteria:

- Every `PluginContext` namespace is represented in the ledger.
- Every Tauri-only browser primitive has an explicit native replacement or an
  explicit unsupported classification.

## Phase 1: Manifest, Config, And Registry

Status: [x] implemented.

Goal: make plugin discovery, validation, persistence, and manager display real.

Native owners:

- `crates/oxideterm-gpui-app/src/workspace/plugin_host.rs`
- `crates/oxideterm-gpui-app/src/workspace/plugin_manager.rs`
- future shared crate candidate: `crates/oxideterm-plugin-host`

Tasks:

- Complete `NativePluginManifest` parity with Tauri `PluginManifest`.
- Add `NativePluginConfig` equivalent to Tauri `PluginGlobalConfig`:
  - plugin id;
  - enabled flag;
  - auto-disabled flag;
  - last error;
  - install path;
  - runtime kind;
  - last loaded version;
  - error count/window metadata.
- Persist config at `~/.oxideterm/plugin-config.json`.
- Add registry states:
  - `discovered`;
  - `disabled`;
  - `unsupportedLegacyJs`;
  - `readyManifestOnly`;
  - `readyWasm`;
  - `readyProcess`;
  - `loading`;
  - `active`;
  - `error`;
  - `autoDisabled`.
- Keep legacy Tauri `main` plugins visible but not executable.
- Validate:
  - plugin id has no path separators, `..`, or control characters;
  - all manifest relative paths are non-empty and cannot escape install dir;
  - manifest required fields are present;
  - `runtime.entry` exists for executable native runtimes;
  - legacy `main` may exist but is not used for execution.
- Update Plugin Manager:
  - list discovered plugins;
  - show runtime kind;
  - show contribution counts;
  - show unsupported legacy JS state;
  - show disabled/error state;
  - do not expose enable for unsupported legacy JS.

Tests:

- `legacy main.js` manifest is discovered but classified as unsupported.
- `runtime.kind = wasm` becomes `readyWasm`.
- `runtime.kind = process` becomes `readyProcess`.
- path traversal is rejected.
- invalid manifest does not crash discovery.
- config round trip preserves disabled/error state.

Exit criteria:

- `cargo test -p oxideterm-gpui-app plugin_host`
- `cargo check -p oxideterm-gpui-app`
- Plugin Manager can inspect installed Tauri plugins without executing them.

## Phase 2: Manifest-Only Contributions

Status: [x] implemented.

Goal: make data-only plugins useful before runtime execution exists.

Tasks:

- Build a host-owned contribution store:
  - plugin commands metadata;
  - keybindings metadata;
  - settings definitions;
  - AI tool metadata;
  - sidebar/tab placeholders;
  - status bar placeholders;
  - terminal transport declarations;
  - connection hook declarations.
- Wire `contributes.settings` into a native Plugin Settings section:
  - setting types: string, number, boolean, select;
  - defaults from manifest;
  - persisted plugin-scoped values;
  - selectable text rendering and shared settings controls.
- Wire `contributes.aiTools` into the AI tool registry as metadata only:
  - name;
  - description;
  - parameters schema;
  - capabilities;
  - risk;
  - target kinds;
  - result schema.
- Wire command metadata into command palette as disabled or pending-runtime
  items until the runtime command handler exists.
- Add contribution cleanup by plugin id.

Tests:

- manifest-only settings render and persist.
- AI tool metadata appears in registry but cannot execute without runtime.
- disabling a plugin removes its contribution rows.
- malformed contribution definitions are rejected with plugin-scoped errors.

Exit criteria:

- A plugin with only `plugin.json` can contribute visible settings and metadata.
- No plugin code execution is needed for this phase.

## Phase 3: Runtime Protocol

Status: [x] implemented for the shared protocol, process runtime, bounded
WASIp1 activation, WASM memory ABI, and persistent dispatch.

Goal: define one native protocol used by both WASM and process runtimes.

Runtime message model:

```text
Host -> Plugin:
  Activate
  Deactivate
  HostEvent
  UiEvent
  SettingsChanged
  CancelRequest

Plugin -> Host:
  RegisterContribution
  DisposeContribution
  CallHostApi
  EmitEvent
  Log
  ReportProgress
  RuntimeReady
  RuntimeError
```

Transport requirements:

- Every request has a `request_id`.
- Every host call has timeout and cancellation.
- Every registration returns a host-owned registration id.
- All plugin errors are captured in plugin logs and cannot panic the workspace.
- Runtime shutdown disposes every registration for that plugin.
- Message schema should be serde-friendly and versioned.

Tasks:

- Add `PluginRuntimeBridge` trait:
  - `activate(manifest, permissions)`;
  - `deactivate()`;
  - `call(request)`;
  - `send_event(event)`;
  - `kill()`;
  - `health()`.
- Add a runtime supervisor:
  - state machine;
  - lifecycle timeout;
  - error circuit breaker;
  - log capture;
  - auto-disable after repeated errors.
- Add protocol types:
  - `PluginRequest`;
  - `PluginResponse`;
  - `PluginEvent`;
  - `PluginRegistration`;
  - `PluginError`.
- Start with process runtime because it is easiest to debug:
  - spawn executable under plugin dir;
  - stdio JSON lines or msgpack frames;
  - kill on unload;
  - reject paths outside plugin dir.
- Add WASM runtime after process protocol stabilizes:
  - [x] bounded WASIp1 `_start` activation through Wasmtime;
  - [x] no ambient filesystem/network unless granted;
  - [x] selected host-call ABI;
  - [x] deterministic host-call boundary for command/event dispatch.

Tests:

- activate/deactivate lifecycle succeeds.
- activate timeout moves plugin to error state.
- runtime crash auto-cleans contributions.
- unknown protocol version is rejected.
- repeated runtime errors trigger auto-disable.

Exit criteria:

- A demo process plugin can activate, register a command, log, show toast, and
  clean up on unload.
- The protocol has no dependency on WebView, DOM, JS, or React.

## Phase 4: Low-Risk `PluginContext` Namespaces

Status: [x] implemented.

Goal: implement API namespaces that do not touch terminal transport or secrets.

Implement:

- `ctx.app`
  - [x] `getTheme`;
  - [x] `getSettings`;
  - [x] `getVersion`;
  - [x] `getPlatform`;
  - [x] `getLocale`;
  - [x] `onThemeChange`;
  - [x] `onSettingsChange`;
  - [x] `getPoolStats`;
  - [x] `refreshAfterExternalSync`.
- `ctx.i18n`
  - [x] `t`;
  - [x] `getLanguage`;
  - [x] `onLanguageChange`.
- `ctx.storage`
  - [x] plugin-scoped JSON KV;
  - [x] size limit;
  - [x] corrupt-file recovery.
- `ctx.settings`
  - [x] declared setting get/set;
  - [x] `onChange`;
  - [x] export/apply syncable settings.
- `ctx.events`
  - [x] plugin-scoped event names;
  - [x] inter-plugin emit/on;
  - [x] lifecycle event bridge placeholders.
- `ctx.ui`
  - [x] `registerCommand`;
  - [x] `registerContextMenu`;
  - [x] `registerStatusBarItem`;
  - [x] `registerKeybinding`;
  - [x] `showToast`;
  - [x] `showNotification`;
  - [x] `showConfirm`;
  - [x] `showProgress`;
  - [x] `getLayout`;
  - [x] `onLayoutChange`.

Important native replacements:

- `showConfirm` uses native protected dialog policies; no window confirm.
- `showProgress` is host-owned and survives plugin event bursts.
- `registerCommand` registers a command id; handler execution is an RPC call.
- context menu items use shared native `context_menu_action` guard.

Tests:

- command registration appears in command palette.
- status item updates and disposes.
- context menu `when` predicate is replaced with host/RPC-visible enabled state;
  native should not execute arbitrary predicates during render.
- toast/notification/progress work after runtime reload.
- storage values are plugin-scoped and cannot collide across plugin ids.

Exit criteria:

- Demo plugin can provide command, status bar item, context menu item, setting,
  storage entry, toast, confirm, and progress.

## Phase 5: Terminal, Sessions, Connections, And Event Log

Status: [x] implemented for terminal, sessions, connections, and event log.

Goal: implement read/write terminal-facing APIs with strict timeout behavior.

Implement:

- `ctx.connections`
  - [x] `getAll`;
  - [x] `get`;
  - [x] `getState`;
  - [x] `getByNode`.
- `ctx.sessions`
  - [x] `getTree`;
  - [x] `getActiveNodes`;
  - [x] `getNodeState`;
  - [x] `onTreeChange`;
  - [x] `onNodeStateChange`.
- `ctx.eventLog`
  - [x] `getEntries`;
  - [x] `onEntry`.
- `ctx.terminal`
  - [x] `getActiveTarget`;
  - [x] `writeToActive`;
  - [x] `writeToNode`;
  - [x] `getNodeBuffer`;
  - [x] `getNodeSelection`;
  - [x] `search`;
  - [x] `getScrollBuffer`;
  - [x] `getBufferSize`;
  - [x] `clearBuffer`;
  - [x] `registerShortcut`;
  - [x] `registerInputInterceptor`;
  - [x] `registerOutputProcessor`;
  - [x] `openTelnet`.

Rules:

- Input interceptors run in registration order.
- Input interceptor timeout defaults to fail-open.
- Output processor timeout defaults to skip processor and preserve original
  bytes.
- A plugin cannot write to disconnected/link-down targets.
- `openTelnet` requires `contributes.terminalTransports` containing `telnet`.
- Buffer reads must be bounded.
- Selection reads must respect terminal/input/read-only selection ownership.

Tests:

- input interceptor modifies/suppresses input in registration order.
- timeout does not block terminal input.
- output processor failure does not corrupt output.
- undeclared telnet transport is rejected.
- disconnected node write is rejected.
- session tree subscription emits stable frozen snapshots.

Exit criteria:

- Terminal hooks can be enabled for a demo plugin without making terminal input
  feel slower or unsafe.

## Phase 6: SFTP, Forwarding, Secrets, Sync, AI, IDE, Transfers, Profiler

Status: [x] implemented for plugin APIs; `ctx.secrets` is wired to
OS keychain-backed plugin-scoped storage, `ctx.sftp` is wired to
NodeRouter-owned SFTP sessions, and `ctx.forward` is wired for existing
forwarding managers plus saved-forward sync snapshots/subscriptions. `ctx.sync`
saved connection snapshots, local sync metadata, apply snapshot, `.oxide`
preflight/export/validate/preview/import, and progress channels use
Workspace-owned stores/bridges. `ctx.transfers` reads SFTP background transfer
snapshots and emits progress/complete/error runtime events. `ctx.profiler`
reads native resource profiler metrics/history/running state and emits
throttled metrics. `ctx.ide` exposes read-only project/open-file/active-file
snapshots and file change subscriptions. `ctx.ai` exposes sanitized read-only
chat/provider snapshots and metadata-only message events. `ctx.api.invoke`
enforces manifest-declared command whitelists and only exposes native-adapted
backend commands, including native-backed system, connection, SFTP, forwarding,
transfer, and plugin HTTP proxy commands.

Goal: implement sensitive and subsystem-heavy APIs after the bridge is stable.

Implement:

- `ctx.secrets`
  - [x] OS keychain;
  - [x] plugin-scoped account id;
  - [x] batch get;
  - [x] no logs of secret values.
- `ctx.sftp`
  - [x] `listDir`;
  - [x] `stat`;
  - [x] `readFile`;
  - [x] `writeFile`;
  - [x] `mkdir`;
  - [x] `delete`;
  - [x] `rename`.
- `ctx.forward`
  - [x] list saved/active forwards;
  - [x] subscribe saved-forward changes;
  - [x] export/apply saved forwards snapshot;
  - [x] create/stop/stopAll/getStats.
- `ctx.sync`
  - [x] saved connection snapshot;
  - [x] local sync metadata;
  - [x] `.oxide` preflight/export/validate/preview;
  - [x] `.oxide` import;
  - [x] selected plugin settings import/export.
- `ctx.transfers`
  - [x] get all/by node;
  - [x] progress/complete/error subscriptions.
- `ctx.profiler`
  - [x] current metrics;
  - [x] history;
  - [x] running state;
  - [x] metrics subscription.
- `ctx.ide`
  - [x] IDE open/project/open files/active file;
  - [x] file open/close/active subscriptions.
- `ctx.ai`
  - [x] conversations;
  - [x] sanitized messages;
  - [x] active provider;
  - [x] available models;
  - [x] message subscription.
- `ctx.api.invoke`
  - [x] only whitelisted commands from `contributes.apiCommands`;
  - [x] native adapter coverage for every supported backend command.

Rules:

- Secrets require explicit plugin id and key validation.
- SFTP write/delete/rename require write capability.
- Forward create/stop requires network/forward capability.
- Sync import/export progress must use host progress channels, not ad hoc UI.
- AI messages are sanitized according to existing AI data policy.
- Backend invoke never bypasses native Rust permission checks.

Tests:

- keychain account id cannot collide across plugin ids.
- undeclared backend command is rejected.
- SFTP path and node access checks reject invalid requests.
- `.oxide` plugin settings round trip matches existing import/export behavior.
- forward create/stop respects ownership and error reporting.

Exit criteria:

- A sync-style plugin can use secrets, storage, `.oxide`, saved connections, and
  progress without WebView.

## Phase 7: Native Declarative UI For Plugin Tabs And Sidebar Panels

Status: [x] implemented for native tab/sidebar declarative schemas.

Goal: replace Tauri React component views with host-rendered native schemas.

Do not implement `React.ComponentType`. Instead, support:

```json
{
  "kind": "form",
  "sections": [
    {
      "id": "deploy",
      "title": "Deploy",
      "controls": [
        { "kind": "text", "id": "target", "label": "Target" },
        { "kind": "select", "id": "env", "label": "Environment", "options": [] },
        { "kind": "button", "id": "run", "label": "Run" }
      ]
    }
  ]
}
```

Initial schema controls:

- text;
- password;
- number;
- checkbox;
- select;
- button;
- markdown;
- code block;
- status badge;
- progress;
- table;
- list;
- empty state;
- divider;
- key/value row.

Host-rendered surfaces:

- [x] plugin tab surface;
- [x] plugin sidebar panel;
- [x] plugin settings panel;
- [x] plugin progress surface through the host-owned progress channel.

Interaction model:

- [x] host owns focus, selection, wheel routing, outside click, clipping, and z-order;
- [x] plugin receives `UiEvent` messages;
- [x] plugin can update schema by re-registering the same view id;
- [x] host validates every schema update before applying.

Tests:

- [x] form schema renders through host GPUI controls.
- [x] button click sends `UiEvent`.
- [x] disabled/loading state blocks action.
- [x] schema patch cannot create unknown controls.
- [x] long lists use native virtual list primitives.

Exit criteria:

- A non-WebView plugin can open a native tab and sidebar panel with interactive
  controls.

## Phase 8: Plugin Install, Update, Uninstall, And Registry

Status: [x] implemented: package registry/install/update/uninstall backend is
implemented and Plugin Manager exposes install, update check, update install,
overwrite confirmation, and uninstall entry points.

Goal: port Tauri package management without the plugin server.

Tasks:

- Port registry index model:
  - [x] id;
  - [x] name;
  - [x] version;
  - [x] description;
  - [x] download url;
  - [x] checksum;
  - [x] runtime compatibility;
  - [x] capabilities summary.
- Port download/install:
  - [x] max package size;
  - [x] max extracted size;
  - [x] max file count;
  - [x] zip slip protection;
  - [x] staging dir;
  - [x] checksum verification;
  - [x] rollback backup;
  - [x] final rename.
- Port install-from-url:
  - [x] detect root or single nested plugin dir;
  - [x] read `plugin.json`;
  - [x] validate id;
  - [x] support overwrite confirmation in Plugin Manager UI.
- Port update check:
  - [x] compare versions;
  - [x] report available updates.
- Port uninstall:
  - [x] disable runtime;
  - [x] dispose contributions;
  - [x] remove install dir;
  - [x] preserve or remove settings based on explicit backend choice.

Tests:

- [x] bad zip cannot escape plugin dir.
- [x] failed install rolls back.
- [x] checksum mismatch rejects package.
- [x] nested GitHub archive layout installs correctly.
- [x] uninstall removes contributions and runtime.

Exit criteria:

- Plugin Manager can install/update/uninstall native plugin packages safely.

## Phase 9: Verification Matrix

Status: [x] complete.

Verified on 2026-05-25:

- `cargo fmt --check`
- `cargo test -p oxideterm-gpui-app plugin_host`
- `cargo test -p oxideterm-gpui-app plugin_runtime`
- `cargo test -p oxideterm-gpui-app plugin_lifecycle`
- `cargo test -p oxideterm-gpui-app plugin_manager`
- `cargo check -p oxideterm-gpui-app`
- `git diff --check`

Before claiming plugin system parity, run this matrix.

General:

- `cargo fmt --check`
- `cargo test -p oxideterm-gpui-app plugin_host`
- runtime protocol tests
- plugin settings storage tests
- plugin manager rendering state tests
- `cargo check -p oxideterm-gpui-app`
- `git diff --check`

Runtime:

- process plugin activate/deactivate;
- WASM plugin activate/deactivate;
- runtime timeout;
- crash cleanup;
- auto-disable after repeated errors;
- disposable cleanup.

Security:

- plugin id validation;
- relative path traversal;
- secret key validation;
- backend command whitelist;
- capability denial;
- SFTP write denial without capability;
- terminal write denial for disconnected target.

UI:

- command palette entries;
- context menu entries;
- status bar entries;
- toast/notification/progress;
- confirm dialog policy;
- tab/sidebar native schema;
- focus-visible and Tab order for plugin dialogs;
- wheel routing inside plugin surfaces.

Terminal:

- input interceptor order;
- input interceptor timeout fail-open;
- output processor timeout skip;
- terminal shortcut registration;
- buffer/selection reads;
- telnet transport declaration gate.

Sync:

- `.oxide` plugin settings export/import;
- preview selected plugin ids;
- import result counts;
- Cloud Sync plugin settings metadata.

## Implementation Order

Use this exact order unless a blocker forces a split.

1. [x] Add `docs/native-plugin-api-ledger.md`.
2. [x] Finish `plugin_host` manifest/config/registry state.
3. [x] Update Plugin Manager to manage discovered/disabled/error states.
4. [x] Implement manifest-only settings contributions.
5. [x] Implement manifest-only AI tool metadata contributions.
6. [x] Implement native command/status/context menu contribution registry.
7. [x] Implement process runtime bridge.
8. [x] Implement runtime supervisor and disposable cleanup.
9. [x] Implement low-risk `ctx.app/i18n/storage/settings/events/ui`.
   - [x] `ctx.app` read-only snapshot calls: theme, settings, version, platform, locale, pool stats.
   - [x] `ctx.i18n.t/getLanguage` with plugin-key fallback.
   - [x] `ctx.storage.get/set/remove` with plugin-scoped JSON KV and size limit.
   - [x] `ctx.settings.get/set` through declared typed setting registry.
   - [x] `ctx.ui.registerCommand/registerKeybinding/registerContextMenu/registerStatusBarItem`.
   - [x] `ctx.ui.showToast/showNotification/showProgress`.
   - [x] `ctx.app`/`ctx.i18n`/`ctx.settings` subscriptions.
   - [x] `ctx.events`.
   - [x] `ctx.ui.showConfirm`.
   - [x] `ctx.ui.getLayout/onLayoutChange`.
10. [x] Implement WASM runtime bridge.
   - [x] Bounded WASIp1 `_start` activation with no ambient host stdio/env/fs/network.
   - [x] WASM host-call ABI and persistent command/event dispatch.
11. [x] Implement terminal/session/connection/eventLog APIs.
   - [x] `ctx.connections.getAll/get/getState/getByNode`.
   - [x] `ctx.sessions.getTree/getActiveNodes/getNodeState/onTreeChange/onNodeStateChange`.
   - [x] `ctx.eventLog.getEntries/onEntry`.
   - [x] `ctx.terminal`.
     - [x] read-only active target/buffer/selection/scroll/size snapshots.
     - [x] write APIs.
     - [x] input/output hook registration, Telnet.
12. [x] Implement secrets/sftp/forward/sync/transfers/profiler/ide/ai APIs.
13. [x] Implement declarative native plugin tab/sidebar UI.
14. [x] Implement registry install/update/uninstall.
15. [x] Run verification matrix and update ledger statuses.

## Notes For Future Code Changes

- Keep API names close to `plugin-api.d.ts`, but prefer Rust-native structs and
  explicit serde names over stringly-typed maps where the schema is known.
- Any comment that says "Tauri parity" must name the Tauri source file or API
  section being matched.
- Any runtime feature must have a denial path before it has a success path.
- Plugin UI must use existing shared native controls so browser-behavior fixes
  apply globally.
- Legacy JS plugins are migration inputs, not executable native plugins.
