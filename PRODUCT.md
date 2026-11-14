# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

## Stack

Rust + GPUI native desktop framework targeting macOS, Windows, and Linux (`crates/oxideterm-theme`, `crates/oxideterm-gpui-ui`, `crates/oxideterm-gpui-app`, `crates/oxideterm-gpui-ide`, `crates/oxideterm-gpui-terminal`, `crates/oxideterm-gpui-editor`, `crates/oxideterm-gpui-remote-desktop`, `crates/oxideterm-gpui-settings-view`, `crates/oxideterm-gpui-cloud-sync`).

## Users

- **Primary**: System administrators, infrastructure engineers, DevOps practitioners, and backend developers managing multiple Linux/Unix/Windows servers daily over SSH, Mosh, Serial, Telnet, RDP, and VNC.
- **Usage context**: Long continuous multi-hour terminal and remote-ops sessions where speed, low memory footprint, keyboard efficiency, and zero visual distraction matter more than marketing polish.

## Product Purpose

OxideTerm unifies terminal sessions, SFTP file management, port forwarding, remote file editing (built-in IDE), live host telemetry & 11 diagnostic tools, remote desktop (RDP/VNC), encrypted cloud sync, knowledge notes, and bring-your-own-key AI assistance (`OxideSens`) into one lightweight native GPU-rendered workspace.

## Positioning

- **Connect once, reuse everywhere**: Terminals, SFTP, port forwards, host monitoring, and remote IDE panes for a server share a single `NodeRouter`-owned SSH connection without re-authenticating or spawning duplicate WebView processes.
- **Survives transient drops**: Built-in probe and 30-second grace-period reconnection keeps interactive TUI state (`vim`, `htop`, etc.) recoverable across brief network interruptions.
- **Native speed & footprint**: Pure-Rust SSH (`russh` + `ring`) and GPUI rendering (~81 MB macOS / ~24 MB Windows idle, zero Electron/WebView overhead).

## Operating Context

- **Multi-pane & multi-protocol workspace**: Up to 16 split panes per tab mixing SSH, local shells, SFTP, remote desktop, notes, and built-in IDE views.
- **Dual fixed tool rails (user-confirmed shell skeleton)**:
  - Left primary activity rail switching workspace domains (Sessions, Connection Manager, Runtime Overview, Extensions, Cloud Sync, Knowledge, Workspace, Files, Notifications, Settings).
  - Right context & host-tools rail switching the 11 host inspection tools (`Monitor`, `Processes`, `GPU`, `Docker`, `Services`, `Logs`, `Ports`, `Tmux`, `Filesystems`, `Schedules`, `Packages`), `OxideSens` AI assistant, and quick app lock.
- **Single unified top chrome grid (`1A`)**: Top window chrome, tab strip, and side panel headers align along one consistent horizontal baseline without misaligned header bands or awkward empty-state label bars.

## Capabilities and Constraints

- **Shell architecture constraint**: Both the left activity rail and right context/host-tools rail MUST be preserved as always-accessible navigation edges; improvements come from visual grouping, contrast discipline, active indicators, and spacing clarity rather than collapsing them into hidden menus.
- **Monochrome-canvas restraint (`2B`)**: Window chrome, sidebars, and primary workspace surfaces share one unified base ground (`ui.bg`) across both light and dark themes, establishing depth via subtle neutral steps (`bg_sunken`, `bg_hover`, `bg_active`), low-alpha hairline dividers (`<= 1px`), typography contrast, and interactive states rather than patchy card boxes.
- **Focused Quick-Launch Welcome (`3B`)**: Empty workspace welcome tab serves as a focused quick-launch surface for pinned/recent remote environments (capped compact items distinct in density and action from the full left session tree) plus top-level launch actions, with zero terminal-cursor halos or decorative costumes.
- **Localization & Scaling**: All user-visible strings must sync across all 11 repository locale packs; all typography must scale cleanly via `ThemeTokens::apply_ui_font_scale`; all animations must honor `UiMotionProfile` and `cx.reduce_motion()`.
- **Ownership & Security**: `NodeRouter` / connection registries own SSH lifetimes independently of panes; sensitive credentials stay zeroized and outside logs/diagnostics/AI contexts.

## Brand Commitments

- **OxideTerm** identity: free, open-source (GPL-3.0), no account/telemetry/vendor cloud lock-in.
- Must continue supporting 20+ built-in dark and light themes (including `default`, `oxide`, `paper-oxide`, `code-light`, `dracula`, etc.) and optional window background imagery with vibrancy fallback.

## Product Principles

1. **Precision over decoration**: Every border, badge, and highlight must communicate active state, threshold, or hierarchy—never cosmetic filler.
2. **One canvas, clear focal plane**: On an Operate workbench, the active terminal/editor/data table owns attention; chrome and peripheral rails stay quiet until hovered or selected.
3. **Consistent native controls**: Buttons, inputs, tabs, list rows, and popovers share one predictable desktop geometry, hover/focus behavior, and keyboard contract.
4. **Progressive disclosure at high density**: Show high-signal telemetry cleanly at rest; reveal secondary per-row operational actions on hover, focus, or row expansion.
5. **Respect user preferences automatically**: Font scale, density profile, reduced motion, and dark/light theme polarity apply uniformly without hardcoded pixel or hex bypasses.

## Accessibility & Inclusion

- High-contrast text and semantic indicators guaranteed across every built-in dark and light palette (`>= 4.5:1` primary text, `>= 3.0:1` semantic UI indicators).
- Keyboard focus visibility (`focus-visible` ring), logical tab order (`tab_index`), and keyboard activation across interactive controls and dialogs.
