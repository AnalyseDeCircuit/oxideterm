# Terminal benchmark workloads

This directory contains reproducible terminal-output workloads for manual and external-harness testing. It deliberately separates fixture generation from payload emission, so fixture construction does not affect the measured terminal path.

The scripts are original OxideTerm scripts. Their workload categories are informed by the public terminal benchmark ecosystem, but no external project source is copied here.

## Run the complete benchmark

Open an OxideTerm terminal pane in the repository and run one command:

```sh
./benchmark/benchmark.sh
```

It automatically prepares missing fixtures, performs one warm-up and three measured runs for every workload, prints a median summary, and saves raw JSONL samples, a machine-readable `summary.json`, and a human-readable `summary.md` under `benchmark/results/`. Both summaries record the running OxideTerm version and the UTC run time.

Example summary:

```text
OxideTerm terminal benchmark summary
Version: 2.0.23 | Run time: 2026-08-21 13:10:42 UTC
Fixture: 16 MiB | warm-ups: 1 | measured runs: 3
workload        median ms     median MiB/s
plain             123.456          129.600
ansi              234.567           68.210
unicode           210.000           76.190
long-csi          250.000           64.000

Raw results: benchmark/results/<run-id>/runs.jsonl
Markdown:    benchmark/results/<run-id>/summary.md
JSON:        benchmark/results/<run-id>/summary.json
```

The Markdown report includes the version, readable UTC time, run configuration, median result table, measurement scope, and links to the raw and JSON files. `summary.json` stores the same run metadata for automated comparisons.

Change fixture size or repetition counts through environment variables:

```sh
OXIDETERM_BENCHMARK_SIZE_MIB=64 \
OXIDETERM_BENCHMARK_WARMUPS=3 \
OXIDETERM_BENCHMARK_RUNS=5 \
./benchmark/benchmark.sh
```

The summary measures process-to-PTY throughput, not completed rendering or input latency. Record the in-app performance overlay separately for drain, snapshot, search, image-preparation, and input-latency data.

## Internal scripts

`benchmark.sh` is the user-facing entry point. `prepare.sh`, `verify.sh`, `run.sh`, and `measure.sh` are its internal fixture, validation, emission, and single-sample helpers. `plain` measures a text flood, `ansi` measures frequent SGR style changes, `unicode` includes wide and combining-prone text, and `long-csi` stresses longer control sequences.

## External comparison with vtebench

[vtebench](https://github.com/alacritty/vtebench) is the recommended cross-terminal PTY-throughput harness. Its built-in executable workloads cover dense and light cells, Unicode, cursor motion, synchronized output, and multiple scrolling regions.

Run it inside a focused OxideTerm terminal pane, then repeat the same command in each comparison terminal with matching window size, font, scrollback limit, theme, power state, and no competing workload:

```sh
git clone https://github.com/alacritty/vtebench.git
cd vtebench
cargo run --release -- --dat oxideterm.dat
```

Do three warm-up runs and retain the median of five measured runs. Keep vtebench results separate from the in-app input-latency results: vtebench measures PTY read throughput, not general terminal responsiveness.

## Fixture storage

Fixtures are written to `benchmark/.data/` by default and ignored by Git. Set `OXIDETERM_BENCHMARK_DATA_DIR` to place them elsewhere, for example on a RAM disk or a shared benchmarking volume.
