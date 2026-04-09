# OxideTerm Agent Binaries

This directory documents how OxideTerm packages the remote agent for IDE mode.

## Bundled Architectures

Only these two Linux targets are bundled with the application and auto-deployed:

- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`

## Building

Use the unified build script from the repository root:

```bash
# Build both bundled targets
./scripts/build-agent.sh

# Build a single bundled target
USE_CROSS=1 ./scripts/build-agent.sh x86_64
USE_CROSS=1 ./scripts/build-agent.sh aarch64
```

Bundled outputs are written to `src-tauri/agents/`.

Recommended: [cross](https://github.com/cross-rs/cross) + Docker

If you use `USE_CROSS=1`, make sure Docker or Podman is installed locally.

On Apple Silicon macOS, prefer Docker Desktop. `cross` with Podman may fail on the x86_64 Linux target under qemu emulation.

The build script auto-installs missing Rust targets by default. Set `AUTO_INSTALL_TARGETS=0` if you want manual control.

## Other Architectures

Prebuilt binaries for other architectures are no longer published in this repository.

If you need an unsupported architecture, build the agent yourself from source with `cargo` or `cross`, then upload the resulting `oxideterm-agent` binary to `~/.oxideterm/oxideterm-agent` on the remote host.
