# OxideTerm VTE Vendor Patches

This directory is a minimally modified copy of the crates.io `vte` 0.15.0
package. Keep upstream formatting intact so future upgrades can distinguish
OxideTerm's behavioral changes from the imported source.

## Exact Upstream Baseline

- Repository: `https://github.com/alacritty/vte`
- Version: `0.15.0`
- Commit: `3b3da71c34cc1256c7e20981cf03f8eb95e08ffc`
- Crates.io archive SHA-256: `a5924018406ce0063cd67f8e008104968b74b563ee1b85dde3ed1f7cb87d3dbd`
- License: `Apache-2.0 OR MIT`
- Imported: `2026-08-23`

The original `LICENSE-APACHE`, `LICENSE-MIT`, `.cargo_vcs_info.json`, normalized
manifest, and source files are retained from the published package.

## Local Patch Inventory

- `rustfmt.toml` prevents workspace formatting from rewriting the imported
  source.
- `src/lib.rs` adds `Perform::print_text` with a scalar default and dispatches
  printable ground-state spans through it while preserving control ordering and
  the existing partial UTF-8 state machine. A single ASCII byte keeps a direct
  scalar path so control-heavy streams do not pay the batch classification cost.
- `src/lib.rs` also forwards complete printable ASCII lines terminated by CRLF
  through `Perform::print_text_lines`, with a default implementation that
  retains the original print and execute ordering.
- `src/ansi.rs` adds `Handler::input_text` with a scalar default. The ANSI
  performer forwards text spans and complete ASCII line batches, retaining the
  final printable character as `preceding_char` for REP sequences.
- Focused tests cover batch dispatch boundaries and scalar compatibility.

Do not add parsing behavior or terminal semantics to this fork. Its only local
responsibility is carrying printable spans across the existing parser boundary.
