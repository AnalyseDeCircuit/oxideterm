# Building Product Pages With GPUI

Use this guide when adding a control or page to the desktop application. Start with the feature's existing implementation and its runtime owner. Framework maintenance has a separate [GPUI CE guide](gpui-ce.md).

## Find A Complete Example

Read the rendering, state, input, and action paths of the closest existing interaction before copying its layout.

| Need | Source to follow |
| --- | --- |
| Settings cards, toggle rows, and select triggers | [settings/cards.rs](../../crates/oxideterm-gpui-app/src/workspace/settings/cards.rs) |
| A general settings page with actions and asynchronous status | [settings/general_terminal_pages.rs](../../crates/oxideterm-gpui-app/src/workspace/settings/general_terminal_pages.rs) and [settings/entity.rs](../../crates/oxideterm-gpui-app/src/workspace/settings/entity.rs) |
| A settings text field | [settings/terminal_controls.rs](../../crates/oxideterm-gpui-app/src/workspace/settings/terminal_controls.rs), [SettingsInput types](../../crates/oxideterm-gpui-settings-view/src/types.rs), and [workspace/ime.rs](../../crates/oxideterm-gpui-app/src/workspace/ime.rs) |
| A form with editable drafts and validation | [settings/totp_credentials_page.rs](../../crates/oxideterm-gpui-app/src/workspace/settings/totp_credentials_page.rs) |
| A multiline editor in a dialog | [terminal paste editor](../../crates/oxideterm-gpui-terminal/src/app/paste.rs) |
| A select inside a scrolling settings page | [settings/controls.rs](../../crates/oxideterm-gpui-app/src/workspace/settings/controls.rs) |
| A feature page with virtualized sections and its own events | [cloud_sync/surface.rs](../../crates/oxideterm-gpui-app/src/workspace/cloud_sync/surface.rs) |

`WorkspaceApp` composes feature entities and routes their events. Keep drafts, loading state, subscriptions, and retained tasks with the entity that owns the interaction. Background results should identify the originating request or generation so a closed or replaced editor cannot receive a late update. Use the existing [delivery budgets](../../crates/oxideterm-gpui-app/src/workspace/delivery.rs) when delivering repeated results to the UI thread.

## Add A Setting Through Its Existing Path

For a terminal toggle, follow `terminal_input_settings_card` in `settings/cards.rs`: it reads the persisted value, uses `checkbox_row` with localized label and hint keys, and supplies the corresponding setter.

1. Add the value and its default or normalization to the owning settings model, following [settings and migrations](settings-data-and-migrations.md).
2. Extend the owning page's card or row collection. Reuse `settings_card`, `plain_settings_card`, `checkbox_row`, or the existing select/text-field wrapper.
3. Route the change through the existing settings action and persistence path. Trace how that path refreshes terminal preferences or the affected service; updating a displayed boolean alone does not apply a runtime setting.
4. Keep temporary validation errors and unfinished text in the page's draft state. Commit at the same point as neighboring controls.
5. Add the label, hint, validation, and accessibility text through the locale catalogs, then verify the behavior after reopening the page and restarting when relevant.

For controls outside Settings, use the shared [button](../../crates/oxideterm-gpui-ui/src/button.rs), [form field](../../crates/oxideterm-gpui-ui/src/form_field.rs), [checkbox](../../crates/oxideterm-gpui-ui/src/checkbox.rs), and [surface](../../crates/oxideterm-gpui-ui/src/surface.rs) primitives. A page-specific composition normally belongs beside that page.

## Text, Focus, And Input Methods

The shared [text_input](../../crates/oxideterm-gpui-ui/src/text_input.rs) functions draw text, selection, caret, and input geometry. They need the owning page's editing and focus handling. For settings fields, follow `settings_text_input_control` through the input identity, value replacement, selection, and `WorkspaceInputHandler` in `workspace/ime.rs`.

Preserve the full path for committed text, marked text, UTF-16 selection ranges, paste, deletion, caret positioning, and focus changes. Typing from the platform text payload must work independently of shortcut key names. Password masking changes display coordinates as well as appearance, so retain the shared raw-to-visible range handling.

Use `TextEditorView` for an existing multiline editing use case such as the paste dialog. Let the entity retain its buffer, undo history, focus handle, and observation. Submission and cancellation release the temporary editor.

Before adding a dialog, trace [modal ownership](../../crates/oxideterm-gpui-app/src/workspace/root/modal_owner.rs) and [root rendering](../../crates/oxideterm-gpui-app/src/workspace/root/render.rs). The active window's modal owns keyboard input. Consumed shortcuts, paste, and IME input must not reach the terminal behind it. Cancel deferred terminal focus when opening the modal, and restore focus after closing it.

## Popups And Scrolling

Use the shared [modal](../../crates/oxideterm-gpui-ui/src/modal.rs), [select](../../crates/oxideterm-gpui-ui/src/select.rs), and [context-menu](../../crates/oxideterm-gpui-ui/src/context_menu.rs) primitives together with the feature's overlay owner.

The settings select implementation stores its anchor with the window identity and renders the popup in the overlay layer. Keep the trigger in the normal row; route the popup through that overlay so later cards and scrolling containers cannot clip it. Preserve Escape, outside-click, selection, navigation, and scroll-close behavior. Popup scroll events must stop before reaching the underlying page or terminal.

Use [ScrollableElement and scrollbar helpers](../../crates/oxideterm-gpui-ui/src/scroll.rs) for the existing scrolling style. For long lists, follow [workspace virtual lists](../../crates/oxideterm-gpui-app/src/workspace/virtual_list.rs) and retain the list state in the owner. Stable row identities, bounded row work, and the existing scroll callbacks matter when rows are reused.

## Theme, Language, And Page Identity

Use `ThemeTokens` for semantic colors, radii, spacing, and typography. Reuse a neighboring component's token choices so focus, disabled, hover, selected, and error states stay consistent. Add user-facing strings in all 11 catalogs using the [internationalization workflow](i18n-and-product-copy.md).

A page may be standalone, embedded in a mixed layout, or displayed in another window. Resolve the page and window that own the action instead of assuming the active top-level tab owns every control. See [page and pane lifetime](runtime-ownership.md#pages-panes-and-window-mounts) before adding singleton navigation, close behavior, or window-specific state.

## Verify The Interaction

Run `cargo check -p oxideterm-gpui-app` and the locale audit for a UI change. Use the closest existing GPUI tests for stable state transitions; follow the [verification matrix](verification.md) for the affected feature.

Exercise the actual field or popup with typing, selection, paste, IME composition, Escape, Enter, Tab, and Shift+Tab. Test scrolling with the popup open, a narrow mixed pane, and a detached window. Confirm that the background terminal receives no modal input. Native window and input behavior still needs a check on the affected operating system.
