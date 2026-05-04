# OxideTerm System Invariants

This document records rules that must stay true while OxideTerm evolves. These
are not optional style preferences. They are guardrails for preventing repeated
regressions in focus, input routing, UI translation, theming, and terminal
correctness.

## Focus And Input Routing

- A visible modal owns keyboard focus.
- While a modal is open, terminal panes must not receive text input, paste input,
  keydown events, IME text, or command actions intended for the modal.
- Modal keyboard handlers must stop propagation and prevent default handling
  after consuming input.
- Workspace-level shortcuts may run while a modal is open only when the modal
  explicitly supports that action. Otherwise, they must be ignored or handled by
  the modal.
- Opening a modal must cancel any pending "focus active terminal pane on next
  frame" behavior.
- Closing a modal may restore focus to the active pane, but only after the modal
  state is cleared.
- Clicking inside a modal input must focus the workspace/modal focus handle, not
  the terminal pane behind it.
- Paste while a form field is focused must write into that field. It must not
  fall through to terminal paste.
- Copy while a modal is open must not copy terminal selection unless the modal
  deliberately delegates that action.

## Text Fields

- GPUI forms must compose shared OxideTerm UI primitives for text input,
  checkbox, button, tabs, modal, and form-field layout. Feature pages must not
  hand-copy those controls inline.
- Text fields must render an explicit caret when they are not backed by a native
  platform text input.
- Text field visuals, including focus border, placeholder, password masking,
  selected text, and caret geometry, must be implemented in the shared text input
  primitive before reuse in feature forms.
- Custom/native text fields must insert committed text from the platform text
  payload, not the keybinding/key-name string. In GPUI terms, form input must
  use `key_char` for typed text and reserve `key` for shortcuts/navigation.
  This is security-critical for SSH passwords because shifted symbols,
  Option/Alt characters, and non-US keyboard layouts can otherwise authenticate
  with different bytes than the user typed.
- The caret blink state belongs to the focused text field or form model, not to
  the terminal cursor.
- Caret blink timing must be centralized or tokenized enough that it can be
  tuned consistently.
- Typing, deleting, changing field focus, and pasting must reset the caret to
  visible.
- Form fields are single-line unless the source UI is explicitly multi-line.
  Pasted line endings should be normalized before insertion.

## Modal Layering

- Modal overlays must cover the whole interactive surface.
- The surface behind a modal may be visible, but it is inert for keyboard input.
- Mouse interaction with controls behind the modal must not happen through the
  modal overlay.
- Modal visual hierarchy must match the source UI being translated: overlay,
  dialog container, header, body, footer, separators, controls.

## Floating Primitive Layering

- Floating UI primitives must not render their popup content as ordinary
  children of a row, card, pane, or scroll item.
- This rule applies to select, menu, dropdown menu, tooltip, popover, context
  menu, autocomplete suggestions, command suggestions, and any future floating
  surface.
- The trigger belongs in normal layout. The floating content belongs in an
  overlay/portal layer owned by the nearest surface or root entity and rendered
  after normal page content.
- Floating content must not rely on local sibling order to appear above later
  cards or rows. If it needs to escape clipping, scrolling, or sibling paint
  order, it belongs in the overlay/portal path.
- Overlay state must be semantic enough to avoid page-specific hacks: track the
  active overlay identity and its anchor/bounds rather than expanding the
  feature view inline.
- Opening a floating primitive must close conflicting floating primitives.
  Surface switches, tab switches, navigation, Escape, item activation, and
  outside click must close the floating surface unless the primitive explicitly
  documents another behavior.
- Floating primitives are focus/input boundaries. Text, paste, IME, and command
  input must not leak to the terminal or underlying pane while the floating
  surface owns that interaction.
- Floating primitive visuals must use semantic tokens for background, border,
  shadow, radius, spacing, typography, text, hover, active, selected, disabled,
  and focus states. Feature pages must not hard-code these values.

## Semantic Design Tokens

- UI code must not introduce raw hard-coded colors for product UI.
- UI code must not introduce raw hard-coded radii for product UI.
- UI code must not introduce raw hard-coded component dimensions when the value
  describes a reusable component, spacing rule, or layout relationship.
- Colors must come from semantic theme tokens such as background, panel,
  elevated surface, border, text, muted text, accent, warning, error, and
  success.
- Radii must come from semantic radius tokens such as xs, sm, md, lg, and any
  component-specific token that is needed.
- Component sizes, gaps, paddings, typography sizes, and layout widths must come
  from semantic metric or spacing tokens when reused or when copied from the
  Tauri UI.
- A raw pixel value is acceptable only for one-off geometry that cannot
  reasonably be themed, and the reason should be obvious from the local code.
- When translating the Tauri UI, first identify the source token or Tailwind
  class, then map it to an OxideTerm token. Do not eyeball values from a
  screenshot when the source code is available.

## Tauri UI Translation

- Native UI parity means preserving component sizes, spacing relationships,
  typography, colors, radii, and control hierarchy from the Tauri source.
- The task is translation, not redesign.
- If a Tauri component uses shared primitives, inspect those primitives before
  implementing the native counterpart.
- If a Tauri value is represented by a Tailwind class, translate the class to its
  pixel/token equivalent instead of inventing a nearby value.
- I18n strings must come from the i18n catalog, not inline UI literals, except
  for protocol constants or temporary debug text.
- Runtime i18n is an 11-locale system: `en`, `zh-CN`, `zh-TW`, `de`, `es-ES`,
  `fr-FR`, `it`, `ja`, `ko`, `pt-BR`, and `vi`. Any new user-facing key must be
  added to every locale domain file in the same change.
- Language names are autonyms in every locale. Do not translate labels such as
  "English" or "Deutsch" into the current UI language.
- I18n domain files should stay split by feature area. Do not rebuild one giant
  per-locale JSON catalog.

## Terminal Surface

- Terminal focus and form/modal focus are separate.
- Terminal input handlers must assume they are active only when their pane owns
  focus and no modal has intercepted the input.
- Terminal resize must be driven by actual pane bounds, not whole-window bounds.
- Terminal backend resize should be coalesced with the same intent as the Tauri
  terminal resize path: UI layout may update per frame, but PTY/SSH resize
  commands must be deduplicated and debounced so window dragging does not flood
  the backend.
- PTY resize must be resent after delayed connection setup if the UI resized
  while the backend was connecting.
- Rendered terminal rows are visual rows. Selection and copy may reason about
  logical wrapped lines, but paint must not merge visual rows.
- Link rendering must not decorate editable active input in a way that conflicts
  with shell completion or syntax highlighting.

## Backend Sessions

- Local and SSH terminal sessions must expose the same session contract:
  read_pending, write_input, resize, shutdown, title, lifecycle, process state,
  search, and snapshot.
- SSH output must pass through Alacritty's Term and Processor before rendering.
- SSH input, resize, and shutdown must map to transport commands. They must not
  bypass the session abstraction.
- SSH keyboard-interactive prompts are modal focus owners. Direct 2FA,
  password-to-KBI fallback, and chained partial-success KBI must all route
  through the same native prompt handler.
- SSH `none` auth probing is intentional and must stay aligned with the Tauri
  backend semantics.
- SSH public-key, certificate, and agent auth must negotiate RSA signature
  algorithms in the same semantic order as the Tauri backend.
- SSH agent forwarding must reject unsolicited server-opened agent channels
  unless forwarding was requested, and must relay accepted channels to the local
  agent socket with a concurrency limit.
- SSH tabs, SFTP, port forwards, and IDE consumers must acquire SSH connections
  through the registry/router path. Long-term shape is one authenticated
  physical SSH connection per connection key, with many channel consumers.
- The registry may own the authenticated physical SSH handle; consumers open
  channels from that shared handle instead of creating redundant physical
  connections for the same connection key.
- Session tests should avoid real network calls unless explicitly marked as
  integration tests.

## Source Boundaries

- OxideTerm native code must not copy, mechanically translate, or closely mirror
  Zed GPL implementation code.
- External terminal references are acceptable only as behavioral references:
  public protocols, user-visible behavior, and independently designed tests.
- Source comments and identifiers should use OxideTerm terminology, not Zed
  terminology, outside of local planning/audit documents.

## Verification Expectations

- Focus and input routing changes require a manual check for: typing in a modal,
  paste in a modal, Escape, Enter, Tab/Shift+Tab, and ensuring the terminal does
  not receive those inputs.
- UI token changes require at least `cargo check -p oxideterm-native`.
- Terminal backend changes require terminal crate tests and native check.
- Whole-workspace changes should run `cargo check --workspace` and
  `git diff --check`.
