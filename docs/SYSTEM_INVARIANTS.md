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

- Text fields must render an explicit caret when they are not backed by a native
  platform text input.
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

## Terminal Surface

- Terminal focus and form/modal focus are separate.
- Terminal input handlers must assume they are active only when their pane owns
  focus and no modal has intercepted the input.
- Terminal resize must be driven by actual pane bounds, not whole-window bounds.
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
- Session tests should avoid real network calls unless explicitly marked as
  integration tests.

## Verification Expectations

- Focus and input routing changes require a manual check for: typing in a modal,
  paste in a modal, Escape, Enter, Tab/Shift+Tab, and ensuring the terminal does
  not receive those inputs.
- UI token changes require at least `cargo check -p oxideterm-native`.
- Terminal backend changes require terminal crate tests and native check.
- Whole-workspace changes should run `cargo check --workspace` and
  `git diff --check`.
