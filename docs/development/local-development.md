# Local Development

## Baseline

Local builds and CI use Rust `1.97.0`, pinned in `rust-toolchain.toml`:

```sh
rustup toolchain install 1.97.0
cargo run
```

The workspace default member is the native application, so the normal local loop is:

```sh
cargo run
```

If rendering is unavailable on the current machine, use the supported compatibility profile:

```sh
OXIDETERM_RENDER_PROFILE=compatibility cargo run
```

The compatibility profile is a diagnostic tool, not a renderer fix. Record whether a bug changes under it, then investigate the renderer or platform boundary with that evidence.

Build the CLI or optional Linux agent only when the change needs them:

```sh
./scripts/build/build-cli.sh
./scripts/build/build-agent.sh
```

## Platform Notes

### macOS

Use a supported macOS release with Xcode Command Line Tools available. Native window and Metal changes should be checked with `cargo check -p gpui-pre-macos` before the application check. Test real window behavior locally; a compile check cannot prove titlebar, input, cursor, or accessibility behavior.

### Windows

Use the MSVC Rust target and a Visual Studio C++ build environment with a Windows SDK. Run the application from a Developer PowerShell when diagnosing linker or SDK problems. The CI platform check compiles `gpui-pre-windows` and `oxideterm-gpui-app`; it does not replace manual validation of IME, pointer capture, DirectWrite, titlebar, or Direct3D behavior.

Before opening local shells in a development build, stage the pinned ConPTY runtime beside the executable:

```sh
python scripts/release/conpty_runtime.py --target x86_64-pc-windows-msvc --destination target/debug
```

Use `aarch64-pc-windows-msvc` for ARM64 and adjust the destination when using `--target` or `--release`. The script downloads Microsoft ConPTY `1.24.260710001` from NuGet and verifies its SHA-256 digest. Windows installers and portable packages include this runtime automatically under `resources/conpty`; the application loads that exact path. A missing runtime is an error, so an unrelated DLL on `PATH` cannot change local shell behavior.

The Windows keyboard integration test launches an isolated console reader. Stage the runtime beside the test executable before running it:

```sh
python scripts/release/conpty_runtime.py --target x86_64-pc-windows-msvc --destination target/debug/deps
cargo test -p oxideterm-gpui-terminal bundled_conpty_delivers_distinct_ctrl_j_and_enter_events -- --ignored
```

### Linux

The CI dependency list is maintained in [`scripts/ci/install-linux-deps.sh`](../../scripts/ci/install-linux-deps.sh). On an Ubuntu-like development machine, review that list and install the matching packages before building. Test the compositor path that is affected: Wayland and X11 have separate native code paths.

Linux packaging uses Ubuntu 22.04, and the native platform check uses Ubuntu 24.04. A distribution with different package names may need equivalent development packages for X11/XKB, font rendering, GStreamer, audio, Kerberos, OpenSSL, and Vulkan. Keep package substitutions local; do not edit the CI installer merely to match one workstation.

### Nix and Git Dependency Hashes

When building via Nix (`nix build .#oxideterm -L --show-trace`) or Nix Flake checks (`nix flake check -L`), `rustPlatform.buildRustPackage` builds inside an isolated sandbox and requires fixed-output hashes in [`nix/package.nix`](../../nix/package.nix) for all Git dependencies (`ironrdp`, `russh`). Standard crates from crates.io do not need entries in `cargoLock.outputHashes` because their checksums are already recorded in `Cargo.lock`.

Whenever adding or bumping a Git dependency in `Cargo.toml`:
1. Update `Cargo.toml` and update `Cargo.lock` (`cargo check` or `cargo update -p <crate>`).
2. Run `nix build .#oxideterm -L --show-trace`.
3. If a hash mismatch error occurs, verify the locked Git revision in `Cargo.lock`, then copy the `got:` SHA-256 value into `nix/package.nix` under `cargoLock.outputHashes`.
4. Re-run `nix build .#oxideterm -L --show-trace` and `nix flake check -L` to confirm the package builds cleanly.

For detailed documentation, see [`nix/README.md`](../../nix/README.md).

## Common Commands

```sh
# Run the desktop application from the workspace default member.
cargo run

# Compile one product layer without running it.
cargo check -p oxideterm-gpui-app

# Build and run the command-line companion from source.
cargo run -p oxideterm-cli -- doctor --strict

# Inspect one dependency boundary before changing it.
cargo tree -p oxideterm-gpui-app
```

Use `cargo test -p <crate>` for the crate that owns a stable regression. Use a workspace test run only after the change crosses several layers or before merging a broad change.

## Data And Secrets During Development

The desktop application's active data directory is visible and configurable in Settings. Avoid replacing or deleting it while debugging. The CLI supports a separate configuration directory for fixture validation, but `connections open` deliberately targets the application's active data directory and rejects isolated CLI contexts.

Use temporary directories for fixtures and test logs. Keep secrets in supported system storage or short-lived test values; never add real credentials to settings fixtures, snapshots, report bundles, test names, or process arguments.

## Before Calling A Platform Bug Fixed

Compilation only proves the active target builds. For a native behavior change, test the actual window system and record:

- host operating-system version and architecture;
- window mode, display scale, and renderer profile;
- input method or pointer sequence when relevant;
- whether the same path works on another platform.

This record makes it possible to distinguish an application regression from a platform-backend regression without guessing.

## Isolated Data

Do not debug against a user's profile when a clean reproduction is possible. The CLI supports a separate configuration directory:

```sh
cargo run -p oxideterm-cli -- --config-dir ./fixture-config doctor --strict
```

Use a temporary directory for test data, connections, and credentials. Never place real passwords, private keys, tokens, or production hostnames in fixtures, logs, commits, or issue comments.
