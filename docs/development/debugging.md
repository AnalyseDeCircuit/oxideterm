# Debugging And Reproduction

## Start With A Small Reproduction

Record the exact product version, operating system and architecture, protocol or shell, input sequence, expected result, actual result, and whether the issue is platform-specific. For a connection problem, state the authentication mode and server family without exposing credentials.

Use this reproduction record in an issue or review:

```text
Version and commit:
Platform, architecture, display scale, and renderer profile:
Protocol, shell, server family, or local workload:
Steps from a fresh launch:
Expected result:
Actual result:
Does it reproduce after restart or with a clean fixture:
Relevant redacted log lines and screenshot/video:
```

Use a clean configuration directory when possible:

```sh
cargo run -p oxideterm-cli -- --config-dir ./fixture-config doctor --strict
```

Do not use production connection data as a test fixture. Redact hostnames, usernames, local paths, commands, keys, passwords, tokens, and MFA responses before sharing anything.

## Logs And Support Bundles

The desktop log is `oxideterm-native.log` in the `logs` directory beside `settings.json`. Open it from the application's Help settings page, rather than guessing an operating-system-specific data path. The file is size-limited to 10 MiB.

Enable debug logging in the app only for a bounded reproduction, then collect a redacted report with the CLI:

```sh
cargo run -p oxideterm-cli -- paths --json
cargo run -p oxideterm-cli -- diagnose --json
cargo run -p oxideterm-cli -- report --bundle ./oxideterm-report.json --json
```

Enable **Settings → Help → Diagnostics → Debug Logging** only for a bounded reproduction, then restart the app before collecting the report. Review a report before sharing it. A support bundle is safer than copying an entire settings directory, but it can still contain private environment details that need redaction.

For a source-run investigation, `RUST_LOG` overrides the normal filter. Keep it narrow and do not upload raw terminal or credential-bearing output:

```sh
RUST_LOG=oxideterm_gpui_app=debug,oxideterm_ssh=debug cargo run
```

The normal file-log filter already records warnings plus application and SSH information. Avoid leaving verbose logging enabled after the reproduction is complete.

## Platform-Specific Evidence

| Area | Capture |
| --- | --- |
| Windows input or frozen cursor | Windows version, GPU, display scale, the last pointer/key sequence, app log, and whether the problem survives a fresh launch |
| macOS window or text behavior | macOS version, display scale, native/full-screen state, input method, and a screen recording when timing matters |
| Linux windowing or rendering | Distribution, desktop session, Wayland or X11, GPU/driver, renderer profile, and the application log |
| Terminal rendering | Shell, terminal theme, font family, exact emitted bytes or command, alternate-screen status, and a screenshot |
| Connection runtime | Protocol, server family, auth method, proxy or jump-host topology, Connection Monitor state, and redacted log lines |

## Performance Work

Measure the smallest workload that reproduces the regression. Record the command, machine, renderer profile, terminal dimensions, sample duration, and baseline commit. Terminal benchmark material lives under [`benchmark/`](../../benchmark/README.md); use the same workload before and after a change. Do not infer a performance improvement from a single UI impression.

For a rendering or interaction regression, separate three questions before optimizing:

1. Does the terminal or protocol model produce an incorrect snapshot?
2. Does the application schedule excessive invalidation, layout, or paint work?
3. Does the native backend spend time in text, scene, GPU, or presentation work?

This separation avoids treating a visible frame-rate symptom as proof that parser, grid, GPUI, and GPU layers all need changes.
