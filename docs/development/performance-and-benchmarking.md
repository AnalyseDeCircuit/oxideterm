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

### Command-Aware Output At The Scrollback Limit

The headless GPUI renderer benchmark feeds real shell-integration events as well as text:

```sh
cargo bench -p oxideterm-gpui-terminal --features bench --bench terminal_render -- semantic_history --noplot --sample-size 10 --warm-up-time 1 --measurement-time 3
```

It compares small history, 2,000 previous commands at the scrollback limit, and the same command history without eviction. Warm redraw and coloring-disabled cases separate classification from rendering. The viewport is 120 × 40, with eight output lines per frame. History facts remain available after visual marks have left the grid; a full scrollback must not retain marks at reused line coordinates.

On macOS ARM64 (Apple M5), the September 2026 regression investigation measured approximately 4.5–4.8 ms/frame with stale command coordinates, versus 0.58–0.66 ms/frame after rebasing. The baseline included the new structured-output classifiers on top of `315785e0f`; this is a comparison of the coordinate fix, not a comparison against a released version. Command-classified rows fell from 21 to one per frame, eliminating the 20 false classifications. This measures the synthetic renderer workload, not end-to-end PTY throughput or Windows/Linux presentation.

For changes to grid scrolling, separately compare the same parser/grid fixtures:

```sh
cargo bench -p oxideterm-terminal --bench terminal_pipeline -- 'terminal_input_breakdown/grid/' --noplot --sample-size 20 --warm-up-time 1 --measurement-time 3 --save-baseline before-scroll-counter
# Run on the changed source with the same settings:
cargo bench -p oxideterm-terminal --bench terminal_pipeline -- 'terminal_input_breakdown/grid/' --noplot --sample-size 20 --warm-up-time 1 --measurement-time 3 --baseline before-scroll-counter
```

Criterion keeps the samples and estimates under `target/criterion/`. Do not run compilation or other CPU-heavy checks during measurement.

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
