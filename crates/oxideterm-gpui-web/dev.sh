#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ "$(wasm-bindgen --version 2>/dev/null || true)" != 'wasm-bindgen 0.2.130' ]]; then
  echo 'Install the matching CLI: cargo install wasm-bindgen-cli --version 0.2.130 --locked' >&2
  exit 1
fi
target_dir=$(cargo metadata --offline --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cargo build --offline -p oxideterm-gpui-web --target wasm32-unknown-unknown
mkdir -p dist/pkg
wasm-bindgen --target web --out-dir dist/pkg "$target_dir/wasm32-unknown-unknown/debug/oxideterm_gpui_web.wasm"
cp index.html dist/
cp ../oxideterm-gpui-app/resources/backgrounds/LICENSE.md dist/background-artwork.md
exec python3 -m http.server "${PORT:-8000}" --bind 127.0.0.1 --directory dist
