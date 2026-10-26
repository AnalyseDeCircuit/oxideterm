# OxideTerm Alacritty Terminal Vendor Patches

This directory is an OxideTerm-maintained fork of the crates.io
`alacritty_terminal` 0.26.0 package. Keep upstream formatting intact so future
upgrades can distinguish OxideTerm's behavioral changes from the imported
source.

## Exact Upstream Baseline

- Repository: `https://github.com/alacritty/alacritty`
- Upstream path: `alacritty_terminal`
- Version: `0.26.0`
- Commit: `94e7c8874e526b1e67b349d9ba30ddf81669119e`
- Crates.io archive SHA-256: `bda177466b9524d59f1b12f0dd30b68696788e9992a7e959021c4a0ed96fcf59`
- License: `Apache-2.0`
- Imported: `2026-08-23`

The original `LICENSE-APACHE`, `.cargo_vcs_info.json`, normalized manifest, and
source files are retained from the published package.

## Local Patch Inventory

- `rustfmt.toml` prevents workspace formatting from rewriting the imported
  source.
- `src/term/mod.rs` implements `Handler::input_text` for printable ASCII spans.
- The fast path is restricted to the ASCII charset, line-wrap mode, overwrite
  mode, single-column printable ASCII, and destinations without wide-cell
  metadata. It uses VTE's printable-text callback contract with `str::is_ascii`
  to avoid rescanning every byte; unsupported terminal state resumes the scalar
  `input` path.
- Batched cells clone the current cursor template, preserve hyperlink and style
  state, and keep cursor wrapping, scrolling, and damage behavior equivalent to
  scalar input.
- `src/term/mod.rs` batches complete short ASCII lines when the cursor and full
  scroll region permit it, advancing the existing grid in bounded groups while
  retaining the scalar path for selections, scrollback views, and complex state.
- `src/term/mod.rs` reports accumulated full-viewport upward scrolling separately
  from incompatible full damage. This adds no work to printable-cell writes and
  lets OxideTerm reuse retained snapshot rows after ordinary output scrolling.
- Differential tests compare the batch and scalar paths across wrapping,
  scrolling, history, styles, Unicode, DEC charset, insert mode, and wide-cell
  overlap.

- `src/grid/row.rs` stores retained history as a cell prefix and an exact repeated
  tail, while keeping writable screen rows dense. Logical reads and reference
  serialization do not expand history. Mutations preserve row identity.
- `src/grid/mod.rs` transfers dense allocations from newly compacted history to
  exposed screen rows. `storage.rs` prepares compact spare slots; `resize.rs`
  restores writable rows and compacts completed history rows during reflow.
- Terminal snapshot, text extraction, search, and line-length readers use logical
  row iteration instead of assuming that history is a contiguous cell slice.
- `src/term/cell/history.rs` adds eight-byte history cells (character, all flags,
  and a row-local attribute index). Exact colors and `CellExtra` values are shared
  without quantization. Repeated-tail compression remains available in both
  representations. Conversion requires at least 25% estimated storage savings;
  attribute discovery is bounded at 16 distinct entries to avoid expensive
  conversion attempts on high-entropy streams. Rejected rows retain full cells.
- `src/grid/history.rs` defines borrowed cell reads and cell-specific history
  storage. Grid and row cell access is explicit (`cell` / `cell_mut`); immutable
  reads, search, selection, and serialization never materialize history. Writable
  screen rows retain the original 24-byte cells. Scrolling reuses both evicted
  packed storage and departing dense screen buffers. Narrowing releases obsolete
  packed capacity; growing default tails does not allocate every implicit column.
- `src/term/cell/history/text.rs` stores suitable history as UTF-8 primary
  characters and exact attribute/flag runs. Admission compares retained capacities
  and limits run discovery to 16 runs; more fragmented rows use eight-byte indexed
  cells or full cells. Wide placeholders and combining metadata retain Alacritty's
  cell semantics rather than being reconstructed from grapheme widths.
- Row readers have native double-ended iterators and run-wise folds. Grid readers
  retain a seek cursor and cache the current cell view. ASCII skipping is constant
  time; Unicode traversal preserves byte offsets instead of rescanning from the
  row start. Mutation expands the affected row; shrinking can decode only the
  removed stored span. Text decoding operates by attribute run.
- The runtime background-search adapter can copy exact ASCII history prefixes
  directly, preserving repeated tails and column limits. Matching still runs
  outside the emulator lock, after bounded chunk copying.
- ASCII input inspects wide-cell overlap directly in its writable screen span,
  keeping history dispatch out of the output hot path. Reference fixtures and
  batch/scalar differential tests preserve the existing write semantics.

The maintained boundary includes the ASCII write path and compact row storage,
including its scrolling, reflow, and read adapters. Protocol interpretation,
terminal modes, and logical cell contents continue to follow upstream behavior.
