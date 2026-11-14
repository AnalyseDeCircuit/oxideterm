---
name: OxideTerm Desktop Design System
description: Quiet Precision Instrument UI system for OxideTerm's GPUI workspace (1A unified chrome, 2B monochrome-canvas hierarchy, 3B focused environment welcome, dual tool rails retained)
colors:
  accent: "#ea580c"
  canvas-dark: "#09090b"
  canvas-light: "#ffffff"
  neutral-sunken-dark: "#050506"
  neutral-sunken-light: "#f2f3f5"
  border-dark: "#2e2e33"
  border-light: "#e2e4e8"
  text-dark: "#f4f4f5"
  text-light: "#1f2328"
  text-muted-dark: "#a1a1aa"
  text-muted-light: "#656d76"
typography:
  body:
    fontFamily: "SF Pro Text"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: "18px"
  headline:
    fontFamily: "SF Pro Text"
    fontSize: "18px"
    fontWeight: 600
    lineHeight: "24px"
  title:
    fontFamily: "SF Pro Text"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: "20px"
  label:
    fontFamily: "SF Pro Text"
    fontSize: "12px"
    fontWeight: 500
    lineHeight: "16px"
  mono:
    fontFamily: "JetBrains Mono"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: "16px"
rounded:
  sm: "4px"
  md: "6px"
  lg: "8px"
spacing:
  one: "4px"
  two: "8px"
  three: "12px"
  four: "16px"
  six: "24px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "#ffffff"
    rounded: "{rounded.md}"
    height: "32px"
    padding: "0 12px"
  button-compact:
    height: "28px"
    rounded: "{rounded.md}"
    padding: "0 10px"
  input-default:
    height: "32px"
    rounded: "{rounded.md}"
    padding: "0 10px"
---

# Design System: OxideTerm

## Overview

**Creative North Star: "Quiet Precision Instrument"**

OxideTerm's interface is engineered to feel like a calm, responsive native desktop workbench rather than a ported Electron web dashboard. The entire window rests on one continuous monochrome ground (`ui.bg`, strategy **2B**) framed by crisp, low-alpha `1px` structural hairlines, clean horizontal alignment across the top chrome (**1A**), and disciplined left/right tool rails.

Because the primary ground is unified across chrome, sidebars, and main canvas, depth and craft come from **perceptual contrast restraint**, **exact vertical/horizontal grid alignment**, **tabular monospace alignment on machine data**, and **progressive disclosure**—never from stacked card boxes or ambient glow effects.

**Key Characteristics:**
- **Single monochrome canvas (`2B`)**: Chrome, rails, sidebars, and workspaces share `ui.bg`; selection, input wells (`bg_sunken`), and hover states provide clean neutral stepping.
- **Single-horizon top chrome (`1A`)**: Window bar, tab strip, and left/right sidebar headers snap to a unified horizontal datum (`34px–36px`) instead of staggered `30px / 36px / 44px` bands.
- **Dual tool rails retained with quiet grouping**: Left primary activity rail and right context/host-tools rail stay fixed as desktop navigation edges (`42px` width), visually organized via semantic grouping hairlines, calmer idle ink weight, and unmistakable active indicators.
- **Zero resting noise**: No drop shadows on in-plane panels (`Z = 0`), no hardcoded dark-only status hexes, and no permanent row button walls on dense monitoring lists.

## Colors

All color decisions derive from active `ThemeTokens.ui` palettes (`crates/oxideterm-theme`) and must support both light (`code-light`, `paper-oxide`, `solarized-light`) and dark (`default`, `oxide`, `github-dark`, etc.) polarity cleanly.

### Primary & Semantic Roles
- **Accent (`ui.accent` / `ui.accent_hover` / `ui.accent_text`)**: Reserved exclusively for primary CTAs, active navigation/tab indicators (`2px` edge bar), keyboard focus rings (`2px` outline), and interactive selection highlights.
- **Semantic States (`ui.success`, `ui.warning`, `ui.error`, `ui.info`)**: Used strictly where telemetry, connection health, latency thresholds, validation errors, or destructive confirmation carry real meaning. All 67 legacy hardcoded Tailwind hex literals (`0x34d399`, `0xf59e0b`, `0xef4444`, `0x3b82f6`, etc.) are prohibited.

### Neutral Layer Hierarchy (Rule `2B`)
- **Primary Canvas (`ui.bg`)**: Shared ground across titlebar, left/right rails, sidebar panels, and main workspace stage.
- **Sunken Control Well (`ui.bg_sunken`)**: Recessed surface for text inputs, search bars, segmented bar tracks, and code/log wells. On light themes (`code-light`, `paper-oxide`), `bg_sunken` and `bg_hover` must maintain distinct luminance separation from `bg` (no white-on-white collapse).
- **Hover / Active Pill (`ui.bg_hover`, `ui.bg_active`)**: Subtle neutral shift on interactive focus/hover/selection (`0x2e`–`0x45` tint or calibrated neutral step).
- **Floating Overlay Surface (`ui.bg_elevated` / `ui.bg_panel`)**: Reserved strictly for `Z > 0` popovers, menus, dialogs, command palette, and toasts.
- **Structural Dividers (`ui.border` at `0x40`–`0x59` alpha)**: `<= 1px` hairlines only where two adjacent scrollable panes or rail clusters require spatial separation.

**The Derived-Theme Polarity Rule.** `derive_ui_colors_from_terminal` must shift `bg_sunken`, `bg_hover`, `bg_elevated`, and `border` proportionally relative to background luminance direction (subtle darkening steps on light grounds, subtle lifting steps on dark grounds) rather than flat `+15/+20` RGB integer additions.

## Typography

**Body / UI Font:** `SF Pro Text` (macOS system UI font stack fallback per OS)
**Data / Code Font:** User-configured monospace (`JetBrains Mono` / terminal font stack)

**Character:** Compact desktop sans scale paired with crisp tabular monospace for host endpoints, PIDs, ports, byte sizes, latencies, timestamps, and shortcuts.

### Hierarchy
- **Display / Welcome Wordmark** (Semibold, `24px–28px`, tight tracking): Clean title on Welcome view only; zero oversized `48px` banner typography or decorative cursor props.
- **Section Heading** (Semibold, `15px–18px`): Panel and settings page headers (`ui_text_base` / scaled token).
- **Primary Body / Control Label** (Regular/Medium, `13px`): Tab titles, sidebar rows, form inputs, primary list titles (`ui_text_sm` / `form_text_font_size`).
- **Secondary / Metadata** (Regular, `12px`): Subtitles, table headers, shortcut hints, status labels (`ui_text_xs`).
- **Dense Technical Micro-Label** (Medium/Mono, `11px` scaled): Scaled proportionally through `ThemeTokens.metrics`—never hardcoded as raw unscaled `px(10.0)` / `px(11.0)`.

**The Zero Unscaled Type Rule.** All UI text sizes must resolve from `ThemeTokens.metrics` (scaled by `apply_ui_font_scale`) so user accessibility font-size settings apply uniformly across every panel, badge, and table cell.

## Layout

- **Single-Horizon Top Grid (`1A`)**:
  - Window drag/traffic-light clearance + top workspace chrome snap cleanly to a unified `34px–36px` horizontal header baseline (`tabbar_height: 34.0`, `sidebar_header_height: 34.0`, macOS titlebar seamlessly aligned with side column headers; empty workspace hides redundant multi-tab placeholder noise like `"全部标签 · 0"` unless scope filtering is active).
- **Fixed Dual Tool Rails (User-Locked Shell)**:
  - **Left Rail (`42px`)**: Top section icons (`Sessions`, `Connections`, `Runtime`, `Extensions`, `Cloud Sync`, `Knowledge`) + hairline divider + bottom utility stack (`Workspace`, `Files`, `Notifications`, `Settings`).
  - **Right Rail (`42px`)**: Aligned context collapse toggle header, followed by 11 Host Tools separated into 3 semantic groups (`Telemetry`, `Services & Network`, `Environment`) via centered `16px × 1px` dividers, with `OxideSens AI` and `App Lock` anchored cleanly at the bottom.
- **Desktop Control Sizing**:
  - Primary inputs & buttons: `32px` height (`ui_control_height: 32.0`, `form_input_height: 32.0`, `form_button_height: 32.0`).
  - Compact toolbar & modal actions: `28px` height (`ui_button_sm_height: 28.0`).
  - Dense table / tree / host-tool rows: `26px–28px` height with predictable column widths (`monospace_datum` right-aligned numeric columns).
- **Split Workbenches (SFTP / IDE / Host Monitor)**: Flush full-bleed `0px` outer padding against `1px` `0x59` split hairlines; zero nested bordered panel boxes inside panels.
- **Welcome Quick-Launch (`3B`)**: Compact centered workspace (`max_w: 760px`) pairing a restrained header + clear primary CTA (`New Connection`) + compact secondary tool actions with a 3–4 item **Pinned / Recent Environments quick-launch list** featuring crisp endpoint alignment and immediate single-click connect, clearly differentiated from the full left session tree and free of terminal-cursor costumes or glow shadows.

## Elevation & Depth

**The Flat-At-Rest Rule (`Z = 0` vs `Z > 0`).**
- **In-plane surfaces (`Z = 0`) carry zero box shadows**: Main chrome, sidebars, tabs, welcome cards, tool inspectors, SFTP split panes, and inline gauges must never render `shadow_sm`, `shadow_md`, `shadow_lg`, or glow `BoxShadow`s (`SurfaceKind::Inspector` shadow removed). Depth comes exclusively from unified ground + `bg_sunken` / `bg_hover` neutral steps and `<= 1px` low-alpha hairlines.
- **Floating overlays (`Z > 0`) use centralized elevation**: Modals, select popovers, context menus, command palette, tooltips, and toasts render over `MaterialRole` surfaces with `theme_overlay_surface_shadow`.

## Shapes

- **Control & Tab Radius**: Crisp desktop geometry (`sm: 4px` for small chips/row buttons/checkboxes, `md: 6px` for inputs/buttons/rail active pills, `lg: 8px` for modals/floating popovers).
- **Workspace Tabs**: Full-height flush rectangular tabs (`0px` bottom corner seam blending directly into workspace canvas, `2px` crisp top accent indicator for the active tab, close `×` disclosed on tab hover or active state).

## Components

### Rail Icon Buttons (Left & Right Rails)
- **Geometry**: Centered `30×30px` hit target inside `42px` rail, `6px` (`md`) radius.
- **Idle State**: Subdued foreground icon (`text_muted` at `0.78–0.85` weight), zero background fill, zero border box.
- **Hover State**: Soft `ui.bg_hover` neutral capsule, crisp `ui.text` foreground.
- **Active State**: Subtle `ui.bg_active` / low-alpha tinted backing (`0x18–0x22`) + clean `2px × 16px` vertical `ui.accent` indicator anchored to the outer edge (left edge on Left Rail, right edge on Right Rail).

### Buttons & Inputs
- **Standard Height**: `32px` (`28px` compact toolbar variant); **Primary CTA** uses `ui.accent` fill with verified contrast `ui.accent_text`; **Secondary** uses clean `Outline` (`1px` `ui.border` at `0x66`) over `ui.bg`; **Tertiary / Icon actions** use `Ghost` (transparent at rest, `bg_hover` on hover).
- **Inputs (`text_input.rs`)**: `ui.bg_sunken` fill, subtle `1px` border (`0x59`), `2px` `ui.accent` focus border/ring on focus, consistent `FocusHandle` + `tab_index`.

### Dense Table & Host-Tool Rows
- **At Rest**: Clean text + tabular monospace metric values (`right-aligned`), `1px` low-alpha bottom hairline (`0x40`–`0x59`).
- **Progressive Actions**: Inline row actions (kill, restart, logs, terminal exec, remove) stay hidden (`opacity: 0.0`) until `.group_hover(...)`, keyboard focus-within, or active/expanded row state. Destructive icons appear ghost at rest and reveal `ui.error` tint on hover only.

## Do's and Don'ts

### Do:
- **Do** align top chrome headers across Left Rail, Left Sidebar, Center Stage/Tabbar, and Right Inspector on a shared `34px–36px` grid (`1A`).
- **Do** keep chrome, rails, sidebars, and workspaces on unified `ui.bg` (`2B`) and distinguish controls through `bg_sunken`, `bg_hover`, hairline dividers, and contrast hierarchy.
- **Do** derive all colors from `ThemeTokens.ui` and all text sizes from scaled `ThemeTokens.metrics`.
- **Do** preserve both Left Activity Rail and Right Host-Tools Rail with clear 3-group organization.
- **Do** wire `FocusHandle`, `tab_index`, and keyboard focus rings (`tauri_focus_visible_ring`) on interactive primitives.

### Don't:
- **Don't** add `BoxShadow` / `.shadow_*()` to in-plane cards, toolbars, zoom chips, brand wordmarks, or inspectors.
- **Don't** hardcode Tailwind hex colors (`0x34d399`, `0xef4444`, etc.) or unscaled `px(10.0)`/`px(11.0)` text sizes.
- **Don't** paint permanent filled grey/red square icon backgrounds at rest inside table rows.
- **Don't** use glowing terminal cursors or decorative badge halos as hero artwork on the Welcome screen.
- **Don't** show raw empty tab counters like `"全部标签 · 0"` as prominent centered titlebar chrome when the workspace is in its initial start state.
