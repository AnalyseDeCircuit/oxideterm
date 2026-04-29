# trzsz Vendored Fork Diff

- Upstream repository: `https://github.com/trzsz/trzsz.js`
- Upstream npm version: `1.1.6`
- Upstream commit: `bac0e5fca4034e1f377ad48949d1af75bd303823`
- Vendored snapshot root: `src/lib/terminal/trzsz/upstream/`
- Last reviewed against local fork: `2026-04-22`

## Maintenance Entry Points

- Metadata check: `pnpm trzsz:check-fork`
- Regression list: `pnpm trzsz:regression:list`
- Regression run: `pnpm trzsz:regression`
- Combined maintenance pass: `pnpm trzsz:maintain`

`pnpm trzsz:maintain` is the default pre-upgrade checkpoint. It validates the recorded upstream version, then runs the pinned regression matrix used for fork maintenance.

## Vendored File Map

All files below are local vendored copies. The `reason` column only describes OxideTerm-specific deltas; ordinary TypeScript path rewrites required by vendoring are omitted unless they are the only change in that file.

- `buffer.ts`: local browser/Tauri compatibility fixes for binary buffer handling used by the vendored transfer core.
- `comm.ts`: keeps the vendored version marker and extends file-writer lifecycle types with `commitFile`, `finishFile`, and `abortFile` so OxideTerm can map temp-file downloads onto Tauri IPC safely.
- `escape.ts`: vendored helper retained with no OxideTerm behavior change beyond import-path normalization.
- `filter.ts`: replaces stock browser and Node file-system entry points with injected Tauri dialogs/readers/writers, adds controller-safe disposal, split-frame handshake buffering, duplicate-handshake suppression for Windows short unique IDs, and Go trzsz 1.2.0 `uniqueId:port` handshake compatibility.
- `options.ts`: narrows upstream options into the explicit OxideTerm injection surface used by `TrzszController`.
- `progress.ts`: vendored helper retained with no OxideTerm behavior change beyond import-path normalization.
- `transfer.ts`: aligns upstream receive/write flow with OxideTerm temp-file semantics, rollback cleanup, explicit directory commit, timeout cleanup, zero-length read protection, and local no-overwrite policy.

## Upgrade Checklist

When rebasing this fork onto a newer upstream release:

1. Vendor the new upstream snapshot into `src/lib/terminal/trzsz/upstream/` without mixing unrelated OxideTerm edits.
2. Update the version and commit bullets in this file and in `src/lib/terminal/trzsz/README.md`.
3. Run `pnpm trzsz:check-fork` before and after any conflict resolution.
4. Run `pnpm trzsz:regression` after the fork compiles.
5. Only then review the local diff in `filter.ts`, `transfer.ts`, `comm.ts`, `buffer.ts`, and `options.ts` for any upstream semantic drift.

## Regression Matrix Coverage

The maintenance matrix is encoded in `scripts/trzsz-regression.cjs` and currently pins these minimum cases:

- single-file upload
- directory upload
- single-file download
- directory download
- cancel cleanup
- malicious path rejection
- full frontend trzsz suite
- full Rust trzsz suite

Do not replace the scripted matrix with prose-only notes. If a future upgrade adds a new required invariant, update the script and this file in the same change.
