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
