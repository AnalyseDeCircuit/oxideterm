# Internationalization And Product Copy

All user-visible native UI text belongs in the locale catalogs under `crates/oxideterm-i18n/locales/`. A compiled English fallback is not permission to leave other catalogs incomplete.

## When A Key Is Required

Add or update a locale key for every label, button, dialog title, hint, error, empty state, status message, keyboard description, and notification shown by the application. Do not hard-code product copy in a render function, action handler, or background-task error path.

Existing code normally accesses text through the `I18n` instance and its `t` method. Reuse an existing key when the meaning and grammatical context are exactly the same. Similar wording with a different action, scope, or plural meaning needs its own key.

## Adding A New String

1. Choose a stable semantic key near the owning feature namespace.
2. Add the key to the English catalog and every other locale catalog with the same JSON structure.
3. Preserve every interpolation placeholder exactly, such as `{{error}}` or `{{count}}`.
4. Use the key from the owning view or action.
5. Run the locale audit and inspect the affected UI in at least English and Chinese.

```sh
python scripts/quality/audit_i18n.py
```

The audit checks JSON parsing, duplicate flattened keys, missing files, missing keys, source-used keys, and placeholder mismatches. It can also report copied English strings; treat those warnings as translation work, not as a reason to remove the key.

## Product Copy Rules

- Describe what the action does and its scope. Prefer “Reconnect node” to an ambiguous “Retry”.
- Keep destructive, authentication, sync, and overwrite wording explicit about the affected data or connection.
- Do not expose passwords, tokens, private paths, or terminal content through interpolated errors or status labels.
- Match existing product terminology. For example, use “node” for a runtime SSH identity, “saved connection” for persisted configuration, and “terminal pane” for a consumer view.
- Do not translate protocol names, commands, file names, or configuration keys when that would make them unusable.

## Reviewing A Locale Change

Check line wrapping, button width, truncation, and right-to-left or CJK font fallback when relevant. A translated string can be key-complete but still break a narrow action row or modal footer.

Locale work often accompanies a UI change. Keep the UI and locale updates in the same change so a new control cannot ship with missing language coverage.
