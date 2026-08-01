#!/bin/sh

# Measure fixture emission while leaving the workload attached to the active terminal.
set -eu

benchmark_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fixture_size_mib=${OXIDETERM_BENCHMARK_SIZE_MIB:-16}
workload=${1:-all}

usage() {
    printf '%s\n' 'Usage: benchmark/measure.sh [plain|ansi|unicode|long-csi|all]' >&2
}

case "$workload" in
    plain|ansi|unicode|long-csi)
        workload_count=1
        ;;
    all)
        workload_count=4
        ;;
    -h|--help)
        usage
        exit 0
        ;;
    *)
        usage
        exit 2
        ;;
esac

case "$fixture_size_mib" in
    *[!0-9]*|'')
        printf '%s\n' 'OXIDETERM_BENCHMARK_SIZE_MIB must be a positive integer.' >&2
        exit 2
        ;;
esac

if [ "$fixture_size_mib" -eq 0 ]; then
    printf '%s\n' 'OXIDETERM_BENCHMARK_SIZE_MIB must be greater than zero.' >&2
    exit 2
fi

if ! command -v python3 >/dev/null 2>&1; then
    printf '%s\n' 'benchmark/measure.sh requires python3 for monotonic timing.' >&2
    exit 2
fi

# Validate files before starting the timer so file setup is not measured.
"$benchmark_root/verify.sh" >/dev/null
emitted_bytes=$((fixture_size_mib * 1024 * 1024 * workload_count))

exec python3 - "$benchmark_root/run.sh" "$workload" "$fixture_size_mib" "$emitted_bytes" <<'PYTHON'
import json
import subprocess
import sys
import time

run_script, workload, fixture_size_mib, emitted_bytes = sys.argv[1:]
started_at = time.monotonic()
completed = subprocess.run([run_script, workload], check=False)
elapsed_seconds = time.monotonic() - started_at
emitted_bytes = int(emitted_bytes)

result = {
    "bytes": emitted_bytes,
    "elapsed_ms": round(elapsed_seconds * 1_000, 3),
    "fixture_size_mib": int(fixture_size_mib),
    "pty_mib_per_second": (
        round(emitted_bytes / (1024 * 1024) / elapsed_seconds, 3)
        if completed.returncode == 0 and elapsed_seconds > 0
        else None
    ),
    "status": "ok" if completed.returncode == 0 else "failed",
    "workload": workload,
}

# Keep the result on stderr so stdout remains the terminal benchmark payload.
print("OXIDETERM_BENCHMARK_RESULT " + json.dumps(result, sort_keys=True), file=sys.stderr)
raise SystemExit(completed.returncode)
PYTHON
