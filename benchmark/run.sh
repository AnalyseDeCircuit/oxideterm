#!/bin/sh

# Emit prepared fixtures without adding timing work to the terminal data path.
set -eu

benchmark_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fixture_size_mib=${OXIDETERM_BENCHMARK_SIZE_MIB:-16}
fixture_data_root=${OXIDETERM_BENCHMARK_DATA_DIR:-"$benchmark_root/.data"}
fixture_directory="$fixture_data_root/$fixture_size_mib-mib"
workload=${1:-all}

usage() {
    printf '%s\n' 'Usage: benchmark/run.sh [plain|ansi|unicode|long-csi|all]' >&2
}

emit_fixture() {
    fixture_path="$fixture_directory/$1.txt"
    if [ ! -f "$fixture_path" ]; then
        printf 'Missing fixture: %s\nRun benchmark/prepare.sh first.\n' "$fixture_path" >&2
        exit 2
    fi
    cat "$fixture_path"
}

case "$workload" in
    plain|ansi|unicode|long-csi)
        emit_fixture "$workload"
        ;;
    all)
        emit_fixture plain
        emit_fixture ansi
        emit_fixture unicode
        emit_fixture long-csi
        ;;
    -h|--help)
        usage
        ;;
    *)
        usage
        exit 2
        ;;
esac
