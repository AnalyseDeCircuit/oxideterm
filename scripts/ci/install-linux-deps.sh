#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# install-linux-deps.sh — Install Linux system dependencies for OxideTerm.
#
# Shared by CI (check/build/test on Ubuntu) and native-package (cross-platform
# packaging on Linux runners).  Callers can set EXTRA_PACKAGES to install
# additional packages (e.g. patchelf, curl) on top of the base set.
# ---------------------------------------------------------------------------
set -euo pipefail

echo "::group::Install Linux system dependencies"

# GitHub ARM runners can advertise unreachable IPv6 mirrors. Prefer IPv4 and
# let apt retry transient mirror failures before failing the job.
readonly APT_RETRY_COUNT=3
APT_GET_OPTIONS=(
  -o "Acquire::Retries=${APT_RETRY_COUNT}"
  -o Acquire::ForceIPv4=true
)

# GitHub ARM runners have intermittently lost access to the Ubuntu Ports HTTP
# endpoint. Keep the canonical mirror while using its reachable HTTPS endpoint.
readonly UBUNTU_PORTS_HTTP_URI='http://ports.ubuntu.com/ubuntu-ports'
readonly UBUNTU_PORTS_HTTPS_URI='https://ports.ubuntu.com/ubuntu-ports'
APT_SOURCE_FILES=(
  /etc/apt/sources.list
  /etc/apt/sources.list.d/ubuntu.sources
)
for apt_source_file in "${APT_SOURCE_FILES[@]}"; do
  if [[ -f "${apt_source_file}" ]]; then
    sudo sed -i \
      "s|${UBUNTU_PORTS_HTTP_URI}|${UBUNTU_PORTS_HTTPS_URI}|g" \
      "${apt_source_file}"
  fi
done

# Do not continue with stale package indexes when any configured source fails.
sudo apt-get "${APT_GET_OPTIONS[@]}" -o APT::Update::Error-Mode=any update

# GitHub-hosted Ubuntu images may preinstall LLVM libunwind-*-dev packages
# that conflict with libunwind-dev (required by GStreamer dev packages).
# Remove ALL LLVM variants with a wildcard before letting apt solve.
echo "Removing preinstalled LLVM libunwind-*-dev packages…"
sudo apt-get remove -y 'libunwind-[0-9]*-dev' 2>/dev/null || true

# Base set: GPUI + native Rust crate build requirements on Linux.
PACKAGES=(
  build-essential
  libasound2-dev
  libfontconfig1-dev
  libfreetype6-dev
  libgstreamer-plugins-base1.0-dev
  libgstreamer1.0-dev
  libgtk-3-dev
  libjavascriptcoregtk-4.1-dev
  libssl-dev
  libsoup-3.0-dev
  libunwind-dev
  libwebkit2gtk-4.1-dev
  libx11-dev
  libxcb-cursor-dev
  libxcb-icccm4-dev
  libxcb-image0-dev
  libxcb-keysyms1-dev
  libxcb-randr0-dev
  libxcb-render0-dev
  libxcb-shape0-dev
  libxcb-xfixes0-dev
  libxcb-xinerama0-dev
  libxkbcommon-dev
  libxkbcommon-x11-dev
  mesa-vulkan-drivers
  pkg-config
)

# Append any caller-requested extras (space or newline separated).
if [[ -n "${EXTRA_PACKAGES:-}" ]]; then
  # shellcheck disable=SC2206
  PACKAGES+=(${EXTRA_PACKAGES})
fi

echo "Installing packages: ${PACKAGES[*]}"
sudo apt-get "${APT_GET_OPTIONS[@]}" install -y "${PACKAGES[@]}"

echo "::endgroup::"
