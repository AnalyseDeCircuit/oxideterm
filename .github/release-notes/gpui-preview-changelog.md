# OxideTerm Native GPUI Preview Changelog

Keep the newest GPUI preview entry first. The native package workflow extracts
the section whose heading matches the release version, so older preview entries
can remain in this file.

## 2.0.0-gpui-preview.15

This preview focuses on cloud sync reliability, saved connection editing, terminal font ligatures, and Windows auto-update reliability.

- Cloud sync export preflight now handles managed SSH keys correctly, so connections that reference locally managed key material no longer block Gist upload with a missing key material error.
- Cloud sync upload, pull, conflict preview, rollback, and delivery-state handling received tighter state transitions and clearer failure accounting.
- Cloud sync selection now keeps unchanged local records out of write requests while still preserving remote tombstone and conflict handling semantics.
- Editing an existing saved connection can now switch from Agent or key-based authentication to password authentication and submit the newly entered password.
- Existing keychain-backed passwords still remain unloaded until explicitly revealed, preserving the previous secret-handling boundary.
- Terminal settings now include a Font Ligatures toggle. It is off by default and enables programming ligatures when the selected terminal font supports them.
- Terminal font features now flow from persisted settings into GPUI terminal rendering instead of always disabling ligature shaping.
- Windows native auto-update now stages NSIS payloads first and lets a small update helper finish replacement after OxideTerm exits.
- The Windows update helper keeps an `old` rollback directory during replacement and uses Restart Manager best-effort handle release before moving staged files into place.
- The Windows installer now exposes shortcut options, with Start Menu shortcuts enabled by default and desktop shortcuts available as an opt-in component.

## 2.0.0-gpui-preview.14

This preview focuses on diagnostics, terminal workflow polish, settings cleanup, and remote SSH project detection reliability.

- Application file logging and opt-in debug logging are now available for native GPUI troubleshooting.
- SSH authentication diagnostics now include more structured debug output while continuing to avoid sensitive credential values.
- The terminal command bar now handles multiline commands and pasted command text more gracefully.
- Remote SSH directory, Git, and project detection is more resilient: repository identity is reported before slower status scans, and expensive status collection is bounded.
- Terminal settings were reorganized so local-terminal options live under the Terminal page, with improved subpage navigation and localization.
- Key and connection-related settings were consolidated to reduce duplicated configuration surfaces.
- Raw UDP profiles received fuller GPUI support and localization coverage.
- Port forwarding and terminal command handling received reliability fixes around host normalization and command construction.
- Settings, session-management dialogs, i18n strings, and the GPUI welcome/visual polish pass received additional refinements.

## 2.0.0-gpui-preview.13

This preview focuses on native plugin runtime reliability, packaging cleanup, and tightening GPUI workflow polish before the 2.0 line becomes the default app.

- Wasm plugin execution is bundled again in standard GPUI preview packages so Wasm plugins work without a separate runtime download.
- The optional `oxideterm-wasm-runtime` sidecar path remains available for future lightweight or externally managed builds.
- The Wasm runtime compatibility model now checks the host update channel, host version, plugin protocol, Wasm guest ABI, WASI profile, platform target, and asset checksum for sidecar installs.
- Native plugin runtime ownership was narrowed so the in-process host API, bundled Wasm executor, and optional sidecar process bridge stay separate.
- Quick Commands, settings, and narrow-width GPUI forms received layout and overflow fixes across the native preview UI.
- SSH authentication selection was simplified into password, key, Agent, and two-factor groups while keeping existing saved connection formats unchanged.
- Session icons, legal notices, onboarding, and plugin manager surfaces received additional polish and localization coverage.
- Serial and VNC work continues in the native preview track; please report device-specific and remote-desktop edge cases with logs and screenshots where possible.
