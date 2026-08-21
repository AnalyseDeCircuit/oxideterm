#!/bin/sh

# Run the complete terminal throughput suite and produce a consolidated result.
set -eu

benchmark_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fixture_size_mib=${OXIDETERM_BENCHMARK_SIZE_MIB:-16}
warmup_runs=${OXIDETERM_BENCHMARK_WARMUPS:-1}
measured_runs=${OXIDETERM_BENCHMARK_RUNS:-3}
results_root=${OXIDETERM_BENCHMARK_RESULTS_DIR:-"$benchmark_root/results"}
workloads='plain ansi unicode long-csi'

require_nonnegative_integer() {
    variable_name=$1
    variable_value=$2
    case "$variable_value" in
        *[!0-9]*|'')
            printf '%s must be a nonnegative integer.\n' "$variable_name" >&2
            exit 2
            ;;
    esac
}

require_nonnegative_integer OXIDETERM_BENCHMARK_SIZE_MIB "$fixture_size_mib"
require_nonnegative_integer OXIDETERM_BENCHMARK_WARMUPS "$warmup_runs"
require_nonnegative_integer OXIDETERM_BENCHMARK_RUNS "$measured_runs"

if [ "$fixture_size_mib" -eq 0 ] || [ "$measured_runs" -eq 0 ]; then
    printf '%s\n' 'Fixture size and measured run count must be greater than zero.' >&2
    exit 2
fi

if ! command -v python3 >/dev/null 2>&1; then
    printf '%s\n' 'benchmark/benchmark.sh requires python3 for timing and aggregation.' >&2
    exit 2
fi

if ! "$benchmark_root/verify.sh" >/dev/null 2>&1; then
    printf 'Preparing %s MiB benchmark fixtures...\n' "$fixture_size_mib" >&2
    "$benchmark_root/prepare.sh" >/dev/null
fi

benchmark_started_at_utc=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
benchmark_run_timestamp=$(printf '%s' "$benchmark_started_at_utc" | tr -d ':-')
benchmark_run_id="${benchmark_run_timestamp}-$$"
result_directory="$results_root/$benchmark_run_id"
raw_result_path="$result_directory/runs.jsonl"
json_summary_path="$result_directory/summary.json"
markdown_summary_path="$result_directory/summary.md"
# Record the running terminal build because it may differ from the repository checkout.
oxideterm_version=${TERM_PROGRAM_VERSION:-unknown}
mkdir -p "$result_directory"
: > "$raw_result_path"

warmup_iteration=1
while [ "$warmup_iteration" -le "$warmup_runs" ]; do
    for workload in $workloads; do
        printf 'Warm-up %s/%s: %s\n' "$warmup_iteration" "$warmup_runs" "$workload" >&2
        "$benchmark_root/measure.sh" "$workload" 2>/dev/null
    done
    warmup_iteration=$((warmup_iteration + 1))
done

# Preserve stdout for the terminal payload while capturing each JSON result.
exec 3>&1
measured_iteration=1
while [ "$measured_iteration" -le "$measured_runs" ]; do
    for workload in $workloads; do
        printf 'Measured run %s/%s: %s\n' "$measured_iteration" "$measured_runs" "$workload" >&2
        if result_line=$("$benchmark_root/measure.sh" "$workload" 2>&1 1>&3); then
            result_prefix='OXIDETERM_BENCHMARK_RESULT '
            case "$result_line" in
                "$result_prefix"*)
                    printf '%s\n' "${result_line#"$result_prefix"}" >> "$raw_result_path"
                    ;;
                *)
                    printf 'Unexpected benchmark result: %s\n' "$result_line" >&2
                    exit 1
                    ;;
            esac
        else
            printf 'Benchmark workload failed: %s\n%s\n' "$workload" "$result_line" >&2
            exit 1
        fi
    done
    measured_iteration=$((measured_iteration + 1))
done
exec 3>&-

python3 - \
    "$raw_result_path" \
    "$json_summary_path" \
    "$markdown_summary_path" \
    "$benchmark_run_id" \
    "$benchmark_started_at_utc" \
    "$oxideterm_version" \
    "$fixture_size_mib" \
    "$warmup_runs" \
    "$measured_runs" <<'PYTHON'
import json
import statistics
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path

(
    raw_result_path,
    json_summary_path,
    markdown_summary_path,
    benchmark_run_id,
    started_at_utc,
    oxideterm_version,
    fixture_size_mib,
    warmup_runs,
    measured_runs,
) = sys.argv[1:]
oxideterm_version = " ".join(oxideterm_version.split()) or "unknown"
started_at = datetime.strptime(started_at_utc, "%Y-%m-%dT%H:%M:%SZ")
started_at_display = started_at.strftime("%Y-%m-%d %H:%M:%S UTC")
samples_by_workload = defaultdict(list)
for line in Path(raw_result_path).read_text(encoding="utf-8").splitlines():
    sample = json.loads(line)
    samples_by_workload[sample["workload"]].append(sample)

summary_results = {}
for workload in ("plain", "ansi", "unicode", "long-csi"):
    samples = samples_by_workload[workload]
    elapsed_values = [sample["elapsed_ms"] for sample in samples]
    throughput_values = [sample["pty_mib_per_second"] for sample in samples]
    summary_results[workload] = {
        "elapsed_ms_median": round(statistics.median(elapsed_values), 3),
        "pty_mib_per_second_median": round(statistics.median(throughput_values), 3),
        "samples": len(samples),
    }

summary = {
    "fixture_size_mib": int(fixture_size_mib),
    "measured_runs": int(measured_runs),
    "oxideterm_version": oxideterm_version,
    "results": summary_results,
    "run_id": benchmark_run_id,
    "schema_version": 2,
    "started_at_utc": started_at_utc,
    "warmup_runs": int(warmup_runs),
}
Path(json_summary_path).write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)

markdown_lines = [
    "# OxideTerm terminal benchmark summary",
    "",
    f"- **OxideTerm version:** `{oxideterm_version}`",
    f"- **Run time:** {started_at_display}",
    f"- **Run ID:** `{benchmark_run_id}`",
    f"- **Fixture:** {fixture_size_mib} MiB",
    f"- **Warm-up runs:** {warmup_runs}",
    f"- **Measured runs:** {measured_runs}",
    "",
    "| Workload | Median time (ms) | Median throughput (MiB/s) |",
    "|---|---:|---:|",
]
for workload, result in summary_results.items():
    markdown_lines.append(
        f"| {workload} | {result['elapsed_ms_median']:.3f} | "
        f"{result['pty_mib_per_second_median']:.3f} |"
    )
markdown_lines.extend(
    [
        "",
        "> This benchmark measures process-to-PTY throughput, not completed rendering or input latency.",
        "",
        "## Result files",
        "",
        "- [Raw samples](./runs.jsonl)",
        "- [Machine-readable summary](./summary.json)",
        "",
    ]
)
Path(markdown_summary_path).write_text("\n".join(markdown_lines), encoding="utf-8")

print("\nOxideTerm terminal benchmark summary")
print(f"Version: {oxideterm_version} | Run time: {started_at_display}")
print(f"Fixture: {fixture_size_mib} MiB | warm-ups: {warmup_runs} | measured runs: {measured_runs}")
print(f"{'workload':<12} {'median ms':>12} {'median MiB/s':>16}")
for workload, result in summary_results.items():
    print(
        f"{workload:<12} "
        f"{result['elapsed_ms_median']:>12.3f} "
        f"{result['pty_mib_per_second_median']:>16.3f}"
    )
print(f"\nRaw results: {raw_result_path}")
print(f"Markdown:    {markdown_summary_path}")
print(f"JSON:        {json_summary_path}")
PYTHON
