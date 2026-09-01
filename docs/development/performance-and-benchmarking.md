# Performance And Benchmarking

Performance work starts with a measured regression and ends with the same workload measured again. A smoother visual impression, one fast run, or an unrelated benchmark is not enough to claim an improvement.

## Measure The Correct Layer

| Symptom | First measurement | Keep separate from |
| --- | --- | --- |
| Slow terminal output parsing | Terminal fixture throughput | GPU presentation and UI input latency |
| Slow scrolling or selection | Viewport/layout and render frame behavior | Parser throughput unless parsing is active |
| Large image or animation cost | Image decode/cache/paint behavior | Ordinary text benchmark results |
| Slow connection or SFTP response | Network/runtime phase timing | Terminal renderer measurements |
| Window hitch, text, or cursor issue | Native platform and renderer evidence | Generic application CPU averages |

## Terminal Benchmark Workloads

[`benchmark/`](../../benchmark/README.md) contains reproducible terminal-output workloads for plain text, ANSI style changes, Unicode, and long control sequences. Run the complete workload in an OxideTerm terminal pane:

```sh
./benchmark/benchmark.sh
```

The script prepares fixtures when needed, performs a warm-up plus measured runs, and writes JSONL, JSON, and Markdown summaries under `benchmark/results/`. Its process-to-PTY throughput result does not measure completed rendering, input latency, or remote-network performance.

Use the same fixture size, warm-up count, measured-run count, terminal dimensions, renderer profile, font, theme, scrollback setting, power mode, and machine for a before/after comparison. Record the baseline commit and the measured commit with the result.

## In-App Performance Work

For a UI or renderer change, capture the smallest reproducible interaction and identify whether cost is in:

1. input or event delivery;
2. application state updates and invalidation;
3. layout or text shaping;
4. scene construction and paint;
5. GPU submission or presentation.

Do not respond to a rendering hitch by rewriting the terminal parser without evidence that parsing is the bottleneck. Likewise, do not claim parser improvement from a benchmark dominated by terminal paint or shell startup.

## Benchmark Discipline

- Warm up before measuring and use a median from repeated runs.
- Change one relevant variable at a time.
- Keep raw samples with the summary when a result informs a merge or release claim.
- Describe regressions by workload and layer, not by an unqualified percentage.
- Repeat any materially surprising result before deciding on an architectural change.

Large one-time operations, such as expanding a command block or rebuilding an image cache, may have a different budget from steady-state terminal output. Document that distinction rather than hiding it inside an averaged number.

## Review Checklist

Before merging a performance-sensitive change, state the original bottleneck, baseline command or interaction, measured result, affected platform, and remaining manual validation. If there is no measured regression or expected hot-path impact, keep the change focused on correctness and do not market it as an optimization.
