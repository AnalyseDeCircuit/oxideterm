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

## Native Render Policy

- Native GPUI does not expose a product-level CPU renderer fallback in the
  current GPUI version. OxideTerm must not present WebGL, Canvas, or CPU
  renderer choices as if they were real native renderers.
- Renderer compatibility settings must be expressed as an OxideTerm render
  policy: quality, low-power, or compatibility. The policy may reduce expensive
  visual work, but it must not change terminal protocol semantics.
- Software-emulated graphics may automatically enter compatibility mode. Unknown
  macOS/Windows GPU information must not be treated as failure; users can still
  choose low-power or compatibility manually.
- Safe-mode environment overrides such as `OXIDETERM_RENDER_PROFILE` apply only
  to the current process. They must not rewrite persisted user settings.
- High-cost visuals must go through the effective render policy before use:
  native vibrancy, background images and blur, animations, terminal inline image
  decoding, image cache budgets, repaint cadence, and per-frame drain budgets.
- Compatibility mode keeps core terminal behavior intact: local/SSH input,
  output, resize, selection, search, copy/paste, mouse reporting, and protocol
  parsing must continue to work. It may replace decoded images with bounded
  placeholders and disable nonessential visual effects.
- If GPUI cannot create a graphics context, startup must fail with a clear
  diagnostic and a safe-mode hint. Do not silently black-screen or claim a CPU
  backend exists.

## Terminal Graphics And UTF-8

- Terminal graphics protocol detection must never corrupt normal UTF-8 text.
- Graphics ingress must not treat raw 8-bit C1 bytes in the `0x80..0xbf` range
  as OSC/DCS/APC graphics delimiters on the main terminal byte stream. Those
  byte values are valid UTF-8 continuation bytes and appear in Powerline glyphs
  such as `❯` and in CJK text.
- Graphics ingress may intercept ESC-form control sequences such as `ESC ]`,
  `ESC P`, and `ESC _`. If support for 8-bit C1 controls is ever reintroduced,
  it must be guarded by an explicit encoding/protocol mode and backed by tests
  proving that UTF-8 Powerline and CJK filenames are unchanged.
- Add or preserve regression coverage for representative UTF-8 terminal text
  whenever graphics parsing changes. The coverage must include at least a
  Powerline prompt glyph and CJK filenames.
- Graphics control sequences must be consumed only after they are positively
  identified as supported image protocol payloads. Unsupported OSC/DCS/APC
  sequences must pass through to the terminal parser in their ESC-form bytes.
- Do not advertise or acknowledge a terminal graphics capability until the
  protocol paths commonly used by real TUI clients are implemented. For Kitty
  Graphics this includes direct payloads and file/temp-file transmission modes
  (`t=d`, `t=f`, and `t=t`) because applications such as `yazi` may send image
  paths rather than inline image bytes after receiving an `OK` response.
- Kitty Graphics cursor movement must follow the upstream protocol exactly:
  `C=0` or omitted moves the cursor after placement, while `C=1` means no cursor
  movement. Do not synthesize placeholder spaces for `C=1`; TUI previewers such
  as `yazi` rely on this for `kgp-old` image uploads.
- Terminal graphics capability/query responses must be written from the backend
  IO path with protocol-level latency. Do not route Kitty/SIXEL/iTerm2 probe
  responses through UI repaint or snapshot polling; TUI clients such as `yazi`
  use short probe deadlines and will otherwise fall back to external preview
  tools.
- Kitty file/temp-file payloads are base64-encoded UTF-8 paths. Decode the path,
  enforce storage limits before reading, and delete temp-file payloads after a
  successful read when the protocol marks them temporary.
- Incomplete graphics sequences and Kitty chunk assembly must obey storage
  limits while still incomplete. Do not wait until final decode to enforce
  memory limits.
- Image decoding must not be allowed to monopolize the PTY reader. Large,
  malformed, or unsupported payloads should fail with a bounded error path
  rather than blocking terminal input, scrolling, or pane close.
- RenderImage creation must not happen in paint for already-decoded image data.
  Use a cache keyed by stable image id and version so cursor blink, scroll, and
  repaint do not repeatedly materialize the same image.
- Terminal graphics protocol/state data is RGBA. GPUI `RenderImage` raster data
  is a renderer contract, not a protocol contract: in GPUI 0.2.2 `RenderImage`
  is documented as BGRA and GPUI's own image element converts raster frames from
  RGBA to BGRA before `RenderImage::new`. Route all terminal image materializing
  through the GPUI adapter/cache boundary and do not swap channels in protocol
  decoders or terminal snapshots. If GPUI exposes a runtime pixel-format API or
  changes this contract, update only that adapter and its tests.
- Terminal graphics changes require manual checks in TUI applications that emit
  image protocols, especially `yazi`, because those workloads combine UTF-8
  filenames, alternate-screen rendering, and image previews.

## Backend Sessions

- Local and SSH terminal sessions must expose the same session contract:
  read_pending, write_input, resize, shutdown, title, lifecycle, process state,
  search, and snapshot.
- Closing a pane must not synchronously block the UI thread while waiting for a
  local PTY/event-loop thread to finish. Running sessions should receive a
  shutdown signal and detach from UI teardown; child-exit cleanup may join only
  after the backend has naturally exited.
- Killing or closing a pane that is running a TUI app must be treated as a
  lifecycle operation, not as a blocking process wait from the render/input
  path.
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
- Terminal graphics changes require tests proving ordinary UTF-8 text passes
  through unchanged, including Powerline glyphs and CJK text.
- Terminal graphics changes require a bounded-memory test or review for
  incomplete control sequences and chunked image payloads.
- Pane lifecycle changes require a manual TUI close/kill check so closing a pane
  running an alternate-screen app does not freeze the UI.
- Whole-workspace changes should run `cargo check --workspace` and
  `git diff --check`.
