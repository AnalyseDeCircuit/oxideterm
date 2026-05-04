# OxideTerm Native Settings Page Plan

This plan describes how to translate the Tauri settings experience into the
native GPUI application without turning `oxideterm-native` into another large
catch-all crate.

The source of truth for behavior and visual structure is the Tauri app, but
Tauri's backend `settings.json` is not a complete settings schema. Native must
not treat that file, or the locale `settings.json` files, as exhaustive. The
native settings model must be the superset produced by auditing the store,
frontend-local persisted state, backend app-settings commands, and every visible
settings control.

- `/Users/dominical/Documents/OxideTerm-main/src/components/settings/SettingsView.tsx`
- `/Users/dominical/Documents/OxideTerm-main/src/store/settingsStore.ts`
- `/Users/dominical/Documents/OxideTerm-main/src/components/settings/tabs/*.tsx`
- `/Users/dominical/Documents/OxideTerm-main/src-tauri/src/commands/app_settings.rs`
- `/Users/dominical/Documents/OxideTerm-main/src/locales/*/settings.json`
- `/Users/dominical/Documents/OxideTerm-main/src/locales/*/settings_view.json`

Known frontend-persisted sources that Phase 1 must inventory and either migrate
or intentionally keep out of the app settings schema:

- `oxide-settings-v2`
- legacy `oxide-settings`
- legacy `oxide-ui-state`
- legacy `oxide-tree-expanded`
- legacy `oxide-focused-node`
- `app_lang`
- `oxideterm_keybindings`
- custom theme storage in `src/lib/themes.ts`
- launcher enablement storage in `src/store/launcherStore.ts`
- agent-role storage in `src/store/agentRolesStore.ts`
- modal-local defaults such as `NewConnectionModal`'s save-connection choice
- export/import timestamps and other non-settings operational metadata

Before coding a field, run and reconcile source inventory with searches like:

- `rg -n "localStorage|sessionStorage|STORAGE_KEY|LEGACY_KEYS" /Users/dominical/Documents/OxideTerm-main/src`
- `rg -n "loadAppSettings|saveAppSettings|applyAppSettingsSnapshot|buildOxideAppSettingsSectionValueMap" /Users/dominical/Documents/OxideTerm-main/src /Users/dominical/Documents/OxideTerm-main/src-tauri/src`
- `rg -n "useSettingsStore|settings\\." /Users/dominical/Documents/OxideTerm-main/src/components /Users/dominical/Documents/OxideTerm-main/src/store`

Native implementation must translate the behavior and UI relationships into
Rust/GPUI. It must not invent new settings organization, copy raw screenshots by
eye, or bypass the system invariants in `docs/SYSTEM_INVARIANTS.md`.

## Goals

- Add a native Settings surface that matches the Tauri settings page structure:
  left settings navigation, right scrollable content, tabbed sections, and
  reusable form controls.
- Move settings data into a dedicated crate so the workspace, terminal renderer,
  SSH stack, theme layer, and future frontends can share one settings model.
- Consolidate settings that Tauri currently splits between backend
  `settings.json`, `settingsStore.ts`, and frontend `localStorage` into one
  native schema with explicit migrations.
- Support runtime i18n, theme tokens, UI radius/density settings, terminal
  display/input settings, connection defaults, SSH/reconnect settings, and local
  terminal settings as first-class native state.
- When this plan is complete, every setting exposed by the Tauri settings UI
  must exist in native settings UI and persistence, even if the subsystem it
  controls is not implemented yet.
- Separate "setting exists and persists" from "setting has live runtime effect".
  Missing runtime behavior is acceptable only when the UI clearly records and
  persists the value for future use.

## Non-Goals For The First Pass

- Do not fully implement every backend feature controlled by settings. Some
  settings will be stored before the feature is wired.
- Do not implement heavyweight management surfaces as fully functional backends
  in the first pass, such as Knowledge document indexing, MCP server runtime
  management, custom theme file import/export, or portable credential dialogs.
  Their user-facing settings, toggles, and persisted fields still must exist.
- Do not build a new visual language. Translate Tauri primitives into semantic
  native tokens and reusable native controls.
- Do not store settings only inside `oxideterm-native`; settings must live in a
  shared crate.

## Phase 1: Settings Core Crate And Persistence

Create `crates/oxideterm-settings`.

Responsibilities:

- Build a settings inventory table before finalizing the Rust structs:
  - source file and storage key
  - setting path
  - default value
  - visible UI control
  - persistence location in Tauri
  - native field name
  - migration behavior
  - runtime effect status
- Define Rust equivalents of the Tauri `PersistedSettingsV2` model:
  - `GeneralSettings`
  - `TerminalSettings`
  - `TerminalAutosuggestSettings`
  - `TerminalCommandBarSettings`
  - `TerminalCommandMarksSettings`
  - `BufferSettings`
  - `AppearanceSettings`
  - `ConnectionDefaults`
  - `TreeUiState`
  - `SidebarUiState`
  - `LocalTerminalSettings`
  - `SftpSettings`
  - `IdeSettings`
  - `ReconnectSettings`
  - `ConnectionPoolSettings`
  - `ExperimentalSettings`
  - AI provider/context/memory/reasoning/tool-use settings
  - MCP server settings
  - embedding settings
  - AI execution profile settings
  - command palette MRU
  - onboarding state
- Keep enum names and values aligned with Tauri persistence:
  - language locale ids
  - terminal font family ids
  - cursor styles
  - renderer modes
  - terminal encodings
  - UI density
  - animation speed
  - frosted glass mode
  - SFTP conflict action
  - IDE agent mode
  - AI thinking style
  - AI reasoning effort
  - tool approval policy keys
- Implement defaults equivalent to `settingsStore.ts`.
- Implement migrations from Tauri's frontend-local persisted state where it is
  part of user settings:
  - read legacy JSON shapes in tests as fixtures
  - migrate `oxide-settings-v2` into the native settings file
  - migrate `app_lang` into `general.language` when no stronger value exists
  - migrate deprecated sidebar/tree keys into `sidebarUI`/`treeUI`
  - migrate keybinding overrides into the native keybinding settings namespace
  - migrate custom themes into the native theme settings namespace
  - leave operational metadata, such as last export timestamp, outside settings
    unless a settings tab visibly exposes it
- Implement normalization helpers:
  - clamp terminal font size, line height, and scrollback
  - normalize terminal encoding
  - normalize sidebar widths
  - normalize connection pool idle timeout
  - preserve unknown future fields only if a forward-compatible wrapper is
    introduced; otherwise bump schema intentionally
- Implement JSON load/save:
  - load from app data directory when available
  - fall back to defaults on missing file
  - keep corrupt-file handling explicit and user visible later
- Add tests:
  - defaults match expected Tauri values
  - all enums serialize to Tauri-compatible strings
  - invalid numeric values normalize safely
  - `serde_json` round trip preserves stable fields
  - legacy localStorage fixtures migrate into the new schema
  - frontend-only settings are represented or explicitly classified as
    non-settings metadata
  - locale fallback handles both `settings.general.language` and old `app_lang`

Native integration in this phase:

- Add the crate to workspace `Cargo.toml`.
- Add `SettingsStore` or equivalent owner inside native workspace state.
- Load settings during `WorkspaceApp::new`.
- Apply these settings immediately:
  - locale into `I18n`
  - sidebar collapsed/default width
  - theme id and UI token overrides
  - terminal font size, line height, cursor blink, copy-on-select, and
    scrollback where supported
  - SSH connection pool idle timeout

Exit criteria:

- `cargo test -p oxideterm-settings`
- `cargo check -p oxideterm-native`
- Native startup uses settings defaults from the new crate, not scattered
  constants.
- The crate can serialize/deserialize every Tauri persisted settings field and
  every frontend-local user setting found by the source inventory. Any
  deliberately unsupported source must be explicitly documented in this file
  before implementation proceeds.
- `settings.json` from Tauri backend and locale files are treated as partial
  inputs only; Phase 1 is not accepted if any visible settings control exists
  without a native persisted field.

## Phase 2: Native Settings Shell And Reusable Controls

Add a settings UI module in native first. If it grows, promote the reusable GPUI
controls into a later `oxideterm-gpui-ui` crate.

Suggested file layout:

- `crates/oxideterm-native/src/settings.rs`
- `crates/oxideterm-native/src/settings/shell.rs`
- `crates/oxideterm-native/src/settings/state.rs`
- `crates/oxideterm-native/src/settings/controls.rs`
- `crates/oxideterm-native/src/settings/tabs/general.rs`
- `crates/oxideterm-native/src/settings/tabs/appearance.rs`
- `crates/oxideterm-native/src/settings/tabs/terminal.rs`
- `crates/oxideterm-native/src/settings/tabs/connections.rs`
- `crates/oxideterm-native/src/settings/tabs/ssh.rs`
- `crates/oxideterm-native/src/settings/tabs/reconnect.rs`
- `crates/oxideterm-native/src/settings/tabs/local_terminal.rs`
- `crates/oxideterm-native/src/settings/tabs/sftp.rs`
- `crates/oxideterm-native/src/settings/tabs/ide.rs`
- `crates/oxideterm-native/src/settings/tabs/ai.rs`
- `crates/oxideterm-native/src/settings/tabs/knowledge.rs`
- `crates/oxideterm-native/src/settings/tabs/keybindings.rs`
- `crates/oxideterm-native/src/settings/tabs/portable.rs`
- `crates/oxideterm-native/src/settings/tabs/help.rs`

UI shell:

- Match Tauri `SettingsView.tsx` structure:
  - full-height settings surface
  - left navigation width `w-56` equivalent
  - panel background and border from theme tokens
  - title area with `settings_view.title`
  - grouped navigation with separators
  - right content area with max width equivalent to `max-w-4xl`
  - content padding equivalent to `p-10`
- Add settings tabs in this order:
  - General
  - Portable
  - Terminal
  - Appearance
  - Local
  - Connections
  - SSH
  - Reconnect
  - SFTP
  - IDE
  - AI
  - Knowledge
  - Keybindings
  - Help
- If a specific control cannot perform its final side effect yet, render the real
  setting control anyway, persist its value, and mark the side effect as not yet
  wired in implementation notes. Avoid fake placeholder-only tabs except for
  pure management lists that have no settings fields.

Reusable controls:

- `SettingsSectionHeader`
- `SettingsCard`
- `SettingsRow`
- `SettingsSeparator`
- `SettingsButton`
- `SettingsIconButton`
- `SettingsTextInput`
- `SettingsNumberInput`
- `SettingsSelect`
- `SettingsCheckbox`
- `SettingsSlider`
- `SettingsSegmentedTabs`
- `SettingsCodeBlock`

Control rules:

- All colors come from `ThemeTokens`.
- All radii come from `ThemeTokens.radii`.
- All reusable sizes and gaps come from metric/spacing tokens.
- Text input must use actual committed text (`key_char`), not keybinding names.
- Controls that edit text must own focus and block terminal input.
- Slider updates that are visually continuous but expensive to persist should
  debounce persistence the same way Tauri does.

Actions and routing:

- Add a workspace state for `ActiveSurface::Terminal | Settings`.
- Connect menu/action `Cmd+,` to open settings.
- Sidebar settings icon should open settings.
- Escape from settings returns to the terminal workspace when no child modal or
  text input has captured Escape.
- Settings changes should notify and apply to live views where possible.

Exit criteria:

- Settings shell opens and closes without stealing terminal input after close.
- Sidebar navigation visually matches Tauri structure.
- All controls are tokenized.
- Every Tauri settings tab has a native tab route.
- Tabs with implemented settings fields use real controls, not empty
  placeholders.
- `cargo check -p oxideterm-native`

Phase 2 implementation status (2026-05-04):

- Implemented in `crates/oxideterm-native/src/workspace/settings.rs` as a
  workspace child module so settings routing, focus handoff, live token updates,
  and persistence stay coordinated with the root workspace entity.
- Added `ActiveSurface::Terminal | Settings`, `Cmd+,`, the macOS Settings menu
  item, and the activity-bar Settings icon route.
- Added the Tauri `SettingsView.tsx` shell translation: left `w-56` equivalent
  navigation, grouped nav sections with separators, `settings_view.title`, right
  scrollable `max-w-4xl` / `p-10` equivalent content area, and the complete tab
  order from Tauri.
- Added native routes for General, Portable, Terminal, Appearance, Local,
  Connections, SSH, Reconnect, SFTP, IDE, AI, Knowledge, Keybindings, and Help.
- Added real persisted controls for the settings already represented in
  `oxideterm-settings`, including common terminal, appearance, local terminal,
  connection pool, reconnect, SFTP, IDE, and AI fields. Rich editors and
  management tables remain Phase 3 work, but the routes are no longer empty
  placeholders.
- Imported Tauri `settings.json` and `settings_view.json` locale domains for all
  11 native locales and wired them into `oxideterm-i18n`.
- Verified with `cargo fmt --check`, `cargo check -p oxideterm-native`,
  `cargo test -p oxideterm-i18n`, and `cargo test -p oxideterm-native`.

## Phase 3: Complete Settings Surface

Implement every Tauri settings tab as a native settings surface. A control is
considered present when it is visible, localized, editable, persisted, and wired
to the shared settings model. Runtime side effects may be marked as pending when
the controlled subsystem does not exist yet.

### General

Source: `GeneralTab.tsx`.

- Language select with all 11 autonyms.
- Data directory display and controls. If native backend support is not ready,
  keep the controls disabled with localized explanation, but preserve the layout.
- CLI companion status/actions. If native backend support is not ready, keep the
  status/action rows present and disabled.

Live behavior:

- Changing language updates `I18n` immediately.
- New user-facing settings strings must be added to all 11 locale domain files.

### Appearance

Source: `AppearanceTab.tsx`.

- Theme select using existing `oxideterm-theme` built-ins.
- Theme preview.
- UI density select.
- Global border radius slider.
- UI font family input.
- Animation speed select.
- Frosted glass select, even if some modes are no-ops on native initially.
- Background image settings surface:
  - enable background
  - image path/display
  - opacity
  - blur
  - fit
  - enabled tab types
  If file picking/import is not ready, keep the value field and controls
  present, with file operations disabled.
- Custom theme create/edit/import/export controls may be present but disabled
  until custom theme persistence is implemented.

Live behavior:

- Theme changes rebuild native `ThemeTokens` and update workspace/sidebar/forms.
- Border radius changes update radius tokens live.
- UI density changes update spacing/metric scale live where implemented.

### Terminal

Source: `TerminalTab.tsx`.

- Internal page switcher: Display, Input, Command Bar, History, Transfer,
  Highlight.
- Display:
  - font family
  - custom font stack
  - font preview
  - font size
  - line height
  - encoding
  - renderer mode as a compatibility setting, even if native ignores WebGL/Canvas
- Input:
  - cursor style
  - cursor blink
  - paste protection placeholder if not implemented
  - smart copy
  - OSC 52
  - copy on select
  - middle click paste
  - selection requires shift
- History:
  - scrollback
  - backend buffer max lines if still relevant
- Transfer and Highlight:
  - in-band transfer enable/provider/limits
  - highlight rule list editor or read-only list with add/edit disabled until
    the editor exists
- Command Bar:
  - enabled
  - show legacy toolbar
  - git status
  - smart completion
  - quick commands settings
  - focus handoff command list
- Command Marks:
  - enabled
  - user input observed
  - heuristic detection
  - hover actions

Live behavior:

- Font size and line height remeasure terminal metrics and resize PTY through the
  existing debounced resize path.
- Cursor blink updates `TerminalPane` behavior immediately.
- Copy/paste settings update `TerminalUiSettings`.

### Connections

Source: `ConnectionsTab.tsx`.

- Default username.
- Default port.
- Connection pool idle timeout.
- Groups management surface. If native saved connection store is incomplete,
  keep create/delete controls disabled or local-only, but the rows must exist.
- SSH config import surface. If native importer is incomplete, show the import
  controls disabled with localized explanation.

Live behavior:

- New connection form defaults read from `ConnectionDefaults`.
- SSH registry pool idle timeout updates from settings.

### Reconnect

Source: `ReconnectTab.tsx`.

- Enabled.
- Max attempts.
- Base delay.
- Max delay.

Live behavior:

- Values feed the native reconnect orchestrator when that layer is active.

### Local Terminal

Source: `LocalTerminalSettings.tsx`.

- Default shell id picker. If shell discovery is incomplete, show the current
  value and a disabled picker.
- Default CWD text field.
- Load shell profile.
- Oh My Posh enable and theme path.
- Custom environment variables editor. If the full editor is not ready, provide
  a simple key/value list with add/remove disabled until implemented.

### SSH

Source: `SshTab.tsx` and SSH-related controls in the Tauri settings store.

Required surface:

- Known-host behavior and host-key policy settings that exist in Tauri.
- Auth default preferences that exist in Tauri.
- Key, certificate, agent, keyboard-interactive, 2FA, and agent forwarding
  related defaults that exist in Tauri.
- Any SSH diagnostic or reset controls present in Tauri should be represented,
  disabled if native backend support is pending.

### SFTP

Source: `SftpTab.tsx`.

Required surface:

- Max concurrent transfers.
- Directory parallelism.
- Speed limit enabled.
- Speed limit KB/s.
- Conflict action.

### IDE

Source: `IdeTab.tsx`.

Required surface:

- Auto save.
- Font size override.
- Line height override.
- Agent mode.
- Word wrap.

### Portable

Source: `PortableTab.tsx`.

Required surface:

- Portable runtime status.
- Portable password/credential related settings and actions.
- Biometric binding settings/actions.
- Unsupported native actions may be disabled, but the surface must exist.

### AI

Source: `AiTab.tsx`.

Required surface:

- Master enable and privacy confirmation state.
- Provider list and active provider/model fields.
- Base URL/model legacy fields where still persisted.
- Context max chars and visible lines.
- Context source toggles.
- Thinking style and default-expanded setting.
- Reasoning effort global/provider/model settings.
- Custom system prompt.
- Memory enabled/content.
- Tool-use enabled, max rounds, auto-approve map, disabled tools.
- Embedding config.
- MCP server config surface.
- Agent role configuration and execution profiles.
- Model context windows and max response token overrides may start as editable
  tables or structured text until richer editors are implemented.

### Knowledge

Source: `DocumentManager.tsx` and `EmbeddingConfigSection.tsx`.

Required surface:

- Settings-visible document/knowledge controls that exist in Tauri.
- Embedding configuration entry point.
- If indexing/storage backend is not ready, show the management table disabled
  but keep the settings route and explanatory text.

### Keybindings

Source: `KeybindingEditorSection.tsx` and `lib/keybindingRegistry.ts`.

Required surface:

- Full read-only shortcut list first.
- Editable keybinding controls may be disabled until the keybinding registry is
  made persistent in native.

### Help

Source: `HelpAboutSection.tsx`.

Required surface:

- Version/about information.
- Keyboard shortcut summary.
- Portable mode status if available.

Exit criteria:

- Every tab in Tauri `SettingsView.tsx` has a native counterpart.
- Every Tauri setting field has a visible native control or a documented,
  disabled management row that preserves the field in settings persistence.
- General, Appearance, Terminal, Connections, Reconnect, Local, SSH, SFTP, IDE,
  Portable, AI, Knowledge, Keybindings, and Help routes are all present.
- Values persist even when live runtime behavior is not wired yet.
- Settings with existing native runtime support apply live.
- `cargo test -p oxideterm-settings`
- `cargo check -p oxideterm-native`
- `git diff --check`

Phase 3 implementation status (2026-05-04):

- Checked the settings navigation icons against Tauri `SettingsView.tsx` and
  `lucide-react@0.577.0`. Native now uses the same tab-to-icon mapping:
  `Monitor`, `HardDrive`, `Terminal`, `Square`, `Shield`, `Key`, `WifiOff`,
  `Code2`, `Sparkles`, `BookOpen`, `Keyboard`, and `HelpCircle`.
- Added the missing Lucide SVG assets used by the Tauri settings sidebar to
  `crates/oxideterm-native/src/assets.rs`.
- Expanded the native General and Portable settings surfaces with language,
  data directory, update channel, portable runtime, biometric/password, CLI
  companion, and disabled native action rows where the backend action is not
  available yet.
- Expanded Terminal settings coverage across display, input, command bar,
  command marks, history, in-band transfer, background image, and highlight-rule
  management surfaces. Values backed by `oxideterm-settings` are editable and
  persisted.
- Expanded Appearance settings coverage for theme/custom-theme management
  surfaces, density, radius, UI font, animation speed, frosted glass, and
  sidebar collapsed default.
- Expanded Local Terminal, Connections, SSH keys/auth status, Reconnect, SFTP,
  IDE, AI, Knowledge, Keybindings, and Help surfaces so every Tauri route has
  visible native rows for the corresponding settings and management actions.
- Language changes from the native settings page now update the in-memory
  `I18n` instance immediately, in addition to being persisted.
- Verified that the settings surface uses existing locale keys from the imported
  Tauri locale domains; no `zh-CN` settings keys are missing.

## Phase 4: Runtime Wiring And Hardening

After every setting exists and persists, finish live runtime effects and backend
bridges.

Runtime wiring:

- Apply all terminal settings live, including future features such as command
  bar, command marks, paste protection, in-band transfer, and highlighting.
- Apply all appearance settings live, including custom themes and background
  images.
- Apply SSH, reconnect, connection pool, SFTP, IDE, Portable, AI, Knowledge,
  MCP, and keybinding settings as their subsystems become native.
- Convert disabled controls into active controls only when the backing native
  command or store is present.

Persistence hardening:

- Add migration tests for older versions.
- Add settings corruption recovery path.
- Add explicit save error state in UI.
- Add a "reset section to defaults" control only after defaults are fully
  verified.

Exit criteria:

- Settings schema covers all Tauri persisted fields and frontend-local user
  settings discovered by Phase 1 inventory.
- UI covers all Tauri settings controls.
- Unsupported runtime effects are consciously preserved and documented, not
  silently omitted.
- Native settings no longer require editing constants in `oxideterm-native`.

## Implementation Notes

- Prefer translating Tauri store semantics into Rust data structures before
  translating visual controls.
- Keep settings data independent from GPUI types.
- Keep GPUI controls independent from terminal/session internals.
- Use semantic tokens for every reusable UI value.
- Update all 11 locale files whenever adding a settings string.
- Do not inline strings such as section titles or option labels.
- Avoid one giant file. If a file grows past roughly 400 lines, split it before
  continuing.

## Suggested First Commit Scope

The first implementation commit should include only:

- A checked-in settings inventory table that reconciles Tauri backend
  `settings.json`, `settingsStore.ts`, settings tabs, and frontend
  `localStorage` keys.
- `oxideterm-settings` crate with complete source-inventory-compatible
  schema/defaults/normalization/migration tests.
- Native loads settings and applies locale/sidebar/theme defaults.
- Settings shell with left nav and every Tauri tab route present.
- Basic reusable settings controls.
- i18n keys for settings shell in all 11 locales.

This gives the rest of the settings migration a stable foundation and avoids a
large, risky all-at-once port.
