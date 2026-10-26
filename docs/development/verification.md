# Verification Matrix

Run the smallest checks that prove the changed contract, then expand only when the change crosses a repository boundary. CI is the source of truth for the full matrix in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) and [`.github/workflows/platform-checks.yml`](../../.github/workflows/platform-checks.yml).

## Baseline

Run these before handing off any Rust change:

```sh
git diff --check
cargo fmt --check
```

Run checks from the repository root. When a command needs an operating-system-specific backend, run it on that host instead of treating a cross-platform compile from another machine as equivalent evidence.

Complete a coherent set of implementation, caller, and test changes before checking it. Reuse successful results while their source, dependencies, configuration, and environment are unchanged. A successful Clippy run already establishes compilation for that configuration; do not follow it with a duplicate `cargo check`. Launch the application only for the manual acceptance that needs it, not after every edit.

## Focused Changes

| Change area | Required focused verification | Manual verification when applicable |
| --- | --- | --- |
| Shared UI, workspace, settings UI | `cargo check -p oxideterm-gpui-app` | Focus, typing, paste, Escape, Enter, Tab and Shift+Tab; confirm the terminal does not receive modal input |
| Terminal behavior or rendering | `cargo test -p oxideterm-gpui-terminal` and the relevant terminal-model check | Selection, scrolling, alternate screen, resize, and the affected shell or TUI workflow |
| SSH, SFTP, forwarding, reconnect | The focused crate tests, then `cargo check -p oxideterm-gpui-app` | Close one consumer while another remains active; explicit disconnect; reconnect and failure cleanup |
| Settings, themes, or locale keys | `python scripts/quality/audit_i18n.py` plus the affected crate check | Inspect every changed locale in the relevant view |
| macOS framework code | `cargo check -p gpui-pre-macos` and `cargo check -p oxideterm-gpui-app` | Native window, input, cursor, and renderer behavior |
| Windows framework code | `cargo check -p gpui-pre-windows` and `cargo check -p oxideterm-gpui-app` on Windows | IME, pointer capture, cursor recovery, titlebar, and Direct3D behavior |
| Linux framework code | Relevant Linux crate and application checks | Both affected compositor paths: Wayland and/or X11 |
| Repository scripts or workflows | The matching script test module | Dispatch only the workflow needed by the change |
| Git dependencies, Nix package, or flake | `nix build .#oxideterm -L --show-trace` and `nix flake check -L` | Confirm `result/bin/oxideterm` builds and runs |
| Documentation only | Relative links, referenced paths, locale/route parity, and `git diff --check`; build the docs website when changed | Inspect changed website pages; no Rust build |

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

### Tab Scopes, Search, and Motion

For tab-scope changes, cover mixed-pane membership, visible-only navigation/closing/reordering, empty scopes, and window moves while hidden work continues. For search/input changes, cover multiline paste, explicit newline insertion, replacement captures, independent undo, horizontal scrolling, and IME coordinates. For motion changes, use the existing controllable-clock tests for interruption/reversal and Off/Reduced behavior, then inspect the actual transition once the edits are complete.

### Remote Desktop Displays

Check the affected plugin and host together. Distinguish single-monitor viewport resize from all-monitor topology. Exercise external-display changes, laptop-lid state, scaling, and reconnect when those paths changed. Compare remote screenshots with local viewer output and reported remote dimensions to identify cropping.

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

The primary CI workflow runs workspace check, formatting, workspace tests, locale auditing, packaging-helper tests, and repository-policy tests. Its Ubuntu job also compiles the Linux GPUI backend and application; there is no second Linux-only compile job. Native platform CI separately checks Windows and macOS GPUI backends and the application, including the Windows shader, message, text and ConPTY tests. A green Linux workspace job therefore does not establish Windows or macOS native behavior.

CI uses the following execution policy:

- Ready code PRs retain the full Linux workspace checks and the path-selected Windows/macOS, Nix, background-media and remote-agent checks.
- Documentation-only changes skip Rust setup, caches, system dependencies, compilation and tests in triggered PR and push runs. Locale and repository-policy checks still run. `scripts/ci/rust-scope.cjs` recognizes `docs/`, root Markdown, `.github/` Markdown and conventional README/CHANGELOG/CONTRIBUTING/AGENTS Markdown files. Other files, including Markdown fixtures under crates, conservatively retain Rust checks.
- Draft PRs skip heavy jobs. `ready_for_review` starts them; `converted_to_draft` cancels superseded work through the existing concurrency groups. Lightweight locale and policy checks continue to run.
- Pushes to `main` and `experiment/rust-native-v2` retain the primary CI workflow's path triggers, Linux workspace validation and lightweight checks. Scope compares the push event's `before` commit with the checkout, fetching that commit when shallow history omits it. New branches receive full validation.
- Windows/macOS, Nix, background-media and remote-agent workflows run on relevant ready PRs or manual dispatch, not again on push. For direct pushes that need these checks, manually dispatch the relevant workflow on the pushed branch. Run the applicable full checks before release. Manual runs always retain full validation.

The required checks keep the names `Rust workspace (check + fmt + test)` and `i18n completeness`. The Rust workflow still starts for documentation PRs and completes its scope step instead of omitting a required status through a workflow-level path filter. PR scope is determined from the checked-out merge commit against its first parent, with both parents fetched. Deleted paths and both sides of renames participate; a failed diff fails the job rather than skipping validation.

The independent [Nix Package workflow](../../.github/workflows/nix-package.yml) runs `nix build .#oxideterm -L --show-trace` and `nix flake check -L` with a 90-minute timeout. It runs automatically on ready PRs when Nix expressions, the flake lock, Cargo manifests, the Cargo lock, or the workflow itself change. Run it manually before a release or when direct pushes or other changes need Nix verification. See the [Nix packaging guide](../../nix/README.md#package-verification) for the trigger paths and manual workflow.
