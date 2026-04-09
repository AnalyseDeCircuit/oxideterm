#!/usr/bin/env bash
# scripts/build-agent.sh
#
# Build bundled OxideTerm remote agent binaries.
#
# Outputs are written to:
#   src-tauri/agents/
#
# Usage:
#   ./scripts/build-agent.sh            # Build both bundled targets
#   ./scripts/build-agent.sh bundled    # Build both bundled targets
#   ./scripts/build-agent.sh x86_64     # Build x86_64 Linux only
#   ./scripts/build-agent.sh aarch64    # Build aarch64 Linux only
#   ./scripts/build-agent.sh list       # Print supported build specs

set -euo pipefail

AGENT_DIR="$(cd "$(dirname "$0")/../agent" && pwd)"
OUTPUT_DIR="$(cd "$(dirname "$0")/../src-tauri/agents" && pwd)"

BUNDLED_TARGETS=(
  "x86_64-linux-musl"
  "aarch64-linux-musl"
)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log() { echo -e "${GREEN}[agent-build]${NC} $*"; }
warn() { echo -e "${YELLOW}[agent-build]${NC} $*"; }
error() { echo -e "${RED}[agent-build]${NC} $*" >&2; }

BUILD_CMD="${USE_CROSS:+cross}"
BUILD_CMD="${BUILD_CMD:-cargo}"
CONTAINER_ENGINE="${CONTAINER_ENGINE:-}"
AUTO_INSTALL_TARGETS="${AUTO_INSTALL_TARGETS:-1}"
HOST_OS="$(uname -s 2>/dev/null || echo unknown)"
HOST_ARCH="$(uname -m 2>/dev/null || echo unknown)"

contains_value() {
  local needle="$1"
  shift
  local item
  for item in "$@"; do
    if [[ "$item" == "$needle" ]]; then
      return 0
    fi
  done
  return 1
}

append_unique() {
  local value="$1"
  if ! contains_value "$value" "${TARGET_SPECS[@]:-}"; then
    TARGET_SPECS+=("$value")
  fi
}

print_usage() {
  cat <<'EOF'
Usage:
  ./scripts/build-agent.sh [bundled|list|SPEC...]

Supported specs:
  bundled
  list
  x86_64
  aarch64
  x86_64-linux-musl
  aarch64-linux-musl

Examples:
  ./scripts/build-agent.sh
  ./scripts/build-agent.sh bundled
  USE_CROSS=1 ./scripts/build-agent.sh x86_64
EOF
}

print_supported_specs() {
  printf '  %s\n' "${BUNDLED_TARGETS[@]}"
}

detect_container_engine() {
  if [[ -n "$CONTAINER_ENGINE" ]]; then
    if command -v "$CONTAINER_ENGINE" >/dev/null 2>&1; then
      echo "$CONTAINER_ENGINE"
      return 0
    fi

    error "CONTAINER_ENGINE is set to '$CONTAINER_ENGINE' but that command is not available."
    return 1
  fi

  if command -v docker >/dev/null 2>&1; then
    echo "docker"
    return 0
  fi

  if command -v podman >/dev/null 2>&1; then
    echo "podman"
    return 0
  fi

  return 1
}

ensure_container_engine_ready() {
  local engine="$1"

  case "$engine" in
    docker)
      if ! docker info >/dev/null 2>&1; then
        error "Docker is installed but not ready. Start Docker Desktop and wait until 'docker info' succeeds."
        exit 1
      fi
      ;;
    podman)
      if ! podman info >/dev/null 2>&1; then
        error "Podman is installed but not ready. Run: podman machine init && podman machine start"
        exit 1
      fi
      ;;
  esac
}

ensure_cross_environment() {
  if [[ "$BUILD_CMD" != "cross" ]]; then
    return 0
  fi

  local engine
  if ! engine="$(detect_container_engine)"; then
    error "cross requires Docker or Podman, but no container engine was found."
    echo ""
    echo "Install one of these, then rerun:"
    echo "  brew install --cask docker"
    echo "  brew install podman"
    echo ""
    echo "Or build with local toolchains without USE_CROSS=1."
    exit 1
  fi

  ensure_container_engine_ready "$engine"
  log "Container engine: $engine"
}

ensure_cross_target_supported() {
  local target="$1"

  if [[ "$BUILD_CMD" == "cross" && "${DETECTED_CONTAINER_ENGINE:-}" == "podman" && "$HOST_OS" == "Darwin" && "$HOST_ARCH" == "arm64" && "$target" == "x86_64-unknown-linux-musl" ]]; then
    error "cross + podman on Apple Silicon is unreliable for $target. Use Docker Desktop or build on an x86_64 host."
    exit 1
  fi
}

run_cross_build() {
  local target="$1"
  local cargo_target_dir="$2"

  if [[ "${DETECTED_CONTAINER_ENGINE:-}" == "docker" && "$HOST_OS" == "Darwin" && "$HOST_ARCH" == "arm64" ]]; then
    log "Using DOCKER_DEFAULT_PLATFORM=linux/amd64 for cross on Apple Silicon"
    (cd "$AGENT_DIR" && DOCKER_DEFAULT_PLATFORM=linux/amd64 CARGO_TARGET_DIR="$cargo_target_dir" cross build --release --target "$target")
    return 0
  fi

  (cd "$AGENT_DIR" && CARGO_TARGET_DIR="$cargo_target_dir" cross build --release --target "$target")
}

ensure_rust_target_installed() {
  local target="$1"
  local installed_targets
  installed_targets="$(rustup target list --installed 2>/dev/null || true)"

  if ! printf '%s\n' "$installed_targets" | grep -qx "$target"; then
    if [[ "$AUTO_INSTALL_TARGETS" == "1" ]]; then
      log "Installing missing Rust target: $target"
      rustup target add "$target"
      return 0
    fi

    error "Rust target '$target' is not installed."
    echo "Install it with:"
    echo "  rustup target add $target"
    exit 1
  fi
}

ensure_local_linker_available() {
  local command_name="$1"
  local target="$2"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    error "Local linker '$command_name' for target '$target' was not found."
    echo "Either install the appropriate cross toolchain, or use:"
    echo "  USE_CROSS=1 ./scripts/build-agent.sh ..."
    exit 1
  fi
}

resolve_target_triple() {
  case "$1" in
    x86_64|x86_64-linux-musl) echo "x86_64-unknown-linux-musl" ;;
    aarch64|aarch64-linux-musl) echo "aarch64-unknown-linux-musl" ;;
    *) return 1 ;;
  esac
}

resolve_output_spec() {
  case "$1" in
    x86_64) echo "x86_64-linux-musl" ;;
    aarch64) echo "aarch64-linux-musl" ;;
    *) echo "$1" ;;
  esac
}

configure_local_linker() {
  local target="$1"

  case "$target" in
    x86_64-unknown-linux-musl)
      export CC_x86_64_unknown_linux_musl="${CC_x86_64_unknown_linux_musl:-x86_64-linux-musl-gcc}"
      export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-x86_64-linux-musl-gcc}"
      ensure_local_linker_available "$CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER" "$target"
      ;;
    aarch64-unknown-linux-musl)
      export CC_aarch64_unknown_linux_musl="${CC_aarch64_unknown_linux_musl:-aarch64-linux-musl-gcc}"
      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER:-aarch64-linux-musl-gcc}"
      ensure_local_linker_available "$CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER" "$target"
      ;;
    *)
      warn "Using cargo for ${target}; make sure the correct linker is installed locally."
      ;;
  esac
}

build_target() {
  local requested_spec="$1"
  local spec
  spec="$(resolve_output_spec "$requested_spec")"
  local target
  target="$(resolve_target_triple "$spec")"
  local output_name="oxideterm-agent-${spec}"
  local cargo_target_dir="$AGENT_DIR/target/build-agent-${spec}"

  log "Building agent for ${target}..."
  ensure_rust_target_installed "$target"
  ensure_cross_target_supported "$target"

  if [[ "$BUILD_CMD" == "cross" ]]; then
    run_cross_build "$target" "$cargo_target_dir"
  else
    configure_local_linker "$target"
    (cd "$AGENT_DIR" && CARGO_TARGET_DIR="$cargo_target_dir" cargo build --release --target "$target")
  fi

  local binary_path="$cargo_target_dir/$target/release/oxideterm-agent"

  if [[ ! -f "$binary_path" ]]; then
    error "Binary not found at $binary_path"
    return 1
  fi

  mkdir -p "$OUTPUT_DIR"
  cp "$binary_path" "$OUTPUT_DIR/$output_name"

  local size
  size=$(wc -c < "$OUTPUT_DIR/$output_name" | tr -d ' ')
  local size_mb
  size_mb=$(echo "scale=1; $size / 1048576" | bc 2>/dev/null || echo "?")

  log "✓ ${output_name} -> ${OUTPUT_DIR} — ${size_mb} MB"
}

TARGET_SPECS=()

if [[ $# -eq 0 ]]; then
  TARGET_SPECS=("${BUNDLED_TARGETS[@]}")
else
  for arg in "$@"; do
    case "$arg" in
      bundled)
        for target_spec in "${BUNDLED_TARGETS[@]}"; do
          append_unique "$target_spec"
        done
        ;;
      list)
        print_supported_specs
        exit 0
        ;;
      help|-h|--help)
        print_usage
        exit 0
        ;;
      *)
        if resolve_target_triple "$(resolve_output_spec "$arg")" >/dev/null 2>&1; then
          append_unique "$(resolve_output_spec "$arg")"
        else
          error "Unknown build spec: $arg"
          echo
          print_usage
          exit 1
        fi
        ;;
    esac
  done
fi

if [[ ${#TARGET_SPECS[@]} -eq 0 ]]; then
  error "No targets selected"
  exit 1
fi

log "Output directory: $OUTPUT_DIR"
log "Build command: $BUILD_CMD"

ensure_cross_environment

if [[ "$BUILD_CMD" == "cross" ]]; then
  DETECTED_CONTAINER_ENGINE="$(detect_container_engine)"
else
  DETECTED_CONTAINER_ENGINE=""
fi

for target in "${TARGET_SPECS[@]}"; do
  build_target "$target"
done

log "Done!"
ls -lh "$OUTPUT_DIR"/oxideterm-agent-* 2>/dev/null || true
