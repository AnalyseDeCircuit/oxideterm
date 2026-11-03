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

### Incremental Terminal Drawing (2026-11-03)

Four shared Rust improvements are retained on top of the decoration/color
changes below, after withdrawal of the architecture-specific ASCII experiment:

- Changed rows reuse an unchanged run's shaping and glyph cache only when its
  column, cell width, text and complete style match. Font size, cell width and line
  height changes invalidate reuse. A bounded row-ID-to-key index references the
  existing row cache; it does not separately retain old text or glyph resources.
- Glyphs with zero vertical shaping offsets share the renderer's existing integer
  raster baseline (`SUBPIXEL_VARIANTS_Y == 1`). Anchoring their cached batch at
  that baseline removes unnecessary rebuilds during fractional vertical scrolling
  and row movement, without changing rasterized glyph coordinates. Other layouts
  retain exact device-phase invalidation. Horizontal positioning is unchanged.
- Semantic roles are cached by absolute logical-line range and active-input state.
  A weak reference versions the immutable command-mark snapshot without extending
  command-text lifetimes. Mark changes invalidate the classifications.
- Blank cells no longer compute unused foreground colors, and theme-default
  backgrounds reuse the already converted color. Selection, explicit backgrounds,
  wide-cell spacers, combining marks and cursors keep their existing handling.

Environment: Windows x64, i9-13900HX, Rust 1.97.0, optimized bench profile, native
DirectWrite fonts in headless windows. No new OS/CPU-specific production branches
or dependencies were introduced. Initial paired measurements used 20 samples,
1 s warmup and 3 s measurement. The table compares printed elapsed-time point
estimates; Criterion's separate change test uses its sample statistics.

| Workload | Before (µs) | After (µs) | Point-estimate change |
| --- | ---: | ---: | ---: |
| Long-row suffix editing | 137.12 | 105.85 | -22.8% |
| Selection dragged across populated rows | 665.29 | 635.86 | -4.4% |
| Fractional-pixel history scrolling | 312.32 | 245.58 | -21.4% |
| Sparse viewport rewritten every frame | 1,483.90 | 1,318.00 | -11.2% |
| 2,000-command history, unchanged redraw | 366.58 | 341.92 | -6.7%, not statistically significant |
| 2,000-command history, streaming | 1,439.70 | 1,092.40 | -24.1% |
| Plain streaming text | 1,110.70 | 758.04 | -31.8% |
| Mixed Unicode streaming | 1,139.10 | 814.33 | -28.5% |
| Unchanged ordinary text | 206.39 | 205.78 | -0.3%, not statistically significant |

Input scene-construction P50 fell from 38 to 12 µs, scrolling from 105 to 60 µs,
and sparse-row layout from 422 to 315 µs. Unchanged semantic-role lookup P50 fell
from 14 to 4 µs; invalidated roles in continuously streaming output instead cost
9 to 10 µs. This small miss overhead is a tradeoff, not a claim that every semantic
operation becomes faster. The semantic history fixtures include occasional costly
highlight rebuilds; the unchanged whole-frame comparison was not significant.
Selection draw P95/P99 was 0.806/1.009 ms before and 0.818/1.027 ms after, so the
mean reduction does not establish improved selection tail latency. Scrolling draw
P95/P99 changed from 0.602/0.849 to 0.569/0.734 ms.

A separate sequential run of the saved baseline and candidate executables used
30 samples and 4 s measurement for both history workloads and the plain/Unicode
controls. Plain output measured 1,108.20 to 773.17 µs (-30.2% by printed estimate),
and Unicode 1,132.90 to 824.00 µs (-27.3%). Streaming command history measured
1,495.20 to 1,123.70 µs (-24.8%). All three passed Criterion's change test.
Unchanged command history measured 363.99 to 354.76 µs, still not significant;
its role-lookup P50 again changed from 14 to 4 µs. The role cache is retained for
this reproducible stage reduction, not for an established whole-frame speedup.
No compiler, formatter or test run overlapped either measurement sequence.

The new fixtures use more distinct text revisions than the row-cache capacity,
actual selection updates and precise-pixel scroll events, and settled viewport
dimensions. They use synthetic text only. Run the paired workload with:

```sh
cargo bench --locked -p oxideterm-gpui-terminal --features bench --bench terminal_render -- 'terminal_incremental_|terminal_internal_(plain|unicode|warm)$' --noplot --sample-size 20 --warm-up-time 1 --measurement-time 3 --save-baseline before-incremental-render
# After changing the implementation:
cargo bench --locked -p oxideterm-gpui-terminal --features bench --bench terminal_render -- 'terminal_incremental_|terminal_internal_(plain|unicode|warm)$' --noplot --sample-size 20 --warm-up-time 1 --measurement-time 3 --baseline before-incremental-render
```

Validation: 494 GPUI and 221 terminal-view unit tests passed; four existing ignored
tests remain unrun. The explicitly run Windows native-font integration test also
passed. It now compares cached/reference draw commands at six vertical offsets
and five scales, including an offset glyph that must use the fallback path, and
checks atlas lookup reuse. New terminal regressions cover changed text/style,
selection refresh, all three layout metrics, resource release, command-mark
replacement, viewport movement and active-input changes. Formatting and diff
whitespace checks passed. These measurements cover CPU layout and scene building,
not GPU presentation, input-to-display latency, or macOS/Linux runtime performance.
The existing tracked allocator also adds instrumentation cost.

Raw source snapshots, paired executables, logs, `comparison.json` and
`confirm-comparison.json` are under
`target/incremental-render-evaluation/`. No application window was launched for
these checks.

### Cached Decorations And Prepared Colors (2026-11-03)

Both changes are retained in the shared Rust renderer; there are no new CPU- or
OS-specific production branches. A nonempty, single full-line decoration run now
uses the existing glyph batch cache and paints its underline/strikethrough as live
primitives. The original nested layer, clipping, pixel snapping, zero-advance
fallback and style/opacity behavior are preserved. Partial/multiple decoration
runs keep the general painter. No separate decoration geometry cache was added.

Prepared monochrome/subpixel glyphs now hold `SceneHsla`. Hue normalization happens
once when a decoration color is selected during batch preparation, while replay
still applies the current clamped opacity. Rebuilding a device-phase-dependent
batch reuses its boxed cache entry and reserves glyph capacity by channel before
filling the vectors. These preparation changes removed the initial candidate's
roughly 3% regression in continuously changing styled output.

The existing native-font terminal fixtures were rebuilt for the baseline after
withdrawal of the x64 ASCII scanner. Baseline and final binaries used the same
Windows x64 / i9-13900HX / Rust 1.97.0 / optimized profile and warmed fixtures.
The confirmation used 30 samples, 1 s warmup and 4 s measurement per case. Printed
elapsed-time estimates (not GPU presentation or input-to-display latency):

| Workload | Before (µs) | Final (µs) | Estimate change |
| --- | ---: | ---: | ---: |
| Plain streaming text | 1,297.10 | 1,144.80 | -11.7% |
| Styled streaming text | 1,071.60 | 1,028.70 | -4.0% |
| Mixed Unicode streaming | 1,221.00 | 1,195.70 | -2.1%, not statistically significant |
| Unchanged text | 207.68 | 200.60 | -3.4% |
| Unchanged underline + strikethrough | 639.85 | 320.75 | -49.9% |
| Identical unchanged text without decorations | 220.00 | 215.64 | -2.0% |

For decorated static text, all-observed draw P95/P99 changed from
0.769/1.289 ms to 0.393/0.500 ms. Mixed Unicode had a noisier full-suite P99
(1.626 to 2.439 ms), so it was checked again with just that case and no concurrent
build or formatter. The isolated repeat measured 1.260 to 1.139 ms, with draw
P95/P99 changing from 1.501/1.925 ms to 1.319/1.625 ms. The earlier tail increase
did not reproduce; no universal Unicode tail-latency improvement is claimed.
Stage histograms include calibration/warmup. The terminal benchmark's existing
tracked allocator adds instrumentation cost, so allocation-related gains are not
guaranteed production percentages.

The independent batch profiler isolates the prepared-color change from decoration
painting and cache preparation. For 4,096 glyphs, monochrome expansion changed
from 21.892 to 16.900 µs (-22.8%), and subpixel expansion from 21.455 to 17.778 µs
(-17.1%). With half the glyphs clipped, reductions were 35.2% and 33.8% respectively.
Polychrome controls, which do not normalize hue, stayed within approximately 1%.
The later preparation refinements do not change this measured expansion loop.

Validation on the final implementation:

- GPUI: 494 tests passed; 3 manual profiling tests are ignored in the normal suite.
- Terminal UI: 219 tests passed; the existing ConPTY test remained ignored because
  its adjacent runtime bundle was unavailable.
- The Windows native-font integration test was explicitly run and passed. It
  records real draw commands with the existing headless atlas, compares cached
  and reference glyph/stroke geometry, colors, atlas tiles and order, and checks
  that opacity/stroke changes do not repeat atlas lookups. It covers regular and
  bold italic text, CJK, combining marks, emoji, split foreground colors with
  wrapped hue, clipping, and scales 1, 1.25, 1.5, 2 and 3. The recorder does not
  claim a GPU screenshot comparison.
- Formatting and diff whitespace checks passed. macOS/Linux runtime performance
  and native GPU presentation were not measured on this Windows host.

The native check is opt-in because it needs DirectWrite and installed fonts:

```sh
cargo test --locked --release -p gpui-pre --no-default-features --features bench,test-support --test cached_line_paint -- --ignored --test-threads=1
```

Raw comparisons and binaries are under `target/decorated-cache-evaluation/`.
`terminal-final-comparison.json` records the final full-suite comparison;
`glyph-comparison.json` records all 12 isolated expansion cases. Logs named
`unicode-*-overlapping-fmt.log` are excluded because a formatter overlapped those
measurements; `unicode-before.log` and `unicode-after.log` are the isolated repeat.

### Decoration Cache And Glyph Expansion Evaluation (2026-11-03)

This was the pre-implementation evaluation; the retained change above supersedes
its proposal. At this stage only measurement infrastructure changed. The independent profiler calls the production
`Scene::insert_prepared_glyph_batch` directly from a private, manually selected
test module. No production profiling API or architecture-specific implementation
was added. Run it explicitly:

```sh
cargo test --locked --release -p gpui-pre --no-default-features --features bench,test-support --lib scene::benchmarks::prepared_glyph_batch_expansion -- --ignored --nocapture --test-threads=1
```

Each case uses an existing layer, warmed vector capacity, a translated origin,
and opacity 0.65. Inputs cover monochrome, subpixel and polychrome batches, with
64 or 4,096 glyphs and either all visible or every second glyph outside the mask.
Fixture checks compare the emitted tile identities and order with explicit input
indices. Timed work includes output-vector clearing, replay-reference release,
batch reference cloning and the real expansion function. It excludes font work,
atlas lookup/rasterization, allocation growth, scene sorting and GPU submission.
The 4,096-glyph case is one synthetic large batch, not an entire terminal frame.

Windows x64 results on the same i9-13900HX, Rust 1.97.0 and optimized release
profile (20 Criterion samples, 0.5 s warmup, 1 s measurement per case):

| Batch type | Glyphs | All visible (µs) | Half clipped (µs) |
| --- | ---: | ---: | ---: |
| Monochrome | 64 | 0.329 | 0.292 |
| Monochrome | 4,096 | 22.102 | 17.902 |
| Subpixel | 64 | 0.333 | 0.289 |
| Subpixel | 4,096 | 21.596 | 18.075 |
| Polychrome | 64 | 0.226 | 0.203 |
| Polychrome | 4,096 | 15.364 | 12.611 |

The optimized test binary's actual expansion function was inspected in assembly
and LLVM IR. Coordinate and clipping components already use vector operations
(`fadd <2 x float>` in IR; `addps`, `subps` and packed comparisons in assembly).
The outer glyph loops still advance one element at a time: 64-byte strides for
monochrome/subpixel and 48-byte strides for polychrome. This is component-level
vectorization, not widening across multiple glyphs. LLVM documents the distinction
between [SLP and loop vectorization](https://llvm.org/docs/Vectorizers.html).
This inspection establishes Windows x64 code generation only, not ARM64 results.

Both colored monochrome/subpixel paths also perform scalar hue normalization
(`frem` / `fmodf`) per glyph, before rejecting clipped glyphs. Pre-normalizing
the cached color or deferring conversion until after clipping are possible
cross-platform follow-ups; neither was implemented or assigned a speedup here.
The batch-type timings alone cannot isolate the cost of hue normalization.

The higher-priority cache proposal is to support a single full-line decoration
run in `LineLayout::paint_cached*`: retain the existing prepared glyph batch and
paint underline/strikethrough separately, preserving current layer ordering,
snapping, clipping and opacity. General partial-style and wrapped paths should
continue through the existing painter. In particular, zero-advance glyphs use
a font-bounds fallback for line width, and decoration colors/thickness/waviness
must remain live when the glyph cache is reused. The initial implementation need
not introduce another geometry cache for only two line primitives.

Validation: all 12 profiling cases completed; 5 scene regression tests and 3
underline-handler geometry tests passed using the same optimized test binary.
The manual Criterion runner is not counted as an additional correctness test.
Raw timings, the compiler-emission wrapper and extracted function assembly/IR
are retained under `target/glyph-evaluation/`. Bulk compiler assembly/IR dumps
were removed after extraction. Formatting and diff whitespace checks passed.

### Native Terminal Drawing On Windows (2026-11-03)

The `terminal_internal_*` cases in
[`terminal_render.rs`](../../crates/oxideterm-gpui-terminal/benches/terminal_render.rs)
measure one terminal's row layout, native text shaping, glyph preparation and
scene construction. They cover plain output, ANSI styles, mixed CJK/combining/RTL
text, unchanged text, and identical unchanged text with or without underline and
strikethrough. The streaming cases feed eight rows per update; the static cases
invalidate the terminal without changing its contents. Window sizing settles
before measurement and is checked again afterward.

On Windows, the standard `#[gpui::bench]` macro acquires
`current_platform(true).text_system()`, which is `NoopTextSystem`. Those results
cannot establish native text rendering cost. The new cases explicitly acquire
DirectWrite from the native platform and pass it to the benchmark's test windows;
they do not open visible application windows. An empty native font list fails the
fixture instead of silently measuring the empty backend. Other platforms keep
their headless platform text system. Windows measurements include native shaping
and glyph rasterization, but exclude final Direct3D submission/presentation.
The test atlas also uses synthetic texture identities, so its reported batch
counts must not be interpreted as production GPU draw-call counts.

Workload-specific fonts and glyphs are warmed before the final fixtures begin
measurement. This matters for styled text: the initial fixture mixed first-use
italic/decorated text preparation into its samples and showed roughly 53 ms frame
P99. That observation alone does not establish a sustained rendering stall.
GPUI stage reports include Criterion calibration/warmup iterations; their P95/P99
are not interactive GUI input latency measurements.

```sh
cargo bench --locked -p oxideterm-gpui-terminal --features bench \
  --bench terminal_render -- terminal_internal_ --noplot \
  --sample-size 20 --warm-up-time 1 --measurement-time 3
```

An experiment deferred construction of each cell's temporary `TextRun` until a
new text run was needed, preserving the existing logical and bidirectional run
boundaries. On the same Windows x64 / i9-13900HX / balanced power configuration as
the parser benchmarks, an initial 20-sample comparison was followed by a
30-sample comparison using saved baseline/candidate executables, without a build
running concurrently. Both versions used identical native-font fixtures.

| Workload | Row layout P50 before / candidate | Criterion whole-frame change (95% interval) |
| --- | --- | --- |
| Plain text | 167 / 158 µs | +0.26% (-1.93% to +2.59%) |
| ANSI styles | 145 / 137 µs | -0.08% (-32.09% to +49.13%) |
| Mixed Unicode | 276 / 275 µs | +0.66% (-2.37% to +3.54%) |
| Unchanged text | Not targeted | +0.55% (-2.34% to +3.80%) |

**The experiment was reverted:** the localized reduction did not establish a
whole-frame improvement. These comparisons used the initial native-font fixture,
before workload-specific prewarming was added; do not mix their baselines with
the final warmed fixtures. Existing row layout caching, prepared glyph batches,
and adjacent background/selection merging remain unchanged. One concrete next
measurement target is decorated text: `LineLayout::paint_cached*` currently
falls back to per-glyph painting for underline/strikethrough. The paired static
fixtures isolate that path from parsing and changing row contents.

The final warmed fixtures were run twice. The first run's streaming cases were
noisy (plain text was 2.847 ms versus 1.220 ms on the unchanged-binary repeat), so
they are not evidence of a renderer regression or a prewarming speedup. The static
decoration comparison was repeatable:

| Identical static text | First / repeat whole-frame estimate | Repeat scene construction P50 | Repeat frame P95 / P99 |
| --- | --- | --- | --- |
| No decorations | 214 / 216 µs | 47 µs | 276 / 376 µs |
| Underline + strikethrough | 634 / 615 µs | 410 µs | 762 / 977 µs |

This establishes a materially more expensive decorated-text path, not the
attainable speedup of any proposed cache change: decorations also add primitives,
and Windows GPU submission is still excluded. The existing renderer was left
intact. Final validation passed 219 terminal UI tests and 4 terminal-model tests;
one ConPTY test was ignored because its adjacent runtime bundle was unavailable.
A pre-existing Windows test compilation failure was repaired by moving the
`TerminalEvent` test import out of its Unix-only import group. The six final
benchmarks, formatting check and diff whitespace check passed.

### OSC Processing And Shell-Scanner Breakdown (2026-11-03)

Baseline: `99102cf7aef9b756cb759a892fbb5dca12ef44c5` plus the preceding printable
ASCII scan candidate documented below. Measurements used the same Windows x64,
Core i9-13900HX, balanced power plan, Rust 1.97.0 and optimized bench profile.
The added workloads use synthetic public URLs and text, not live terminal contents.
The x64-only scan was subsequently withdrawn; these are historical paired results
with that same scan present in both versions, not a fresh measurement after its removal.

The breakdown separates graphics ingress alone, ANSI/grid processing with graphics
enabled or disabled, recording playback with graphics enabled or disabled, and the
production encoding detector/decoder and privilege-prompt scanner used by live
Shells. The standalone scanner benchmark includes the private production source;
no production API was added. Combined outer-scanner samples use complete UTF-8
codepoint boundaries, matching the residual guard's contract. Network transport,
PTY delivery, trigger rules, recording persistence and GUI rendering are excluded.

The graphics benchmark uses about 1 MiB, a 120×40 grid and 1,000 history rows;
recording playback uses about 256 KiB, 120×40 and 20,000 history rows. Outer scans
use about 1 MiB with approximately 8 KiB chunks. Long title bodies repeat
`terminal title ` 128 times; results should not be generalized to short titles.
Baseline and first candidate runs used 20 samples, one-second warm-up and two-second
measurement. Retained OSC improvements were confirmed with 30 samples and a
three-second measurement. Values below are median elapsed microseconds:

| Layer / workload | Input | Before (µs) | Retained (µs) | Time change |
| --- | --- | ---: | ---: | ---: |
| graphics_ingress/ordered/osc-title | 1 MiB | 13000.17 | 91.36 | -99.30% |
| graphics_ingress/ordered/osc-links | 1 MiB | 13718.34 | 3417.51 | -75.09% |
| terminal_output/graphics/osc-title | 1 MiB | 14011.38 | 595.44 | -95.75% |
| terminal_output/graphics/osc-links | 1 MiB | 23302.36 | 12441.45 | -46.61% |
| terminal_input_breakdown/pipeline/osc-title | 256 KiB | 3904.51 | 401.78 | -89.71% |
| terminal_input_breakdown/pipeline/osc-links | 256 KiB | 6289.33 | 3903.09 | -37.94% |

The retained change only batches opaque OSC payload bytes in the graphics ingress,
using the existing `memchr` dependency to stop at BEL or ESC. The terminator and
protocol dispatch paths remain unchanged, including iTerm2 image handling, forwarded
OSC terminator canonicalization, private-OSC filtering downstream, and the exact
encoded-size admission/discard boundary. Ordinary text, ANSI, Unicode and long-CSI
graphics controls did not show a comparable regression in the initial full sweep.
This is an input-processing improvement for OSC-heavy workloads, not a terminal-FPS
or SSH-throughput claim.

**Rejected experiment:** bulk skipping OSC/DCS/APC/PM bodies in the privilege
scanner improved OSC cases but regressed ordinary and ANSI log scans by about
20–34%. Outlining the search did not resolve that regression. Both candidate
implementations were removed; the production privilege scanner and encoding
detector remain unchanged. The baseline outer-scanner breakdown was:

| Workload | Encoding detector (µs/MiB) | Combined outer scanners (µs/MiB) |
| --- | ---: | ---: |
| Plain | 19.17 | 873.01 |
| ANSI | 18.45 | 978.50 |
| Unicode | 372.70 | 3414.43 |
| Colon-heavy log | 18.82 | 5498.73 |
| OSC title | 18.55 | 1581.61 |
| OSC links | 18.70 | 1768.29 |

Restoring the original scanner and forcing a fresh compilation produced: plain 872.11 µs (-0.10%); ansi 978.26 µs (-0.02%); unicode 3363.83 µs (-1.48%).

Validation: 36 graphics-protocol tests passed (one existing ignored manual benchmark),
203 final terminal-core tests passed (three existing ignored tests), and five encoding
tests passed. New coverage checks opaque OSC byte preservation at every split,
BEL/ST boundaries, the encoded-size limit and recovery after discard. Existing
terminal tests cover private-OSC recording filtering and protocol ordering. Workspace
format and whitespace checks passed. Graphics-library Clippy with `--no-deps` and
warnings denied passed; the broader invocation was blocked by four pre-existing
Alacritty lint errors (integer-boundary simplification, missing `is_empty`, MSRV and
let-and-return).
The final terminal suite was recompiled after restoring the prompt scanner.

Reproduction uses the repository's existing benchmark targets:

```sh
cargo bench --locked -p oxideterm-terminal-graphics --bench graphics_ingress -- '^(graphics_ingress|terminal_output)/' --noplot --sample-size 20 --warm-up-time 1 --measurement-time 2 --save-baseline before-osc-scans
cargo bench --locked -p oxideterm-terminal --bench terminal_pipeline -- 'terminal_input_breakdown/(grid|pipeline|pipeline-no-graphics)/(osc-title|osc-links)$' --noplot --sample-size 20 --warm-up-time 1 --measurement-time 2 --save-baseline before-osc-scans
cargo bench --locked -p oxideterm-terminal --bench privilege_prompt_stream -- '^(privilege_prompt_stream|shell_outer_scanners)/' --noplot --sample-size 20 --warm-up-time 1 --measurement-time 2 --save-baseline before-osc-scans
```

After the change, replace `--save-baseline` with `--baseline`. Raw logs, rejected
candidate source snapshots and comparison JSON are under `target/osc-evaluation/`.
Criterion retains the samples under `target/criterion/`. Files prefixed `stale-`
are excluded: a Windows source-copy operation preserved the old modification time
and Cargo initially reused a candidate binary after source restoration. Rewriting
the unchanged source bytes forced recompilation before the final restoration check.

### Withdrawn Printable ASCII Scanning Experiment (2026-11-03)

**Withdrawn at the user's request:** the custom SIMD implementation covered x64
only, while other architectures used a scalar fallback. The user's requirement
for this optimization is SIMD coverage across the supported architectures.
The helper, both modified scanning loops, and SIMD-specific tests were reverted.
The independent split-UTF-8 C1 control fix and its event regression test remain.
The figures below document the withdrawn experiment, not the current implementation.
Rollback validation passed 64 VTE all-features tests, 42 no-default-features tests
(overlapping configurations), and 203 terminal-core tests with 3 existing ignored
tests. Workspace formatting and diff whitespace checks also passed.

Baseline: `99102cf7aef9b756cb759a892fbb5dca12ef44c5`. Environment: Windows x64,
Intel Core i9-13900HX, balanced power plan, Rust 1.97.0, the optimized bench
profile (opt-level 3, thin LTO, one codegen unit). The existing `terminal_pipeline`
fixtures use approximately 256 KiB of synthetic input, 120 columns, 40 screen rows,
and 20,000 history rows. No user terminal contents were sampled.

The candidate's VTE printable-span and complete-ASCII-line scans used bounded, unaligned SSE2
loads on x64 with SSE2 enabled. Other targets retained a scalar scan. ESC searching,
UTF-8 validation, and the non-ASCII dispatch loop remain in place; no UTF-32
intermediate buffer, new dependency, or AVX CPU requirement was introduced.
The scan remains outside the generic dispatcher on x64: the first inlined
candidate increased the mixed-Unicode parser workload by about 10–12%.
The outlined candidate still had a smaller parser-only Unicode cost.

Initial measurements covered all 11 corpora at parser, grid, and full-input layers.
The final confirmation used separately compiled baseline and candidate executables,
with no compilation during measurement. Each case ran both binaries consecutively,
alternating which binary ran first between cases. Both used 20 samples, a one-second
warm-up, and a two-second measurement interval. The table reports median elapsed
microseconds and elapsed-time change, not throughput change:

| Layer / workload | Baseline (µs) | Candidate (µs) | Time change |
| --- | ---: | ---: | ---: |
| parser/ansi | 281.04 | 248.31 | -11.65% |
| parser/cjk | 239.50 | 239.09 | -0.18% |
| parser/crlf-ascii | 102.00 | 27.19 | -73.34% |
| parser/plain | 96.96 | 25.07 | -74.14% |
| parser/unicode | 319.96 | 333.44 | +4.21% |
| parser/wrapped-ascii | 113.98 | 15.25 | -86.62% |
| pipeline/ansi | 2243.89 | 2129.27 | -5.11% |
| pipeline/cjk | 2247.85 | 2181.29 | -2.96% |
| pipeline/crlf-ascii | 1593.32 | 1481.85 | -7.00% |
| pipeline/dec-lines | 2716.67 | 2559.23 | -5.80% |
| pipeline/long-csi | 819.33 | 813.54 | -0.71% |
| pipeline/mixed-log | 2406.85 | 2408.89 | +0.08% |
| pipeline/osc-links | 6612.22 | 6581.86 | -0.46% |
| pipeline/osc-title | 3662.00 | 3725.30 | +1.73% |
| pipeline/plain | 1380.67 | 1376.69 | -0.29% |
| pipeline/unicode | 3453.08 | 3467.87 | +0.43% |
| pipeline/wrapped-ascii | 1345.26 | 1243.11 | -7.59% |

The full input pipeline benefited most from wrapped ASCII, CRLF ASCII, DEC line
characters, and ANSI output. Ordinary plain logs and mixed logs were effectively
unchanged in the paired confirmation. Mixed Unicode parsing was 4.21% slower,
while its full input pipeline was 0.43% slower; OSC title input was 1.73% slower.
These tradeoffs do not support a universal no-regression or terminal-FPS claim.
The pipeline uses recording playback through the real input-processing path; it
excludes PTY/SSH delivery, desktop rendering, and interactive input latency.
ARM64 and native macOS/Linux performance were not measured.

The boundary regression fixture also exposed a pre-existing inconsistency:
a UTF-8 encoded C1 control completed in a later read was printed instead of
executed. The unchanged parser reproduced the failure. Both partial-UTF-8
completion branches now agree with whole-buffer dispatch. The experiment's tests covered every
byte value around vector boundaries and unaligned slice tails, batch line counts
and maximum widths, and split ANSI/UTF-8 input with explicit expected events.

Validation on Windows x64: VTE's all-features suite passed 65 tests and its
no-default-features suite passed 43 (overlapping configurations); the terminal
grid suite passed 154; the application terminal-core suite passed 203 with three
existing ignored tests. VTE library Clippy with all features and warnings denied,
workspace format checking, and diff whitespace checking passed. No desktop GUI
acceptance or other-platform execution is claimed.

Reproduction of the original baseline sweep:

```sh
cargo bench --locked -p oxideterm-terminal --bench terminal_pipeline -- 'terminal_input_breakdown/(parser|grid|pipeline)/' --noplot --sample-size 20 --warm-up-time 1 --measurement-time 2 --save-baseline before-ascii-scan
```

The paired confirmation uses the same arguments, with an exact per-case filter
and baseline names `scalar-final` / `simd-final`, alternating the executable order.
Raw logs and comparison JSON are under `target/simd-evaluation/`; Criterion samples
are under `target/criterion/terminal_input_breakdown/`. Source snapshots and both
benchmark executables are retained in the local evaluation directory.

### Complete Animation Frame Caching (2026-10-29)

The final playback policy restores decode-once, full-frame reuse for image widgets
(file/SFTP previews and chat images) and terminal-protocol animations. It replaces
the intermediate 64 MiB GPU-cache compromise. The separate desktop background/video
media decoder is unchanged.

- GIF and animated WebP sources decode, resize, rotate, and convert their complete
  frame sets once per display resolution. Animation storage is raw BGRA, not QOI
  frames decompressed on each loop. The streaming decoder worker and three-frame
  sliding CPU cache have been removed.
- Terminal animations prepare their complete BGRA frame sets, reserving the full
  pixel allocation before conversion. Static terminal images retain the shared-RGBA
  optimization. Image admission/eviction fixes and compact search coordinates remain.
- Every uploaded frame of a visible animation remains in the atlas for subsequent
  loops, including animations larger than the former 64 MiB limit. Hidden images,
  superseded resolutions, explicit removal, and renderer resets release their
  textures. The general image cache still evicts cold resources, not visible players.
- Deadline-based wakeups remain: image widgets and terminal images request a frame
  when the next image frame is due rather than continuously at display refresh rate.
  Hidden views cancel deadline tasks and inactive windows are not notified.

Regression coverage includes stable borrowed pixel addresses across repeated loops;
17 distinct 4 MiB frames all remaining in the GPU cache; release after hiding/removal;
independent playback cursors; resizing and rotation; GIF disposal and transparency;
pause, loading, finite loops and gapless frames. The image suite passed 10 tests
(four manual benchmarks unrun), the image-cache lifecycle suite passed three tests,
the atlas reuse test passed, and all 214 terminal-view tests passed.

The macOS ordinary application build passed without test-support features.
The prior atlas-query build error is fixed across Metal, DirectX, WGPU, headless and
test implementations. Windows/Linux native builds were not run on this macOS host.

Complete decoded frames and visible GPU frames now scale with animation dimensions
and frame count. The low-memory animation preparation numbers in the earlier section
below describe the superseded implementation; the static-image and search results
still apply. No standalone GUI instance was launched and hardware power was not
measured. General scrollback compression stays paused.

### Terminal Image Ownership And Search Coordinates (2026-10-29)

Baseline: `b3aae458f`. Measurements use the same machine and optimized profiles as
the search baseline below, with one warm-up and five recorded samples. General
scrollback compression remains paused.

The graphics ingress now accounts for decoded image bytes across protocols and
emits deletion events when evicting images, releasing its Kitty animation sources
as well as consumer placements. The default ingress allowance is 16 MiB; the
existing terminal-state safety budget remains in place. PNG/JPEG/GIF dimensions
are checked before pixel decoding, GIF accumulation is bounded, and GIF frame
zero shares the primary image allocation. The GIF iterator can temporarily hold
one additional canvas while detecting EOF. Sixel still uses its decoder's own
allocation limit before the terminal checks the decoded dimensions.

Terminal rendering shares protocol RGBA canvases. It converts only a requested
frame/crop to BGRA during texture upload instead of retaining a second complete
frame set. An upload-workspace reservation follows the render image through
retirement. Animation controls with unchanged pixels/delays reuse the image;
new pixel revisions retire old cache entries. Hidden panes release derived image
resources while retaining protocol data and animation clocks, and late preparation
results cannot repopulate a suspended cache.

| Image workload | Before | After | Median preparation/ingress time |
| --- | ---: | ---: | ---: |
| 100 distinct 1024×1024 PNG uploads, parser-retained pixels | 400 MiB | 16 MiB | 73.04 → 79.89 ms |
| One 512×256 static image, extra preparation peak heap | 513.30 KiB | 1.52 KiB | 137 → 1 µs |
| 48 frames at 512×256, extra preparation peak heap | 24.01 MiB | 6.52 KiB | 3,913 → 4 µs |

The upload benchmark counts pixel allocations still held by the parser via weak
references, not RSS. The preparation benchmark tracks requested Rust heap above
the already-created protocol source. Its 48-frame source remains 24 MiB; a requested
frame needs a temporary 512 KiB conversion buffer during upload. GPU textures and
first-paint latency are outside these measurements. Conversion moves to upload;
the preparation-time improvement is not an end-to-end rendering speedup. Admission
and eviction add about 9% to this small-PNG ingress workload.

Search row mappings use 12-byte checked coordinates for ordinary rows, promoting
only rows whose offsets/columns require `usize`. ASCII rows retain no map object.
The benchmark uses the real background-search and result-reconciliation paths,
20,000 history rows at 400 columns plus 40 screen rows. All paired match counts agree.

| Search workload | Extra peak heap before → after | Median scan + resolution before → after |
| --- | ---: | ---: |
| ASCII, regex hit | 18.67 → 18.26 MiB | 6.79 → 6.64 ms |
| Alternating cell colors, regex hit | 18.67 → 18.26 MiB | 38.11 → 35.09 ms |
| CJK + combining marks, regex miss | 115.38 → 69.88 MiB | 43.62 → 42.46 ms |
| CJK + combining marks, regex hit | 119.79 → 74.13 MiB | 71.04 → 67.26 ms |
| CJK + combining marks, cross-line literal | 119.79 → 74.13 MiB | 53.61 → 48.98 ms |

Validation: 194 terminal-core tests passed, with five existing ignored cases;
26 graphics-protocol tests passed, with the manual benchmark separately executed.
The final map-storage refinement passed all five focused search tests. The terminal
view suite passed 212 tests and retained the previously reproduced failure in
`terminal_background_animation_plays_and_pauses` at `app/render.rs:3027`.
The final cache lifecycle tests and the GPUI shared-RGBA upload test passed.
The native application build passed. Native image-playback inspection was stopped
at the user's request; no interactive image-playback acceptance is claimed.

Reproduction commands:

```sh
cargo test -p oxideterm-terminal-graphics --release image_retention_benchmark -- --ignored --nocapture
cargo bench -p oxideterm-gpui-terminal --bench image_cache
cargo bench -p oxideterm-terminal --bench history_search
```

Raw paired output is under `target/memory-evaluation/`: `images-before.txt`,
`images-after.txt`, `image-cache-before.csv`, `image-cache-after.csv`,
`search-mapping-before.csv`, and `search-mapping-after.csv`.

### Shared Background-Search Text (2026-10-28)

The background search path now retains one text owner per matched logical line, or one
owner for the complete cross-line subject. Rows carry byte ranges into that owner instead
of keeping a second copy of every row's text. Extraction still holds the terminal lock
for at most 32 rows at a time; a reusable chunk string separates that bounded copy from
growth of the complete subject outside the lock. Immutable Unicode column adjustments
use exactly sized boxed slices rather than retaining vector growth capacity.

Arbitrary cross-line regex semantics remain unchanged: no fixed search window or match
limit was introduced. Stable row identities, screen-content checks, neighbor context,
written spaces, wide cells and combining marks still determine result validation.

Measured on the same Apple M5 / macOS 26.6.2 / Rust 1.97.0 setup as below, optimized bench
profile, one warm-up and five measured samples. The fixture contains 20,000 history rows
at 400 columns plus 40 screen rows. The baseline is the original search implementation
at `f11dff350`; the earlier hidden-cache changes do not participate in this backend fixture.
Queries are compiled before measurement, matching the UI's compiled-query reuse. Peak
heap is additional requested Rust heap above the populated terminal and compiled query,
covering both scanning and result reconciliation. It excludes allocator slack and RSS.
Time is the median sum of scanning and reconciliation; the CSV also separates both stages.

| Workload | Peak before | Peak after | Time before | Time after |
| --- | ---: | ---: | ---: | ---: |
| Full ASCII, regex miss | 22.40 MiB | 14.26 MiB | 3.60 ms | 3.18 ms |
| Full ASCII, regex hit | 27.11 MiB | 18.67 MiB | 7.34 ms | 6.85 ms |
| Full ASCII, cross-line literal | 27.11 MiB | 18.67 MiB | 3.86 ms | 3.36 ms |
| Alternating per-cell colors, regex miss | 24.54 MiB | 14.26 MiB | 31.82 ms | 37.17 ms |
| Alternating per-cell colors, regex hit | 29.25 MiB | 18.67 MiB | 35.78 ms | 40.52 ms |
| CJK + combining marks, regex miss | 161.11 MiB | 115.38 MiB | 50.35 ms | 44.52 ms |
| CJK + combining marks, regex hit | 165.82 MiB | 119.79 MiB | 75.05 ms | 71.13 ms |
| CJK + combining marks, cross-line literal | 165.82 MiB | 119.79 MiB | 58.78 ms | 54.02 ms |

Retained because memory is the priority: the alternating-color cases cost about 3–5 ms
more per search, while plain text and mixed Unicode regex searches improve. Dense ASCII
literal results still consume about 68 MiB at peak for 280,546 hits; result objects are
not capped or discarded. Unicode mapping data remains the largest mixed-text allocation.

All paired benchmark result counts are unchanged. The complete terminal-core unit suite
passed with its existing ignored cases left ignored; all 11 selected terminal-view search
tests passed, including SSH search refresh during output, cancellation, manual-scroll
preservation and result selection near the viewport. The real backend Unicode case now
also checks a dot-all regex spanning three rows against explicit column ranges. Formatting
and diff checks passed. This batch does not change the pre-existing animation-test failure
recorded below.

Raw data: `target/memory-evaluation/search-before.csv`, `search-shared.csv` (text sharing
before tightening coordinate allocations), and `search-after.csv`.

```sh
cargo bench -p oxideterm-terminal --bench history_search
cargo test -p oxideterm-terminal --lib
cargo test -p oxideterm-gpui-terminal search
```

### Hidden Terminal Caches And Rejected Palette Expansion (2026-10-28)

Baseline: `f11dff350`, Apple M5, macOS 26.6.2, Rust 1.97.0. The homepage redesign
was reverted before this work. Only hidden terminal text-cache reclamation was retained;
both expanded-history-attribute candidates were rejected and their production changes removed.

Hidden tabs now release retained row layouts, text shaping/paint caches, highlights and
link layouts. Visibility follows the tab host, including every split pane and detached
window, rather than keyboard focus. App locking releases these caches in all panes.
Session state, selection, scroll position, search state, and image animation clocks remain
owned by the pane. Visible panes rebuild text caches on demand. Images and background
media are not evicted by this change.

The native `terminal_tab_switch_frame` benchmark opens eight playback terminals and presents
32 batches of 16 distinct log lines in each before cycling between tabs. Both revisions
use the same tracking allocator and workload. The heap measurement is the requested Rust
heap increment after setup with seven tabs hidden; it excludes native font allocations,
GPU resources and allocator size classes. It is a setup measurement, not process RSS.
Frame measurements use native text shaping and the Metal headless renderer, with ten
Criterion samples, one second of warm-up and two seconds of measurement.

| Measurement | Before | Retained candidate |
| --- | ---: | ---: |
| Eight-tab requested heap increment | 60.02 MiB | 35.25 MiB |
| Tab-switch iteration estimate | 0.894 ms | 1.299 ms |
| Tab-switch dirty-to-draw p99 | 3.557 ms | 3.330 ms |
| Continuous playback-output iteration estimate | 0.835 ms | 0.816 ms |

The heap reduction is 24.77 MiB (41.3%) in this fixture. Switching pays about 0.405 ms
for rebuilding discarded layouts; all measured tab-switch frames remained inside the
120 Hz frame budget. Continuous playback output showed no statistically significant change.
This measures rendering and playback parsing, not end-to-end PTY or SSH throughput.

History experiments used the existing 20,000-line, 400-column benchmark, extended with
17- and 64-color indexed palettes. Values below are medians of five measured samples
after one warm-up. The generic candidate used a hash table beyond 16 attributes; the
restricted candidate used direct indexed-color lookup and kept the old complex-color fallback.

| Input / candidate | Retained heap | Steady output | Narrow to 200 columns and restore |
| --- | ---: | ---: | ---: |
| 17 colors / baseline | 189.65 MiB | 160.58 ms | 24.31 ms |
| 17 colors / generic table | 72.77 MiB | 231.01 ms | 132.36 ms |
| 17 colors / direct palette | 72.77 MiB | 179.25 ms | 59.48 ms |
| 64 colors / baseline | 189.65 MiB | 160.12 ms | 23.77 ms |
| 64 colors / generic table | 87.11 MiB | 265.42 ms | 200.80 ms |
| 64 colors / direct palette | 87.11 MiB | 179.92 ms | 67.12 ms |
| Unique RGB per cell / baseline | 189.65 MiB | 212.14 ms | 26.11 ms |
| Unique RGB per cell / generic table | 189.65 MiB | 430.12 ms | 288.42 ms |

Both candidates saved memory on repeated palettes, but their output and narrowing costs
were unacceptable for this iteration. The existing 16-attribute admission policy remains.

Validation: 212 of 213 terminal-view tests passed, including hidden output/search/selection/
scroll preservation and release/rebuild of cached text. The extended application test for
split panes, background tabs, detached windows, return-to-main and app-lock visibility passed.
The existing `terminal_background_animation_plays_and_pauses` test failed at `app/render.rs:3027`;
running it with both modified terminal source files restored to HEAD reproduced the same
failure (`false` versus expected `true`). Its test and animation implementation were not changed.
Formatting and diff checks passed. Interactive testing in a user's live SSH sessions was not run.

Local raw results: `target/memory-evaluation/history-before.csv`, `history-candidate.csv`,
`history-palette.csv`, `tabs-before.txt`, `tabs-after.txt`, and `animation-baseline.txt`.
The rejected direct-palette patch is retained there as `rejected-palette.patch`.

```sh
cargo bench -p alacritty_terminal --bench history
cargo bench -p oxideterm-gpui-terminal --features bench --bench terminal_render -- \
  'terminal_(tab_switch_frame|playback_output_frame)' \
  --sample-size 10 --warm-up-time 1 --measurement-time 2
```

### Text-Run History And Sequential Reads (2026-10-26)

The third-stage baseline is the completed eight-byte implementation, saved before production edits in `target/text-history/baseline-src/` and `phase2.patch`. Both revisions were measured on the same Apple M5, ARM64 macOS 26.6.2, Rust 1.97.0, optimized bench profile, with one warm-up and five measured samples. Values below are medians; MB means 1,000,000 requested heap bytes.

```sh
cargo bench --locked -p alacritty_terminal --bench history
cargo bench --locked -p oxideterm-terminal --bench history_search
```

The kernel harness retains 20,000 history rows and 40 screen rows. It fills 20,040 lines and writes another 20,040 at the limit. The width round trips are original → double → original, followed by original → half → original. The latter exercises actual reflow and the existing history-limit eviction policy. The expanded baseline was rebuilt from its saved source in `target/text-history/baseline-crate/`, using the same local VTE fork, dependency versions, release settings and harness, without replacing the working source.

| Workload | Retained heap before → after (MB) | Steady output before → after (ms) | Double-width round trip (ms) | Half-width reflow round trip (ms) |
| --- | ---: | ---: | ---: | ---: |
| Short ASCII, 120 columns | 16.83 → 8.31 | 5.19 → 4.74 | 1.13 → 1.41 | 11.81 → 12.57 |
| Short ASCII, 400 columns | 17.37 → 8.84 | 5.17 → 4.63 | 1.57 → 1.29 | 0.87 → 0.97 |
| Pseudorandom full ASCII, 400 columns | 70.65 → 15.50 | 24.35 → 23.10 | 3.47 → 2.63 | 29.26 → 28.57 |
| Full ASCII, ten runs of eight colors | 72.89 → 21.26 | 30.69 → 29.14 | 4.17 → 3.66 | 33.48 → 29.69 |
| Full ASCII, two colors alternating every cell | 70.97 → 71.50 | 162.06 → 156.91 | 4.15 → 4.63 | 63.90 → 60.69 |
| Full ASCII, distinct RGB per column | 198.33 → 198.86 | 216.70 → 217.11 | 35.71 → 37.67 | 24.49 → 26.98 |
| Full CJK/ASCII with combining marks | 72.69 → 73.23 | 143.84 → 139.38 | 1.90 → 2.72 | 53.65 → 53.20 |

Ordinary full-line retained heap is **15.50 MB**, a **78.1% reduction** from the eight-byte baseline. The ten-run color case retains **21.26 MB**. After the double-width round trip these retain 14.34 MB and 20.74 MB, with peaks of 20.32 MB and 26.38 MB respectively. Reading and kernel searching leave retained heap unchanged in every case. These counters cover the terminal/parser allocations, excluding the prebuilt input, allocator size classes, desktop UI and GPU allocations.

#### Background search uses the real runtime path

The runtime harness calls `TerminalSearchSource::search_matches_with_options`, including bounded row copying, matching and result resolution. Each case has 20,000 history rows at 400 columns. ASCII content repeats the alphabet to provide a known literal match; styles vary independently. The literal query `zabcdefghijklm` has matches in the ASCII cases and none in the Unicode case. The other queries are a literal miss and the regex miss `[A-Z]{12}`.

| Workload | Literal miss before → after (ms) | Literal query before → after (ms) | Regex miss before → after (ms) |
| --- | ---: | ---: | ---: |
| Full ASCII alphabet sequence, 400 columns | 36.81 → 2.63 | 49.00 → 14.42 | 37.96 → 3.84 |
| Full ASCII, ten runs of eight colors | 38.94 → 2.76 | 50.55 → 13.87 | 40.18 → 3.94 |
| Full ASCII, two colors alternating every cell | 37.46 → 31.02 | 49.60 → 41.98 | 38.78 → 31.51 |
| Full CJK/ASCII with combining marks | 47.67 → 44.64 | 47.07 → 42.88 | 51.97 → 47.45 |

Eligible ASCII text is copied directly, including exact implicit tails and occupied-column limits. Width markers, NUL normalization and combining characters use cell iteration. The 32-row lock chunks, lock-free matching and row-identity reconciliation are unchanged.

Kernel regex traversal and plain text extraction are measured separately:

| Workload | Read all rows before → after (ms) | Forward kernel search (ms) | Reverse kernel search (ms) |
| --- | ---: | ---: | ---: |
| Short ASCII, 120 columns | 2.59 → 1.22 | 13.82 → 13.95 | 13.53 → 13.91 |
| Short ASCII, 400 columns | 7.99 → 3.17 | 50.39 → 45.94 | 48.40 → 46.27 |
| Pseudorandom full ASCII, 400 columns | 7.78 → 3.45 | 45.85 → 45.73 | 45.16 → 45.99 |
| Full ASCII, ten runs of eight colors | 7.79 → 3.43 | 46.01 → 51.48 | 45.39 → 50.60 |
| Full ASCII, two colors alternating every cell | 7.88 → 6.00 | 46.07 → 46.88 | 45.25 → 45.80 |
| Full ASCII, distinct RGB per column | 7.50 → 5.75 | 42.95 → 44.39 | 43.50 → 48.02 |
| Full CJK/ASCII with combining marks | 9.42 → 7.53 | 42.98 → 44.34 | 46.24 → 49.79 |

Ordinary and ten-run colored output now improve on the eight-byte baseline for steady output and both resize workloads. Remaining costs are workload-specific: distinct-RGB half-width reflow rises from 24.49 to 26.98 ms; mixed Unicode double-width resizing rises from 1.90 to 2.72 ms; colored kernel regex traversal rises from 46.01 to 51.48 ms forward. Runtime background search improves in all measured cases.

#### Storage and hot paths

- Writable screen cells remain 24 bytes. History selects UTF-8 primary characters plus exact attribute/flag runs, eight-byte indexed cells, or full cells. Every stored primary character still corresponds to one Alacritty cell, including wide-character placeholders; no grapheme-width inference changes terminal contents.
- Text admission compares retained capacities with indexed/full-cell costs. Run discovery is bounded at 16 runs to keep conversion and repeated reflow costs bounded. More fragmented rows prefer indexed cells; their attribute table retains its 16-entry work limit. Dense width/combining metadata keeps the Unicode benchmark indexed. This is a conversion-work policy as well as a memory comparison.
- Repeated-tail compression remains in every representation. Recycled rows can switch between text and indexed storage without keeping obsolete content. Indexed/dense cases incur approximately 0.53 MB of additional row metadata at this workload size.
- Row iteration has native forward/backward state; ASCII skipping is constant time and Unicode scans retain byte offsets. Grid iteration keeps a seek cursor and caches the current view. Sequential folds process attribute runs without repeating storage dispatch per cell.
- ASCII input checks wide-cell overlap directly in the writable screen span. The earlier generic history iterator in this check was responsible for much of the output regression. The isolated 20,040-conversion probe is approximately unchanged for full ASCII (5.54 → 5.65 ms), while the complete output path improves from 24.35 to 23.10 ms.
- Shrinking decodes the removed stored span; writable mutations expand the affected row. Run-wise decoding uses an exactly sized destination. Initial run boundaries are retained during admission rather than rediscovered during encoding. Empty repeated tails are not expanded.

Paired raw results are `target/text-history/conversion-baseline.csv` / `accepted.csv` and `search-baseline.csv` / `search-accepted.csv`. The kernel CSV includes fill, isolated conversion, forward/reverse search, read, retained heap and resize-peak measurements. Earlier files in that directory preserve the diagnosed iterator, conversion and resize regressions; they are not the accepted results.

Validation passed **603 tests**: 149 kernel unit tests, 45 upstream reference recordings, 193 runtime unit tests, five real-PTY integration tests and 211 terminal-view tests. Coverage includes mixed-direction iteration, ASCII skips, Unicode positions, wide placeholders, combining marks, hyperlinks, underline colors, copy-on-write after decoding, representation reuse, implicit tails, serialization, search across automatic wraps and lock-chunk boundaries, and resizing. Five existing ignored runtime tests and the manual serial-device test remain unrun. Native graphical-window interaction and frame latency remain manual acceptance checks.

### Shared-Attribute History (2026-10-26)

The second-stage baseline is the completed repeated-tail implementation below, not the original upstream grid. Its source snapshot is retained locally at `target/packed-history/tail-baseline-src/`, with the baseline harness at `target/packed-history/baseline-harness.rs`. The benchmark now includes deterministic pseudorandom full lines, eight foreground colors in 40-column runs, distinct RGB colors at every column, and CJK/ASCII mixed with a combining mark in each four-column group. Each line ends with an SGR reset and CRLF. These inputs differ from the first-stage repeated-digit corpus, so use the paired baseline in this section.

Run `cargo bench --locked -p alacritty_terminal --bench history`. Hardware, toolchain, history limit, screen height, warm-up and five measured samples are unchanged from the first stage. Each sample fills 20,000 history rows, writes another 20,040 lines, reads the retained text, searches the complete buffer for a missing regex, then doubles and restores the width. Search measures the kernel's `RegexSearch` traversal, not the desktop background-search UI.

| Workload | Retained heap before → after (MB) | Steady output before → after (ms) | Search before → after (ms) | Width round trip before → after (ms) |
| --- | ---: | ---: | ---: | ---: |
| Short ASCII, 120 columns | 36.09 → 16.83 | 5.36 → 4.95 | 12.74 → 13.80 | 1.13 → 0.90 |
| Short ASCII, 400 columns | 36.63 → 17.37 | 5.20 → 4.88 | 40.83 → 45.49 | 1.14 → 1.01 |
| Pseudorandom full ASCII, 400 columns | 196.47 → 70.65 | 18.04 → 24.05 | 40.12 → 46.23 | 23.30 → 3.37 |
| Full ASCII, eight foreground colors | 196.47 → 72.89 | 23.45 → 30.32 | 40.31 → 46.34 | 22.81 → 4.14 |
| Full ASCII, distinct RGB color per column | 196.47 → 198.33 | 206.06 → 211.16 | 40.32 → 43.67 | 22.91 → 36.06 |
| Full CJK/ASCII with combining marks | 340.75 → 72.69 | 103.03 → 142.53 | 37.37 → 42.98 | 25.72 → 1.56 |

The ordinary full-line target is met: 70.65 MB after steady output and 69.84 MB after resizing, with a 74.83 MB resize peak. The eight-color case retains 72.89 MB (72.40 MB after resizing; 77.38 MB peak). Reading and searching do not increase retained heap in any case. The measurements exclude the input corpus, allocator size classes, desktop UI and GPU resources.

There is a throughput tradeoff. Relative to the repeated-tail baseline, ordinary full-line steady output takes about 33% longer and its search about 15% longer; mixed combining-character output also costs more CPU. Width changes are substantially faster for packed rows. High-entropy color rows keep dense cells and incur extra conversion-attempt/read-view overhead, including slower resizing. These results do not establish a no-regression performance claim.

History uses eight-byte character/flags/attribute-index entries and row-local exact colors and extension references. Attribute construction stops at 16 distinct entries and requires at least 25% estimated savings; rejected rows keep the dense or repeated-tail representation. This work budget deliberately leaves some potentially compressible high-style-count rows unpacked. Mutable access restores only the affected row, while scrolling recycles both packed allocations and dense screen buffers. Narrowing invalidates cached rejection decisions because removed columns may contain the costly attributes.

Raw paired results: `target/packed-history/baseline.csv` and `accepted.csv`. The final harness also records heap after search and resize. Intermediate files retain the conversion, allocation-reuse, and read-view-only diagnostic measurements; the temporary encoding-disabled diagnostic was restored before validation.

Validation: 147 kernel unit tests, 45 upstream reference recordings, 192 runtime unit tests, five real-PTY integration tests, and 211 terminal-view tests passed (600 total). Five existing ignored runtime tests remain unrun. New tests cover packed character/flag fidelity, exact RGB/named colors, links, combining marks, underline colors, copy-on-write after unpacking, capacity reclamation, high-entropy rejection, and reconsideration after narrowing. The narrowing regression was demonstrated failing before its correction. Native graphical-window interaction and desktop frame latency remain manual checks.

### Repeated-Tail History: First-Stage Baseline (2026-10-26)

The first-stage history benchmark exercised the parser and grid directly, without launching the desktop. Its archived measurements below used the original three-case harness; the command now runs the expanded second-stage workloads:

```sh
cargo bench --locked -p alacritty_terminal --bench history
```

Baseline: `f2593f3bd6d950b5a4bd8be88c2932e82a69bd25`, with the same benchmark harness added before changing storage. Environment: Apple M5, ARM64, macOS 26.6.2, Rust 1.97.0, Cargo's optimized bench profile. Each case has 40 screen rows and a 20,000-row history limit. One warm-up precedes five samples. Short lines contain 66 ASCII characters; the full-line case contains 400. Each sample fills the terminal with 20,040 lines, writes another 20,040 lines at the history limit, reads all retained rows, then doubles and restores the column count.

Heap figures count live requested allocations owned by the terminal and parser, excluding the prebuilt input. They do not include allocator size classes, GPU resources, or the rest of the desktop process. The measured `Cell` size is 24 bytes. Reading all history left the retained allocation count unchanged in every sample.

| Workload | Retained heap before → after (MB) | Change | Steady output before → after (ms) | Width round trip before → after (ms) |
| --- | ---: | ---: | ---: | ---: |
| Short lines, 120 columns | 61.26 → 36.09 | −41.1% | 5.37 → 4.51 | 10.72 → 0.64 |
| Short lines, 400 columns | 196.20 → 36.63 | −81.3% | 5.11 → 3.91 | 29.21 → 0.73 |
| Full lines, 400 columns | 196.20 → 196.47 | +0.14% | 17.74 → 15.31 | 29.35 → 23.32 |

Times are medians of five samples on each revision. The final run follows the constant-time row-iterator skipping correction and the complete regression tests. Earlier 120-column runs showed timing variability (steady-output medians 6.16 and 4.23 ms); their samples remain available alongside the final 4.51 ms result. The final full-line run also contains an outlier, retained in the raw data. These are kernel operation timings, not GPU frame or native window latency measurements.

Peak live allocations during resizing fell from 120.58 to 38.01 MB for short120, 390.19 to 38.56 MB for short400, and 390.19 to 199.24 MB for full400. The full-line case has no repeated tail to compact at its original width, so it retains full cells plus a small per-row metadata cost. The short-line savings come from exact repeated-tail storage and reuse of dense screen allocations. Shrinking an implicit default tail does not materialize it.

Raw local samples for the table are in `target/compact-history/baseline.csv` and `verified.csv`. Intermediate measurements (`initial.csv`, `resize-fixed.csv`, `final.csv`, and `final-repeat.csv`) in the same directory record the initial full-line resize regression, its correction, and repeated timing observations before final validation.

Validation passed 144 terminal-kernel unit tests, 45 upstream reference recordings, 192 terminal-runtime unit tests, five real-PTY integration tests, and 211 terminal-view tests. Five pre-existing ignored runtime tests were not run (performance probes and the external tmux-control scenario). Added coverage checks compact-row memory retention and buffer reuse, Unicode and link metadata, colored-tail resize semantics, stable row identity, reference serialization, and runtime search/snapshot reads across resizing. Native graphical-window interaction remains a manual acceptance check.

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
