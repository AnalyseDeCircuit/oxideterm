# Testing And Fixtures

Tests exist to protect a user-visible contract or a high-risk boundary. They are not a checklist for every private helper or a substitute for manual validation of a native window, GPU, server, or input method.

## Choose The Test Level

| Contract | Preferred coverage | Example boundary |
| --- | --- | --- |
| Pure parsing, normalization, serialization, or state transition | Unit test in the owning crate | Settings migration, terminal parser, connection identity normalization |
| GPUI interaction that can run without a real platform backend | Existing `gpui::test` or `TestAppContext` test module | Modal ownership, keyboard routing, terminal interaction state |
| SSH, SFTP, proxy, or forwarding behavior with a real protocol dependency | Existing focused integration test only when a controlled fixture exists | SSH upstream-proxy end-to-end coverage |
| Native input, window, renderer, IME, or platform capture | Manual host validation plus a narrow logic test when possible | Windows pointer capture recovery, macOS titlebar behavior |
| Performance characteristic | Benchmark before and after, using the same workload | Terminal parse or scroll throughput |

## Test Placement

Keep a test beside the code that owns the behavior:

- Module tests belong in the owning Rust file or its existing `tests` submodule.
- Terminal interaction and viewport tests live under `crates/oxideterm-gpui-terminal/src/terminal_view/tests/` or the owning terminal app module.
- Workspace feature tests stay with their feature module, such as `workspace/sftp/`, `workspace/connection_monitor/`, or `workspace/session_manager/`.
- Repository automation tests live under `scripts/tests/`, `scripts/quality/tests/`, or `scripts/automation/tests/` according to the script owner.

Do not create a broad test utility crate merely to share one fixture. Reuse the closest existing constructor, builder, or test support module first.

## GPUI Tests

`oxideterm-gpui-app` and `oxideterm-gpui-terminal` already use GPUI test contexts for application-owned behavior. These tests are useful for deterministic focus, modal, selection, and state-machine behavior, but they do not prove operating-system message delivery or GPU presentation.

When a UI defect is platform-specific, split the evidence:

1. Add a focused GPUI test only for the platform-independent state transition.
2. Record the manual reproduction on the affected operating system.
3. Keep the native platform check in the verification scope.

This avoids tests that pretend a synthetic window reproduces Win32, AppKit, X11, or Wayland behavior.

## Fixtures And Data

Use `tempfile` or the existing temporary-directory helpers for settings, export, and file fixtures. Fixture data must be deterministic, small enough for the test's purpose, and safe to include in source control.

Never put production hosts, usernames, passwords, private keys, access tokens, cloud-sync endpoints, or copied terminal transcripts into fixtures. Test authentication and secret behavior with explicitly synthetic values, then assert that diagnostics, `Debug`, serialization, and user-facing errors do not reveal them.

For a network test, prefer a controlled local server or an existing integration fixture. Do not make unit tests depend on public hosts, a personal JumpServer, a real smart card, an attached USB device, or a developer's SSH agent.

## Asynchronous And Lifecycle Tests

Avoid arbitrary sleeps as a way to make a test pass. Wait for the observable state transition, task completion, or explicit test executor advance supplied by the existing test harness. A test must also clean up its spawned task, listener, fixture directory, and consumer registration.

For node ownership changes, test the important negative case: closing one consumer must not stop unrelated consumers. For failure paths, assert that failed setup does not leave a registry consumer, projection, or background task behind.

## Before Adding A Test

Ask four questions:

1. What user-visible regression would this catch?
2. Can the contract be exercised without private infrastructure or nondeterministic timing?
3. Is there already a closer test that should be extended?
4. Will a manual platform check still be required?

Run the focused command from the [verification matrix](verification.md) after changing tests. Do not weaken an existing test to accommodate a behavior change until the old contract has been shown to be wrong or intentionally retired.
