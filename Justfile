python := if os() == "windows" { "py -3" } else { "python3" }

# List the available repository tasks.
default:
    @just --list

# Check Rust formatting without rewriting files.
fmt:
    cargo fmt --all -- --check

# Run OxideTerm with optional Cargo arguments.
run *args:
    cargo run {{ args }}

# Generate the aggregated third-party license notices.
notices:
    {{ python }} scripts/release/generate_third_party_notices.py

# Build and stage the CLI companion for an optional target triple.
build-cli target="":
    bash scripts/build/build-cli.sh {{ target }}

# Build and stage the bundled Linux remote agents.
build-agent:
    bash scripts/build/build-agent.sh

# Run the terminal throughput benchmark in the active OxideTerm terminal.
benchmark:
    sh benchmark/benchmark.sh

# Update the workspace version, with optional bump script flags.
bump-version version *options:
    {{ python }} scripts/release/bump_version.py {{ options }} {{ version }}

# Build native release packages for an optional target triple.
package target="":
    {{ python }} scripts/release/package_native.py {{ target }}
