# OxideTerm Alacritty Terminal Vendor Patches

This directory is a minimally modified copy of the crates.io
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
  metadata. Every unsupported case resumes the existing scalar `input` path.
- Batched cells clone the current cursor template, preserve hyperlink and style
  state, and keep cursor wrapping, scrolling, and damage behavior equivalent to
  scalar input.
- `src/term/mod.rs` batches complete short ASCII lines when the cursor and full
  scroll region permit it, advancing the existing grid in bounded groups while
  retaining the scalar path for selections, scrollback views, and complex state.
- Differential tests compare the batch and scalar paths across wrapping,
  scrolling, history, styles, Unicode, DEC charset, insert mode, and wide-cell
  overlap.

Do not expand this fork into a replacement grid or scrollback implementation.
Its only local responsibility is the proven common-case ASCII write path.
