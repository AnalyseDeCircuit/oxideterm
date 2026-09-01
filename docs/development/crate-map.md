# Crate Map

The workspace is intentionally split by responsibility. Start from the user-visible behavior, then follow dependencies inward; do not begin in a renderer or a transport crate unless the observed failure is already below the application boundary.

| Area | First entry points | Supporting crates | Ownership boundary |
| --- | --- | --- | --- |
| Desktop application and workspace | `crates/oxideterm-gpui-app/src/main.rs`, `src/workspace.rs`, `src/workspace/` | `oxideterm-gpui-ui`, `oxideterm-gpui-platform`, `oxideterm-gpui-settings-view` | Window composition, navigation, modal ownership, persisted settings integration |
| Terminal UI and interaction | `crates/oxideterm-gpui-terminal` | `oxideterm-terminal`, `oxideterm-session-adapter`, `oxideterm-theme` | Pane interaction, selection, terminal overlays, render snapshots |
| Terminal protocol and model | `crates/oxideterm-terminal` | `alacritty-terminal`, `vte`, `oxideterm-terminal-model`, `oxideterm-terminal-encoding`, `oxideterm-terminal-graphics`, `oxideterm-terminal-semantic`, `oxideterm-terminal-unicode` | Bytes enter the terminal model before a UI snapshot is rendered |
| SSH nodes and authentication | `crates/oxideterm-ssh` | `oxideterm-ssh-launch`, `oxideterm-connections`, `oxideterm-topology`, `oxideterm-network-proxy`, `russh` | `SshConnectionRegistry` and `NodeRouter` own shared transport lifetime |
| SFTP and local files | `crates/oxideterm-sftp`, `crates/oxideterm-local-files`, `crates/oxideterm-gpui-app/src/workspace/sftp.rs` | `russh-sftp`, `oxideterm-preview` | SFTP is a node consumer, not a terminal-pane side effect |
| Port forwarding | `crates/oxideterm-forwarding`, `crates/oxideterm-gpui-app/src/workspace/forwards.rs` | `oxideterm-ssh`, `oxideterm-connections` | A forwarding owner retains its listener, bridge tasks, and cancellation path |
| Settings, secrets, and data | `crates/oxideterm-settings`, `crates/oxideterm-settings-model` | `oxideterm-secret-store`, `oxideterm-atomic-file`, `oxideterm-portable-runtime`, `oxideterm-cloud-sync` | Settings contain safe data; secret storage owns credentials |
| Internationalization and theme | `crates/oxideterm-i18n/locales`, `crates/oxideterm-theme` | `oxideterm-gpui-ui` | Every user-visible UI key must exist in every locale catalog |
| Remote desktop and media | `crates/oxideterm-remote-desktop`, `crates/oxideterm-gpui-remote-desktop` | `oxideterm-rdp-helper`, `oxideterm-vnc-helper`, `oxideterm-pcm-audio` | Viewer lifetime is distinct from terminal and SSH-node consumers |
| Plugins, CLI, and automation | `crates/oxideterm-plugin-*`, `crates/oxideterm-cli`, `crates/oxideterm-public-mcp` | `scripts/`, `agent/` | Capability APIs must not bypass the application runtime or secret boundary |
| Framework and native platform code | `crates/gpui-ce/gpui` | `gpui_windows`, `gpui_macos`, `gpui_linux`, `gpui_wgpu`, `gpui_platform` | Maintained vendor closure; see [GPUI CE maintenance](gpui-ce.md) |

## Main Runtime Path

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

## Common Traces

- A terminal display or input problem: start in `oxideterm-gpui-terminal`; inspect `oxideterm-terminal` and then `alacritty-terminal` only if the snapshot is already wrong.
- An SSH, reconnect, jump-host, SFTP, or forwarding problem: start in the relevant `workspace` module and trace the registered `ConnectionConsumer` through `NodeRouter` and `SshConnectionRegistry`.
- A settings page or text-field problem: start in `oxideterm-gpui-app/src/workspace/settings/`, then `oxideterm-settings`, `oxideterm-i18n`, and shared UI primitives as needed.
- A platform-only window, cursor, text, or renderer problem: reproduce it on that platform before considering `crates/gpui-ce/`.

Use `cargo tree -p <crate>` when a dependency boundary is unclear. A crate name is not proof of ownership: follow the runtime owner and the persisted-data owner before adding state or a background task.

## Where To Add Code

| Need | Preferred location | Do not place it in |
| --- | --- | --- |
| A page-specific action or rendering decision | The corresponding `workspace/<feature>/` module | A global UI primitive or a GPUI CE crate |
| A reusable visual primitive | `oxideterm-gpui-ui` | A feature module copied into multiple views |
| Persisted product setting | `oxideterm-settings` plus its model and localized settings view | An ad-hoc static or view-local field |
| A new text label | `oxideterm-i18n/locales/` and the owning view | A hard-coded string in render code |
| A transport capability | The protocol/domain crate plus its application adapter | A terminal pane or modal callback |
| A platform event, renderer, or native-window correction | The matching GPUI CE platform crate after reproduction | Product UI code that guesses native state |

## Useful Entry Files

- `crates/oxideterm-gpui-app/src/workspace/root/` assembles the root view, modal order, workspace initialization, and window state.
- `crates/oxideterm-gpui-app/src/workspace/runtime_entity.rs` coordinates node readiness, connection runtime events, reconnection, and registry interactions.
- `crates/oxideterm-gpui-app/src/workspace/tabs/create.rs` creates terminal consumers and is the normal route for opening terminal tabs.
- `crates/oxideterm-gpui-app/src/workspace/sftp/` and `workspace/forwards/` own SFTP and forwarding UI/runtime adaptation.
- `crates/oxideterm-gpui-terminal/src/app/` owns terminal input, interaction state, and high-level terminal rendering; `terminal_view/` owns viewport layout and paint data.
- `crates/oxideterm-ssh` owns SSH transport, authentication, channel, registry, and topology behavior.

When a file has become large, split by responsibility inside its current module before creating a new cross-cutting crate. New crates need a real, stable ownership boundary rather than a convenient place to move lines.
