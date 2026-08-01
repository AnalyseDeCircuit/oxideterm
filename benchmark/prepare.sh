#!/bin/sh

# Generate deterministic terminal-output fixtures outside the measured path.
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

fixture_bytes=$((fixture_size_mib * 1024 * 1024))
mkdir -p "$fixture_directory"

write_fixture() {
    fixture_name=$1
    fixture_line=$2
    fixture_path="$fixture_directory/$fixture_name.txt"

    # head truncates the repeating source to an exact, comparable byte length.
    yes "$fixture_line" | head -c "$fixture_bytes" > "$fixture_path"
    printf '%s\n' "$fixture_path"
}

escape_character=$(printf '\033')
write_fixture plain 'OxideTerm benchmark plain text 0123456789 abcdefghijklmnopqrstuvwxyz'
write_fixture ansi "${escape_character}[1;38;2;72;183;255;48;2;18;24;38mOxideTerm ANSI style workload${escape_character}[0m"
write_fixture unicode 'OxideTerm Unicode workload: 中文 日本語 한국어 Δοκιμή 🚀 café naïve'
write_fixture long-csi "${escape_character}[1;2;3;4;5;7;8;9;22;23;24;25;27;28;29;38;2;72;183;255;48;2;18;24;38mOxideTerm long CSI workload${escape_character}[0m"
