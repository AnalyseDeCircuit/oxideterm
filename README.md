# OxideTerm Native v2

Clean Rust-native experiment branch for OxideTerm.

This branch intentionally does **not** contain the previous Tauri/React app.
The first target is a fully usable local terminal built from:

- `alacritty_terminal` for PTY spawning, event loop I/O, ANSI parsing, grid, modes, and scrollback state
- `gpui` for the native product shell and terminal surface

Run:

```sh
cargo run -p oxideterm-native
```

This is a rewrite lab, not a migration branch. The goal is to build native
foundations without carrying the old frontend dependency graph along for the
ride.
