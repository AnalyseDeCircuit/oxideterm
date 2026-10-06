# Nix Packaging and Maintenance Guide

This directory contains the Nix packaging and NixOS module definitions for OxideTerm:

- `package.nix`: Package derivation definition using `pkgs.rustPlatform.buildRustPackage`.
- `module.nix`: NixOS module definition (`programs.oxideterm`).
- `../flake.nix`: Nix Flake exposing default packages, dev shells, overlays, NixOS modules, and checks.

## Common Commands

```sh
# Build the default OxideTerm package
nix build .#oxideterm -L --show-trace

# Run OxideTerm directly from the flake
nix run .#oxideterm

# Check all flake outputs and packages
nix flake check -L

# Enter the Nix development shell
nix develop
```

## Git Dependency Hash Synchronization

### Why `outputHashes` Is Required

Nix builds Rust packages in an isolated network sandbox. During the build:
1. Standard crates from `crates.io` have their checksums recorded directly in `Cargo.lock`. Nix uses these checksums to verify downloaded crates.
2. Git dependencies (such as `russh`) are cloned as fixed-output derivations. Because `Cargo.lock` only records Git commit SHAs (and not Nix tree SHA-256 hashes), Nix requires explicit hashes in `cargoLock.outputHashes` inside `nix/package.nix`.

If a Git dependency commit is bumped or added in `Cargo.toml` / `Cargo.lock` without updating `nix/package.nix`, Nix will fail with a fixed-output derivation hash mismatch:

```text
error: hash mismatch in fixed-output derivation '/nix/store/...-russh-...drv':
  specified: sha256-o9p0ocNLF0QigbvykApRF+OyVbJ9aFgybaf8I+f17Do=
  got:       sha256-ymdLCqupKWdxM96nGE80sOkmorwd99uvJ2bRazA4IYs=
```

### Dependency Update Workflow

Whenever modifying, bumping, or adding a Git dependency in `Cargo.toml`:

1. Update `Cargo.toml` and generate the updated `Cargo.lock` (`cargo check` or `cargo update -p <crate>`).
2. Run the local Nix build to detect any missing or changed hashes:
   ```sh
   nix build .#oxideterm -L --show-trace
   ```
3. If Nix reports a hash mismatch:
   - Verify that the locked Git commit in `Cargo.lock` matches the expected upstream revision.
   - Update the matching crate entry in `nix/package.nix` under `cargoLock.outputHashes` using the `got:` hash from the Nix error output.
4. Run the full build and flake checks to ensure clean completion:
   ```sh
   nix build .#oxideterm -L --show-trace
   nix flake check -L
   ```

## Package Verification

The independent [Nix Package workflow](../.github/workflows/nix-package.yml) runs the package build and flake checks with a 90-minute timeout.

It runs automatically for pushes and pull requests targeting `main` or `experiment/rust-native-v2` when any of these files change:

- Nix expressions under `nix/`, `flake.nix`, or `flake.lock`.
- The root `Cargo.toml`, `Cargo.lock`, or any crate's `Cargo.toml` under `crates/`.
- `.github/workflows/nix-package.yml`.

Before a release, or when other source or build changes need Nix verification, run **Nix Package** manually from GitHub Actions and select the branch to check. The workflow validates the build and Git dependency hashes.
