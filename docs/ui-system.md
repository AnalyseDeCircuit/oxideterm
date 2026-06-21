# OxideTerm UI System

Status: active
Owner: GPUI/native app maintainers
Last updated: 2026-06-21

This document defines the direction for making OxideTerm's UI feel more modern
without turning every screen into a redesign project. It is intentionally
engineering-oriented: it names the current primitives, the layout patterns we
should converge on, and the migration rules that keep changes small.

## Product Position

OxideTerm is a terminal-first native developer cockpit. The interface should
feel fast, dense, and calm. It should not look like a landing page, a settings
gallery, or a stack of decorative cards.

Use these principles when changing UI:

1. The terminal owns the canvas.
   Secondary UI should help the active session, not compete with it.

2. Results come before controls.
   If a surface exists to inspect state, show the state first and put actions
   near the affected result.

3. Density is a feature.
   Prefer compact rows, grouped toolbars, split inspectors, and inline metadata
   over large empty cards.

4. Shared primitives beat local chrome.
   A feature may own behavior and data mapping, but repeated visual shells
   belong in `crates/oxideterm-gpui-ui`.

5. Tokens beat raw styling.
   Use `ThemeTokens`, semantic color roles, radii, metrics, and spacing. New raw
   `rgba(0x...)` values should be limited to protocol colors, terminal palette
   colors, or documented compatibility cases.

6. Machine text is monospace.
   Paths, commands, branches, SHAs, remotes, host strings, ports, env vars,
   package names, and file names should use the terminal mono family or a shared
   `MonospaceDatum` primitive.

7. One scroll owner per surface.
   Scroll-heavy surfaces should use `List`/`ListState` or the shared scrollbar
   wrapper. Avoid nested overflow containers that hide rows or make overlays
   stale.

8. Accent is a signal, not decoration.
   Orange should mean selected, primary, active, or warning-like emphasis. Do
   not outline whole panels in accent by default.

## Current Inventory

### Theme and token layer

The app already has a central token model:

- `crates/oxideterm-theme/src/lib.rs`
  - `AppUiColors` defines `bg`, `bg_panel`, `bg_card`, `bg_hover`,
    `bg_active`, `bg_secondary`, `bg_elevated`, `bg_sunken`, text roles,
    border roles, accent roles, and status colors.
  - `UiMetrics` defines titlebar, activity bar, sidebar, modal, form, control,
    menu, select, slider, tooltip, toast, and command metrics.
  - `UiRadii` defines `xs`, `sm`, `md`, `lg`, and `active_indicator`.
  - `UiSpacing` defines the current 4/8/12 px base scale.
- `crates/oxideterm-gpui-app/src/workspace/root/helpers.rs`
  - `tokens_from_settings` maps the user-configured border radius onto
    `UiRadii`, so radius-sensitive components should not bake in fixed values.

### Shared GPUI UI crate

The shared UI crate already exposes many primitives:

- `crates/oxideterm-gpui-ui/src/button.rs`
  - `button`, `icon_button`, `toolbar_button`, focus rings, button variants,
    disabled/loading guards, compact toolbar options.
- `crates/oxideterm-gpui-ui/src/menu.rs`
  - menu content, items, labels, separators, shortcuts.
- `crates/oxideterm-gpui-ui/src/context_menu.rs`
  - context menu backdrops, rows, actionable styles, dismissal behavior.
- `crates/oxideterm-gpui-ui/src/command.rs`
  - command palette shell, input, list, group, item, separator, shortcut.
- `crates/oxideterm-gpui-ui/src/table.rs`
  - table header, row, cells, sortable header, mono metadata cells.
- `crates/oxideterm-gpui-ui/src/scroll.rs`
  - GPUI native scroll tracking, visible scrollbar layers, and
    `ScrollViewportContract` declarations for virtual lists, tracked overflow,
    terminal scroll, and horizontal tab scrollers.
- `crates/oxideterm-gpui-ui/src/surface.rs`
  - `semantic_surface`, `surface_chrome`, semantic surface kinds, and legacy
    Tauri-card color/shadow compatibility helpers.
- `crates/oxideterm-gpui-ui/src/entity_row.rs`
  - `entity_list_row` for compact entity/result rows with dedicated leading,
    title, subtitle, badge, and trailing action slots.
- `crates/oxideterm-gpui-ui/src/command_panel.rs`
  - `command_panel` and `command_panel_body` for terminal-owned and workspace
    command popovers.
- `crates/oxideterm-gpui-ui/src/section.rs`
  - `section_header` for compact panel and inspector sections.
- `crates/oxideterm-gpui-ui/src/typography.rs`
  - `monospace_datum` and middle truncation for paths, branches, hosts,
    commands, and other machine-readable values.
- `crates/oxideterm-gpui-ui/src/state.rs`
  - empty, loading, error, inline empty, and notice states.
- `crates/oxideterm-gpui-ui/src/modal.rs`, `dialog.rs`, `confirm.rs`,
  `select.rs`, `slider.rs`, `checkbox.rs`, `radio_group.rs`, `tabs.rs`,
  `toast.rs`, `tooltip.rs`, `text_input.rs`, `badge.rs`, and `tree.rs`.
- `crates/oxideterm-gpui-ui/src/ai/*`
  - AI-specific chat, inline panel, agent, model selector, indicators, and tool
    call primitives.

This means the missing piece is not "invent a UI library". The missing piece is
semantic convergence: page surfaces, entity rows, compact inspectors, project
chips, and popover shells should stop being recreated per feature.

### Native convergence already applied

The following GPUI surfaces are now expected to use the shared primitives:

- Terminal command bar popovers: Git, current directory, and project tasks use
  `command_panel`; core candidate/header rows use `entity_list_row`.
- Quick Commands: the popover shell uses `command_panel`, and risk/source
  metadata uses `status_pill`.
- Completion suggestions: risk/source metadata uses `status_pill`.
- Cloud Sync: the main plugin card uses `semantic_surface`, and status/diff/
  health indicators use `status_pill`.
- Plugin Manager: action, installed, and package-manager cards use
  `semantic_surface`; plugin runtime states use `status_pill`.
- Settings: `settings_card_surface` applies shared `surface_chrome` so settings
  cards inherit the same semantic frame.
- Runtime and Connection Pool: overview stat cards and connection cards use
  shared surfaces; pool states use `status_pill`.

When adding a new GPUI screen, start from these primitives before creating
feature-local rounded/border/background shells.

### App-local helper layer

The app crate still owns behavior-aware wrappers:

- `crates/oxideterm-gpui-app/src/workspace/root/helpers.rs`
  - `workspace_icon_action_button`, `workspace_toolbar_action_button`,
    `workspace_context_menu_*`, tooltip scheduling, disabled-event guards.
- `crates/oxideterm-gpui-app/src/workspace/settings/cards.rs`
  - `settings_card`, `plain_settings_card`, `card_title`, `card_separator`,
    `settings_card_surface`.

These helpers are useful, but their names and responsibilities show the
migration history. Future work should keep app-specific event wiring here while
moving reusable shape, spacing, state, and row composition into
`oxideterm-gpui-ui`.

## Design Audit

### Major: card stacking makes operational UI feel older than it is

Evidence:

- Settings pages use `settings_card` and many page-specific subcards.
- Cloud Sync has many `render_cloud_sync_*_card` functions in
  `crates/oxideterm-gpui-app/src/workspace/cloud_sync.rs`.
- Plugin Manager has action, installed, browse, registry, diagnostic, and
  detail cards in `crates/oxideterm-gpui-app/src/workspace/plugin_manager.rs`.
- Session import/export dialogs have nested `render_oxide_*_card` and subcard
  helpers.

Fix:

- Use cards only for repeated entities, modals, and genuinely framed tools.
- Page sections should usually be unframed bands or panels with a constrained
  inner width.
- In settings, prefer rows inside one panel over card-per-control.
- In entity managers, prefer list rows plus an optional inspector over
  repeating large cards.

### Major: local surface chrome drifts by feature

Evidence:

- `terminal_command_bar.rs`, `plugin_manager.rs`, `cloud_sync.rs`, settings
  pages, SFTP, and session manager all compose local rounded borders, shadow,
  alpha backgrounds, and hover states.
- The shared `surface.rs` still exposes `tauri_card_surface` and
  `tauri_card_shadow`, which is useful for compatibility but should not be the
  semantic name of new native UI.

Fix:

- Add a semantic surface primitive in `oxideterm-gpui-ui`:
  `SurfaceKind::Panel`, `SurfaceKind::ElevatedPopover`,
  `SurfaceKind::InsetGroup`, `SurfaceKind::EntityRow`,
  `SurfaceKind::Inspector`, and `SurfaceKind::TerminalOverlay`.
- Keep the old Tauri helpers as compatibility wrappers until every caller has a
  native semantic owner.

### Major: command and state surfaces need result-oriented anatomy

Evidence:

- Recent Git and environment work exposed a repeated problem: actions, command
  names, and large containers can crowd out the actual state the user asked to
  inspect.
- Command-like panels currently mix "run this command" affordances with
  "show this result" affordances.

Fix:

- Use a stable anatomy for command-adjacent panels:
  header, search/filter if needed, result list, inline actions, optional
  command preview in a muted mono slot.
- If the user clicked a result, keep the panel open unless the action explicitly
  navigates away or executes a terminal command.
- If an item is an action, show a button/icon. If it is a result, show the
  result first and make any command secondary.

### Major: scroll behavior should be a layout decision, not an afterthought

Evidence:

- `crates/oxideterm-gpui-app/src/workspace/virtual_list.rs` already contains
  the shared `ListState` path.
- Many surfaces use `ListState`, but some still use plain overflow or nested
  scroll containers.
- `crates/oxideterm-gpui-ui/src/scroll.rs` provides scrollbar layers, but each
  surface still decides when to use them.

Fix:

- Every page, popover, and dialog should declare one scroll owner.
- Variable-height or heavy surfaces should use `ListState`.
- Lightweight overflow surfaces should use
  `overflow_y_scroll().track_scroll(...)` or the shared scrollbar wrapper.
- Overlay position must be anchored to the scroll owner when the overlay can
  remain open during scrolling.

### Minor: typography roles are not explicit enough

Evidence:

- `table.rs` already supports `TauriTableCellStyle::MetaMono`.
- Git/environment UI, terminal command bar, quick commands, Cloud Sync, Plugin
  Manager, SFTP, and settings all show paths, commands, branch names, package
  names, host strings, or files.

Fix:

- Add a shared `monospace_datum`/`code_text` primitive instead of repeating
  `font_family(settings_mono_font_family(...))` or leaving machine text in the
  UI family.
- Use UI font for labels, titles, statuses, and explanations.
- Use mono for data copied from terminals, filesystems, Git, package managers,
  runtimes, and network endpoints.

### Minor: the accent role is overloaded

Evidence:

- Several surfaces use accent borders or orange-heavy shells for whole cards,
  popovers, and selected sections.

Fix:

- Accent may mark the active tab, selected row, primary action, focused input,
  alert badge, or small leading indicator.
- Neutral panel borders should remain neutral.
- Destructive, warning, success, and info states should use their semantic
  colors instead of accent.

## Layout Archetypes

### Terminal canvas

Use for terminal panes, splits, and overlays that are part of the active
session.

Rules:

- Keep chrome minimal and attached to the terminal edge.
- Bottom bars should be compact and stable in height.
- Terminal overlays should never hide the prompt or active selection without a
  clear reason.
- Scrollbars and overlays must track terminal scroll smoothly.

### Command popover

Use for command palette, quick commands, branch picker, cwd picker, project task
picker, file chooser, and similar transient selection panels.

Rules:

- Width adapts to content but has sensible min/max bounds.
- Search sits at the top when filtering is useful.
- Results are keyboard navigable.
- Rows should not execute destructive or surprising actions on simple
  selection.
- Keep result text visible; put command previews in a muted mono trailing slot.

### Entity manager page

Use for Cloud Sync, Plugin Manager, Runtime, Session Manager, and future
managers.

Rules:

- Header: title, short status, primary actions.
- Body: list/table of entities, not card soup.
- Inspector: details for the selected entity or expandable row content.
- Empty/error/loading states use `state.rs` primitives.
- A page may use one prominent summary panel, but not a stack of unrelated
  decorative cards.

### Settings page

Use for preferences that persist.

Rules:

- Left navigation stays compact.
- Right content uses sections and rows.
- One section may be a panel; every setting should not become its own card.
- Controls align in a predictable trailing column.
- Long descriptions should be rare and muted.

### Companion sidebar

Use for AI, session lists, host tools, logs, and future side panels.

Rules:

- Treat vertical space as scarce.
- Prefer compact rows, tabs, filters, and progressive disclosure.
- Avoid large banners and repeated rounded cards.
- Entity names remain highest-priority text when width is tight.

### Modal and inspector

Use for bounded workflows and detail views.

Rules:

- Header says what object/workflow is being edited.
- Body owns a single scroll container.
- Footer actions are stable and do not jump.
- Destructive actions are separated from primary save/confirm actions.

## Component Backlog

Implement these in `crates/oxideterm-gpui-ui` before broad visual rewrites.

### `Surface`

Purpose: replace ad hoc panel/card/popover styling.

Kinds:

- `Panel`: persistent page or sidebar panel.
- `ElevatedPopover`: command palette, menus, pickers.
- `InsetGroup`: grouped rows inside a panel.
- `EntityRow`: rows in managers and lists.
- `Inspector`: detail pane or expandable entity detail.
- `TerminalOverlay`: terminal-owned popovers and HUDs.

Acceptance:

- Uses `ThemeTokens`.
- Has documented radius, border, background, shadow, and background-image
  behavior.
- Keeps old `tauri_*` helpers as compatibility wrappers, not as new semantic
  API.

### `SectionHeader`

Purpose: consistent title, count, description, and trailing actions.

Use in:

- Settings sections
- entity manager sections
- Git/environment panels
- Cloud Sync lifecycle groups

### `EntityListRow`

Purpose: unify file rows, plugin rows, cloud sync records, Git files, host tool
rows, quick command rows, and session rows where possible.

Slots:

- leading icon
- title
- subtitle/details
- status badges
- trailing action group
- optional expandable body

Rules:

- Title stays visible first.
- Actions do not wrap into awkward second rows unless the row explicitly enters
  a narrow compact mode.
- Row activation and action buttons must not fight for pointer events.

### `CommandPanel`

Purpose: make command-like popovers consistent.

Slots:

- search/input
- optional current context chip
- grouped results
- optional footer/status

Use in:

- command palette
- quick commands
- branch picker
- cwd picker if it returns
- project task picker
- file/directory pickers

### `StatusPill` and `CountBadge`

Purpose: replace one-off badge styling.

Rules:

- Status pills use semantic tones: neutral, accent, success, warning, error,
  info.
- Count badges are compact and never carry long text.
- Machine-readable counts remain mono only when mixed with code-like data.

### `MonospaceDatum`

Purpose: make paths, commands, remotes, branches, SHAs, ports, and host strings
consistent.

Rules:

- Uses the current terminal mono family when available.
- Supports truncation from the middle for paths and remotes.
- Supports copy affordance only where the user expects copying.

### `ScrollViewport`

Purpose: make scroll ownership explicit.

Kinds:

- virtual list
- tracked native overflow
- terminal scroll
- horizontal tab strip

Acceptance:

- Each scroll surface documents whether it supports visible scrollbar, smooth
  thumb updates, keyboard focus, and anchored overlays.

## Migration Roadmap

### Phase 0: inventory and naming cleanup

Do now:

- Keep this document current as surfaces are migrated.
- Add file-level TODOs only when the next owner is clear.
- Avoid new `tauri_*` names for native-only APIs.
- Avoid raw alpha constants in new feature code unless they model an existing
  compatibility contract.

Verification:

- New UI code has a shared primitive or a written reason why it is local.
- New UI copy updates all shipped locale files.

### Phase 1: primitives without redesign

Build:

- `Surface`
- `SectionHeader`
- `EntityListRow`
- `CommandPanel`
- `StatusPill`/`CountBadge`
- `MonospaceDatum`
- `ScrollViewport` conventions

Verification:

- Existing screens can adopt the primitives without changing feature behavior.
- Screenshots before/after should show small visual convergence, not a full
  redesign.

### Phase 2: high-traffic surfaces

Migrate in this order:

1. Terminal bottom bar, command bar, and terminal overlays.
2. Git/environment popovers and project task UI.
3. Quick Commands.
4. Cloud Sync.
5. Plugin Manager.
6. Runtime/Connection Monitor.
7. Settings sections.

Why this order:

- Terminal-adjacent UI is most visible.
- Git/environment and quick commands expose the weakest current command-panel
  semantics.
- Cloud Sync and Plugin Manager are entity manager pages with repeated cards.
- Settings can be improved gradually once row and section primitives are stable.

### Phase 3: remove duplicate local chrome

After Phase 2:

- Delete feature-local panel/card helpers that have semantic equivalents.
- Keep behavior wrappers in `WorkspaceApp` only for app state, events,
  workspace overlay dismissal, and tooltip scheduling.
- Rename remaining compatibility helpers or mark them as compatibility-only.

### Phase 4: visual verification

Every migrated surface should be checked in at least these states:

- empty
- loading
- populated
- error
- disabled/unavailable actions
- narrow width
- wide width
- with terminal background image enabled
- with long CJK labels
- with long paths/commands/remotes

## File-Level Targets

Use this map when choosing the next cleanup slice.

| Area | Current files | Direction |
| --- | --- | --- |
| Shared primitives | `crates/oxideterm-gpui-ui/src/*` | Add semantic native primitives instead of feature-local chrome. |
| Tokens | `crates/oxideterm-theme/src/lib.rs` | Keep semantic roles; avoid adding one-off colors for a single screen. |
| Workspace wrappers | `crates/oxideterm-gpui-app/src/workspace/root/helpers.rs` | Keep behavior wrappers; push reusable visuals into the UI crate. |
| Settings cards | `crates/oxideterm-gpui-app/src/workspace/settings/cards.rs` | Move from card-first layout to section/row layout. |
| Terminal command UI | `crates/oxideterm-gpui-app/src/workspace/terminal_command_bar.rs` | Convert repeated popover and action-row styling to `CommandPanel`. |
| Git/environment UI | `crates/oxideterm-gpui-app/src/workspace/terminal_git.rs`, `terminal_project.rs`, `terminal_cwd.rs` | Use command-panel and mono data primitives. |
| Quick Commands | `crates/oxideterm-gpui-app/src/workspace/quick_commands_*` | Keep pin/play semantics clear; use entity rows and command panels. |
| Cloud Sync | `crates/oxideterm-gpui-app/src/workspace/cloud_sync.rs` | Treat as entity manager plus lifecycle inspector. |
| Plugin Manager | `crates/oxideterm-gpui-app/src/workspace/plugin_manager.rs` | Treat installed/browse/update rows as entity lists. |
| Connection Monitor | `crates/oxideterm-gpui-app/src/workspace/connection_monitor/*` | Use responsive table/entity rows with ListState. |
| SFTP/File Manager | `crates/oxideterm-gpui-app/src/workspace/sftp/*`, `file_manager/*` | Keep file rows dense; share preview/toolbars where possible. |
| Session Manager | `crates/oxideterm-gpui-app/src/workspace/session_manager/*` | Reduce nested import/export card stacks after shared rows exist. |

## New UI Checklist

Before a GPUI UI change is considered ready:

- It has one clear layout archetype from this document.
- It uses `ThemeTokens` and shared primitives.
- Any local visual helper has a documented reason to remain local.
- It avoids nested cards unless the inner card is a repeated item, modal, or
  framed tool.
- It has one scroll owner.
- It keeps overlays anchored to the correct scroll owner.
- It uses mono text for paths, commands, branches, SHAs, remotes, hosts, ports,
  files, packages, and environment variables.
- It handles long CJK text and long machine strings.
- It keeps button text on one line or switches to a documented compact mode.
- It uses semantic status colors instead of accent for every state.
- It has empty, loading, populated, disabled, and error states where applicable.
- It updates all locale files for any user-facing copy.
- It is verified with terminal background image enabled when translucency or
  card opacity is touched.

## What Not To Do

- Do not redesign every page at once.
- Do not add a second design system beside `oxideterm-gpui-ui`.
- Do not copy layouts or code from other terminals or editors.
- Do not keep introducing feature-local cards, badges, and row shapes when the
  shared UI crate can own them.
- Do not make modernity mean lower information density.
- Do not make the accent color carry the whole visual identity.
- Do not hide operational data behind decorative hero sections or large empty
  surfaces.
