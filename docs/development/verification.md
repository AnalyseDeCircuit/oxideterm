# Verification Matrix

Run the smallest checks that prove the changed contract, then expand only when the change crosses a repository boundary. CI is the source of truth for the full matrix in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) and [`.github/workflows/platform-checks.yml`](../../.github/workflows/platform-checks.yml).

## Baseline

Run these before handing off any Rust change:

```sh
git diff --check
cargo fmt --check
```

Run checks from the repository root. When a command needs an operating-system-specific backend, run it on that host instead of treating a cross-platform compile from another machine as equivalent evidence.

## Focused Changes

| Change area | Required focused verification | Manual verification when applicable |
| --- | --- | --- |
| Shared UI, workspace, settings UI | `cargo check -p oxideterm-gpui-app` | Focus, typing, paste, Escape, Enter, Tab and Shift+Tab; confirm the terminal does not receive modal input |
| Terminal behavior or rendering | `cargo test -p oxideterm-gpui-terminal` and the relevant terminal-model check | Selection, scrolling, alternate screen, resize, and the affected shell or TUI workflow |
| SSH, SFTP, forwarding, reconnect | The focused crate tests, then `cargo check -p oxideterm-gpui-app` | Close one consumer while another remains active; explicit disconnect; reconnect and failure cleanup |
| Settings, themes, or locale keys | `python scripts/quality/audit_i18n.py` plus the affected crate check | Inspect every changed locale in the relevant view |
| macOS framework code | `cargo check -p gpui_macos` and `cargo check -p oxideterm-gpui-app` | Native window, input, cursor, and renderer behavior |
| Windows framework code | `cargo check -p gpui_windows` and `cargo check -p oxideterm-gpui-app` on Windows | IME, pointer capture, cursor recovery, titlebar, and Direct3D behavior |
| Linux framework code | Relevant Linux crate and application checks | Both affected compositor paths: Wayland and/or X11 |
| Repository scripts or workflows | The matching script test module | Dispatch only the workflow needed by the change |

## Selecting The Scope

- A localized UI wording or layout change needs the owning application check and the locale audit; it does not need a terminal benchmark.
- A terminal parser or viewport change needs its focused tests and a manual terminal workload; it does not automatically need all remote-desktop checks.
- A connection ownership change needs a consumer-lifecycle scenario even when unit tests pass, because the high-risk contract is what remains alive after one surface closes.
- A framework change needs the platform checks for every modified backend and a manual check on the platform where the defect was reproduced.

When an existing test already covers the contract, run it rather than adding a duplicate. Add a new test only when the failure can be represented stably without a live server, GPU-specific timing, or private credentials.

## Manual Checklists

### Focus, Modals, And Text Fields

Check typing, paste, selection, Escape, Enter, Tab, Shift+Tab, click-away behavior, and the return of focus to the previous owner. Confirm that a terminal behind a modal receives none of these keystrokes.

### Terminal And Terminal Graphics

Check normal output, a long scrolling workload, selection, resize, alternate screen, UTF-8 text, and the affected image or color protocol if one changed. Test with the actual shell or TUI named in the report whenever possible.

### Shared SSH Runtime

Open two consumers for the same node, then close one. Confirm that the remaining terminal, SFTP surface, or forward continues to work. Test explicit disconnect and reconnect separately; they have different ownership consequences.

### Platform Windowing

For pointer and titlebar work, move the pointer outside the window while pressed, switch focus, return, and verify no stuck cursor, pressed control, capture, or drag state remains. For IME work, verify composition start, commit, cancellation, and focus changes.

## Full Repository Boundary

Run the CI-equivalent Rust checks when the change affects shared primitives, workspace-wide types, dependency wiring, or several crates:

```sh
cargo check --workspace --all-targets
cargo fmt --check
cargo test --workspace
python scripts/quality/audit_i18n.py
```

Do not add tests merely to increase coverage. Add a focused regression test when the bug has a stable, observable contract; use the existing closest test module rather than inventing a new test framework.

## CI Mapping

The primary CI workflow runs workspace check, formatting, workspace tests, locale auditing, packaging-helper tests, and repository-policy tests. Native platform CI separately checks Windows and macOS GPUI backends and the application. A green Linux workspace job therefore does not establish Windows or macOS native behavior.
