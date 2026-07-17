#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
AGENT_MANIFEST="$ROOT_DIR/agent/Cargo.toml"
OUT_DIR="$ROOT_DIR/crates/oxideterm-gpui-app/resources/agents"
TARGETS=(
  "x86_64-unknown-linux-musl:x86_64-linux-musl"
  "aarch64-unknown-linux-musl:aarch64-linux-musl"
)

mkdir -p "$OUT_DIR"

export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-rust-lld}"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER:-rust-lld}"

for pair in "${TARGETS[@]}"; do
  rust_target="${pair%%:*}"
  artifact_suffix="${pair##*:}"
  echo "==> Building oxideterm-agent for $rust_target"
  cargo build --manifest-path "$AGENT_MANIFEST" --release --target "$rust_target"
  cp "$ROOT_DIR/agent/target/$rust_target/release/oxideterm-agent" \
    "$OUT_DIR/oxideterm-agent-$artifact_suffix"
  chmod +x "$OUT_DIR/oxideterm-agent-$artifact_suffix"
  if command -v llvm-strip >/dev/null 2>&1; then
    # llvm-strip handles both bundled Linux agent targets from one host runner.
    llvm-strip "$OUT_DIR/oxideterm-agent-$artifact_suffix"
  fi
done

echo "Agent artifacts written to $OUT_DIR"
