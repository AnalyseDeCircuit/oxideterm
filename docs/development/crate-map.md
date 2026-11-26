# Crate Map

The workspace is intentionally split by responsibility. Start from the user-visible behavior, then follow dependencies inward; do not begin in a renderer or a transport crate unless the observed failure is already below the application boundary.

| Area | First entry points | Supporting crates | Ownership boundary |
| --- | --- | --- | --- |
| Workspace chrome and host actions | `crates/oxideterm-gpui-shell` | `oxideterm-gpui-app`, `oxideterm-gpui-web` | Shared frame, rails, tab strip, pane tree, palette and overlay primitives; each host supplies pages, routes actions and owns sessions |
| Terminal UI and interaction | `crates/oxideterm-gpui-terminal-view` | `oxideterm-gpui-terminal`, `oxideterm-gpui-web` | Shared canvas, input and selection; the desktop pane adapts native sessions, while the browser subscribes to `TerminalViewEvent` |
| Terminal emulation | `crates/oxideterm-terminal-emulator` | `oxideterm-terminal-model`, `oxideterm-terminal-graphics`, `alacritty-terminal`, `vte` | Grid, parser and snapshot generation without PTY or SSH ownership; `oxideterm-terminal` owns native terminal sessions |
| SSH nodes and authentication | `crates/oxideterm-ssh` | `oxideterm-ssh-launch`, `oxideterm-connections`, `oxideterm-topology`, `oxideterm-network-proxy`, `russh` | `SshConnectionRegistry` and `NodeRouter` own shared transport lifetime |
| SFTP and local files | `crates/oxideterm-sftp`, `crates/oxideterm-local-files`, `crates/oxideterm-gpui-app/src/workspace/sftp.rs` | `russh-sftp`, `oxideterm-preview` | SFTP is a node consumer, not a terminal-pane side effect |
| Project state and file access | `oxideterm-gpui-ide`, `oxideterm-ide-core`, `oxideterm-ide-fs` | App `workspace/ide.rs`, `NodeRouter` | `IdeSurface` owns the project and async coordination; transport-free `IdeWorkspace` tracks versions and dirty-close decisions; the scoped backend owns file access |
| Text editing, syntax and search | `oxideterm-gpui-editor`, `oxideterm-editor-core`, `oxideterm-editor-syntax` | Language plugins, shared UI inputs, `oxideterm-gpui-markdown` | Text/undo state, background syntax metadata and GPUI presentation have separate owners; IDE and SFTP reuse the editor/search path |
| Port forwarding | `crates/oxideterm-forwarding`, `crates/oxideterm-gpui-app/src/workspace/forwards/runtime_service.rs` | `oxideterm-ssh`, `oxideterm-connections` | `ForwardingRuntimeService` retains managers, listeners and SSH consumer bindings independently of page mounts |
| Settings, secrets, and data | `crates/oxideterm-settings`, `crates/oxideterm-settings-model` | `oxideterm-secret-store`, `oxideterm-atomic-file`, `oxideterm-portable-runtime`, `oxideterm-cloud-sync` | Settings contain safe data; secret storage owns credentials |
| Internationalization and theme | `crates/oxideterm-i18n/locales`, `crates/oxideterm-theme` | `oxideterm-gpui-ui` | Every user-visible UI key must exist in every locale catalog |
| Remote desktop and media | `crates/oxideterm-remote-desktop`, `crates/oxideterm-gpui-remote-desktop` | Marketplace `plugins/rdp` and `plugins/vnc` in `oxideterm-plugins`; host audio in `oxideterm-pcm-audio` | Protocol helpers are installed plugins; the host owns profiles and viewer sessions separately from terminal and SSH-node consumers |
| Plugins, CLI, and automation | `crates/oxideterm-plugin-*`, `crates/oxideterm-cli`, `crates/oxideterm-public-mcp` | `scripts/`, `agent/` | Capability APIs must not bypass the application runtime or secret boundary |
| Framework and native platform code | `crates/gpui-pre/gpui` | `gpui-pre-apple`, `gpui-pre-windows`, `gpui-pre-macos`, `gpui-pre-linux`, `gpui-pre-wgpu`, `gpui-pre-web`, `gpui-pre-platform` | Seven locally patched packages with registry helpers; see [GPUI maintenance](gpui-pre.md) |

## Desktop and browser entry points

Desktop starts in [oxideterm-gpui-app](../../crates/oxideterm-gpui-app/src/main.rs). Its [window shell](../../crates/oxideterm-gpui-app/src/workspace/window_shell.rs) mounts the shared frame; the app retains native windows, node consumers and shutdown handlers. The browser starts in [boot.rs](../../crates/oxideterm-gpui-web/src/boot.rs). It selects the single-threaded web backend through [gpui-platform](../../crates/oxideterm-gpui-platform/src/lib.rs) and mounts a browser-owned workspace in the same frame.

| Feature | Shared boundary | What the browser host supplies |
| --- | --- | --- |
| Settings and connections | `PortableSettingsSurface`, `PortableCatalogView` and the portable connection form | [Workspace setup](../../crates/oxideterm-gpui-web/src/host.rs) and [connection actions](../../crates/oxideterm-gpui-web/src/connections.rs); these views do not open native SSH connections |
| Files and IDE | [FileSystemBridge](../../crates/oxideterm-gpui-file-manager/src/filesystem.rs), [SftpLocalHost](../../crates/oxideterm-gpui-sftp/src/local_host.rs), [PortableIdeHost](../../crates/oxideterm-gpui-ide/src/portable.rs) | Demo file pages share in-memory files. Desktop disk access, SSH channels and native editor syntax backends stay outside the portable views |
| Host Tools | [HostToolsBridge](../../crates/oxideterm-gpui-host-tools/src/bridge.rs) | [Sample host and captures](../../crates/oxideterm-gpui-web/src/tools/monitor.rs), rather than desktop command execution |
| AI | [AiHostBridge](../../crates/oxideterm-gpui-ai/src/bridge.rs) | The demo assistant can [call a configured provider via browser fetch](../../crates/oxideterm-gpui-web/src/ai.rs); [browser services](../../crates/oxideterm-gpui-web/src/ai/services.rs) do not acquire desktop keychain or agent processes |

For the terminal, follow [terminal.rs](../../crates/oxideterm-gpui-web/src/terminal.rs): output goes to the view for that session, and input travels back through the selected transport. No gateway parameter selects the in-memory demo. A supplied gateway selects the [WSS client](../../crates/oxideterm-gpui-web/src/transport.rs), not a browser SSH stack. [Web Bluetooth](../../crates/oxideterm-gpui-web/src/ble.rs) uses browser device permission rather than the gateway. See [local development](local-development.md#browser-wasm) to run the browser host.

## Desktop runtime path

Most product operations follow the same shape:

```text
GPUI event
  -> WorkspaceApp action or feature entity
  -> domain service or runtime owner
  -> protocol/session task
  -> typed event or snapshot
  -> WorkspaceApp / terminal render projection
```

Keep these stages separate. Render code should project existing state rather than create a network task; a protocol task should report state through its owner rather than mutate a view directly. This is especially important for terminal output, SFTP listings, connection status, and remote-desktop frames.

## State And Delivery Entry Points

| Question | Start here |
| --- | --- |
| Who retains panes, window mounts and session locations? | [Tab host](../../crates/oxideterm-gpui-app/src/workspace/tabs/entity.rs) |
| Who owns probes, reconnect jobs and cancellation? | [Workspace runtime](../../crates/oxideterm-gpui-app/src/workspace/runtime_entity.rs) |
| Why did an IDE save leave newer edits dirty? | [Save completion](../../crates/oxideterm-gpui-ide/src/surface/editor_actions.rs) |
| Why did an old syntax result not apply? | [Syntax scheduling](../../crates/oxideterm-gpui-editor/src/surface/syntax_task.rs) |
| Where are remote desktop frame lineage and queue limits checked? | [Frame queue](../../crates/oxideterm-remote-desktop/src/frame_queue.rs) |
| Where are local sync apply and remote publication separated? | [Sync coordinator](../../crates/oxideterm-cloud-sync/src/operation/synchronize.rs) and [live-owner integration](../../crates/oxideterm-gpui-app/src/workspace/cloud_sync/causal.rs) |
| Who budgets UI result delivery? | [Delivery primitives](../../crates/oxideterm-gpui-app/src/workspace/delivery.rs); identity checks remain in each domain consumer |

The [architecture guide](../user-guide/en/architecture.md) maps these owners and their failure/recovery paths. Use the entry points here to inspect the current implementation.

## Common Traces

- A terminal display or input problem: start in `oxideterm-gpui-terminal-view` for shared layout and input, `oxideterm-terminal-emulator` for an incorrect grid snapshot, or the affected host for session delivery.
- An SSH, reconnect, jump-host, SFTP, or forwarding problem: start in the relevant `workspace` module and trace the registered `ConnectionConsumer` through `NodeRouter` and `SshConnectionRegistry`.
- A settings page or text-field problem: start in `oxideterm-gpui-app/src/workspace/settings/`, then `oxideterm-settings`, `oxideterm-i18n`, and shared UI primitives as needed.
- A platform-only window, cursor, text, or renderer problem: reproduce it on that platform before considering `crates/gpui-pre/`.

Use `cargo tree -p <crate>` when a dependency boundary is unclear. A crate name is not proof of ownership: follow the runtime owner and the persisted-data owner before adding state or a background task.

## Where To Add Code

| Need | Preferred location | Do not place it in |
| --- | --- | --- |
| A page-specific action or rendering decision | The owning `oxideterm-gpui-*` feature crate; use a host adapter for window or runtime services | `WorkspaceApp` or the shared frame |
| A reusable visual primitive | `oxideterm-gpui-ui` | A feature module copied into multiple views |
| Persisted product setting | `oxideterm-settings` plus its model and localized settings view | An ad-hoc static or view-local field |
| A new text label | `oxideterm-i18n/locales/` and the owning view | A hard-coded string in render code |
| A transport capability | The protocol/domain crate plus its application adapter | A terminal pane or modal callback |
| A platform event, renderer, or native-window correction | The matching patched GPUI platform crate after reproduction | Product UI code that guesses native state |

## Useful Entry Files

- `crates/oxideterm-gpui-app/src/workspace/root/` assembles the root view, modal order, workspace initialization, and window state.
- `crates/oxideterm-gpui-app/src/workspace/runtime_entity.rs` coordinates node readiness, connection runtime events, reconnection, and registry interactions.
- `crates/oxideterm-gpui-app/src/workspace/tabs/create.rs` creates terminal consumers and is the normal route for opening terminal tabs.
- `crates/oxideterm-gpui-shell/src/panes.rs` defines the shared split tree. `crates/oxideterm-gpui-app/src/workspace/tabs/` owns desktop tab membership, session association and detached window mounts.
- `crates/oxideterm-gpui-editor/src/surface/search_bar.rs` owns the shared editor search interaction; do not create a separate SFTP search implementation.
- `crates/oxideterm-gpui-app/src/workspace/sftp/` and `workspace/forwards/` own SFTP and forwarding UI/runtime adaptation.
- `crates/oxideterm-gpui-terminal/src/terminal_view.rs` implements `TerminalCanvasHost` for the desktop pane. Shared viewport layout and paint live in `crates/oxideterm-gpui-terminal-view/src/element/`.
- `crates/oxideterm-ssh` owns SSH transport, authentication, channel, registry, and topology behavior.

When a file has become large, split by responsibility inside its current module before creating a new cross-cutting crate. New crates need a real, stable ownership boundary rather than a convenient place to move lines.
