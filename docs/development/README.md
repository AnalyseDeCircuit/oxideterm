# OxideTerm Development Guide

This guide is the entry point for contributors working on the native desktop application. Product workflows belong in the [user guide](../user-guide/en/README.md); personal investigations and future proposals under `docs/local/` are not development contracts.

## Start Here

1. Read the repository [contribution guidance](../CONTRIBUTING.md) and the root `AGENTS.md` before choosing an implementation.
2. Use the [crate map](crate-map.md) to identify the owning layer. Product behavior normally belongs outside `crates/gpui-ce/`.
3. Set up a local machine with [local development](local-development.md), then run the narrow checks in the [verification matrix](verification.md).
4. Read [runtime ownership](runtime-ownership.md) before changing SSH nodes, terminal panes, SFTP, forwarding, reconnection, or background work.

## First Contribution Workflow

Use this sequence for a focused bug fix or small feature:

1. State the observable contract: what the user does, what should happen, and on which platform or protocol it matters.
2. Identify the owner with the [crate map](crate-map.md). A view that displays state is often not the runtime owner of that state.
3. Read the closest implementation and existing focused tests before deciding on a fix. Do not infer a platform or transport bug from a screenshot alone.
4. Keep the change inside the owning layer. Add a shared primitive only when two or more real call sites require the same contract.
5. Run the focused checks and manual workflow selected by the [verification matrix](verification.md).
6. Review `git diff`, `git diff --check`, and `git status` before staging only the files belonging to the change.

## How The Repository Is Organized

The application is built from four concentric layers:

```text
User workflow
  -> workspace and feature UI
  -> product services and protocol adapters
  -> terminal, SSH, storage, and other domain crates
  -> GPUI CE and operating-system backends
```

Move inward only when evidence says the outer layer is correct. For example, an SFTP button that opens the wrong connection normally belongs in workspace identity resolution, not in the SSH transport or a GPUI hit-test path.

## Topic Map

| Need | Read |
| --- | --- |
| Find the correct crate or entry point | [Crate map](crate-map.md) |
| Build and run on macOS, Windows, or Linux | [Local development](local-development.md) |
| Choose checks for a focused change | [Verification matrix](verification.md) |
| Change a shared SSH connection or long-running task | [Runtime ownership](runtime-ownership.md) |
| Investigate or modify the vendored UI framework | [GPUI CE maintenance](gpui-ce.md) |
| Collect a minimal reproduction, logs, or a performance sample | [Debugging](debugging.md) |
| Write or review regression coverage and fixtures | [Testing and fixtures](testing-and-fixtures.md) |
| Add product copy or a localized control | [Internationalization and product copy](i18n-and-product-copy.md) |
| Handle credentials, diagnostics, or external process input | [Secrets and sensitive data](secrets-and-sensitive-data.md) |
| Change persisted settings, export, sync, or migration behavior | [Settings, data, and migrations](settings-data-and-migrations.md) |
| Make a performance claim or change a hot path | [Performance and benchmarking](performance-and-benchmarking.md) |
| Prepare a stable release or repair release assets | [Release process](release-process.md) |

## Scope

These pages describe current repository behavior and contributor workflows. They do not replace the system-level rules in [System Invariants](../SYSTEM_INVARIANTS.md), which remain the authority for input routing, modal layering, rendering boundaries, session behavior, and verification expectations.

Use the documentation by audience:

- `docs/development/` explains how to change and validate the repository.
- `docs/user-guide/` explains supported product behavior.
- `docs/design/` records durable public or architectural contracts.
- `docs/local/` contains maintainer planning and investigation material; it is not a stable contribution API.
