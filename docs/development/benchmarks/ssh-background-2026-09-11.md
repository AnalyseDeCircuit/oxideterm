# SSH background parsing benchmark — 2026-09-11

The background worker completes these loopback SSH workloads faster while removing parsing from UI callbacks. This measures transport, parsing and scheduling together; it does not measure a faster VT algorithm or physical display presentation.

Environment: Apple M5, 10 logical CPUs, 32 GiB RAM, macOS 26.6.1 (25G76), Rust 1.94.1, release profile. Base commit: `070360ec3`, with the SSH background-parser implementation in the worktree. Both modes use the same core and protocol pipeline.

Each round includes one warm-up and three measured runs for each approximately 16 MiB workload. All tables show medians of measured runs; warm-ups are retained in the [raw JSON](ssh-background-2026-09-11.json). See [the implementation and reproduction commands](../ssh-terminal-parsing.md#performance-comparison) for budgets, snapshot timing and fixture configuration.

## Parser completion

Round 1 showed substantial variation in synchronous long-CSI output (562–822 ms), so the complete comparison was repeated. Both rounds are retained below. No samples are removed. Between rounds only a closed-writer error branch gained buffer zeroization; the measured successful stream path was unchanged.

| Workload | Round 1 sync ms | Round 1 worker ms | Round 2 sync ms | Round 2 worker ms | Round 2 ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| plain | 219.918 | 90.964 | 198.419 | 87.605 | 2.26× |
| ansi | 329.772 | 131.439 | 255.602 | 132.401 | 1.93× |
| unicode | 416.881 | 155.371 | 312.926 | 154.505 | 2.03× |
| long-csi | 565.510 | 138.337 | 286.872 | 146.334 | 1.96× |

## Completion stages (round 2)

Producer completion means submission to the server API, not remote consumption. Final snapshot readiness includes the snapshot request cadence; it is not GPU presentation.

| Workload | Producer sync / worker ms | Parser sync / worker ms | Final snapshot sync / worker ms |
| --- | ---: | ---: | ---: |
| plain | 115.439 / 60.776 | 198.419 / 87.605 | 198.604 / 93.611 |
| ansi | 157.192 / 81.832 | 255.602 / 132.401 | 256.703 / 135.684 |
| unicode | 213.669 / 92.354 | 312.926 / 154.505 | 318.718 / 160.331 |
| long-csi | 171.338 / 89.561 | 286.872 / 146.334 | 289.274 / 151.513 |

## Callback cost and process resources (round 2)

Callback P95 includes drains, input probes and any snapshot taken in that callback; most worker callbacks only drain a report. Raw timings are rounded to three decimal places in milliseconds; 0.000 does not mean zero work. CPU includes the peer; peak RSS includes fixture buffers and is a process high-water mark, so it is not an independent per-run memory measurement.

| Workload | Callback P95 sync / worker ms | Snapshot P95 sync / worker ms | CPU sync / worker s | Peak RSS sync / worker MiB |
| --- | ---: | ---: | ---: | ---: |
| plain | 2.073 / 0.000 | 0.182 / 0.140 | 0.167 / 0.144 | 51.05 / 51.17 |
| ansi | 2.116 / 0.001 | 0.085 / 0.099 | 0.210 / 0.190 | 51.39 / 51.36 |
| unicode | 2.137 / 0.000 | 0.105 / 0.117 | 0.248 / 0.211 | 51.61 / 51.61 |
| long-csi | 2.164 / 0.000 | 0.128 / 0.137 | 0.231 / 0.201 | 51.64 / 51.72 |

## Input probes and limits

Probes are sent during output, at least 5 ms apart, with only one awaiting receipt at a time. Receipt is timestamped by the SSH server. These short runs yield few samples and variable tail latency; the data do not establish an input-latency improvement.

| Workload | Round 2 median input P95 sync / worker ms | Per-run sample count sync | Per-run sample count worker |
| --- | ---: | --- | --- |
| plain | 41.984 / 54.392 | 11 / 15 / 13 | 6 / 9 / 6 |
| ansi | 116.677 / 56.038 | 20 / 14 / 14 | 9 / 10 / 6 |
| unicode | 76.022 / 35.821 | 24 / 17 / 20 | 12 / 10 / 8 |
| long-csi | 43.083 / 46.697 | 20 / 23 / 16 | 9 / 9 / 10 |

Both modes passed the benchmark completion checks. Separate real-SSH GPUI tests cover headless layout/painting and tmux click ordering. Windows hardware, WAN throughput, simultaneous multi-terminal load and physical display latency were not measured here.
