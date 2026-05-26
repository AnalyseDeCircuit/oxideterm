#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <tauri-fixture.oxide> <report-dir>" >&2
  echo "set OXIDE_FIXTURE_PASSWORD to avoid interactive password prompts" >&2
  exit 64
fi

fixture_path="$1"
report_dir="$2"
password_env="${OXIDE_FIXTURE_PASSWORD:-}"

mkdir -p "$report_dir"

password_args=()
if [[ -n "$password_env" ]]; then
  export OXIDE_FIXTURE_PASSWORD
  password_args=(--password-env OXIDE_FIXTURE_PASSWORD)
else
  password_args=(--password-stdin)
fi

cargo run -p oxideterm-cli -- oxide validate "$fixture_path" --json \
  > "$report_dir/validate.json"

if [[ -n "$password_env" ]]; then
  cargo run -p oxideterm-cli -- oxide preview-import "$fixture_path" "${password_args[@]}" --json \
    > "$report_dir/preview-import.json"
  cargo run -p oxideterm-cli -- oxide import "$fixture_path" "${password_args[@]}" --dry-run --json \
    > "$report_dir/import-dry-run.json"
else
  echo "interactive password mode is not supported for multi-step fixture verification" >&2
  echo "set OXIDE_FIXTURE_PASSWORD and rerun" >&2
  exit 64
fi

echo "wrote fixture parity report to $report_dir"
