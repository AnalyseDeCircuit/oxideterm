---
name: split-crate-by-responsibility
description: Use when refactoring Rust monoliths by extracting crates or modules, especially when a user asks to split a huge .rs file, move responsibilities out of an app crate, reduce crate scope, avoid one-file crates, or verify that a new crate owns real behavior rather than becoming a lib.rs bucket.
---

# Split Crate By Responsibility

## Non-Negotiables

Treat a new crate as a responsibility boundary, not as a storage folder.

- Do not create a crate whose only meaningful file is `lib.rs`, unless it is a tiny facade under 200 lines and all behavior lives in other modules.
- Do not move structs into a crate while leaving all behavior in the old crate. Move the state transitions, validation, storage, protocol conversion, workers, or runtime services that make the data meaningful.
- Do not mix UI concerns into a domain crate. GPUI rendering, focus handles, `ListState`, scroll state, anchors, popover booleans, and visual caches stay in the GPUI app.
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

5. **Keep the app crate thin**
   The app crate should retain:
   - GPUI rendering and layout,
   - focus/selection/list/scroll/popover state,
   - window and action wiring,
   - adapters that need live UI handles.

   The extracted crate should own:
   - domain state and transitions,
   - validation and normalization,
   - stores and registries,
   - protocol DTOs,
   - background service coordination that does not need GPUI types.

6. **Replace broad field sprawl**
   When moving business state out of an app root struct, replace many fields with one focused state object, then update methods to access that object. Do not leave duplicate old fields.

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

- `lib.rs` contains most implementation.
- A crate is mostly `pub use`.
- Types moved but methods stayed behind.
- The old root struct still owns most business fields.
- The new crate imports UI rendering dependencies for non-UI work.
- Tests only prove compilation, not moved behavior.

If any of these happen, do not present the refactor as complete. Either continue splitting or explicitly call it a partial migration.
