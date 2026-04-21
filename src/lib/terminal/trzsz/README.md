# trzsz Integration Baseline

Status: Phase 0.5 completed on 2026-04-22

This directory is the OxideTerm-owned adapter boundary for trzsz integration. Phase 0 deliberately avoids runtime logic changes and only freezes upstream metadata, repository layout, and the future file map.

## Frozen Upstream

- npm package: `trzsz@1.1.6`
- npm dist-tag used for freeze: `latest = 1.1.6`
- upstream repository: `git+https://github.com/trzsz/trzsz.js.git`
- release commit from npm registry `gitHead`: `bac0e5fca4034e1f377ad48949d1af75bd303823`
- freeze date: `2026-04-15`
- chosen integration mode: vendored minimal fork under `src/lib/terminal/trzsz/upstream/`

The freeze source of truth is the npm-published package version plus `gitHead`. If a future upgrade needs to diff upstream, use this commit as the baseline before comparing local modifications.

## Phase 0 Verification Notes

- Frontend dependency install check passed with `pnpm install --frozen-lockfile --ignore-scripts`.
- No existing `trzsz` implementation was found in `src/` or `src-tauri/` before creating this baseline.
- Existing terminal integration touchpoints were reviewed:
  - `src/components/terminal/TerminalView.tsx`
  - `src/components/terminal/LocalTerminalView.tsx`
  - `src/lib/api.ts`
- Existing native dialog usage was reviewed and is sufficient for Phase 3 dialog wiring:
  - package dependency: `@tauri-apps/plugin-dialog`
  - example usage: `src/components/modals/NewConnectionModal.tsx`

## Why Stock Browser / Node Paths Are Not Acceptable

OxideTerm runs inside a Tauri WebView. That environment is neither a Node.js runtime with direct `fs` access nor a reliable target for the stock browser save path.

The integration must therefore:

1. keep trzsz protocol and transfer logic intact;
2. replace file selection with `@tauri-apps/plugin-dialog`;
3. replace file reads and writes with Tauri commands;
4. block fallback to the stock browser auto-save path.

## Allowed Modification Boundary

Allowed local modifications are limited to the following categories:

1. environment detection for Tauri vs stock browser vs Node;
2. file selection injection points;
3. file reader / writer adapter injection points;
4. type exports required by OxideTerm controller code;
5. minimal wrapper glue needed to expose a stable filter entry to OxideTerm.

The following are out of bounds for the local fork unless the plan is explicitly revised:

1. trzsz protocol state machine changes;
2. frame parsing behavior changes unrelated to Tauri integration;
3. transport semantics changes;
4. product-specific UI logic inside vendored upstream files.

## Directory Contract

- `README.md`: Phase 0 freeze record and future file map.
- `upstream/`: vendored upstream or minimal fork files only.
- root-level TypeScript files in this directory: OxideTerm-owned adapter layer, controller, and Tauri bridge wrappers.

Do not move or rename this directory layout without updating this file and the implementation plan.

## Planned File Map By Phase

This is the reserved naming map for Phase 1 through Phase 5. Minor supporting files may be added later, but the main implementation should use these paths unless the plan is updated first.

### Phase 1

Modified files:

- `src/components/terminal/TerminalView.tsx`
- `src/lib/api.ts`

New files:

- `src/lib/terminal/trzsz/controller.ts`
- `src/lib/terminal/trzsz/transport.ts`
- `src/lib/terminal/trzsz/capabilities.ts`

Phase 1 completed on `2026-04-21`.

Implemented outputs:

1. `TerminalView` now owns a per-runtime `TrzszController` placeholder bound to `sessionId + connectionId + wsUrl`.
2. Remote terminals now register `onBinary` and dispose it on teardown.
3. Server output flows through `processServerOutput(...)` before renderer write scheduling.
4. Interactive input flows through `processTerminalInput(...)` / `processBinaryInput(...)` before WebSocket send.
5. Resize now updates controller columns and uses the shared transport adapter.
6. `api.getTrzszCapabilities()` exists as a safe placeholder probe that degrades to `unavailable` when the backend command does not exist yet.
7. Controller teardown is wired into disconnect, reconnect, and unmount cleanup paths.

### Phase 2

Modified files:

- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`

New files:

- `src-tauri/src/commands/trzsz.rs`
- `src-tauri/src/trzsz/mod.rs`
- `src-tauri/src/trzsz/error.rs`
- `src-tauri/src/trzsz/path_guard.rs`
- `src-tauri/src/trzsz/upload.rs`
- `src-tauri/src/trzsz/download.rs`

Phase 2 completed on `2026-04-21`.

Implemented outputs:

1. `trzsz_get_capabilities()` now provides an explicit backend handshake with `apiVersion` and feature flags.
2. Upload-side commands now scan selected files/directories, reject unsafe paths and symlink escapes, open owner-scoped reader handles, stream chunk reads, and close handles explicitly.
3. Download-side commands now prepare a canonical root, create parent directories safely, write through temp files, flush and rename on finish, and remove temp artifacts on abort.
4. All local path validation is centralized in `path_guard.rs`, including NFC normalization, traversal rejection, Windows reserved-name filtering, canonical root enforcement, per-segment symlink rejection, and finish-time target revalidation before final rename.
5. Rust-side `TrzszState` now tracks owner-scoped upload/download handles with a default 15-minute TTL, a hard cap of 256 active handles per owner, and forced reclamation through `trzsz_cleanup_owner(ownerId)`.
6. Phase 2 backend coverage now includes path-guard, upload, and download tests, and the module passes `cargo check` plus targeted `cargo test trzsz:: --lib`.

### Phase 3

Modified files:

- `src/components/terminal/TerminalView.tsx`
- `src/lib/api.ts`
- `src/lib/terminal/trzsz/controller.ts`
- `src/lib/terminal/trzsz/upstream/` (vendored fork content)

New files:

- `src/lib/terminal/trzsz/types.ts`
- `src/lib/terminal/trzsz/TauriFileReader.ts`
- `src/lib/terminal/trzsz/TauriFileWriter.ts`
- `src/lib/terminal/trzsz/dialogs.ts`

Phase 3 completed on `2026-04-21`.

Implemented outputs:

1. `TerminalView` now instantiates a real `TrzszController` runtime with owner-scoped cleanup via `trzsz_cleanup_owner(ownerId)`.
2. Frontend Tauri dialog adapters now handle upload file selection, upload directory selection, and download root selection with consistent cancel semantics.
3. `TauriFileReader` now maps upstream reader semantics onto Phase 2 upload handles, including recursive directory entry hydration and explicit chunked reads.
4. `TauriFileWriter` now maps upstream writer semantics onto Phase 2 download handles, including owner-scoped directory creation, temp-file writes, finish-after-MD5, abort cleanup, and frontend collision naming for top-level downloads.
5. OxideTerm now vendors the trzsz `1.1.6` protocol core under `upstream/` and replaces stock browser/node file-system branches with injected Tauri capabilities.
6. `TrzszController` now routes live server output, terminal input, binary input, upload selection, and download save requests through the vendored filter instead of the Phase 1 passthrough placeholder.
7. Phase 3 validation passed with targeted `vitest` coverage for `src/test/lib/terminal/trzsz/controller.test.ts` plus `cargo check` and `cargo test trzsz:: --lib` on the Rust bridge.

### Phase 0.5

Modified files:

- `src/lib/terminal/trzsz/README.md`
- `src/lib/terminal/trzsz/UPSTREAM_DIFF.md`
- `scripts/local-verify.cjs`
- `package.json`

New files:

- `scripts/check-trzsz-version.cjs`
- `scripts/trzsz-regression.cjs`
- `src/test/lib/terminal/trzsz/TauriFileReader.test.ts`

Phase 0.5 completed on `2026-04-22`.

Implemented outputs:

1. `UPSTREAM_DIFF.md` now records the vendored upstream repository, npm version, commit, modified file list, and per-file maintenance rationale next to the vendored fork.
2. `scripts/check-trzsz-version.cjs` now verifies that the frozen version and commit recorded in `README.md`, `UPSTREAM_DIFF.md`, and `upstream/comm.ts` stay in sync and that every documented vendored file still exists.
3. `scripts/trzsz-regression.cjs` now provides a repeatable maintenance matrix covering single-file upload, directory upload, single-file download, directory download, cancel cleanup, malicious path rejection, and full frontend/Rust suites.
4. `src/test/lib/terminal/trzsz/TauriFileReader.test.ts` now gives the maintenance matrix an explicit frontend anchor for upload-reader semantics and recursive directory upload entry hydration.
5. `package.json` now exposes `pnpm trzsz:check-fork`, `pnpm trzsz:regression:list`, `pnpm trzsz:regression`, and `pnpm trzsz:maintain` as the stable fork-maintenance entrypoints.
6. `scripts/local-verify.cjs` now prints a post-verify maintainer hint that points directly at the trzsz fork metadata and regression entrypoints.

### Phase 4

Modified files:

- `src/store/settingsStore.ts`
- `src/components/settings/tabs/TerminalTab.tsx`
- `src/components/terminal/TerminalView.tsx`
- `src/lib/terminal/trzsz/controller.ts`
- `src/locales/en/terminal.json`
- `src/locales/en/settings.json`

New files:

- none expected as primary integration files

### Phase 5

Modified files:

- `src/test/store/settingsStore.test.ts`
- `src-tauri/src/trzsz/path_guard.rs`
- `src-tauri/src/trzsz/upload.rs`
- `src-tauri/src/trzsz/download.rs`

New files:

- `src/test/lib/terminal/trzsz/controller.test.ts`
- `src/test/lib/terminal/trzsz/dialogs.test.ts`
- `src/test/components/terminal/TerminalView.trzsz.test.tsx`

## Naming Rules Locked By Phase 0

1. Do not create a second parallel trzsz adapter root elsewhere in `src/`.
2. Do not introduce a new package workspace for trzsz unless this file and the plan are both updated.
3. Keep OxideTerm-owned glue code outside `upstream/` whenever possible.
4. Keep vendored upstream diffs explainable file by file in Phase 0.5.
