#!/bin/sh

# Verify fixture presence and byte size without rendering the payload.
set -eu

benchmark_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fixture_size_mib=${OXIDETERM_BENCHMARK_SIZE_MIB:-16}
fixture_data_root=${OXIDETERM_BENCHMARK_DATA_DIR:-"$benchmark_root/.data"}
fixture_directory="$fixture_data_root/$fixture_size_mib-mib"

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

expected_bytes=$((fixture_size_mib * 1024 * 1024))

for fixture_name in plain ansi unicode long-csi; do
    fixture_path="$fixture_directory/$fixture_name.txt"
    if [ ! -f "$fixture_path" ]; then
        printf 'Missing fixture: %s\n' "$fixture_path" >&2
        exit 1
    fi
    actual_bytes=$(wc -c < "$fixture_path" | tr -d ' ')
    if [ "$actual_bytes" -ne "$expected_bytes" ]; then
        printf 'Unexpected fixture size for %s: got %s, expected %s bytes.\n' \
            "$fixture_name" "$actual_bytes" "$expected_bytes" >&2
        exit 1
    fi
done

printf 'Verified %s MiB fixtures in %s\n' "$fixture_size_mib" "$fixture_directory"
