#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_ROOT="$ROOT_DIR/crates/oxideterm-gpui-app/resources/cli-bin"
TARGET_TRIPLE="${1:-}"
PROFILE="${OXIDETERM_CLI_PROFILE:-release}"
BIN_NAME="oxideterm"

if [[ "$PROFILE" != "release" ]]; then
  echo "Only release CLI packaging is supported; set OXIDETERM_CLI_PROFILE=release." >&2
  exit 2
fi

build_args=(-p oxideterm-cli --release)
if [[ -n "$TARGET_TRIPLE" ]]; then
  build_args+=(--target "$TARGET_TRIPLE")
fi

echo "==> Building OxideTerm CLI${TARGET_TRIPLE:+ for $TARGET_TRIPLE}"
# Build the CLI as a standalone release artifact before GPUI app bundling.
export CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$ROOT_DIR/target/clang-module-cache}"
mkdir -p "$CLANG_MODULE_CACHE_PATH"
cargo build "${build_args[@]}"

HOST_TRIPLE="$(rustc -vV | awk '/host:/ { print $2 }')"
ARTIFACT_TRIPLE="${TARGET_TRIPLE:-$HOST_TRIPLE}"
SOURCE_BIN="$ROOT_DIR/target/$ARTIFACT_TRIPLE/release/$BIN_NAME"
if [[ -z "$TARGET_TRIPLE" ]]; then
  SOURCE_BIN="$ROOT_DIR/target/release/$BIN_NAME"
fi

if [[ "$ARTIFACT_TRIPLE" == *windows* ]]; then
  SOURCE_BIN="${SOURCE_BIN}.exe"
fi

OUT_DIR="$OUT_ROOT/$ARTIFACT_TRIPLE"
mkdir -p "$OUT_DIR"

# The bundle resource keeps per-target subdirectories so universal packaging can
# stage multiple CLI binaries without changing the app's Cargo bundle config.
DEST_BIN="$OUT_DIR/$(basename "$SOURCE_BIN")"
cp "$SOURCE_BIN" "$DEST_BIN"
chmod +x "$DEST_BIN" 2>/dev/null || true

echo "CLI artifact written to $DEST_BIN"
