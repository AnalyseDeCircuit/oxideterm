---
name: split-crate-by-responsibility
description: Use when deciding where new domain or GPUI feature code belongs (avoiding dumping new features into oxideterm-gpui-app or WorkspaceApp), and when refactoring Rust monoliths by extracting crates or modules, avoiding one-file crates, or verifying that an extracted crate owns real state, rendering, and behavior.
---

# Split Crate By Responsibility

## Non-Negotiables

Treat every crate as a responsibility boundary, not a dumping folder—and do not grow `oxideterm-gpui-app` / `WorkspaceApp` by default when adding or refactoring features.

- Place new pure logic, protocols, storage, validation, and runtime services in non-UI domain crates (`oxideterm-*` without `gpui`). Never import GPUI into a domain crate.
- Place new self-contained feature panels, dialogs, editors, and settings pages in an existing or new `oxideterm-gpui-*` feature UI crate—not directly in `oxideterm-gpui-app`.
- Do not create a crate whose only meaningful file is `lib.rs`, unless it is a tiny facade under 200 lines and all behavior lives in other modules.
- Do not move structs into a crate while leaving all behavior in the old crate. Move the state transitions, validation, storage, protocol conversion, workers, or runtime services that make the data meaningful.
- For `oxideterm-gpui-*` feature UI crates, move UI state AND rendering together (`Entity` state, drafts, `ListState`/virtual-list caches, `impl Render` views, localized `cx.notify()`, and UI tests). Do not export stateless `render()` shims driven by full-window `cx.observe(..., |_, _, cx| cx.notify())` hooks in `oxideterm-gpui-app`.
- Keep `oxideterm-gpui-app` (`WorkspaceApp`) limited to window chrome, tab/pane orchestration, global IME/focus/shortcut routing, active runtime registries, and narrow `HostBridge` / `*WindowRequest` adapters.
- Do not leave the source crate compiling only because the new crate re-exports everything. Re-exports are allowed as a migration shim, not as the destination design.
- Do not claim success until file sizes, module boundaries, dependency direction, and tests have been checked.

## Workflow

1. **Audit before editing**
   - List the largest files and current module tree.
   - Identify clusters by behavior: storage, validation, registry, runtime, protocol, UI state, rendering, workers, adapters, tests.
   - Run `scripts/audit_crate_responsibility.py <crate-dir>` when available to catch one-file crates and oversized modules.

2. **Write the responsibility contract**
   Before creating or editing a crate, state:
   - `Owns:` the behaviors the crate will own.
   - `Does not own:` behaviors that must remain elsewhere.
   - `Inputs/outputs:` public API types and side effects.
   - `Dependencies:` allowed dependency direction.
   - `Acceptance:` target module layout, max file sizes, and tests to run.

3. **Design modules first**
   A real extracted crate usually has several of these:
   - `lib.rs`: facade, explicit exports, short crate docs.
   - `types.rs`: shared domain DTOs only when they are not enough to justify their own module.
   - `state.rs`: state container and state transitions.
   - `config.rs` or `storage.rs`: persistence and loading.
   - `validation.rs`: manifest/input invariants.
   - `runtime.rs`, `worker.rs`, or `service.rs`: async/background behavior.
   - `adapter.rs`: conversion between app/UI/backend shapes.
   - `tests.rs` or focused test modules.

4. **Move behavior with the data**
   If a struct moves, move at least one of:
   - constructors and defaults that encode domain rules,
   - mutation methods,
   - validation functions,
   - persistence/load/save code,
   - async delivery/channel coordination,
   - protocol conversion,
   - tests that prove the behavior.

5. **Keep the app crate thin by crate type**
   `oxideterm-gpui-app` should retain only:
   - window chrome, tab/pane orchestration, and active session/node registries,
   - window-level IME, focus, and global shortcut routing,
   - narrow `*WindowRequest` / `HostBridge` adapters for OS dialogs, secret store, terminal/tab launch, and notifications.

   Domain crates (`oxideterm-*` without `gpui`) should own:
   - domain state and transitions,
   - validation and normalization,
   - stores and registries,
   - protocol DTOs,
   - background service coordination without GPUI dependencies.

   Feature UI crates (`oxideterm-gpui-*`) should own:
   - feature state entities, drafts, `ListState`/virtual-list caches, and local timers,
   - self-contained `impl Render` views/dialogs and localized `cx.notify()` (never full-window root `cx.observe -> cx.notify()`),
   - bundled child view handles (e.g., `*Views`) rather than adding one `WorkspaceApp` field per dialog,
   - read-only `*Environment` / frame inputs + unified `*WindowRequest` dispatch back to `oxideterm-gpui-app`.

6. **Replace broad field sprawl**
   When moving business state out of an app root struct, replace many fields with one focused state/views handle, then update methods to access that handle. Do not leave duplicate old fields or add a new `WorkspaceApp` field per sub-dialog.

7. **Validate**
   Run the narrow crate checks first, then the dependent app:
   - `cargo fmt --check`
   - `cargo check -p <new-crate>`
   - relevant `cargo test -p <new-crate>`
   - `cargo check -p <old-app-crate>`
   - relevant old-crate tests

## Acceptance Gates

Use these gates unless the repo has stricter local standards:

- `lib.rs` should normally be under 200 lines and act as a facade.
- No non-test implementation file should remain above 1500 lines without a written reason.
- A newly extracted crate must contain at least two responsibility modules beyond `lib.rs` for non-trivial work.
- The old crate should lose both fields/types and behavior, not merely import the same monolith from a new location.
- The new crate should not depend on the old app crate.
- Domain crates should not import GPUI or app-local UI modules.

## Failure Patterns To Stop And Fix

Stop immediately when you see:

- New feature panels, dialogs, or domain logic dumped directly into `oxideterm-gpui-app` / `WorkspaceApp` instead of an appropriate domain or `oxideterm-gpui-*` crate.
- `lib.rs` contains most implementation, or a crate is mostly `pub use`.
- Types moved but methods stayed behind, or the old root struct still owns most business fields (or grows one new field per dialog).
- A `oxideterm-gpui-*` UI crate leaves `impl Render` in `WorkspaceApp` or relies on root `cx.observe(..., |_, _, cx| cx.notify())` repaints for local edits/streams/frames.
- A non-UI domain crate imports GPUI or app-local UI modules.
- Tests only prove compilation, not moved behavior.

If any of these happen, do not present the work as complete. Either continue splitting or explicitly call it a partial migration.
